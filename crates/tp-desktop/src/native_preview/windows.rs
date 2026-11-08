use super::*;
use ::windows::{
    core::{w, PCWSTR},
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

// Read-only diagnostics of our own window; no desktop capture or input injection.
unsafe fn trace_visibility(hwnd: HWND) {
    if cfg!(debug_assertions) && std::env::var_os("TP_DIAGNOSTIC_PREVIEW").is_some() {
        let mut rect = RECT::default();
        let bounds = GetWindowRect(hwnd, &mut rect);
        eprintln!(
            "large-preview-window: visible={} foreground={} bounds=({}, {}, {}, {}) bounds_ok={}",
            IsWindowVisible(hwnd).as_bool(),
            GetForegroundWindow() == hwnd,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            bounds.is_ok(),
        );
    }
}

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
    bits: *mut [u8; 4],
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
        let bits: *mut [u8; 4] = bits.cast();
        if let Some(p) = pixels {
            std::ptr::copy_nonoverlapping(p.as_ptr(), bits.cast(), p.len());
        } else {
            // GDI may leave the unused fourth byte untouched.
            bits.write_bytes(0, width as usize * height as usize);
        }
        let old = SelectObject(dc, bitmap.into());
        Ok(Self {
            dc,
            bitmap,
            old,
            bits,
            width,
            height,
        })
    }

    // The caller flushes pending GDI draws before blending into this DIB.
    unsafe fn fill_rounded_rect(&mut self, rect: RECT, view: SIZE, radius: f64, color: [u8; 4]) {
        let left = rect.left.max(0);
        let right = rect.right.min(view.cx).min(self.width);
        let top = rect.top.max(0);
        let bottom = rect.bottom.min(view.cy).min(self.height);
        if left >= right || top >= bottom {
            return;
        }
        let radius = radius
            .min((rect.right - rect.left) as f64 / 2.)
            .min((rect.bottom - rect.top) as f64 / 2.);
        let inner_left = rect.left as f64 + radius;
        let inner_right = rect.right as f64 - radius;
        let inner_top = rect.top as f64 + radius;
        let inner_bottom = rect.bottom as f64 - radius;

        for y in top..bottom {
            let start = y as usize * self.width as usize + left as usize;
            let row = std::slice::from_raw_parts_mut(self.bits.add(start), (right - left) as usize);
            let center_y = y as f64 + 0.5;
            let dy = (inner_top - center_y).max(center_y - inner_bottom);
            if dy <= 0. {
                row.fill(color);
                continue;
            }
            for (x, pixel) in (left..right).zip(row) {
                let center_x = x as f64 + 0.5;
                let dx = (inner_left - center_x).max(center_x - inner_right).max(0.);
                // Blend across a one-pixel band along the curved edge.
                let coverage = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0., 1.);
                if coverage == 1. {
                    *pixel = color;
                } else if coverage > 0. {
                    for (channel, fill) in pixel[..3].iter_mut().zip(color) {
                        *channel = (*channel as f64 + (fill as f64 - *channel as f64) * coverage)
                            .round() as u8;
                    }
                }
            }
        }
    }

    unsafe fn draw_close_icon(&mut self, rect: RECT, view: SIZE, dpi: f64) {
        let size = (CLOSE_ICON_SIZE * dpi)
            .min((rect.right - rect.left) as f64)
            .min((rect.bottom - rect.top) as f64);
        let cx = (rect.left as f64 + rect.right as f64) / 2.;
        let cy = (rect.top as f64 + rect.bottom as f64) / 2.;
        let left = ((cx - size / 2.).floor() as i32).max(0);
        let right = ((cx + size / 2.).ceil() as i32)
            .min(view.cx)
            .min(self.width);
        let top = ((cy - size / 2.).floor() as i32).max(0);
        let bottom = ((cy + size / 2.).ceil() as i32)
            .min(view.cy)
            .min(self.height);
        for y in top..bottom {
            for x in left..right {
                let coverage = close_icon_coverage(
                    x as f64 + 0.5 - cx,
                    y as f64 + 0.5 - cy,
                    size / CLOSE_ICON_SIZE,
                );
                if coverage > 0. {
                    let pixel = &mut *self.bits.add(y as usize * self.width as usize + x as usize);
                    for channel in &mut pixel[..3] {
                        *channel =
                            (*channel as f64 + (255. - *channel as f64) * coverage).round() as u8;
                    }
                }
            }
        }
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

fn control_bounds(rect: Rect) -> RECT {
    RECT {
        left: rect.x as i32,
        top: rect.y as i32,
        right: (rect.x + rect.w) as i32,
        bottom: (rect.y + rect.h) as i32,
    }
}

unsafe fn preview_font(height: f64) -> HFONT {
    CreateFontW(
        -height.round() as i32,
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
        ANTIALIASED_QUALITY,
        DEFAULT_PITCH.0 as u32,
        w!("Segoe UI"),
    )
}

struct Surface {
    app: Callbacks,
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
        let size = SIZE {
            cx: width,
            cy: height,
        };
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
        let target = self.target.as_mut().unwrap();
        let dc = target.dc;
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
            let font = preview_font(13. * dpi);
            let old_font = SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00ffffff));
            if let Some(message) = self.picture.unavailable {
                let mut text: Vec<u16> = crate::i18n::tr(message).encode_utf16().collect();
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
            if self.hovered {
                let controls = controls(
                    view.w,
                    view.h,
                    dpi,
                    self.geometry.scale,
                    self.picture.index,
                    self.picture.count,
                );
                let _ = GdiFlush();
                for (rect, divider) in control_backgrounds(&controls, dpi) {
                    let (radius, color) = if divider {
                        (0., [0x5b, 0x4c, 0x42, 0xff])
                    } else {
                        (4. * dpi, [0x27, 0x18, 0x11, 0xff])
                    };
                    target.fill_rounded_rect(control_bounds(rect), size, radius, color);
                }
                target.draw_close_icon(control_bounds(controls[0].rect), size, dpi);

                let size_font = controls
                    .iter()
                    .any(|control| control.group == ControlGroup::Size)
                    .then(|| preview_font(12. * dpi));
                for control in controls.into_iter().skip(1) {
                    let (label_font, color) = if control.group == ControlGroup::Size {
                        (size_font.unwrap_or(font), COLORREF(0x00f2ebe7))
                    } else {
                        (font, COLORREF(0x00ffffff))
                    };
                    SelectObject(dc, label_font.into());
                    SetTextColor(dc, color);
                    let mut rect = control_bounds(control.rect);
                    let mut text: Vec<u16> = control.text.encode_utf16().collect();
                    DrawTextW(
                        dc,
                        &mut text,
                        &mut rect,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                    );
                }
                SelectObject(dc, font.into());
                SetTextColor(dc, COLORREF(0x00ffffff));
                if let Some(size_font) = size_font {
                    let _ = DeleteObject(size_font.into());
                }
            }
            if self.hovered && self.picture.video && height > 65 {
                let mut text: Vec<u16> = crate::i18n::tr("视频首帧").encode_utf16().collect();
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
            Some(&size),
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
pub(super) fn show(app: Callbacks, mut picture: Picture) -> Result<()> {
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
            let title: Vec<u16> = crate::i18n::tr("TP 大图预览")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("TPNativeImagePreview"),
                PCWSTR(title.as_ptr()),
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
        let main = if app.owner == 0 {
            hwnd
        } else {
            HWND(app.owner as *mut _)
        };
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
        refresh_language();
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        trace_visibility(hwnd);
        SetTimer(Some(hwnd), 1, 16, None);
        Ok(())
    }
}
pub(super) fn refresh_language() {
    WINDOW.with(|window| {
        let hwnd = window.get();
        if hwnd.is_invalid() {
            return;
        }
        let video = SURFACE.with(|s| {
            s.borrow()
                .as_ref()
                .is_some_and(|surface| surface.picture.video)
        });
        let title: Vec<u16> = crate::i18n::tr(if video {
            "TP 视频首帧预览"
        } else {
            "TP 大图预览"
        })
        .encode_utf16()
        .chain(Some(0))
        .collect();
        unsafe {
            let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
        }
        SURFACE.with(|surface| {
            if let Some(surface) = surface.borrow_mut().as_mut() {
                unsafe {
                    let _ = surface.draw(hwnd);
                }
            }
        });
    });
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
