use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct FontStyle {
    pub name: String,
    pub family: String,
    pub weight: u16,
    pub stretch: u16,
    pub style: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}
impl Default for FontStyle {
    fn default() -> Self {
        Self {
            name: "Regular".into(),
            family: String::new(),
            weight: 400,
            stretch: 100,
            style: "normal".into(),
            aliases: Vec::new(),
        }
    }
}
pub fn valid_name(value: &str) -> bool {
    value.chars().count() <= 256 && !value.chars().any(char::is_control)
}
impl FontStyle {
    pub fn valid(&self) -> bool {
        valid_name(&self.name)
            && valid_name(&self.family)
            && (1..=1000).contains(&self.weight)
            && (50..=200).contains(&self.stretch)
            && ["normal", "italic", "oblique"].contains(&self.style.as_str())
            && self.aliases.len() <= 64
            && self.aliases.iter().all(|name| valid_name(name))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FontFamily {
    pub name: String,
    pub aliases: Vec<String>,
    pub styles: Vec<FontStyle>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FontSelection {
    pub size: u16,
    pub family: String,
    pub style: FontStyle,
}

impl FontFamily {
    pub fn default_style(&self) -> FontStyle {
        self.styles
            .iter()
            .find(|s| s.name == "Regular")
            .or_else(|| {
                self.styles.iter().min_by_key(|s| {
                    s.weight.abs_diff(400) as u32
                        + s.stretch.abs_diff(100) as u32 * 10
                        + if s.style == "normal" { 0 } else { 1000 }
                })
            })
            .cloned()
            .unwrap_or_default()
    }
}
pub fn resolve(fonts: &[FontFamily], family: &str, style: &FontStyle) -> (String, FontStyle) {
    let group = fonts
        .iter()
        .find(|f| f.name == family)
        .or_else(|| fonts.iter().find(|f| f.aliases.iter().any(|a| a == family)));
    let Some(group) = group else {
        return (family.into(), style.clone());
    };
    let exact = group.styles.iter().find(|s| {
        s.name == style.name
            && s.family == style.family
            && s.weight == style.weight
            && s.stretch == style.stretch
            && s.style == style.style
    });
    let legacy = if group.name != family {
        group
            .styles
            .iter()
            .find(|s| s.family == family || s.aliases.iter().any(|a| a == family))
    } else {
        None
    };
    (
        group.name.clone(),
        exact
            .or(legacy)
            .cloned()
            .unwrap_or_else(|| group.default_style()),
    )
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn legacy_alias_resolves_real_style_and_missing_families_are_preserved() {
        let light = FontStyle {
            name: "Light".into(),
            family: "微软雅黑 Light".into(),
            weight: 290,
            aliases: vec!["Microsoft YaHei Light".into()],
            ..Default::default()
        };
        let fonts = vec![FontFamily {
            name: "微软雅黑".into(),
            aliases: vec!["Microsoft YaHei Light".into()],
            styles: vec![
                light.clone(),
                FontStyle {
                    family: "微软雅黑".into(),
                    ..Default::default()
                },
            ],
        }];
        assert_eq!(
            resolve(&fonts, "Microsoft YaHei Light", &FontStyle::default()),
            ("微软雅黑".into(), light.clone())
        );
        assert_eq!(fonts[0].default_style().name, "Regular");
        assert_eq!(
            resolve(&fonts, "Missing", &light),
            ("Missing".into(), light)
        );
    }
}
