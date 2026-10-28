use super::{
    common::{err, Result},
    filesystem::copy_path,
};
use rusqlite::{params, Connection, OpenFlags};
use std::{fs, path::Path};

/// Copy into a private staging directory before publishing. Never modify the legacy data.
pub(super) fn migrate_data_directory(source: &Path, target: &Path) -> Result<()> {
    if target.join("history.sqlite3").exists() || !source.join("history.sqlite3").exists() {
        return Ok(());
    }
    if target.exists() && fs::read_dir(target).map_err(err)?.next().is_some() {
        return Err("新数据目录非空，停止迁移以避免覆盖已有文件".into());
    }
    let parent = target.parent().ok_or("数据目录没有父目录")?;
    fs::create_dir_all(parent).map_err(err)?;
    let staging = parent.join(format!(".TP-migration-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&staging).map_err(err)?;
    let result = (|| {
        // VACUUM INTO includes committed WAL contents and gives us a consistent database.
        let old = Connection::open_with_flags(
            source.join("history.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(err)?;
        old.execute(
            "VACUUM INTO ?1",
            [staging.join("history.sqlite3").to_string_lossy().as_ref()],
        )
        .map_err(err)?;
        drop(old);
        if source.join("assets").exists() {
            copy_path(&source.join("assets"), &staging.join("assets")).map_err(err)?;
        }
        let mut conn = Connection::open(staging.join("history.sqlite3")).map_err(err)?;
        let tx = conn.transaction().map_err(err)?;
        let rows = {
            let mut stmt = tx.prepare("SELECT id,files FROM records").map_err(err)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(err)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err)?
        };
        for (id, json) in rows {
            // Preserve unknown fields and original paths (which contribute to record identity).
            let mut files: Vec<serde_json::Value> = serde_json::from_str(&json).map_err(err)?;
            let mut changed = false;
            for file in &mut files {
                if let Some(path) = file.get("stored").and_then(|v| v.as_str()) {
                    if let Ok(relative) = Path::new(path).strip_prefix(source.join("assets")) {
                        if relative
                            .components()
                            .any(|c| !matches!(c, std::path::Component::Normal(_)))
                        {
                            return Err("附件路径包含非法相对路径".into());
                        }
                        file["stored"] = target
                            .join("assets")
                            .join(relative)
                            .to_string_lossy()
                            .into_owned()
                            .into();
                        changed = true;
                    }
                }
            }
            if changed {
                tx.execute(
                    "UPDATE records SET files=?1 WHERE id=?2",
                    params![serde_json::to_string(&files).map_err(err)?, id],
                )
                .map_err(err)?;
            }
        }
        tx.commit().map_err(err)?;
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(err)?;
        if integrity != "ok" {
            return Err("迁移后的数据库完整性检查失败".into());
        }
        drop(conn);
        if target.exists() {
            fs::remove_dir(target).map_err(err)?;
        }
        fs::rename(&staging, target).map_err(err)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(test)]
#[path = "tests/migration.rs"]
mod tests;
