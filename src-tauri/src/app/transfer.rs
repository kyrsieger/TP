use super::{
    common::{err, now, Result},
    events::changed,
    model::{FileItem, Settings},
    storage::{settings, Store},
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const VERSION: u32 = 1;
const MAX_MANIFEST: u64 = 64 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavedRecord {
    id: String,
    digest: String,
    kind: String,
    body: String,
    title: String,
    files: Vec<FileItem>,
    favorite: bool,
    pinned_at: Option<i64>,
    group_id: Option<i64>,
    used_at: i64,
    created_at: Option<i64>,
    storage_mode: String,
    status: String,
    preview: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedGroup {
    id: i64,
    name: String,
    position: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Asset {
    path: String,
    directory: bool,
    size: u64,
    sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    version: u32,
    exported_at: i64,
    records: Vec<SavedRecord>,
    groups: Vec<SavedGroup>,
    assets: Vec<Asset>,
    warnings: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct TransferResult {
    path: String,
    records: usize,
    excluded_sensitive: usize,
    warnings: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct RestorePreview {
    token: String,
    records: usize,
    groups: usize,
    cleanup_count: usize,
    current_max_ordinary: u32,
    next_max_ordinary: u32,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum RestoreResult {
    ConfirmationRequired { preview: RestorePreview },
    Completed { records: usize, cleaned: usize },
}

struct Workspace {
    path: PathBuf,
    persistent: bool,
}
impl Workspace {
    fn new(dir: &Path) -> Result<Self> {
        let parent = dir.join("transfers");
        fs::create_dir_all(&parent).map_err(err)?;
        let path = parent.join(Uuid::new_v4().to_string());
        fs::create_dir(&path).map_err(err)?;
        Ok(Self {
            path,
            persistent: false,
        })
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if !self.persistent {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
struct Prepared {
    workspace: Workspace,
    manifest: Manifest,
    token: String,
}
#[derive(Default)]
pub(super) struct TransferSessions(Mutex<Option<Prepared>>);

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', ':', '\0'])
        && !value.chars().any(char::is_control)
        && !value.ends_with(['.', ' '])
        && !matches!(
            value
                .split('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
        && !value.contains(['<', '>', '"', '|', '?', '*'])
}

fn safe_asset_path(value: &str, ids: &HashSet<String>) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() >= 2
        && parts[0] == "assets"
        && ids.contains(parts[1])
        && parts.iter().all(|part| safe_component(part))
}

fn portable_path(path: &Path) -> Result<String> {
    path.components()
        .map(|part| match part {
            Component::Normal(name) => name
                .to_str()
                .filter(|name| safe_component(name))
                .map(str::to_string)
                .ok_or_else(|| "附件名称无法在 Windows 和 macOS 间迁移".to_string()),
            _ => Err("附件路径无效".into()),
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn ensure_settled(conn: &Connection) -> Result<()> {
    let pending: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE status='pending')",
            [],
            |r| r.get(0),
        )
        .map_err(err)?;
    if pending {
        return Err("仍有文件副本待保存，请稍后重试".into());
    }
    Ok(())
}

fn collect_manifest(conn: &Connection, dir: &Path) -> Result<(Manifest, usize)> {
    ensure_settled(conn)?;
    let excluded: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM records WHERE expires_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .map_err(err)?;
    let mut stmt = conn.prepare("SELECT id,digest,kind,body,title,files,favorite,pinned_at,group_id,used_at,created_at,storage_mode,status,preview FROM records WHERE expires_at IS NULL ORDER BY pinned_at IS NULL,pinned_at DESC,used_at DESC,id").map_err(err)?;
    let mut records = stmt
        .query_map([], |r| {
            let files: String = r.get(5)?;
            Ok(SavedRecord {
                id: r.get(0)?,
                digest: r.get(1)?,
                kind: r.get(2)?,
                body: r.get(3)?,
                title: r.get(4)?,
                files: serde_json::from_str(&files).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                favorite: r.get(6)?,
                pinned_at: r.get(7)?,
                group_id: r.get(8)?,
                used_at: r.get(9)?,
                created_at: r.get(10)?,
                storage_mode: r.get(11)?,
                status: r.get(12)?,
                preview: r.get(13)?,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    let mut stmt = conn
        .prepare("SELECT id,name,position FROM groups ORDER BY position,id")
        .map_err(err)?;
    let groups = stmt
        .query_map([], |r| {
            Ok(SavedGroup {
                id: r.get(0)?,
                name: r.get(1)?,
                position: r.get(2)?,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    let mut warnings = Vec::new();
    for record in &mut records {
        if !safe_component(&record.id) {
            return Err("记录标识无效，无法导出".into());
        }
        for file in &mut record.files {
            file.stored = match file.stored.as_deref() {
                Some(stored) => {
                    let relative = Path::new(stored)
                        .strip_prefix(dir)
                        .map_err(|_| "文件副本位于应用数据目录之外".to_string())?;
                    let relative = portable_path(relative)?;
                    if !relative.starts_with(&format!("assets/{}/", record.id)) {
                        return Err("文件副本不属于该记录".into());
                    }
                    if !Path::new(stored).exists() {
                        warnings.push(format!("{}：文件已不可用（{}）", record.id, file.original));
                    }
                    Some(relative)
                }
                None => None,
            };
            if record.storage_mode != "copy" || file.stored.is_none() {
                warnings.push(format!(
                    "{}：仅包含原路径，未包含文件本体（{}）",
                    record.id, file.original
                ));
            }
        }
        if record.kind == "image"
            && record.files.is_empty()
            && !dir
                .join("assets")
                .join(&record.id)
                .join("image.png")
                .is_file()
        {
            warnings.push(format!("{}：图片副本不可用", record.id));
        }
    }
    Ok((
        Manifest {
            version: VERSION,
            exported_at: now(),
            records,
            groups,
            assets: Vec::new(),
            warnings,
        },
        excluded,
    ))
}

fn zip_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true)
}

fn add_asset(
    writer: &mut ZipWriter<File>,
    root: &Path,
    path: &Path,
    manifest: &mut Manifest,
) -> Result<()> {
    let relative = portable_path(path.strip_prefix(root).map_err(err)?)?;
    let metadata = fs::symlink_metadata(path).map_err(err)?;
    // Never follow a link or Windows junction into user-owned files.
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if metadata.file_type().is_symlink() || reparse {
        manifest
            .warnings
            .push(format!("{relative}：跳过链接，未读取链接指向的内容"));
        return Ok(());
    }
    if metadata.is_dir() {
        writer
            .add_directory(format!("{relative}/"), zip_options())
            .map_err(err)?;
        manifest.assets.push(Asset {
            path: relative,
            directory: true,
            size: 0,
            sha256: String::new(),
        });
        let mut children = fs::read_dir(path)
            .map_err(err)?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(err)?;
        children.sort_by_key(|child| child.file_name());
        for child in children {
            add_asset(writer, root, &child.path(), manifest)?;
        }
    } else if metadata.is_file() {
        writer.start_file(&relative, zip_options()).map_err(err)?;
        let mut source = File::open(path).map_err(err)?;
        let mut hash = Sha256::new();
        let mut size = 0;
        let mut buffer = [0; 65536];
        loop {
            let count = source.read(&mut buffer).map_err(err)?;
            if count == 0 {
                break;
            }
            writer.write_all(&buffer[..count]).map_err(err)?;
            hash.update(&buffer[..count]);
            size += count as u64;
        }
        manifest.assets.push(Asset {
            path: relative,
            directory: false,
            size,
            sha256: format!("{:x}", hash.finalize()),
        });
    } else {
        manifest
            .warnings
            .push(format!("{relative}：附件类型不支持"));
    }
    Ok(())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn href(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"/-_.".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn display_time(value: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(value) * 1_000_000)
        .ok()
        .and_then(|date| {
            date.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| "未知".into())
}
fn filename_time(date: chrono::DateTime<chrono::FixedOffset>) -> String {
    date.format("%Y%m%d-%H%M%S").to_string()
}
fn content_html(manifest: &Manifest) -> String {
    let mut html = String::from("<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\"><title>TP 导出内容</title><style>body{font:15px/1.6 system-ui;max-width:900px;margin:32px auto;padding:0 20px;color:#222;background:#f7f7f8}article{padding:20px;margin:16px 0;border:1px solid #ddd;border-radius:12px;background:white}pre{white-space:pre-wrap;overflow-wrap:anywhere}img{max-width:100%;max-height:420px}small{color:#666}a{overflow-wrap:anywhere}li{margin:6px 0}</style><h1>TP 导出内容</h1>");
    html.push_str(&format!("<p>共 {} 条记录。完整原文与整理信息见 <a href=\"records.json\">JSON 数据</a>。此导出包未加密，不包含应用设置和敏感到期标记记录。</p>", manifest.records.len()));
    if !manifest.warnings.is_empty() {
        html.push_str("<details open><summary>文件可用性说明</summary><ul>");
        for warning in &manifest.warnings {
            html.push_str(&format!("<li>{}</li>", escape_html(warning)));
        }
        html.push_str("</ul></details>");
    }
    for record in &manifest.records {
        let group = record
            .group_id
            .and_then(|id| manifest.groups.iter().find(|g| g.id == id))
            .map(|g| g.name.as_str())
            .unwrap_or("未分组");
        let kind = match record.kind.as_str() {
            "text" => "文字",
            "link" => "链接",
            "color" => "颜色",
            "image" => "图片",
            "video" => "视频",
            _ => "文件",
        };
        let heading = if record.title.is_empty() {
            record.body.trim().chars().take(80).collect::<String>()
        } else {
            record.title.clone()
        };
        html.push_str(&format!(
            "<article><h2>{}</h2><small>{kind} · {}{} · 最后使用时间 {} · 创建时间 {}</small>",
            escape_html(if heading.is_empty() { kind } else { &heading }),
            escape_html(group),
            if record.favorite { " · 收藏" } else { "" },
            display_time(record.used_at),
            record
                .created_at
                .map(display_time)
                .unwrap_or_else(|| "未知".into())
        ));
        if !record.body.is_empty() {
            html.push_str(&format!("<pre>{}</pre>", escape_html(&record.body)));
        }
        let image = format!(
            "assets/{}/{}",
            record.id,
            if record.kind == "video" {
                "frame.png"
            } else {
                "image.png"
            }
        );
        if matches!(record.kind.as_str(), "image" | "video")
            && manifest.assets.iter().any(|a| a.path == image)
        {
            html.push_str(&format!(
                "<img src=\"{}\" alt=\"{}\">",
                href(&image),
                if record.kind == "video" {
                    "视频首帧预览，不能代替视频本体"
                } else {
                    "图片"
                }
            ));
        }
        html.push_str("<ul>");
        for file in &record.files {
            let present = file
                .stored
                .as_deref()
                .filter(|path| manifest.assets.iter().any(|a| a.path == *path));
            let name = escape_html(&file.original);
            if let Some(path) = present {
                html.push_str(&format!(
                    "<li><a href=\"{}\">{name}</a></li>",
                    href(&format!(
                        "{path}{}",
                        if file.is_directory { "/" } else { "" }
                    ))
                ));
            } else {
                html.push_str(&format!("<li>{name} — 文件本体未包含，仅保留原路径</li>"));
            }
        }
        html.push_str("</ul></article>");
    }
    html.push_str("</html>");
    html
}

fn write_archive(conn: &Connection, dir: &Path, destination: &Path) -> Result<TransferResult> {
    let (mut manifest, excluded) = collect_manifest(conn, dir)?;
    let temp = destination.with_file_name(format!(".tp-export-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(err)?;
        let mut writer = ZipWriter::new(file);
        let ids: Vec<_> = manifest.records.iter().map(|r| r.id.clone()).collect();
        for id in ids {
            let path = dir.join("assets").join(id);
            if path.exists() {
                add_asset(&mut writer, dir, &path, &mut manifest)?;
            }
        }
        writer
            .start_file("index.html", zip_options())
            .map_err(err)?;
        writer
            .write_all(content_html(&manifest).as_bytes())
            .map_err(err)?;
        writer
            .start_file("records.json", zip_options())
            .map_err(err)?;
        let metadata = serde_json::to_vec_pretty(&manifest).map_err(err)?;
        validate_manifest(&manifest)?;
        if metadata.len() as u64 > MAX_MANIFEST {
            return Err("记录清单超过 64 MiB，未生成导出包".into());
        }
        writer.write_all(&metadata).map_err(err)?;
        writer.finish().map_err(err)?.sync_all().map_err(err)?;
        fs::rename(&temp, destination).map_err(err)?;
        Ok(TransferResult {
            path: destination.to_string_lossy().into(),
            records: manifest.records.len(),
            excluded_sensitive: excluded,
            warnings: manifest.warnings,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.version != VERSION {
        return Err("不支持此 TP 导出包的版本".into());
    }
    let mut ids = HashSet::new();
    let mut filesystem_ids = HashSet::new();
    let mut digests = HashSet::new();
    let mut groups = HashSet::new();
    let mut names = HashSet::new();
    for group in &manifest.groups {
        if group.id <= 0
            || !groups.insert(group.id)
            || group.name.trim().is_empty()
            || group.name.chars().count() > 40
            || !names.insert(&group.name)
        {
            return Err("导出包分组信息无效".into());
        }
    }
    for record in &manifest.records {
        if !safe_component(&record.id)
            || !record
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            || !filesystem_ids.insert(record.id.to_ascii_lowercase())
            || !ids.insert(record.id.clone())
            || !digests.insert(&record.digest)
            || record.digest.len() != 64
            || !record.digest.bytes().all(|b| b.is_ascii_hexdigit())
            || !matches!(
                record.kind.as_str(),
                "text" | "link" | "color" | "image" | "video" | "file"
            )
            || !matches!(record.storage_mode.as_str(), "" | "copy" | "reference")
            || !matches!(record.status.as_str(), "ready" | "missing" | "copy_failed")
            || record.group_id.is_some_and(|id| !groups.contains(&id))
            || record.title.chars().count() > 100
            || record.title.contains(['\r', '\n'])
        {
            return Err("导出包记录信息无效".into());
        }
    }
    let mut paths = HashSet::new();
    for asset in &manifest.assets {
        if !safe_asset_path(&asset.path, &ids)
            || !paths.insert(asset.path.to_lowercase())
            || (!asset.directory
                && (asset.sha256.len() != 64
                    || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit())))
            || (asset.directory && (asset.size != 0 || !asset.sha256.is_empty()))
        {
            return Err("导出包附件路径或校验信息无效".into());
        }
    }
    for record in &manifest.records {
        for file in &record.files {
            if file.stored.as_deref().is_some_and(|path| {
                !safe_asset_path(path, &ids) || !path.starts_with(&format!("assets/{}/", record.id))
            }) {
                return Err("导出包文件副本路径无效".into());
            }
        }
    }
    Ok(())
}

fn prepare_archive(dir: &Path, source: &Path) -> Result<Prepared> {
    let mut archive = ZipArchive::new(File::open(source).map_err(err)?).map_err(err)?;
    archive.validate_header_consistency().map_err(err)?;
    let manifest = {
        let mut entry = archive
            .by_name("records.json")
            .map_err(|_| "导出包缺少 records.json，无法导入".to_string())?;
        if entry.size() > MAX_MANIFEST {
            return Err("记录清单过大".into());
        }
        let mut data = Vec::new();
        (&mut entry)
            .take(MAX_MANIFEST + 1)
            .read_to_end(&mut data)
            .map_err(err)?;
        if data.len() as u64 > MAX_MANIFEST {
            return Err("记录清单过大".into());
        }
        serde_json::from_slice::<Manifest>(&data).map_err(|e| format!("记录清单无效：{e}"))?
    };
    validate_manifest(&manifest)?;
    if archive.len() != manifest.assets.len() + 2 || archive.by_name("index.html").is_err() {
        return Err("导出包缺少浏览目录或包含清单之外的文件".into());
    }
    let workspace = Workspace::new(dir)?;
    fs::create_dir(workspace.path.join("assets")).map_err(err)?;
    let mut seen = HashSet::new();
    let expected: HashMap<_, _> = manifest
        .assets
        .iter()
        .map(|asset| (asset.path.as_str(), asset))
        .collect();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(err)?;
        let entry_name = entry.name().map_err(err)?;
        if !seen.insert(entry_name.to_lowercase()) {
            return Err("导出包包含重复路径".into());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
            || entry.enclosed_name().is_none()
        {
            return Err("导出包包含不安全路径或链接".into());
        }
        if matches!(entry_name.as_ref(), "records.json" | "index.html") {
            continue;
        }
        let name = entry_name.trim_end_matches('/');
        let asset = expected.get(name).ok_or("导出包附件未列入清单")?;
        let expected_name = if asset.directory {
            format!("{}/", asset.path)
        } else {
            asset.path.clone()
        };
        if entry_name != expected_name {
            return Err("导出包附件名称不规范".into());
        }
        if entry.is_dir() != asset.directory || entry.size() != asset.size {
            return Err("导出包附件大小或类型不匹配".into());
        }
        let target = workspace.path.join(&asset.path);
        if asset.directory {
            fs::create_dir_all(&target).map_err(err)?;
            continue;
        }
        fs::create_dir_all(target.parent().ok_or("附件路径无效")?).map_err(err)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(err)?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0; 65536];
        loop {
            let size = entry.read(&mut buffer).map_err(err)?;
            if size == 0 {
                break;
            }
            count = count.checked_add(size as u64).ok_or("附件过大")?;
            if count > asset.size {
                return Err("附件解压大小超过清单".into());
            }
            hash.update(&buffer[..size]);
            output.write_all(&buffer[..size]).map_err(err)?;
        }
        if count != asset.size || format!("{:x}", hash.finalize()) != asset.sha256 {
            return Err("导出包附件校验失败，未恢复任何记录".into());
        }
        output.sync_all().map_err(err)?;
    }
    Ok(Prepared {
        workspace,
        manifest,
        token: Uuid::new_v4().to_string(),
    })
}

fn restore_preview(
    prepared: &Prepared,
    config: &Settings,
    timestamp: i64,
) -> Result<RestorePreview> {
    let ordinary = prepared
        .manifest
        .records
        .iter()
        .filter(|r| !r.favorite && r.group_id.is_none())
        .count();
    let ordinary = u32::try_from(ordinary).map_err(|_| "导出包记录数量过多")?;
    let next = if ordinary > config.max_ordinary {
        ordinary.checked_add(100).ok_or("导出包记录数量过多")?
    } else {
        config.max_ordinary
    };
    let cutoff = timestamp - i64::from(config.retention_days) * 86_400_000;
    let count = prepared
        .manifest
        .records
        .iter()
        .filter(|r| !r.favorite && r.group_id.is_none() && r.used_at < cutoff)
        .count();
    Ok(RestorePreview {
        token: prepared.token.clone(),
        records: prepared.manifest.records.len(),
        groups: prepared.manifest.groups.len(),
        cleanup_count: count,
        current_max_ordinary: config.max_ordinary,
        next_max_ordinary: next,
        warnings: prepared.manifest.warnings.clone(),
    })
}

#[derive(Serialize, Deserialize)]
struct Journal {
    stage: String,
}

/// Runs before opening the application's SQLite connection. The commit marker is
/// written in the same transaction as the replacement records and settings.
pub(super) fn recover_interrupted_restore(dir: &Path) -> Result<()> {
    recover_restore_journal(dir)?;
    // Only sweep after recovery succeeds: its workspace may hold the sole
    // remaining copy of the previous database and attachments.
    if let Err(error) = cleanup_transfer_workspaces(dir) {
        eprintln!("Failed to clean up transfer workspaces: {error}");
    }
    Ok(())
}

fn cleanup_transfer_workspaces(dir: &Path) -> Result<()> {
    let entries = match fs::read_dir(dir.join("transfers")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(err(error)),
    };
    for entry in entries {
        let entry = entry.map_err(err)?;
        if entry.file_type().map_err(err)?.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| Uuid::parse_str(name).is_ok())
        {
            fs::remove_dir_all(entry.path()).map_err(err)?;
        }
    }
    Ok(())
}

fn recover_restore_journal(dir: &Path) -> Result<()> {
    let journal_path = dir.join("restore-journal.json");
    if !journal_path.exists() {
        return Ok(());
    }
    let journal: Journal =
        serde_json::from_slice(&fs::read(&journal_path).map_err(err)?).map_err(err)?;
    if Uuid::parse_str(&journal.stage).is_err() {
        return Err("恢复日志路径无效".into());
    }
    let stage = dir.join("transfers").join(&journal.stage);
    let committed = {
        let conn = Connection::open(dir.join("history.sqlite3")).map_err(err)?;
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='transfer_commits')", [], |r| r.get(0)).map_err(err)?;
        exists
            && conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM transfer_commits WHERE id=?1)",
                    [&journal.stage],
                    |r| r.get(0),
                )
                .map_err(err)?
    };
    if !committed {
        if stage.join("previous-assets").exists() {
            if dir.join("assets").exists() {
                fs::remove_dir_all(dir.join("assets")).map_err(err)?;
            }
            fs::rename(stage.join("previous-assets"), dir.join("assets")).map_err(err)?;
        }
        if !stage.join("previous.sqlite3").is_file() {
            return Err("恢复中断，原数据库副本不存在；请保留数据目录以便排查".into());
        }
        for suffix in ["-wal", "-shm"] {
            let path = dir.join(format!("history.sqlite3{suffix}"));
            if path.exists() {
                fs::remove_file(path).map_err(err)?;
            }
        }
        let rollback = dir.join("history-rollback.tmp");
        fs::copy(stage.join("previous.sqlite3"), &rollback).map_err(err)?;
        OpenOptions::new()
            .write(true)
            .open(&rollback)
            .map_err(err)?
            .sync_all()
            .map_err(err)?;
        fs::rename(rollback, dir.join("history.sqlite3")).map_err(err)?;
    }
    fs::remove_file(journal_path).map_err(err)?;
    let _ = fs::remove_dir_all(stage);
    Ok(())
}

fn replace_data(
    store: &Store,
    conn: &mut Connection,
    prepared: &mut Prepared,
    keep_expired: bool,
    preview: &RestorePreview,
    timestamp: i64,
) -> Result<RestoreResult> {
    ensure_settled(conn)?;
    let stage = &prepared.workspace.path;
    let cutoff = timestamp - i64::from(settings(conn)?.retention_days) * 86_400_000;
    conn.execute(
        "VACUUM INTO ?1",
        [stage.join("previous.sqlite3").to_string_lossy().as_ref()],
    )
    .map_err(err)?;
    let stage_id = stage
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("恢复目录无效")?
        .to_string();
    let journal_path = store.dir.join("restore-journal.json");
    if journal_path.exists() {
        return Err("上次恢复尚未完成，请重启 TP 后重试".into());
    }
    let mut journal = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("journal.json"))
        .map_err(err)?;
    journal
        .write_all(
            &serde_json::to_vec(&Journal {
                stage: stage_id.clone(),
            })
            .map_err(err)?,
        )
        .map_err(err)?;
    journal.sync_all().map_err(err)?;
    drop(journal);
    fs::rename(stage.join("journal.json"), &journal_path).map_err(err)?;
    prepared.workspace.persistent = true;
    let assets = store.dir.join("assets");
    let previous_assets = stage.join("previous-assets");
    let mut moved_old = false;
    let mut moved_new = false;
    let result = (|| {
        fs::rename(&assets, &previous_assets).map_err(err)?;
        moved_old = true;
        fs::rename(stage.join("assets"), &assets).map_err(err)?;
        moved_new = true;
        let tx = conn.transaction().map_err(err)?;
        tx.execute_batch("DELETE FROM records; DELETE FROM groups; CREATE TABLE IF NOT EXISTS transfer_commits(id TEXT PRIMARY KEY);").map_err(err)?;
        for group in &prepared.manifest.groups {
            tx.execute(
                "INSERT INTO groups(id,name,position) VALUES(?1,?2,?3)",
                params![group.id, group.name, group.position],
            )
            .map_err(err)?;
        }
        let mut restored = 0;
        for record in &prepared.manifest.records {
            let expired = !record.favorite && record.group_id.is_none() && record.used_at < cutoff;
            if expired && !keep_expired {
                continue;
            }
            let mut files = record.files.clone();
            for file in &mut files {
                if let Some(relative) = &file.stored {
                    file.stored = Some(store.dir.join(relative).to_string_lossy().into_owned());
                }
            }
            tx.execute("INSERT INTO records(id,digest,kind,body,title,files,favorite,pinned_at,group_id,used_at,created_at,expires_at,storage_mode,status,preview) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL,?12,?13,?14)",
                params![record.id, record.digest, record.kind, record.body, record.title, serde_json::to_string(&files).map_err(err)?, record.favorite, record.pinned_at, record.group_id,
                    if expired { timestamp } else { record.used_at }, record.created_at, record.storage_mode, record.status, record.preview]).map_err(err)?;
            restored += 1;
        }
        let mut config = settings(&tx)?;
        config.max_ordinary = preview.next_max_ordinary;
        tx.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&config).map_err(err)?],
        )
        .map_err(err)?;
        tx.execute("INSERT INTO transfer_commits(id) VALUES(?1)", [&stage_id])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(RestoreResult::Completed {
            records: restored,
            cleaned: prepared.manifest.records.len() - restored,
        })
    })();
    if result.is_err() {
        let rollback = (|| -> Result<()> {
            if moved_new {
                fs::rename(&assets, stage.join("assets")).map_err(err)?;
            }
            if moved_old {
                fs::rename(&previous_assets, &assets).map_err(err)?;
            }
            fs::remove_file(&journal_path).map_err(err)?;
            Ok(())
        })();
        if let Err(error) = rollback {
            store
                .recovery_required
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Err(format!(
                "恢复失败且附件回退未完成，请重启 TP 自动恢复：{error}"
            ));
        }
        prepared.workspace.persistent = false;
    } else {
        if !keep_expired {
            for record in &prepared.manifest.records {
                if !record.favorite && record.group_id.is_none() && record.used_at < cutoff {
                    let _ = fs::remove_dir_all(assets.join(&record.id));
                }
            }
        }
        // Leaving a committed journal is safe: startup only finishes removing old data.
        if fs::remove_file(&journal_path).is_ok() {
            prepared.workspace.persistent = false;
        }
    }
    result
}

#[tauri::command(async)]
pub(super) fn export_data(store: tauri::State<Store>) -> Result<Option<TransferResult>> {
    let name = format!(
        "tp-export-{}.zip",
        filename_time(chrono::Local::now().fixed_offset())
    );
    let Some(destination) = rfd::FileDialog::new()
        .set_title("导出（ZIP 未加密）")
        .add_filter("ZIP", &["zip"])
        .set_file_name(name)
        .save_file()
    else {
        return Ok(None);
    };
    let parent = destination
        .parent()
        .ok_or("保存路径无效")?
        .canonicalize()
        .map_err(err)?;
    if parent.starts_with(store.dir.canonicalize().map_err(err)?) {
        return Err("请选择应用数据目录之外的位置保存".into());
    }
    let _data = store.write_data()?;
    let conn = store.db.lock().map_err(err)?;
    write_archive(&conn, &store.dir, &destination).map(Some)
}

#[tauri::command(async)]
pub(super) fn inspect_backup(
    store: tauri::State<Store>,
    sessions: tauri::State<TransferSessions>,
) -> Result<Option<RestorePreview>> {
    let Some(source) = rfd::FileDialog::new()
        .set_title("选择 TP 导出包")
        .add_filter("TP 导出 ZIP", &["zip"])
        .pick_file()
    else {
        return Ok(None);
    };
    let prepared = prepare_archive(&store.dir, &source)?;
    let preview = {
        let _data = store.read_data()?;
        restore_preview(
            &prepared,
            &settings(&*store.db.lock().map_err(err)?)?,
            now(),
        )?
    };
    *sessions.0.lock().map_err(err)? = Some(prepared);
    Ok(Some(preview))
}

#[tauri::command(async)]
pub(super) fn restore_backup(
    app: tauri::AppHandle,
    store: tauri::State<Store>,
    sessions: tauri::State<TransferSessions>,
    token: String,
    keep_expired: bool,
    expected_cleanup_count: usize,
    expected_max_ordinary: u32,
) -> Result<RestoreResult> {
    let mut session = sessions.0.lock().map_err(err)?;
    let prepared = session
        .as_mut()
        .filter(|p| p.token == token)
        .ok_or("导入预览已失效，请重新选择备份")?;
    let _data = store.write_data()?;
    let mut conn = store.db.lock().map_err(err)?;
    let timestamp = now();
    let preview = restore_preview(prepared, &settings(&conn)?, timestamp)?;
    if preview.cleanup_count > expected_cleanup_count
        || preview.next_max_ordinary != expected_max_ordinary
    {
        return Ok(RestoreResult::ConfirmationRequired { preview });
    }
    let result = replace_data(
        &store,
        &mut conn,
        prepared,
        keep_expired,
        &preview,
        timestamp,
    );
    session.take();
    drop(conn);
    drop(_data);
    if result.is_ok() {
        changed(&app);
    }
    result
}

#[tauri::command]
pub(super) fn discard_backup(
    sessions: tauri::State<TransferSessions>,
    token: String,
) -> Result<()> {
    let mut session = sessions.0.lock().map_err(err)?;
    if session.as_ref().is_some_and(|p| p.token == token) {
        session.take();
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/transfer.rs"]
mod tests;
