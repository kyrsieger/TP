//! One canonical form for native shortcut capture and list matching.
use gpui::Keystroke;

/// Panel shortcut capture mirrors shortcutFromKey; native registration decides
/// whether a captured function key is supported by the platform.
pub fn panel_key(stroke: &Keystroke) -> Option<String> {
    if !stroke.modifiers.modified() {
        return None;
    }
    let key = stroke.key.to_uppercase();
    let function = key.strip_prefix('F').is_some_and(|digits| {
        (1..=2).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
    });
    if !function && !(key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric()) {
        return None;
    }
    Some(canonical(stroke, &key))
}

pub fn rename_conflicts(shortcut: &str, panel_shortcut: &str) -> bool {
    if shortcut.is_empty() {
        return false;
    }
    shortcut == panel_shortcut
        || shortcut
            .strip_prefix("Control+")
            .is_some_and(|key| key.len() == 1 && b"123456789ADFGP".contains(&key.as_bytes()[0]))
}

fn canonical(stroke: &Keystroke, key: &str) -> String {
    let m = stroke.modifiers;
    let mut parts = Vec::new();
    if m.control {
        parts.push("Control");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    if m.platform {
        parts.push("Super");
    }
    parts.push(key);
    parts.join("+")
}

pub fn rename_key(stroke: &Keystroke) -> Option<String> {
    let key = stroke.key.to_uppercase();
    let m = stroke.modifiers;
    let mut parts = Vec::new();
    if m.control {
        parts.push("Control");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    if m.platform {
        parts.push("Super");
    }
    let function = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n) && key == format!("F{n}"));
    let character =
        !parts.is_empty() && key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric();
    if !function && !character {
        return None;
    }
    parts.push(&key);
    Some(parts.join("+"))
}

pub fn display(shortcut: &str) -> String {
    shortcut
        .split('+')
        .map(|key| if key == "Control" { "Ctrl" } else { key })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_capture_and_rename_conflicts_match_original_ui_rules() {
        for (input, expected) in [
            ("f2", None),
            ("ctrl-f2", Some("Control+F2")),
            ("ctrl-f25", Some("Control+F25")),
            ("alt-7", Some("Alt+7")),
            ("ctrl-enter", None),
            ("ctrl-!", None),
        ] {
            assert_eq!(
                panel_key(&Keystroke::parse(input).unwrap()).as_deref(),
                expected
            );
        }
        for key in ["Control+F", "Control+1", "Control+A", "Control+P", "Alt+R"] {
            assert!(rename_conflicts(key, "Alt+R"), "{key}");
        }
        for key in ["", "Control+C", "Control+0", "Control+Shift+F", "F2"] {
            assert!(!rename_conflicts(key, "Alt+R"), "{key}");
        }
    }
    #[test]
    fn capture_and_matching_share_function_and_modified_character_rules() {
        for (input, expected) in [
            ("f2", Some("F2")),
            ("ctrl-shift-r", Some("Control+Shift+R")),
            ("alt-7", Some("Alt+7")),
            ("a", None),
            ("enter", None),
            ("ctrl-!", None),
            ("f25", None),
        ] {
            let stroke = Keystroke::parse(input).unwrap();
            assert_eq!(rename_key(&stroke).as_deref(), expected, "{input}");
        }
        assert_eq!(display("Control+Alt+F2"), "Ctrl+Alt+F2");
    }
}
