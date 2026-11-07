//! Selection units refer to original UTF-8 bytes, including original line endings.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Normalize only the displayed line endings. Selection and clipboard offsets
/// remain in the original source, so a CRLF is never silently copied as LF.
pub struct DisplayText {
    pub text: String,
    source_offsets: Vec<usize>,
}
impl DisplayText {
    pub fn new(source: &str) -> Self {
        let mut bytes = Vec::with_capacity(source.len());
        let mut source_offsets = vec![0];
        let mut index = 0;
        while index < source.len() {
            let byte = source.as_bytes()[index];
            index += 1;
            if byte == b'\r' {
                if source.as_bytes().get(index) == Some(&b'\n') {
                    index += 1;
                }
                bytes.push(b'\n');
            } else {
                bytes.push(byte);
            }
            source_offsets.push(index);
        }
        Self {
            text: String::from_utf8(bytes).expect("newline normalization preserves UTF-8"),
            source_offsets,
        }
    }
    pub fn source_index(&self, display: usize) -> usize {
        self.source_offsets[display.min(self.text.len())]
    }
    pub fn display_range(&self, source: Range<usize>) -> Range<usize> {
        let offset = |index| {
            self.source_offsets
                .partition_point(|offset| *offset <= index)
                .saturating_sub(1)
        };
        offset(source.start)..offset(source.end)
    }
}

#[derive(Clone, Copy, Default)]
pub enum Unit {
    #[default]
    Character,
    Word,
    Line,
}

pub fn span(text: &str, index: usize, unit: Unit) -> Range<usize> {
    let index = index.min(text.len());
    match unit {
        Unit::Character => {
            if index == text.len() {
                return index..index;
            }
            let index = text
                .grapheme_indices(true)
                .map(|(offset, _)| offset)
                .take_while(|offset| *offset <= index)
                .last()
                .unwrap_or(0);
            if text.is_empty() {
                0..0
            } else {
                index..index
            }
        }
        Unit::Word => text
            .split_word_bound_indices()
            .map(|(offset, word)| offset..offset + word.len())
            .find(|range| range.contains(&index))
            .or_else(|| {
                text.split_word_bound_indices()
                    .last()
                    .map(|(offset, word)| offset..offset + word.len())
            })
            .unwrap_or(0..0),
        Unit::Line => {
            let mut start = 0;
            let mut cursor = 0;
            while cursor < text.len() {
                let byte = text.as_bytes()[cursor];
                cursor += 1;
                if matches!(byte, b'\r' | b'\n') {
                    if byte == b'\r' && text.as_bytes().get(cursor) == Some(&b'\n') {
                        cursor += 1;
                    }
                    if index < cursor {
                        return start..cursor;
                    }
                    start = cursor;
                }
            }
            start..text.len()
        }
    }
}

pub fn extend(text: &str, origin: Range<usize>, index: usize, unit: Unit) -> (usize, usize) {
    if matches!(unit, Unit::Character) {
        return (origin.start, span(text, index, unit).start);
    }
    let target = span(text, index, unit);
    if target.start < origin.start {
        (origin.end, target.start)
    } else {
        (origin.start, target.end.max(origin.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_normalizes_mixed_newlines_without_changing_copied_source() {
        let source = "甲\r\n😀\r乙\n";
        let display = DisplayText::new(source);
        assert_eq!(display.text, "甲\n😀\n乙\n");
        let source_range = 5..10;
        let visible = display.display_range(source_range.clone());
        assert_eq!(&display.text[visible.clone()], "😀\n");
        assert_eq!(
            &source[display.source_index(visible.start)..display.source_index(visible.end)],
            "😀\r"
        );
        assert_eq!(&source[span(source, 9, Unit::Line)], "😀\r");
        assert_eq!(display.source_index(display.text.len()), source.len());
        assert_eq!(DisplayText::new("").source_index(0), 0);
    }
    #[test]
    fn character_drag_preserves_graphemes_and_end_of_text() {
        let text = "a👩‍💻";
        assert_eq!(
            span(text, text.len(), Unit::Character),
            text.len()..text.len()
        );
        assert_eq!(extend(text, 0..0, 5, Unit::Character), (0, 1));
        assert_eq!(
            extend(text, 0..0, text.len(), Unit::Character),
            (0, text.len())
        );
        assert_eq!(span("last line", 9, Unit::Line), 0..9);
    }
    #[test]
    fn word_selection_keeps_combining_marks_and_emoji_intact() {
        let text = "hello café e\u{301} 👩‍💻中文";
        assert_eq!(&text[span(text, 2, Unit::Word)], "hello");
        let accent = text.find("e\u{301}").unwrap();
        assert_eq!(&text[span(text, accent + 1, Unit::Word)], "e\u{301}");
        let emoji = text.find('👩').unwrap();
        assert_eq!(&text[span(text, emoji + 4, Unit::Word)], "👩‍💻");
    }
    #[test]
    fn line_selection_keeps_crlf_and_final_empty_line() {
        let text = "甲\r\n乙\n";
        assert_eq!(&text[span(text, 1, Unit::Line)], "甲\r\n");
        assert_eq!(&text[span(text, 5, Unit::Line)], "乙\n");
        assert_eq!(span(text, text.len(), Unit::Line), text.len()..text.len());
        assert_eq!(span("", 0, Unit::Word), 0..0);
    }
    #[test]
    fn word_drag_preserves_original_unit_in_both_directions() {
        let text = "one two three";
        let origin = span(text, 5, Unit::Word);
        assert_eq!(extend(text, origin.clone(), 10, Unit::Word), (4, 13));
        assert_eq!(extend(text, origin.clone(), 0, Unit::Word), (7, 0));
        assert_eq!(extend(text, origin, 5, Unit::Word), (4, 7));
    }
}
