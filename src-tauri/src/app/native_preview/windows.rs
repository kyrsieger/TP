use super::*;
use ::windows::{
    core::w,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::GetModuleHandleW, Registry::*},
        UI::{
            HiDpi::*,
            Input::{KeyboardAndMouse::*, *},
            Shell::*,
            WindowsAndMessaging::*,
        },
    },
};
use std::cell::{Cell, RefCell};

const REFRESH_DPI: u32 = WM_APP + 1;

#[derive(Clone, Copy)]
struct PointerDrag {
    last: (f64, f64),
}
impl PointerDrag {
    fn move_to(&mut self, geometry: &mut Geometry, point: (f64, f64)) {
        geometry.move_to(
            geometry.x + point.0 - self.last.0,
            geometry.y + point.1 - self.last.1,
        );
        self.last = point;
    }
}

struct Bitmap {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    width: i32,
    height: i32,
}
impl Bitmap {
    unsafe fn new(width: i32, height: i32, pixels: Option<&[u8]>) -> Result<Self> {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return Err(err(::windows::core::Error::from_win32()));
        }
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(v) => v,
            Err(e) => {
                let _ = DeleteDC(dc);
                return Err(err(e));
            }
        };
        if let Some(p) = pixels {
            std::ptr::copy_nonoverlapping(p.as_ptr(), bits.cast(), p.len());
        }
        let old = SelectObject(dc, bitmap.into());
        Ok(Self {
            dc,
            bitmap,
            old,
            width,
            height,
        })
    }
}
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
struct Surface {
    app: tauri::AppHandle,
    picture: Picture,
    geometry: Geometry,
    source: Bitmap,
    target: Option<Bitmap>,
    visible: Rect,
    hovered: bool,
    // Last processed pointer position; incremental dragging survives a DPI rebase.
    drag: Option<PointerDrag>,
    pending_index: usize,
}
thread_local! {
    static SURFACE: RefCell<Option<Surface>> = const { RefCell::new(None) };
    static WINDOW: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
}
unsafe fn work_area(monitor: HMONITOR) -> Result<Rect> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    GetMonitorInfoW(monitor, &mut info).ok().map_err(err)?;
    let mut r = info.rcWork;
    // Reserve the auto-hide bar's reveal edge too, so a borderless window never
    // blocks its activation strip. Do not alter the user's taskbar settings.
    for edge in [ABE_LEFT, ABE_TOP, ABE_RIGHT, ABE_BOTTOM] {
        let mut bar = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            uEdge: edge,
            rc: info.rcMonitor,
            ..Default::default()
        };
        if SHAppBarMessage(ABM_GETAUTOHIDEBAREX, &mut bar) != 0 {
            if edge == ABE_LEFT {
                r.left = r.left.max(info.rcMonitor.left + 2);
            }
            if edge == ABE_TOP {
                r.top = r.top.max(info.rcMonitor.top + 2);
            }
            if edge == ABE_RIGHT {
                r.right = r.right.min(info.rcMonitor.right - 2);
            }
            if edge == ABE_BOTTOM {
                r.bottom = r.bottom.min(info.rcMonitor.bottom - 2);
            }
        }
    }
    Ok(Rect {
        x: r.left as f64,
        y: r.top as f64,
        w: (r.right - r.left) as f64,
        h: (r.bottom - r.top) as f64,
    })
}
impl Surface {
    fn navigate(&mut self, direction: isize) {
        step(
            &self.app,
            &self.picture.id,
            self.pending_index,
            self.picture.count,
            direction,
        );
        self.pending_index = (self.pending_index as isize + direction)
            .rem_euclid(self.picture.count as isize) as usize;
    }
    unsafe fn draw(&mut self, hwnd: HWND) -> Result<()> {
        let image = self.geometry.rect();
        let view = image.intersect(self.geometry.work);
        if view.w < 1. || view.h < 1. {
            return Ok(());
        }
        let width = view.w as i32;
        let height = view.h as i32;
        if self
            .target
            .as_ref()
            .is_none_or(|b| b.width < width || b.height < height)
        {
            self.target = Some(Bitmap::new(
                (width + 255) / 256 * 256,
                (height + 255) / 256 * 256,
                None,
            )?);
        }
        let dc = self.target.as_ref().unwrap().dc;
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, None).ok().map_err(err)?;
        StretchBlt(
            dc,
            (image.x - view.x) as i32,
            (image.y - view.y) as i32,
            image.w as i32,
            image.h as i32,
            Some(self.source.dc),
            0,
            0,
            self.source.width,
            self.source.height,
            SRCCOPY,
        )
        .ok()
        .map_err(err)?;
        if self.hovered || self.picture.unavailable.is_some() {
            let dpi = self.geometry.dpi;
            let font = CreateFontW(
                -(13. * dpi).round() as i32,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32,
                w!("Segoe UI"),
            );
            let old_font = SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00ffffff));
            if let Some(message) = self.picture.unavailable {
                let mut text: Vec<u16> = message.encode_utf16().collect();
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                };
                DrawTextW(
                    dc,
                    &mut text,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            for control in controls(
                view.w,
                view.h,
                dpi,
                self.geometry.scale,
                self.picture.index,
                self.picture.count,
            )
            .into_iter()
            .filter(|_| self.hovered)
            {
                let r = control.rect;
                // Fill only: close control has no white border.
                if matches!(control.action, Some(Action::Close)) {
                    let shadow = CreateSolidBrush(COLORREF(0x00101010));
                    let old = SelectObject(dc, shadow.into());
                    let pen = SelectObject(dc, GetStockObject(NULL_PEN));
                    let offset = (2. * dpi).round() as i32;
                    let _ = RoundRect(
                        dc,
                        r.x as i32,
                        r.y as i32 + offset,
                        (r.x + r.w) as i32,
                        (r.y + r.h) as i32 + offset,
                        (12. * dpi) as i32,
                        (12. * dpi) as i32,
                    );
                    SelectObject(dc, old);
                    SelectObject(dc, pen);
                    let _ = DeleteObject(shadow.into());
                }
                let brush = CreateSolidBrush(COLORREF(0x00271811));
                let old_brush = SelectObject(dc, brush.into());
                let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
                let _ = RoundRect(
                    dc,
                    r.x as i32,
                    r.y as i32,
                    (r.x + r.w) as i32,
                    (r.y + r.h) as i32,
                    (8. * dpi) as i32,
                    (8. * dpi) as i32,
                );
                SelectObject(dc, old_brush);
                SelectObject(dc, old_pen);
                let _ = DeleteObject(brush.into());
                let mut rect = RECT {
                    left: r.x as i32,
                    top: r.y as i32,
                    right: (r.x + r.w) as i32,
                    bottom: (r.y + r.h) as i32,
                };
                let mut text: Vec<u16> = control.text.encode_utf16().collect();
                DrawTextW(
                    dc,
                    &mut text,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
            }
            if self.hovered && self.picture.video && height > 65 {
                let mut text: Vec<u16> = "视频首帧".encode_utf16().collect();
                let mut rect = RECT {
                    left: 8,
                    top: height - 28,
                    right: 160,
                    bottom: height - 5,
                };
                DrawTextW(dc, &mut text, &mut rect, DT_LEFT | DT_SINGLELINE);
            }
            SelectObject(dc, old_font);
            let _ = DeleteObject(font.into());
        }
        UpdateLayeredWindow(
            hwnd,
            None,
            Some(&POINT {
                x: view.x as i32,
                y: view.y as i32,
            }),
            Some(&SIZE {
                cx: width,
                cy: height,
            }),
            Some(dc),
            Some(&POINT::default()),
            COLORREF(0),
            None,
            ULW_OPAQUE,
        )
        .map_err(err)?;
        self.visible = view;
        Ok(())
    }
    fn action(&mut self, action: Action) {
        let anchor = (
            self.visible.x + self.visible.w / 2.,
            self.visible.y + self.visible.h / 2.,
        );
        match action {
            Action::Smaller => self.geometry.zoom(1. / 1.25, anchor),
            Action::Bigger => self.geometry.zoom(1.25, anchor),
            Action::Original => self.geometry.zoom(1. / self.geometry.target, anchor),
            Action::Fit => self.geometry.fit(),
            Action::Fill => self.geometry.fill(),
            Action::Previous => self.navigate(-1),
            Action::Next => self.navigate(1),
            Action::Close => {}
        }
    }
}
pub(super) fn show(app: tauri::AppHandle, mut picture: Picture) -> Result<()> {
    unsafe {
        let hwnd = WINDOW.with(|cell| -> Result<HWND> {
            if !cell.get().is_invalid() {
                return Ok(cell.get());
            }
            let instance = GetModuleHandleW(None).map_err(err)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: w!("TPNativeImagePreview"),
                hCursor: LoadCursorW(None, IDC_ARROW).map_err(err)?,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("TPNativeImagePreview"),
                w!("TP 大图预览"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(err)?;
            cell.set(hwnd);
            Ok(hwnd)
        })?;
        let main = app
            .get_webview_window("main")
            .and_then(|w| w.hwnd().ok())
            .unwrap_or(hwnd);
        // An opening follows the panel; page changes stay on the preview's screen.
        let location = SURFACE.with(|s| if s.borrow().is_some() { hwnd } else { main });
        let monitor = MonitorFromWindow(location, MONITOR_DEFAULTTONEAREST);
        let dpi = (GetDpiForWindow(location) as f64 / 96.).max(1.);
        let geometry = Geometry::new(
            picture.image.width(),
            picture.image.height(),
            dpi,
            work_area(monitor)?,
        );
        // Match the existing opaque preview background for transparent image pixels.
        for pixel in picture.image.chunks_exact_mut(4) {
            let a = pixel[3] as u32;
            for c in &mut pixel[..3] {
                *c = (((*c as u32) * a + 24 * (255 - a) + 127) / 255) as u8;
            }
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let source = Bitmap::new(
            picture.image.width() as i32,
            picture.image.height() as i32,
            Some(&picture.image),
        )?;
        // The DIB owns the original pixels; do not retain a second full decoded copy.
        picture.image = image::RgbaImage::new(0, 0);
        let mut surface = Surface {
            pending_index: picture.index,
            app,
            picture,
            geometry,
            source,
            target: None,
            visible: Rect::default(),
            hovered: false,
            drag: None,
        };
        surface.draw(hwnd)?;
        SURFACE.with(|s| *s.borrow_mut() = Some(surface));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        SetTimer(Some(hwnd), 1, 16, None);
        Ok(())
    }
}
pub(super) fn hide() {
    unsafe {
        WINDOW.with(|h| {
            let hwnd = h.get();
            if !hwnd.is_invalid() {
                let _ = KillTimer(Some(hwnd), 1);
                let _ = ReleaseCapture();
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        });
        SURFACE.with(|s| {
            s.borrow_mut().take();
        });
    }
}
fn close() {
    let app = SURFACE.with(|s| s.borrow().as_ref().map(|s| s.app.clone()));
    hide();
    if let Some(app) = app {
        closed(&app);
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == WM_DPICHANGED {
        // UpdateLayeredWindow may synchronously send this while Surface is borrowed.
        // Read the current HWND DPI after that frame commits, rather than applying a
        // stale suggested rectangle to the full image's separately clipped geometry.
        let _ = PostMessageW(Some(hwnd), REFRESH_DPI, WPARAM(0), LPARAM(0));
        return LRESULT(0);
    }
    if message == WM_CLOSE || (message == WM_KEYDOWN && (wp.0 == 27 || wp.0 == 32)) {
        close();
        return LRESULT(0);
    }
    let mut close_requested = false;
    let handled = SURFACE.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return false;
        };
        let Some(s) = slot.as_mut() else {
            return false;
        };
        let mut redraw = false;
        match message {
            WM_TIMER | REFRESH_DPI => {
                let mut point = POINT::default();
                let _ = GetCursorPos(&mut point);
                let hovered = WindowFromPoint(point) == hwnd;
                if hovered != s.hovered {
                    s.hovered = hovered;
                    redraw = true;
                }
                if let Ok(work) = work_area(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)) {
                    if s.geometry.work != work {
                        s.geometry.work = work;
                        redraw = true;
                    }
                }
                let anchor = s.drag.map(|drag| drag.last).unwrap_or_else(|| {
                    if hovered {
                        (point.x as f64, point.y as f64)
                    } else {
                        (
                            s.visible.x + s.visible.w / 2.,
                            s.visible.y + s.visible.h / 2.,
                        )
                    }
                });
                let dpi = GetDpiForWindow(hwnd);
                if dpi != 0 {
                    redraw |= s.geometry.update_dpi(dpi as f64 / 96., anchor);
                }
                redraw |= s.geometry.tick();
            }
            WM_MOUSEWHEEL => {
                let mut delta = ((wp.0 >> 16) as u16 as i16) as f64;
                let mut source = INPUT_MESSAGE_SOURCE::default();
                if wp.0 & 8 == 0
                    && GetCurrentInputMessageSource(&mut source).is_ok()
                    && source.deviceType == IMDT_TOUCHPAD
                {
                    let mut reversed = 0u32;
                    let mut bytes = 4;
                    let _ = RegGetValueW(
                        HKEY_CURRENT_USER,
                        w!("Software\\Microsoft\\Windows\\CurrentVersion\\PrecisionTouchPad"),
                        w!("ScrollDirection"),
                        RRF_RT_REG_DWORD,
                        None,
                        Some((&mut reversed as *mut u32).cast()),
                        Some(&mut bytes),
                    );
                    if reversed == 0 {
                        delta = -delta;
                    }
                }
                let anchor = (
                    (lp.0 as u16 as i16) as f64,
                    ((lp.0 >> 16) as u16 as i16) as f64,
                );
                s.geometry.zoom((delta * 0.002).exp(), anchor);
            }
            WM_LBUTTONDOWN => {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                let x = p.x as f64 - s.visible.x;
                let y = p.y as f64 - s.visible.y;
                let hit = controls(
                    s.visible.w,
                    s.visible.h,
                    s.geometry.dpi,
                    s.geometry.scale,
                    s.picture.index,
                    s.picture.count,
                )
                .into_iter()
                .find(|c| s.hovered && c.rect.contains(x, y));
                if let Some(control) = hit {
                    if let Some(action) = control.action {
                        if matches!(action, Action::Close) {
                            close_requested = true;
                        } else {
                            s.action(action);
                            redraw = true;
                        }
                    }
                } else {
                    s.geometry.target = s.geometry.scale;
                    s.drag = Some(PointerDrag {
                        last: (p.x as f64, p.y as f64),
                    });
                    SetCapture(hwnd);
                }
            }
            WM_MOUSEMOVE => {
                if let Some(drag) = s.drag.as_mut() {
                    let mut p = POINT::default();
                    let _ = GetCursorPos(&mut p);
                    drag.move_to(&mut s.geometry, (p.x as f64, p.y as f64));
                    if let Ok(work) = work_area(MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST)) {
                        s.geometry.work = work;
                    }
                    redraw = true;
                }
            }
            WM_LBUTTONUP | WM_CAPTURECHANGED => {
                s.drag = None;
                if message == WM_LBUTTONUP {
                    let _ = ReleaseCapture();
                }
            }
            WM_KEYDOWN => {
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let action = match wp.0 {
                    0x25 => Some(Action::Previous),
                    0x27 => Some(Action::Next),
                    0xBB | 0x6B if ctrl => Some(Action::Bigger),
                    0xBD | 0x6D if ctrl => Some(Action::Smaller),
                    0x30 | 0x60 if ctrl => Some(Action::Fit),
                    _ => None,
                };
                if let Some(action) = action {
                    s.action(action);
                    redraw = true;
                }
            }
            WM_ERASEBKGND => return true,
            _ => return false,
        }
        if redraw {
            if let Err(error) = s.draw(hwnd) {
                eprintln!("Native preview: {error}");
            }
        }
        true
    });
    if close_requested {
        close();
    }
    if handled {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, message, wp, lp)
    }
}

#[cfg(test)]
#[path = "tests/dpi.rs"]
mod dpi_tests;
