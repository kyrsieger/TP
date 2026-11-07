use dpi::{PhysicalPosition, PhysicalSize};
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tp_core::{
    panel_geometry::{panel_position, WorkArea},
    settings::Settings,
};

/// Apply saved physical client size and original placement rules before revealing the panel.
pub fn place(window: &Window, value: &Settings) -> Result<(), String> {
    value.validate().map_err(tp_core::i18n::tr)?;
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::{HWND, POINT, RECT},
            Graphics::Gdi::{
                GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO,
                MONITOR_DEFAULTTONEAREST,
            },
            UI::WindowsAndMessaging::{
                GetClientRect, GetCursorPos, GetWindowRect, SetWindowPos, SWP_NOACTIVATE,
                SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            },
        };
        let hwnd = HWND(handle.hwnd.get() as _);
        return (|| -> windows::core::Result<()> {
            unsafe {
                let mut outer = RECT::default();
                let mut client = RECT::default();
                GetWindowRect(hwnd, &mut outer)?;
                GetClientRect(hwnd, &mut client)?;
                // Account for the current native frame; saved dimensions describe the client area.
                let width = value.panel_width.round() as i32 + outer.right
                    - outer.left
                    - (client.right - client.left);
                let height = value.panel_height.round() as i32 + outer.bottom
                    - outer.top
                    - (client.bottom - client.top);
                SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE,
                )?;
                GetWindowRect(hwnd, &mut outer)?;
                let mut cursor = POINT::default();
                let following = value.position_mode == "cursor";
                let monitor = if following {
                    GetCursorPos(&mut cursor)?;
                    MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
                } else {
                    MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
                };
                let mut info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                GetMonitorInfoW(monitor, &mut info).ok()?;
                let r = info.rcWork;
                let area = WorkArea {
                    position: PhysicalPosition::new(r.left, r.top),
                    size: PhysicalSize::new((r.right - r.left) as u32, (r.bottom - r.top) as u32),
                };
                let size = PhysicalSize::new(
                    (outer.right - outer.left) as u32,
                    (outer.bottom - outer.top) as u32,
                );
                let cursor =
                    following.then(|| PhysicalPosition::new(cursor.x as f64, cursor.y as f64));
                let position = panel_position(value, &area, size, cursor);
                SetWindowPos(
                    hwnd,
                    None,
                    position.x,
                    position.y,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                )?;
                Ok(())
            }
        })()
        .map_err(|e| e.to_string());
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSEvent, NSScreen, NSView};
        use objc2_foundation::{NSPoint, NSSize};
        let mtm = MainThreadMarker::new().ok_or("Window placement requires the main thread")?;
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
        let native = view.window().ok_or("Panel window is unavailable")?;
        let screens = NSScreen::screens(mtm);
        let primary = screens.firstObject().ok_or("No display is available")?;
        let primary_height = primary.frame().size.height;
        let mouse = NSEvent::mouseLocation();
        let following = value.position_mode == "cursor";
        let target = if following {
            screens.iter().find(|screen| {
                let r = screen.frame();
                mouse.x >= r.origin.x
                    && mouse.x < r.origin.x + r.size.width
                    && mouse.y >= r.origin.y
                    && mouse.y < r.origin.y + r.size.height
            })
        } else {
            native.screen()
        }
        .unwrap_or_else(|| primary.clone());
        let scale = native.backingScaleFactor();
        native.setContentSize(NSSize::new(
            value.panel_width.round() / scale,
            value.panel_height.round() / scale,
        ));
        let frame = native.frame();
        let visible = target.visibleFrame();
        let target_scale = target.backingScaleFactor();
        let area = WorkArea {
            position: PhysicalPosition::new(
                (visible.origin.x * target_scale).round() as i32,
                ((primary_height - visible.origin.y - visible.size.height) * target_scale).round()
                    as i32,
            ),
            size: PhysicalSize::new(
                (visible.size.width * target_scale).round() as u32,
                (visible.size.height * target_scale).round() as u32,
            ),
        };
        let size = PhysicalSize::new(
            (frame.size.width * scale).round() as u32,
            (frame.size.height * scale).round() as u32,
        );
        let cursor = following.then(|| {
            PhysicalPosition::new(
                mouse.x * primary.backingScaleFactor(),
                (primary_height - mouse.y) * primary.backingScaleFactor(),
            )
        });
        let position = panel_position(value, &area, size, cursor);
        native.setFrameTopLeftPoint(NSPoint::new(
            position.x as f64 / scale,
            primary_height - position.y as f64 / scale,
        ));
        return Ok(());
    }
    Err(tp_core::i18n::tr("面板窗口不存在").into())
}
