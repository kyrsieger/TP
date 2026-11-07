use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

pub struct Session {
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(target_os = "macos")]
    native: objc2::rc::Retained<objc2_app_kit::NSWindow>,
    #[cfg(target_os = "macos")]
    frame: objc2_foundation::NSRect,
    #[cfg(target_os = "macos")]
    mouse: objc2_foundation::NSPoint,
    edge: Edge,
    #[cfg(windows)]
    frame: windows::Win32::Foundation::RECT,
    #[cfg(windows)]
    mouse: windows::Win32::Foundation::POINT,
    #[cfg(windows)]
    minimum: (i32, i32),
}

pub fn begin(
    window: &Window,
    edge: Edge,
    position: gpui::Point<gpui::Pixels>,
) -> Result<Option<Session>, String> {
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::{HWND, POINT, RECT},
            Graphics::Gdi::ClientToScreen,
            UI::WindowsAndMessaging::{GetClientRect, GetWindowRect},
        };
        let hwnd = HWND(handle.hwnd.get() as _);
        let scale = window.scale_factor();
        let mut mouse = POINT {
            x: (f32::from(position.x) * scale).round() as i32,
            y: (f32::from(position.y) * scale).round() as i32,
        };
        let mut frame = RECT::default();
        let mut client = RECT::default();
        unsafe {
            ClientToScreen(hwnd, &mut mouse)
                .ok()
                .map_err(|e| e.to_string())?;
            GetWindowRect(hwnd, &mut frame).map_err(|e| e.to_string())?;
            GetClientRect(hwnd, &mut client).map_err(|e| e.to_string())?;
        }
        let minimum = (
            (340. * scale).round() as i32 + frame.right - frame.left - client.right + client.left,
            (400. * scale).round() as i32 + frame.bottom - frame.top - client.bottom + client.top,
        );
        return Ok(Some(Session {
            hwnd: handle.hwnd.get(),
            frame,
            mouse,
            minimum,
            edge,
        }));
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        let _ = position;
        use objc2_app_kit::{NSEvent, NSView};
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
        let native = view.window().ok_or("Panel window is unavailable")?;
        return Ok(Some(Session {
            frame: native.frame(),
            native,
            mouse: NSEvent::mouseLocation(),
            edge,
        }));
    }
    Err("Panel window is unavailable".into())
}

impl Session {
    /// Poll screen coordinates so dragging continues beyond the panel and applies the release position.
    pub fn update(&self) -> Result<bool, String> {
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSEvent;
            let native = &self.native;
            let mouse = NSEvent::mouseLocation();
            let dx = mouse.x - self.mouse.x;
            let dy = mouse.y - self.mouse.y;
            let mut frame = self.frame;
            match self.edge {
                Edge::Left => {
                    frame.size.width = (frame.size.width - dx).max(340.);
                    frame.origin.x += self.frame.size.width - frame.size.width;
                }
                Edge::Right => frame.size.width = (frame.size.width + dx).max(340.),
                Edge::Top => frame.size.height = (frame.size.height + dy).max(400.),
                Edge::Bottom => {
                    frame.size.height = (frame.size.height - dy).max(400.);
                    frame.origin.y += self.frame.size.height - frame.size.height;
                }
            }
            native.setFrame_display(frame, true);
            return Ok(NSEvent::pressedMouseButtons() & 1 != 0);
        }
        #[cfg(windows)]
        {
            use windows::Win32::{
                Foundation::{HWND, POINT},
                UI::{
                    Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
                    WindowsAndMessaging::{
                        GetCursorPos, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
                    },
                },
            };
            let mut mouse = POINT::default();
            unsafe {
                GetCursorPos(&mut mouse).map_err(|e| e.to_string())?;
                let dx = mouse.x - self.mouse.x;
                let dy = mouse.y - self.mouse.y;
                let mut frame = self.frame;
                match self.edge {
                    Edge::Left => frame.left = (frame.left + dx).min(frame.right - self.minimum.0),
                    Edge::Right => {
                        frame.right = (frame.right + dx).max(frame.left + self.minimum.0)
                    }
                    Edge::Top => frame.top = (frame.top + dy).min(frame.bottom - self.minimum.1),
                    Edge::Bottom => {
                        frame.bottom = (frame.bottom + dy).max(frame.top + self.minimum.1)
                    }
                }
                SetWindowPos(
                    HWND(self.hwnd as _),
                    None,
                    frame.left,
                    frame.top,
                    frame.right - frame.left,
                    frame.bottom - frame.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
                .map_err(|e| e.to_string())?;
                Ok(GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0)
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Ok(false)
        }
    }
}

pub fn reset(window: &Window, edge: Edge) -> Result<impl FnOnce() -> Result<(), String>, String> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|e| e.to_string())?
        .as_raw();
    let scale = window.scale_factor();
    Ok(move || apply_reset(handle, scale, edge))
}

fn apply_reset(handle: RawWindowHandle, scale: f32, edge: Edge) -> Result<(), String> {
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle {
        use windows::Win32::{
            Foundation::{HWND, RECT},
            UI::WindowsAndMessaging::{
                GetClientRect, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE,
                SWP_NOZORDER,
            },
        };
        let hwnd = HWND(handle.hwnd.get() as _);
        unsafe {
            let mut outer = RECT::default();
            let mut client = RECT::default();
            GetWindowRect(hwnd, &mut outer).map_err(|e| e.to_string())?;
            GetClientRect(hwnd, &mut client).map_err(|e| e.to_string())?;
            let width = outer.right - outer.left;
            let height = outer.bottom - outer.top;
            return SetWindowPos(
                hwnd,
                None,
                0,
                0,
                if edge.horizontal() {
                    (420. * scale).round() as i32 + width - (client.right - client.left)
                } else {
                    width
                },
                if edge.horizontal() {
                    height
                } else {
                    (600. * scale).round() as i32 + height - (client.bottom - client.top)
                },
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
            )
            .map_err(|e| e.to_string());
        }
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle {
        use objc2_app_kit::NSView;
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
        let native = view.window().ok_or("Panel window is unavailable")?;
        let frame = native.frame();
        let mut size = native.contentRectForFrameRect(frame).size;
        if edge.horizontal() {
            size.width = 420.;
        } else {
            size.height = 600.;
        }
        native.setContentSize(size);
        return Ok(());
    }
    Err("Panel window is unavailable".into())
}
