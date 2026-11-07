pub mod hover_preview;
mod ime;
pub use ime::install_ime_guard;
mod placement;
pub mod resize;
use gpui::Window;
pub use placement::place;

pub fn reduced_motion() -> bool {
    #[cfg(windows)]
    {
        use windows::{
            core::BOOL,
            Win32::UI::WindowsAndMessaging::{
                SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            },
        };
        let mut enabled = BOOL(1);
        let result = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                Some((&mut enabled as *mut BOOL).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        result.is_ok() && !enabled.as_bool()
    }
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        false
    }
}

pub fn geometry(window: &Window) -> Result<tp_core::preferences::PanelGeometry, String> {
    use tp_core::preferences::PanelGeometry;
    if window.is_maximized() || window.is_fullscreen() {
        return Err("Panel has no normal window geometry".into());
    }
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::{HWND, RECT},
            UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, IsIconic},
        };
        let hwnd = HWND(handle.hwnd.get() as _);
        unsafe {
            if IsIconic(hwnd).as_bool() {
                return Err("Panel is minimized".into());
            }
            let mut outer = RECT::default();
            let mut client = RECT::default();
            GetWindowRect(hwnd, &mut outer).map_err(|e| e.to_string())?;
            GetClientRect(hwnd, &mut client).map_err(|e| e.to_string())?;
            return Ok(PanelGeometry {
                panel_x: Some(outer.left as f64),
                panel_y: Some(outer.top as f64),
                panel_width: (client.right - client.left) as f64,
                panel_height: (client.bottom - client.top) as f64,
            });
        }
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        use objc2_app_kit::{NSScreen, NSView};
        let mtm =
            objc2::MainThreadMarker::new().ok_or("Window geometry requires the main thread")?;
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
        let native = view.window().ok_or("Panel window is unavailable")?;
        if native.isMiniaturized() {
            return Err("Panel is minimized".into());
        }
        let primary = NSScreen::screens(mtm)
            .firstObject()
            .ok_or("No display is available")?;
        let frame = native.frame();
        let content = native.contentRectForFrameRect(frame);
        let scale = native.backingScaleFactor();
        return Ok(PanelGeometry {
            panel_x: Some((frame.origin.x * scale).round()),
            panel_y: Some(
                ((primary.frame().size.height - frame.origin.y - frame.size.height) * scale)
                    .round(),
            ),
            panel_width: (content.size.width * scale).round(),
            panel_height: (content.size.height * scale).round(),
        });
    }
    Err(tp_core::i18n::tr("面板窗口不存在").into())
}
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Start the native drag without entering a Windows move loop inside GPUI's input borrow.
pub fn start_move(window: &Window) -> Result<(), String> {
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            UI::{
                Input::KeyboardAndMouse::ReleaseCapture,
                WindowsAndMessaging::{PostMessageW, SC_MOVE, WM_SYSCOMMAND},
            },
        };
        unsafe {
            let _ = ReleaseCapture();
            return PostMessageW(
                Some(HWND(handle.hwnd.get() as _)),
                WM_SYSCOMMAND,
                WPARAM((SC_MOVE | 2) as usize),
                LPARAM(0),
            )
            .map_err(|e| e.to_string());
        }
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        use objc2_app_kit::{NSApplication, NSView};
        let mtm = objc2::MainThreadMarker::new().ok_or("Window drag requires the main thread")?;
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
        let native = view.window().ok_or("Panel window is unavailable")?;
        let event = NSApplication::sharedApplication(mtm)
            .currentEvent()
            .ok_or("No window drag event")?;
        native.performWindowDragWithEvent(&event);
        return Ok(());
    }
    Err(tp_core::i18n::tr("面板窗口不存在").into())
}

/// GPUI's Windows zoom only maximizes; macOS zoom already toggles restoration.
pub fn toggle_maximize(window: &mut Window) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{IsZoomed, ShowWindowAsync, SW_MAXIMIZE, SW_RESTORE},
        };
        let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(tp_core::i18n::tr("面板窗口不存在").into());
        };
        let hwnd = HWND(handle.hwnd.get() as _);
        unsafe {
            ShowWindowAsync(
                hwnd,
                if IsZoomed(hwnd).as_bool() {
                    SW_RESTORE
                } else {
                    SW_MAXIMIZE
                },
            )
            .ok()
            .map_err(|e| e.to_string())
        }
    }
    #[cfg(not(windows))]
    {
        window.zoom_window();
        Ok(())
    }
}

/// GPUI initializes OLE on its UI thread before creating windows.
/// Match the original Windows taskbar option without changing Alt-Tab or window styles.
pub fn set_taskbar_icon(window: &Window, enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HWND,
            System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            UI::Shell::{ITaskbarList, TaskbarList},
        };
        let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(tp_core::i18n::tr("面板窗口不存在").into());
        };
        let result = unsafe {
            (|| -> windows::core::Result<()> {
                let taskbar: ITaskbarList =
                    CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
                taskbar.HrInit()?;
                let hwnd = HWND(handle.hwnd.get() as _);
                if enabled {
                    taskbar.AddTab(hwnd)
                } else {
                    taskbar.DeleteTab(hwnd)
                }
            })()
        };
        result.map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = (window, enabled);
        Ok(())
    }
}

/// Called on the UI thread. Changing the level must not move, resize or activate the panel.
pub fn set_always_on_top(window: &Window, enabled: bool) -> Result<(), String> {
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            },
        };
        return unsafe {
            SetWindowPos(
                HWND(handle.hwnd.get() as _),
                Some(if enabled {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                }),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
        }
        .map_err(|error| error.to_string());
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        use objc2_app_kit::{NSFloatingWindowLevel, NSNormalWindowLevel};
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const objc2_app_kit::NSView) };
        if let Some(window) = view.window() {
            window.setLevel(if enabled {
                NSFloatingWindowLevel
            } else {
                NSNormalWindowLevel
            });
            return Ok(());
        }
    }
    Err(tp_core::i18n::tr("面板窗口不存在").into())
}

pub fn hide(window: &Window) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{ShowWindow, SW_HIDE},
        };
        unsafe {
            let _ = ShowWindow(HWND(handle.hwnd.get() as _), SW_HIDE);
        }
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const objc2_app_kit::NSView) };
        if let Some(window) = view.window() {
            window.orderOut(None);
        }
    }
}

pub fn show(window: &Window) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{ShowWindow, SW_SHOW},
        };
        unsafe {
            let _ = ShowWindow(HWND(handle.hwnd.get() as _), SW_SHOW);
        }
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const objc2_app_kit::NSView) };
        if let Some(window) = view.window() {
            window.makeKeyAndOrderFront(None);
        }
    }
    window.activate_window();
}

/// The shared Windows preview uses the panel's monitor when it first opens.
pub fn owner(window: &Window) -> usize {
    #[cfg(windows)]
    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            return handle.hwnd.get() as usize;
        }
    }
    #[cfg(not(windows))]
    let _ = window;
    0
}
