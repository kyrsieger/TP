use super::*;
fn retention_label(days: u16) -> String {
    match days {
        1 => choose("24 小时", "24 hours").into(),
        30 => choose("1 个月", "1 month").into(),
        _ => format!("{days} {}", choose("天", "days")),
    }
}
impl SettingsView {
    // The popup is painted after the form, so its automatic tab insertion order
    // need not match the original inline DOM order. Keep this small span explicit.
    pub(crate) fn retention_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use gpui::Focusable;
        if !self.retention_open
            || self.busy
            || self.pending.is_some()
            || self.cleanup.is_some()
            || self.transfer.is_some()
        {
            return false;
        }
        let keys = [
            "retention-trigger",
            "retention-option-1",
            "retention-option-3",
            "retention-option-5",
            "retention-option-7",
            "retention-option-30",
        ];
        let handles = self.toggle_focus.borrow();
        let index = keys.iter().position(|key| {
            handles
                .get(*key)
                .is_some_and(|handle| handle.is_focused(window))
        });
        let sensitive_focused = self.numbers[2].read(cx).focus_handle(cx).is_focused(window);
        let target = match (index, reverse) {
            (Some(index), false) if index < keys.len() - 1 => Some(index + 1),
            (Some(index), true) if index > 0 => Some(index - 1),
            (None, true) if sensitive_focused => Some(keys.len() - 1),
            _ => None,
        };
        if let Some(target) = target.and_then(|index| handles.get(keys[index]).cloned()) {
            drop(handles);
            target.focus(window);
            cx.notify();
            return true;
        }
        drop(handles);
        if index == Some(keys.len() - 1) && !reverse {
            self.numbers[2].update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn retention_control(
        &self,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry("retention-trigger".into())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let menu_bounds =
            std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::<gpui::Pixels>::default()));
        let outside_bounds = menu_bounds.clone();
        let mut control = div()
            .id("retention-control")
            .relative()
            .flex_shrink_0()
            .on_mouse_down_out(
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if this.retention_open && !outside_bounds.get().contains(&event.position) {
                        this.retention_open = false;
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .id("retention-trigger")
                    .track_focus(&focus.clone().tab_index(0).tab_stop(!disabled))
                    .w(px(110.))
                    .h(px(28.))
                    .px(px(9.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if self.retention_open {
                        p.accent
                    } else {
                        p.control_border
                    })
                    .bg(if self.retention_open {
                        p.surface
                    } else {
                        p.control
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .relative()
                    .text_size(px(13. * scale))
                    .when(disabled, |d| d.opacity(0.55))
                    .when(!disabled, |d| d.cursor_pointer())
                    .child(retention_label(self.value.retention_days))
                    .child(
                        div()
                            .absolute()
                            .right(px(9.))
                            .child(crate::icons::icon("chevron", 12.).text_color(p.subtle)),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !disabled {
                            focus.focus(window);
                            this.retention_open = !this.retention_open;
                            cx.notify();
                        }
                    })),
            );
        if self.retention_open && !disabled {
            let mut menu = div()
                .id("retention-menu")
                .absolute()
                .top(px(33.))
                .right_0()
                .w(px(150.))
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
            for days in [1, 3, 5, 7, 30] {
                let selected = self.value.retention_days == days;
                let option_focus = self
                    .toggle_focus
                    .borrow_mut()
                    .entry(format!("retention-option-{days}"))
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                menu = menu.child(
                    div()
                        .id(("retention-option", days as usize))
                        .track_focus(&option_focus.clone().tab_index(0).tab_stop(true))
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
                        .child(retention_label(days))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(focus) = this.toggle_focus.borrow().get("retention-trigger")
                            {
                                focus.focus(window);
                            }
                            this.retention_open = false;
                            this.save(Patch::Retention(days), None, window, cx);
                            cx.notify();
                        })),
                );
            }
            control = control.child(gpui::deferred(menu).priority(5));
        }
        control
    }
}
