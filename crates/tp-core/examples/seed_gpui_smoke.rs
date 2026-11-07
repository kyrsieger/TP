//! Creates a new, isolated native-panel fixture. Refuses existing databases.
use std::path::PathBuf;
use tp_core::{common::now, store::Store};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected new fixture directory")?,
    );
    if dir.exists() {
        return Err("fixture directory already exists".into());
    }
    let store: Store = Store::open(dir).map_err(std::io::Error::other)?;
    let conn = store.db.lock().unwrap();
    conn.execute(
        "INSERT INTO groups(id,name,position) VALUES(1,'项目',0)",
        [],
    )?;
    for (id, kind, body, title, favorite, group) in [
        (
            "welcome",
            "text",
            "中文输入与搜索测试\n保留完整原文",
            "测试记录",
            false,
            None,
        ),
        (
            "link",
            "link",
            "https://example.com/gpui",
            "GPUI 文档",
            true,
            Some(1),
        ),
        ("color", "color", "#4F46E5", "靛蓝", false, None),
    ] {
        conn.execute("INSERT INTO records(id,digest,kind,body,title,favorite,group_id,used_at,created_at) VALUES(?1,?8,?2,?3,?4,?5,?6,?7,?7)",
            rusqlite::params![id, kind, body, title, favorite, group, now(), tp_core::digest::hash(kind, body.as_bytes())])?;
    }
    for index in 0..30 {
        let body = format!("滚动测试记录 {index:02}");
        conn.execute("INSERT INTO records(id,digest,kind,body,used_at,created_at) VALUES(?1,?4,'text',?2,?3,?3)",
            rusqlite::params![format!("row-{index}"), body, now() - 1000 - index, tp_core::digest::hash("text", body.as_bytes())])?;
    }
    println!("{}", store.dir.display());
    Ok(())
}
