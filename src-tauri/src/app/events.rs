use super::session::PanelLifecycle;
use tauri::{Emitter, Manager};
pub(super) fn changed(app: &tauri::AppHandle) {
    let active = app.try_state::<PanelLifecycle>().is_some_and(|lifecycle| {
        lifecycle
            .state
            .lock()
            .is_ok_and(|state| state.accepts_history_events())
    });
    if active {
        let _ = app.emit_to("main", "history-changed", ());
    }
}
