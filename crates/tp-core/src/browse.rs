//! Browsing keeps the original record order supplied by storage.
use crate::records::Record;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSession {
    pub view: String,
    pub kind_filter: String,
    pub search: String,
}

impl Default for PanelSession {
    fn default() -> Self {
        Self {
            view: "default".into(),
            kind_filter: "all".into(),
            search: String::new(),
        }
    }
}

impl PanelSession {
    /// Clear only the browsing fields whose retention options are disabled.
    pub fn clear_hidden(&mut self, retain_search: bool, retain_filter: bool) {
        if !retain_search {
            self.search.clear();
        }
        if !retain_filter {
            self.kind_filter = "all".into();
        }
    }
    pub fn filtered<'a>(&self, records: &'a [Record]) -> Vec<&'a Record> {
        let needle = self.search.to_lowercase();
        records
            .iter()
            .filter(|record| {
                if self.view == "favorite" && !record.favorite {
                    return false;
                }
                if let Some(group) = self.view.strip_prefix("group:") {
                    if group
                        .parse::<i64>()
                        .ok()
                        .is_none_or(|id| record.group_id != Some(id))
                    {
                        return false;
                    }
                }
                if self.view == "default"
                    && self.kind_filter != "all"
                    && record.kind != self.kind_filter
                {
                    return false;
                }
                let paths = record
                    .files
                    .iter()
                    .map(|file| file.original.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{} {} {}", record.title, record.body, paths)
                    .to_lowercase()
                    .contains(&needle)
            })
            .collect()
    }
}

/// Removes only trailing Unicode whitespace, without modifying persisted originals.
pub fn text_for_use(body: &str, remove_trailing: bool) -> &str {
    if remove_trailing {
        body.trim_end_matches(char::is_whitespace)
    } else {
        body
    }
}

pub fn first_line_text(body: &str) -> String {
    let mut lines = body.split(['\r', '\n']);
    let first = lines.next().unwrap_or_default();
    let has_more = lines.any(|line| !line.trim().is_empty());
    match (first.is_empty(), has_more) {
        (true, true) => "...".into(),
        (true, false) => crate::i18n::choose("空文本", "Empty text").into(),
        (false, true) => format!("{first} ..."),
        (false, false) => first.into(),
    }
}

/// List summaries never replace originals used by search or clipboard writeback.
pub fn record_list_text(record: &Record, search: &str, remove_trailing: bool) -> String {
    let body = text_for_use(&record.body, remove_trailing);
    let filenames = record
        .files
        .iter()
        .map(|file| file.original.rsplit(['/', '\\']).next().unwrap_or_default())
        .collect::<Vec<_>>();
    let usual = match filenames.len() {
        0 => first_line_text(body),
        1 => {
            if filenames[0].is_empty() {
                crate::i18n::choose("文件", "File").into()
            } else {
                filenames[0].into()
            }
        }
        count => crate::format_message!(
            "{} 个文件 · {}",
            "{} files · {}",
            count,
            filenames.join(crate::i18n::choose("、", ", "))
        ),
    };
    if search.is_empty() {
        return usual;
    }
    let needle = search.to_lowercase();
    std::iter::once(body)
        .chain(record.files.iter().map(|file| file.original.as_str()))
        .find(|source| source.to_lowercase().contains(&needle))
        .map(str::to_owned)
        .unwrap_or(usual)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hidden_retention_choices_are_independent_and_preserve_the_view() {
        for search in [false, true] {
            for filter in [false, true] {
                let mut session = super::PanelSession {
                    view: "group:42".into(),
                    search: "query".into(),
                    kind_filter: "image".into(),
                };
                session.clear_hidden(search, filter);
                assert_eq!(session.search, if search { "query" } else { "" });
                assert_eq!(session.kind_filter, if filter { "image" } else { "all" });
                assert_eq!(session.view, "group:42");
                session.clear_hidden(search, filter);
                assert_eq!(session.search, if search { "query" } else { "" });
            }
        }
    }
    use super::*;
    use crate::records::FileItem;

    fn record(id: &str, kind: &str) -> Record {
        Record {
            id: id.into(),
            kind: kind.into(),
            body: "正文".into(),
            title: String::new(),
            files: vec![],
            favorite: false,
            pinned_at: None,
            group_id: None,
            used_at: 0,
            created_at: None,
            expires_at: None,
            storage_mode: String::new(),
            status: "ready".into(),
            preview: false,
            asset_path: None,
        }
    }

    #[test]
    fn type_filter_only_applies_to_default_view() {
        let mut image = record("image", "image");
        image.favorite = true;
        image.group_id = Some(7);
        let records = vec![record("text", "text"), image];
        let mut session = PanelSession {
            kind_filter: "text".into(),
            ..Default::default()
        };
        assert_eq!(session.filtered(&records)[0].id, "text");
        session.view = "favorite".into();
        assert_eq!(session.filtered(&records)[0].id, "image");
        session.view = "group:7".into();
        assert_eq!(session.filtered(&records)[0].id, "image");
        session.view = "group:invalid".into();
        assert!(session.filtered(&records).is_empty());
    }

    #[test]
    fn searches_original_title_body_and_paths_without_reordering() {
        let mut first = record("first", "file");
        first.title = "项目 Project".into();
        first.files.push(FileItem {
            original: "C:\\Work\\计划.PDF".into(),
            stored: None,
            is_directory: false,
        });
        let mut second = record("second", "text");
        second.body = "PROJECT 正文".into();
        let records = vec![first, second];
        let mut session = PanelSession {
            search: "project".into(),
            ..Default::default()
        };
        assert_eq!(
            session
                .filtered(&records)
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        session.search = "计划.pdf".into();
        assert_eq!(session.filtered(&records)[0].id, "first");
    }

    #[test]
    fn trailing_cleanup_preserves_indentation_internal_lines_and_zero_width_space() {
        let original = "  缩进\n正文\u{200b}\u{00a0}\t\r\n";
        assert_eq!(text_for_use(original, true), "  缩进\n正文\u{200b}");
        assert_eq!(text_for_use(original, false), original);
    }

    #[test]
    fn list_text_retains_first_line_rules_and_reveals_path_search_matches() {
        assert_eq!(first_line_text("  first\r\nsecond"), "  first ...");
        assert_eq!(first_line_text("first\r\n \n"), "first");
        assert_eq!(first_line_text("\nsecond"), "...");
        let mut item = record("file", "file");
        item.files.push(FileItem {
            original: "C:\\Projects\\计划.pdf".into(),
            stored: None,
            is_directory: false,
        });
        item.title = "项目".into();
        assert_eq!(record_list_text(&item, "", true), "计划.pdf");
        assert_eq!(
            record_list_text(&item, "PROJECTS", true),
            "C:\\Projects\\计划.pdf"
        );
        assert_eq!(record_list_text(&item, "项目", true), "计划.pdf");
    }
}
