use super::*;

impl Panel {
    pub(super) fn titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let scale = self.snapshot.settings.font_size as f32 / 13.;
        let bar = div()
            .id("panel-titlebar")
            .w_full()
            .h(px(30.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .pl(px(12.))
            .bg(p.bg)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if let Err(error) = crate::native_window::start_move(window) {
                        this.show_toast(error, cx);
                        cx.notify();
                    }
                }),
            )
            .child(crate::icons::icon("tp-mark", 16.).text_color(gpui::rgb(0x123F35)))
            .child(
                div()
                    .ml(px(7.))
                    .text_size(px(11. * scale))
                    .line_height(px(11. * scale))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(p.subtle)
                    .child("TP"),
            );
        let mut controls = div().ml_auto().h(px(30.)).flex().flex_shrink_0();
        for (id, icon, size, label) in [
            ("minimize", "minus", 13., choose("最小化", "Minimize")),
            (
                "maximize",
                "square",
                11.,
                choose("最大化或还原", "Maximize or restore"),
            ),
            ("hide", "x", 13., choose("隐藏面板", "Hide panel")),
        ] {
            controls = controls.child(
                div()
                    .id(id)
                    .group(id)
                    .w(px(44.))
                    .h(px(30.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(p.subtle)
                    .hover(move |style| {
                        if id == "hide" {
                            style
                                .bg(gpui::rgb(0xE81123))
                                .text_color(gpui::rgb(0xFFFFFF))
                        } else {
                            style.bg(p.hover)
                        }
                    })
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                    })
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        match id {
                            "minimize" => window.minimize_window(),
                            "maximize" => {
                                if let Err(error) = crate::native_window::toggle_maximize(window) {
                                    this.show_toast(error, cx);
                                    cx.notify();
                                }
                            }
                            _ => this.hide(window, cx),
                        }
                    }))
                    .child(
                        crate::icons::icon(icon, size)
                            .text_color(p.subtle)
                            .group_hover(id, move |style| {
                                style.text_color(if id == "hide" {
                                    gpui::rgb(0xFFFFFF).into()
                                } else {
                                    p.subtle
                                })
                            }),
                    ),
            );
        }
        crate::theme::typography(bar.child(controls), &self.snapshot.settings)
    }
}
