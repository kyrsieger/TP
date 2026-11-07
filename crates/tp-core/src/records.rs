use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
    pub original: String,
    pub stored: Option<String>,
    #[serde(default)]
    pub is_directory: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub body: String,
    pub title: String,
    pub files: Vec<FileItem>,
    pub favorite: bool,
    pub pinned_at: Option<i64>,
    pub group_id: Option<i64>,
    pub used_at: i64,
    pub created_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub storage_mode: String,
    pub status: String,
    pub preview: bool,
    /// 直接复制的图片没有原文件，其“原始路径”使用本工具保存的图片副本路径。
    pub asset_path: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: i64,
    pub name: String,
}

/// Clipboard bitmap records have no original file list. Their saved bitmap is
/// the path shown in preview; file-backed records retain their original paths.
pub fn preview_paths<'a>(files: &'a [FileItem], asset_path: Option<&'a str>) -> Vec<&'a str> {
    if files.is_empty() {
        asset_path
            .filter(|path| !path.is_empty())
            .into_iter()
            .collect()
    } else {
        files.iter().map(|file| file.original.as_str()).collect()
    }
}

pub fn file_tag(files: &[FileItem]) -> String {
    use crate::i18n::choose;
    let extension = |file: &FileItem| {
        if file.is_directory {
            return choose("目录", "Folder").to_owned();
        }
        let name = file.original.rsplit(['/', '\\']).next().unwrap_or("");
        match name.rfind('.') {
            Some(dot) if dot > 0 && dot + 1 < name.len() => name[dot + 1..].to_uppercase(),
            _ => choose("文件", "File").to_owned(),
        }
    };
    let Some(first) = files.first().map(extension) else {
        return choose("文件", "File").into();
    };
    if files.iter().skip(1).all(|file| extension(file) == first) {
        first
    } else {
        choose("多格式", "Mixed types").into()
    }
}

#[cfg(test)]
mod tag_tests {
    use super::*;
    #[test]
    fn preview_paths_use_saved_bitmap_only_without_original_files() {
        assert_eq!(
            preview_paths(&[], Some("/cache/image.png")),
            vec!["/cache/image.png"]
        );
        assert!(preview_paths(&[], None).is_empty());
        assert!(preview_paths(&[], Some("")).is_empty());
        let files = [FileItem {
            original: "C:\\原图.png".into(),
            stored: Some("/cache/copy.png".into()),
            is_directory: false,
        }];
        assert_eq!(
            preview_paths(&files, Some("/cache/image.png")),
            vec!["C:\\原图.png"]
        );
    }
    #[test]
    fn tags_follow_original_filename_rules_on_both_platforms() {
        let file = |name: &str| FileItem {
            original: name.into(),
            stored: None,
            is_directory: false,
        };
        assert_eq!(
            file_tag(&[file("C:\\folder\\report.pdf"), file("/tmp/book.PDF")]),
            "PDF"
        );
        assert_eq!(file_tag(&[file("/tmp/archive.tar.gz")]), "GZ");
        assert_eq!(
            file_tag(&[file(".gitignore")]),
            crate::i18n::choose("文件", "File")
        );
        assert_eq!(
            file_tag(&[file("ending.")]),
            crate::i18n::choose("文件", "File")
        );
        assert_eq!(
            file_tag(&[file("a.txt"), file("a.pdf")]),
            crate::i18n::choose("多格式", "Mixed types")
        );
    }
}
