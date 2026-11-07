//! Isolated file hover fixture; refuses to overwrite an existing directory.
use std::path::PathBuf;
use tp_core::{common::now, records::FileItem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args_os().nth(1).ok_or("expected new directory")?);
    if dir.exists() {
        return Err("fixture directory already exists".into());
    }
    let store = tp_desktop::Store::open(dir)?;
    let path = store.dir.join("中文文件-选择测试.txt");
    std::fs::write(&path, "第一行：中文与😀\r\n第二行：保留原文\r\n")?;
    let files = vec![FileItem {
        original: path.to_string_lossy().into_owned(),
        stored: None,
        is_directory: false,
    }];
    store.db.lock().unwrap().execute(
        "INSERT INTO records(id,digest,kind,title,files,used_at,created_at,storage_mode) VALUES('file-hover','file-hover','file','文件文本预览',?1,?2,?2,'reference')",
        rusqlite::params![serde_json::to_string(&files)?, now()],
    )?;
    println!("{}", store.dir.display());
    Ok(())
}
