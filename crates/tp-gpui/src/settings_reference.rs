//! Shortcut reference copied from SettingsPanel.tsx; labels follow saved settings.
use crate::theme::Palette;
use gpui::{div, prelude::*, px};
use tp_core::{i18n::choose, settings::Settings};

pub fn render(settings: &Settings, narrow: bool) -> impl gpui::IntoElement {
    let p = Palette::for_id(&settings.theme);
    let scale = settings.font_size as f32 / 13.;
    let shortcut = |value: &str| {
        if value.is_empty() {
            choose("未设置", "Not set").to_owned()
        } else {
            crate::shortcuts::display(value)
        }
    };
    let rows = vec![
        (shortcut(&settings.shortcut), choose("唤出面板（全局快捷键）", "Open panel (global shortcut)")),
        (shortcut(&settings.rename_shortcut), choose("重命名当前记录（仅记录列表）", "Rename current record (record list only)")),
        ("Ctrl+F".into(), choose("聚焦搜索框", "Focus search")),
        ((if cfg!(target_os="macos") { "Delete / Fn+Delete" } else { "Backspace / Delete" }).into(), choose("搜索框聚焦时编辑搜索词", "Edit search text when search is focused")),
        ("Tab / Shift+Tab".into(), choose("切换到下一个 / 上一个分组", "Switch to next / previous group")),
        ("←".into(), choose("选中图片或视频记录时打开大图预览", "Open large preview of selected image or video")),
        ("→".into(), choose("显示当前选中记录的悬浮预览", "Show hover preview of selected record")),
        ("↑ / ↓".into(), choose("选择上一条 / 下一条记录，首尾循环；分组菜单中切换选项", "Select previous / next record, wrapping at the ends; move through group menu options")),
        (choose("Shift+↑ / Shift+↓、Shift+点击", "Shift+↑ / Shift+↓, Shift+click").into(), choose("按当前列表顺序连续选择多条记录", "Select a continuous range of records")),
        ("Space".into(), choose("打开或关闭大图预览；分组菜单中确认选择", "Toggle large preview; confirm selection in group menu")),
        ("1～9".into(), choose("搜索框未聚焦时应用当前可见序号对应的记录", "Apply the visible numbered record when search is not focused")),
        ("Ctrl+1～9".into(), choose("应用当前可见序号对应的记录，搜索框聚焦时也可使用", "Apply the visible numbered record, even when search is focused")),
        ("Ctrl+D".into(), choose("收藏或取消收藏所有选中记录", "Toggle favorites for all selected records")),
        ((if cfg!(target_os="macos") { "Ctrl+Delete" } else { "Ctrl+Backspace / Delete" }).into(), choose("搜索框未聚焦时删除所有选中记录", "Delete all selected records when search is not focused")),
        ("Ctrl+G".into(), choose("打开分组菜单，移动所有选中记录", "Open group menu to move all selected records")),
        ("Ctrl+P".into(), choose("置顶或取消置顶所有选中记录", "Toggle pinning for all selected records")),
        ("Ctrl+A".into(), choose("输入框聚焦时全选框内文字；面板其他区域禁用全选", "Select all text in a focused input; disabled elsewhere in the panel")),
        (choose("长按分组并拖拽", "Hold and drag a group").into(), choose("长按自建分组 200 毫秒后左右拖拽排序，松开保存；默认和收藏不参与排序", "Hold a custom group for 200 ms, then drag horizontally to reorder and release to save. Default and Favorites stay fixed")),
    ];
    let x = if narrow { 10. } else { 14. };
    let y = if narrow { 8. } else { 9. };
    let mut table = div()
        .w_full()
        .text_size(px(12. * scale))
        .line_height(px(12. * scale * 1.55))
        .text_color(p.muted)
        .child(
            div()
                .flex()
                .border_b_1()
                .border_color(p.line)
                .text_color(p.text)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(
                    div()
                        .w(gpui::relative(0.42))
                        .flex_shrink_0()
                        .px(px(x))
                        .py(px(y))
                        .child(choose("按键", "Key")),
                )
                .child(
                    div()
                        .flex_1()
                        .px(px(x))
                        .py(px(y))
                        .child(choose("作用", "Action")),
                ),
        );
    let count = rows.len();
    for (index, (key, label)) in rows.into_iter().enumerate() {
        // CSS overflow-wrap:anywhere permits breaks even inside a long shortcut.
        let wrapped_key = key
            .chars()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("\u{200b}");
        table = table.child(
            div()
                .flex()
                .items_start()
                .when(index + 1 < count, |d| d.border_b_1().border_color(p.line))
                .child(
                    div()
                        .w(gpui::relative(0.42))
                        .flex_shrink_0()
                        .px(px(x))
                        .py(px(y))
                        .text_color(p.text)
                        .flex()
                        .flex_col()
                        .items_start()
                        .child(
                            div()
                                .max_w_full()
                                .px(px(6.))
                                .py(px(2.))
                                .border_1()
                                .border_color(p.control_border)
                                .rounded(px(5.))
                                .bg(p.control)
                                .text_size(px(11. * scale))
                                .line_height(px(11. * scale * 1.5))
                                .shadow(vec![gpui::BoxShadow {
                                    color: p.line.into(),
                                    offset: gpui::point(px(0.), px(1.)),
                                    blur_radius: px(0.),
                                    spread_radius: px(0.),
                                }])
                                .child(wrapped_key),
                        ),
                )
                .child(div().flex_1().min_w_0().px(px(x)).py(px(y)).child(label)),
        );
    }
    table
}
