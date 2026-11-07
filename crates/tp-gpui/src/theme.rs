//! Exact base palette values from the existing panel themes.
use gpui::{rgb, Rgba};

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    fn native_base_colors_match_all_original_theme_roles() {
        // Use the original UI's independent theme definitions as the migration
        // reference, rather than repeating the native palette in this test.
        let reference: std::collections::BTreeMap<String, Vec<String>> =
            serde_json::from_str(include_str!("../resources/themes.json")).unwrap();
        let mut count = 0;
        for (id, colors) in &reference {
            let expected: Vec<_> = colors
                .iter()
                .map(|hex| rgb(u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap()))
                .collect();
            let palette = Palette::for_id(id);
            let actual = [
                palette.accent,
                palette.bg,
                palette.surface,
                palette.control,
                palette.soft,
                palette.selected,
                palette.text,
                palette.muted,
                palette.line,
                palette.button_text,
            ];
            assert_eq!(
                expected.len(),
                actual.len(),
                "reference format changed: {id}"
            );
            for ((actual, expected), role) in actual.iter().zip(&expected).zip([
                "accent",
                "background",
                "surface",
                "control",
                "selected background",
                "selected text",
                "text",
                "muted text",
                "border",
                "button text",
            ]) {
                assert_eq!(actual, expected, "theme {id}, role {role}");
            }
            count += 1;
        }
        assert_eq!(
            count, 11,
            "review theme coverage when the reference changes"
        );
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub bg: Rgba,
    pub surface: Rgba,
    pub control: Rgba,
    pub soft: Rgba,
    pub selected: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub line: Rgba,
    pub button_text: Rgba,
    pub hover: Rgba,
    pub soft_hover: Rgba,
    pub disabled: Rgba,
    pub group_soft: Rgba,
    pub group_text: Rgba,
    pub group_idle: Rgba,
    pub subtle: Rgba,
    pub star: Rgba,
    pub warning: Rgba,
    pub indicator_bg: Rgba,
    pub indicator_dot: Rgba,
    pub indicator_active: Rgba,
    pub danger: Rgba,
    pub danger_bg: Rgba,
    pub danger_bg_hover: Rgba,
    pub on_danger: Rgba,
    pub danger_muted: Rgba,
    pub link: Rgba,
    pub file_tag_bg: Rgba,
    pub file_tag_text: Rgba,
    pub control_border: Rgba,
    pub separator: Rgba,
}
impl Palette {
    pub fn popup_shadow() -> gpui::BoxShadow {
        gpui::BoxShadow {
            color: gpui::Rgba {
                a: 0.14,
                ..rgb(0x171C28)
            }
            .into(),
            offset: gpui::point(gpui::px(0.), gpui::px(12.)),
            blur_radius: gpui::px(28.),
            spread_radius: gpui::px(0.),
        }
    }

    pub fn apply(settings: &tp_core::settings::Settings, cx: &mut gpui::App) {
        use gpui_component::{Theme, ThemeMode};
        let dark =
            ["forest-night", "graphite", "violet", "charcoal"].contains(&settings.theme.as_str());
        Theme::change(
            if dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            None,
            cx,
        );
        let p = Self::for_id(&settings.theme);
        let theme = Theme::global_mut(cx);
        theme.background = p.bg.into();
        theme.foreground = p.text.into();
        theme.input = p.control.into();
        theme.border = p.control_border.into();
        theme.muted = p.control.into();
        theme.muted_foreground = p.muted.into();
        theme.caret = p.accent.into();
        theme.selection = p.soft.into();
        theme.primary = p.accent.into();
        theme.primary_hover = p.accent_hover.into();
        let scrollbar = if settings.theme == "indigo" {
            rgb(0xD4D9E4)
        } else {
            p.line
        };
        theme.scrollbar = gpui::transparent_black();
        theme.scrollbar_thumb = scrollbar.into();
        theme.scrollbar_thumb_hover = scrollbar.into();
        theme.primary_foreground = p.button_text.into();
        theme.font_size = gpui::px(settings.font_size as f32);
        theme.font_family = font_family(settings).into();
    }

    pub fn for_id(id: &str) -> Self {
        let colors = match id {
            "indigo" => [
                0x4F46E5, 0xFFFFFF, 0xFFFFFF, 0xF3F4F8, 0xEEF0FF, 0x312E81, 0x171C28, 0x5B6472,
                0xEEF0F5, 0xFFFFFF,
            ],
            "forest-mist" => [
                0x3D755D, 0xF4F7F4, 0xFBFCFA, 0xE9EFEA, 0xDFEBE2, 0x28513D, 0x22332A, 0x5A6C61,
                0xD8E2DA, 0xFFFFFF,
            ],
            "forest-night" => [
                0x93C6AB, 0x202C26, 0x29382F, 0x324338, 0x365B45, 0xE1F1E6, 0xE8EFE9, 0xBECCBF,
                0x445B4B, 0x183426,
            ],
            "oat" => [
                0x8F622D, 0xF8F5EF, 0xFDFBF7, 0xEFE9DF, 0xEFE2CE, 0x70491C, 0x382F23, 0x6F6352,
                0xE5DDCF, 0xFFFFFF,
            ],
            "graphite" => [
                0xA6BFF0, 0x22262D, 0x2A3039, 0x323A45, 0x354764, 0xDCE8FF, 0xE7EBF2, 0xABB6C6,
                0x424C5B, 0x1C2A40,
            ],
            "clay" => [
                0xA35D43, 0xF8F0E8, 0xFCF7F1, 0xF0E4D8, 0xF0DCCF, 0x7C412E, 0x3D2C25, 0x745E53,
                0xE7D7CC, 0xFFFFFF,
            ],
            "olive" => [
                0x70733F, 0xF4F3E8, 0xFAFAF2, 0xE9E9D9, 0xE3E5CD, 0x50552B, 0x323629, 0x626753,
                0xDADCC8, 0xFFFFFF,
            ],
            "ice" => [
                0x3C6881, 0xEEF4F7, 0xF8FCFD, 0xE0EAF0, 0xD3E6F0, 0x274C63, 0x233A46, 0x4D6874,
                0xCDDDE5, 0xFFFFFF,
            ],
            "berry" => [
                0x8A5670, 0xF7F0F4, 0xFCF8FA, 0xEEE2E9, 0xEAD6E1, 0x6C3B53, 0x3F2C37, 0x725868,
                0xE2CFDA, 0xFFFFFF,
            ],
            "violet" => [
                0xC5B2E3, 0x292532, 0x342F40, 0x413A50, 0x514366, 0xEFE5FF, 0xECE6F4, 0xC0B4CF,
                0x574B64, 0x332840,
            ],
            "charcoal" => [
                0xE2B77A, 0x282522, 0x322E28, 0x3D372E, 0x584632, 0xF7DFC0, 0xEFE9DF, 0xC2B8A9,
                0x544B3E, 0x342719,
            ],
            _ => return Self::for_id("indigo"),
        };
        let [accent, bg, surface, control, soft, selected, text, muted, line, button_text] =
            colors.map(rgb);
        Self {
            accent,
            accent_hover: if id == "indigo" {
                rgb(0x4338CA)
            } else {
                // Original color-mix(in srgb, accent 88%, text).
                Rgba {
                    r: accent.r * 0.88 + text.r * 0.12,
                    g: accent.g * 0.88 + text.g * 0.12,
                    b: accent.b * 0.88 + text.b * 0.12,
                    a: 1.,
                }
            },
            bg,
            surface,
            control,
            soft,
            selected,
            text,
            muted,
            line,
            button_text,
            hover: if id == "indigo" {
                rgb(0xF6F7FB)
            } else {
                control
            },
            soft_hover: if id == "indigo" {
                rgb(0xE1E4FF)
            } else {
                Rgba {
                    r: soft.r * 0.85 + accent.r * 0.15,
                    g: soft.g * 0.85 + accent.g * 0.15,
                    b: soft.b * 0.85 + accent.b * 0.15,
                    a: 1.,
                }
            },
            disabled: if id == "indigo" { rgb(0xC9CEDB) } else { muted },
            group_soft: if id == "indigo" { rgb(0xE8EDFA) } else { soft },
            group_text: if id == "indigo" {
                rgb(0x5168BA)
            } else {
                selected
            },
            group_idle: if id == "indigo" { rgb(0x6B7385) } else { muted },
            subtle: if id == "indigo" { rgb(0x9AA2B1) } else { muted },
            link: if id == "indigo" {
                rgb(0x059669)
            } else {
                accent
            },
            file_tag_bg: if id == "indigo" { rgb(0xF5F3FF) } else { soft },
            file_tag_text: if id == "indigo" {
                rgb(0x7C3AED)
            } else {
                selected
            },
            danger_bg: rgb(if id == "indigo" { 0xc73737 } else { 0xb83e3e }),
            danger_bg_hover: rgb(if id == "indigo" { 0xad2727 } else { 0x9b3030 }),
            on_danger: rgb(0xffffff),
            danger: rgb(if id == "indigo" {
                0xEF4444
            } else if ["forest-night", "graphite", "violet", "charcoal"].contains(&id) {
                0xF09595
            } else {
                0xAD3D3D
            }),
            danger_muted: rgb(if id == "indigo" {
                0xB35B5B
            } else if ["forest-night", "graphite", "violet", "charcoal"].contains(&id) {
                0xF09595
            } else {
                0xAD3D3D
            }),
            indicator_bg: Rgba {
                a: if id == "indigo" { 0.78 } else { 0.85 },
                ..surface
            },
            indicator_dot: if id == "indigo" { rgb(0xB0BAC7) } else { muted },
            indicator_active: if id == "indigo" {
                rgb(0x8F9BAB)
            } else {
                accent
            },
            warning: rgb(if id == "indigo" {
                0xA16207
            } else if ["forest-night", "graphite", "violet", "charcoal"].contains(&id) {
                0xE5BC71
            } else {
                0x986513
            }),
            star: rgb(if id == "indigo" {
                0xF59E0B
            } else if ["forest-night", "graphite", "violet", "charcoal"].contains(&id) {
                0xE5BC71
            } else {
                0x986513
            }),
            separator: if id == "indigo" {
                Rgba {
                    a: 0.05,
                    ..rgb(0x171C28)
                }
            } else {
                line
            },
            control_border: if id == "indigo" { rgb(0xE5E7EE) } else { line },
        }
    }
}

/// Prefer the concrete face name, preserving legacy family names such as Light.
pub fn font_family(settings: &tp_core::settings::Settings) -> String {
    if settings.font_family.is_empty() {
        if cfg!(target_os = "macos") {
            "PingFang SC".into()
        } else {
            "Segoe UI".into()
        }
    } else if !settings.font_style.family.is_empty() {
        settings.font_style.family.clone()
    } else {
        settings.font_family.clone()
    }
}

/// The file-content block has its own default font stack in preview.css.
/// Resolve it once when the content arrives, rather than enumerating fonts per frame.
pub fn file_preview_font_family(
    settings: &tp_core::settings::Settings,
    window: &gpui::Window,
) -> String {
    if !settings.font_family.is_empty() {
        return font_family(settings);
    }
    #[cfg(target_os = "macos")]
    {
        // WebKit's ui-monospace uses the system monospaced design.
        let font = objc2_app_kit::NSFont::monospacedSystemFontOfSize_weight(12., 0.);
        if let Some(family) = font.familyName() {
            return family.to_string();
        }
    }
    let available = window.text_system().all_font_names();
    ["Cascadia Code", "Consolas", "Courier New"]
        .into_iter()
        .find(|candidate| {
            available
                .iter()
                .any(|name| name.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or("Courier New")
        .to_owned()
}

pub fn typography<T: gpui::Styled>(mut element: T, settings: &tp_core::settings::Settings) -> T {
    let custom = !settings.font_family.is_empty();
    let style = element.text_style().get_or_insert_with(Default::default);
    style.font_family = Some(font_family(settings).into());
    style.font_weight = Some(gpui::FontWeight(if custom {
        settings.font_style.weight as f32
    } else {
        400.
    }));
    style.font_style = Some(if !custom {
        gpui::FontStyle::Normal
    } else {
        match settings.font_style.style.as_str() {
            "italic" => gpui::FontStyle::Italic,
            "oblique" => gpui::FontStyle::Oblique,
            _ => gpui::FontStyle::Normal,
        }
    });
    element
}

/// Measure a panel label with the same font refinement used during painting.
pub fn label_width(
    window: &gpui::Window,
    settings: &tp_core::settings::Settings,
    text: &str,
    base_size: f32,
) -> gpui::Pixels {
    use gpui::{Refineable, Styled};
    let mut style = window.text_style();
    let mut sample = typography(gpui::div(), settings);
    if let Some(refinement) = sample.text_style().as_ref() {
        style.refine(refinement);
    }
    window
        .text_system()
        .shape_line(
            text.to_owned().into(),
            gpui::px(base_size * settings.font_size as f32 / 13.),
            &[style.to_run(text.len())],
            None,
        )
        .width
}
