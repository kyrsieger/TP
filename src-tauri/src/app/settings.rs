use super::{
    cleanup::{cleanup_ids, remove_record, settings_cleanup_confirmation},
    common::{err, now, Result},
    events::changed,
    model::Settings,
    panel_runtime::resume_panel,
    session::PanelLifecycle,
    storage::{settings, Store},
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
#[tauri::command]
pub(super) fn autostart_enabled(app: tauri::AppHandle) -> Result<bool> {
    app.autolaunch().is_enabled().map_err(err)
}

#[tauri::command]
pub(super) fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<()> {
    let store = app.state::<Store>();
    let _data = store.read_data()?;
    let conn = store.db.lock().map_err(err)?;
    let current = app.autolaunch().is_enabled().map_err(err)?;
    save_autostart(&conn, enabled, current, |value| {
        apply_autostart(&app, value)
    })
}

fn apply_autostart(app: &tauri::AppHandle, enabled: bool) -> Result<()> {
    if enabled {
        app.autolaunch().enable().map_err(err)
    } else if app.autolaunch().is_enabled().map_err(err)? {
        // Windows 的插件在启动项已经不存在时删除会报错。
        app.autolaunch().disable().map_err(err)
    } else {
        Ok(())
    }
}

fn saved_autostart(conn: &Connection) -> Result<Option<bool>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='autostart'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?;
    json.map(|value| serde_json::from_str(&value).map_err(err))
        .transpose()
}

fn write_autostart(conn: &Connection, enabled: bool) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key,value) VALUES('autostart',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [if enabled { "true" } else { "false" }],
    ).map_err(err)?;
    Ok(())
}

fn save_autostart(
    conn: &Connection,
    enabled: bool,
    current: bool,
    mut apply: impl FnMut(bool) -> Result<()>,
) -> Result<()> {
    let transaction = conn.unchecked_transaction().map_err(err)?;
    write_autostart(&transaction, enabled)?;
    if let Err(error) = apply(enabled) {
        let _ = apply(current);
        return Err(error);
    }
    if let Err(error) = transaction.commit() {
        let _ = apply(current);
        return Err(err(error));
    }
    Ok(())
}

fn restore_autostart(
    conn: &Connection,
    current: bool,
    mut apply: impl FnMut(bool) -> Result<()>,
) -> Result<()> {
    match saved_autostart(conn)? {
        // 即使已开启也重新写入，以更新重装后可能改变的可执行文件路径。
        Some(true) => apply(true),
        Some(false) if current => apply(false),
        Some(false) => Ok(()),
        // 旧版本没有保存此项；首次迁移接收当前系统状态。
        None => write_autostart(conn, current),
    }
}

pub(super) fn restore_saved_autostart(app: &tauri::AppHandle) -> Result<()> {
    let store = app.state::<Store>();
    let conn = store.db.lock().map_err(err)?;
    let current = app.autolaunch().is_enabled().map_err(err)?;
    restore_autostart(&conn, current, |value| apply_autostart(app, value))
}

/// Coordinates are saved independently of user preferences, so a delayed window
/// resize cannot restore a stale copy of the entire settings object.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PanelGeometry {
    panel_x: Option<f64>,
    panel_y: Option<f64>,
    panel_width: f64,
    panel_height: f64,
}

fn persist_panel_geometry(conn: &Connection, value: PanelGeometry) -> Result<()> {
    if !value.panel_width.is_finite()
        || !value.panel_height.is_finite()
        || value.panel_x.is_some_and(|v| !v.is_finite())
        || value.panel_y.is_some_and(|v| !v.is_finite())
    {
        return Err("面板位置或尺寸无效".into());
    }
    let mut current = settings(conn)?;
    current.panel_x = value.panel_x;
    current.panel_y = value.panel_y;
    current.panel_width = value.panel_width;
    current.panel_height = value.panel_height;
    current.validate()?;
    conn.execute(
        "UPDATE settings SET value=?1 WHERE key='main'",
        [serde_json::to_string(&current).map_err(err)?],
    )
    .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub(super) fn save_panel_geometry(store: tauri::State<Store>, value: PanelGeometry) -> Result<()> {
    let _data = store.read_data()?;
    let conn = store.db.lock().map_err(err)?;
    persist_panel_geometry(&conn, value)
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum SettingsSaveResult {
    Saved,
    ConfirmationRequired { count: usize },
}

#[tauri::command]
pub(super) fn get_theme(store: tauri::State<Store>) -> Result<String> {
    let conn = store.db.lock().map_err(err)?;
    Ok(settings(&conn)?.theme)
}

#[tauri::command]
pub(super) fn get_font(store: tauri::State<Store>) -> Result<super::fonts::FontSelection> {
    let conn = store.db.lock().map_err(err)?;
    let settings = settings(&conn)?;
    Ok(super::fonts::FontSelection {
        size: settings.font_size,
        family: settings.font_family,
        style: settings.font_style,
    })
}

#[tauri::command]
pub(super) fn save_settings(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    value: Settings,
    confirmed_count: Option<usize>,
) -> Result<SettingsSaveResult> {
    let _data = store.read_data()?;
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
    {
        let lifecycle = app.state::<PanelLifecycle>();
        let mut state = lifecycle.state.lock().map_err(err)?;
        state.clear_hidden_search(value.retain_search_input);
        state.clear_hidden_type_filter(value.retain_type_filter);
        state.set_background_resident(value.background_resident);
        let hidden = state.hidden_at.is_some();
        let generation = state.generation;
        drop(state);
        if previous.background_resident != value.background_resident {
            if value.background_resident {
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(error) = resume_panel(&window) {
                        eprintln!("Failed to resume resident panel: {error}");
                    }
                }
            }
            if hidden {
                if let Err(error) = app.emit_to("main", "panel-hidden", generation) {
                    eprintln!("Failed to synchronize resident panel: {error}");
                }
            }
        }
    }
    for id in &ids {
        remove_record(&conn, &store.dir, id)?;
    }
    drop(conn);
    if !ids.is_empty() {
        changed(&app);
    }
    Ok(SettingsSaveResult::Saved)
}

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
