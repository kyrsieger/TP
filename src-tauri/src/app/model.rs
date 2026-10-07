use super::common::Result;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Settings {
    pub(super) max_ordinary: u16,
    pub(super) retention_days: u16,
    pub(super) sensitive_seconds: u32,
    pub(super) shortcut: String,
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
    #[serde(default = "default_show_taskbar_icon")]
    pub(super) show_taskbar_icon: bool,
}

fn default_show_taskbar_icon() -> bool {
    true
}

fn default_retain_search_input() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_ordinary: 300,
            retention_days: 7,
            sensitive_seconds: 90,
            shortcut: String::new(),
            position_mode: "fixed".into(),
            fixed_position: "right".into(),
            remember_position: false,
            panel_width: 420.0,
            panel_height: 600.0,
            panel_x: None,
            panel_y: None,
            always_on_top: false,
            retain_search_input: true,
            show_taskbar_icon: true,
        }
    }
}

impl Settings {
    pub(super) fn validate(&self) -> Result<()> {
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
