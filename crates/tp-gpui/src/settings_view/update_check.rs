use super::*;
use crate::updates::{self, CURRENT_VERSION};
use std::time::Duration;

const CHECK_BUTTON: &str = "check-for-updates";
const FEEDBACK_VISIBLE: Duration = Duration::from_millis(3500);

pub(super) enum State {
    Idle,
    Feedback {
        message: &'static str,
        _expiry: gpui::Task<()>,
    },
}

impl SettingsView {
    pub(super) fn reset_update_check(&mut self) {
        self.update_task = None;
        self.update_check = State::Idle;
        self.available_update = None;
    }

    fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_task.is_some() || self.has_modal() {
            return;
        }

        self.update_focus(CHECK_BUTTON, cx).focus(window);
        self.update_check = State::Idle;
        self.update_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { updates::check() })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.update_task = None;
                match result {
                    Ok(Some(release)) => {
                        cx.emit(Event::UpdateAvailable(release.clone()));
                        this.available_update = Some(release);
                    }
                    Ok(None) if this.available_update.is_none() => {
                        this.show_update_feedback(choose("已是最新版", "You’re up to date"), cx);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("Could not check TP updates: {error:#}");
                        this.show_update_feedback(
                            choose(
                                "检查更新失败，请重试。",
                                "Couldn’t check for updates. Try again.",
                            ),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn show_update_feedback(&mut self, message: &'static str, cx: &mut Context<Self>) {
        self.update_check = State::Feedback {
            message,
            _expiry: cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FEEDBACK_VISIBLE).await;
                let _ = this.update(cx, |this, cx| {
                    this.update_check = State::Idle;
                    cx.notify();
                });
            }),
        };
    }

    pub(super) fn update_card(&self, disabled: bool, cx: &mut Context<Self>) -> gpui::Div {
        let palette = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let checking = self.update_task.is_some();
        let status = match self.update_check {
            State::Idle => None,
            State::Feedback { message, .. } => Some(message),
        };
        let row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .text_color(palette.text)
                    .text_ellipsis()
                    .child(format!("v{CURRENT_VERSION}")),
            )
            .child(
                div().ml_auto().child(
                    self.update_button(
                        CHECK_BUTTON,
                        if checking {
                            choose("正在检查…", "Checking…")
                        } else {
                            choose("检查更新", "Check for updates")
                        },
                        false,
                        disabled || checking,
                        cx,
                    )
                    .on_click(
                        cx.listener(|this, _, window, cx| this.check_for_updates(window, cx)),
                    ),
                ),
            );
        self.card()
            .mt(px(14.))
            .px(px(14.))
            .py(px(13.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .text_color(palette.muted)
            .text_size(px(13. * scale))
            .child(
                div()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(palette.text)
                    .child(choose("版本更新", "Software update")),
            )
            .child(row)
            .when(
                status.is_some() || self.available_update.is_some(),
                |card| {
                    card.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .pt(px(9.))
                            .border_t_1()
                            .border_color(palette.line)
                            .when_some(self.available_update.as_ref(), |feedback, release| {
                                feedback.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(format!(
                                            "{}v{}",
                                            choose("有新版本：", "New version: "),
                                            release.version,
                                        ))
                                        .child(
                                            div().ml_auto().child(
                                                self.update_button(
                                                    "open-update-release",
                                                    choose("去下载", "Download"),
                                                    true,
                                                    disabled,
                                                    cx,
                                                )
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    let Some(release) = &this.available_update
                                                    else {
                                                        return;
                                                    };
                                                    if !this.has_modal() {
                                                        cx.open_url(&release.page_url());
                                                    }
                                                })),
                                            ),
                                        ),
                                )
                            })
                            .when_some(status, |feedback, status| feedback.child(status)),
                    )
                },
            )
    }

    fn update_button(
        &self,
        id: &'static str,
        label: &'static str,
        text_only: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let palette = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let focus = self.update_focus(id, cx);
        div()
            .id(id)
            .track_focus(&focus.tab_index(0).tab_stop(!disabled))
            .flex()
            .items_center()
            .justify_center()
            .min_h(px(34.))
            .max_w_full()
            .px(px(11.))
            .py(px(5.))
            .text_size(px(13. * scale))
            .line_height(px(18. * scale))
            .when(text_only, |button| button.text_color(palette.accent))
            .when(!text_only, |button| {
                button
                    .rounded(px(8.))
                    .border_1()
                    .border_color(palette.control_border)
                    .bg(palette.control)
                    .text_color(palette.text)
            })
            .when(disabled, |button| button.opacity(0.55))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .when(text_only, |button| {
                        button
                            .focus(|button| button.underline())
                            .hover(|button| button.text_color(palette.accent_hover).underline())
                    })
                    .when(!text_only, |button| {
                        button
                            .focus(|button| button.border_color(palette.accent))
                            .hover(|button| button.bg(palette.soft))
                    })
            })
            .child(div().min_w_0().child(label))
    }

    fn update_focus(&self, id: &str, cx: &mut Context<Self>) -> gpui::FocusHandle {
        self.toggle_focus
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }
}
