use super::*;
use base64::Engine;

fn image_data() -> String {
    let image = image::RgbaImage::from_pixel(32, 18, image::Rgba([10, 20, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    )
}

#[test]
fn failed_first_video_keeps_navigation_and_later_items_can_load() {
    let failed =
        picture_from_data("group".into(), 0, 2, true, Err("missing frame".into())).unwrap();
    assert_eq!(failed.unavailable, Some("首帧预览不可用"));
    assert_eq!(
        (failed.id.as_str(), failed.index, failed.count),
        ("group", 0, 2)
    );
    let buttons = controls(
        failed.image.width() as f64,
        failed.image.height() as f64,
        1.,
        1.,
        failed.index,
        failed.count,
    );
    assert!(buttons
        .iter()
        .any(|button| matches!(button.action, Some(Action::Next))));
    assert!(buttons
        .iter()
        .any(|button| matches!(button.action, Some(Action::Previous))));
    assert!(buttons.iter().any(|button| button.text == "1/2"));

    let next =
        picture_from_data(failed.id.clone(), 1, failed.count, true, Ok(image_data())).unwrap();
    assert_eq!(next.unavailable, None);
    assert_eq!(next.image.dimensions(), (32, 18));
    assert_eq!(next.image.get_pixel(0, 0).0, [10, 20, 30, 255]);
    assert_eq!((next.index, next.count), (1, 2));

    let previous =
        picture_from_data(next.id, 0, next.count, true, Err("missing frame".into())).unwrap();
    assert_eq!(previous.unavailable, Some("首帧预览不可用"));
    assert_eq!((previous.index, previous.count), (0, 2));
}

#[test]
fn undecodable_later_image_becomes_current_placeholder() {
    let first = picture_from_data("group".into(), 0, 3, false, Ok(image_data())).unwrap();
    assert!(first.unavailable.is_none());
    let failed = picture_from_data(
        first.id,
        1,
        first.count,
        false,
        Ok("data:image/png;base64,bm90IGFuIGltYWdl".into()),
    )
    .unwrap();
    assert_eq!(failed.unavailable, Some("预览图不可用"));
    assert_eq!((failed.index, failed.count), (1, 3));
    let last = picture_from_data(failed.id, 2, failed.count, false, Ok(image_data())).unwrap();
    assert!(last.unavailable.is_none());
    assert_eq!((last.index, last.count), (2, 3));
}

#[test]
fn a_single_failed_item_still_reports_the_loading_error() {
    let failed = picture_from_data("single".into(), 0, 1, true, Err("missing frame".into()));
    assert!(matches!(failed, Err(error) if error == "missing frame"));
    assert!(picture_from_data("single".into(), 0, 1, false, Ok("invalid".into())).is_err());
}
