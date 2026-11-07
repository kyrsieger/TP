use super::*;

pub(super) fn full_text_tooltip(
    text: String,
) -> impl Fn(&mut Window, &mut gpui::App) -> gpui::AnyView {
    move |window, cx| {
        // Leave room for the tooltip's border, padding and outer margins.
        let width = (window.viewport_size().width - px(48.)).max(px(1.));
        let text = text.clone();
        gpui_component::tooltip::Tooltip::element(move |_, _| {
            div().max_w(width).whitespace_normal().child(text.clone())
        })
        .build(window, cx)
    }
}

pub(super) fn marker(
    record: &tp_core::records::Record,
    selected: bool,
    scale: f32,
    p: Palette,
) -> Option<gpui::AnyElement> {
    if record.kind == "text" {
        return None;
    }
    if record.kind == "file" {
        let base = p.file_tag_bg;
        let mixed = gpui::Rgba {
            r: base.r * 0.78 + p.accent.r * 0.22,
            g: base.g * 0.78 + p.accent.g * 0.22,
            b: base.b * 0.78 + p.accent.b * 0.22,
            a: 1.,
        };
        return Some(
            div()
                .id(gpui::SharedString::from(format!("file-tag:{}", record.id)))
                .tooltip(full_text_tooltip(tp_core::records::file_tag(&record.files)))
                .flex_shrink_0()
                .max_w(px(52.))
                .px(px(5.))
                .py(px(2.))
                .rounded(px(4.))
                .bg(if selected { mixed } else { base })
                .text_color(p.file_tag_text)
                .text_size(px(10. * scale))
                .line_height(px(14. * scale))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .truncate()
                .child(tp_core::records::file_tag(&record.files))
                .into_any_element(),
        );
    }
    if record.kind == "color" && tp_core::digest::is_hex_color(&record.body) {
        if let Ok(color) = gpui::Rgba::try_from(record.body.trim()) {
            return Some(
                div()
                    .size(px(12.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(color)
                    .into_any_element(),
            );
        }
    }
    Some(
        crate::icons::icon(&record.kind, 12.)
            .text_color(if record.kind == "link" {
                p.link
            } else {
                p.subtle
            })
            .into_any_element(),
    )
}
