use super::*;
fn position_label(id: &str) -> &'static str {
    match id {
        "left" => choose("左下角", "Bottom left"),
        "right" => choose("右下角", "Bottom right"),
        _ => choose("下方居中", "Bottom center"),
    }
}
impl SettingsView {
    pub(super) fn fixed_position_disclosure(
        &self,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let expanded = self.fixed_position_expanded;
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry("fixed-position-disclosure".into())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let label = if expanded {
            choose("收起固定位置设置", "Collapse fixed position settings")
        } else {
            choose("展开固定位置设置", "Expand fixed position settings")
        };
        let mut chevron = crate::icons::icon("chevron", 16.).text_color(p.accent);
        if expanded {
            chevron = chevron.with_transformation(gpui::Transformation::rotate(gpui::radians(
                std::f32::consts::PI,
            )));
        }
        div()
            .id("fixed-position-disclosure")
            .track_focus(&focus.clone().tab_index(0).tab_stop(!disabled))
            .size(px(32.))
            .flex_shrink_0()
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .when(disabled, |d| d.opacity(0.55))
            .when(!disabled, |d| d.cursor_pointer().hover(|d| d.bg(p.hover)))
            .focus(|d| d.bg(p.hover))
            .child(chevron)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if !disabled && !this.busy && !this.has_modal() {
                    focus.focus(window);
                    this.fixed_position_expanded = !this.fixed_position_expanded;
                    this.fixed_position_open = false;
                    cx.notify();
                }
            }))
    }

    // The popup is painted after the form, so its automatic tab insertion order
    // need not match the original inline DOM order. Keep this small span explicit.
    pub(crate) fn fixed_position_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.fixed_position_open || self.value.position_mode != "fixed" || self.has_modal() {
            return false;
        }
        let keys = [
            "fixed-position-trigger",
            "fixed-position-option-left",
            "fixed-position-option-right",
            "fixed-position-option-center",
        ];
        let handles = self.toggle_focus.borrow();
        let index = keys.iter().position(|key| {
            handles
                .get(*key)
                .is_some_and(|handle| handle.is_focused(window))
        });
        let remember_focused = handles
            .get("remember-position")
            .is_some_and(|focus| focus.is_focused(window));
        let target = match (index, reverse) {
            (Some(index), false) if index < keys.len() - 1 => Some(index + 1),
            (Some(index), true) if index > 0 => Some(index - 1),
            (None, true) if remember_focused => Some(keys.len() - 1),
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
            if let Some(focus) = self.toggle_focus.borrow().get("remember-position") {
                focus.focus(window);
            }
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn fixed_position_control(
        &self,
        disabled: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let disabled = disabled || self.value.position_mode != "fixed";
        let p = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        // The compact baseline is sufficient for Chinese at normal size, but
        // translated labels at large sizes must still leave room for the arrow.
        let trigger_width = crate::theme::label_width(
            window,
            &self.value,
            position_label(&self.value.fixed_position),
            13.,
        ) + px(40.);
        let menu_width = ["left", "right", "center"]
            .into_iter()
            .map(|id| {
                crate::theme::label_width(window, &self.value, position_label(id), 12.) + px(30.)
            })
            .fold(px(150.), |width, next| width.max(next));
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry("fixed-position-trigger".into())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let menu_bounds =
            std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::<gpui::Pixels>::default()));
        let outside_bounds = menu_bounds.clone();
        let mut control = div()
            .id("fixed-position-control")
            .relative()
            .flex_shrink_0()
            .on_mouse_down_out(
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if this.fixed_position_open && !outside_bounds.get().contains(&event.position) {
                        this.fixed_position_open = false;
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .id("fixed-position-trigger")
                    .track_focus(&focus.clone().tab_index(0).tab_stop(!disabled))
                    .w(trigger_width.max(px(110.)))
                    .h(px(28.))
                    .px(px(9.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if self.fixed_position_open {
                        p.accent
                    } else {
                        p.control_border
                    })
                    .bg(if self.fixed_position_open {
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
                    .when(!disabled, |d| d.cursor_pointer())
                    .child(position_label(&self.value.fixed_position))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(crate::icons::icon("chevron", 12.).text_color(p.subtle)),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !disabled && !this.busy && !this.has_modal() {
                            focus.focus(window);
                            this.fixed_position_open = !this.fixed_position_open;
                            this.retention_open = false;
                            this.language_open = false;
                            this.theme_open = false;
                            this.font_picker = None;
                            this.font_subscription = None;
                            cx.notify();
                        }
                    })),
            );
        if self.fixed_position_open && !disabled {
            let mut menu = div()
                .id("fixed-position-menu")
                .absolute()
                .top(px(33.))
                .right_0()
                .w(menu_width)
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
            for (index, id) in ["left", "right", "center"].into_iter().enumerate() {
                let selected = self.value.fixed_position == id;
                let option_focus = self
                    .toggle_focus
                    .borrow_mut()
                    .entry(format!("fixed-position-option-{id}"))
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                menu = menu.child(
                    div()
                        .id(("fixed-position-option", index))
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
                        .child(position_label(id))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.busy || this.has_modal() {
                                return;
                            }
                            if let Some(focus) =
                                this.toggle_focus.borrow().get("fixed-position-trigger")
                            {
                                focus.focus(window);
                            }
                            this.fixed_position_open = false;
                            this.save(Patch::FixedPosition(id.into()), None, window, cx);
                            cx.notify();
                        })),
                );
            }
            control = control.child(gpui::deferred(menu).priority(5));
        }
        control
    }
}
