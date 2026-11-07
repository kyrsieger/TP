use super::{
    common::{err, Result},
    filesystem::{copy_path, file_size, unique_file_name},
    media::{save_image_file_preview, save_list_thumbnail},
    model::FileItem,
    storage::Store,
};
use crate::platform;
use clipboard_rs::{Clipboard, ClipboardContext};
use rusqlite::{params, OptionalExtension};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
fn paths_by_directory(paths: &[PathBuf]) -> Result<BTreeMap<PathBuf, Vec<PathBuf>>> {
    if paths.is_empty() {
        return Err(tp_core::i18n::tr("文件路径不可用").into());
    }
    let mut directories = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    for path in paths {
        if !path.is_absolute() || !path.is_file() {
            return Err(tp_core::i18n::format_message!(
                "文件已不存在，无法选中：{}",
                "The file no longer exists and cannot be selected: {}",
                path.display()
            ));
        }
        let directory = path.parent().ok_or(tp_core::i18n::tr("文件路径不可用"))?;
        directories
            .entry(directory.to_path_buf())
            .or_default()
            .push(path.clone());
    }
    Ok(directories)
}

fn record_action_paths(store: &Store, id: &str, stored_only: bool) -> Result<Vec<PathBuf>> {
    let (kind, files_json): (String, String) = store
        .db
        .lock()
        .map_err(err)?
        .query_row("SELECT kind,files FROM records WHERE id=?1", [id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()
        .map_err(err)?
        .ok_or(tp_core::i18n::tr("记录不存在"))?;
    if !matches!(kind.as_str(), "file" | "image" | "video") {
        return Err(tp_core::i18n::tr("该记录不支持文件操作").into());
    }
    let files: Vec<FileItem> = serde_json::from_str(&files_json).map_err(err)?;
    if kind == "image" && files.is_empty() {
        let path = store.dir.join("assets").join(id).join("image.png");
        return path
            .is_file()
            .then_some(vec![path])
            .ok_or(tp_core::i18n::tr("图片副本不可用").into());
    }
    if files.is_empty() || files.iter().any(|file| file.is_directory) {
        return Err(tp_core::i18n::tr("目录记录不支持此操作").into());
    }
    files
        .iter()
        .map(|file| {
            let original = PathBuf::from(&file.original);
            let stored = file.stored.as_deref().map(PathBuf::from);
            let path = if !stored_only && original.is_file() {
                original
            } else if let Some(path) = stored.filter(|path| path.is_file()) {
                path
            } else if stored_only {
                return Err(tp_core::i18n::tr("文件副本不可用").into());
            } else {
                return Err(tp_core::i18n::format_message!(
                    "文件不可用：{}",
                    "File unavailable: {}",
                    file.original
                ));
            };
            Ok(path)
        })
        .collect()
}

pub fn copy_record_paths(store: &Store, id: String) -> Result<()> {
    let _data = store.read_data()?;
    let paths = record_action_paths(&store, &id, false)?;
    let text = paths
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n");
    // “复制路径”是用户主动发起的复制，不做主动写回抑制：文本路径与其他复制内容一样进入历史记录。
    ClipboardContext::new()
        .map_err(err)?
        .set_text(text)
        .map_err(err)
}

pub fn open_record_directory(store: &Store, id: String) -> Result<()> {
    let _data = store.read_data()?;
    let paths = record_action_paths(&store, &id, false)?;
    platform::reveal_files(&paths_by_directory(&paths)?).map_err(err)
}

pub fn save_record_files(store: &Store, id: String) -> Result<()> {
    let _data = store.read_data()?;
    let files = record_action_paths(&store, &id, true)?;
    let Some(destination) = rfd::FileDialog::new()
        .set_title(tp_core::i18n::tr("选择保存目录"))
        .pick_folder()
    else {
        return Ok(());
    };
    save_paths_to(&files, &destination)
}

/// GPUI's Windows background executor uses WinRT threads. Let the async
/// dialog backend own its apartment instead of opening a synchronous STA
/// dialog on that executor. Do not hold the data gate while the user chooses.
pub async fn save_record_files_async(store: &Store, id: String) -> Result<()> {
    {
        let _data = store.read_data()?;
        record_action_paths(store, &id, true)?;
    }
    let Some(destination) = rfd::AsyncFileDialog::new()
        .set_title(tp_core::i18n::tr("选择保存目录"))
        .pick_folder()
        .await
    else {
        return Ok(());
    };
    save_record_files_to(store, &id, destination.path())
}

fn save_record_files_to(store: &Store, id: &str, destination: &Path) -> Result<()> {
    let _data = store.read_data()?;
    let files = record_action_paths(store, id, true)?;
    save_paths_to(&files, destination)
}

fn save_paths_to(files: &[PathBuf], destination: &Path) -> Result<()> {
    let targets = files
        .iter()
        .map(|source| {
            let name = source
                .file_name()
                .ok_or(tp_core::i18n::tr("文件名不可用"))?;
            Ok(destination.join(name))
        })
        .collect::<Result<Vec<_>>>()?;
    if targets.iter().any(|target| target.exists()) {
        return Err(tp_core::i18n::tr("目标目录中已有同名文件，未保存任何文件").into());
    }
    for (source, target) in files.iter().zip(targets) {
        fs::copy(source, target).map_err(err)?;
    }
    Ok(())
}

pub fn save_files(
    store: std::sync::Arc<Store>,
    changed: crate::Changed,
    id: String,
    kind: String,
    mut files: Vec<FileItem>,
) {
    let Ok(_data) = store.read_data() else {
        return;
    };
    // A deleted record must not leave a queued worker writing orphaned attachments.
    if !store.db.lock().is_ok_and(|conn| {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE id=?1 AND status='pending')",
            [&id],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false)
    }) {
        return;
    }
    let total = files.iter().try_fold(0u64, |sum, file| {
        file_size(Path::new(&file.original))
            .ok()
            .map(|size| sum.saturating_add(size))
    });
    let mut mode = "reference";
    let mut status = "ready";
    if total.is_some_and(|size| size <= 100 * 1024 * 1024) {
        // 整组副本平铺在同一个 files 目录下，使“打开所在目录”只需打开一个窗口即可看到整组文件。
        // 组内重名时按原文件名追加序号，避免相互覆盖。
        let dir = store.dir.join("assets").join(&id).join("files");
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(&dir)?;
            let mut taken = HashSet::<OsString>::new();
            for file in files.iter_mut() {
                let name = Path::new(&file.original)
                    .file_name()
                    .ok_or_else(|| std::io::Error::other("invalid filename"))?;
                let name = unique_file_name(name, &mut taken);
                let target = dir.join(name);
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
    if kind == "image" {
        let source = files[0].stored.as_ref().unwrap_or(&files[0].original);
        let dir = store.dir.join("assets").join(&id);
        let preview_ready = match store.media_lock.lock() {
            Ok(_media) => {
                dir.join("image.png").exists()
                    || save_image_file_preview(Path::new(source), &dir).is_ok()
            }
            Err(_) => false,
        };
        if preview_ready {
            if let Ok(conn) = store.db.lock() {
                let _ = conn.execute("UPDATE records SET preview=1 WHERE id=?1", [&id]);
            }
        }
    } else if kind == "video" {
        let source = files[0].stored.as_ref().unwrap_or(&files[0].original);
        let dir = store.dir.join("assets").join(&id);
        let frame = dir.join("frame.png");
        if platform::video_preview(Path::new(source), &frame) {
            let _ = save_list_thumbnail(&frame, &dir.join("thumbnail.png"));
            if let Ok(conn) = store.db.lock() {
                let _ = conn.execute("UPDATE records SET preview=1 WHERE id=?1", [&id]);
            }
        }
    }
    changed();
}

#[cfg(test)]
#[path = "tests/files.rs"]
mod tests;
