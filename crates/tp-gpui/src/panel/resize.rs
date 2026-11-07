use super::*;
use crate::native_window::resize::{self, Edge};

impl Panel {
    pub(super) fn resize_edges(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size = window.viewport_size();
        let mut layer = div()
            .absolute()
            .top_0()
            .left_0()
            .w(size.width)
            .h(size.height);
        for (id, edge) in [
            ("left", Edge::Left),
            ("right", Edge::Right),
            ("top", Edge::Top),
            ("bottom", Edge::Bottom),
        ] {
            let mut handle = div().id(id).absolute();
            handle = if edge.horizontal() {
                handle
                    .top(px(8.))
                    .bottom(px(8.))
                    .w(px(7.))
                    .h(size.height - px(16.))
                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                    .when(edge == Edge::Left, |d| d.left_0())
                    .when(edge == Edge::Right, |d| d.right_0())
                    // The settings scrollbar occupies this edge below the toolbar.
                    // Keep right-edge resizing above it instead of intercepting
                    // every scrollbar drag with an invisible resize hitbox.
                    .when(edge == Edge::Right && self.settings_view.is_some(), |d| {
                        d.bottom_auto().h(px(74.))
                    })
            } else {
                handle
                    .left(px(8.))
                    .right(px(8.))
                    .h(px(7.))
                    .w(size.width - px(16.))
                    .cursor(gpui::CursorStyle::ResizeUpDown)
                    .when(edge == Edge::Top, |d| d.top_0())
                    .when(edge == Edge::Bottom, |d| d.bottom_0())
            };
            layer = layer.child(handle.occlude().on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.close_floating(cx);
                    this.preview.hide();
                    if event.click_count == 2 {
                        match resize::reset(window, edge) {
                            Ok(reset) => {
                                this.resize_task =
                                    Some(cx.spawn_in(window, async move |this, cx| {
                                        cx.background_executor()
                                            .timer(std::time::Duration::from_millis(1))
                                            .await;
                                        if let Err(error) = reset() {
                                            let _ = this.update_in(cx, |this, _, cx| {
                                                this.show_toast(error, cx);
                                                cx.notify();
                                            });
                                        }
                                    }));
                            }
                            Err(error) => {
                                this.show_toast(error, cx);
                                cx.notify();
                            }
                        }
                        return;
                    }
                    match resize::begin(window, edge, event.position) {
                        Ok(Some(session)) => {
                            this.resize_task =
                                Some(cx.spawn_in(window, async move |this, cx| loop {
                                    cx.background_executor()
                                        .timer(std::time::Duration::from_millis(16))
                                        .await;
                                    let active = match session.update() {
                                        Ok(active) => active,
                                        Err(error) => {
                                            let _ = this.update_in(cx, |this, _, cx| {
                                                this.show_toast(error, cx);
                                                cx.notify();
                                            });
                                            false
                                        }
                                    };
                                    if !active {
                                        break;
                                    }
                                }));
                        }
                        Ok(None) => {}
                        Err(error) => {
                            this.show_toast(error, cx);
                            cx.notify();
                        }
                    }
                }),
            ));
        }
        layer
    }
}
