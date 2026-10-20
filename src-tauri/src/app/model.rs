use super::common::Result;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Settings {
    #[serde(default = "default_theme", deserialize_with = "deserialize_theme")]
    pub(super) theme: String,
    #[serde(default)]
    pub(super) font_family: String,
    #[serde(default)]
    pub(super) font_style: super::fonts::FontStyle,
    pub(super) max_ordinary: u16,
    pub(super) retention_days: u16,
    pub(super) sensitive_seconds: u32,
    pub(super) shortcut: String,
    #[serde(default = "default_rename_shortcut")]
    pub(super) rename_shortcut: String,
    pub(super) position_mode: String,
    pub(super) fixed_position: String,
    pub(super) remember_position: bool,
    pub(super) panel_width: f64,
    pub(super) panel_height: f64,
    pub(super) panel_x: Option<f64>,
    pub(super) panel_y: Option<f64>,
    #[serde(default)]
    pub(super) always_on_top: bool,
    #[serde(default = "default_retain_search_input")]
    pub(super) retain_search_input: bool,
    #[serde(default = "default_retain_type_filter")]
    pub(super) retain_type_filter: bool,
    #[serde(default = "default_show_taskbar_icon")]
    pub(super) show_taskbar_icon: bool,
    #[serde(default)]
    pub(super) background_resident: bool,
    #[serde(default = "default_remove_trailing_newlines")]
    pub(super) remove_trailing_newlines: bool,
}

pub(super) const THEMES: [&str; 11] = [
    "indigo",
    "forest-mist",
    "forest-night",
    "oat",
    "graphite",
    "clay",
    "olive",
    "ice",
    "berry",
    "violet",
    "charcoal",
];
fn default_theme() -> String {
    "indigo".into()
}
fn deserialize_theme<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    let theme = String::deserialize(deserializer)?;
    Ok(if THEMES.contains(&theme.as_str()) {
        theme
    } else {
        default_theme()
    })
}

fn default_show_taskbar_icon() -> bool {
    true
}

fn default_rename_shortcut() -> String {
    "F2".into()
}

fn valid_rename_shortcut(shortcut: &str) -> bool {
    if shortcut.is_empty() {
        return true;
    }
    let parts: Vec<_> = shortcut.split('+').collect();
    let key = parts[parts.len() - 1];
    let mut previous = None;
    for part in &parts[..parts.len() - 1] {
        let Some(index) = ["Control", "Alt", "Shift", "Super"]
            .iter()
            .position(|name| name == part)
        else {
            return false;
        };
        if previous.is_some_and(|previous| index <= previous) {
            return false;
        }
        previous = Some(index);
    }
    let function_key = key
        .strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number) && key == format!("F{number}"));
    let modified_character = parts.len() > 1
        && key.len() == 1
        && (key.as_bytes()[0].is_ascii_uppercase() || key.as_bytes()[0].is_ascii_digit());
    (function_key || modified_character)
        && ![
            "Control+A",
            "Control+D",
            "Control+F",
            "Control+G",
            "Control+P",
            "Control+1",
            "Control+2",
            "Control+3",
            "Control+4",
            "Control+5",
            "Control+6",
            "Control+7",
            "Control+8",
            "Control+9",
        ]
        .contains(&shortcut)
}

fn default_retain_search_input() -> bool {
    true
}

fn default_retain_type_filter() -> bool {
    true
}

fn default_remove_trailing_newlines() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            font_family: String::new(),
            font_style: super::fonts::FontStyle::default(),
            max_ordinary: 300,
            retention_days: 7,
            sensitive_seconds: 90,
            shortcut: String::new(),
            rename_shortcut: default_rename_shortcut(),
            position_mode: "fixed".into(),
            fixed_position: "right".into(),
            remember_position: false,
            panel_width: 420.0,
            panel_height: 600.0,
            panel_x: None,
            panel_y: None,
            always_on_top: false,
            retain_search_input: true,
            retain_type_filter: true,
            show_taskbar_icon: true,
            background_resident: false,
            remove_trailing_newlines: true,
        }
    }
}

impl Settings {
    pub(super) fn validate(&self) -> Result<()> {
        if !THEMES.contains(&self.theme.as_str()) {
            return Err("无效的主题配色".into());
        }
        if !super::fonts::valid_name(&self.font_family) || !self.font_style.valid() {
            return Err("无效的字体名称".into());
        }
        if !valid_rename_shortcut(&self.rename_shortcut)
            || (!self.rename_shortcut.is_empty() && self.rename_shortcut == self.shortcut)
        {
            return Err("重命名快捷键无效或与其他操作冲突".into());
        }
        if !(1..=500).contains(&self.max_ordinary)
            || ![1, 3, 5, 7, 30].contains(&self.retention_days)
            || self.sensitive_seconds == 0
            || !["fixed", "cursor"].contains(&self.position_mode.as_str())
            || !["left", "right", "center"].contains(&self.fixed_position.as_str())
            || self.panel_width < 340.0
            || self.panel_height < 400.0
        {
            return Err("设置值超出允许范围".into());
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileItem {
    pub(super) original: String,
    pub(super) stored: Option<String>,
    #[serde(default)]
    pub(super) is_directory: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Record {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) body: String,
    pub(super) title: String,
    pub(super) files: Vec<FileItem>,
    pub(super) favorite: bool,
    pub(super) pinned_at: Option<i64>,
    pub(super) group_id: Option<i64>,
    pub(super) used_at: i64,
    pub(super) created_at: Option<i64>,
    pub(super) expires_at: Option<i64>,
    pub(super) storage_mode: String,
    pub(super) status: String,
    pub(super) preview: bool,
    /// 直接复制的图片没有原文件，其“原始路径”使用本工具保存的图片副本路径。
    pub(super) asset_path: Option<String>,
}

#[derive(Serialize)]
pub(super) struct Group {
    pub(super) id: i64,
    pub(super) name: String,
}

#[derive(Serialize)]
pub(super) struct Snapshot {
    pub(super) records: Vec<Record>,
    pub(super) groups: Vec<Group>,
    pub(super) settings: Settings,
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;
