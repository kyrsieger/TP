use super::*;
use gpui::{MouseButton, Pixels, Point};

pub(super) struct GroupEditor {
    id: Option<i64>,
    input: Entity<InputState>,
    _subscription: Subscription,
    select_on_render: std::cell::Cell<bool>,
}
pub(super) struct GroupMenu {
    id: i64,
    position: Point<Pixels>,
    focus: FocusHandle,
    active: Option<usize>,
}

impl Panel {
    pub(super) fn focus_group(&mut self, view: String, window: &mut Window) {
        self.focused_group_view = view;
        self.group_space_pending = None;
        self.group_focus.focus(window);
    }

    pub(super) fn activate_focused_group(&mut self, cx: &mut Context<Self>) {
        let view = self.focused_group_view.clone();
        if view != "default"
            && view != "favorite"
            && !self
                .snapshot
                .groups
                .iter()
                .any(|group| view == format!("group:{}", group.id))
        {
            return;
        }
        if view == self.session.view && self.scroll.show_back_to_top() {
            self.scroll.return_to_top(
                crate::native_window::reduced_motion(),
                std::time::Instant::now(),
            );
        } else {
            self.session.view = view;
            self.record_edit = None;
            self.filter_open &= self.session.view == "default";
            self.selection = Default::default();
            self.reconcile();
            self.scroll.scroll_to_item(0);
        }
        cx.notify();
    }

    pub(super) fn group_key_up(
        &mut self,
        event: &gpui::KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key != "space" {
            return;
        }
        let Some(view) = self.group_space_pending.take() else {
            return;
        };
        let modifiers = event.keystroke.modifiers;
        if !self.group_focus.is_focused(window)
            || view != self.focused_group_view
            || modifiers.control
            || modifiers.alt
            || modifiers.platform
            || modifiers.shift
        {
            return;
        }
        // A focused HTML group button activates on Space release. It does not
        // open the selected record's media preview or start record renaming.
        self.activate_focused_group(cx);
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }

    pub(super) fn start_group_edit(
        &mut self,
        id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_edit.is_some() {
            return;
        }
        self.group_menu = None;
        self.move_menu = None;
        self.record_menu = None;
        self.filter_open = false;
        let name = id
            .and_then(|id| {
                self.snapshot
                    .groups
                    .iter()
                    .find(|g| g.id == id)
                    .map(|g| g.name.clone())
            })
            .unwrap_or_default();
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx)
                .max_length_utf16(40)
                .placeholder(if id.is_none() {
                    choose("分组名称", "Group name")
                } else {
                    ""
                });
            input.set_value(name, window, cx);
            input.focus(window, cx);
            input
        });
        let subscription =
            cx.subscribe_in(
                &input,
                window,
                |this, source, event, window, cx| match event {
                    InputEvent::PressEnter { secondary: false } => {
                        this.commit_group_edit(window, cx)
                    }
                    InputEvent::Blur => {
                        let source_id = source.entity_id();
                        cx.defer_in(window, move |this, window, cx| {
                            if this.group_edit.as_ref().is_some_and(|edit| {
                                edit.input.entity_id() == source_id
                                    && !edit.input.read(cx).focus_handle(cx).is_focused(window)
                            }) {
                                this.group_edit = None;
                                cx.notify();
                            }
                        });
                    }
                    _ => {}
                },
            );
        self.group_edit = Some(GroupEditor {
            id,
            input,
            _subscription: subscription,
            select_on_render: std::cell::Cell::new(id.is_some()),
        });
        // Apply after layout: the 26px add button becomes a 116px input,
        // and a renamed tab can also change width. Reveal the new bounds.
        let index = id
            .and_then(|id| self.snapshot.groups.iter().position(|group| group.id == id))
            .unwrap_or(self.snapshot.groups.len());
        self.group_scroll.scroll_to_item(index + 2);
        cx.notify();
    }

    fn commit_group_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &self.group_edit else {
            return;
        };
        if edit.input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        let name = edit.input.read(cx).value().trim().to_owned();
        if edit.id.is_some_and(|id| {
            self.snapshot
                .groups
                .iter()
                .any(|group| group.id == id && group.name == name)
        }) {
            self.group_edit = None;
            self.list_focus.focus(window);
            cx.notify();
            return;
        }
        if name.is_empty()
            || ["默认", "收藏", "Default", "Favorites"].contains(&name.as_str())
            || name.chars().count() > 40
            || self
                .snapshot
                .groups
                .iter()
                .any(|g| g.name == name && Some(g.id) != edit.id)
        {
            edit.input.read(cx).focus_handle(cx).focus(window);
            edit.select_on_render.set(edit.id.is_some());
            self.show_toast(
                choose("分组名称不可用", "Group name unavailable").into(),
                cx,
            );
            cx.notify();
            return;
        }
        let result = match edit.id {
            Some(id) => operations::rename_group(&self.store, id, name),
            None => operations::add_group(&self.store, name),
        };
        if result.is_ok() {
            self.group_edit = None;
            self.list_focus.focus(window);
        } else if let Some(edit) = &self.group_edit {
            edit.input.read(cx).focus_handle(cx).focus(window);
            edit.select_on_render.set(edit.id.is_some());
        }
        self.refresh(result, cx);
    }

    pub(super) fn group_edit_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = &self.group_edit else {
            return false;
        };
        if matches!(event.keystroke.key.as_str(), "enter" | "escape")
            && !edit.input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        {
            if event.keystroke.key == "enter" {
                self.commit_group_edit(window, cx);
            } else {
                self.group_edit = None;
                self.list_focus.focus(window);
            }
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
        }
        // Other keys belong to text editing, including IME and Ctrl+A.
        true
    }

    pub(super) fn group_input(
        &self,
        id: Option<i64>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let edit = self.group_edit.as_ref().filter(|e| e.id == id)?;
        if edit.select_on_render.replace(false) {
            let focus = edit.input.read(cx).focus_handle(cx);
            window.on_next_frame(move |window, cx| {
                focus.dispatch_action(&gpui_component::input::SelectAll, window, cx);
            });
        }
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let mut input = Input::new(&edit.input)
            .appearance(false)
            .px(px(9.))
            .py_0()
            .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
            .text_color(p.text);
        input.style().size.height = Some(px(24.).into());
        Some(
            div()
                .w(px(116.))
                .flex_shrink_0()
                .h(px(26.))
                .border_1()
                .border_color(p.group_text)
                .rounded(px(7.))
                .bg(p.surface)
                .when(
                    edit.input.read(cx).focus_handle(cx).is_focused(window),
                    |d| {
                        d.shadow(vec![gpui::BoxShadow {
                            color: p.group_text.into(),
                            offset: gpui::point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(1.),
                        }])
                    },
                )
                .child(input)
                .into_any_element(),
        )
    }

    pub(super) fn open_group_menu(
        &mut self,
        id: i64,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_edit.is_some() {
            return;
        }
        self.move_menu = None;
        self.record_menu = None;
        self.filter_open = false;
        let focus = cx.focus_handle();
        focus.focus(window);
        self.group_menu = Some(GroupMenu {
            id,
            position,
            focus,
            active: None,
        });
        cx.notify();
    }

    fn group_menu_action(&mut self, action: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.group_menu.take() else {
            return;
        };
        if action == 0 {
            self.start_group_edit(Some(menu.id), window, cx);
        } else {
            let result = operations::delete_group(&self.store, menu.id);
            self.refresh(result, cx);
            self.list_focus.focus(window);
        }
    }

    pub(super) fn group_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = &mut self.group_menu else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                let view = format!("group:{}", menu.id);
                self.group_menu = None;
                self.focus_group(view, window);
            }
            "up" | "down" => {
                menu.active = Some(context_menu::next_item(
                    menu.active,
                    2,
                    event.keystroke.key == "up",
                ));
            }
            "enter" | "space" if !event.is_held => {
                if let Some(action) = menu.active {
                    self.group_menu_action(action, window, cx);
                }
            }
            "enter" | "space" => {}
            "a" if event.keystroke.modifiers.control
                && !event.keystroke.modifiers.alt
                && !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.shift => {}
            _ => {
                self.group_menu = None;
                cx.notify();
                return false;
            }
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
        true
    }

    pub(super) fn render_group_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let state = self.group_menu.as_ref()?;
        let size = window.viewport_size();
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let labels = [choose("重命名", "Rename"), choose("删除", "Delete")];
        let width = labels
            .iter()
            .map(|label| {
                crate::theme::label_width(window, &self.snapshot.settings, label, 12.) + px(30.)
            })
            .fold(px(116.), |width, next| width.max(next));
        let mut menu = div()
            .id("group-menu")
            .track_focus(&state.focus)
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.group_menu = None;
                this.list_focus.focus(window);
                cx.notify();
            }))
            .absolute()
            .left(
                state
                    .position
                    .x
                    .min(size.width - width - px(8.))
                    .max(px(8.)),
            )
            .top(state.position.y.min(size.height - px(80.)).max(px(8.)))
            .w(width)
            .p(px(5.))
            .rounded(px(9.))
            .border_1()
            .border_color(p.line)
            .bg(p.surface)
            .shadow(vec![Palette::popup_shadow()])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (index, label) in labels.into_iter().enumerate() {
            menu = menu.child(
                div()
                    .id(index)
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
                    .text_color(if index == 1 { p.danger_muted } else { p.muted })
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(Some(index) == state.active, |d| d.bg(p.hover))
                    .hover(|d| d.bg(p.hover))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.group_menu_action(index, window, cx)
                    })),
            );
        }
        Some(menu.occlude().into_any_element())
    }
}
