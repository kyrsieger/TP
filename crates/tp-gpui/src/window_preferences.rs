//! Settings whose native effects must run on the window's UI thread.
use gpui::BorrowAppContext;
use std::sync::atomic::Ordering;

pub fn save_patch(
    store: &tp_desktop::Store,
    confirmed: Option<usize>,
    edit: impl FnOnce(&mut tp_core::settings::Settings),
    window: &gpui::Window,
    cx: &mut gpui::App,
) -> Result<(tp_core::preferences::SaveOutcome, tp_core::Snapshot), String> {
    use tp_core::preferences::SaveOutcome;
    cx.update_global::<crate::host::Host, _>(|host, _| {
        let outcome =
            tp_core::preferences::save_patch(store, confirmed, edit, |previous, next| {
                if previous.shortcut != next.shortcut {
                    host.replace_shortcut(&next.shortcut)?;
                }
                if previous.always_on_top != next.always_on_top {
                    crate::native_window::set_always_on_top(window, next.always_on_top)?;
                }
                if previous.show_taskbar_icon != next.show_taskbar_icon {
                    crate::native_window::set_taskbar_icon(window, next.show_taskbar_icon)?;
                }
                Ok(())
            })?;
        if matches!(outcome, SaveOutcome::Saved { .. }) {
            host.background.revision.fetch_add(1, Ordering::Release);
        }
        Ok((outcome, store.snapshot()?))
    })
}
