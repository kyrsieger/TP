use super::*;
use image::DynamicImage;
use uuid::Uuid;

#[test]
fn image_files_use_image_kind_without_changing_mixed_file_groups() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let png = dir.join("one.PNG");
    let jpeg = dir.join("two.jpg");
    let video = dir.join("clip.mp4");
    let text = dir.join("notes.txt");
    DynamicImage::new_rgb8(12, 8).save(&png).unwrap();
    DynamicImage::new_rgb8(12, 8).save(&jpeg).unwrap();
    fs::write(&video, b"video").unwrap();
    fs::write(&text, b"text").unwrap();
    let png = png.to_string_lossy().into_owned();
    let jpeg = jpeg.to_string_lossy().into_owned();
    let video = video.to_string_lossy().into_owned();
    let text = text.to_string_lossy().into_owned();

    assert_eq!(file_kind(&[png.clone()]), "image");
    assert_eq!(file_kind(&[png.clone(), jpeg]), "image");
    assert_eq!(file_kind(&[video.clone(), video.clone()]), "video");
    assert_eq!(file_kind(&[png.clone(), video]), "file");
    assert_eq!(file_kind(&[png, text]), "file");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn directory_named_like_video_stays_file_kind() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    let folder = dir.join("archive.mp4");
    fs::create_dir_all(&folder).unwrap();
    assert_eq!(file_kind(&[folder.to_string_lossy().into_owned()]), "file");
    fs::remove_dir_all(dir).unwrap();
}
