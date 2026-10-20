//! Trim clipboard representations without flattening HTML or RTF formatting.
use scraper::{Html, Node};

pub fn trim_html(source: &str) -> String {
    use html5ever::{driver::parse_fragment, tendril::TendrilSink, QualName};
    let lower = source.trim_start().to_ascii_lowercase();
    let full_document = lower.starts_with("<!doctype") || lower.starts_with("<html");
    let context =
        if lower.starts_with("<td") || lower.starts_with("<th>") || lower.starts_with("<th ") {
            "tr"
        } else if lower.starts_with("<tr") {
            "tbody"
        } else if ["<tbody", "<thead", "<tfoot", "<caption", "<colgroup"]
            .iter()
            .any(|tag| lower.starts_with(tag))
        {
            "table"
        } else if lower.starts_with("<col") {
            "colgroup"
        } else {
            "body"
        };
    let mut changed = false;
    let mut document = if full_document {
        Html::parse_document(source)
    } else {
        parse_fragment(
            scraper::HtmlTreeSink::new(Html::new_fragment()),
            Default::default(),
            QualName::new(None, "http://www.w3.org/1999/xhtml".into(), context.into()),
            Vec::new(),
            false,
        )
        .one(source)
    };
    let root = if full_document {
        document
            .select(&scraper::Selector::parse("body").unwrap())
            .next()
            .unwrap()
            .id()
    } else {
        document.tree.root().id()
    };
    let ids: Vec<_> = document
        .tree
        .get(root)
        .unwrap()
        .descendants()
        .map(|n| n.id())
        .collect();
    for id in ids.into_iter().rev() {
        let node = document.tree.get(id).unwrap();
        if node.ancestors().any(|n| matches!(n.value(), Node::Element(e) if matches!(e.name(), "style" | "script" | "template" | "svg" | "canvas"))) {
            continue;
        }
        match node.value() {
            Node::Text(text) => {
                let trimmed = text.trim_end_matches(char::is_whitespace).to_owned();
                changed |= trimmed != text.text.as_ref();
                let empty = trimmed.is_empty();
                if let Node::Text(text) = document.tree.get_mut(id).unwrap().value() {
                    text.text = trimmed.into();
                }
                if !empty {
                    break;
                }
            }
            Node::Element(e)
                if matches!(e.name(), "style" | "script" | "template" | "meta" | "link") => {}
            Node::Element(e) if e.name() == "br" => {
                changed = true;
                document.tree.get_mut(id).unwrap().detach();
            }
            Node::Element(e)
                if matches!(
                    e.name(),
                    "p" | "div"
                        | "span"
                        | "a"
                        | "b"
                        | "strong"
                        | "i"
                        | "em"
                        | "u"
                        | "s"
                        | "strike"
                        | "font"
                        | "small"
                        | "big"
                        | "sub"
                        | "sup"
                        | "mark"
                        | "code"
                        | "pre"
                        | "blockquote"
                ) =>
            {
                // Only empty text containers may be removed. Other elements,
                // including empty table cells and list items, preserve structure
                // and mark the boundary of trailing text whitespace.
                changed = true;
                document.tree.get_mut(id).unwrap().detach();
            }
            Node::Element(e) if e.name() != "html" && e.name() != "body" => break,
            _ => {}
        }
    }
    if !changed {
        return source.to_owned();
    }
    if full_document {
        document.html()
    } else {
        document.root_element().inner_html()
    }
}

// CF_HTML offsets are UTF-8 byte offsets. Keep the original surrounding document,
// source URL and selection fragment; only replace the selected fragment itself.
pub fn trim_windows_html(bytes: &[u8]) -> Option<Vec<u8>> {
    let source = std::str::from_utf8(bytes).ok()?.trim_end_matches('\0');
    let offset = |key: &str| -> Option<usize> {
        source
            .lines()
            .find_map(|line| line.strip_prefix(key)?.trim().parse().ok())
    };
    let start = offset("StartFragment:")?;
    let end = offset("EndFragment:")?;
    let html_start = offset("StartHTML:")?;
    let html_end = offset("EndHTML:")?;
    let fragment = source.get(start..end)?;
    let trimmed = trim_html(fragment);
    if trimmed == fragment {
        return Some(bytes.to_vec());
    }
    let html = format!(
        "{}{}{}",
        source.get(html_start..start)?,
        trimmed,
        source.get(end..html_end)?
    );
    let before_fragment = start.checked_sub(html_start)?;
    let mut header = String::from("Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n");
    if let Some(url) = source.lines().find(|line| line.starts_with("SourceURL:")) {
        header.push_str(url.trim_end_matches('\r'));
        header.push_str("\r\n");
    }
    let start_html = header.len();
    let start_fragment = start_html + before_fragment;
    for (key, value) in [
        ("StartHTML", start_html),
        ("EndHTML", start_html + html.len()),
        ("StartFragment", start_fragment),
        ("EndFragment", start_fragment + trimmed.len()),
    ] {
        header = header.replace(&format!("{key}:0000000000"), &format!("{key}:{value:010}"));
    }
    Some(format!("{header}{html}\0").into_bytes())
}

/// Keep all RTF control groups and remove only trailing visible whitespace tokens.
/// Byte escapes are interpreted only for Windows-1252/default ANSI. Any other
/// declared code page or font charset preserves the entire original RTF, since a
/// whitespace-looking byte may belong to a multibyte or symbol character.
pub fn trim_rtf(source: &[u8]) -> Vec<u8> {
    #[derive(Clone, Copy, Default)]
    struct State {
        hidden: bool,
        uc: usize,
    }
    let mut state = State {
        hidden: false,
        uc: 1,
    };
    let mut stack = Vec::new();
    let mut spaces = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let start = i;
        let mut visible = None;
        match source[i] {
            b'{' => {
                stack.push(state);
                i += 1;
            }
            b'}' => {
                state = stack.pop().unwrap_or_default();
                i += 1;
            }
            b'\r' | b'\n' | 0 => {
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= source.len() {
                    break;
                }
                match source[i] {
                    b'*' => {
                        state.hidden = true;
                        i += 1;
                    }
                    b'~' => {
                        visible = Some(true);
                        i += 1;
                    }
                    b'\\' | b'{' | b'}' | b'_' | b'-' => {
                        visible = Some(false);
                        i += 1;
                    }
                    b'\'' if i + 2 < source.len() => {
                        let code = std::str::from_utf8(&source[i + 1..i + 3])
                            .ok()
                            .and_then(|s| u8::from_str_radix(s, 16).ok());
                        visible = Some(matches!(code, Some(9 | 10 | 13 | 32 | 160)));
                        i += 3;
                    }
                    _ => {
                        let word_start = i;
                        while i < source.len() && source[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word = &source[word_start..i];
                        let number_start = i;
                        if source.get(i) == Some(&b'-') {
                            i += 1;
                        }
                        while i < source.len() && source[i].is_ascii_digit() {
                            i += 1;
                        }
                        let number = std::str::from_utf8(&source[number_start..i])
                            .ok()
                            .and_then(|s| s.parse::<i32>().ok());
                        if source.get(i) == Some(&b' ') {
                            i += 1;
                        }
                        match word {
                            b"ansicpg" | b"cpg" if number != Some(1252) => {
                                return source.to_vec();
                            }
                            b"fcharset" if number != Some(0) => return source.to_vec(),
                            b"mac" | b"pc" | b"pca" => return source.to_vec(),
                            b"fonttbl" | b"colortbl" | b"stylesheet" | b"info" | b"pict"
                            | b"object" | b"fldinst" | b"header" | b"footer" | b"filetbl"
                            | b"listtable" | b"listoverridetable" => state.hidden = true,
                            b"uc" => state.uc = number.unwrap_or(1).max(0) as usize,
                            b"par" | b"line" | b"tab" | b"page" | b"sect" => visible = Some(true),
                            b"u" => {
                                let code = number.unwrap_or(0) as i16 as u16;
                                visible = Some(
                                    char::from_u32(code as u32).is_some_and(char::is_whitespace),
                                );
                                // Skip Unicode fallback characters (including escaped ANSI bytes).
                                for _ in 0..state.uc {
                                    if source.get(i) == Some(&b'\\')
                                        && source.get(i + 1) == Some(&b'\'')
                                        && i + 3 < source.len()
                                    {
                                        i += 4;
                                    } else if source.get(i) == Some(&b'\\') && i + 1 < source.len()
                                    {
                                        i += 2;
                                    } else if source
                                        .get(i)
                                        .is_some_and(|b| !matches!(b, b'{' | b'}' | b'\\'))
                                    {
                                        i += 1;
                                    }
                                }
                            }
                            b"bin" => {
                                i = (i + number.unwrap_or(0).max(0) as usize).min(source.len());
                            }
                            b"emdash" | b"endash" | b"bullet" | b"lquote" | b"rquote"
                            | b"ldblquote" | b"rdblquote" => visible = Some(false),
                            _ => {}
                        }
                        if i == word_start {
                            i += 1;
                        }
                    }
                }
            }
            byte => {
                visible = Some(byte.is_ascii_whitespace());
                i += 1;
            }
        }
        if !state.hidden {
            match visible {
                Some(true) => spaces.push(start..i),
                Some(false) => spaces.clear(),
                None => {}
            }
        }
    }
    let mut result = source.to_vec();
    for range in spaces.into_iter().rev() {
        result.drain(range);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_keeps_styles_links_and_inner_whitespace() {
        let html = trim_html("<div style=\"color:red\"><b>  first  </b><br><a href=\"https://example.com\">second&nbsp; </a><br><p><br></p></div>");
        assert_eq!(html, "<div style=\"color:red\"><b>  first  </b><br><a href=\"https://example.com\">second</a></div>");
        assert!(trim_html("<b>text</b><img src=\"x\"> ").contains("<img src=\"x\">"));
    }
    #[test]
    fn codex_selection_removes_only_terminal_empty_paragraphs_and_is_idempotent() {
        let source = "<p><strong style=\"color:red\">first</strong></p><p></p><p>    <a href=\"https://example.com\">last</a></p><p><span>&nbsp;</span></p>";
        let expected = "<p><strong style=\"color:red\">first</strong></p><p></p><p>    <a href=\"https://example.com\">last</a></p>";
        assert_eq!(trim_html(source), expected);
        assert_eq!(trim_html(expected), expected);
        let unchanged = "<p class='keep-this-spelling'><b>text</b></p>";
        assert_eq!(trim_html(unchanged), unchanged);
    }
    #[test]
    fn html_preserves_table_structure_and_document_styles() {
        assert_eq!(
            trim_html("<tr><td style=\"color:red\">cell&nbsp; </td></tr>"),
            "<tr><td style=\"color:red\">cell</td></tr>"
        );
        let full = trim_html("<!doctype html><html><head><style>b { color: red }</style></head><body style=\"font-size:20px\"><b>文字 </b><br></body></html>");
        assert!(full.contains("<style>b { color: red }</style>"));
        assert!(full.contains("<body style=\"font-size:20px\"><b>文字</b></body>"));
    }

    #[test]
    fn html_preserves_trailing_empty_table_cells_and_rows() {
        for source in [
            "<table><tr><td>A</td><td></td></tr></table>",
            "<table><tr><td>A</td><td></td></tr><tr><td></td><td></td></tr></table>",
            "<table><thead><tr><th>A</th><th></th></tr></thead><tbody><tr><td></td><td></td></tr></tbody><tfoot><tr><td colspan=\"2\"></td></tr></tfoot></table>",
            "<tr><td>A</td><td></td></tr><tr><td></td><td></td></tr>",
            "<td>A</td><td></td>",
            "<table><tr><td></td></tr></table>",
        ] {
            assert_eq!(trim_html(source), source);
        }
    }

    #[test]
    fn html_cleans_empty_paragraphs_without_removing_table_or_list_structure() {
        let source = "<div><table><tbody><tr><td style=\"color:red\">A</td><td colspan=\"2\"><p><span>&nbsp;</span><br></p></td></tr><tr><td></td><td></td></tr></tbody></table><p><br></p></div>";
        let expected = "<div><table><tbody><tr><td style=\"color:red\">A</td><td colspan=\"2\"><p><span>&nbsp;</span><br></p></td></tr><tr><td></td><td></td></tr></tbody></table></div>";
        assert_eq!(trim_html(source), expected);
        assert_eq!(trim_html(expected), expected);
        assert_eq!(
            trim_html("<ul><li>A</li><li></li></ul><p><br></p>"),
            "<ul><li>A</li><li></li></ul>"
        );
        assert_eq!(
            trim_html("<table><tbody><tr><td>A</td><td><p><br></p></td></tr></tbody></table>"),
            "<table><tbody><tr><td>A</td><td></td></tr></tbody></table>"
        );
    }

    #[test]
    fn cf_html_offsets_follow_unicode_bytes_and_preserve_context() {
        let header = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\nSourceURL:https://example.com/\r\n";
        let before = "<html><head><style>b{color:red}</style></head><body><!--StartFragment-->";
        let fragment = "<b>中文&nbsp; </b><br>";
        let after = "<!--EndFragment--></body></html>";
        let start = header.len() + before.len();
        let mut source = header.to_owned();
        for (key, offset) in [
            ("StartHTML", header.len()),
            ("EndHTML", start + fragment.len() + after.len()),
            ("StartFragment", start),
            ("EndFragment", start + fragment.len()),
        ] {
            source = source.replace(&format!("{key}:0000000000"), &format!("{key}:{offset:010}"));
        }
        source.push_str(&format!("{before}{fragment}{after}"));
        let bytes = trim_windows_html(source.as_bytes()).unwrap();
        let output = String::from_utf8(bytes).unwrap();
        let read_offset = |key: &str| -> usize {
            output
                .lines()
                .find_map(|s| s.strip_prefix(key)?.trim().parse().ok())
                .unwrap()
        };
        assert_eq!(
            &output[read_offset("StartFragment:")..read_offset("EndFragment:")],
            "<b>中文</b>"
        );
        assert!(output.contains("<style>b{color:red}</style>"));
        assert!(output.contains("SourceURL:https://example.com/"));
        assert_eq!(read_offset("EndHTML:"), output.trim_end_matches('\0').len());
        assert!(trim_windows_html(b"broken HTML header").is_none());
    }
    #[test]
    fn rtf_keeps_formatting_groups_and_inner_lines() {
        assert_eq!(
            trim_rtf(br"{\rtf1{\fonttbl{\f0 Arial;}}\b  first  \line second\b0  \par\par}"),
            br"{\rtf1{\fonttbl{\f0 Arial;}}\b  first  \line second\b0 }"
        );
        assert_eq!(trim_rtf(br"{\rtf1{\b text  }\par}"), br"{\rtf1{\b text}}");
        assert_eq!(
            trim_rtf(br"{\rtf1 text\u160?\'a0\tab\par}"),
            br"{\rtf1 text}"
        );
        assert_eq!(trim_rtf(br"{\rtf1 text \emdash}"), br"{\rtf1 text \emdash}");
    }

    #[test]
    fn rtf_preserves_multibyte_and_unknown_code_pages_verbatim() {
        for source in [
            br"{\rtf1\ansi\ansicpg932 \'82\'a0}".as_slice(),
            br"{\rtf1\ansi\ansicpg932 \'82\'a0\par}".as_slice(),
            br"{\rtf1\ansi\ansicpg936 \'81\'a0 \par}".as_slice(),
            br"{\rtf1\ansi\ansicpg99999 text\'a0\par}".as_slice(),
            // A later encoding declaration must also undo any tentative trimming.
            br"{\rtf1 text\par{\ansicpg932 \'82\'a0}}".as_slice(),
        ] {
            assert_eq!(trim_rtf(source), source);
        }
    }

    #[test]
    fn rtf_preserves_unsupported_font_and_legacy_encodings_verbatim() {
        for source in [
            br"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fcharset128 MS Gothic;}}\f0 \'82\'a0\par}"
                .as_slice(),
            br"{\rtf1\ansi{\fonttbl{\f0\cpg932 MS Gothic;}}\f0 \'82\'a0\par}".as_slice(),
            br"{\rtf1\ansi{\fonttbl{\f0\fcharset2 Symbol;}}\f0 \'a0\par}".as_slice(),
            br"{\rtf1\ansi{\fonttbl{\f0\fcharset1 Default;}}\f0 \'a0\par}".as_slice(),
            br"{\rtf1\mac text\'a0\par}".as_slice(),
            br"{\rtf1\pc text\'a0\par}".as_slice(),
            br"{\rtf1\pca text\'a0\par}".as_slice(),
        ] {
            assert_eq!(trim_rtf(source), source);
        }
        assert_eq!(
            trim_rtf(br"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fcharset0\cpg1252 Arial;}}\f0 text\'a0\tab\par}"),
            br"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fcharset0\cpg1252 Arial;}}\f0 text}"
        );
    }
}
