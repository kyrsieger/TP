use super::common::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct FontStyle {
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
pub(super) fn valid_name(value: &str) -> bool {
    value.chars().count() <= 256 && !value.chars().any(char::is_control)
}
impl FontStyle {
    pub(super) fn valid(&self) -> bool {
        valid_name(&self.name)
            && valid_name(&self.family)
            && (1..=1000).contains(&self.weight)
            && (50..=200).contains(&self.stretch)
            && ["normal", "italic", "oblique"].contains(&self.style.as_str())
            && self.aliases.len() <= 64
            && self.aliases.iter().all(|name| valid_name(name))
    }
}
#[derive(Serialize)]
pub(super) struct FontFamily {
    pub name: String,
    pub aliases: Vec<String>,
    pub styles: Vec<FontStyle>,
}
#[derive(Serialize)]
pub(super) struct FontSelection {
    pub size: u16,
    pub family: String,
    pub style: FontStyle,
}

fn group_fonts(faces: Vec<(String, FontStyle, Vec<String>)>) -> Vec<FontFamily> {
    let mut groups: BTreeMap<String, FontFamily> = BTreeMap::new();
    for (name, mut face, aliases) in faces {
        if name.is_empty() || name.starts_with(['@', '.']) || !valid_name(&name) || !face.valid() {
            continue;
        }
        if ["normal", "roman", "regular"].contains(&face.name.to_lowercase().as_str())
            && face.weight == 400
            && face.style == "normal"
            && face.stretch == 100
        {
            face.name = "Regular".into();
        }
        let group = groups.entry(name.clone()).or_insert_with(|| FontFamily {
            name,
            aliases: Vec::new(),
            styles: Vec::new(),
        });
        group.aliases.extend(aliases);
        group.aliases.extend(face.aliases.iter().cloned());
        if let Some(existing) = group.styles.iter_mut().find(|existing| {
            existing.name == face.name
                && existing.weight == face.weight
                && existing.stretch == face.stretch
                && existing.style == face.style
        }) {
            existing.aliases.extend(face.aliases);
            existing.aliases.sort();
            existing.aliases.dedup();
        } else {
            group.styles.push(face);
        }
    }
    for group in groups.values_mut() {
        group.aliases = std::mem::take(&mut group.aliases)
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        group.styles.sort_by_key(|face| {
            (
                face.name != "Regular",
                face.weight,
                face.stretch,
                face.style.clone(),
                face.name.clone(),
            )
        });
    }
    groups.into_values().collect()
}
#[tauri::command]
pub(super) fn get_system_fonts() -> Result<Vec<FontFamily>> {
    let groups = group_fonts(platform_fonts()?);
    if groups.is_empty() {
        return Err("未能读取系统字体，请重试".into());
    }
    Ok(groups)
}

#[cfg(windows)]
fn platform_fonts() -> Result<Vec<(String, FontStyle, Vec<String>)>> {
    use super::common::err;
    use windows::{
        core::{w, BOOL},
        Win32::Graphics::DirectWrite::*,
    };
    unsafe fn text(strings: &IDWriteLocalizedStrings, index: u32) -> windows::core::Result<String> {
        let length = strings.GetStringLength(index)?;
        let mut buffer = vec![0u16; length as usize + 1];
        strings.GetString(index, &mut buffer)?;
        Ok(String::from_utf16_lossy(&buffer[..length as usize]))
    }
    unsafe fn localized(
        strings: &IDWriteLocalizedStrings,
        chinese: bool,
    ) -> windows::core::Result<String> {
        for locale in if chinese {
            [w!("zh-cn"), w!("en-us")]
        } else {
            [w!("en-us"), w!("zh-cn")]
        } {
            let mut index = 0;
            let mut exists = BOOL::default();
            strings.FindLocaleName(locale, &mut index, &mut exists)?;
            if exists.as_bool() {
                return text(strings, index);
            }
        }
        text(strings, 0)
    }
    unsafe fn info(
        font: &IDWriteFont,
        kind: DWRITE_INFORMATIONAL_STRING_ID,
    ) -> windows::core::Result<Option<IDWriteLocalizedStrings>> {
        let mut strings = None;
        let mut exists = BOOL::default();
        font.GetInformationalStrings(kind, &mut strings, &mut exists)?;
        Ok(if exists.as_bool() { strings } else { None })
    }
    unsafe fn names(strings: &IDWriteLocalizedStrings) -> windows::core::Result<Vec<String>> {
        (0..strings.GetCount())
            .map(|index| text(strings, index))
            .collect()
    }
    unsafe fn enumerate() -> windows::core::Result<Vec<(String, FontStyle, Vec<String>)>> {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let mut collection = None;
        factory.GetSystemFontCollection(&mut collection, true)?;
        let collection = collection.unwrap();
        let mut faces = Vec::new();
        for index in 0..collection.GetFontFamilyCount() {
            let family = collection.GetFontFamily(index)?;
            let family_names = family.GetFamilyNames()?;
            for index in 0..family.GetFontCount() {
                let font = family.GetFont(index)?;
                if font.GetSimulations() != DWRITE_FONT_SIMULATIONS_NONE {
                    continue;
                }
                let typographic =
                    info(&font, DWRITE_INFORMATIONAL_STRING_TYPOGRAPHIC_FAMILY_NAMES)?
                        .unwrap_or_else(|| family_names.clone());
                let legacy = info(&font, DWRITE_INFORMATIONAL_STRING_WIN32_FAMILY_NAMES)?
                    .unwrap_or_else(|| family_names.clone());
                let face_names = info(
                    &font,
                    DWRITE_INFORMATIONAL_STRING_TYPOGRAPHIC_SUBFAMILY_NAMES,
                )?
                .unwrap_or(font.GetFaceNames()?);
                let stretch = [100, 50, 63, 75, 88, 100, 113, 125, 150, 200]
                    [font.GetStretch().0.clamp(0, 9) as usize];
                let face = FontStyle {
                    name: localized(&face_names, false)?,
                    family: localized(&legacy, true)?,
                    weight: font.GetWeight().0.clamp(1, 1000) as u16,
                    stretch,
                    style: match font.GetStyle() {
                        DWRITE_FONT_STYLE_ITALIC => "italic",
                        DWRITE_FONT_STYLE_OBLIQUE => "oblique",
                        _ => "normal",
                    }
                    .into(),
                    aliases: names(&legacy)?,
                };
                faces.push((localized(&typographic, true)?, face, names(&typographic)?));
            }
        }
        Ok(faces)
    }
    unsafe { enumerate().map_err(err) }
}

#[cfg(target_os = "macos")]
fn platform_fonts() -> Result<Vec<(String, FontStyle, Vec<String>)>> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSFontManager, NSFontTraitMask};
    use objc2_foundation::{NSNumber, NSString};
    let mtm = MainThreadMarker::new().ok_or("读取系统字体需要在主线程执行")?;
    let manager = NSFontManager::sharedFontManager(mtm);
    let mut faces = Vec::new();
    for family in manager.availableFontFamilies().iter() {
        let Some(members) = manager.availableMembersOfFontFamily(&family) else {
            continue;
        };
        for member in members.iter() {
            if member.count() < 4 {
                continue;
            }
            let (postscript, label, weight, traits) = (
                member.objectAtIndex(0),
                member.objectAtIndex(1),
                member.objectAtIndex(2),
                member.objectAtIndex(3),
            );
            let (Some(postscript), Some(label), Some(weight), Some(traits)) = (
                postscript.downcast_ref::<NSString>(),
                label.downcast_ref::<NSString>(),
                weight.downcast_ref::<NSNumber>(),
                traits.downcast_ref::<NSNumber>(),
            ) else {
                continue;
            };
            let traits = NSFontTraitMask(traits.unsignedIntegerValue());
            let weight = [
                100, 100, 200, 300, 400, 400, 500, 600, 600, 700, 800, 800, 900, 900, 950, 1000,
            ][weight.integerValue().clamp(0, 15) as usize];
            let stretch = if traits.intersects(
                NSFontTraitMask::CondensedFontMask
                    | NSFontTraitMask::NarrowFontMask
                    | NSFontTraitMask::CompressedFontMask,
            ) {
                75
            } else if traits.contains(NSFontTraitMask::ExpandedFontMask) {
                125
            } else {
                100
            };
            faces.push((
                family.to_string(),
                FontStyle {
                    name: label.to_string(),
                    family: postscript.to_string(),
                    weight,
                    stretch,
                    style: if traits.contains(NSFontTraitMask::ItalicFontMask) {
                        "italic"
                    } else {
                        "normal"
                    }
                    .into(),
                    aliases: vec![postscript.to_string()],
                },
                vec![family.to_string()],
            ));
        }
    }
    Ok(faces)
}
#[cfg(not(any(windows, target_os = "macos")))]
fn platform_fonts() -> Result<Vec<(String, FontStyle, Vec<String>)>> {
    Err("当前平台不支持读取系统字体".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grouping_uses_metadata_and_keeps_real_styles() {
        let regular = FontStyle {
            family: "Example".into(),
            ..Default::default()
        };
        let light = FontStyle {
            name: "Light".into(),
            family: "Example Light".into(),
            weight: 300,
            ..regular.clone()
        };
        let groups = group_fonts(vec![
            ("Example".into(), regular.clone(), vec!["Example".into()]),
            ("Example".into(), light, vec!["Example Light".into()]),
            ("Example".into(), regular.clone(), vec![]),
            ("Unrelated Light".into(), regular, vec![]),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0]
                .styles
                .iter()
                .map(|style| style.name.as_str())
                .collect::<Vec<_>>(),
            ["Regular", "Light"]
        );
        assert!(groups[0].aliases.contains(&"Example Light".into()));
    }
    #[cfg(windows)]
    #[test]
    fn windows_groups_yahei_light_with_regular_and_bold() {
        let fonts = get_system_fonts().unwrap();
        let yahei = fonts
            .iter()
            .find(|font| font.name == "微软雅黑" || font.name == "Microsoft YaHei")
            .unwrap();
        assert!(!fonts
            .iter()
            .any(|font| font.name == "微软雅黑 Light" || font.name == "Microsoft YaHei Light"));
        assert!(yahei
            .styles
            .iter()
            .any(|style| style.name == "Light" && style.weight < 400));
        for weight in [400, 700] {
            assert!(
                yahei.styles.iter().any(|style| style.weight == weight),
                "missing weight {weight}"
            );
        }
        assert!(yahei.styles.iter().any(|style| style.name == "Regular"));
        assert!(fonts.windows(2).all(|pair| pair[0].name < pair[1].name));
    }
}
