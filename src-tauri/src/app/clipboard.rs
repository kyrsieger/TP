use super::{
    cleanup::clean,
    common::{err, now, Result},
    digest::{file_digest, hash, is_hex_color},
    events::changed,
    files::save_files,
    filesystem::file_kind,
    media::list_thumbnail,
    model::FileItem,
    panel::hide_panel,
    storage::{settings, Store},
};
use crate::platform;
use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContext, ClipboardHandler, RustImageData,
};
use rusqlite::{params, OptionalExtension};
use std::{fs, path::Path, thread};
use tauri::Manager;
use uuid::Uuid;

fn text_kind(text: &str) -> &'static str {
    if is_hex_color(text) {
        "color"
    } else if url::Url::parse(text.trim())
        .is_ok_and(|url| ["http", "https"].contains(&url.scheme()))
    {
        "link"
    } else {
        "text"
    }
}

fn text_for_use(body: &str, remove_trailing_newlines: bool) -> &str {
    if remove_trailing_newlines {
        body.trim_end_matches(['\r', '\n'])
    } else {
        body
    }
}

fn text_writeback(body: &str, remove_trailing_newlines: bool) -> (&str, String) {
    let text = text_for_use(body, remove_trailing_newlines);
    // The watcher classifies and hashes the actual clipboard text, not the stored original.
    (text, hash(text_kind(text), text.as_bytes()))
}
#[tauri::command]
pub(super) fn apply_record(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    id: String,
) -> Result<()> {
    let (kind, body, files, mode, remove_trailing_newlines): (
        String,
        String,
        String,
        String,
        bool,
    ) = {
        let conn = store.db.lock().map_err(err)?;
        let (kind, body, files, mode) = conn
            .query_row(
                "SELECT kind,body,files,storage_mode FROM records WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(err)?;
        (
            kind,
            body,
            files,
            mode,
            settings(&conn)?.remove_trailing_newlines,
        )
    };
    let ctx = ClipboardContext::new().map_err(err)?;
    let files: Vec<FileItem> = serde_json::from_str(&files).map_err(err)?;
    match kind.as_str() {
        "text" | "link" | "color" => {
            let (text, digest) = text_writeback(&body, remove_trailing_newlines);
            *store.suppressed_digest.lock().map_err(err)? = Some(digest);
            ctx.set_text(text.to_owned()).map_err(err)?;
        }
        "image" if files.is_empty() => {
            let path = store
                .dir
                .join("assets")
                .join(&id)
                .join("image.png")
                .to_string_lossy()
                .into_owned();
            // 写回后剪贴板带文件路径，监听器按文件路径计算摘要，因此按该路径重新计算。
            *store.suppressed_digest.lock().map_err(err)? =
                Some(file_digest(&[path.clone()], "image")?);
            platform::set_clipboard_image_with_file(Path::new(&path)).map_err(err)?;
        }
        "image" | "file" | "video" => {
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
            let _ = hide_panel(app.clone());
        }
        let _ = platform::paste_to_target(target);
    }
    Ok(())
}

fn ingest(app: &tauri::AppHandle, ctx: &ClipboardContext) -> Result<()> {
    let store = app.state::<Store>();
    let files = ctx.get_files().unwrap_or_default();
    // 写回图片记录时会把本工具保存的图片文件放上剪贴板，且一次写回可能触发多次更新通知；
    // 这类自有文件不再记入历史，避免重复记录。
    if !files.is_empty()
        && files
            .iter()
            .all(|path| Path::new(path).starts_with(store.dir.join("assets")))
    {
        return Ok(());
    }
    let (kind, body, file_items, image_bytes, digest) = if !files.is_empty() {
        let kind = file_kind(&files);
        let digest = file_digest(&files, kind)?;
        (
            kind.to_string(),
            String::new(),
            files
                .into_iter()
                .map(|original| FileItem {
                    is_directory: Path::new(&original).is_dir(),
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
        let kind = text_kind(&text);
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
        let created_at = now();
        conn.execute("INSERT INTO records(id,digest,kind,body,files,used_at,created_at,expires_at,storage_mode,status) VALUES(?1,?2,?3,?4,?5,?6,?6,?7,?8,?9)",
            params![id, digest, kind, body, serde_json::to_string(&file_items).map_err(err)?, created_at, expires,
                if file_items.is_empty() { "" } else { "reference" },
                if file_items.is_empty() { "ready" } else { "pending" }]).map_err(err)?;
    }
    changed(app);
    if let Some(bytes) = image_bytes {
        let dir = store.dir.join("assets").join(&id);
        fs::create_dir_all(&dir).map_err(err)?;
        fs::write(dir.join("image.png"), &bytes).map_err(err)?;
        let image = RustImageData::from_bytes(&bytes).map_err(err)?;
        list_thumbnail(&image.get_dynamic_image().map_err(err)?)
            .save(dir.join("thumbnail.png"))
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
        thread::spawn(move || save_files(app, id, kind, file_items));
    }
    if clean(&store)? {
        changed(app);
    }
    Ok(())
}

pub(super) struct Watcher {
    pub(super) app: tauri::AppHandle,
    pub(super) ctx: ClipboardContext,
}
impl ClipboardHandler for Watcher {
    fn on_clipboard_change(&mut self) {
        if let Err(error) = ingest(&self.app, &self.ctx) {
            eprintln!("Clipboard capture failed: {error}");
        }
    }
}

#[cfg(test)]
#[path = "tests/clipboard.rs"]
mod tests;
