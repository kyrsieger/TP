use super::*;

const INDICATOR_SIZE: f32 = 24.;
const INDICATOR_MARGIN: f32 = 4.;

fn record_indicators(
    record: &tp_core::records::Record,
    p: Palette,
) -> [(bool, &'static str, gpui::Rgba); 3] {
    [
        (record.pinned_at.is_some(), "pin", p.accent),
        (record.favorite, "star-fill", p.star),
        (record.group_id.is_some(), "group", p.accent),
    ]
}

pub(super) fn record_indicator_width(record: &tp_core::records::Record, p: Palette) -> f32 {
    let count = record_indicators(record, p)
        .into_iter()
        .filter(|(visible, _, _)| *visible)
        .count();
    count as f32 * (INDICATOR_SIZE + INDICATOR_MARGIN)
}

impl Panel {
    pub(super) fn record_actions(
        &self,
        record: &tp_core::records::Record,
        media: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let hovered = self.hovered_record.as_deref() == Some(record.id.as_str());
        let mut actions = div()
            .id(gpui::SharedString::from(format!("actions:{}", record.id)))
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(2.))
            .ml(px(4.))
            .p(px(2.))
            .rounded(px(8.))
            .bg(p.surface)
            .when(media, |d| d.absolute().top(px(8.)).right(px(8.)).ml_0());
        for (action, icon, enabled) in [
            ("pin", "pin", record.pinned_at.is_some()),
            (
                "favorite",
                if record.favorite { "star-fill" } else { "star" },
                record.favorite,
            ),
            ("move", "group", record.group_id.is_some()),
            ("delete", "x", false),
        ] {
            let id = record.id.clone();
            let label = match action {
                "pin" if enabled => choose("取消置顶记录", "Unpin record"),
                "pin" => choose("置顶记录", "Pin record"),
                "favorite" if enabled => choose("取消收藏", "Remove from favorites"),
                "favorite" => choose("收藏", "Favorites"),
                "move" => choose("移动到分组", "Move to group"),
                _ => choose("删除记录", "Delete record"),
            };
            let active_color = if action == "favorite" {
                p.star
            } else if action == "delete" {
                p.danger
            } else {
                p.accent
            };
            actions = actions.child(
                div()
                    .group(gpui::SharedString::from(format!("{action}:{id}")))
                    .id(gpui::SharedString::from(format!("{action}:{id}")))
                    .size(px(24.))
                    .relative()
                    .when(
                        action == "move" && self.focused_move_trigger.as_ref() == Some(&id),
                        |d| d.track_focus(&self.move_trigger_focus),
                    )
                    .when(action == "move", |d| {
                        let bounds_cell = self.move_trigger_bounds.clone();
                        let trigger = id.clone();
                        d.child(
                            gpui::canvas(
                                move |bounds, _, _| {
                                    *bounds_cell.borrow_mut() = Some((trigger.clone(), bounds))
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                    })
                    .text_color(if enabled { active_color } else { p.muted })
                    .hover(move |d| d.bg(p.hover).text_color(active_color))
                    // The row's click recognizer suppresses its hover while pressed.
                    // Keep it from removing these controls between mouse-down and mouse-up.
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        crate::icons::icon(icon, if action == "delete" { 12. } else { 13. })
                            .text_color(if enabled { active_color } else { p.muted })
                            .group_hover(
                                gpui::SharedString::from(format!("{action}:{id}")),
                                move |d| d.text_color(active_color),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        let result = match action {
                            "pin" => {
                                operations::set_record_pinned(&this.store, id.clone(), !enabled)
                            }
                            "favorite" => {
                                operations::set_favorite(&this.store, id.clone(), !enabled)
                            }
                            "delete" => operations::delete_record(&this.store, id.clone()),
                            _ => {
                                if this
                                    .move_menu
                                    .as_ref()
                                    .is_some_and(|menu| menu.trigger == id)
                                {
                                    this.close_move(window, cx);
                                } else {
                                    let anchor = this
                                        .move_trigger_bounds
                                        .borrow()
                                        .as_ref()
                                        .filter(|(trigger, _)| trigger == &id)
                                        .map(|(_, bounds)| *bounds);
                                    this.open_move(
                                        vec![id.clone()],
                                        id.clone(),
                                        anchor,
                                        window,
                                        cx,
                                    );
                                }
                                return;
                            }
                        };
                        this.refresh(result, cx);
                    })),
            );
        }
        let mut indicators = div()
            .id(gpui::SharedString::from(format!(
                "indicators:{}",
                record.id
            )))
            .flex()
            .items_center()
            .flex_shrink_0()
            .when(media, |d| {
                d.absolute()
                    .top(px(8.))
                    .right(px(8.))
                    .rounded(px(6.))
                    .bg(gpui::Rgba {
                        a: 0.9,
                        ..p.surface
                    })
            });
        for (visible, icon, color) in record_indicators(record, p) {
            if visible {
                indicators = indicators.child(
                    div()
                        .size(px(INDICATOR_SIZE))
                        .ml(px(INDICATOR_MARGIN))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(color)
                        .child(crate::icons::icon(icon, 13.).text_color(color)),
                );
            }
        }
        // The absolute media controls must be positioned directly within the row.
        if hovered {
            actions
        } else {
            indicators
        }
    }
}
