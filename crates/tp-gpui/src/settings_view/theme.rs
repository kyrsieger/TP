use super::*;

// Order computed by the original sortThemes (CIEDE2000 nearest palette,
// light themes before dark themes). Keep aligned with resources/themes.json.
const THEMES: [(&str, &str, &str); 11] = [
    ("indigo", "靛蓝", "Indigo"),
    ("ice", "靛青", "Ice"),
    ("forest-mist", "墨绿", "Forest mist"),
    ("olive", "橄榄", "Olive"),
    ("oat", "燕麦", "Oat"),
    ("clay", "陶土", "Clay"),
    ("berry", "莓灰", "Berry"),
    ("violet", "暮紫", "Violet"),
    ("graphite", "石墨", "Graphite"),
    ("forest-night", "森林", "Forest night"),
    ("charcoal", "炭黑", "Charcoal"),
];
fn strip(id: &str, trigger: bool) -> impl IntoElement {
    let p = Palette::for_id(id);
    let mut colors = [
        p.accent, p.bg, p.surface, p.control, p.soft, p.text, p.muted,
    ];
    // CIELAB lightness is monotonic in relative luminance (D65).
    let luminance = |color: &gpui::Rgba| {
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126729 * linear(color.r) + 0.7151522 * linear(color.g) + 0.072175 * linear(color.b)
    };
    colors.sort_by(|a, b| luminance(a).total_cmp(&luminance(b)));
    div()
        .flex()
        .w(px(if trigger { 56. } else { 175. }))
        .min_w(px(if trigger { 56. } else { 98. }))
        .h(px(if trigger { 10. } else { 14. }))
        .rounded(px(3.))
        .overflow_hidden()
        .children(colors.map(|color| div().flex_1().min_w_0().h_full().bg(color)))
}
impl SettingsView {
    pub(crate) fn theme_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use gpui::Focusable;
        if !self.theme_open
            || self.pending.is_some()
            || self.cleanup.is_some()
            || self.transfer.is_some()
        {
            return false;
        }
        let handles = self.toggle_focus.borrow();
        let index = (0..=THEMES.len()).find(|i| {
            let key = if *i == 0 {
                "theme-trigger".into()
            } else {
                format!("theme-option-{}", i - 1)
            };
            handles.get(&key).is_some_and(|f| f.is_focused(window))
        });
        let next = match (index, reverse) {
            (Some(i), false) if i < THEMES.len() => Some(i + 1),
            (Some(i), true) if i > 0 => Some(i - 1),
            (None, true) if self.numbers[0].read(cx).focus_handle(cx).is_focused(window) => {
                Some(THEMES.len())
            }
            _ => None,
        };
        if let Some(next) = next {
            let key = if next == 0 {
                "theme-trigger".into()
            } else {
                format!("theme-option-{}", next - 1)
            };
            if let Some(focus) = handles.get(&key) {
                focus.focus(window);
            }
            drop(handles);
            if next > 0 {
                self.theme_scroll.scroll_to_item(next - 1);
                self.choose_theme(THEMES[next - 1].0, window, cx);
            }
            cx.notify();
            return true;
        }
        drop(handles);
        if index == Some(THEMES.len()) && !reverse {
            self.numbers[0].update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
            return true;
        }
        false
    }
    pub(super) fn choose_theme(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            self.queued_theme.push(id);
        } else if self.value.theme != id {
            self.save(Patch::Theme(id.into()), None, window, cx);
        }
    }
    pub(super) fn close_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_open = false;
        if let Some(focus) = self.toggle_focus.borrow().get("theme-trigger") {
            focus.focus(window);
        }
        cx.notify();
    }
    fn theme_keys(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if !matches!(key, "up" | "down" | "home" | "end") {
            return;
        }
        if !self.theme_open && !matches!(key, "up" | "down") {
            return;
        }
        if self.pending.is_some() || self.cleanup.is_some() || self.transfer.is_some() {
            return;
        }
        let current = (0..THEMES.len())
            .find(|i| {
                self.toggle_focus
                    .borrow()
                    .get(&format!("theme-option-{i}"))
                    .is_some_and(|f| f.is_focused(window))
            })
            .unwrap_or_else(|| {
                THEMES
                    .iter()
                    .position(|t| t.0 == self.value.theme)
                    .unwrap_or(0)
            });
        let next = match key {
            "home" => 0,
            "end" => THEMES.len() - 1,
            "up" => (current + THEMES.len() - 1) % THEMES.len(),
            _ => (current + 1) % THEMES.len(),
        };
        self.theme_open = true;
        self.language_open = false;
        self.retention_open = false;
        self.font_picker = None;
        self.font_subscription = None;
        if let Some(focus) = self
            .toggle_focus
            .borrow()
            .get(&format!("theme-option-{next}"))
        {
            focus.focus(window);
        }
        self.theme_scroll.scroll_to_item(next);
        self.choose_theme(THEMES[next].0, window, cx);
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }
    pub(super) fn theme_control(
        &self,
        disabled: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry("theme-trigger".into())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        for i in 0..THEMES.len() {
            self.toggle_focus
                .borrow_mut()
                .entry(format!("theme-option-{i}"))
                .or_insert_with(|| cx.focus_handle());
        }
        let current = THEMES
            .iter()
            .find(|t| t.0 == self.value.theme)
            .unwrap_or(&THEMES[0]);
        let menu_bounds =
            std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::<gpui::Pixels>::default()));
        let outside = menu_bounds.clone();
        let mut control = div()
            .id("theme-control")
            .relative()
            .flex_shrink_0()
            .on_key_down(cx.listener(Self::theme_keys))
            .on_mouse_down_out(
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if this.theme_open && !outside.get().contains(&event.position) {
                        this.theme_open = false;
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .id("theme-trigger")
                    .track_focus(&focus.clone().tab_index(0).tab_stop(!disabled))
                    .h(px(28.))
                    .px(px(9.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if self.theme_open {
                        p.accent
                    } else {
                        p.control_border
                    })
                    .bg(if self.theme_open {
                        p.surface
                    } else {
                        p.control
                    })
                    .hover(|d| d.bg(p.hover))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .text_size(px(13. * scale))
                    .when(disabled, |d| d.opacity(0.55))
                    .when(!disabled, |d| d.cursor_pointer())
                    .child(strip(current.0, true))
                    .child(choose(current.1, current.2))
                    .child(crate::icons::icon("chevron", 12.).text_color(p.subtle))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if disabled {
                            return;
                        }
                        if this.theme_open {
                            this.close_theme(window, cx);
                        } else {
                            this.theme_open = true;
                            this.language_open = false;
                            this.retention_open = false;
                            this.font_picker = None;
                            this.font_subscription = None;
                            cx.notify();
                        }
                    })),
            );
        if self.theme_open {
            let mut menu = div()
                .id("theme-menu")
                .w_full()
                .max_h(
                    (window.viewport_size().height - px(250.))
                        .min(px(352.))
                        .max(px(30.)),
                )
                .p(px(5.))
                .border_1()
                .border_color(p.line)
                .rounded(px(9.))
                .bg(p.surface)
                .shadow(vec![Palette::popup_shadow()])
                .occlude()
                .overflow_y_scroll()
                .track_scroll(&self.theme_scroll);
            for (index, &(id, zh, en)) in THEMES.iter().enumerate() {
                let focus = self
                    .toggle_focus
                    .borrow()
                    .get(&format!("theme-option-{index}"))
                    .unwrap()
                    .clone();
                menu = menu.child(
                    div()
                        .id(("theme-option", index))
                        .track_focus(&focus.tab_index(0).tab_stop(true))
                        .h(px(30.))
                        .px(px(9.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.))
                        .text_size(px(12. * scale))
                        .text_color(p.muted)
                        .cursor_pointer()
                        .when(index == 7, |d| {
                            d.mt(px(4.)).border_t_1().border_color(p.line)
                        })
                        .when(self.value.theme == id, |d| {
                            d.bg(p.soft).text_color(p.selected)
                        })
                        .hover(|d| d.bg(p.soft).text_color(p.selected))
                        .focus(|d| d.bg(p.soft).text_color(p.selected))
                        .child(choose(zh, en))
                        .child(strip(id, false))
                        .on_hover(cx.listener(move |this, hover, window, cx| {
                            if *hover {
                                this.choose_theme(id, window, cx);
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.choose_theme(id, window, cx);
                            this.close_theme(window, cx);
                        })),
                );
            }
            control = control.child(
                gpui::deferred(
                    div()
                        .absolute()
                        .top(px(33.))
                        .right_0()
                        .w(px(268.))
                        .max_w((window.viewport_size().width - px(64.)).max(px(98.)))
                        .child(menu)
                        // Keep the bounds probe outside the scroll content: GPUI includes
                        // absolute children in its content extent, then adds padding.
                        .child(
                            gpui::canvas(
                                move |bounds, _, _| menu_bounds.set(bounds),
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .child(
                            div().absolute().inset_0().child(
                                gpui_component::scroll::Scrollbar::vertical(&self.theme_scroll)
                                    .id("theme-scrollbar")
                                    .thin(px(7.))
                                    .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                            ),
                        ),
                )
                .priority(8),
            );
        }
        control
    }
}
