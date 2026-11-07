//! Desktop-framework-independent clipboard history types and browsing rules.
pub mod browse;
pub mod cleanup;
pub mod common;
pub mod digest;
pub mod filesystem;
pub mod font_search;
pub mod fonts;
pub mod group_drag;
pub mod hover_preview_geometry;
pub mod i18n;
pub mod list_layout;
pub mod migration;
pub mod operations;
pub mod panel_geometry;
pub mod preferences;
pub mod records;
pub mod search_excerpt;
pub mod search_highlight;
pub mod selection;
pub mod settings;
pub mod storage;
pub mod store;
pub mod transfer;

#[cfg(test)]
mod test_support;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub records: Vec<records::Record>,
    pub groups: Vec<records::Group>,
    pub settings: settings::Settings,
}
