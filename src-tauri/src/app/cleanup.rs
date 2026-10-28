use super::{
    common::{err, now, Result},
    events::changed,
    filesystem::file_size,
    model::{FileItem, Settings},
    storage::{settings, Store},
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Component, Path},
};
pub(super) fn remove_record(conn: &Connection, dir: &Path, id: &str) -> Result<()> {
    let status: Option<String> = conn
        .query_row("SELECT status FROM records WHERE id=?1", [id], |r| r.get(0))
        .optional()
        .map_err(err)?;
    conn.execute("DELETE FROM records WHERE id=?1", [id])
        .map_err(err)?;
    let path = dir.join("assets").join(id);
    if status.as_deref() != Some("pending") && path.exists() {
        fs::remove_dir_all(path).map_err(err)?;
    }
    Ok(())
}

pub(super) fn clean(store: &Store) -> Result<bool> {
    clean_at(store, now())
}

pub(super) fn clean_at(store: &Store, timestamp: i64) -> Result<bool> {
    let conn = store.db.lock().map_err(err)?;
    let config = settings(&conn)?;
    let ids = cleanup_ids(&conn, &config, timestamp)?;
    for id in &ids {
        remove_record(&conn, &store.dir, id)?;
    }
    Ok(!ids.is_empty())
}

pub(super) fn cleanup_ids(
    conn: &Connection,
    config: &Settings,
    timestamp: i64,
) -> Result<Vec<String>> {
    let cutoff = timestamp - i64::from(config.retention_days) * 86_400_000;
    let mut ids = Vec::new();
    let mut stmt = conn.prepare("SELECT id FROM records WHERE favorite=0 AND group_id IS NULL AND (used_at < ?1 OR (expires_at IS NOT NULL AND expires_at <= ?2))").map_err(err)?;
    ids.extend(
        stmt.query_map(params![cutoff, timestamp], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?,
    );
    let mut stmt = conn.prepare("SELECT id FROM records WHERE favorite=0 AND group_id IS NULL AND used_at >= ?2 AND (expires_at IS NULL OR expires_at > ?3) ORDER BY used_at DESC LIMIT -1 OFFSET ?1").map_err(err)?;
    ids.extend(
        stmt.query_map(params![config.max_ordinary, cutoff, timestamp], |r| {
            r.get::<_, String>(0)
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?,
    );
    ids.sort();
    ids.dedup();
    Ok(ids)
}

pub(super) fn settings_cleanup_confirmation(
    conn: &Connection,
    value: &Settings,
    timestamp: i64,
    confirmed_count: usize,
) -> Result<Option<usize>> {
    let previous = settings(conn)?;
    if previous.max_ordinary == value.max_ordinary
        && previous.retention_days == value.retention_days
    {
        return Ok(None);
    }
    let count = cleanup_ids(conn, value, timestamp)?.len();
    Ok((count > confirmed_count).then_some(count))
}

fn protected_assets(conn: &Connection, assets: &Path) -> Result<HashSet<OsString>> {
    let mut protected = HashSet::<OsString>::new();
    let mut stmt = conn
        .prepare("SELECT id,files FROM records WHERE favorite != 0 OR group_id IS NOT NULL")
        .map_err(err)?;
    let records = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    for (id, files) in records {
        protected.insert(OsString::from(id));
        for file in serde_json::from_str::<Vec<FileItem>>(&files).map_err(err)? {
            for path in [Some(file.original.as_str()), file.stored.as_deref()]
                .into_iter()
                .flatten()
            {
                if let Ok(relative) = Path::new(path).strip_prefix(&assets) {
                    if let Some(Component::Normal(name)) = relative.components().next() {
                        protected.insert(name.to_os_string());
                    }
                }
            }
        }
    }
    Ok(protected)
}

fn cache_size_data(store: &Store) -> Result<u64> {
    let conn = store.db.lock().map_err(err)?;
    let assets = store.dir.join("assets");
    let protected = protected_assets(&conn, &assets)?;
    let mut size = 0u64;
    for entry in fs::read_dir(assets).map_err(err)? {
        let entry = entry.map_err(err)?;
        if !protected.contains(&entry.file_name()) {
            size = size.saturating_add(file_size(&entry.path()).map_err(err)?);
        }
    }
    Ok(size)
}

fn clear_cache_data(store: &Store, all_records: bool) -> Result<usize> {
    let conn = store.db.lock().map_err(err)?;
    let assets = store.dir.join("assets");
    let protected = if all_records {
        HashSet::new()
    } else {
        protected_assets(&conn, &assets)?
    };
    let mut stmt = conn
        .prepare(if all_records {
            "SELECT id FROM records"
        } else {
            "SELECT id FROM records WHERE favorite=0 AND group_id IS NULL"
        })
        .map_err(err)?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    for id in &ids {
        if protected.contains(std::ffi::OsStr::new(id)) {
            conn.execute("DELETE FROM records WHERE id=?1", [id])
                .map_err(err)?;
        } else {
            remove_record(&conn, &store.dir, id)?;
        }
    }
    for entry in fs::read_dir(assets).map_err(err)? {
        let entry = entry.map_err(err)?;
        if protected.contains(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        if entry.file_type().map_err(err)?.is_dir() {
            fs::remove_dir_all(path).map_err(err)?;
        } else {
            fs::remove_file(path).map_err(err)?;
        }
    }
    Ok(ids.len())
}

#[tauri::command]
pub(super) fn clear_cache(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    all_records: bool,
) -> Result<usize> {
    let _data = store.write_data()?;
    let result = clear_cache_data(&store, all_records);
    changed(&app);
    result
}

#[tauri::command]
pub(super) fn get_cache_size(store: tauri::State<Store>) -> Result<u64> {
    let _data = store.read_data()?;
    cache_size_data(&store)
}

#[cfg(test)]
#[path = "tests/cleanup.rs"]
mod tests;
