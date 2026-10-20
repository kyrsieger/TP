use super::{
    cleanup::{clean, remove_record},
    common::{err, now, Result},
    events::changed,
    storage::Store,
};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::path::Path;
#[tauri::command]
pub(super) fn rename_record(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: String,
    title: String,
) -> Result<()> {
    let conn = store.db.lock().map_err(err)?;
    set_title(&conn, &id, &title)?;
    drop(conn);
    changed(&app);
    Ok(())
}

fn set_title(conn: &Connection, id: &str, title: &str) -> Result<()> {
    let title = title.trim();
    if title.chars().count() > 100 || title.contains(['\r', '\n']) {
        return Err("记录标题最多 100 个字符，且不能换行".into());
    }
    let updated = conn
        .execute(
            "UPDATE records SET title=?1 WHERE id=?2",
            params![title, id],
        )
        .map_err(err)?;
    if updated == 0 {
        return Err("记录不存在".into());
    }
    Ok(())
}

#[tauri::command]
pub(super) fn add_group(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    name: String,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name == "默认" || name == "收藏" || name.chars().count() > 40 {
        return Err("分组名称无效".into());
    }
    store
        .db
        .lock()
        .map_err(err)?
        .execute(
            "INSERT INTO groups(name,position) SELECT ?1,COALESCE(MAX(position),-1)+1 FROM groups",
            [name],
        )
        .map_err(err)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(super) fn reorder_groups(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    ids: Vec<i64>,
) -> Result<()> {
    let mut conn = store.db.lock().map_err(err)?;
    set_group_order(&mut conn, &ids)?;
    drop(conn);
    changed(&app);
    Ok(())
}

fn set_group_order(conn: &mut Connection, ids: &[i64]) -> Result<()> {
    let tx = conn.transaction().map_err(err)?;
    let existing = {
        let mut stmt = tx
            .prepare("SELECT id FROM groups ORDER BY id")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err)?
    };
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    if sorted != existing {
        return Err("分组已变化，请重试".into());
    }
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE groups SET position=?1 WHERE id=?2",
            params![position as i64, id],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

#[tauri::command]
pub(super) fn rename_group(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: i64,
    name: String,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name == "默认" || name == "收藏" || name.chars().count() > 40 {
        return Err("分组名称无效".into());
    }
    let updated = store
        .db
        .lock()
        .map_err(err)?
        .execute("UPDATE groups SET name=?1 WHERE id=?2", params![name, id])
        .map_err(err)?;
    if updated == 0 {
        return Err("分组不存在".into());
    }
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(super) fn delete_group(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: i64,
) -> Result<()> {
    store
        .db
        .lock()
        .map_err(err)?
        .execute("DELETE FROM groups WHERE id=?1", [id])
        .map_err(err)?;
    clean(&store)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(super) fn set_favorite(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: String,
    favorite: bool,
) -> Result<()> {
    store
        .db
        .lock()
        .map_err(err)?
        .execute(
            "UPDATE records SET favorite=?1 WHERE id=?2",
            params![favorite, id],
        )
        .map_err(err)?;
    clean(&store)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(super) fn set_record_pinned(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: String,
    pinned: bool,
) -> Result<()> {
    let conn = store.db.lock().map_err(err)?;
    set_pinned(&conn, &id, pinned)?;
    drop(conn);
    changed(&app);
    Ok(())
}

fn set_pinned(conn: &Connection, id: &str, pinned: bool) -> Result<()> {
    conn.execute(
        "UPDATE records SET pinned_at=CASE WHEN ?2 THEN MAX(?3, COALESCE((SELECT MAX(pinned_at)+1 FROM records), ?3)) ELSE NULL END WHERE id=?1",
        params![id, pinned, now()],
    )
    .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub(super) fn move_group(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: String,
    group_id: Option<i64>,
) -> Result<()> {
    store
        .db
        .lock()
        .map_err(err)?
        .execute(
            "UPDATE records SET group_id=?1 WHERE id=?2",
            params![group_id, id],
        )
        .map_err(err)?;
    clean(&store)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(super) fn delete_record(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: String,
) -> Result<()> {
    let conn = store.db.lock().map_err(err)?;
    remove_record(&conn, &store.dir, &id)?;
    changed(&app);
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum BatchOperation {
    Delete,
    SetFavorite {
        favorite: bool,
    },
    SetPinned {
        pinned: bool,
    },
    MoveGroup {
        #[serde(rename = "groupId")]
        group_id: Option<i64>,
    },
}

fn apply_batch(
    conn: &Connection,
    dir: &Path,
    ids: &[String],
    operation: &BatchOperation,
) -> Result<()> {
    for id in ids {
        match operation {
            BatchOperation::Delete => remove_record(conn, dir, id)?,
            BatchOperation::SetFavorite { favorite } => {
                conn.execute(
                    "UPDATE records SET favorite=?1 WHERE id=?2",
                    params![favorite, id],
                )
                .map_err(err)?;
            }
            BatchOperation::SetPinned { pinned } => set_pinned(conn, id, *pinned)?,
            BatchOperation::MoveGroup { group_id } => {
                conn.execute(
                    "UPDATE records SET group_id=?1 WHERE id=?2",
                    params![group_id, id],
                )
                .map_err(err)?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub(super) fn batch_records(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    ids: Vec<String>,
    operation: BatchOperation,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let conn = store.db.lock().map_err(err)?;
    let result = apply_batch(&conn, &store.dir, &ids, &operation);
    drop(conn);
    let result = result.and_then(|_| {
        if matches!(
            operation,
            BatchOperation::SetFavorite { .. } | BatchOperation::MoveGroup { .. }
        ) {
            clean(&store)?;
        }
        Ok(())
    });
    changed(&app);
    result
}

#[cfg(test)]
#[path = "tests/records.rs"]
mod tests;
