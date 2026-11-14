use super::*;
use std::time::{Duration, Instant};

const SEARCH_DURATION: Duration = Duration::from_millis(180);
const SEARCH_BUTTON_WIDTH: f32 = 30.;
pub(super) const HEIGHT: f32 = 52.;

#[derive(Default)]
pub(super) struct SearchReveal {
    expanded: bool,
    amount: f32,
    transition: Option<(Instant, f32)>,
}

impl SearchReveal {
    fn advance(&mut self, now: Instant) -> bool {
        let Some((started, from)) = self.transition else {
            return false;
        };
        let progress = (now.saturating_duration_since(started).as_secs_f32()
            / SEARCH_DURATION.as_secs_f32())
        .min(1.);
        let target = if self.expanded { 1. } else { 0. };
        self.amount = from + (target - from) * (1. - (1. - progress).powi(3));
        if progress >= 1. {
            self.transition = None;
        }
        self.transition.is_some()
    }

    pub(super) fn set_expanded(&mut self, expanded: bool) {
        if self.expanded == expanded {
            return;
        }
        let now = Instant::now();
        self.advance(now);
        self.expanded = expanded;
        if crate::native_window::reduced_motion() {
            self.reset(expanded);
        } else {
            self.transition = Some((now, self.amount));
        }
    }

    pub(super) fn reset(&mut self, expanded: bool) {
        self.expanded = expanded;
        self.amount = if expanded { 1. } else { 0. };
        self.transition = None;
    }

    pub(super) fn expanded(&self) -> bool {
        self.expanded
    }
}

impl Panel {
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.move_menu.is_some()
            || self.group_edit.is_some()
            || self.record_edit.is_some()
            || self.settings_view.is_some()
        {
            return;
        }
        self.filter_open = false;
        self.group_menu = None;
        self.record_menu = None;
        self.search_reveal.set_expanded(true);
        self.search.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn search_control(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if self.search_reveal.advance(Instant::now()) {
            window.request_animation_frame();
        }
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let available = self.settings_view.is_none();
        let amount = if available {
            self.search_reveal.amount
        } else {
            0.
        };
        let expanded = available && self.search_reveal.expanded;
        let visible = expanded || amount > 0.;
        let focus = self.search.read(cx).focus_handle(cx);
        let focused = focus.contains_focused(window, cx);
        let has_query = !self.session.search.is_empty();
        div()
            .id("toolbar-search-area")
            .relative()
            .flex_1()
            .min_w_0()
            .h(px(30.))
            .child(
                div()
                    .size_full()
                    .pr(px(SEARCH_BUTTON_WIDTH + 10.))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .opacity(1. - amount)
                    .child(crate::icons::icon("tp-mark", 16.).text_color(gpui::rgb(0x123F35)))
                    .child(
                        div()
                            .text_size(px(11. * self.snapshot.settings.font_size as f32 / 13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(p.subtle)
                            .child("TP"),
                    ),
            )
            .when(available, |area| {
                area.child(
                    div()
                        .id("search-box")
                        .absolute()
                        // Keep the search icon anchored while revealing the field to its left.
                        .right_0()
                        .top_0()
                        .h(px(30.))
                        .w(gpui::relative(amount))
                        .min_w(px(SEARCH_BUTTON_WIDTH))
                        .max_w_full()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(if focused && visible {
                            p.accent
                        } else {
                            gpui::rgba(0x00000000)
                        })
                        .overflow_hidden()
                        .when(visible, |d| {
                            d.bg(if focused { p.surface } else { p.control })
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .when(visible, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .left(px(9.))
                                    .right(px(if has_query { 55. } else { 34. }))
                                    .top_0()
                                    .h(px(28.))
                                    .overflow_hidden()
                                    .opacity(amount)
                                    .child({
                                        let mut input = Input::new(&self.search)
                                            .appearance(false)
                                            .p_0()
                                            .text_size(px(12.
                                                * self.snapshot.settings.font_size as f32
                                                / 13.));
                                        input.style().size.height = Some(px(28.).into());
                                        input
                                    }),
                            )
                            .when(has_query, |d| {
                                d.child(
                                    div()
                                        .id("clear-search")
                                        .absolute()
                                        .right(px(32.))
                                        .top(px(5.))
                                        .size(px(18.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .opacity(amount)
                                        .hover(|d| d.bg(p.soft))
                                        .tooltip(|window, cx| {
                                            gpui_component::tooltip::Tooltip::new(choose(
                                                "清空搜索",
                                                "Clear search",
                                            ))
                                            .build(window, cx)
                                        })
                                        .child(crate::icons::icon("x", 13.).text_color(p.subtle))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.search.update(cx, |input, cx| {
                                                input.set_value("", window, cx);
                                                input.focus(window, cx);
                                            });
                                        })),
                                )
                            })
                        })
                        .child(
                            div()
                                .id("open-search")
                                .absolute()
                                .right_0()
                                .top_0()
                                .w(px(28.))
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(7.))
                                .cursor_pointer()
                                .hover(|d| d.bg(p.soft))
                                .tooltip(|window, cx| {
                                    gpui_component::tooltip::Tooltip::new(choose(
                                        "搜索 · Ctrl+F",
                                        "Search · Ctrl+F",
                                    ))
                                    .build(window, cx)
                                })
                                .child(crate::icons::icon("search", 14.).text_color(p.subtle))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_search(window, cx);
                                })),
                        ),
                )
            })
    }

    pub(super) fn toolbar_close(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_id(&self.snapshot.settings.theme);
        div()
            .id("hide-panel")
            .group("hide-panel")
            .size(px(28.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .hover(|d| d.bg(gpui::rgb(0xE81123)))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new(choose("隐藏面板", "Hide panel"))
                    .build(window, cx)
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| this.hide(window, cx)))
            .child(
                crate::icons::icon("x", 14.)
                    .text_color(p.subtle)
                    .group_hover("hide-panel", |d| d.text_color(gpui::rgb(0xFFFFFF))),
            )
    }
}
