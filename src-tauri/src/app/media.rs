use super::{
    common::{err, Result},
    model::FileItem,
    storage::Store,
};
use crate::platform;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, GenericImageView};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};
pub(super) fn list_thumbnail(image: &DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    if u64::from(width) * 5 <= u64::from(height) * 2 {
        let top = (height - width) / 10;
        let crop = image.crop_imm(0, top, width, width);
        crop.thumbnail(width.min(320), width.min(320))
    } else if u64::from(width) >= u64::from(height) * 3 {
        let crop_width = (u64::from(height) * 9 / 4) as u32;
        image
            .crop_imm((width - crop_width) / 2, 0, crop_width, height)
            .thumbnail(crop_width.min(720), height.min(320))
    } else {
        image.thumbnail(width.min(720), height.min(320))
    }
}

pub(super) fn save_list_thumbnail(source: &Path, output: &Path) -> Result<()> {
    let image = image::open(source).map_err(err)?;
    list_thumbnail(&image).save(output).map_err(err)
}

fn hover_preview_path(source: &Path, dir: &Path, index: usize) -> Result<PathBuf> {
    let output = dir.join(if index == 0 {
        "hover.png".to_string()
    } else {
        format!("hover-{index}.png")
    });
    if !output.exists() {
        let image = image::open(source).map_err(err)?;
        image
            .thumbnail(image.width().min(1440), image.height().min(1440))
            .save(&output)
            .map_err(err)?;
    }
    Ok(output)
}

fn save_indexed_image_file_preview(source: &Path, dir: &Path, index: usize) -> Result<PathBuf> {
    let image = image::ImageReader::open(source)
        .map_err(err)?
        .with_guessed_format()
        .map_err(err)?
        .decode()
        .map_err(err)?;
    fs::create_dir_all(dir).map_err(err)?;
    let output = if index == 0 {
        dir.join("image.png")
    } else {
        dir.join(format!("image-{index}.png"))
    };
    image.save(&output).map_err(err)?;
    if index == 0 {
        list_thumbnail(&image)
            .save(dir.join("thumbnail.png"))
            .map_err(err)?;
    }
    Ok(output)
}

pub(super) fn save_image_file_preview(source: &Path, dir: &Path) -> Result<()> {
    save_indexed_image_file_preview(source, dir, 0).map(|_| ())
}

fn text_file_extension(path: &Path) -> bool {
    const EXTENSIONS: &str =
        "md markdown mdx txt text log json jsonc json5 csv tsv ts tsx js jsx mjs cjs \
        css scss less html htm xml yml yaml toml ini cfg conf properties kt kts java py rs \
        go c cpp h hpp cs swift sh bash zsh ps1 bat cmd sql rb php vue svelte svg";
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        EXTENSIONS
            .split_ascii_whitespace()
            .any(|ext| ext.eq_ignore_ascii_case(s))
    })
}

fn read_text_file_preview(path: &Path) -> Result<Option<String>> {
    if !text_file_extension(path) || !path.is_file() {
        return Ok(None);
    }
    if fs::metadata(path).map_err(err)?.len() > 1024 * 1024 {
        return Err("文件超过 1 MB，无法预览".into());
    }
    let bytes = fs::read(path).map_err(err)?;
    let text = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        String::from_utf8(bytes[3..].to_vec()).map_err(|_| "文件内容不是可预览的文本")?
    } else if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little_endian = bytes[0] == 0xff;
        if bytes.len() % 2 != 0 {
            return Err("文件内容不是可预览的文本".into());
        }
        let units = bytes[2..].chunks_exact(2).map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        });
        String::from_utf16(&units.collect::<Vec<_>>()).map_err(|_| "文件内容不是可预览的文本")?
    } else {
        String::from_utf8(bytes).map_err(|_| "文件内容不是可预览的文本")?
    };
    if text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err("文件内容不是可预览的文本".into());
    }
    Ok(Some(text))
}

fn file_text_preview_path(conn: &Connection, id: &str, index: usize) -> Result<Option<PathBuf>> {
    let (kind, files, mode): (String, String, String) = conn
        .query_row(
            "SELECT kind,files,storage_mode FROM records WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(err)?;
    if kind != "file" {
        return Ok(None);
    }
    let files: Vec<FileItem> = serde_json::from_str(&files).map_err(err)?;
    let file = files.get(index).ok_or("无效的文件序号")?;
    if !text_file_extension(Path::new(&file.original)) {
        return Ok(None);
    }
    let path = if mode == "copy" {
        file.stored.as_deref().ok_or("文件副本不可用")?
    } else {
        &file.original
    };
    Ok(Some(PathBuf::from(path)))
}

#[tauri::command]
pub(super) fn file_text_preview(
    store: tauri::State<Store>,
    id: String,
    index: usize,
) -> Result<Option<String>> {
    let _data = store.read_data()?;
    let path = {
        let conn = store.db.lock().map_err(err)?;
        file_text_preview_path(&conn, &id, index)?
    };
    match path {
        Some(path) => read_text_file_preview(&path),
        None => Ok(None),
    }
}

#[tauri::command(async)]
pub(super) fn preview_data(
    store: tauri::State<Store>,
    id: String,
    role: Option<String>,
    index: Option<usize>,
) -> Result<String> {
    let _data = store.read_data()?;
    let (kind, files): (String, String) = {
        let conn = store.db.lock().map_err(err)?;
        conn.query_row("SELECT kind,files FROM records WHERE id=?1", [&id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(err)?
    };
    if kind != "image" && kind != "video" {
        return Err("此记录没有图片预览".into());
    }
    let role = role.as_deref().unwrap_or("thumbnail");
    if !matches!(role, "thumbnail" | "preview" | "full") {
        return Err("无效的预览类型".into());
    }
    let files: Vec<FileItem> = serde_json::from_str(&files).map_err(err)?;
    let index = index.unwrap_or(0);
    if (files.is_empty() && index != 0) || (!files.is_empty() && index >= files.len()) {
        return Err("无效的预览序号".into());
    }
    if role == "thumbnail" && index != 0 {
        return Err("缩略图只用于首个文件".into());
    }
    let path = {
        let _media = store.media_lock.lock().map_err(err)?;
        let dir = store.dir.join("assets").join(id);
        let thumbnail = dir.join("thumbnail.png");
        let legacy = dir.join("preview.png");
        let source = files.get(index).map(|file| {
            file.stored
                .as_deref()
                .filter(|path| Path::new(path).exists())
                .unwrap_or(&file.original)
        });
        if kind == "image" {
            let original = if index == 0 {
                dir.join("image.png")
            } else {
                dir.join(format!("image-{index}.png"))
            };
            if !original.exists() {
                if let Some(source) = source {
                    save_indexed_image_file_preview(Path::new(source), &dir, index)?;
                }
            }
            if role == "thumbnail" && !thumbnail.exists() {
                save_list_thumbnail(&original, &thumbnail)?;
                let _ = fs::remove_file(&legacy);
            }
            if role == "thumbnail" {
                thumbnail
            } else if role == "preview" {
                hover_preview_path(&original, &dir, index)?
            } else {
                original
            }
        } else {
            let frame = if index == 0 {
                dir.join("frame.png")
            } else {
                dir.join(format!("frame-{index}.png"))
            };
            if !frame.exists() {
                if let Some(source) = source.filter(|path| Path::new(path).exists()) {
                    let _ = platform::video_preview(Path::new(source), &frame);
                }
            }
            if !thumbnail.exists() {
                if index == 0 {
                    let source = if frame.exists() { &frame } else { &legacy };
                    if source.exists() {
                        save_list_thumbnail(source, &thumbnail)?;
                        if frame.exists() {
                            let _ = fs::remove_file(&legacy);
                        }
                    }
                }
            }
            if role == "thumbnail" {
                thumbnail
            } else if frame.exists() {
                frame
            } else if index == 0 {
                legacy
            } else {
                frame
            }
        }
    };
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(fs::read(path).map_err(err)?)
    ))
}

#[cfg(test)]
#[path = "tests/media.rs"]
mod tests;
