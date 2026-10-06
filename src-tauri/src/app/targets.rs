use super::{
    common::{err, Result},
    storage::Store,
};
use crate::platform;
#[tauri::command]
pub(super) fn remember_paste_target(store: tauri::State<Store>) -> Result<()> {
    if let Some(target) = platform::capture_paste_target() {
        *store.paste_target.lock().map_err(err)? = Some(target);
    }
    Ok(())
}
