//! Opt-in native clipboard test. The caller must retain/restore the user's clipboard.
use crate::platform;
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};

#[cfg(windows)]
#[test]
#[ignore = "uses the real system clipboard; run with an external clipboard backup"]
fn native_clean_text_removes_html_document_footer_spaces() {
    let ctx = ClipboardContext::new().unwrap();
    let text = "  first\n    后台常驻";
    ctx.set(vec![
        ClipboardContent::Text(text.into()),
        ClipboardContent::Html("<b style=\"color:red\">  first</b><br>    后台常驻".into()),
    ])
    .unwrap();
    let body_text = || {
        let bytes = ctx.get_buffer("HTML Format").unwrap();
        let source = std::str::from_utf8(&bytes).unwrap().trim_end_matches('\0');
        let start: usize = source
            .lines()
            .find_map(|line| line.strip_prefix("StartHTML:")?.trim().parse().ok())
            .unwrap();
        let end: usize = source
            .lines()
            .find_map(|line| line.strip_prefix("EndHTML:")?.trim().parse().ok())
            .unwrap();
        scraper::Html::parse_document(&source[start..end])
            .select(&scraper::Selector::parse("body").unwrap())
            .next()
            .unwrap()
            .text()
            .collect::<String>()
    };
    assert!(
        body_text().ends_with("\n\n"),
        "fixture must reproduce the two footer newlines"
    );
    assert!(
        platform::normalize_clipboard_text(platform::clipboard_revision(), text, text).unwrap()
    );
    assert_eq!(ctx.get_text().unwrap(), text);
    assert!(!body_text().ends_with(char::is_whitespace));
    assert!(ctx
        .get_html()
        .unwrap()
        .contains("<b style=\"color:red\">  first</b><br>    后台常驻"));
    let revision = platform::clipboard_revision();
    assert!(!platform::normalize_clipboard_text(revision, text, text).unwrap());
    assert_eq!(platform::clipboard_revision(), revision);
}

#[test]
#[ignore = "uses the real system clipboard; run with an external clipboard backup"]
fn native_clipboard_cleanup_preserves_formats_and_rejects_a_newer_copy() {
    let ctx = ClipboardContext::new().unwrap();
    let original = "  TP clipboard test\n\tsecond \r\n";
    ctx.set(vec![
        ClipboardContent::Text(original.into()),
        ClipboardContent::Html("<div style=\"color:red\"><b>  TP clipboard test</b><br>\t<a href=\"https://example.com\">second&nbsp; </a><br></div>".into()),
        ClipboardContent::Rtf(r"{\rtf1\b  TP clipboard test\b0\line\tab second \par}".into()),
        ClipboardContent::Other("TP.ClipboardTest.Metadata".into(), b"keep me".to_vec()),
    ]).unwrap();
    let revision = platform::clipboard_revision();
    assert!(platform::normalize_clipboard_text(revision, original, original.trim_end()).unwrap());
    assert_eq!(ctx.get_text().unwrap(), "  TP clipboard test\n\tsecond");
    let html = ctx.get_html().unwrap();
    assert!(html.contains("style=\"color:red\""));
    assert!(html.contains("<b>  TP clipboard test</b>"));
    assert!(html.contains("href=\"https://example.com\""));
    assert!(!html.contains("&nbsp;"));
    assert!(!html.ends_with("<br></div>"));
    assert_eq!(
        ctx.get_rich_text().unwrap().trim_end_matches('\0'),
        r"{\rtf1\b  TP clipboard test\b0\line\tab second}"
    );
    assert!(ctx
        .get_buffer("TP.ClipboardTest.Metadata")
        .unwrap()
        .starts_with(b"keep me"));
    let stale = platform::clipboard_revision();
    ctx.set_text("newer copy".into()).unwrap();
    assert!(!platform::normalize_clipboard_text(stale, "newer copy", "wrong").unwrap());
    assert_eq!(ctx.get_text().unwrap(), "newer copy");
    ctx.set_text(" \r\n\t".into()).unwrap();
    assert!(
        platform::normalize_clipboard_text(platform::clipboard_revision(), " \r\n\t", "").unwrap()
    );
    assert_eq!(ctx.get_text().unwrap(), "");
}

#[test]
#[ignore = "uses the real system clipboard; run with an external clipboard backup"]
fn native_clean_plain_text_still_removes_html_empty_paragraphs() {
    let ctx = ClipboardContext::new().unwrap();
    let text = "TP rich-only cleanup\n    last line";
    #[cfg(windows)]
    {
        use clipboard_win::{options, raw, register_format, Clipboard as WinClipboard};
        use windows::{
            core::w,
            Win32::Foundation::HWND,
            Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE},
        };
        struct FixtureWindow(HWND);
        impl Drop for FixtureWindow {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let owner = FixtureWindow(
            unsafe {
                CreateWindowExW(
                    Default::default(),
                    w!("STATIC"),
                    w!("TP Clipboard Test"),
                    Default::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )
            }
            .unwrap(),
        );
        let _guard = WinClipboard::new_attempts_for(owner.0 .0, 20).expect("open native fixture");
        raw::empty().expect("empty native fixture");
        raw::set_string_with(text, options::NoClear).expect("write fixture text");
        raw::set_html_with(register_format("HTML Format").unwrap().get(), "<p><strong style=\"color:red\">TP rich-only cleanup</strong></p><p>    <a href=\"https://example.com\">last line</a></p><p></p>", options::NoClear).expect("write fixture HTML");
        raw::set_without_clear(
            register_format("Rich Text Format").unwrap().get(),
            br"{\rtf1\b TP rich-only cleanup\b0\line\tab last line\par}",
        )
        .expect("write fixture RTF");
        raw::set_without_clear(
            register_format("TP.ClipboardTest.Metadata").unwrap().get(),
            b"keep me",
        )
        .expect("write fixture metadata");
    }
    #[cfg(target_os = "macos")]
    ctx.set(vec![
        ClipboardContent::Text(text.into()),
        ClipboardContent::Html("<p><strong style=\"color:red\">TP rich-only cleanup</strong></p><p>    <a href=\"https://example.com\">last line</a></p><p></p>".into()),
        ClipboardContent::Rtf(r"{\rtf1\b TP rich-only cleanup\b0\line\tab last line\par}".into()),
        ClipboardContent::Other("TP.ClipboardTest.Metadata".into(), b"keep me".to_vec()),
    ]).unwrap();
    assert_eq!(
        ctx.get_text().unwrap(),
        text,
        "native fixture must be fully readable before normalization"
    );
    platform::normalize_clipboard_text(platform::clipboard_revision(), text, text).unwrap();
    assert_eq!(ctx.get_text().unwrap(), text);
    let html = ctx.get_html().unwrap();
    assert!(html.contains("<strong style=\"color:red\">"));
    assert!(html.contains("href=\"https://example.com\""));
    assert!(!html.ends_with("<p></p>"));
    assert!(!ctx.get_rich_text().unwrap().contains(r"\par"));
    assert!(ctx
        .get_buffer("TP.ClipboardTest.Metadata")
        .unwrap()
        .starts_with(b"keep me"));
    let revision = platform::clipboard_revision();
    assert!(!platform::normalize_clipboard_text(revision, text, text).unwrap());
    assert_eq!(platform::clipboard_revision(), revision);
}
