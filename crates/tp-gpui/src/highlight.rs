use crate::theme::Palette;
use gpui::{HighlightStyle, StyledText};

/// Measure after flex layout, using the same font run that will paint the text.
pub fn excerpt(
    body: String,
    query: String,
    palette: Palette,
    font_size: f32,
    line_height: f32,
) -> impl gpui::IntoElement {
    use gpui::{canvas, div, prelude::*, px, size, AvailableSpace};
    let single_line = body.replace("\r\n", " ").replace(['\r', '\n', '\t'], " ");
    let prepared = tp_core::search_excerpt::PreparedExcerpt::new(single_line, &query);
    canvas(
        move |bounds, window, cx| {
            let style = window.text_style();
            let fitted = prepared.fit(f32::from(bounds.size.width), |candidate| {
                let run = style.to_run(candidate.len());
                f32::from(
                    window
                        .text_system()
                        .shape_line(candidate.to_owned().into(), px(font_size), &[run], None)
                        .width,
                )
            });
            let mut element = div()
                .w(bounds.size.width)
                .h(px(line_height))
                .overflow_hidden()
                .text_size(px(font_size))
                .line_height(px(line_height))
                .child(text(fitted, &query, palette))
                .into_any_element();
            element.prepaint_as_root(
                bounds.origin,
                size(
                    AvailableSpace::Definite(bounds.size.width),
                    AvailableSpace::Definite(bounds.size.height),
                ),
                window,
                cx,
            );
            element
        },
        |_, mut element, window, cx| {
            element.paint(window, cx);
        },
    )
    .w_full()
    .h(px(line_height))
}

pub fn text(text: String, query: &str, palette: Palette) -> StyledText {
    let ranges = tp_core::search_highlight::ranges(&text, query);
    StyledText::new(text).with_highlights(ranges.into_iter().map(|range| {
        (
            range,
            HighlightStyle {
                color: Some(palette.button_text.into()),
                background_color: Some(palette.accent.into()),
                ..Default::default()
            },
        )
    }))
}
