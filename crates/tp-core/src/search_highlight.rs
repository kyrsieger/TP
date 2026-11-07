//! Non-overlapping case-insensitive matches expressed in original UTF-8 byte offsets.
use std::ops::Range;

pub fn ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let needle = query.to_lowercase();
    // Lowercasing can expand a character (İ → i + combining dot). Map each
    // lowercase byte back to the complete original character, never a partial glyph.
    let mut starts = Vec::with_capacity(lower.len());
    let mut ends = Vec::with_capacity(lower.len());
    for (offset, ch) in text.char_indices() {
        let len: usize = ch.to_lowercase().map(char::len_utf8).sum();
        starts.extend(std::iter::repeat_n(offset, len));
        ends.extend(std::iter::repeat_n(offset + ch.len_utf8(), len));
    }
    let mut result = Vec::new();
    let mut consumed = 0;
    for (index, _) in lower.match_indices(&needle) {
        let start = starts[index].max(consumed);
        let end = ends[index + needle.len() - 1];
        if end > consumed {
            result.push(start..end);
            consumed = end;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn matches<'a>(text: &'a str, query: &str) -> Vec<&'a str> {
        ranges(text, query)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }
    #[test]
    fn literal_non_overlapping_matches_preserve_original_content() {
        assert_eq!(matches("AaAa <B>&", "aa"), ["Aa", "Aa"]);
        assert_eq!(matches("<B>&", "<b>"), ["<B>"]);
        assert_eq!(matches("aaaa", "aaa"), ["aaa"]);
        assert!(ranges("text", "").is_empty());
        assert!(ranges("text", "missing").is_empty());
    }
    #[test]
    fn unicode_expansion_and_context_preserve_byte_boundaries() {
        assert_eq!(matches("😀İ中文İ", "i"), ["İ", "İ"]);
        assert_eq!(matches("İi", "\u{307}"), ["İ"]);
        assert_eq!(matches("😀中文😀", "😀"), ["😀", "😀"]);
        assert_eq!(matches("ΟΣ", "ος"), ["ΟΣ"]);
        assert_eq!(matches("中文中文", "中文"), ["中文", "中文"]);
    }
}
