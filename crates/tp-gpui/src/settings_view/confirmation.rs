use super::*;
use crate::cleanup_view::{Cancel, Next, Previous};
impl SettingsView {
    pub(super) fn resolve_pending(
        &mut self,
        confirm: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui::Focusable;
        let Some((patch, count)) = self.pending.take() else {
            return;
        };
        let focus = match &patch {
            Patch::Capacity(_) => Some(self.numbers[1].read(cx).focus_handle(cx)),
            Patch::Retention(_) => self.toggle_focus.borrow().get("retention-trigger").cloned(),
            _ => None,
        };
        if let Some(focus) = focus {
            focus.focus(window);
        }
        if confirm {
            self.save(patch, Some(count), window, cx);
        } else {
            self.reset_numbers(window, cx);
            self.finish_settings_work(window, cx);
        }
        cx.notify();
    }
    fn pending_key(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.pending.is_none() || self.busy {
            return;
        }
        match action {
            "cancel" => self.resolve_pending(false, window, cx),
            _ => {
                let next = usize::from(!self.pending_focus[1].is_focused(window));
                self.pending_focus[next].focus(window);
                cx.notify();
            }
        }
    }
    pub(super) fn pending_dialog(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let prefix = format!(
            "{} ",
            choose("新设置将删除", "The new settings will delete ")
        );
        let number = count.to_string();
        let description = format!(
            "{prefix}{number} {}",
            choose(
                "条已有普通记录及其本地文件。",
                " existing ordinary records and their local files."
            )
        );
        let description = gpui::StyledText::new(description).with_highlights([(
            prefix.len()..prefix.len() + number.len(),
            gpui::HighlightStyle {
                color: Some(p.danger_bg.into()),
                font_weight: Some(gpui::FontWeight(650.)),
                ..Default::default()
            },
        )]);
        let mut actions = div()
            .mt(px(20.))
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(px(8.));
        for (index, confirm) in [false, true].into_iter().enumerate() {
            actions = actions.child(
                div()
                    .id(if confirm {
                        "confirm-cleanup"
                    } else {
                        "cancel-cleanup"
                    })
                    .track_focus(
                        &self.pending_focus[index]
                            .clone()
                            .tab_index(0)
                            .tab_stop(true),
                    )
                    .h(px(34.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if confirm { p.danger_bg } else { p.line })
                    .bg(if confirm { p.danger_bg } else { p.control })
                    .text_color(if confirm { p.on_danger } else { p.muted })
                    .text_size(px(12. * scale))
                    .cursor_pointer()
                    .focus(|d| d.border_color(p.accent))
                    .hover(move |d| {
                        if confirm {
                            d.bg(p.danger_bg_hover).border_color(p.danger_bg_hover)
                        } else {
                            d.bg(p.hover)
                        }
                    })
                    .child(if confirm {
                        choose("删除并应用", "Delete and apply")
                    } else {
                        choose("取消", "Cancel")
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.resolve_pending(confirm, window, cx)
                    })),
            );
        }
        div().id("settings-cleanup-backdrop").absolute().inset_0().top(px(-52.)).occlude().p(px(16.))
            .flex().items_center().justify_center().bg(gpui::rgba(0x171c282e)).key_context("TPCleanup")
            .on_action(cx.listener(|this,_:&Next,window,cx|this.pending_key("tab",window,cx)))
            .on_action(cx.listener(|this,_:&Previous,window,cx|this.pending_key("tab",window,cx)))
            .on_action(cx.listener(|this,_:&Cancel,window,cx|this.pending_key("cancel",window,cx)))
            .child(div().id("settings-cleanup-confirmation").w(px(340.)).max_w_full().max_h_full().overflow_y_scroll()
                .p(px(20.)).rounded(px(12.)).border_1().border_color(p.line).bg(p.surface)
                .shadow(vec![Palette::popup_shadow()]).text_color(p.text)
                .child(div().mb(px(14.)).text_size(px(17.*scale)).font_weight(gpui::FontWeight(650.))
                    .child(choose("应用设置并清理记录？","Apply settings and clean up records?")))
                .child(div().mb(px(10.)).text_size(px(12.*scale)).line_height(px(19.2*scale)).text_color(p.muted)
                    .child(description))
                .child(div().mb(px(10.)).text_size(px(11.*scale)).line_height(px(17.6*scale)).text_color(p.subtle)
                    .child(choose("收藏和自建分组中的记录会保留。此操作无法撤销。","Favorites and records in custom groups are kept. This cannot be undone.")))
                .child(actions))
    }
}
