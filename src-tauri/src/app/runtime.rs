use super::{
    cleanup::clean,
    clipboard::Watcher,
    events::changed,
    panel::{release_idle_panel, request_panel},
    storage::Store,
};
use crate::platform;
use clipboard_rs::{ClipboardContext, ClipboardWatcher, ClipboardWatcherContext};
use std::{thread, time::Duration};
use tauri::Manager;
pub(super) fn start(app: &tauri::AppHandle) {
    let handle = app.clone();
    thread::spawn(move || {
        if let (Ok(ctx), Ok(mut watcher)) =
            (ClipboardContext::new(), ClipboardWatcherContext::new())
        {
            watcher.add_handler(Watcher { app: handle, ctx });
            watcher.start_watch();
        }
    });
    let handle = app.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        if let Some(store) = handle.try_state::<Store>() {
            if matches!(clean(&store), Ok(true)) {
                changed(&handle);
            }
        }
        release_idle_panel(&handle);
    });
    let handle = app.clone();
    thread::spawn(move || loop {
        if let (Some(store), Some(target)) = (
            handle.try_state::<Store>(),
            platform::capture_paste_target(),
        ) {
            if let Ok(mut current) = store.paste_target.lock() {
                *current = Some(target);
            }
        }
        thread::sleep(Duration::from_millis(100));
    });
}

pub(super) fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let open = MenuItem::with_id(app, "open", "打开剪贴板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                request_panel(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                request_panel(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
