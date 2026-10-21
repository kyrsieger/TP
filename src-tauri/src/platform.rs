use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use active_win_pos_rs::get_active_window;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

#[cfg(windows)]
pub fn reveal_files(groups: &BTreeMap<PathBuf, Vec<PathBuf>>) -> std::io::Result<()> {
    use std::{io, os::windows::ffi::OsStrExt};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::Com::{CoInitializeEx, CoUninitialize, IBindCtx, COINIT_APARTMENTTHREADED},
            UI::Shell::{
                Common::ITEMIDLIST, ILFindLastID, ILFree, SHOpenFolderAndSelectItems,
                SHParseDisplayName,
            },
        },
    };

    struct Pidl(*mut ITEMIDLIST);
    impl Drop for Pidl {
        fn drop(&mut self) {
            unsafe { ILFree(Some(self.0)) };
        }
    }
    fn parse(path: &Path) -> io::Result<Pidl> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut pidl = std::ptr::null_mut();
        unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None) }
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Pidl(pidl))
    }

    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if com.is_err() && com != RPC_E_CHANGED_MODE {
        return Err(io::Error::other(format!("COM 初始化失败：{com:?}")));
    }
    let result = (|| {
        for (directory, files) in groups {
            let folder = parse(directory)?;
            let items = files
                .iter()
                .map(|path| parse(path))
                .collect::<io::Result<Vec<_>>>()?;
            let children: Vec<*const ITEMIDLIST> = items
                .iter()
                .map(|item| unsafe { ILFindLastID(item.0) as *const ITEMIDLIST })
                .collect();
            unsafe { SHOpenFolderAndSelectItems(folder.0, Some(&children), 0) }
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        Ok(())
    })();
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

#[cfg(target_os = "macos")]
pub fn reveal_files(groups: &BTreeMap<PathBuf, Vec<PathBuf>>) -> std::io::Result<()> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSArray, NSString, NSURL};

    let urls: Vec<_> = groups
        .values()
        .flatten()
        .map(|path| NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy())))
        .collect();
    NSWorkspace::sharedWorkspace()
        .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&urls));
    Ok(())
}

/// 写回图片记录：一次提供图片数据、文件路径和纯文本路径。
///
/// - 图片数据（`CF_DIB` / `PNG`）供聊天框按图片粘贴；
/// - 文件（`CF_HDROP`）供 conhost、Windows Terminal、VS Code 等按文件粘贴路径；
/// - 纯文本（`CF_UNICODETEXT`）供 Zed 等只读文本的编辑器/终端粘贴路径。
///
/// 三种表示同时写入，行为与从资源管理器复制图片文件一致。
#[cfg(windows)]
pub fn set_clipboard_image_with_file(path: &Path) -> std::io::Result<()> {
    use clipboard_rs::{common::RustImage, RustImageData};
    use clipboard_win::{formats, options, raw, register_format, Clipboard as WindowsClipboard};

    let io_err = |error: String| std::io::Error::other(error);
    let path = path.to_string_lossy().into_owned();
    let text = quote_path(&path);
    let image = RustImageData::from_path(&path).map_err(|e| io_err(e.to_string()))?;
    let png = image.to_png().map_err(|e| io_err(e.to_string()))?;
    let bitmap = image.to_bitmap().map_err(|e| io_err(e.to_string()))?;
    // CF_DIB 是 BMP 文件去掉 14 字节 BITMAPFILEHEADER 后的内容。
    let dib = bitmap
        .get_bytes()
        .get(14..)
        .ok_or_else(|| io_err("图片位图数据不完整".into()))?;
    let png_format = register_format("PNG")
        .ok_or_else(|| io_err("注册 PNG 剪贴板格式失败".into()))?
        .get();
    // clipboard-rs 的多格式写入会在图片分支提前关闭剪贴板，导致文件写入失败；
    // Windows 的 OpenClipboard 不是引用计数，因此这里在同一会话内自己按顺序写入。
    let clipboard = WindowsClipboard::new_attempts(20).map_err(|e| io_err(e.to_string()))?;
    raw::empty().map_err(|e| io_err(e.to_string()))?;
    raw::set_string_with(&text, options::NoClear).map_err(|e| io_err(e.to_string()))?;
    raw::set_without_clear(png_format, png.get_bytes()).map_err(|e| io_err(e.to_string()))?;
    raw::set_without_clear(formats::CF_DIB, dib).map_err(|e| io_err(e.to_string()))?;
    raw::set_file_list_with(&[path], options::NoClear).map_err(|e| io_err(e.to_string()))?;
    drop(clipboard);
    Ok(())
}

/// 路径含空格时加双引号，和终端按文件粘贴时的行为保持一致。
fn quote_path(path: &str) -> String {
    if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path.to_string()
    }
}

#[cfg(target_os = "macos")]
pub fn set_clipboard_image_with_file(path: &Path) -> std::io::Result<()> {
    use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, RustImageData};

    let io_err = |error: String| std::io::Error::other(error);
    let path = path.to_string_lossy().into_owned();
    let text = quote_path(&path);
    let image = RustImageData::from_path(&path).map_err(|e| io_err(e.to_string()))?;
    let ctx = ClipboardContext::new().map_err(|e| io_err(e.to_string()))?;
    ctx.set(vec![
        ClipboardContent::Text(text),
        ClipboardContent::Image(image),
        ClipboardContent::Files(vec![path]),
    ])
    .map_err(|e| io_err(e.to_string()))
}

#[derive(Clone)]
pub struct PasteTarget {
    #[cfg(windows)]
    window: isize,
    #[cfg(target_os = "macos")]
    process_id: u64,
}

pub fn source_is_1password() -> bool {
    get_active_window().is_ok_and(|active| {
        active.app_name.to_ascii_lowercase().contains("1password")
            || active
                .process_path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("1password")
    })
}

#[cfg(windows)]
std::thread_local! {
    static LAST_BORDER_PRESS: std::cell::RefCell<Option<(u32, u32, i32, i32)>> = const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
pub fn install_panel_resize_double_click(
    hwnd: windows::Win32::Foundation::HWND,
) -> std::io::Result<()> {
    use windows::Win32::UI::Shell::SetWindowSubclass;

    if unsafe { SetWindowSubclass(hwnd, Some(panel_resize_subclass), 1, 0) }.as_bool() {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
unsafe extern "system" fn panel_resize_subclass(
    hwnd: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _: usize,
    _: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{
        Foundation::{LRESULT, RECT},
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::GetDoubleClickTime,
            Shell::{DefSubclassProc, RemoveWindowSubclass},
            WindowsAndMessaging::{
                GetClientRect, GetMessageTime, GetSystemMetrics, GetWindowRect, SetWindowPos,
                HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
                HTTOPRIGHT, SM_CXDOUBLECLK, SM_CYDOUBLECLK, SWP_NOACTIVATE, SWP_NOMOVE,
                SWP_NOZORDER, WM_NCDESTROY, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN,
            },
        },
    };

    if message == WM_NCLBUTTONDOWN || message == WM_NCLBUTTONDBLCLK {
        let hit = wparam.0 as u32;
        let width = matches!(
            hit,
            HTLEFT | HTRIGHT | HTTOPLEFT | HTTOPRIGHT | HTBOTTOMLEFT | HTBOTTOMRIGHT
        );
        let height = matches!(
            hit,
            HTTOP | HTBOTTOM | HTTOPLEFT | HTTOPRIGHT | HTBOTTOMLEFT | HTBOTTOMRIGHT
        );
        if width || height {
            let time = GetMessageTime() as u32;
            let x = (lparam.0 as u32 as u16 as i16) as i32;
            let y = ((lparam.0 as u32 >> 16) as u16 as i16) as i32;
            let double_click = LAST_BORDER_PRESS.with(|last| {
                let mut last = last.borrow_mut();
                let within_range = match *last {
                    Some((previous_hit, previous_time, previous_x, previous_y)) => {
                        hit == previous_hit
                            && time.wrapping_sub(previous_time) <= GetDoubleClickTime()
                            && x.abs_diff(previous_x)
                                <= (GetSystemMetrics(SM_CXDOUBLECLK) / 2) as u32
                            && y.abs_diff(previous_y)
                                <= (GetSystemMetrics(SM_CYDOUBLECLK) / 2) as u32
                    }
                    None => false,
                };
                *last = if within_range || message == WM_NCLBUTTONDBLCLK {
                    None
                } else {
                    Some((hit, time, x, y))
                };
                within_range || message == WM_NCLBUTTONDBLCLK
            });
            if double_click {
                let (mut outer, mut client) = (RECT::default(), RECT::default());
                if GetWindowRect(hwnd, &mut outer).is_ok()
                    && GetClientRect(hwnd, &mut client).is_ok()
                {
                    let scale = GetDpiForWindow(hwnd) as f64 / 96.0;
                    let target_width = (420.0 * scale).round() as i32;
                    let target_height = (600.0 * scale).round() as i32;
                    let outer_width = outer.right - outer.left
                        + if width {
                            target_width - (client.right - client.left)
                        } else {
                            0
                        };
                    let outer_height = outer.bottom - outer.top
                        + if height {
                            target_height - (client.bottom - client.top)
                        } else {
                            0
                        };
                    if SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        outer_width,
                        outer_height,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                    .is_ok()
                    {
                        return LRESULT(0);
                    }
                }
            }
        }
    }
    if message == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(panel_resize_subclass), 1);
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

#[cfg(target_os = "macos")]
std::thread_local! {
    static PANEL_RESIZE_MONITOR: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "macos")]
pub fn remove_panel_resize_double_click_macos() {
    let remove = PANEL_RESIZE_MONITOR.with(|monitor| monitor.borrow_mut().take());
    if let Some(remove) = remove {
        remove();
    }
}

#[cfg(target_os = "macos")]
pub fn install_panel_resize_double_click_macos(
    ns_window: *mut std::ffi::c_void,
) -> std::io::Result<()> {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSWindow};

    let native = NonNull::new(ns_window.cast::<NSWindow>())
        .ok_or_else(|| std::io::Error::other("面板窗口不存在"))?;
    remove_panel_resize_double_click_macos();
    let window_number = unsafe { native.as_ref().windowNumber() };
    let window_pointer = native.as_ptr() as usize;
    let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let mouse = unsafe { event.as_ref() };
        if mouse.windowNumber() == window_number && mouse.clickCount() == 2 {
            let window = unsafe { &*(window_pointer as *const NSWindow) };
            let frame = window.frame();
            let point = window.convertPointToScreen(mouse.locationInWindow());
            let width = (point.x - frame.origin.x).abs() <= 8.0
                || (point.x - frame.origin.x - frame.size.width).abs() <= 8.0;
            let height = (point.y - frame.origin.y).abs() <= 8.0
                || (point.y - frame.origin.y - frame.size.height).abs() <= 8.0;
            if width || height {
                let mut size = window.contentRectForFrameRect(frame).size;
                if width {
                    size.width = 420.0;
                }
                if height {
                    size.height = 600.0;
                }
                window.setContentSize(size);
                return std::ptr::null_mut();
            }
        }
        event.as_ptr()
    });
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::LeftMouseDown, &handler)
    };
    if let Some(monitor) = monitor {
        PANEL_RESIZE_MONITOR.with(|current| {
            *current.borrow_mut() = Some(Box::new(move || unsafe {
                NSEvent::removeMonitor(&monitor)
            }));
        });
        Ok(())
    } else {
        Err(std::io::Error::other("无法监听面板边框双击"))
    }
}

#[cfg(windows)]
pub fn capture_paste_target() -> Option<PasteTarget> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
    };
    let window = unsafe { GetForegroundWindow() };
    if window.0.is_null() {
        return None;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
    if process_id == std::process::id() {
        return None;
    }
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(window, &mut class) } as usize;
    if matches!(
        String::from_utf16_lossy(&class[..len]).as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "NotifyIconOverflowWindow"
    ) {
        return None;
    }
    Some(PasteTarget {
        window: window.0 as isize,
    })
}

#[cfg(target_os = "macos")]
pub fn capture_paste_target() -> Option<PasteTarget> {
    get_active_window().ok().and_then(|active| {
        (active.process_id != u64::from(std::process::id())).then_some(PasteTarget {
            process_id: active.process_id,
        })
    })
}

pub fn paste_to_target(target: PasteTarget) -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow},
        };
        if !unsafe { SetForegroundWindow(HWND(target.window as _)) }.as_bool() {
            return false;
        }
        thread::sleep(Duration::from_millis(120));
        if unsafe { GetForegroundWindow() }.0 as isize != target.window {
            return false;
        }
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"System Events\" to set frontmost of (first process whose unix id is {}) to true",
            target.process_id
        );
        if !Command::new("osascript")
            .args(["-e", &script])
            .status()
            .is_ok_and(|s| s.success())
        {
            return false;
        }
        thread::sleep(Duration::from_millis(120));
        if !get_active_window().is_ok_and(|active| active.process_id == target.process_id) {
            return false;
        }
    }
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return false;
    };
    #[cfg(windows)]
    let modifier = Key::Control;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    if enigo.key(modifier, Direction::Press).is_err() {
        return false;
    }
    let clicked = enigo.key(Key::V, Direction::Click).is_ok();
    let released = enigo.key(modifier, Direction::Release).is_ok();
    clicked && released
}

// ffmpeg is optional at runtime; the record and video file remain usable without a preview.
pub fn video_preview(source: &Path, output: &Path) -> bool {
    if let Some(parent) = output.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let ready = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(source)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale='min(1280,iw)':'min(1280,ih)':force_original_aspect_ratio=decrease",
        ])
        .arg(output)
        .status()
        .is_ok_and(|status| status.success() && output.exists());
    if !ready {
        let _ = std::fs::remove_file(output);
    }
    ready
}

/// Sequence captured before reading formats; prevents rewriting a newer copy.
#[cfg(windows)]
pub fn clipboard_revision() -> Option<u64> {
    clipboard_win::raw::seq_num().map(|n| n.get() as u64)
}

#[cfg(target_os = "macos")]
pub fn clipboard_revision() -> Option<u64> {
    objc2::rc::autoreleasepool(|_| {
        Some(objc2_app_kit::NSPasteboard::generalPasteboard().changeCount() as u64)
    })
}

#[cfg(windows)]
pub fn normalize_clipboard_text(
    revision: Option<u64>,
    original: &str,
    text: &str,
) -> std::io::Result<bool> {
    use crate::clipboard_diagnostics::{event, text_metadata};
    use clipboard_win::{formats, options, raw, register_format, Clipboard};
    use serde_json::json;
    event(
        "normalize_begin",
        || json!({"revision": revision, "current_revision": clipboard_revision(), "original": text_metadata(original), "cleaned": text_metadata(text)}),
    );
    let io_err = |e: clipboard_win::ErrorCode| std::io::Error::other(e.to_string());
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE},
        },
    };
    struct ClipboardOwner(HWND);
    impl Drop for ClipboardOwner {
        fn drop(&mut self) {
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }
    // Use a real owner window for this transaction. NULL ownership cannot safely
    // take over another application's clipboard and is unsuitable for rewrites.
    let owner = ClipboardOwner(
        unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!("TP Clipboard"),
                Default::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .map_err(|e| std::io::Error::other(e.to_string()))?,
    );
    let _clipboard = Clipboard::new_attempts_for(owner.0 .0, 10).map_err(io_err)?;
    if revision.is_none() || clipboard_revision() != revision {
        event(
            "normalize_stale",
            || json!({"revision": revision, "current_revision": clipboard_revision()}),
        );
        return Ok(false);
    }
    let mut current = Vec::new();
    raw::get_string(&mut current)
        .map_err(|e| std::io::Error::other(format!("读取剪贴板纯文本失败: {e}")))?;
    if current != original.as_bytes() {
        event(
            "normalize_text_mismatch",
            || json!({"revision": revision, "current_bytes": current.len()}),
        );
        return Ok(false);
    }
    // Retain every representation before taking ownership. Windows requires
    // EmptyClipboard before writing when the clipboard belongs to another app.
    // ANSI/OEM variants are synthesized from the new Unicode text by Windows.
    let mut retained = Vec::new();
    for format in raw::EnumFormats::new() {
        event("normalize_format", || json!({"format": format}));
        if [
            formats::CF_TEXT,
            formats::CF_OEMTEXT,
            formats::CF_LOCALE,
            formats::CF_UNICODETEXT,
        ]
        .contains(&format)
        {
            continue;
        }
        let mut bytes = Vec::new();
        raw::get_vec(format, &mut bytes)
            .map_err(|e| std::io::Error::other(format!("读取剪贴板格式 {format} 失败: {e}")))?;
        event(
            "normalize_format_read",
            || json!({"format": format, "bytes": bytes.len(), "current_revision": clipboard_revision()}),
        );
        retained.push((format, bytes));
    }
    let html_format = register_format("HTML Format")
        .ok_or_else(|| std::io::Error::other("无法注册 HTML 剪贴板格式"))?
        .get();
    let rtf_format = register_format("Rich Text Format")
        .ok_or_else(|| std::io::Error::other("无法注册 RTF 剪贴板格式"))?
        .get();
    let html = retained
        .iter()
        .find(|(format, _)| *format == html_format)
        .map(|(_, bytes)| {
            crate::clipboard_text::trim_windows_html(bytes)
                .ok_or_else(|| std::io::Error::other("HTML 剪贴板偏移无效，保留原始复制内容"))
        })
        .transpose()?;
    let rtf = retained
        .iter()
        .find(|(format, _)| *format == rtf_format)
        .map(|(_, bytes)| crate::clipboard_text::trim_rtf(bytes));
    let mut changed = original != text;
    for (format, bytes) in &mut retained {
        if *format == html_format {
            let updated = html.as_ref().unwrap();
            changed |= bytes != updated;
            *bytes = updated.clone();
        }
        if *format == rtf_format {
            let updated = rtf.as_ref().unwrap();
            changed |= bytes != updated;
            *bytes = updated.clone();
        }
    }
    if !changed {
        event("normalize_unchanged", || json!({"revision": revision}));
        return Ok(false);
    }
    raw::empty().map_err(|e| std::io::Error::other(format!("接管剪贴板失败: {e}")))?;
    for (format, bytes) in retained {
        raw::set_without_clear(format, &bytes)
            .map_err(|e| std::io::Error::other(format!("写回剪贴板格式 {format} 失败: {e}")))?;
    }
    raw::set_string_with(text, options::NoClear)
        .map_err(|e| std::io::Error::other(format!("写回剪贴板纯文本失败: {e}")))?;

    event(
        "normalize_written",
        || json!({"revision": revision, "current_revision": clipboard_revision()}),
    );

    Ok(true)
}

#[cfg(target_os = "macos")]
pub fn normalize_clipboard_text(
    revision: Option<u64>,
    original: &str,
    text: &str,
) -> std::io::Result<bool> {
    use objc2_app_kit::{
        NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString,
    };
    use objc2_foundation::NSData;
    objc2::rc::autoreleasepool(|_| {
        let board = NSPasteboard::generalPasteboard();
        if revision.is_none() || Some(board.changeCount() as u64) != revision {
            return Ok(false);
        }
        if board
            .stringForType(unsafe { NSPasteboardTypeString })
            .map(|s| s.to_string())
            .as_deref()
            != Some(original)
        {
            return Ok(false);
        }
        let types = board
            .types()
            .ok_or_else(|| std::io::Error::other("无法读取剪贴板格式"))?;
        let mut data = Vec::new();
        let mut changed = original != text;
        for kind in types.iter() {
            let value = board
                .dataForType(&kind)
                .ok_or_else(|| std::io::Error::other("无法保留剪贴板格式"))?;
            let bytes = if &*kind == unsafe { NSPasteboardTypeHTML } {
                let source = String::from_utf8(value.to_vec()).map_err(std::io::Error::other)?;
                crate::clipboard_text::trim_html(&source).into_bytes()
            } else if &*kind == unsafe { NSPasteboardTypeRTF } {
                crate::clipboard_text::trim_rtf(&value.to_vec())
            } else if &*kind == unsafe { NSPasteboardTypeString } {
                text.as_bytes().to_vec()
            } else {
                value.to_vec()
            };
            changed |= bytes != value.to_vec();
            data.push((kind, NSData::with_bytes(&bytes)));
        }
        if Some(board.changeCount() as u64) != revision {
            return Ok(false);
        }
        if !changed {
            return Ok(false);
        }
        // Take ownership only after retaining all representations. macOS has no
        // cross-process compare-and-swap for NSPasteboard.
        unsafe { board.declareTypes_owner(&types, None) };
        for (kind, value) in data {
            if !board.setData_forType(Some(&value), &kind) {
                return Err(std::io::Error::other("写回剪贴板格式失败"));
            }
        }

        Ok(true)
    })
}
