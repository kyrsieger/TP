use super::*;

fn ingest_capture(
    store: &std::sync::Arc<Store>,
    changed: &crate::Changed,
    capture: Captured,
    revision: Option<u64>,
    sensitive: bool,
    normalize: impl FnOnce(&str, &str) -> Result<bool>,
) -> Result<()> {
    let _data = store.read_data()?;
    ingest_capture_locked(store, changed, capture, revision, sensitive, normalize)
}

#[test]
fn capture_stores_original_before_normalizing_and_reuses_record_identity() {
    use std::sync::Arc;
    let store = Arc::new(crate::test_support::test_store());
    let changed: crate::Changed = Arc::new(|| {});
    let original = "  中文原文\r\n";
    ingest_capture(
        &store,
        &changed,
        Captured::Text(original.into()),
        Some(1),
        true,
        |body, text| {
            assert_eq!(body, original);
            assert_eq!(text, "  中文原文");
            assert_eq!(store.snapshot().unwrap().records[0].body, original);
            Ok(false)
        },
    )
    .unwrap();
    let first = store.snapshot().unwrap().records.remove(0);
    assert!(first.expires_at.unwrap() >= first.created_at.unwrap() + 89_000);
    tp_core::operations::set_favorite(&store, first.id.clone(), true).unwrap();
    ingest_capture(
        &store,
        &changed,
        Captured::Text(original.into()),
        Some(2),
        false,
        |_, _| Ok(false),
    )
    .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.records.len(), 1);
    assert_eq!(snapshot.records[0].id, first.id);
    assert_eq!(snapshot.records[0].created_at, first.created_at);
    assert!(snapshot.records[0].favorite);
    assert_eq!(snapshot.records[0].expires_at, None);
}

#[test]
fn image_capture_persists_original_and_thumbnail_without_clipboard_access() {
    use std::{io::Cursor, sync::Arc};
    let store = Arc::new(crate::test_support::test_store());
    let changed: crate::Changed = Arc::new(|| {});
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(12, 20)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    ingest_capture(
        &store,
        &changed,
        Captured::Image(bytes.into_inner()),
        None,
        false,
        |_, _| panic!("image normalization"),
    )
    .unwrap();
    let snapshot = store.snapshot().unwrap();
    let item = &snapshot.records[0];
    assert_eq!(item.kind, "image");
    assert!(item.preview);
    let path = store.dir.join("assets").join(&item.id);
    assert_eq!(image::open(path.join("image.png")).unwrap().width(), 12);
    assert!(path.join("thumbnail.png").is_file());
}

#[test]
fn file_capture_finishes_a_whole_group_as_independent_copies() {
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };
    let store = Arc::new(crate::test_support::test_store());
    let first = store.dir.join("first.txt");
    let second = store.dir.join("second.txt");
    fs::write(&first, "one").unwrap();
    fs::write(&second, "two").unwrap();
    let (tx, rx) = mpsc::channel();
    let changed: crate::Changed = Arc::new(move || {
        let _ = tx.send(());
    });
    ingest_capture(
        &store,
        &changed,
        Captured::Files(vec![
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ]),
        None,
        false,
        |_, _| panic!("file normalization"),
    )
    .unwrap();
    let final_record = loop {
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let item = store.snapshot().unwrap().records.remove(0);
        if item.status != "pending" {
            break item;
        }
    };
    assert_eq!(final_record.storage_mode, "copy");
    assert_eq!(final_record.files.len(), 2);
    fs::remove_file(first).unwrap();
    fs::remove_file(second).unwrap();
    let bodies = final_record
        .files
        .iter()
        .map(|f| fs::read_to_string(f.stored.as_ref().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bodies, ["one", "two"]);
}

#[test]
fn writeback_preserves_original_and_removes_trailing_whitespace() {
    for (original, expected) in [
        ("DSH 0.2.1-rc.2\r\n", "DSH 0.2.1-rc.2"),
        ("\n    first\r\n\tsecond\r\n\r\n", "\n    first\r\n\tsecond"),
        ("first\rsecond\r", "first\rsecond"),
        ("text  \n", "text"),
        ("text\n  ", "text"),
        ("text ", "text"),
        ("text  ", "text"),
        ("text\t\r\n \t", "text"),
        ("text\u{00a0}\u{0085}\u{3000}", "text"),
        (" \t\r\n\u{00a0}", ""),
        ("  first  \n\tsecond \t\n", "  first  \n\tsecond"),
        ("text\u{200b}", "text\u{200b}"),
        ("\r\n\n\r", ""),
        ("", ""),
    ] {
        let original = original.to_owned();
        let saved = original.clone();
        let (text, digest) = text_writeback(&original, true);
        assert_eq!(text, expected);
        assert_eq!(digest, hash(text_kind(expected), expected.as_bytes()));
        assert_eq!(text_writeback(&original, false).0, saved);
        assert_eq!(original, saved);
    }
}

#[test]
fn writeback_suppresses_actual_text_for_all_text_kinds() {
    for original in ["text\r\n", "https://example.com\n", "#abc\r"] {
        for remove in [true, false] {
            let (written, suppressed) = text_writeback(original, remove);
            assert_eq!(suppressed, hash(text_kind(written), written.as_bytes()));
            if remove {
                assert_ne!(suppressed, hash(text_kind(original), original.as_bytes()));
            }
        }
    }
}

#[test]
fn automatic_cleanup_inspects_rich_formats_even_when_plain_text_is_clean() {
    use super::super::test_support::test_store;
    let store = test_store();
    assert!(!normalize_copied_text(&store, "text \n", false, |_| panic!(
        "disabled must not write"
    ))
    .unwrap());
    let mut inspected = false;
    assert!(!normalize_copied_text(&store, "clean text", true, |text| {
        inspected = true;
        assert_eq!(text, "clean text");
        Ok(false)
    })
    .unwrap());
    assert!(inspected);
    assert!(store.suppressed_digest.lock().unwrap().is_none());
    assert!(normalize_copied_text(&store, "clean text", true, |text| {
        assert_eq!(text, "clean text");
        Ok(true) // HTML had a trailing empty paragraph although text was clean.
    })
    .unwrap());
    *store.suppressed_digest.lock().unwrap() = None;
    for succeed in [false, true] {
        let result = normalize_copied_text(&store, "  first  \n\tsecond \r\n", true, |text| {
            assert_eq!(text, "  first  \n\tsecond");
            Ok(succeed)
        })
        .unwrap();
        assert_eq!(result, succeed);
        assert_eq!(store.suppressed_digest.lock().unwrap().is_some(), succeed);
    }
    let previous = store.suppressed_digest.lock().unwrap().clone();
    assert!(
        normalize_copied_text(&store, "different  ", true, |_| Err("write failed".into())).is_err()
    );
    assert_eq!(*store.suppressed_digest.lock().unwrap(), previous);
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn self_write_notifications_are_suppressed_but_a_new_identical_copy_is_captured() {
    use super::super::test_support::test_store;
    let store = test_store();
    *store.suppressed_digest.lock().unwrap() = Some(("digest".into(), Some(10)));
    assert!(is_suppressed_copy(&store, "digest", Some(10)).unwrap());
    assert!(is_suppressed_copy(&store, "digest", Some(10)).unwrap());
    assert!(!is_suppressed_copy(&store, "digest", Some(11)).unwrap());
    assert!(store.suppressed_digest.lock().unwrap().is_none());
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
