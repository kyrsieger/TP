use tauri::Emitter;
pub(super) fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("history-changed", ());
}
