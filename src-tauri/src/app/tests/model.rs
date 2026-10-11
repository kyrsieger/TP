use super::*;

#[test]
fn rename_shortcut_defaults_and_rejects_invalid_or_conflicting_keys() {
    let mut value = serde_json::to_value(Settings::default()).unwrap();
    value.as_object_mut().unwrap().remove("renameShortcut");
    let restored: Settings = serde_json::from_value(value).unwrap();
    assert_eq!(restored.rename_shortcut, "F2");
    let mut config = Settings::default();
    for shortcut in ["", "F2", "F24", "Shift+F3", "Control+Shift+R", "Super+R"] {
        config.rename_shortcut = shortcut.into();
        assert!(config.validate().is_ok(), "{shortcut}");
    }
    for shortcut in [
        "F0",
        "F25",
        "F02",
        "A",
        "Enter",
        "Control+D",
        "Control+1",
        "Alt+Control+R",
        "Control+Control+R",
    ] {
        config.rename_shortcut = shortcut.into();
        assert!(config.validate().is_err(), "{shortcut}");
    }
    config.shortcut = "Control+Shift+R".into();
    config.rename_shortcut = config.shortcut.clone();
    assert!(config.validate().is_err());
}

#[test]
fn older_settings_show_taskbar_icon_without_resetting_preferences() {
    let mut config = Settings::default();
    config.max_ordinary = 100;
    config.shortcut = "Control+Shift+V".into();
    config.retain_search_input = false;
    let mut value = serde_json::to_value(config).unwrap();
    value.as_object_mut().unwrap().remove("showTaskbarIcon");
    let restored: Settings = serde_json::from_value(value).unwrap();
    assert!(restored.show_taskbar_icon);
    assert_eq!(restored.max_ordinary, 100);
    assert_eq!(restored.shortcut, "Control+Shift+V");
    assert!(!restored.retain_search_input);
}

#[test]
fn taskbar_icon_setting_round_trips_both_values() {
    assert!(Settings::default().show_taskbar_icon);
    for show in [true, false] {
        let mut config = Settings::default();
        config.show_taskbar_icon = show;
        let value = serde_json::to_value(config).unwrap();
        assert_eq!(value["showTaskbarIcon"], show);
        let restored: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(restored.show_taskbar_icon, show);
        assert!(restored.validate().is_ok());
    }
}

#[test]
fn older_settings_default_to_not_pinned() {
    let mut value = serde_json::to_value(Settings::default()).unwrap();
    value.as_object_mut().unwrap().remove("alwaysOnTop");
    let restored: Settings = serde_json::from_value(value).unwrap();
    assert!(!restored.always_on_top);
}

#[test]
fn older_settings_default_to_retaining_search_without_resetting_preferences() {
    let mut config = Settings::default();
    config.max_ordinary = 100;
    config.shortcut = "Control+Shift+V".into();
    let mut value = serde_json::to_value(config).unwrap();
    value.as_object_mut().unwrap().remove("retainSearchInput");
    let restored: Settings = serde_json::from_value(value).unwrap();
    assert!(restored.retain_search_input);
    assert_eq!(restored.max_ordinary, 100);
    assert_eq!(restored.shortcut, "Control+Shift+V");
}

#[test]
fn search_retention_setting_round_trips_both_values() {
    assert!(Settings::default().retain_search_input);
    for retain in [true, false] {
        let mut config = Settings::default();
        config.retain_search_input = retain;
        let value = serde_json::to_value(config).unwrap();
        assert_eq!(value["retainSearchInput"], retain);
        let restored: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(restored.retain_search_input, retain);
        assert!(restored.validate().is_ok());
    }
}
