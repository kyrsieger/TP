use super::*;
impl SettingsView {
    pub(super) fn storage_cards(&self, disabled: bool, cx: &mut Context<Self>) -> gpui::Div {
        let p = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let mut actions = div().flex().flex_wrap().justify_end().gap(px(8.));
        for (import, label) in [
            (false, choose("导出", "Export")),
            (true, choose("导入", "Import")),
        ] {
            let id = if import { "import-data" } else { "export-data" };
            let focus = self
                .toggle_focus
                .borrow_mut()
                .entry(id.into())
                .or_insert_with(|| cx.focus_handle())
                .clone();
            actions = actions.child(
                div()
                    .id(id)
                    .track_focus(&focus.tab_index(0).tab_stop(!disabled))
                    .px(px(10.))
                    .py(px(7.))
                    .border_1()
                    .border_color(p.control_border)
                    .rounded(px(6.))
                    .bg(p.control)
                    .text_color(p.text)
                    .text_size(px(12. * scale))
                    .when(disabled, |d| d.opacity(0.55))
                    .when(!disabled, |d| d.cursor_pointer().hover(|d| d.bg(p.hover)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !disabled {
                            this.open_transfer(import, window, cx);
                        }
                    })),
            );
        }
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry("clear-cache".into())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        div().child(self.card().p(px(14.)).mb(px(14.))
            .child(div().mb(px(6.)).text_size(px(13.*scale)).font_weight(gpui::FontWeight::MEDIUM)
                .child(choose("导出与导入","Export and import")))
            .child(div().text_size(px(12.*scale)).line_height(px(18.*scale)).text_color(p.muted)
                .child(choose("导出包可直接浏览，也可重新导入 TP。不包含设置和敏感标记记录，ZIP 包未加密。",
                    "Browse an export or import it back into TP. Settings and sensitive records are excluded. The ZIP is not encrypted.")))
            .child(actions.mt(px(14.))))
            .child(self.card().p(px(14.))
                .child(div().mb(px(4.)).text_size(px(13.*scale)).font_weight(gpui::FontWeight::MEDIUM)
                    .child(choose("清空数据","Clear data")))
                .child(div().text_size(px(12.*scale)).line_height(px(19.2*scale)).text_color(p.muted)
                    .child(format!("{}{}{}",choose("可选择清空普通记录或全部记录，以及对应的本地文件和孤立附件。普通记录可清理大小：",
                        "Clear ordinary or all records, their local files, and orphaned attachments. Space used by ordinary records: "),self.cache_size,choose("。此操作无法撤销。",". This cannot be undone."))))
                .child(div().mt(px(14.)).flex().justify_end().child(div().id("clear-cache")
                    .track_focus(&focus.tab_index(0).tab_stop(!disabled))
                    .px(px(12.)).py(px(6.)).rounded(px(6.)).bg(p.danger_bg).text_color(p.on_danger)
                    .when(disabled, |d| d.opacity(0.55))
                    .when(!disabled,|d|d.cursor_pointer().hover(move |d|d.bg(p.danger_bg_hover)))
                    .child(choose("清空","Clear"))
                    .on_click(cx.listener(move |this,_,window,cx| {if !disabled {this.open_cleanup(window,cx);}})))))
    }
}
