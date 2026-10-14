#[cfg(windows)]
use super::webview::filter_browser_context_menu;
use super::{
    common::{err, Result},
    panel_runtime::{resume_panel, suspend_panel},
    preview::hide_preview_window,
    session::{PanelIdlePhase, PanelLifecycle, PanelSession},
    storage::{settings, Store},
    targets::remember_paste_target,
};
use crate::platform;
use std::{thread, time::Instant};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
fn create_panel(app: &tauri::AppHandle) -> Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "main")
        .ok_or("面板窗口配置不存在")?;
    let settings = {
        let store = app.state::<Store>();
        let conn = store.db.lock().map_err(err)?;
        settings(&conn)?
    };
    let builder = tauri::WebviewWindowBuilder::from_config(app, config).map_err(err)?;
    app.state::<PanelLifecycle>()
        .state
        .lock()
        .map_err(err)?
        .set_background_resident(settings.background_resident);
    #[cfg(windows)]
    let builder = builder.skip_taskbar(!settings.show_taskbar_icon);
    let window = builder
        .always_on_top(settings.always_on_top)
        .on_page_load(|window, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                let visible = window.is_visible().unwrap_or(false);
                if let Ok(mut state) = window.state::<PanelLifecycle>().state.lock() {
                    state.document_loading(visible);
                }
            }
        })
        .build()
        .map_err(err)?;
    #[cfg(windows)]
    {
        window
            .with_webview(|webview| unsafe {
                use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
                use windows::core::Interface;
                let result = webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.Settings())
                    .and_then(|settings| settings.cast::<ICoreWebView2Settings3>())
                    .and_then(|settings| settings.SetAreBrowserAcceleratorKeysEnabled(false));
                if let Err(error) = result {
                    eprintln!("Failed to disable panel browser shortcuts: {error}");
                }
            })
            .map_err(err)?;
        filter_browser_context_menu(&window).map_err(err)?;
        platform::install_panel_resize_double_click(window.hwnd().map_err(err)?).map_err(err)?;
    }
    #[cfg(target_os = "macos")]
    platform::install_panel_resize_double_click_macos(window.ns_window().map_err(err)?)
        .map_err(err)?;
    Ok(())
}

// Window creation and destruction are serialized on Tauri's main thread.
pub(super) fn open_panel(app: &tauri::AppHandle) -> Result<()> {
    let lifecycle = app.state::<PanelLifecycle>();
    let mut state = lifecycle.state.lock().map_err(err)?;
    let ready = state.ready;
    let generation = state.generation;
    if ready {
        state.open_requested = false;
    }
    drop(state);
    if app.get_webview_window("main").is_none() {
        if let Err(error) = create_panel(app) {
            lifecycle.state.lock().map_err(err)?.open_requested = false;
            return Err(error);
        }
    } else if ready {
        let window = app.get_webview_window("main").ok_or("面板窗口不存在")?;
        resume_panel(&window).map_err(err)?;
        app.emit_to("main", "open-requested", generation)
            .map_err(err)?;
    }
    Ok(())
}

pub(super) fn request_panel(app: &tauri::AppHandle) {
    let _ = remember_paste_target(app.state::<Store>());
    let wake = if let Ok(mut state) = app.state::<PanelLifecycle>().state.lock() {
        let wake = !state.open_requested;
        state.cancel_idle();
        state.open_requested = true;
        wake
    } else {
        false
    };
    if wake {
        // Hotkey messages alone may not wake Tauri. Queue outside the shortcut
        // callback, which holds the plugin lock during its handler.
        let handle = app.clone();
        thread::spawn(move || {
            let _ = handle.run_on_main_thread(|| {});
        });
    }
}

pub(super) fn register_panel_shortcut_inner(app: &tauri::AppHandle, shortcut: &str) -> Result<()> {
    let lifecycle = app.state::<PanelLifecycle>();
    let mut active = lifecycle.shortcut.lock().map_err(err)?;
    if *active == shortcut {
        return Ok(());
    }
    if !shortcut.is_empty() {
        app.global_shortcut()
            .on_shortcut(shortcut, |app, _, event| {
                if event.state == ShortcutState::Pressed {
                    request_panel(app);
                }
            })
            .map_err(err)?;
    }
    if !active.is_empty() {
        if let Err(error) = app.global_shortcut().unregister(active.as_str()) {
            if !shortcut.is_empty() {
                let _ = app.global_shortcut().unregister(shortcut);
            }
            return Err(err(error));
        }
    }
    *active = shortcut.into();
    lifecycle.state.lock().map_err(err)?.shortcut_error = None;
    Ok(())
}

#[tauri::command]
pub(super) async fn register_panel_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<()> {
    register_panel_shortcut_inner(&app, &shortcut)
}

#[tauri::command]
pub(super) fn get_panel_session(lifecycle: tauri::State<PanelLifecycle>) -> Result<PanelSession> {
    Ok(lifecycle.state.lock().map_err(err)?.session.clone())
}

#[tauri::command]
pub(super) fn save_panel_session(
    store: tauri::State<Store>,
    lifecycle: tauri::State<PanelLifecycle>,
    session: PanelSession,
) -> Result<()> {
    let conn = store.db.lock().map_err(err)?;
    let settings = settings(&conn)?;
    let mut state = lifecycle.state.lock().map_err(err)?;
    state.session = session;
    state.clear_hidden_search(settings.retain_search_input);
    state.clear_hidden_type_filter(settings.retain_type_filter);
    Ok(())
}

#[tauri::command]
pub(super) fn panel_ready(
    app: tauri::AppHandle,
    lifecycle: tauri::State<PanelLifecycle>,
) -> Result<Option<String>> {
    let visible = app
        .get_webview_window("main")
        .is_some_and(|window| window.is_visible().unwrap_or(false));
    let mut state = lifecycle.state.lock().map_err(err)?;
    let opening = state.document_ready(visible);
    let generation = state.generation;
    let error = state.shortcut_error.clone();
    drop(state);
    if let Some(generation) = opening {
        app.emit_to("main", "open-requested", generation)
            .map_err(err)?;
    } else {
        // Synchronize a page reloaded while hidden before it receives idle-release.
        app.emit_to("main", "panel-hidden", generation)
            .map_err(err)?;
    }
    Ok(error)
}

pub(super) fn suspend_idle_panel(app: &tauri::AppHandle) {
    let due = app
        .state::<PanelLifecycle>()
        .state
        .lock()
        .is_ok_and(|state| state.idle_due(Instant::now()));
    if !due {
        return;
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let lifecycle = handle.state::<PanelLifecycle>();
        let Ok(mut state) = lifecycle.state.lock() else {
            return;
        };
        let Some(window) = handle.get_webview_window("main") else {
            return;
        };
        if window.is_visible().unwrap_or(true) {
            return;
        }
        let Some(generation) = state.begin_release(Instant::now()) else {
            return;
        };
        drop(state);
        if let Err(error) = handle.emit_to("main", "panel-idle-release", generation) {
            eprintln!("Failed to request panel content release: {error}");
            if let Ok(mut state) = lifecycle.state.lock() {
                if state.generation == generation && state.idle_phase == PanelIdlePhase::Clearing {
                    state.idle_phase = PanelIdlePhase::Awake;
                }
            }
        }
    });
}

#[tauri::command]
pub(super) fn panel_content_released(app: tauri::AppHandle, generation: u64) -> Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window("main") else {
            return;
        };
        if window.is_visible().unwrap_or(true) {
            return;
        }
        let lifecycle = handle.state::<PanelLifecycle>();
        let Ok(mut state) = lifecycle.state.lock() else {
            return;
        };
        if !state.begin_suspend(generation) {
            return;
        }
        drop(state);
        for label in ["preview", "large-preview"] {
            if let Some(window) = handle.get_webview_window(label) {
                if window.is_visible().is_ok_and(|visible| !visible) {
                    if let Err(error) = window.destroy() {
                        eprintln!("Failed to release {label}: {error}");
                    }
                }
            }
        }
        let callback = handle.clone();
        if let Err(error) = suspend_panel(&window, move |succeeded| {
            finish_panel_suspend(&callback, generation, succeeded);
        }) {
            eprintln!("Failed to access panel for suspension: {error}");
            finish_panel_suspend(&handle, generation, false);
        }
    })
    .map_err(err)
}

fn finish_panel_suspend(app: &tauri::AppHandle, generation: u64, succeeded: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let lifecycle = handle.state::<PanelLifecycle>();
        let Ok(mut state) = lifecycle.state.lock() else {
            return;
        };
        let accepted = state.finish_suspend(generation, succeeded);
        let resume = !accepted && state.idle_phase == PanelIdlePhase::Awake;
        drop(state);
        // A shortcut can arrive while the native suspend request is still completing.
        if resume {
            if let Some(window) = handle.get_webview_window("main") {
                let _ = resume_panel(&window);
            }
        }
    });
}

#[tauri::command]
pub(super) fn show_panel(app: tauri::AppHandle, store: tauri::State<Store>) -> Result<()> {
    remember_paste_target(store)?;
    let window = app.get_webview_window("main").ok_or("面板窗口不存在")?;
    {
        let lifecycle = app.state::<PanelLifecycle>();
        let mut state = lifecycle.state.lock().map_err(err)?;
        state.cancel_idle();
        state.open_requested = false;
    }
    resume_panel(&window).map_err(err)?;
    window.show().map_err(err)?;
    window.unminimize().map_err(err)?;
    window.set_focus().map_err(err)?;
    Ok(())
}

#[tauri::command]
pub(super) fn hide_panel(app: tauri::AppHandle) -> Result<()> {
    hide_preview_window(&app, "preview")?;
    hide_preview_window(&app, "large-preview")?;
    if let Some(window) = app.get_webview_window("main") {
        let store = app.state::<Store>();
        let conn = store.db.lock().map_err(err)?;
        let settings = settings(&conn)?;
        window.hide().map_err(err)?;
        let (newly_hidden, generation) = {
            let lifecycle = app.state::<PanelLifecycle>();
            let mut state = lifecycle.state.lock().map_err(err)?;
            state.open_requested = false;
            let newly_hidden = state.mark_hidden();
            state.clear_hidden_search(settings.retain_search_input);
            state.clear_hidden_type_filter(settings.retain_type_filter);
            (newly_hidden, state.generation)
        };
        drop(conn);
        if newly_hidden {
            app.emit_to("main", "panel-hidden", generation)
                .map_err(err)?;
        }
    }
    Ok(())
}
