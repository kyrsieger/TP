//! Synthetic multi-file previews, including missing and undecodable items.
use std::path::PathBuf;
use tp_core::{common::now, records::FileItem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected new fixture directory")?,
    );
    if dir.exists() {
        return Err("fixture directory already exists".into());
    }
    let store = tp_desktop::Store::open(dir)?;
    for (kind, id) in [("image", "images"), ("video", "videos")] {
        let assets = store.dir.join("assets").join(id);
        std::fs::create_dir_all(&assets)?;
        let files: Vec<_> = (0..3)
            .map(|index| FileItem {
                original: assets
                    .join(format!("missing-{index}"))
                    .to_string_lossy()
                    .into_owned(),
                stored: None,
                is_directory: false,
            })
            .collect();
        for (index, color) in [(1, [35, 120, 200]), (2, [200, 100, 35])] {
            let filename = if kind == "image" {
                format!("image-{index}.png")
            } else {
                format!("frame-{index}.png")
            };
            image::RgbImage::from_pixel(800, 450, image::Rgb(color)).save(assets.join(filename))?;
        }
        // The first page is deliberately missing; later pages must still be reachable.
        store.db.lock().unwrap().execute(
            "INSERT INTO records(id,digest,kind,title,files,used_at,created_at) VALUES(?1,?1,?2,?3,?4,?5,?5)",
            rusqlite::params![id,kind,format!("{kind}: missing / blue / orange"),serde_json::to_string(&files)?,now()],
        )?;
    }
    println!("{}", store.dir.display());
    Ok(())
}
