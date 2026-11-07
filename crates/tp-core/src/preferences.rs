use crate::{
    cleanup::{cleanup_ids, remove_record, settings_cleanup_confirmation},
    common::{err, now, Result},
    settings::Settings,
    storage::read_settings as settings,
    store::Store,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;

pub enum SaveOutcome {
    ConfirmationRequired { count: usize },
    Saved { previous: Settings, removed: usize },
}

/// Persists preferences only after the host accepts its system changes. The
/// callback must restore the requested state when called during rollback.
pub fn save<T>(
    store: &Store<T>,
    value: &Settings,
    confirmed_count: Option<usize>,
    apply: impl FnMut(&Settings, &Settings) -> Result<()>,
) -> Result<SaveOutcome> {
    save_patch(
        store,
        confirmed_count,
        |current| *current = value.clone(),
        apply,
    )
}

/// Merge a field edit while holding the same lock used by geometry and other
/// settings writers. Confirmation retries always edit the latest stored value.
pub fn save_patch<T>(
    store: &Store<T>,
    confirmed_count: Option<usize>,
    edit: impl FnOnce(&mut Settings),
    mut apply: impl FnMut(&Settings, &Settings) -> Result<()>,
) -> Result<SaveOutcome> {
    let _data = store.read_data()?;
    let conn = store.db.lock().map_err(err)?;
    let previous = settings(&conn)?;
    let mut proposed = previous.clone();
    edit(&mut proposed);
    proposed.validate().map_err(crate::i18n::tr)?;
    let value = &proposed;
    let timestamp = now();
    if let Some(count) =
        settings_cleanup_confirmation(&conn, value, timestamp, confirmed_count.unwrap_or(0))?
    {
        return Ok(SaveOutcome::ConfirmationRequired { count });
    }
    let ids = cleanup_ids(&conn, value, timestamp)?;
    let transaction = conn.unchecked_transaction().map_err(err)?;
    transaction
        .execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(value).map_err(err)?],
        )
        .map_err(err)?;
    if let Err(error) = apply(&previous, value) {
        let _ = apply(value, &previous);
        return Err(error);
    }
    if let Err(error) = transaction.commit() {
        let _ = apply(value, &previous);
        return Err(err(error));
    }
    for id in &ids {
        remove_record(&conn, &store.dir, id)?;
    }
    Ok(SaveOutcome::Saved {
        previous,
        removed: ids.len(),
    })
}

fn saved_autostart(conn: &Connection) -> Result<Option<bool>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='autostart'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?;
    json.map(|value| serde_json::from_str(&value).map_err(err))
        .transpose()
}

fn write_autostart(conn: &Connection, enabled: bool) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key,value) VALUES('autostart',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [if enabled { "true" } else { "false" }],
    ).map_err(err)?;
    Ok(())
}

pub fn save_autostart(
    conn: &Connection,
    enabled: bool,
    current: bool,
    mut apply: impl FnMut(bool) -> Result<()>,
) -> Result<()> {
    let transaction = conn.unchecked_transaction().map_err(err)?;
    write_autostart(&transaction, enabled)?;
    if let Err(error) = apply(enabled) {
        let _ = apply(current);
        return Err(error);
    }
    if let Err(error) = transaction.commit() {
        let _ = apply(current);
        return Err(err(error));
    }
    Ok(())
}

pub fn restore_autostart(
    conn: &Connection,
    current: bool,
    mut apply: impl FnMut(bool) -> Result<()>,
) -> Result<()> {
    match saved_autostart(conn)? {
        // 即使已开启也重新写入，以更新重装后可能改变的可执行文件路径。
        Some(true) => apply(true),
        Some(false) if current => apply(false),
        Some(false) => Ok(()),
        // 旧版本没有保存此项；首次迁移接收当前系统状态。
        None => write_autostart(conn, current),
    }
}

/// Coordinates are saved independently of user preferences, so a delayed window
/// resize cannot restore a stale copy of the entire settings object.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelGeometry {
    pub panel_x: Option<f64>,
    pub panel_y: Option<f64>,
    pub panel_width: f64,
    pub panel_height: f64,
}

pub fn persist_panel_geometry(conn: &Connection, value: PanelGeometry) -> Result<()> {
    if !value.panel_width.is_finite()
        || !value.panel_height.is_finite()
        || value.panel_x.is_some_and(|v| !v.is_finite())
        || value.panel_y.is_some_and(|v| !v.is_finite())
    {
        return Err(crate::i18n::tr("面板位置或尺寸无效").into());
    }
    let mut current = settings(conn)?;
    current.panel_x = value.panel_x;
    current.panel_y = value.panel_y;
    current.panel_width = value.panel_width;
    current.panel_height = value.panel_height;
    current.validate().map_err(crate::i18n::tr)?;
    conn.execute(
        "UPDATE settings SET value=?1 WHERE key='main'",
        [serde_json::to_string(&current).map_err(err)?],
    )
    .map_err(err)?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/preferences.rs"]
mod tests;
