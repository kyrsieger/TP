//! TP extension: preserve the insertion prefix that fits an HTML-style limit.
pub(super) fn prefix(text: &str, available: usize) -> &str {
    let mut used = 0;
    for (offset, ch) in text.char_indices() {
        used += ch.len_utf16();
        if used > available {
            return &text[..offset];
        }
    }
    text
}

/// IME selection is relative to the new composition, not the whole document
/// or the replaced range's old end. Return document UTF-8 byte boundaries.
pub(super) fn composition_selection(
    text: &str,
    selection: std::ops::Range<usize>,
    insertion_start: usize,
) -> std::ops::Range<usize> {
    let start = prefix(text, selection.start).len();
    let end = prefix(text, selection.end).len().max(start);
    insertion_start + start..insertion_start + end
}

#[cfg(test)]
mod tests {
    use super::{composition_selection, prefix};

    #[test]
    fn composition_caret_is_relative_to_new_text_after_multibyte_prefix() {
        let insertion_start = "已有😀".len();
        assert_eq!(composition_selection("拼音", 1..1, insertion_start), 13..13);
        assert_eq!(composition_selection("abcd", 2..2, insertion_start), 12..12);
    }

    #[test]
    fn composition_selection_uses_new_text_and_clamps_to_its_boundaries() {
        assert_eq!(composition_selection("😀中", 2..3, 7), 11..14);
        assert_eq!(composition_selection("😀中", 1..1, 7), 7..7);
        assert_eq!(composition_selection("😀中", 50..90, 7), 14..14);
        assert_eq!(composition_selection("", 0..8, 7), 7..7);
    }

    #[test]
    fn ascii_and_chinese_use_one_unit_each() {
        assert_eq!(prefix("abc中文", 4), "abc中");
        assert_eq!(prefix("abc", 40), "abc");
        assert_eq!(prefix("abc", 0), "");
    }

    #[test]
    fn surrogate_pair_is_never_split() {
        assert_eq!(prefix("😀abc", 1), "");
        assert_eq!(prefix("😀abc", 2), "😀");
        assert_eq!(prefix("a😀b", 2), "a");
        assert_eq!(prefix("a😀b", 3), "a😀");
    }

    #[test]
    fn replacement_keeps_suffix_and_uses_released_capacity() {
        let original = "甲".repeat(38) + "尾巴";
        let selected = 37 * 3..38 * 3;
        let retained = original[..selected.start].encode_utf16().count()
            + original[selected.end..].encode_utf16().count();
        let inserted = prefix("新名字", 40 - retained);
        assert_eq!(inserted, "新");
        let result = format!(
            "{}{}{}",
            &original[..selected.start],
            inserted,
            &original[selected.end..]
        );
        assert!(result.ends_with("新尾巴"));
        assert_eq!(result.encode_utf16().count(), 40);
    }
}
