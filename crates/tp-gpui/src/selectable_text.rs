//! Plain text selection uses byte offsets into the original, never a display copy.
use crate::theme::Palette;
use gpui::{
    div, prelude::*, ClipboardItem, Context, FocusHandle, HighlightStyle, MouseButton, StyledText,
    TextLayout, Window,
};
use std::ops::Range;

pub struct SelectableText {
    text: String,
    display: crate::text_selection::DisplayText,
    query: String,
    palette: Palette,
    focus: FocusHandle,
    layout: Option<TextLayout>,
    anchor: usize,
    end: usize,
    dragging: bool,
    drag_unit: crate::text_selection::Unit,
    drag_origin: Range<usize>,
    filename: Option<Range<usize>>,
    search_scroll: Option<gpui::ScrollHandle>,
    search_width: std::rc::Rc<std::cell::Cell<f32>>,
    _blur: gpui::Subscription,
}

fn segments(text: &str, query: &str, selection: Range<usize>) -> Vec<(Range<usize>, bool)> {
    let hits = tp_core::search_highlight::ranges(text, query);
    let mut boundaries = vec![0, text.len(), selection.start, selection.end];
    for hit in &hits {
        boundaries.extend([hit.start, hit.end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
        .windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            let selected = selection.contains(&range.start);
            (selected || hits.iter().any(|hit| hit.contains(&range.start)))
                .then_some((range, selected))
        })
        .collect()
}

impl SelectableText {
    pub fn new(
        text: String,
        query: String,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.end = this.anchor;
            this.dragging = false;
            cx.notify();
        });
        Self {
            display: crate::text_selection::DisplayText::new(&text),
            text,
            query,
            palette,
            focus,
            _blur: blur,
            layout: None,
            anchor: 0,
            end: 0,
            dragging: false,
            drag_unit: Default::default(),
            drag_origin: 0..0,
            filename: None,
            search_scroll: None,
            search_width: Default::default(),
        }
    }
    pub fn reveal_search_in(mut self, scroll: gpui::ScrollHandle) -> Self {
        self.search_scroll = Some(scroll);
        self
    }
    pub fn file_path(
        text: String,
        query: String,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let start = text.rfind(['/', '\\']).map_or(0, |index| index + 1);
        let end = text.len();
        let mut view = Self::new(text, query, palette, window, cx);
        // Match PreviewCard's final path component. A trailing separator has
        // no filename; keep the whole directory path in the muted style.
        view.filename = Some(start..end);
        view
    }
    fn drag_to(&mut self, position: gpui::Point<gpui::Pixels>) {
        (self.anchor, self.end) = crate::text_selection::extend(
            &self.text,
            self.drag_origin.clone(),
            self.index(position),
            self.drag_unit,
        );
    }
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.end)..self.anchor.max(self.end)
    }
    fn index(&self, position: gpui::Point<gpui::Pixels>) -> usize {
        let Some(layout) = &self.layout else {
            return 0;
        };
        let mut index = self.display.source_index(
            layout
                .index_for_position(position)
                .unwrap_or_else(|index| index),
        );
        while !self.text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }
}

impl_view_render!(SelectableText);

impl SelectableText {
    #[cfg_attr(all(debug_assertions, feature = "hot-reload"), inline(never))]
    fn render_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let p = self.palette;
        let highlights = segments(&self.text, &self.query, self.selection());
        let mut boundaries = vec![0, self.text.len()];
        for (range, _) in &highlights {
            boundaries.extend([range.start, range.end]);
        }
        if let Some(range) = &self.filename {
            boundaries.extend([range.start, range.end]);
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let text = StyledText::new(self.display.text.clone()).with_highlights(
            boundaries.windows(2).filter_map(|pair| {
                let range = pair[0]..pair[1];
                let mark = highlights
                    .iter()
                    .find(|(mark, _)| mark.contains(&range.start))
                    .map(|(_, selected)| *selected);
                let filename = self
                    .filename
                    .as_ref()
                    .is_some_and(|name| name.contains(&range.start));
                if mark.is_none() && self.filename.is_none() {
                    return None;
                }
                let color = match mark {
                    Some(false) => p.button_text,
                    Some(true) if cfg!(target_os = "macos") => gpui::rgb(0x000000),
                    Some(true) => gpui::rgb(0xFFFFFF),
                    None if filename => p.text,
                    None => p.muted,
                };
                let background_color = mark.map(|selected| {
                    if !selected {
                        p.accent
                    } else if cfg!(target_os = "macos") {
                        gpui::rgb(0xB3D7FF)
                    } else {
                        gpui::rgb(0x0078D7)
                    }
                    .into()
                });
                Some((
                    self.display.display_range(range),
                    HighlightStyle {
                        color: Some(color.into()),
                        background_color,
                        font_weight: filename.then_some(gpui::FontWeight::SEMIBOLD),
                        ..Default::default()
                    },
                ))
            }),
        );
        self.layout = Some(text.layout().clone());
        let search_target = self.search_scroll.clone().and_then(|scroll| {
            tp_core::search_highlight::ranges(&self.text, &self.query)
                .first()
                .map(|hit| (scroll, self.display.display_range(hit.clone()).start))
        });
        let layout = text.layout().clone();
        let search_width = self.search_width.clone();
        div()
            .id("selectable-text")
            .relative()
            .track_focus(&self.focus)
            .cursor_text()
            .child(text)
            .when_some(search_target, |element, (scroll, index)| {
                element.child(
                    gpui::canvas(
                        move |_, window, cx| {
                            let width = f32::from(layout.bounds().size.width);
                            if width <= 0. || search_width.replace(width) == width {
                                return;
                            }
                            if let Some(position) = layout.position_for_index(index) {
                                let viewport = scroll.bounds();
                                // Coordinates include the current scroll offset. Undo it
                                // before centering, so a resize cannot compound scrolling.
                                let y = position.y
                                    - viewport.top()
                                    - scroll.offset().y
                                    - viewport.size.height / 2.;
                                let scroll = scroll.clone();
                                window.defer(cx, move |window, _| {
                                    scroll.set_offset(gpui::point(
                                        gpui::px(0.),
                                        -y.max(gpui::px(0.)),
                                    ));
                                    window.refresh();
                                });
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.focus.focus(window);
                    use crate::text_selection::{span, Unit};
                    let index = this.index(event.position);
                    this.drag_unit = match event.click_count {
                        2 => Unit::Word,
                        3.. => Unit::Line,
                        _ => Unit::Character,
                    };
                    if event.modifiers.shift && event.click_count == 1 {
                        this.drag_origin = this.anchor..this.anchor;
                        this.end = index;
                    } else {
                        this.drag_origin = span(&this.text, index, this.drag_unit);
                        this.anchor = this.drag_origin.start;
                        this.end = this.drag_origin.end;
                    }
                    this.dragging = true;
                    if std::env::var_os("TP_DIAGNOSTIC_SELECTION").is_some() {
                        eprintln!("selection down {}", this.end);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.dragging {
                    this.drag_to(event.position);
                    if std::env::var_os("TP_DIAGNOSTIC_SELECTION").is_some() {
                        eprintln!("selection move {}..{}", this.anchor, this.end);
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                    if this.dragging {
                        this.drag_to(event.position);
                        this.dragging = false;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                    if this.dragging {
                        this.drag_to(event.position);
                        this.dragging = false;
                        cx.notify();
                    }
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let copy_modifier = if cfg!(target_os = "macos") {
                    modifiers.platform
                } else {
                    modifiers.control
                };
                if event.keystroke.key == "c" && copy_modifier && !modifiers.alt && !modifiers.shift
                {
                    let range = this.selection();
                    if std::env::var_os("TP_DIAGNOSTIC_SELECTION").is_some() {
                        eprintln!("selection copy {range:?}");
                    }
                    if !range.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            this.text[range].to_owned(),
                        ));
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_unicode_search_and_selection_keep_original_byte_boundaries() {
        let text = "甲乙\r\n😀乙";
        let result = segments(text, "乙", 3..12);
        assert_eq!(result, vec![(3..6, true), (6..12, true), (12..15, false)]);
        assert_eq!(&text[3..12], "乙\r\n😀");
        for (range, _) in result {
            assert!(text.get(range).is_some());
        }
    }
}
