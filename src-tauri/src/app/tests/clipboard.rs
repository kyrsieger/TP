use super::*;

#[test]
fn writeback_preserves_original_and_only_removes_terminal_cr_lf() {
    for (original, expected) in [
        ("DSH 0.2.1-rc.2\r\n", "DSH 0.2.1-rc.2"),
        ("\n    first\r\n\tsecond\r\n\r\n", "\n    first\r\n\tsecond"),
        ("first\rsecond\r", "first\rsecond"),
        ("text  \n", "text  "),
        ("text\n  ", "text\n  "),
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
