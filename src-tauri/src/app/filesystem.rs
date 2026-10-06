use std::{collections::HashSet, ffi::OsString, fs, path::Path};
fn video(path: &str) -> bool {
    let path = Path::new(path);
    path.is_file()
        && path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "wmv"
            )
        })
}

fn image_file(path: &str) -> bool {
    let path = Path::new(path);
    path.is_file()
        && path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "ico"
            )
        })
}

pub(super) fn file_kind(paths: &[String]) -> &'static str {
    if paths.iter().all(|path| video(path)) {
        "video"
    } else if paths.iter().all(|path| image_file(path)) {
        "image"
    } else {
        "file"
    }
}

pub(super) fn file_size(path: &Path) -> std::io::Result<u64> {
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

/// 组内重名时按 `<主名> (n)<扩展名>` 追加序号，返回本次使用的不重名文件名。
pub(super) fn unique_file_name(name: &std::ffi::OsStr, taken: &mut HashSet<OsString>) -> OsString {
    if taken.insert(name.to_os_string()) {
        return name.to_os_string();
    }
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or(name);
    let extension = path.extension();
    for index in 2.. {
        let mut candidate = stem.to_os_string();
        candidate.push(format!(" ({index})"));
        if let Some(extension) = extension {
            candidate.push(".");
            candidate.push(extension);
        }
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("序号递增必然产生可用文件名")
}

pub(super) fn copy_path(from: &Path, to: &Path) -> std::io::Result<()> {
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

#[cfg(test)]
#[path = "tests/filesystem.rs"]
mod tests;
