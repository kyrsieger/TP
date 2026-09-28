use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher,
    ClipboardWatcherContext, RustImageData,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use uuid::Uuid;

use crate::platform;

type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn is_hex_color(text: &str) -> bool {
    let Some(digits) = text.trim().strip_prefix('#') else {
        return false;
    };
    [3, 4, 6, 8].contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    max_ordinary: u16,
    retention_days: u16,
    sensitive_seconds: u32,
    shortcut: String,
    position_mode: String,
    fixed_position: String,
    remember_position: bool,
    panel_width: f64,
    panel_height: f64,
    panel_x: Option<f64>,
    panel_y: Option<f64>,
    #[serde(default)]
    always_on_top: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_ordinary: 300,
            retention_days: 7,
            sensitive_seconds: 90,
            shortcut: String::new(),
            position_mode: "fixed".into(),
            fixed_position: "right".into(),
            remember_position: false,
            panel_width: 420.0,
            panel_height: 600.0,
            panel_x: None,
            panel_y: None,
            always_on_top: false,
        }
    }
}

impl Settings {
    fn validate(&self) -> Result<()> {
        if !(1..=500).contains(&self.max_ordinary)
            || ![1, 3, 5, 7, 30].contains(&self.retention_days)
            || self.sensitive_seconds == 0
            || !["fixed", "cursor"].contains(&self.position_mode.as_str())
            || !["left", "right", "center"].contains(&self.fixed_position.as_str())
            || self.panel_width < 340.0
            || self.panel_height < 400.0
        {
            return Err("设置值超出允许范围".into());
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileItem {
    original: String,
    stored: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    id: String,
    kind: String,
    body: String,
    files: Vec<FileItem>,
    favorite: bool,
    pinned_at: Option<i64>,
    group_id: Option<i64>,
    used_at: i64,
    expires_at: Option<i64>,
    storage_mode: String,
    status: String,
    preview: bool,
}

#[derive(Serialize)]
struct Group {
    id: i64,
    name: String,
}

#[derive(Serialize)]
struct Snapshot {
    records: Vec<Record>,
    groups: Vec<Group>,
    settings: Settings,
}

struct Store {
    db: Mutex<Connection>,
    dir: PathBuf,
    paste_target: Mutex<Option<platform::PasteTarget>>,
    suppressed_digest: Mutex<Option<String>>,
}

fn settings(conn: &Connection) -> Result<Settings> {
    let json: String = conn
        .query_row("SELECT value FROM settings WHERE key='main'", [], |r| {
            r.get(0)
        })
        .map_err(err)?;
    serde_json::from_str(&json).map_err(err)
}

fn initialize(dir: PathBuf) -> Result<Store> {
    fs::create_dir_all(dir.join("assets")).map_err(err)?;
    let conn = Connection::open(dir.join("history.sqlite3")).map_err(err)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS groups (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
        CREATE TABLE IF NOT EXISTS records (
            id TEXT PRIMARY KEY, digest TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '', files TEXT NOT NULL DEFAULT '[]',
            favorite INTEGER NOT NULL DEFAULT 0, pinned_at INTEGER, group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
            used_at INTEGER NOT NULL, expires_at INTEGER, storage_mode TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'ready', preview INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS records_used ON records(used_at DESC);
        PRAGMA foreign_keys=ON;").map_err(err)?;
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
    Ok(Store {
        db: Mutex::new(conn),
        dir,
        paste_target: Mutex::new(None),
        suppressed_digest: Mutex::new(None),
    })
}

fn snapshot(store: &Store) -> Result<Snapshot> {
    let conn = store.db.lock().map_err(err)?;
    let mut stmt = conn.prepare("SELECT id,kind,body,files,favorite,pinned_at,group_id,used_at,expires_at,storage_mode,status,preview FROM records ORDER BY pinned_at IS NULL, pinned_at DESC, used_at DESC").map_err(err)?;
    let mut records = stmt
        .query_map([], |r| {
            let files: String = r.get(3)?;
            Ok(Record {
                id: r.get(0)?,
                kind: r.get(1)?,
                body: r.get(2)?,
                files: serde_json::from_str(&files).unwrap_or_default(),
                favorite: r.get::<_, i64>(4)? != 0,
                pinned_at: r.get(5)?,
                group_id: r.get(6)?,
                used_at: r.get(7)?,
                expires_at: r.get(8)?,
                storage_mode: r.get(9)?,
                status: r.get(10)?,
                preview: r.get::<_, i64>(11)? != 0,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    for record in &mut records {
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

fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("history-changed", ());
}

fn remove_record(conn: &Connection, dir: &Path, id: &str) -> Result<()> {
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

fn clean(store: &Store) -> Result<bool> {
    let conn = store.db.lock().map_err(err)?;
    let config = settings(&conn)?;
    let cutoff = now() - i64::from(config.retention_days) * 86_400_000;
    let mut ids = Vec::new();
    let mut stmt = conn.prepare("SELECT id FROM records WHERE favorite=0 AND group_id IS NULL AND (used_at < ?1 OR (expires_at IS NOT NULL AND expires_at <= ?2))").map_err(err)?;
    ids.extend(
        stmt.query_map(params![cutoff, now()], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?,
    );
    let mut stmt = conn.prepare("SELECT id FROM records WHERE favorite=0 AND group_id IS NULL ORDER BY used_at DESC LIMIT -1 OFFSET ?1").map_err(err)?;
    ids.extend(
        stmt.query_map([config.max_ordinary], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?,
    );
    ids.sort();
    ids.dedup();
    for id in &ids {
        remove_record(&conn, &store.dir, id)?;
    }
    Ok(!ids.is_empty())
}

fn clear_cache_data(store: &Store) -> Result<usize> {
    let conn = store.db.lock().map_err(err)?;
    let assets = store.dir.join("assets");
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
    let mut stmt = conn
        .prepare("SELECT id FROM records WHERE favorite=0 AND group_id IS NULL")
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
fn clear_cache(app: tauri::AppHandle, store: tauri::State<Store>) -> Result<usize> {
    let result = clear_cache_data(&store);
    changed(&app);
    result
}

#[tauri::command]
fn get_state(store: tauri::State<Store>) -> Result<Snapshot> {
    snapshot(&store)
}

#[tauri::command]
fn autostart_enabled(app: tauri::AppHandle) -> Result<bool> {
    app.autolaunch().is_enabled().map_err(err)
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<()> {
    if enabled {
        app.autolaunch().enable().map_err(err)
    } else {
        app.autolaunch().disable().map_err(err)
    }
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, store: tauri::State<Store>, value: Settings) -> Result<()> {
    value.validate()?;
    let conn = store.db.lock().map_err(err)?;
    if settings(&conn)?.always_on_top != value.always_on_top {
        app.get_webview_window("main")
            .ok_or("面板窗口不存在")?
            .set_always_on_top(value.always_on_top)
            .map_err(err)?;
    }
    conn.execute(
        "UPDATE settings SET value=?1 WHERE key='main'",
        [serde_json::to_string(&value).map_err(err)?],
    )
    .map_err(err)?;
    drop(conn);
    if clean(&store)? {
        changed(&app);
    }
    Ok(())
}

#[tauri::command]
fn add_group(app: tauri::AppHandle, store: tauri::State<Store>, name: String) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name == "默认" || name == "收藏" || name.chars().count() > 40 {
        return Err("分组名称无效".into());
    }
    store
        .db
        .lock()
        .map_err(err)?
        .execute("INSERT INTO groups(name) VALUES(?1)", [name])
        .map_err(err)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn rename_group(
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
fn delete_group(app: tauri::AppHandle, store: tauri::State<Store>, id: i64) -> Result<()> {
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
fn set_favorite(
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
fn set_record_pinned(
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
fn move_group(
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
fn delete_record(app: tauri::AppHandle, store: tauri::State<Store>, id: String) -> Result<()> {
    let conn = store.db.lock().map_err(err)?;
    remove_record(&conn, &store.dir, &id)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn preview_data(store: tauri::State<Store>, id: String) -> Result<String> {
    let conn = store.db.lock().map_err(err)?;
    let kind: String = conn
        .query_row("SELECT kind FROM records WHERE id=?1", [&id], |r| r.get(0))
        .map_err(err)?;
    if kind != "image" && kind != "video" {
        return Err("此记录没有图片预览".into());
    }
    let path = store.dir.join("assets").join(id).join("preview.png");
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(fs::read(path).map_err(err)?)
    ))
}

#[tauri::command]
fn remember_paste_target(store: tauri::State<Store>) -> Result<()> {
    if let Some(target) = platform::capture_paste_target() {
        *store.paste_target.lock().map_err(err)? = Some(target);
    }
    Ok(())
}

#[tauri::command]
fn show_panel(app: tauri::AppHandle, store: tauri::State<Store>) -> Result<()> {
    remember_paste_target(store)?;
    let window = app.get_webview_window("main").ok_or("面板窗口不存在")?;
    window.show().map_err(err)?;
    window.set_focus().map_err(err)?;
    Ok(())
}

#[tauri::command]
fn hide_panel(app: tauri::AppHandle) -> Result<()> {
    app.get_webview_window("main")
        .ok_or("面板窗口不存在")?
        .hide()
        .map_err(err)
}

#[tauri::command]
fn apply_record(app: tauri::AppHandle, store: tauri::State<Store>, id: String) -> Result<String> {
    let (kind, body, files, mode, digest): (String, String, String, String, String) = {
        let conn = store.db.lock().map_err(err)?;
        conn.query_row(
            "SELECT kind,body,files,storage_mode,digest FROM records WHERE id=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(err)?
    };
    let ctx = ClipboardContext::new().map_err(err)?;
    match kind.as_str() {
        "text" | "link" | "color" => {
            *store.suppressed_digest.lock().map_err(err)? = Some(digest.clone());
            ctx.set_text(body).map_err(err)?;
        }
        "image" => {
            let path = store.dir.join("assets").join(&id).join("image.png");
            *store.suppressed_digest.lock().map_err(err)? = Some(digest.clone());
            ctx.set_image(RustImageData::from_path(&path.to_string_lossy()).map_err(err)?)
                .map_err(err)?;
        }
        "file" | "video" => {
            let files: Vec<FileItem> = serde_json::from_str(&files).map_err(err)?;
            let paths: Vec<String> = files
                .iter()
                .map(|file| {
                    if mode == "copy" {
                        file.stored.as_ref().unwrap_or(&file.original).clone()
                    } else {
                        file.original.clone()
                    }
                })
                .collect();
            if paths.iter().any(|path| !Path::new(path).exists()) {
                return Err("文件已不存在，无法放回剪贴板".into());
            }
            *store.suppressed_digest.lock().map_err(err)? = Some(file_digest(&paths, &kind)?);
            ctx.set_files(paths).map_err(err)?;
        }
        _ => return Err("不支持的记录类型".into()),
    }
    store
        .db
        .lock()
        .map_err(err)?
        .execute(
            "UPDATE records SET used_at=?1 WHERE id=?2",
            params![now(), id],
        )
        .map_err(err)?;
    changed(&app);
    if let Some(target) = store.paste_target.lock().map_err(err)?.take() {
        let pinned = {
            let conn = store.db.lock().map_err(err)?;
            settings(&conn)?.always_on_top
        };
        if !pinned {
            let _ = app.get_webview_window("main").map(|window| window.hide());
        }
        if platform::paste_to_target(target) {
            return Ok("已放回剪贴板，并已尝试粘贴到原窗口".into());
        }
    }
    Ok("已放回系统剪贴板，请在目标位置手动粘贴".into())
}

fn video(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "wmv"
            )
        })
}

fn file_size(path: &Path) -> std::io::Result<u64> {
    let meta = fs::metadata(path)?;
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut size = 0;
    for item in fs::read_dir(path)? {
        size += file_size(&item?.path())?;
    }
    Ok(size)
}

fn copy_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for item in fs::read_dir(from)? {
            let item = item?;
            copy_path(&item.path(), &to.join(item.file_name()))?;
        }
    } else {
        fs::copy(from, to)?;
    }
    Ok(())
}

fn save_files(app: tauri::AppHandle, id: String, mut files: Vec<FileItem>) {
    let store = app.state::<Store>();
    let total = files.iter().try_fold(0u64, |sum, file| {
        file_size(Path::new(&file.original))
            .ok()
            .map(|size| sum.saturating_add(size))
    });
    let mut mode = "reference";
    let mut status = "ready";
    if total.is_some_and(|size| size <= 100 * 1024 * 1024) {
        let dir = store.dir.join("assets").join(&id).join("files");
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(&dir)?;
            for (index, file) in files.iter_mut().enumerate() {
                let name = Path::new(&file.original)
                    .file_name()
                    .ok_or_else(|| std::io::Error::other("invalid filename"))?;
                let target_dir = dir.join(index.to_string());
                fs::create_dir_all(&target_dir)?;
                let target = target_dir.join(name);
                copy_path(Path::new(&file.original), &target)?;
                file.stored = Some(target.to_string_lossy().into_owned());
            }
            Ok(())
        })();
        if result.is_ok() {
            mode = "copy";
        } else {
            status = "copy_failed";
            files.iter_mut().for_each(|file| file.stored = None);
            let _ = fs::remove_dir_all(&dir);
        }
    }
    let updated = store
        .db
        .lock()
        .ok()
        .and_then(|conn| {
            conn.execute(
                "UPDATE records SET files=?1,storage_mode=?2,status=?3 WHERE id=?4",
                params![
                    serde_json::to_string(&files).unwrap_or_default(),
                    mode,
                    status,
                    id
                ],
            )
            .ok()
        })
        .unwrap_or(0);
    if updated == 0 {
        let _ = fs::remove_dir_all(store.dir.join("assets").join(&id));
        return;
    }
    if files.len() == 1 && video(&files[0].original) {
        let source = files[0].stored.as_ref().unwrap_or(&files[0].original);
        let preview = store.dir.join("assets").join(&id).join("preview.png");
        if platform::video_preview(Path::new(source), &preview) {
            if let Ok(conn) = store.db.lock() {
                let _ = conn.execute("UPDATE records SET preview=1 WHERE id=?1", [&id]);
            }
        }
    }
    changed(&app);
}

fn hash(kind: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn file_digest(paths: &[String], kind: &str) -> Result<String> {
    let mut signature = String::new();
    for path in paths {
        let metadata = fs::metadata(path).map_err(err)?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|t| t.as_millis())
            .unwrap_or(0);
        signature.push_str(&format!("{path}\0{}\0{modified}\0", metadata.len()));
    }
    Ok(hash(kind, signature.as_bytes()))
}

fn ingest(app: &tauri::AppHandle, ctx: &ClipboardContext) -> Result<()> {
    let store = app.state::<Store>();
    let files = ctx.get_files().unwrap_or_default();
    let (kind, body, file_items, image_bytes, digest) = if !files.is_empty() {
        let kind = if files.iter().all(|path| video(path)) {
            "video"
        } else {
            "file"
        };
        let digest = file_digest(&files, kind)?;
        (
            kind.to_string(),
            String::new(),
            files
                .into_iter()
                .map(|original| FileItem {
                    original,
                    stored: None,
                })
                .collect(),
            None,
            digest,
        )
    } else if let Ok(image) = ctx.get_image() {
        let bytes = image.to_png().map_err(err)?.get_bytes().to_vec();
        let digest = hash("image", &bytes);
        (
            "image".into(),
            String::new(),
            Vec::new(),
            Some(bytes),
            digest,
        )
    } else if let Ok(text) = ctx.get_text() {
        if text.is_empty() {
            return Ok(());
        }
        let kind = if is_hex_color(&text) {
            "color"
        } else if url::Url::parse(text.trim())
            .is_ok_and(|url| ["http", "https"].contains(&url.scheme()))
        {
            "link"
        } else {
            "text"
        };
        let digest = hash(kind, text.as_bytes());
        (kind.into(), text, Vec::new(), None, digest)
    } else {
        return Ok(());
    };

    if store
        .suppressed_digest
        .lock()
        .map_err(err)?
        .take()
        .as_deref()
        == Some(&digest)
    {
        return Ok(());
    }
    let sensitive = platform::source_is_1password();
    let config = settings(&*store.db.lock().map_err(err)?)?;
    let expires = sensitive.then(|| now() + i64::from(config.sensitive_seconds) * 1000);
    let id = Uuid::new_v4().to_string();
    {
        let conn = store.db.lock().map_err(err)?;
        let existing: Option<String> = conn
            .query_row("SELECT id FROM records WHERE digest=?1", [&digest], |r| {
                r.get(0)
            })
            .optional()
            .map_err(err)?;
        if existing.is_some() {
            conn.execute(
                "UPDATE records SET used_at=?1,expires_at=?2 WHERE digest=?3",
                params![now(), expires, digest],
            )
            .map_err(err)?;
            drop(conn);
            changed(app);
            return Ok(());
        }
        conn.execute("INSERT INTO records(id,digest,kind,body,files,used_at,expires_at,storage_mode,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![id, digest, kind, body, serde_json::to_string(&file_items).map_err(err)?, now(), expires,
                if file_items.is_empty() { "" } else { "reference" },
                if file_items.is_empty() { "ready" } else { "pending" }]).map_err(err)?;
    }
    changed(app);
    if let Some(bytes) = image_bytes {
        let dir = store.dir.join("assets").join(&id);
        fs::create_dir_all(&dir).map_err(err)?;
        fs::write(dir.join("image.png"), &bytes).map_err(err)?;
        let image = RustImageData::from_bytes(&bytes).map_err(err)?;
        image
            .thumbnail(380, 260)
            .map_err(err)?
            .save_to_path(&dir.join("preview.png").to_string_lossy())
            .map_err(err)?;
        store
            .db
            .lock()
            .map_err(err)?
            .execute("UPDATE records SET preview=1 WHERE id=?1", [&id])
            .map_err(err)?;
        changed(app);
    }
    if !file_items.is_empty() {
        let app = app.clone();
        thread::spawn(move || save_files(app, id, file_items));
    }
    if clean(&store)? {
        changed(app);
    }
    Ok(())
}

struct Watcher {
    app: tauri::AppHandle,
    ctx: ClipboardContext,
}
impl ClipboardHandler for Watcher {
    fn on_clipboard_change(&mut self) {
        if let Err(error) = ingest(&self.app, &self.ctx) {
            eprintln!("Clipboard capture failed: {error}");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .setup(|app| {
            app.manage(initialize(app.path().app_data_dir()?).map_err(std::io::Error::other)?);
            let pinned = {
                let store = app.state::<Store>();
                let conn = store
                    .db
                    .lock()
                    .map_err(|_| std::io::Error::other("无法读取设置"))?;
                settings(&conn)
                    .map_err(std::io::Error::other)?
                    .always_on_top
            };
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| std::io::Error::other("面板窗口不存在"))?;
            window.set_always_on_top(pinned)?;
            if !std::env::args().any(|arg| arg == "--autostart") {
                window.show()?;
            }
            #[cfg(windows)]
            platform::install_panel_resize_double_click(window.hwnd()?)?;
            #[cfg(target_os = "macos")]
            platform::install_panel_resize_double_click_macos(window.ns_window()?)?;
            let handle = app.handle().clone();
            thread::spawn(move || {
                if let (Ok(ctx), Ok(mut watcher)) =
                    (ClipboardContext::new(), ClipboardWatcherContext::new())
                {
                    watcher.add_handler(Watcher { app: handle, ctx });
                    watcher.start_watch();
                }
            });
            let handle = app.handle().clone();
            thread::spawn(move || loop {
                thread::sleep(Duration::from_secs(1));
                if let Some(store) = handle.try_state::<Store>() {
                    if matches!(clean(&store), Ok(true)) {
                        changed(&handle);
                    }
                }
            });
            let handle = app.handle().clone();
            thread::spawn(move || loop {
                if let (Some(store), Some(target)) = (
                    handle.try_state::<Store>(),
                    platform::capture_paste_target(),
                ) {
                    if let Ok(mut current) = store.paste_target.lock() {
                        *current = Some(target);
                    }
                }
                thread::sleep(Duration::from_millis(100));
            });
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let open = MenuItem::with_id(app, "open", "打开剪贴板", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => {
                        let _ = app.emit("open-requested", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            autostart_enabled,
            set_autostart,
            save_settings,
            add_group,
            rename_group,
            delete_group,
            set_favorite,
            set_record_pinned,
            move_group,
            delete_record,
            clear_cache,
            preview_data,
            remember_paste_target,
            show_panel,
            hide_panel,
            apply_record
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_complete_hex_colors() {
        for value in ["#abc", "#ABCD", "#12ab34", "#12345678", "  #abc  "] {
            assert!(is_hex_color(value), "{value}");
        }
        for value in [
            "abc",
            "#ab",
            "#abcde",
            "#123456789",
            "#12xz34",
            "color: #abc",
            "#abc\n#def",
        ] {
            assert!(!is_hex_color(value), "{value}");
        }
    }

    #[test]
    fn existing_hex_text_becomes_color_without_losing_metadata() {
        let store = test_store();
        let dir = store.dir.clone();
        {
            let conn = store.db.lock().unwrap();
            conn.execute("INSERT INTO groups(id,name) VALUES(1,'调色板')", [])
                .unwrap();
            conn.execute(
                "INSERT INTO records(id,digest,kind,body,used_at,favorite,group_id) VALUES('old',?1,'text','#AbC',?2,1,1)",
                params![hash("text", b"#AbC"), now()],
            ).unwrap();
        }
        drop(store);
        let store = initialize(dir.clone()).unwrap();
        let conn = store.db.lock().unwrap();
        let (kind, digest, favorite, group_id): (String, String, i64, i64) = conn
            .query_row(
                "SELECT kind,digest,favorite,group_id FROM records WHERE id='old'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(kind, "color");
        assert_eq!(digest, hash("color", b"#AbC"));
        assert_eq!((favorite, group_id), (1, 1));
        drop(conn);
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn older_settings_default_to_not_pinned() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("alwaysOnTop");
        let restored: Settings = serde_json::from_value(value).unwrap();
        assert!(!restored.always_on_top);
    }

    fn test_store() -> Store {
        let dir = std::env::temp_dir().join(format!("tp-clipboard-test-{}", Uuid::new_v4()));
        initialize(dir).unwrap()
    }

    #[test]
    fn pinned_records_sort_by_pin_time_and_unpin_restores_use_order() {
        let store = test_store();
        {
            let conn = store.db.lock().unwrap();
            for (id, used_at) in [("new", 3000), ("middle", 2000), ("old", 1000)] {
                conn.execute(
                    "INSERT INTO records(id,digest,kind,used_at) VALUES(?1,?1,'text',?2)",
                    params![id, used_at],
                )
                .unwrap();
            }
            set_pinned(&conn, "old", true).unwrap();
            set_pinned(&conn, "middle", true).unwrap();
        }
        let records = snapshot(&store).unwrap().records;
        assert_eq!(
            records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["middle", "old", "new"]
        );
        assert!(records[0].pinned_at > records[1].pinned_at);

        set_pinned(&store.db.lock().unwrap(), "middle", false).unwrap();
        assert_eq!(
            snapshot(&store)
                .unwrap()
                .records
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            vec!["old", "new", "middle"]
        );
        set_pinned(&store.db.lock().unwrap(), "old", false).unwrap();
        assert_eq!(
            snapshot(&store)
                .unwrap()
                .records
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "middle", "old"]
        );

        let dir = store.dir.clone();
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn existing_records_gain_nullable_pin_time() {
        let dir = std::env::temp_dir().join(format!("tp-clipboard-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let conn = Connection::open(dir.join("history.sqlite3")).unwrap();
        conn.execute_batch(
            "CREATE TABLE records (
            id TEXT PRIMARY KEY, digest TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '', files TEXT NOT NULL DEFAULT '[]',
            favorite INTEGER NOT NULL DEFAULT 0, group_id INTEGER,
            used_at INTEGER NOT NULL, expires_at INTEGER, storage_mode TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'ready', preview INTEGER NOT NULL DEFAULT 0
        ); INSERT INTO records(id,digest,kind,used_at) VALUES('old','old','text',1);",
        )
        .unwrap();
        drop(conn);

        let store = initialize(dir.clone()).unwrap();
        let records = snapshot(&store).unwrap().records;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].pinned_at, None);
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_keeps_favorites_and_grouped_records() {
        let store = test_store();
        {
            let conn = store.db.lock().unwrap();
            conn.execute("INSERT INTO groups(id,name) VALUES(1,'项目')", [])
                .unwrap();
            for (id, favorite, group_id, expires_at) in [
                ("old", 0, None, None),
                ("sensitive", 0, None, Some(now() - 1)),
                ("favorite", 1, None, Some(now() - 1)),
                ("grouped", 0, Some(1), Some(now() - 1)),
            ] {
                conn.execute("INSERT INTO records(id,digest,kind,used_at,favorite,group_id,expires_at) VALUES(?1,?1,'text',?2,?3,?4,?5)",
                    params![id, now() - 8 * 86_400_000, favorite, group_id, expires_at]).unwrap();
            }
        }
        assert!(clean(&store).unwrap());
        let mut remaining: Vec<String> = snapshot(&store)
            .unwrap()
            .records
            .into_iter()
            .map(|r| r.id)
            .collect();
        remaining.sort();
        assert_eq!(remaining, vec!["favorite", "grouped"]);
        let dir = store.dir.clone();
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn count_limit_only_applies_to_ordinary_records() {
        let store = test_store();
        {
            let conn = store.db.lock().unwrap();
            let mut value = settings(&conn).unwrap();
            value.max_ordinary = 1;
            conn.execute(
                "UPDATE settings SET value=?1 WHERE key='main'",
                [serde_json::to_string(&value).unwrap()],
            )
            .unwrap();
            let base = now();
            for (id, age, favorite) in [("new", 1, 0), ("old", 2, 0), ("saved", 3, 1)] {
                conn.execute("INSERT INTO records(id,digest,kind,used_at,favorite) VALUES(?1,?1,'text',?2,?3)",
                    params![id, base - age * 1000, favorite]).unwrap();
            }
        }
        assert!(clean(&store).unwrap());
        let remaining: Vec<String> = snapshot(&store)
            .unwrap()
            .records
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(remaining, vec!["new", "saved"]);
        let dir = store.dir.clone();
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clear_cache_removes_ordinary_records_and_orphan_assets() {
        let store = test_store();
        let assets = store.dir.join("assets");
        let source = store.dir.join("original.txt");
        fs::write(&source, b"original").unwrap();
        for id in [
            "image", "video", "file", "favorite", "grouped", "both", "shared", "orphan",
        ] {
            fs::create_dir_all(assets.join(id)).unwrap();
            fs::write(assets.join(id).join("data.bin"), id).unwrap();
        }
        fs::write(assets.join("orphan.bin"), b"orphan").unwrap();
        {
            let conn = store.db.lock().unwrap();
            conn.execute("INSERT INTO groups(id,name) VALUES(1,'项目')", [])
                .unwrap();
            for (id, kind, favorite, group_id, stored_id) in [
                ("text", "text", 0, None, None),
                ("image", "image", 0, None, None),
                ("video", "video", 0, None, Some("video")),
                ("file", "file", 0, None, Some("file")),
                ("favorite", "image", 1, None, None),
                ("grouped", "file", 0, Some(1), Some("grouped")),
                ("both", "video", 1, Some(1), Some("both")),
                ("shared", "file", 0, None, Some("shared")),
                ("shared-owner", "file", 1, None, Some("shared")),
            ] {
                let files = stored_id.map_or_else(
                    || "[]".to_string(),
                    |owner| {
                        serde_json::to_string(&vec![FileItem {
                            original: source.to_string_lossy().into_owned(),
                            stored: Some(
                                assets
                                    .join(owner)
                                    .join("data.bin")
                                    .to_string_lossy()
                                    .into_owned(),
                            ),
                        }])
                        .unwrap()
                    },
                );
                conn.execute("INSERT INTO records(id,digest,kind,files,used_at,favorite,group_id,storage_mode) VALUES(?1,?1,?2,?3,?4,?5,?6,'copy')",
                    params![id, kind, files, now(), favorite, group_id]).unwrap();
            }
        }
        assert_eq!(clear_cache_data(&store).unwrap(), 5);
        let mut remaining: Vec<String> = snapshot(&store)
            .unwrap()
            .records
            .into_iter()
            .map(|r| r.id)
            .collect();
        remaining.sort();
        assert_eq!(
            remaining,
            vec!["both", "favorite", "grouped", "shared-owner"]
        );
        for id in ["image", "video", "file", "orphan"] {
            assert!(!assets.join(id).exists());
        }
        assert!(!assets.join("orphan.bin").exists());
        for id in ["favorite", "grouped", "both", "shared"] {
            assert!(assets.join(id).join("data.bin").exists());
        }
        assert!(source.exists());
        assert!(store.dir.join("history.sqlite3").exists());
        assert_eq!(
            settings(&store.db.lock().unwrap()).unwrap().max_ordinary,
            300
        );
        fs::write(assets.join("late-orphan.bin"), b"orphan").unwrap();
        assert_eq!(clear_cache_data(&store).unwrap(), 0);
        assert!(!assets.join("late-orphan.bin").exists());
        let dir = store.dir.clone();
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }
}
