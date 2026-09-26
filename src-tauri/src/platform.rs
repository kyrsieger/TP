use std::{path::Path, process::Command, thread, time::Duration};

use active_win_pos_rs::get_active_window;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

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
                SWP_NOZORDER, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCDESTROY,
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
pub fn install_panel_resize_double_click_macos(
    ns_window: *mut std::ffi::c_void,
) -> std::io::Result<()> {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSWindow};

    let native = NonNull::new(ns_window.cast::<NSWindow>())
        .ok_or_else(|| std::io::Error::other("面板窗口不存在"))?;
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
    if monitor.is_some() {
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
    Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(source)
        .args(["-frames:v", "1", "-vf", "scale=360:-1"])
        .arg(output)
        .status()
        .is_ok_and(|status| status.success() && output.exists())
}
