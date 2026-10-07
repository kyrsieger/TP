use super::{
    cleanup::{cleanup_ids, remove_record, settings_cleanup_confirmation},
    common::{err, now, Result},
    events::changed,
    model::Settings,
    session::PanelLifecycle,
    storage::{settings, Store},
};
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;
#[tauri::command]
pub(super) fn autostart_enabled(app: tauri::AppHandle) -> Result<bool> {
    app.autolaunch().is_enabled().map_err(err)
}

#[tauri::command]
pub(super) fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<()> {
    if enabled {
        app.autolaunch().enable().map_err(err)
    } else {
        app.autolaunch().disable().map_err(err)
    }
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum SettingsSaveResult {
    Saved,
    ConfirmationRequired { count: usize },
}

#[tauri::command]
pub(super) fn save_settings(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    value: Settings,
    confirmed_count: Option<usize>,
) -> Result<SettingsSaveResult> {
    value.validate()?;
    let conn = store.db.lock().map_err(err)?;
    let timestamp = now();
    if let Some(count) =
        settings_cleanup_confirmation(&conn, &value, timestamp, confirmed_count.unwrap_or(0))?
    {
        return Ok(SettingsSaveResult::ConfirmationRequired { count });
    }
    let ids = cleanup_ids(&conn, &value, timestamp)?;
    let previous = settings(&conn)?;
    if previous.always_on_top != value.always_on_top {
        app.get_webview_window("main")
            .ok_or("面板窗口不存在")?
            .set_always_on_top(value.always_on_top)
            .map_err(err)?;
    }
    #[cfg(windows)]
    let taskbar_window = if previous.show_taskbar_icon != value.show_taskbar_icon {
        app.get_webview_window("main")
    } else {
        None
    };
    #[cfg(windows)]
    if let Some(window) = &taskbar_window {
        window
            .set_skip_taskbar(!value.show_taskbar_icon)
            .map_err(err)?;
    }
    if let Err(error) = conn.execute(
        "UPDATE settings SET value=?1 WHERE key='main'",
        [serde_json::to_string(&value).map_err(err)?],
    ) {
        #[cfg(windows)]
        if let Some(window) = &taskbar_window {
            let _ = window.set_skip_taskbar(!previous.show_taskbar_icon);
        }
        return Err(err(error));
    }
    for id in &ids {
        remove_record(&conn, &store.dir, id)?;
    }
    app.state::<PanelLifecycle>()
        .state
        .lock()
        .map_err(err)?
        .clear_hidden_search(value.retain_search_input);
    drop(conn);
    if !ids.is_empty() {
        changed(&app);
    }
    Ok(SettingsSaveResult::Saved)
}
