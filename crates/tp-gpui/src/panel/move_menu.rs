use super::*;
use gpui::MouseButton;

pub(super) struct PendingMove {
    pub ids: Vec<String>,
    pub trigger: String,
    pub view: String,
    pub search: String,
    pub kind_filter: String,
}

impl PendingMove {
    fn matches_selection(&self, selection: &tp_core::selection::Selection) -> bool {
        selection.current.as_ref() == Some(&self.trigger)
            && self.ids.len() == selection.ids.len()
            && self.ids.iter().all(|id| selection.ids.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_move_rejects_changed_members_even_when_current_record_is_unchanged() {
        let pending = PendingMove {
            ids: vec!["a".into(), "b".into()],
            trigger: "b".into(),
            view: "default".into(),
            search: String::new(),
            kind_filter: "all".into(),
        };
        let visible = ["a", "b", "c"].map(String::from);
        let mut selection = tp_core::selection::Selection::default();
        selection.select(&visible, "a", false);
        selection.select(&visible, "b", true);
        assert!(pending.matches_selection(&selection));
        selection.ids.insert("c".into());
        assert!(!pending.matches_selection(&selection));
        selection.ids.remove("a");
        assert!(!pending.matches_selection(&selection));
        selection.select(&visible, "b", false);
        assert!(!pending.matches_selection(&selection));
        selection.select(&visible, "a", true);
        assert!(!pending.matches_selection(&selection));
    }
}

pub(super) struct MoveMenu {
    pub ids: Vec<String>,
    pub(super) trigger: String,
    anchor: Option<gpui::Bounds<gpui::Pixels>>,
    active: usize,
    focus: FocusHandle,
    scroll: ScrollHandle,
    reveal_pending: std::rc::Rc<std::cell::Cell<bool>>,
}

impl Panel {
    pub(super) fn finish_keyboard_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.scroll.has_pending_reveal() {
            return;
        }
        let Some(pending) = self.keyboard_move_pending.take() else {
            return;
        };
        if self.hidden
            || self.settings_view.is_some()
            || self.record_edit.is_some()
            || self.group_edit.is_some()
            || !pending.matches_selection(&self.selection)
            || self.session.view != pending.view
            || self.session.search != pending.search
            || self.session.kind_filter != pending.kind_filter
        {
            return;
        }
        self.open_move(pending.ids, pending.trigger, None, window, cx);
    }
    pub(super) fn open_move(
        &mut self,
        ids: Vec<String>,
        trigger: String,
        anchor: Option<gpui::Bounds<gpui::Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            return;
        }
        let records: Vec<_> = self
            .snapshot
            .records
            .iter()
            .filter(|r| ids.contains(&r.id))
            .collect();
        if records.is_empty() {
            return;
        }
        let common = records[0].group_id;
        let active = if records.iter().all(|r| r.group_id == common) {
            common
                .and_then(|id| {
                    self.snapshot
                        .groups
                        .iter()
                        .position(|g| g.id == id)
                        .map(|i| i + 1)
                })
                .unwrap_or(0)
        } else {
            0
        };
        let focus = cx.focus_handle();
        focus.focus(window);
        let scroll = ScrollHandle::new();
        scroll.scroll_to_item(active);
        self.filter_open = false;
        self.group_menu = None;
        self.record_menu = None;
        self.move_menu = Some(MoveMenu {
            ids,
            trigger,
            anchor,
            active,
            focus,
            scroll,
            reveal_pending: std::rc::Rc::new(std::cell::Cell::new(true)),
        });
        cx.notify();
    }

    pub(super) fn close_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move_pending = None;
        self.move_menu = None;
        self.list_focus.focus(window);
        cx.notify();
    }

    pub(super) fn move_trigger_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.move_trigger_focus.is_focused(window)
            || !matches!(event.keystroke.key.as_str(), "enter" | "space")
        {
            return false;
        }
        let Some(id) = self.focused_move_trigger.clone() else {
            return false;
        };
        // A hidden DOM action button cannot retain focus in the original UI.
        if self.hovered_record.as_ref() != Some(&id) || !self.visible_ids().contains(&id) {
            self.focused_move_trigger = None;
            self.panel_focus.focus(window);
            return false;
        }
        if !event.is_held {
            let anchor = self
                .move_trigger_bounds
                .borrow()
                .as_ref()
                .filter(|(trigger, _)| trigger == &id)
                .map(|(_, bounds)| *bounds);
            self.open_move(vec![id.clone()], id, anchor, window, cx);
        }
        cx.stop_propagation();
        window.prevent_default();
        true
    }

    fn commit_move(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.move_menu.take() else {
            return;
        };
        let group_id = if index == 0 {
            None
        } else {
            let Some(group) = self.snapshot.groups.get(index - 1) else {
                return;
            };
            Some(group.id)
        };
        let changed = self
            .snapshot
            .records
            .iter()
            .any(|r| menu.ids.contains(&r.id) && r.group_id != group_id);
        if changed {
            let result = operations::batch_records(
                &self.store,
                menu.ids,
                operations::BatchOperation::MoveGroup { group_id },
            );
            self.refresh(result, cx);
        }
        self.list_focus.focus(window);
        cx.notify();
    }

    pub(super) fn move_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = &mut self.move_menu else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                let trigger = menu.trigger.clone();
                self.close_move(window, cx);
                if self.hovered_record.as_ref() == Some(&trigger)
                    && self.visible_ids().contains(&trigger)
                {
                    self.focused_move_trigger = Some(trigger);
                    self.move_trigger_focus.focus(window);
                }
            }
            "up" | "down" => {
                let direction = if event.keystroke.key == "up" { -1 } else { 1 };
                menu.active = (menu.active as isize + direction)
                    .rem_euclid(self.snapshot.groups.len() as isize + 1)
                    as usize;
                menu.scroll.scroll_to_item(menu.active);
            }
            "enter" | "space" if !event.is_held => {
                let active = menu.active;
                self.commit_move(active, window, cx);
            }
            "tab" => {
                self.cycle_group(
                    if event.keystroke.modifiers.shift {
                        -1
                    } else {
                        1
                    },
                    window,
                    cx,
                );
            }
            _ => {}
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
        true
    }

    pub(super) fn render_move(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let state = self.move_menu.as_ref()?;
        let ids = self.visible_ids();
        let Some(index) = ids.iter().position(|id| *id == state.trigger) else {
            self.move_menu = None;
            return None;
        };
        let bounds = state.anchor.or_else(|| {
            self.scroll.bounds_for_item(index).map(|mut bounds| {
                bounds.origin.y += self.scroll.offset().y;
                bounds
            })
        })?;
        let size = window.viewport_size();
        let label_width = std::iter::once(choose("未分组", "Ungrouped"))
            .chain(self.snapshot.groups.iter().map(|group| group.name.as_str()))
            .map(|label| crate::theme::label_width(window, &self.snapshot.settings, label, 12.))
            .fold(px(0.), |width, next| width.max(next));
        // Border + menu padding + item padding + gap + fixed check column.
        let width = (label_width + px(54.))
            .max(px(140.))
            .min((size.width - px(16.)).max(px(0.)));
        let height = px(((self.snapshot.groups.len() + 1) * 30 + 12) as f32)
            .min(px(280.))
            .min((size.height - px(16.)).max(px(30.)));
        let overflows = px(((self.snapshot.groups.len() + 1) * 30) as f32) > height - px(12.);
        let below = bounds.bottom() + px(4.);
        let top = if below + height <= size.height - px(8.) {
            below
        } else {
            (bounds.top() - height - px(4.)).max(px(8.))
        };
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let mut menu = div()
            .id("move-menu")
            .track_focus(&state.focus)
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    let on_trigger = this
                        .move_trigger_bounds
                        .borrow()
                        .as_ref()
                        .is_some_and(|(_, bounds)| bounds.contains(&event.position));
                    if !on_trigger {
                        this.close_move(window, cx);
                    }
                }),
            )
            .absolute()
            .top(top)
            .left(
                (bounds.right() - width)
                    .min(size.width - width - px(8.))
                    .max(px(8.)),
            )
            .w(width)
            .h(height)
            .p(px(5.))
            .border_1()
            .border_color(p.line)
            .rounded(px(9.))
            .bg(p.surface)
            .shadow(vec![Palette::popup_shadow()])
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let mut options = div()
            .id("move-options")
            .size_full()
            .when(overflows, |options| options.pr(px(16.)))
            .overflow_y_scroll()
            .track_scroll(&state.scroll);
        let rows = std::iter::once((None, choose("未分组", "Ungrouped").to_owned())).chain(
            self.snapshot
                .groups
                .iter()
                .map(|g| (Some(g.id), g.name.clone())),
        );
        let records: Vec<_> = self
            .snapshot
            .records
            .iter()
            .filter(|r| state.ids.contains(&r.id))
            .collect();
        for (index, (group_id, label)) in rows.enumerate() {
            let checked = !records.is_empty() && records.iter().all(|r| r.group_id == group_id);
            options = options.child(
                div()
                    .id(index)
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .gap(px(8.))
                    .text_color(p.muted)
                    .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
                    .cursor_pointer()
                    .when(state.active == index, |d| d.bg(p.soft).text_color(p.accent))
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .child(
                        div()
                            .w(px(16.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(checked, |d| {
                                d.child(crate::icons::icon("check", 15.).text_color(
                                    if state.active == index {
                                        p.accent
                                    } else {
                                        p.muted
                                    },
                                ))
                            }),
                    )
                    .on_hover(cx.listener(move |this, hover, _, cx| {
                        if *hover {
                            if let Some(menu) = &mut this.move_menu {
                                menu.active = index;
                            }
                            cx.notify();
                        }
                    }))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.commit_move(index, window, cx)),
                    ),
            );
        }
        let pending = state.reveal_pending.clone();
        let scroll = state.scroll.clone();
        let active = state.active;
        let panel = cx.entity().downgrade();
        menu = menu.child(
            div()
                .relative()
                .size_full()
                .child(options)
                .child(
                    gpui::canvas(
                        move |_, _, cx| {
                            // Options have populated their scroll bounds before this probe.
                            if pending.get() && scroll.bounds_for_item(active).is_some() {
                                pending.set(false);
                                scroll.scroll_to_item(active);
                                cx.defer(move |cx| {
                                    let _ = panel.update(cx, |_, cx| cx.notify());
                                });
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(
                    div().absolute().inset_0().child(
                        gpui_component::scroll::Scrollbar::vertical(&state.scroll)
                            .id("move-menu-scrollbar")
                            .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                    ),
                ),
        );
        Some(menu.occlude().into_any_element())
    }
}
