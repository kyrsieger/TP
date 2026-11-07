use crate::fonts::FontFamily;
use pinyin_pro::{
    options::{NonZh, PatternKind, PinyinOptions, PinyinOutput, ToneType, TypeMode},
    pinyin,
};
use std::collections::{HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

fn normalize(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace())
        .collect()
}
fn terms(name: &str) -> Vec<String> {
    let mut result = vec![normalize(name)];
    for pattern in [PatternKind::First, PatternKind::Pinyin] {
        if let PinyinOutput::Arr(parts) = pinyin(
            name,
            PinyinOptions {
                pattern,
                tone_type: ToneType::None,
                type_mode: TypeMode::Array,
                non_zh: NonZh::Consecutive,
                ..Default::default()
            },
        ) {
            result.push(normalize(&parts.join("")));
        }
    }
    result.sort();
    result.dedup();
    result
}
pub struct FontSearchIndex(Vec<(String, Vec<String>)>);
impl FontSearchIndex {
    pub fn new(fonts: &[FontFamily], selected: &str) -> Self {
        let mut entries = vec![(
            String::new(),
            [terms("系统默认"), terms("System default")].concat(),
        )];
        let mut seen = HashSet::from([String::new()]);
        let mut cache = HashMap::new();
        for font in fonts {
            if !seen.insert(font.name.clone()) {
                continue;
            }
            let mut prepared = Vec::new();
            for name in std::iter::once(&font.name).chain(font.aliases.iter()) {
                prepared.extend(
                    cache
                        .entry(name.clone())
                        .or_insert_with(|| terms(name))
                        .iter()
                        .cloned(),
                );
            }
            prepared.sort();
            prepared.dedup();
            entries.push((font.name.clone(), prepared));
        }
        if !selected.is_empty() && seen.insert(selected.into()) {
            entries.push((selected.into(), terms(selected)));
        }
        Self(entries)
    }
    pub fn search(&self, query: &str) -> Vec<String> {
        let needle = normalize(query);
        self.0
            .iter()
            .filter(|(_, terms)| needle.is_empty() || terms.iter().any(|t| t.contains(&needle)))
            .map(|(name, _)| name.clone())
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn family(name: &str, aliases: &[&str]) -> FontFamily {
        FontFamily {
            name: name.into(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            styles: vec![],
        }
    }
    #[test]
    fn original_search_cases_preserve_aliases_pinyin_context_and_order() {
        let fonts = vec![
            family("霞鹜文楷", &["LXGW WenKai"]),
            family("微软雅黑", &["Microsoft YaHei"]),
            family("Arial", &[]),
            family("华文行楷", &[]),
        ];
        let index = FontSearchIndex::new(&fonts, "未安装字体");
        for q in [
            "XW",
            "XWWK",
            "wk",
            "xiawu",
            "xiawuwenkai",
            "霞鹜",
            "文楷",
            "LXGW   WENKAI",
            "ＸＷ",
            " x w ",
        ] {
            assert_eq!(index.search(q), vec!["霞鹜文楷"], "{q}");
        }
        for q in ["WRYH", "weiruan", "MICROSOFT YAHEI"] {
            assert_eq!(index.search(q), vec!["微软雅黑"]);
        }
        assert_eq!(index.search("XTMR"), vec![""]);
        assert_eq!(index.search("WAZZT"), vec!["未安装字体"]);
        assert_eq!(
            index.search("  "),
            vec![
                "",
                "霞鹜文楷",
                "微软雅黑",
                "Arial",
                "华文行楷",
                "未安装字体"
            ]
        );
        assert!(index.search("does-not-exist").is_empty());
        let context = FontSearchIndex::new(
            &[
                family("重庆字体", &[]),
                family("华文行楷", &[]),
                family("LXGW 霞鹜文楷 GB", &[]),
                family("霞雾文楷", &[]),
            ],
            "",
        );
        assert_eq!(context.search("CQZT"), vec!["重庆字体"]);
        assert_eq!(context.search("HWXK"), vec!["华文行楷"]);
        assert_eq!(context.search("LXGWXWWKGB"), vec!["LXGW 霞鹜文楷 GB"]);
        assert_eq!(
            context.search("xiawu"),
            vec!["LXGW 霞鹜文楷 GB", "霞雾文楷"]
        );
    }
}
