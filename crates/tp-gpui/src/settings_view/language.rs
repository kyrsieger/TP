use super::*;
const OPTIONS: [(&str, &str); 3] = [("system", ""), ("zh-CN", "简体中文"), ("en", "English")];
fn language_label(id: &str) -> &'static str {
    match id {
        "zh-CN" => "简体中文",
        "en" => "English",
        _ => choose("系统默认", "System default"),
    }
}
fn language_icon(id: &str) -> impl IntoElement {
    let system = tp_core::i18n::state().system;
    let resolved = if id == "system" { system } else { id };
    div()
        .w(px(21.))
        .h(px(14.))
        .flex_shrink_0()
        .rounded(px(2.))
        .overflow_hidden()
        .child(
            gpui::img(if resolved == "zh-CN" {
                "icons/language-zh.svg"
            } else {
                "icons/language-en.svg"
            })
            .w_full()
            .h_full(),
        )
}
impl SettingsView {
    pub(super) fn watch_language_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = std::iter::once("language-trigger".to_owned())
            .chain((0..OPTIONS.len()).map(|index| format!("language-option-{index}")));
        for key in keys {
            let focus = self
                .toggle_focus
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| cx.focus_handle())
                .clone();
            self._subscriptions
                .push(cx.on_blur(&focus, window, |this, window, cx| {
                    if !this.language_open {
                        return;
                    }
                    let handles = this.toggle_focus.borrow();
                    let inside = handles
                        .get("language-trigger")
                        .is_some_and(|f| f.is_focused(window))
                        || (0..OPTIONS.len()).any(|index| {
                            handles
                                .get(&format!("language-option-{index}"))
                                .is_some_and(|f| f.is_focused(window))
                        });
                    drop(handles);
                    if !inside {
                        // Match LanguageSetting's onBlur, including shortcut-driven
                        // focus changes. Moving between its options keeps it open.
                        this.language_open = false;
                        cx.notify();
                    }
                }));
        }
    }

    pub(crate) fn leave_language_for_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.language_open {
            return;
        }
        let handles = self.toggle_focus.borrow();
        let option_focused = (0..OPTIONS.len()).any(|index| {
            handles
                .get(&format!("language-option-{index}"))
                .is_some_and(|focus| focus.is_focused(window))
        });
        // Options have tabindex=-1 in the original menu. Resume the form's
        // sequence at the trigger instead of the deferred popup's paint order.
        if option_focused {
            if let Some(focus) = handles.get("language-trigger") {
                focus.focus(window);
            }
        }
        self.language_open = false;
        cx.notify();
    }

    fn open_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.cleanup.is_some() || self.transfer.is_some()
        {
            return;
        }
        self.theme_open = false;
        self.language_open = true;
        self.retention_open = false;
        self.font_picker = None;
        self.font_subscription = None;
        let index = OPTIONS
            .iter()
            .position(|(id, _)| *id == self.value.language)
            .unwrap_or(0);
        if let Some(focus) = self
            .toggle_focus
            .borrow()
            .get(&format!("language-option-{index}"))
        {
            focus.focus(window);
        }
        cx.notify();
    }
    fn language_keys(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.has_modal() || event.keystroke.modifiers.modified() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if !matches!(key, "up" | "down" | "home" | "end") {
            return;
        }
        if !self.language_open {
            if matches!(key, "up" | "down") {
                self.open_language(window, cx);
            } else {
                return;
            }
        } else {
            let handles = self.toggle_focus.borrow();
            let current = (0..OPTIONS.len())
                .find(|index| {
                    handles
                        .get(&format!("language-option-{index}"))
                        .is_some_and(|focus| focus.is_focused(window))
                })
                .unwrap_or(0);
            let next = match key {
                "home" => 0,
                "end" => OPTIONS.len() - 1,
                "up" => (current + OPTIONS.len() - 1) % OPTIONS.len(),
                _ => (current + 1) % OPTIONS.len(),
            };
            if let Some(focus) = handles.get(&format!("language-option-{next}")) {
                focus.focus(window);
            }
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }
    pub(super) fn language_control(
        &self,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry("language-trigger".into())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let menu_bounds =
            std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::<gpui::Pixels>::default()));
        let outside_bounds = menu_bounds.clone();
        for index in 0..OPTIONS.len() {
            self.toggle_focus
                .borrow_mut()
                .entry(format!("language-option-{index}"))
                .or_insert_with(|| cx.focus_handle());
        }
        let mut control = div()
            .id("language-control")
            .on_key_down(cx.listener(Self::language_keys))
            .relative()
            .flex_shrink_0()
            .on_mouse_down_out(
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if this.language_open && !outside_bounds.get().contains(&event.position) {
                        this.language_open = false;
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .id("language-trigger")
                    .track_focus(&focus.clone().tab_index(0).tab_stop(!disabled))
                    .min_w(px(110.))
                    .h(px(28.))
                    .px(px(9.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if self.language_open {
                        p.accent
                    } else {
                        p.control_border
                    })
                    .bg(if self.language_open {
                        p.surface
                    } else {
                        p.control
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .relative()
                    .text_size(px(13. * scale))
                    .when(disabled, |d| d.opacity(0.55))
                    .when(!disabled, |d| d.cursor_pointer())
                    .child(language_icon(&self.value.language))
                    .child(language_label(&self.value.language))
                    .child(div().child(crate::icons::icon("chevron", 12.).text_color(p.subtle)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !disabled && !this.busy && !this.has_modal() {
                            if this.language_open {
                                this.language_open = false;
                                focus.focus(window);
                            } else {
                                this.open_language(window, cx);
                            }
                            cx.notify();
                        }
                    })),
            );
        if self.language_open && !disabled {
            let mut menu = div()
                .id("language-menu")
                .absolute()
                .top(px(33.))
                .right_0()
                .min_w(px(150.))
                .p(px(5.))
                .border_1()
                .border_color(p.line)
                .rounded(px(9.))
                .bg(p.surface)
                .shadow(vec![Palette::popup_shadow()])
                .occlude()
                .child(
                    gpui::canvas(move |bounds, _, _| menu_bounds.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                );
            for (index, (id, _)) in OPTIONS.iter().enumerate() {
                let id = *id;
                let selected = self.value.language == id;
                let option_focus = self
                    .toggle_focus
                    .borrow_mut()
                    .entry(format!("language-option-{index}"))
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                menu = menu.child(
                    div()
                        .id(("language-option", index))
                        .track_focus(&option_focus.clone().tab_index(0).tab_stop(false))
                        .focus(|d| d.bg(p.hover))
                        .h(px(30.))
                        .px(px(9.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .text_size(px(12. * scale))
                        .text_color(if selected { p.accent } else { p.muted })
                        .when(selected, |d| d.bg(p.soft))
                        .hover(|d| d.bg(p.hover))
                        .cursor_pointer()
                        .gap(px(8.))
                        .child(language_icon(id))
                        .child(language_label(id))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.busy || this.has_modal() {
                                return;
                            }
                            if let Some(focus) = this.toggle_focus.borrow().get("language-trigger")
                            {
                                focus.focus(window);
                            }
                            this.language_open = false;
                            this.save(Patch::Language(id.into()), None, window, cx);
                            cx.notify();
                        })),
                );
            }
            control = control.child(gpui::deferred(menu).priority(5));
        }
        control
    }
}
