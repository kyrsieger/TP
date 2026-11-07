//! Native coordinates for the floating card; the shared core owns placement policy.
use dpi::{PhysicalPosition, PhysicalSize};
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tp_core::{
    hover_preview_geometry::{self, Placement},
    panel_geometry::WorkArea,
};

pub struct Anchor {
    pub placement: Placement,
    #[cfg(target_os = "macos")]
    scale: f64,
    #[cfg(target_os = "macos")]
    primary_height: f64,
}

/// Take one coherent screen/owner snapshot before laying out a preview card.
/// `row_top` is relative to the panel client area, in logical pixels.
pub fn anchor(panel: &Window, row_top: f64) -> Result<Anchor, String> {
    let handle = HasWindowHandle::window_handle(panel).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        use windows::Win32::{
            Foundation::{HWND, POINT, RECT},
            Graphics::Gdi::{
                ClientToScreen, GetMonitorInfoW, MonitorFromPoint, MONITORINFO,
                MONITOR_DEFAULTTONEAREST,
            },
            UI::WindowsAndMessaging::GetWindowRect,
        };
        return (|| -> windows::core::Result<Anchor> {
            unsafe {
                let hwnd = HWND(handle.hwnd.get() as _);
                let mut outer = RECT::default();
                GetWindowRect(hwnd, &mut outer)?;
                let mut client = POINT::default();
                ClientToScreen(hwnd, &mut client).ok()?;
                let monitor = MonitorFromPoint(
                    POINT {
                        x: outer.left + (outer.right - outer.left) / 2,
                        y: outer.top + (outer.bottom - outer.top) / 2,
                    },
                    MONITOR_DEFAULTTONEAREST,
                );
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
                let scale = f64::from(panel.scale_factor());
                // Native frame height is not part of GPUI row bounds.
                let row_top = row_top + f64::from(client.y - outer.top) / scale;
                Ok(Anchor {
                    placement: hover_preview_geometry::place(
                        &area,
                        PhysicalPosition::new(outer.left, outer.top),
                        PhysicalSize::new(
                            (outer.right - outer.left) as u32,
                            (outer.bottom - outer.top) as u32,
                        ),
                        scale,
                        row_top,
                    ),
                })
            }
        })()
        .map_err(|e| e.to_string());
    }
    #[cfg(target_os = "macos")]
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        use objc2_app_kit::{NSScreen, NSView};
        let mtm =
            objc2::MainThreadMarker::new().ok_or("Preview placement requires the main thread")?;
        let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
        let native = view.window().ok_or("Panel window is unavailable")?;
        let screens = NSScreen::screens(mtm);
        let primary = screens.firstObject().ok_or("No display is available")?;
        let frame = native.frame();
        let center_x = frame.origin.x + frame.size.width / 2.;
        let center_y = frame.origin.y + frame.size.height / 2.;
        let target = screens
            .iter()
            .find(|screen| {
                let r = screen.frame();
                center_x >= r.origin.x
                    && center_x < r.origin.x + r.size.width
                    && center_y >= r.origin.y
                    && center_y < r.origin.y + r.size.height
            })
            .or_else(|| native.screen())
            .unwrap_or(primary.clone());
        let visible = target.visibleFrame();
        let scale = native.backingScaleFactor();
        let primary_height = primary.frame().size.height;
        // Use one scale for the common coordinate space; mixed-DPI screen origins
        // must not be independently multiplied by each screen's backing factor.
        let area = WorkArea {
            position: PhysicalPosition::new(
                (visible.origin.x * scale).round() as i32,
                ((primary_height - visible.origin.y - visible.size.height) * scale).round() as i32,
            ),
            size: PhysicalSize::new(
                (visible.size.width * scale).round() as u32,
                (visible.size.height * scale).round() as u32,
            ),
        };
        let content = native.contentRectForFrameRect(frame);
        let top_inset = frame.origin.y + frame.size.height - content.origin.y - content.size.height;
        let placement = hover_preview_geometry::place(
            &area,
            PhysicalPosition::new(
                (frame.origin.x * scale).round() as i32,
                ((primary_height - frame.origin.y - frame.size.height) * scale).round() as i32,
            ),
            PhysicalSize::new(
                (frame.size.width * scale).round() as u32,
                (frame.size.height * scale).round() as u32,
            ),
            scale,
            row_top + top_inset,
        );
        return Ok(Anchor {
            placement,
            scale,
            primary_height,
        });
    }
    Err(tp_core::i18n::tr("面板窗口不存在").into())
}

impl Anchor {
    /// Size and position a borderless card without making it the key window.
    /// Content must be ready before calling this to avoid flashing an empty preview.
    pub fn reveal(&self, handle: RawWindowHandle, measured_height: f64) -> Result<(), String> {
        let bounds = self.placement.measured(measured_height);
        #[cfg(windows)]
        if let RawWindowHandle::Win32(handle) = handle {
            use windows::Win32::{
                Foundation::{HWND, POINT, RECT},
                Graphics::Gdi::ClientToScreen,
                UI::WindowsAndMessaging::{
                    GetClientRect, GetWindowRect, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE,
                    SWP_SHOWWINDOW,
                },
            };
            return unsafe {
                let hwnd = HWND(handle.hwnd.get() as _);
                let mut outer = RECT::default();
                let mut client = RECT::default();
                let mut origin = POINT::default();
                // GPUI's hidden-titlebar Windows still have non-client insets.
                // Placement and measured height describe the card's client area,
                // not the native frame; preserve that size and screen position.
                GetWindowRect(hwnd, &mut outer)
                    .and_then(|_| GetClientRect(hwnd, &mut client))
                    .and_then(|_| ClientToScreen(hwnd, &mut origin).ok())
                    .map_err(|e| e.to_string())?;
                let left = origin.x - outer.left;
                let top = origin.y - outer.top;
                let right = outer.right - origin.x - (client.right - client.left);
                let bottom = outer.bottom - origin.y - (client.bottom - client.top);
                SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    bounds.x.round() as i32 - left,
                    bounds.y.round() as i32 - top,
                    bounds.width.round() as i32 + left + right,
                    bounds.height.round() as i32 + top + bottom,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            }
            .map_err(|e| e.to_string());
        }
        #[cfg(target_os = "macos")]
        if let RawWindowHandle::AppKit(handle) = handle {
            use objc2_app_kit::{NSFloatingWindowLevel, NSView};
            use objc2_foundation::{NSPoint, NSSize};
            let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
            let native = view.window().ok_or("Preview window is unavailable")?;
            native.setLevel(NSFloatingWindowLevel);
            native.setContentSize(NSSize::new(
                bounds.width / self.scale,
                bounds.height / self.scale,
            ));
            native.setFrameTopLeftPoint(NSPoint::new(
                bounds.x / self.scale,
                self.primary_height - bounds.y / self.scale,
            ));
            native.orderFront(None);
            return Ok(());
        }
        Err(tp_core::i18n::tr("预览窗口不存在").into())
    }
}
