//! Isolated fixture for variable-height media rows and a long native history.
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
    let mut settings = store.snapshot()?.settings;
    settings.max_ordinary = 6000;
    let mut conn = store.db.lock().unwrap();
    conn.execute(
        "UPDATE settings SET value=?1 WHERE key='main'",
        [serde_json::to_string(&settings)?],
    )?;
    let tx = conn.transaction()?;
    for index in 0..5000 {
        tx.execute("INSERT INTO records(id,digest,kind,body,used_at,created_at) VALUES(?1,?1,'text',?2,?3,?3)",
            rusqlite::params![format!("row-{index}"),format!("虚拟列表记录 {index:04}"),now()-1000-index])?;
    }
    for (id, title, width, height) in [("wide", "宽图测试", 960, 320), ("tall", "", 200, 900)] {
        let dir = store.dir.join("assets").join(id);
        std::fs::create_dir_all(&dir)?;
        let pixels = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x * 255 / width) as u8, (y * 255 / height) as u8, 150])
        });
        pixels.save(dir.join("image.png"))?;
        tp_desktop::media::save_list_thumbnail(&dir.join("image.png"), &dir.join("thumbnail.png"))?;
        tx.execute("INSERT INTO records(id,digest,kind,title,preview,used_at,created_at) VALUES(?1,?1,'image',?2,1,?3,?3)",rusqlite::params![id,title,now()])?;
    }
    tx.execute("INSERT INTO records(id,digest,kind,title,used_at,created_at) VALUES('video','video','video','不可用视频预览',?1,?1)",[now()-10])?;
    tx.commit()?;
    println!("{}", store.dir.display());
    Ok(())
}
