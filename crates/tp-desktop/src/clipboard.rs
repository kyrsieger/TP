use super::{
    cleanup::clean,
    common::{err, now, Result},
    digest::{file_digest, hash, is_hex_color},
    files::save_files,
    filesystem::file_kind,
    media::list_thumbnail,
    model::FileItem,
    storage::{settings, Store},
};
use crate::clipboard_diagnostics::{event, text_metadata};
use crate::platform;
use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContext, ClipboardHandler, RustImageData,
};
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use std::{fs, path::Path, thread};
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
        body.trim_end_matches(char::is_whitespace)
    } else {
        body
    }
}

fn text_writeback(body: &str, remove_trailing_newlines: bool) -> (&str, String) {
    let text = text_for_use(body, remove_trailing_newlines);
    // The watcher classifies and hashes the actual clipboard text, not the stored original.
    (text, hash(text_kind(text), text.as_bytes()))
}
// Self writes can emit multiple clipboard notifications. Keep suppressing the same
// content until a different copy arrives, rather than consuming the marker once.
fn is_suppressed_copy(store: &Store, digest: &str, revision: Option<u64>) -> Result<bool> {
    let mut suppressed = store.suppressed_digest.lock().map_err(err)?;
    if let Some((expected, expected_revision)) = suppressed.as_mut() {
        if expected == digest && (expected_revision.is_none() || *expected_revision == revision) {
            *expected_revision = revision;
            return Ok(true);
        }
    }
    *suppressed = None;
    Ok(false)
}

// The writer checks that the clipboard still contains the captured copy before
// replacing it. A newer copy or a failed write must not leave a suppression marker.
fn normalize_copied_text(
    store: &Store,
    body: &str,
    enabled: bool,
    write: impl FnOnce(&str) -> Result<bool>,
) -> Result<bool> {
    let (text, digest) = text_writeback(body, enabled);
    // A clean plain-text representation may still have trailing empty HTML paragraphs.
    if !enabled {
        return Ok(false);
    }
    let mut suppressed = store.suppressed_digest.lock().map_err(err)?;
    let previous = suppressed.replace((digest, None));
    match write(text) {
        Ok(true) => {
            suppressed.as_mut().unwrap().1 = platform::clipboard_revision();
            Ok(true)
        }
        result => {
            *suppressed = previous;
            result
        }
    }
}

pub fn write_record(store: &Store, id: String) -> Result<()> {
    let _data = store.read_data()?;
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
            *store.suppressed_digest.lock().map_err(err)? = Some((digest, None));
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
                Some((file_digest(&[path.clone()], "image")?, None));
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
                return Err(tp_core::i18n::tr("文件已不存在，无法放回剪贴板").into());
            }
            *store.suppressed_digest.lock().map_err(err)? =
                Some((file_digest(&paths, &kind)?, None));
            ctx.set_files(paths).map_err(err)?;
        }
        _ => return Err(tp_core::i18n::tr("不支持的记录类型").into()),
    }
    if let Some((_, revision)) = store.suppressed_digest.lock().map_err(err)?.as_mut() {
        *revision = platform::clipboard_revision();
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
    Ok(())
}

#[derive(Clone)]
enum Captured {
    Files(Vec<String>),
    Image(Vec<u8>),
    Text(String),
}

pub fn ingest(
    store: &std::sync::Arc<Store>,
    changed: &crate::Changed,
    ctx: &ClipboardContext,
) -> Result<()> {
    let _data = store.read_data()?;
    let revision = platform::clipboard_revision();
    event("capture_begin", || json!({"revision": revision}));
    let files = ctx.get_files().unwrap_or_default();
    let capture = if !files.is_empty() {
        Captured::Files(files)
    } else if let Ok(image) = ctx.get_image() {
        Captured::Image(image.to_png().map_err(err)?.get_bytes().to_vec())
    } else if let Ok(text) = ctx.get_text() {
        Captured::Text(text)
    } else {
        event("capture_unreadable", || json!({"revision": revision}));
        return Ok(());
    };
    ingest_capture_locked(
        store,
        changed,
        capture,
        revision,
        platform::source_is_1password(),
        |body, text| platform::normalize_clipboard_text(revision, body, text).map_err(err),
    )
}

// The caller holds the data guard across clipboard reads and persistence, so an
// import cannot replace the data between capture and insertion.
fn ingest_capture_locked(
    store: &std::sync::Arc<Store>,
    changed: &crate::Changed,
    capture: Captured,
    revision: Option<u64>,
    sensitive: bool,
    normalize: impl FnOnce(&str, &str) -> Result<bool>,
) -> Result<()> {
    let (kind, body, file_items, image_bytes, digest) = match capture {
        Captured::Files(files) => {
            if files
                .iter()
                .all(|path| Path::new(path).starts_with(store.dir.join("assets")))
            {
                return Ok(());
            }
            let kind = file_kind(&files);
            let digest = file_digest(&files, kind)?;
            (
                kind.to_owned(),
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
        }
        Captured::Image(bytes) => {
            let digest = hash("image", &bytes);
            (
                "image".into(),
                String::new(),
                Vec::new(),
                Some(bytes),
                digest,
            )
        }
        Captured::Text(text) => {
            if text.is_empty() {
                return Ok(());
            }
            let kind = text_kind(&text);
            let digest = hash(kind, text.as_bytes());
            (kind.into(), text, Vec::new(), None, digest)
        }
    };
    event(
        "capture_read",
        || json!({"revision": revision, "current_revision": platform::clipboard_revision(), "kind": kind, "text": text_metadata(&body)}),
    );
    if is_suppressed_copy(&store, &digest, revision)? {
        event("capture_suppressed", || json!({"revision": revision}));
        return Ok(());
    }

    let config = settings(&*store.db.lock().map_err(err)?)?;
    event(
        "capture_settings",
        || json!({"revision": revision, "enabled": config.remove_trailing_newlines}),
    );
    let expires = sensitive.then(|| now() + i64::from(config.sensitive_seconds) * 1000);
    let id = Uuid::new_v4().to_string();
    let existing = {
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
        } else {
            let created_at = now();
            conn.execute("INSERT INTO records(id,digest,kind,body,files,used_at,created_at,expires_at,storage_mode,status) VALUES(?1,?2,?3,?4,?5,?6,?6,?7,?8,?9)",
            params![id, digest, kind, body, serde_json::to_string(&file_items).map_err(err)?, created_at, expires,
                if file_items.is_empty() { "" } else { "reference" },
                if file_items.is_empty() { "ready" } else { "pending" }]).map_err(err)?;
        }
        existing.is_some()
    };
    // Normalize only after the original is safely stored, including duplicates.
    if matches!(kind.as_str(), "text" | "link" | "color") {
        let result =
            normalize_copied_text(&store, &body, config.remove_trailing_newlines, |text| {
                normalize(&body, text)
            });
        event(
            "capture_cleanup_result",
            || json!({"revision": revision, "current_revision": platform::clipboard_revision(), "result": result}),
        );
        if let Err(error) = result {
            eprintln!("Clipboard whitespace cleanup failed: {error}");
        }
    }
    changed();
    if existing {
        return Ok(());
    }
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
        changed();
    }
    if !file_items.is_empty() {
        let store = store.clone();
        let changed = changed.clone();
        thread::spawn(move || save_files(store, changed, id, kind, file_items));
    }
    if clean(&store)? {
        changed();
    }
    Ok(())
}

pub struct Watcher {
    pub store: std::sync::Arc<Store>,
    pub changed: crate::Changed,
    pub ctx: ClipboardContext,
}
impl ClipboardHandler for Watcher {
    fn on_clipboard_change(&mut self) {
        if let Err(error) = ingest(&self.store, &self.changed, &self.ctx) {
            eprintln!("Clipboard capture failed: {error}");
        }
    }
}

#[cfg(test)]
#[path = "tests/clipboard.rs"]
mod tests;
