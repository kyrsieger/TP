use super::super::test_support::test_store;
use super::*;
use rusqlite::params;
use uuid::Uuid;

#[test]
fn text_file_preview_accepts_text_and_rejects_binary_files() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    for extension in ["md", "JSON", "txt", "ts", "tsx", "css", "kt", "java"] {
        let path = dir.join(format!("sample.{extension}"));
        fs::write(&path, "line 1\n中文").unwrap();
        assert_eq!(
            read_text_file_preview(&path).unwrap().as_deref(),
            Some("line 1\n中文")
        );
    }
    for extension in ["pdf", "docx", "xlsx"] {
        let path = dir.join(format!("sample.{extension}"));
        fs::write(&path, b"plain bytes").unwrap();
        assert_eq!(read_text_file_preview(&path).unwrap(), None);
    }
    let disguised = dir.join("disguised.txt");
    fs::write(&disguised, b"abc\0def").unwrap();
    assert!(read_text_file_preview(&disguised).is_err());
    fs::write(&disguised, [0xff, 0xfe, b'A', 0, 0x2d, 0x4e]).unwrap();
    assert_eq!(
        read_text_file_preview(&disguised).unwrap().as_deref(),
        Some("A中")
    );
    fs::write(&disguised, vec![b'a'; 1024 * 1024 + 1]).unwrap();
    assert!(read_text_file_preview(&disguised).is_err());
    fs::create_dir(dir.join("folder.txt")).unwrap();
    assert_eq!(
        read_text_file_preview(&dir.join("folder.txt")).unwrap(),
        None
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn text_file_preview_reads_copy_and_reference_by_record_mode() {
    let store = test_store();
    let original = store.dir.join("original.md");
    let stored = store.dir.join("stored.md");
    fs::write(&original, "original").unwrap();
    fs::write(&stored, "stored").unwrap();
    let files = serde_json::to_string(&vec![FileItem {
        original: original.to_string_lossy().into_owned(),
        stored: Some(stored.to_string_lossy().into_owned()),
        is_directory: false,
    }])
    .unwrap();
    let conn = store.db.lock().unwrap();
    for (id, mode) in [("copy", "copy"), ("reference", "reference")] {
        conn.execute(
                "INSERT INTO records(id,digest,kind,files,storage_mode,used_at) VALUES(?1,?1,'file',?2,?3,1)",
                params![id, files, mode],
            ).unwrap();
    }
    assert_eq!(
        file_text_preview_path(&conn, "copy", 0).unwrap(),
        Some(stored)
    );
    assert_eq!(
        file_text_preview_path(&conn, "reference", 0).unwrap(),
        Some(original)
    );
    assert!(file_text_preview_path(&conn, "copy", 1).is_err());
    drop(conn);
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn hover_previews_preserve_the_whole_image_without_upscaling() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    for (index, (width, height, expected)) in [
        (3200, 1600, (1440, 720)),
        (200, 4000, (72, 1440)),
        (12, 8, (12, 8)),
    ]
    .into_iter()
    .enumerate()
    {
        let source = dir.join(format!("source-{index}.png"));
        let image = image::ImageBuffer::from_fn(width, height, |_, y| {
            if y < height / 2 {
                image::Rgba([255u8, 0, 0, 255])
            } else {
                image::Rgba([0u8, 0, 255, 255])
            }
        });
        image.save(&source).unwrap();
        let output = hover_preview_path(&source, &dir, index).unwrap();
        let preview = image::open(&output).unwrap();
        assert_eq!(preview.dimensions(), expected);
        assert_eq!(preview.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(
            preview.get_pixel(0, preview.height() - 1).0,
            [0, 0, 255, 255]
        );
        assert_eq!(image::open(&source).unwrap().dimensions(), (width, height));
    }
    assert!(dir.join("hover.png").exists());
    assert!(dir.join("hover-1.png").exists());
    assert!(dir.join("hover-2.png").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn hover_previews_reuse_the_generated_file() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.png");
    DynamicImage::new_rgb8(12, 8).save(&source).unwrap();
    let output = hover_preview_path(&source, &dir, 0).unwrap();
    fs::remove_file(&source).unwrap();
    assert_eq!(hover_preview_path(&source, &dir, 0).unwrap(), output);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn image_file_preview_decodes_jpeg() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.jpg");
    DynamicImage::new_rgb8(12, 8).save(&source).unwrap();
    let preview = dir.join("preview");

    save_image_file_preview(&source, &preview).unwrap();

    assert_eq!(
        image::open(preview.join("image.png")).unwrap().dimensions(),
        (12, 8)
    );
    assert_eq!(
        image::open(preview.join("thumbnail.png"))
            .unwrap()
            .dimensions(),
        (12, 8)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn image_file_preview_uses_file_contents_when_extension_is_wrong() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let jpeg = dir.join("source.jpg");
    let source = dir.join("source.png");
    DynamicImage::new_rgb8(12, 8).save(&jpeg).unwrap();
    fs::rename(jpeg, &source).unwrap();

    save_image_file_preview(&source, &dir.join("preview")).unwrap();

    assert_eq!(
        image::open(dir.join("preview/image.png"))
            .unwrap()
            .dimensions(),
        (12, 8)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn image_file_previews_keep_each_file_in_its_own_slot() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.png");
    let second = dir.join("second.png");
    DynamicImage::new_rgb8(12, 8).save(&first).unwrap();
    DynamicImage::new_rgb8(8, 12).save(&second).unwrap();
    let previews = dir.join("previews");

    save_indexed_image_file_preview(&first, &previews, 0).unwrap();
    save_indexed_image_file_preview(&second, &previews, 1).unwrap();

    assert_eq!(
        image::open(previews.join("image.png"))
            .unwrap()
            .dimensions(),
        (12, 8)
    );
    assert_eq!(
        image::open(previews.join("image-1.png"))
            .unwrap()
            .dimensions(),
        (8, 12)
    );
    assert!(previews.join("thumbnail.png").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn list_thumbnails_crop_long_images_and_keep_normal_images() {
    let tall = DynamicImage::ImageRgba8(image::ImageBuffer::from_fn(100, 600, |_, y| {
        if y < 150 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    }));
    let tall_thumb = list_thumbnail(&tall);
    assert_eq!(tall_thumb.dimensions(), (100, 100));
    assert_eq!(tall_thumb.get_pixel(50, 50).0, [255, 0, 0, 255]);

    let wide = DynamicImage::ImageRgba8(image::ImageBuffer::from_fn(600, 100, |x, _| {
        if (150..450).contains(&x) {
            image::Rgba([0, 255, 0, 255])
        } else {
            image::Rgba([255, 0, 0, 255])
        }
    }));
    let wide_thumb = list_thumbnail(&wide);
    assert_eq!(wide_thumb.dimensions(), (225, 100));
    assert_eq!(wide_thumb.get_pixel(112, 50).0, [0, 255, 0, 255]);

    assert_eq!(
        list_thumbnail(&DynamicImage::new_rgba8(200, 100)).dimensions(),
        (200, 100)
    );
}
