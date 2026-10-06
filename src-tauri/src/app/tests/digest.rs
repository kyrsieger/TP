use super::*;

#[test]
fn recognizes_only_complete_hex_colors() {
    for value in ["#abc", "#ABCD", "#12ab34", "#12345678", "  #abc  "] {
        assert!(is_hex_color(value), "{value}");
    }
    for value in [
        "abc",
        "#ab",
        "#abcde",
        "#123456789",
        "#12xz34",
        "color: #abc",
        "#abc\n#def",
    ] {
        assert!(!is_hex_color(value), "{value}");
    }
}
