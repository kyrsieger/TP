use super::*;

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
