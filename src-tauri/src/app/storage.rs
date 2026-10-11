use super::{
    common::{err, Result},
    digest::{hash, is_hex_color},
    filesystem::unique_file_name,
    model::{FileItem, Group, Record, Settings, Snapshot},
};
use crate::platform;
use rusqlite::{params, Connection};
use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
pub(super) struct Store {
    pub(super) db: Mutex<Connection>,
    pub(super) dir: PathBuf,
    pub(super) media_lock: Mutex<()>,
    pub(super) paste_target: Mutex<Option<platform::PasteTarget>>,
    pub(super) suppressed_digest: Mutex<Option<String>>,
}

pub(super) fn settings(conn: &Connection) -> Result<Settings> {
    let json: String = conn
        .query_row("SELECT value FROM settings WHERE key='main'", [], |r| {
            r.get(0)
        })
        .map_err(err)?;
    serde_json::from_str(&json).map_err(err)
}

pub(super) fn initialize(dir: PathBuf) -> Result<Store> {
    fs::create_dir_all(dir.join("assets")).map_err(err)?;
    let conn = Connection::open(dir.join("history.sqlite3")).map_err(err)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS groups (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
        CREATE TABLE IF NOT EXISTS records (
            id TEXT PRIMARY KEY, digest TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '', title TEXT NOT NULL DEFAULT '', files TEXT NOT NULL DEFAULT '[]',
            favorite INTEGER NOT NULL DEFAULT 0, pinned_at INTEGER, group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
            used_at INTEGER NOT NULL, created_at INTEGER, expires_at INTEGER, storage_mode TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'ready', preview INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS records_used ON records(used_at DESC);
        PRAGMA foreign_keys=ON;").map_err(err)?;
    let has_title: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('records') WHERE name='title')",
            [],
            |row| row.get(0),
        )
        .map_err(err)?;
    if !has_title {
        conn.execute(
            "ALTER TABLE records ADD COLUMN title TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(err)?;
    }
    let has_pinned_at: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('records') WHERE name='pinned_at')",
            [],
            |row| row.get(0),
        )
        .map_err(err)?;
    if !has_pinned_at {
        conn.execute("ALTER TABLE records ADD COLUMN pinned_at INTEGER", [])
            .map_err(err)?;
    }
    let has_created_at: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('records') WHERE name='created_at')",
            [],
            |row| row.get(0),
        )
        .map_err(err)?;
    if !has_created_at {
        conn.execute("ALTER TABLE records ADD COLUMN created_at INTEGER", [])
            .map_err(err)?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO settings(key,value) VALUES('main',?1)",
        [serde_json::to_string(&Settings::default()).map_err(err)?],
    )
    .map_err(err)?;
    let old_colors: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id,body FROM records WHERE kind='text'")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;
        rows
    };
    for (id, body) in old_colors {
        if is_hex_color(&body) {
            conn.execute(
                "UPDATE records SET kind='color',digest=?1 WHERE id=?2",
                params![hash("color", body.as_bytes()), id],
            )
            .map_err(err)?;
        }
    }
    migrate_flat_file_copies(&conn, &dir)?;
    Ok(Store {
        db: Mutex::new(conn),
        dir,
        media_lock: Mutex::new(()),
        paste_target: Mutex::new(None),
        suppressed_digest: Mutex::new(None),
    })
}

/// 把旧格式的副本 `files/<序号>/<文件名>` 迁移为平铺的 `files/<文件名>`。
///
/// 迁移只改写 `stored` 路径，`digest` 由原路径与元数据计算，不受影响。
fn migrate_flat_file_copies(conn: &Connection, dir: &Path) -> Result<()> {
    let records: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id,files FROM records WHERE storage_mode='copy' AND status!='pending'")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        rows
    };
    for (id, files) in records {
        let mut items: Vec<FileItem> = serde_json::from_str(&files).unwrap_or_default();
        let files_dir = dir.join("assets").join(&id).join("files");
        let mut taken = HashSet::<OsString>::new();
        let mut migrated = false;
        for file in &mut items {
            let Some(stored) = file.stored.as_deref() else {
                continue;
            };
            let stored = PathBuf::from(stored);
            let Ok(relative) = stored.strip_prefix(&files_dir) else {
                continue;
            };
            // 旧格式为 files/<序号>/<文件名>；平铺格式只有一个路径组件，无需迁移。
            let parts = relative
                .components()
                .filter_map(|component| match component {
                    Component::Normal(name) => Some(name.to_os_string()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let Some(name) = parts.last().cloned() else {
                continue;
            };
            if parts.len() < 2 {
                taken.insert(name);
                continue;
            }
            let target = files_dir.join(unique_file_name(&name, &mut taken));
            if stored != target && stored.is_file() {
                fs::rename(&stored, &target).map_err(err)?;
            }
            if target.is_file() {
                file.stored = Some(target.to_string_lossy().into_owned());
                migrated = true;
            }
        }
        if migrated {
            conn.execute(
                "UPDATE records SET files=?1 WHERE id=?2",
                params![serde_json::to_string(&items).unwrap_or_default(), id],
            )
            .map_err(err)?;
            remove_numbered_dirs(&files_dir);
        }
    }
    Ok(())
}

/// 删除旧格式遗留的 `files/<纯数字>/` 空目录；失败不影响启动。
fn remove_numbered_dirs(files_dir: &Path) {
    let Ok(entries) = fs::read_dir(files_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if entry.path().is_dir() && name.to_string_lossy().parse::<u64>().is_ok() {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

pub(super) fn snapshot(store: &Store) -> Result<Snapshot> {
    let conn = store.db.lock().map_err(err)?;
    let mut stmt = conn.prepare("SELECT id,kind,body,files,favorite,pinned_at,group_id,used_at,created_at,expires_at,storage_mode,status,preview,title FROM records ORDER BY pinned_at IS NULL, pinned_at DESC, used_at DESC").map_err(err)?;
    let mut records = stmt
        .query_map([], |r| {
            let files: String = r.get(3)?;
            Ok(Record {
                id: r.get(0)?,
                kind: r.get(1)?,
                body: r.get(2)?,
                title: r.get(13)?,
                files: serde_json::from_str(&files).unwrap_or_default(),
                favorite: r.get::<_, i64>(4)? != 0,
                pinned_at: r.get(5)?,
                group_id: r.get(6)?,
                used_at: r.get(7)?,
                created_at: r.get(8)?,
                expires_at: r.get(9)?,
                storage_mode: r.get(10)?,
                status: r.get(11)?,
                preview: r.get::<_, i64>(12)? != 0,
                asset_path: None,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    for record in &mut records {
        for file in &mut record.files {
            if !file.is_directory {
                file.is_directory = Path::new(&file.original).is_dir()
                    || file
                        .stored
                        .as_ref()
                        .is_some_and(|path| Path::new(path).is_dir());
            }
        }
        // 直接复制的图片（截图等只有位图的复制）没有原文件，悬停预览按记录的图片副本显示路径。
        if record.kind == "image" && record.files.is_empty() {
            let path = store.dir.join("assets").join(&record.id).join("image.png");
            if path.is_file() {
                record.asset_path = Some(path.to_string_lossy().into_owned());
            }
        }
        if record.status != "pending" && !record.files.is_empty() {
            let available = record.files.iter().all(|file| {
                let path = if record.storage_mode == "copy" {
                    file.stored.as_ref().unwrap_or(&file.original)
                } else {
                    &file.original
                };
                Path::new(path).exists()
            });
            if !available {
                record.status = "missing".into();
            }
        }
    }
    let mut stmt = conn
        .prepare("SELECT id,name FROM groups ORDER BY id")
        .map_err(err)?;
    let groups = stmt
        .query_map([], |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(Snapshot {
        records,
        groups,
        settings: settings(&conn)?,
    })
}

#[tauri::command]
pub(super) fn get_state(store: tauri::State<Store>) -> Result<Snapshot> {
    snapshot(&store)
}

#[cfg(test)]
#[path = "tests/storage.rs"]
mod tests;
