use crate::theme::Palette;
use gpui::{div, prelude::*, px, Context, EventEmitter, Window};
use std::sync::{atomic::Ordering, Arc};
use tp_core::{
    i18n::choose,
    transfer::{self, RestorePreview, RestoreResult, TransferSessions},
};
use tp_desktop::Store;

gpui::actions!(tp_transfer, [Next, Previous, Cancel]);
pub fn init(cx: &mut gpui::App) {
    // Focused GPUI buttons dispatch on_click on Enter/Space key-up. Do not
    // also bind key-down activation: closing then restoring the opener's focus
    // would let the same key-up reopen the dialog (or toggle a checkbox twice).
    cx.bind_keys([
        gpui::KeyBinding::new("tab", Next, Some("TPTransfer")),
        gpui::KeyBinding::new("shift-tab", Previous, Some("TPTransfer")),
        gpui::KeyBinding::new("escape", Cancel, Some("TPTransfer")),
    ]);
}

pub enum Event {
    Closed,
    Restored,
    Failed(String),
}
pub struct TransferView {
    store: Arc<Store>,
    sessions: Arc<TransferSessions>,
    settings: tp_core::settings::Settings,
    import: bool,
    busy: bool,
    preview: Option<RestorePreview>,
    keep_expired: bool,
    warnings_open: bool,
    error: Option<String>,
    focus: gpui::FocusHandle,
    controls: [gpui::FocusHandle; 4],
}
impl EventEmitter<Event> for TransferView {}
impl TransferView {
    pub(crate) fn retain_on_hide(&self) -> bool {
        retain_transfer_on_hide(self.busy, self.import, self.preview.is_some())
    }

    pub(crate) fn focus(&self, window: &mut Window) {
        if self.busy {
            self.focus.focus(window);
        } else {
            self.controls[2].focus(window);
        }
    }

    pub fn new(
        store: Arc<Store>,
        settings: tp_core::settings::Settings,
        import: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let controls = std::array::from_fn(|_| cx.focus_handle());
        if import {
            focus.focus(window);
        } else {
            controls[2].focus(window);
        }
        let mut this = Self {
            store,
            sessions: Arc::new(TransferSessions::default()),
            settings,
            import,
            busy: false,
            preview: None,
            keep_expired: false,
            warnings_open: false,
            error: None,
            focus,
            controls,
        };
        if import {
            this.inspect(window, cx);
        }
        this
    }
    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        // Dropping the session releases its prepared temporary workspace.
        self.sessions = Arc::new(TransferSessions::default());
        self.preview = None;
        cx.emit(Event::Closed);
    }
    fn inspect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let store = self.store.clone();
        let sessions = self.sessions.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let Some(path) = rfd::AsyncFileDialog::new()
                        .set_title(choose("选择 TP 导出包", "Choose a TP export archive"))
                        .add_filter("TP ZIP", &["zip"])
                        .pick_file()
                        .await
                    else {
                        return Ok(None);
                    };
                    transfer::inspect_archive(&store, &sessions, path.path()).map(Some)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.controls[2].focus(window);
                match result {
                    Ok(Some(preview)) => this.preview = Some(preview),
                    Ok(None) => cx.emit(Event::Closed),
                    Err(error) => {
                        cx.emit(Event::Failed(format!(
                            "{}{error}",
                            choose("无法读取导出包：", "Could not read export: ")
                        )));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn key(&mut self, key: &str, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if key == "escape" {
            self.cancel(cx);
            return;
        }
        let enabled = tab_order(
            self.busy,
            self.preview.as_ref().is_some_and(|p| p.cleanup_count > 0),
            self.preview
                .as_ref()
                .is_some_and(|p| !p.warnings.is_empty()),
            !self.import || self.preview.is_some(),
        );
        if enabled.is_empty() {
            self.focus.focus(window);
            return;
        }
        let current = enabled
            .iter()
            .position(|&i| self.controls[i].is_focused(window));
        if key == "tab" {
            let next = match current {
                Some(i) if shift => (i + enabled.len() - 1) % enabled.len(),
                Some(i) => (i + 1) % enabled.len(),
                None => enabled.iter().position(|&i| i == 2).unwrap_or(0),
            };
            self.controls[enabled[next]].focus(window);
            return;
        }
    }
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || (self.import && self.preview.is_none()) {
            return;
        }
        self.focus.focus(window);
        self.busy = true;
        self.error = None;
        let store = self.store.clone();
        let sessions = self.sessions.clone();
        let preview = self.preview.clone();
        let keep_expired = self.keep_expired;
        let revision = cx.global::<crate::host::Host>().background.revision.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                if let Some(preview) = preview {
                    let result = transfer::restore_backup(&store, &sessions, preview.token, keep_expired, preview.cleanup_count, preview.next_max_ordinary)?;
                    if matches!(result, RestoreResult::Completed { .. }) { revision.fetch_add(1, Ordering::Release); }
                    Ok(Some(result))
                } else {
                    let name = format!("tp-export-{}.zip", transfer::filename_time(chrono::Local::now().fixed_offset()));
                    if let Some(path) = rfd::AsyncFileDialog::new().set_title(choose("导出（ZIP 未加密）", "Export (unencrypted ZIP)")).add_filter("ZIP", &["zip"]).set_file_name(name).save_file().await {
                        transfer::export_archive(&store, path.path())?;
                    }
                    Ok::<_, String>(None)
                }
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.controls[2].focus(window);
                match result {
                    Ok(Some(RestoreResult::ConfirmationRequired { preview })) => {
                        this.preview = Some(preview);
                        this.error = Some(choose("保留规则或过期数量已变化，请核对后再次确认。", "Retention rules or the expired record count changed. Review and confirm again.").into());
                    }
                    Ok(Some(RestoreResult::Completed { .. })) => cx.emit(Event::Restored),
                    Ok(None) => cx.emit(Event::Closed),
                    Err(error) => {
                        if this.import {
                            this.preview = None;
                            this.sessions = Arc::new(TransferSessions::default());
                            cx.emit(Event::Failed(format!("{}{error}", choose("导入失败：", "Import failed: "))));
                        } else {
                            this.error = Some(format!("{}{error}", choose("保存失败：", "Save failed: ")));
                        }
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
// Match TransferController.reset: confirmed work survives hiding; inspection
// and unconfirmed previews may be discarded without interrupting replacement.
fn retain_transfer_on_hide(busy: bool, import: bool, has_preview: bool) -> bool {
    busy && (!import || has_preview)
}

#[cfg(test)]
mod hide_tests {
    use super::retain_transfer_on_hide;

    #[test]
    fn only_started_export_or_confirmed_import_survives_hide() {
        for import in [false, true] {
            for preview in [false, true] {
                assert!(!retain_transfer_on_hide(false, import, preview));
            }
        }
        assert!(retain_transfer_on_hide(true, false, false));
        assert!(retain_transfer_on_hide(true, true, true));
        assert!(!retain_transfer_on_hide(true, true, false));
    }
}

impl_view_render!(TransferView);

impl TransferView {
    #[cfg_attr(all(debug_assertions, feature = "hot-reload"), inline(never))]
    fn render_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let p = Palette::for_id(&self.settings.theme);
        let scale = self.settings.font_size as f32 / 13.;
        let title = if self.busy {
            if self.import {
                if self.preview.is_some() {
                    choose("正在导入…", "Importing…")
                } else {
                    choose("正在读取导出包…", "Reading export…")
                }
            } else {
                choose("正在导出…", "Exporting…")
            }
        } else if self.import {
            choose("导入？", "Import?")
        } else {
            choose("导出", "Export")
        };
        let paragraph = |text: String| div().mb(px(10.)).text_color(p.muted).child(text);
        let mut dialog = div().id("data-transfer-dialog").w(px(380.)).max_w_full().max_h_full().overflow_y_scroll().p(px(20.)).border_1().border_color(p.line).rounded(px(12.)).bg(p.surface).shadow(vec![Palette::popup_shadow()]).text_color(p.text).text_size(px(12.*scale)).line_height(px(19.2*scale))
            .child(div().mb(px(14.)).text_size(px(17.*scale)).font_weight(gpui::FontWeight(650.)).child(title))
            .child(paragraph(choose(if self.import { "将替换全部现有记录和自建分组。此操作无法撤销，应用设置沿用本机配置。" } else { "导出全部历史记录、整理信息和 TP 持有的附件。解压后可浏览内容，也可使用同一个 ZIP 重新导入 TP。" }, if self.import { "This replaces all existing records and custom groups and cannot be undone. Local app settings are kept." } else { "Export all history, organization information, and attachments stored by TP. Extract the ZIP to browse, or import it back into TP." }).into()));
        if !self.import {
            dialog = dialog.child(paragraph(choose("ZIP 包未加密，不包含应用设置和当前带敏感到期标记的记录。路径引用只保留原路径，不额外读取用户原文件。", "The ZIP is not encrypted. App settings and records with a sensitive expiry marker are excluded. File references retain their original paths; original files are not read.").into()));
        }
        if let Some(preview) = &self.preview {
            let strong_paragraph = |prefix: String, number: String, suffix: String| {
                // HTML collapses adjacent spaces introduced by translated fragments.
                let prefix = prefix.replace("  ", " ");
                let suffix = suffix.replace("  ", " ");
                let range = prefix.len()..prefix.len() + number.len();
                div().mb(px(10.)).text_color(p.muted).child(
                    gpui::StyledText::new(format!("{prefix}{number}{suffix}")).with_highlights([(
                        range,
                        gpui::HighlightStyle {
                            font_weight: Some(gpui::FontWeight::BOLD),
                            ..Default::default()
                        },
                    )]),
                )
            };
            dialog = dialog.child(strong_paragraph(
                format!("{} ", choose("导出包包含", "The export contains ")),
                preview.records.to_string(),
                format!(
                    " {}{}{}",
                    choose("条记录、", " records and "),
                    preview.groups,
                    choose("个自建分组。", " custom groups.")
                ),
            ));
            if preview.next_max_ordinary != preview.current_max_ordinary {
                dialog = dialog.child(strong_paragraph(
                    format!("{} {} {} ",choose("为保留导入数据，普通记录数量上限将从","To preserve imported data, the ordinary record limit will increase from "),preview.current_max_ordinary,choose("自动调整为"," to ")),
                    preview.next_max_ordinary.to_string(),choose("。",".").into(),
                ));
            }
            dialog = dialog.child(strong_paragraph(
                choose("按本机保留规则，","Under local retention rules, ").into(),
                preview.cleanup_count.to_string(),
                format!(" {}",choose("条普通记录会因时间过期立即清理。"," ordinary records will be cleaned up immediately because they have expired.")),
            ));
            if preview.cleanup_count > 0 {
                dialog = dialog.child(div().id("keep-expired").track_focus(&self.controls[0].clone().tab_index(0).tab_stop(!self.busy)).my(px(12.)).flex().items_start().gap(px(8.)).when(!self.busy, |d|d.cursor_pointer())
                    .child(div().mt(px(4.)).size(px(13.)).flex_shrink_0().rounded(px(2.)).border_1().border_color(if self.keep_expired { p.accent } else { p.control_border }).bg(if self.keep_expired { p.accent } else { p.surface })
                        .when(self.keep_expired, |d| d.child(crate::icons::icon("check", 11.).text_color(p.button_text))))
                    .child(div().flex_1().min_w_0().child(choose("保留这些过期记录，将其最后使用时间改为当前时间，创建时间不变。", "Keep these expired records by setting their last-used time to now. Creation times stay unchanged.")))
                    .focus(|d| d.bg(p.soft))
                    .on_click(cx.listener(|this, event, window, cx| { if !this.busy && !matches!(event, gpui::ClickEvent::Keyboard(key) if key.button == gpui::KeyboardButton::Enter) { this.controls[0].focus(window); this.keep_expired = !this.keep_expired; cx.notify(); } })));
            }
            if !preview.warnings.is_empty() {
                dialog = dialog.child(
                    div()
                        .id("transfer-warnings")
                        .track_focus(&self.controls[1])
                        .tab_index(0)
                        .focus(|d| d.bg(p.soft))
                        .cursor_pointer()
                        .child(format!(
                            "{} {}{} {}",
                            if self.warnings_open { "▾" } else { "▸" },
                            choose("文件可用性说明（", "File availability ("),
                            preview.warnings.len(),
                            choose("项）", " items)")
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.warnings_open = !this.warnings_open;
                            cx.notify();
                        })),
                );
                if self.warnings_open {
                    dialog = dialog.child(
                        div()
                            .id("transfer-warning-list")
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .pl(px(20.))
                            .children(preview.warnings.iter().map(|warning| {
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(6.))
                                    .child("•")
                                    .child(div().flex_1().min_w_0().child(warning.clone()))
                            })),
                    );
                }
            }
            dialog = dialog.child(div().mb(px(10.)).text_size(px(11.*scale)).line_height(px(16.5*scale)).text_color(p.muted).child(choose("文件缺失时沿用“文件已不可用”的标记，已有预览可能仍可显示。", "Missing files retain the “File unavailable” status. Existing previews may still be shown.")));
        }
        if let Some(error) = &self.error {
            dialog = dialog.child(div().mb(px(10.)).text_color(p.danger).child(error.clone()));
        }
        if self.busy {
            dialog = dialog.child(div().mt(px(16.)).text_color(p.accent).child(choose(
                "正在处理，请稍候。大附件可能需要较长时间。",
                "Working, please wait. Large attachments may take longer.",
            )));
        } else {
            let can_confirm = !self.import || self.preview.is_some();
            dialog = dialog.child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.))
                    .mt(px(20.))
                    .child(
                        div()
                            .id("transfer-cancel")
                            .track_focus(&self.controls[2])
                            .tab_index(0)
                            .focus(|d| d.border_color(p.accent))
                            .px(px(14.))
                            .h(px(34.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .bg(p.control)
                            .text_color(p.muted)
                            .hover(|d| d.bg(p.hover))
                            .border_1()
                            .border_color(p.line)
                            .cursor_pointer()
                            .child(choose("取消", "Cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    )
                    .child(
                        div()
                            .id("transfer-confirm")
                            .track_focus(
                                &self.controls[3].clone().tab_index(0).tab_stop(can_confirm),
                            )
                            .border_1()
                            .border_color(if self.import { p.danger_bg } else { p.accent })
                            .focus(|d| d.border_color(p.text))
                            .px(px(14.))
                            .h(px(34.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .bg(if self.import { p.danger_bg } else { p.accent })
                            .when(!can_confirm, |d| d.opacity(0.5))
                            .text_color(if self.import {
                                p.on_danger
                            } else {
                                p.button_text
                            })
                            .when(can_confirm && self.import, |d| {
                                d.hover(|d| d.bg(p.danger_bg_hover).border_color(p.danger_bg_hover))
                            })
                            .when(can_confirm && !self.import, |d| {
                                d.hover(|d| d.bg(p.accent_hover).border_color(p.accent_hover))
                            })
                            .cursor_pointer()
                            .child(choose(
                                if self.import {
                                    "确认导入"
                                } else {
                                    "选择保存位置"
                                },
                                if self.import {
                                    "Import"
                                } else {
                                    "Choose save location"
                                },
                            ))
                            .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                    ),
            );
        }
        div()
            .id("transfer-backdrop")
            .occlude()
            .track_focus(&self.focus)
            .size_full()
            .p(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x171c282e))
            .key_context("TPTransfer")
            .on_action(cx.listener(|this, _: &Next, window, cx| this.key("tab", false, window, cx)))
            .on_action(
                cx.listener(|this, _: &Previous, window, cx| this.key("tab", true, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Cancel, window, cx| this.key("escape", false, window, cx)),
            )
            .child(dialog)
            .into_any_element()
    }
}

// Match the original modal: busy work disables the checkbox and hides action
// buttons, but native <details> remains available to inspect file warnings.
fn tab_order(busy: bool, expired: bool, warnings: bool, can_confirm: bool) -> Vec<usize> {
    let mut controls = Vec::new();
    if expired && !busy {
        controls.push(0);
    }
    if warnings {
        controls.push(1);
    }
    if !busy {
        controls.push(2);
        if can_confirm {
            controls.push(3);
        }
    }
    controls
}
#[cfg(test)]
mod navigation_tests {
    use super::tab_order;
    #[test]
    fn busy_preview_preserves_warning_access_without_mutable_controls() {
        assert_eq!(tab_order(true, true, true, true), vec![1]);
        assert!(tab_order(true, true, false, true).is_empty());
        assert_eq!(tab_order(false, true, true, true), vec![0, 1, 2, 3]);
        assert_eq!(tab_order(false, false, false, true), vec![2, 3]);
        assert_eq!(tab_order(false, false, false, false), vec![2]);
    }
}
