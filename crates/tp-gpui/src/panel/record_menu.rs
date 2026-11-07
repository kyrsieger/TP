use super::*;
use gpui::{MouseButton, Pixels, Point};

pub(super) struct RecordEditor {
    pub id: String,
    input: Entity<InputState>,
    select_on_render: std::cell::Cell<bool>,
    _subscription: Subscription,
}
pub(super) struct RecordMenu {
    pub(super) scroll_offset: Point<Pixels>,
    id: String,
    position: Point<Pixels>,
    focus: FocusHandle,
    active: Option<usize>,
    files: bool,
}

impl Panel {
    pub(super) fn start_record_edit(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = self.snapshot.records.iter().find(|r| r.id == id) else {
            return;
        };
        let title = record.title.clone();
        self.close_floating(cx);
        self.group_edit = None;
        self.record_menu = None;
        self.move_menu = None;
        self.group_menu = None;
        self.filter_open = false;
        self.selection.select(&self.visible_ids(), &id, false);
        if let Some(index) = self.visible_ids().iter().position(|r| *r == id) {
            self.scroll.scroll_to_item(index);
        }
        let input = cx.new(|cx| {
            let mut input =
                InputState::new(window, cx).placeholder(choose("输入标题", "Enter a title"));
            input.set_value(title, window, cx);
            input.focus(window, cx);
            input
        });
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                InputEvent::PressEnter { secondary: false } => this.commit_record_edit(window, cx),
                InputEvent::Blur => {
                    this.record_edit = None;
                    cx.notify();
                }
                _ => {}
            });
        self.record_edit = Some(RecordEditor {
            id,
            input,
            select_on_render: std::cell::Cell::new(true),
            _subscription: subscription,
        });
        cx.notify();
    }

    fn commit_record_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &self.record_edit else {
            return;
        };
        if edit.input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        let title = edit.input.read(cx).value().trim().to_owned();
        if title.chars().count() > 100 || title.contains(['\r', '\n']) {
            edit.input.read(cx).focus_handle(cx).focus(window);
            edit.select_on_render.set(true);
            self.show_toast(
                choose(
                    "记录标题最多 100 个字符，且不能换行",
                    "Record titles must be at most 100 characters with no line breaks",
                )
                .into(),
                cx,
            );
            return;
        }
        if self
            .snapshot
            .records
            .iter()
            .any(|record| record.id == edit.id && record.title == title)
        {
            self.record_edit = None;
            self.list_focus.focus(window);
            cx.notify();
            return;
        }
        let result = operations::rename_record(&self.store, edit.id.clone(), title);
        match result {
            Ok(()) => {
                self.record_edit = None;
                self.refresh(Ok(()), cx);
                self.list_focus.focus(window);
            }
            Err(error) => {
                if let Some(edit) = &self.record_edit {
                    edit.input.read(cx).focus_handle(cx).focus(window);
                    edit.select_on_render.set(true);
                }
                self.show_toast(error, cx);
            }
        }
    }

    pub(super) fn record_edit_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = &self.record_edit else {
            return false;
        };
        if matches!(event.keystroke.key.as_str(), "enter" | "escape")
            && !edit.input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        {
            if event.keystroke.key == "enter" {
                self.commit_record_edit(window, cx);
            } else {
                self.record_edit = None;
                self.list_focus.focus(window);
            }
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
        }
        true
    }

    pub(super) fn record_input(
        &self,
        id: &str,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let edit = self.record_edit.as_ref().filter(|e| e.id == id)?;
        if edit.select_on_render.replace(false) {
            let focus = edit.input.read(cx).focus_handle(cx);
            window.on_next_frame(move |window, cx| {
                focus.dispatch_action(&gpui_component::input::SelectAll, window, cx)
            });
        }
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let mut input = Input::new(&edit.input)
            .appearance(false)
            .px(px(9.))
            .py_0()
            .text_size(px(12.5 * self.snapshot.settings.font_size as f32 / 13.))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(p.text);
        // Input::h only affects multiline mode; refine the single-line layout.
        input.style().size.height = Some(px(30.).into());
        Some(
            div()
                .flex_1()
                .min_w_0()
                .h(px(32.))
                .border_1()
                .border_color(p.accent)
                .rounded(px(6.))
                .bg(p.surface)
                .child(input)
                .into_any_element(),
        )
    }

    pub(super) fn rename_key_matches(&self, event: &KeyDownEvent) -> bool {
        crate::shortcuts::rename_key(&event.keystroke).is_some_and(|key| {
            !self.snapshot.settings.rename_shortcut.is_empty()
                && key == self.snapshot.settings.rename_shortcut
        })
    }

    pub(super) fn open_record_menu(
        &mut self,
        id: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_edit.is_some() {
            return;
        }
        let Some(record) = self.snapshot.records.iter().find(|r| r.id == id) else {
            return;
        };
        let files = (record.kind == "image" && record.files.is_empty())
            || (!record.files.is_empty() && record.files.iter().all(|f| !f.is_directory));
        // The original context menu cancels an unsaved title edit before
        // opening, including when the user right-clicks another record.
        self.record_edit = None;
        self.close_floating(cx);
        let focus = cx.focus_handle();
        focus.focus(window);
        self.move_menu = None;
        self.group_menu = None;
        self.filter_open = false;
        self.record_menu = Some(RecordMenu {
            scroll_offset: self.scroll.displayed_offset(),
            id,
            position,
            focus,
            active: None,
            files,
        });
        cx.notify();
    }

    fn record_menu_action(&mut self, action: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.record_menu.take() else {
            return;
        };
        if action == 0 {
            self.start_record_edit(menu.id, window, cx);
            return;
        }
        if !menu.files {
            return;
        }
        self.list_focus.focus(window);
        let store = self.store.clone();
        cx.spawn(async move |panel, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match action {
                        1 => tp_desktop::files::copy_record_paths(&store, menu.id),
                        2 => tp_desktop::files::open_record_directory(&store, menu.id),
                        3 => tp_desktop::files::save_record_files_async(&store, menu.id).await,
                        _ => Ok(()),
                    }
                })
                .await;
            let _ = panel.update(cx, |panel, cx| {
                if let Err(error) = result {
                    panel.show_toast(error, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn record_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = &mut self.record_menu else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                self.record_menu = None;
                self.list_focus.focus(window);
            }
            "up" | "down" => {
                menu.active = Some(context_menu::next_item(
                    menu.active,
                    if menu.files { 4 } else { 1 },
                    event.keystroke.key == "up",
                ));
            }
            "enter" | "space" if !event.is_held => {
                if let Some(active) = menu.active {
                    self.record_menu_action(active, window, cx);
                }
            }
            "enter" | "space" => {}
            "a" if event.keystroke.modifiers.control
                && !event.keystroke.modifiers.alt
                && !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.shift => {}
            _ => {
                // Original context menus dismiss on other keys and let the
                // panel continue handling them (Tab, Ctrl+F, shortcuts).
                self.record_menu = None;
                cx.notify();
                return false;
            }
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
        true
    }

    pub(super) fn render_record_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let state = self.record_menu.as_ref()?;
        let size = window.viewport_size();
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let count = if state.files { 4 } else { 1 };
        let labels = [
            choose("重命名", "Rename"),
            choose("复制路径", "Copy paths"),
            choose("打开所在目录", "Open containing folder"),
            choose("另存为", "Save as"),
        ];
        let shortcut = self
            .snapshot
            .settings
            .rename_shortcut
            .split('+')
            .map(|part| if part == "Control" { "Ctrl" } else { part })
            .collect::<Vec<_>>()
            .join("+");
        let width = labels
            .iter()
            .take(count)
            .enumerate()
            .map(|(index, label)| {
                let extra = if index == 0 && !shortcut.is_empty() {
                    px(24.)
                        + crate::theme::label_width(window, &self.snapshot.settings, &shortcut, 11.)
                } else {
                    px(0.)
                };
                crate::theme::label_width(window, &self.snapshot.settings, label, 12.)
                    + extra
                    + px(30.)
            })
            .fold(px(128.), |width, next| width.max(next));
        let mut menu = div()
            .id("record-menu")
            .track_focus(&state.focus)
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                if cfg!(debug_assertions) && std::env::var_os("TP_DIAGNOSTIC_LAYOUT").is_some() {
                    eprintln!("record menu closed: outside mouse down");
                }
                this.record_menu = None;
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
            .top(
                state
                    .position
                    .y
                    .min(size.height - px(count as f32 * 30. + 20.))
                    .max(px(8.)),
            )
            .w(width)
            .p(px(5.))
            .rounded(px(9.))
            .border_1()
            .border_color(p.line)
            .bg(p.surface)
            .shadow(vec![Palette::popup_shadow()])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (index, label) in labels.into_iter().take(count).enumerate() {
            menu = menu.child(
                div()
                    .id(index)
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
                    .text_color(p.muted)
                    .gap(px(24.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(Some(index) == state.active, |d| d.bg(p.hover))
                    .hover(|d| d.bg(p.hover))
                    .child(div().flex_1().child(label))
                    .when(index == 0 && !shortcut.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(11. * self.snapshot.settings.font_size as f32 / 13.))
                                .flex_shrink_0()
                                .text_color(p.subtle)
                                .child(shortcut.clone()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.record_menu_action(index, window, cx)
                    })),
            );
        }
        Some(menu.occlude().into_any_element())
    }
}
