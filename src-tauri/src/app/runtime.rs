use super::{
    cleanup::clean,
    clipboard::Watcher,
    events::changed,
    panel::{request_panel, suspend_idle_panel},
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
        suspend_idle_panel(&handle);
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
    #[cfg(target_os = "macos")]
    let icon_bytes = include_bytes!("../../icons/tray-template.png").as_slice();
    #[cfg(windows)]
    let icon_bytes = include_bytes!("../../icons/tray-windows.png").as_slice();
    let rgba = image::load_from_memory(icon_bytes)
        .map_err(|error| {
            tauri::Error::InvalidIcon(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })?
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    let tray = TrayIconBuilder::new().icon(tauri::image::Image::new_owned(
        rgba.into_raw(),
        width,
        height,
    ));
    #[cfg(target_os = "macos")]
    let tray = tray.icon_as_template(true);
    tray.tooltip("TP")
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
