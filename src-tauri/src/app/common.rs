use std::time::{SystemTime, UNIX_EPOCH};
pub(super) type Result<T> = std::result::Result<T, String>;
pub(super) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(super) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
