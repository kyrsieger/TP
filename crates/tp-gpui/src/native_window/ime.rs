//! Keep IME-consumed Windows keys out of application shortcuts.
//!
//! GPUI 0.2.2 recovers VK_PROCESSKEY with ImmGetVirtualKey before dispatch.
//! At composition end, marked_text_range can already be empty, so Escape or
//! Enter can reach the panel despite having been handled by the IME.
pub fn install_ime_guard(window: &gpui::Window) -> Result<(), String> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows::Win32::{Foundation::HWND, UI::Shell::SetWindowSubclass};
        let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            let hwnd = HWND(handle.hwnd.get() as _);
            if !unsafe { SetWindowSubclass(hwnd, Some(ime_subclass), SUBCLASS_ID, 0) }.as_bool() {
                return Err("Failed to install IME key guard".into());
            }
        }
    }
    #[cfg(not(windows))]
    let _ = window;
    Ok(())
}

#[cfg(windows)]
const SUBCLASS_ID: usize = 0x5450494d;

#[cfg(windows)]
unsafe extern "system" fn ime_subclass(
    hwnd: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _: usize,
    _: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{
        Foundation::LRESULT,
        UI::{
            Input::KeyboardAndMouse::VK_PROCESSKEY,
            Shell::{DefSubclassProc, RemoveWindowSubclass},
            WindowsAndMessaging::{TranslateMessage, MSG, WM_KEYDOWN, WM_NCDESTROY, WM_SYSKEYDOWN},
        },
    };
    if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN) && wparam.0 == VK_PROCESSKEY.0 as usize {
        // Preserve the raw IME message for translation, just as GPUI does while
        // a marked range exists. Do not recover it as an application key.
        let _ = TranslateMessage(&MSG {
            hwnd,
            message,
            wParam: wparam,
            lParam: lparam,
            ..Default::default()
        });
        return LRESULT(0);
    }
    if message == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(ime_subclass), SUBCLASS_ID);
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}
