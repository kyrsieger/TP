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
pub fn capture_paste_target() -> Option<PasteTarget> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    if get_active_window().is_ok_and(|active| active.process_id == u64::from(std::process::id())) {
        return None;
    }
    let window = unsafe { GetForegroundWindow() }.0 as isize;
    (window != 0).then_some(PasteTarget { window })
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
    let clicked = enigo.key(Key::Unicode('v'), Direction::Click).is_ok();
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
