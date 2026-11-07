use crate::theme::Palette;
use gpui::{div, prelude::*, px, Context, EventEmitter, Window};
use std::sync::{atomic::Ordering, Arc};
use tp_core::i18n::choose;
use tp_desktop::Store;

gpui::actions!(tp_cleanup, [Next, Previous, Cancel, Up, Down]);
pub fn init(cx: &mut gpui::App) {
    // Focused GPUI buttons dispatch on_click on Enter/Space key-up. Do not
    // also bind key-down activation: closing then restoring the opener's focus
    // would let the same key-up reopen the dialog (or toggle a checkbox twice).
    cx.bind_keys([
        gpui::KeyBinding::new("tab", Next, Some("TPCleanup")),
        gpui::KeyBinding::new("shift-tab", Previous, Some("TPCleanup")),
        gpui::KeyBinding::new("escape", Cancel, Some("TPCleanup")),
        gpui::KeyBinding::new("up", Up, Some("TPCleanup")),
        gpui::KeyBinding::new("down", Down, Some("TPCleanup")),
        gpui::KeyBinding::new("left", Up, Some("TPCleanup")),
        gpui::KeyBinding::new("right", Down, Some("TPCleanup")),
    ]);
}
pub enum Event {
    Closed,
    Finished {
        all: bool,
        result: Result<usize, String>,
    },
}
pub struct CleanupView {
    store: Arc<Store>,
    settings: tp_core::settings::Settings,
    all: bool,
    busy: bool,
    modal_focus: gpui::FocusHandle,
    focus: [gpui::FocusHandle; 4],
}
impl EventEmitter<Event> for CleanupView {}
impl CleanupView {
    pub(crate) fn is_busy(&self) -> bool {
        self.busy
    }

    pub(crate) fn focus(&self, window: &mut Window) {
        if self.busy {
            self.modal_focus.focus(window);
        } else {
            self.focus[2].focus(window);
        }
    }

    pub fn new(
        store: Arc<Store>,
        settings: tp_core::settings::Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = std::array::from_fn(|_| cx.focus_handle());
        focus[2].focus(window);
        Self {
            store,
            settings,
            all: false,
            busy: false,
            modal_focus: cx.focus_handle(),
            focus,
        }
    }
    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            cx.emit(Event::Closed);
        }
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.modal_focus.focus(window);
        let store = self.store.clone();
        let all = self.all;
        let revision = cx.global::<crate::host::Host>().background.revision.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = (|| {
                        let _data = store.write_data()?;
                        tp_core::cleanup::clear_cache_data(&store, all)
                    })();
                    // A partial file-system failure can still have removed records.
                    revision.fetch_add(1, Ordering::Release);
                    result
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                cx.emit(Event::Finished { all, result });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn key(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.busy {
            return;
        }
        let current = self
            .focus
            .iter()
            .position(|f| f.is_focused(window))
            .unwrap_or(2);
        match action {
            "cancel" => self.cancel(cx),
            "next" | "previous" => {
                let order = [usize::from(self.all), 2, 3];
                let index = order.iter().position(|&v| v == current).unwrap_or(1);
                let next = (index + if action == "next" { 1 } else { 2 }) % 3;
                self.focus[order[next]].focus(window);
            }
            "arrow" if current < 2 => {
                self.all = !self.all;
                self.focus[usize::from(self.all)].focus(window);
                cx.notify();
            }
            _ => {}
        }
    }
}
impl_view_render!(CleanupView);

impl CleanupView {
    #[cfg_attr(all(debug_assertions, feature = "hot-reload"), inline(never))]
    fn render_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let p = Palette::for_id(&self.settings.theme);
        let scale = self.settings.font_size as f32 / 13.;
        let mut card = div().id("cleanup-confirmation").w(px(340.)).max_w_full().max_h_full().overflow_y_scroll().p(px(20.)).rounded(px(12.)).border_1().border_color(p.line).bg(p.surface).shadow(vec![Palette::popup_shadow()]).text_size(px(12.*scale)).text_color(p.text)
            .child(div().mb(px(14.)).text_size(px(17.*scale)).font_weight(gpui::FontWeight(650.)).child(choose("清空数据？", "Clear data?")))
            .child(div().mb(px(10.)).line_height(px(19.2*scale)).text_color(p.muted).child(choose("清空后记录及对应的本地文件无法找回，请确认清空范围。", "Records and their local files cannot be recovered after clearing. Choose what to clear.")));
        let mut choices = div().flex().flex_col().gap(px(10.)).my(px(16.));
        for (index, label) in [
            choose("普通记录", "Ordinary records"),
            choose("全部记录", "All records"),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = self.all == (index == 1);
            choices = choices.child(
                div()
                    .id(("cleanup-scope", index))
                    .track_focus(&self.focus[index].clone().tab_index(0).tab_stop(selected && !self.busy))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(!self.busy, |d|d.cursor_pointer())
                    .line_height(px(19.2*scale))
                    .text_color(p.muted)
                    .focus(|d| d.bg(p.soft))
                    .child(
                        div()
                            .size(px(13.))
                            .flex_shrink_0()
                            .rounded_full()
                            .border_1()
                            .border_color(if selected { p.accent } else { p.control_border })
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(selected, |d| {
                                d.child(div().size(px(7.)).rounded_full().bg(p.accent))
                            }),
                    )
                    .child(label)
                    .on_click(cx.listener(move |this, event, window, cx| {
                        if !this.busy && !matches!(event, gpui::ClickEvent::Keyboard(key) if key.button == gpui::KeyboardButton::Enter) {
                            this.all = index == 1;
                            this.focus[index].focus(window);
                            cx.notify();
                        }
                    })),
            );
        }
        card = card.child(choices).child(div().mb(px(10.)).text_size(px(11.*scale)).line_height(px(17.6*scale)).text_color(p.subtle).child(format!("{}{}", choose(if self.all { "包含收藏和所有自建分组中的记录，分组和应用设置保留。" } else { "收藏和自建分组中的记录会保留。" }, if self.all { "Includes favorites and records in all custom groups. Groups and settings are kept." } else { "Favorites and records in custom groups are kept." }), choose("此操作无法撤销。", "This cannot be undone."))));
        if self.busy {
            card = card.child(
                div()
                    .mt(px(16.))
                    .text_color(p.accent)
                    .child(choose("清空中…", "Clearing…")),
            );
        } else {
            card = card.child(
                div()
                    .mt(px(20.))
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("cleanup-cancel")
                            .track_focus(&self.focus[2])
                            .tab_index(0)
                            .h(px(34.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(p.line)
                            .bg(p.control)
                            .text_color(p.muted)
                            .hover(|d| d.bg(p.hover))
                            .focus(|d| d.border_color(p.accent))
                            .cursor_pointer()
                            .child(choose("取消", "Cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    )
                    .child(
                        div()
                            .id("cleanup-confirm")
                            .track_focus(&self.focus[3])
                            .tab_index(0)
                            .h(px(34.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(p.danger_bg)
                            .bg(p.danger_bg)
                            .text_color(p.on_danger)
                            .hover(|d| d.bg(p.danger_bg_hover).border_color(p.danger_bg_hover))
                            .focus(|d| d.border_color(p.text))
                            .cursor_pointer()
                            .child(choose("确认清空", "Clear data"))
                            .on_click(cx.listener(|this, _, window, cx| this.clear(window, cx))),
                    ),
            );
        }
        div()
            .id("cleanup-backdrop")
            .track_focus(&self.modal_focus)
            .occlude()
            .size_full()
            .p(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x171c282e))
            .key_context("TPCleanup")
            .on_action(cx.listener(|this, _: &Next, window, cx| this.key("next", window, cx)))
            .on_action(
                cx.listener(|this, _: &Previous, window, cx| this.key("previous", window, cx)),
            )
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.key("cancel", window, cx)))
            .on_action(cx.listener(|this, _: &Up, window, cx| this.key("arrow", window, cx)))
            .on_action(cx.listener(|this, _: &Down, window, cx| this.key("arrow", window, cx)))
            .child(card)
            .into_any_element()
    }
}
