mod cleanup;
mod clipboard;
mod common;
mod digest;
mod events;
mod files;
mod filesystem;
mod fonts;
mod media;
mod model;
mod panel;
mod panel_geometry;
mod panel_runtime;
mod preview;
mod records;
mod runtime;
mod session;
mod settings;
mod storage;
mod targets;
mod webview;

#[cfg(target_os = "macos")]
use crate::platform;
use panel::{hide_panel, open_panel, register_panel_shortcut_inner};
use preview::hide_preview_window;
use session::PanelLifecycle;
use storage::{initialize, settings, Store};
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PanelLifecycle::default())
        .plugin(
            tauri::plugin::Builder::<_, ()>::new("store")
                .setup(|app, _| {
                    app.manage(
                        initialize(app.path().app_data_dir()?).map_err(std::io::Error::other)?,
                    );
                    Ok(())
                })
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            if let Err(error) = settings::restore_saved_autostart(app.handle()) {
                eprintln!("Failed to restore autostart: {error}");
            }
            let shortcut = {
                let store = app.state::<Store>();
                let conn = store
                    .db
                    .lock()
                    .map_err(|_| std::io::Error::other("无法读取设置"))?;
                settings(&conn).map_err(std::io::Error::other)?.shortcut
            };
            if let Err(error) = register_panel_shortcut_inner(app.handle(), &shortcut) {
                eprintln!("Failed to register panel shortcut: {error}");
                app.state::<PanelLifecycle>()
                    .state
                    .lock()
                    .map_err(|_| std::io::Error::other("无法读取面板状态"))?
                    .shortcut_error = Some(error);
            }
            runtime::start(app.handle());
            runtime::install_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) && window.label() == "main" {
                #[cfg(target_os = "macos")]
                platform::remove_panel_resize_double_click_macos();
                let lifecycle = window.app_handle().state::<PanelLifecycle>();
                if let Ok(mut state) = lifecycle.state.lock() {
                    state.ready = false;
                    state.cancel_idle();
                };
                panel_runtime::forget_panel();
            }
            if matches!(event, tauri::WindowEvent::Destroyed)
                && matches!(window.label(), "preview" | "large-preview")
            {
                let _ = window.app_handle().emit_to(
                    "main",
                    &format!("{}-destroyed", window.label()),
                    (),
                );
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label() == "main" {
                    let _ = hide_panel(window.app_handle().clone());
                } else {
                    let _ = hide_preview_window(window.app_handle(), window.label());
                }
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            storage::get_state,
            files::copy_record_paths,
            files::open_record_directory,
            files::save_record_files,
            settings::autostart_enabled,
            settings::set_autostart,
            settings::save_settings,
            settings::save_panel_geometry,
            settings::get_theme,
            settings::get_font,
            fonts::get_system_fonts,
            records::add_group,
            records::rename_group,
            records::reorder_groups,
            records::rename_record,
            records::delete_group,
            records::set_favorite,
            records::set_record_pinned,
            records::move_group,
            records::delete_record,
            records::batch_records,
            cleanup::clear_cache,
            cleanup::get_cache_size,
            media::preview_data,
            media::file_text_preview,
            targets::remember_paste_target,
            panel::register_panel_shortcut,
            panel::get_panel_session,
            panel::save_panel_session,
            panel::panel_ready,
            panel::panel_content_released,
            panel::debug_release_panel,
            panel::show_panel,
            panel_geometry::place_panel,
            panel::hide_panel,
            preview::ensure_preview_window,
            preview::release_preview_window,
            preview::show_preview,
            preview::hide_preview,
            preview::preview_focused,
            preview::show_large_preview,
            preview::hide_large_preview,
            clipboard::apply_record
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            tauri::RunEvent::MainEventsCleared => {
                let main_absent = app.get_webview_window("main").is_none();
                let reopen = app
                    .state::<PanelLifecycle>()
                    .state
                    .lock()
                    .is_ok_and(|state| state.open_requested && (state.ready || main_absent));
                if reopen {
                    if let Err(error) = open_panel(app) {
                        eprintln!("Failed to reopen panel: {error}");
                    }
                }
            }
            _ => {}
        });
}

#[cfg(test)]
mod test_support;
