use super::*;

pub(super) struct GroupPress {
    pub state: tp_core::group_drag::Press,
    started: std::time::Instant,
    start_x: f32,
    rect: gpui::Bounds<gpui::Pixels>,
    pub before: Option<i64>,
    label: String,
}

impl Panel {
    pub(super) fn cancel_group_press(&mut self, cx: &mut Context<Self>) {
        self.group_press = None;
        self.group_drag_scroll = None;
        cx.notify();
    }

    pub(super) fn begin_group_press(
        &mut self,
        id: i64,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.group_click_suppressed = false;
        if self.group_edit.is_some() || self.record_edit.is_some() {
            return;
        }
        let Some(rect) = self.group_bounds.borrow().get(&id).copied() else {
            return;
        };
        let Some(group) = self.snapshot.groups.iter().find(|g| g.id == id) else {
            return;
        };
        self.group_press = Some(GroupPress {
            state: tp_core::group_drag::Press::new(
                id,
                f32::from(position.x),
                f32::from(position.y),
                0,
            ),
            started: std::time::Instant::now(),
            start_x: f32::from(position.x),
            rect,
            before: None,
            label: group.name.clone(),
        });
        self.group_drag_scroll = Some(cx.spawn_in(window, async move |panel, cx| {
            let mut delay = 200;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(delay))
                    .await;
                delay = 16;
                let keep_running = panel
                    .update_in(cx, |panel, window, cx| {
                        if panel.hidden || !window.is_window_active() {
                            panel.group_press = None;
                            return false;
                        }
                        panel.update_group_pointer(window.mouse_position(), cx);
                        let Some(press) = &mut panel.group_press else {
                            return false;
                        };
                        if press.state.active {
                            let bounds = panel.group_scroll.bounds();
                            let step = -tp_core::group_drag::edge_step(
                                press.state.x,
                                f32::from(bounds.left()),
                                f32::from(bounds.right()),
                            );
                            let previous = panel.group_scroll.offset();
                            let limit = panel.group_scroll.max_offset().width.max(px(0.));
                            let next = (previous.x + px(step)).max(-limit).min(px(0.));
                            panel.group_scroll.set_offset(gpui::point(next, previous.y));
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        }));
    }

    pub(super) fn update_group_pointer(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(press) = &mut self.group_press else {
            return;
        };
        let was_active = press.state.active;
        if !press.state.pointer_at(
            f32::from(position.x),
            f32::from(position.y),
            press.started.elapsed().as_millis() as u64,
        ) {
            self.group_press = None;
            cx.notify();
            return;
        }
        if press.state.active {
            self.group_click_suppressed = true;
            if !was_active {
                self.filter_open = false;
                self.group_menu = None;
                self.record_menu = None;
                self.move_menu = None;
                self.preview.hide();
            }
            let bounds = self.group_bounds.borrow();
            press.before = tp_core::group_drag::insertion_before(
                press.state.id,
                press.state.x,
                self.snapshot.groups.iter().filter_map(|g| {
                    bounds
                        .get(&g.id)
                        .map(|b| (g.id, f32::from(b.left()), f32::from(b.size.width)))
                }),
            );
            cx.notify();
        }
    }

    pub(super) fn finish_group_press(
        &mut self,
        event: &gpui::MouseUpEvent,
        cx: &mut Context<Self>,
    ) {
        if event.button != gpui::MouseButton::Left {
            return;
        }
        self.update_group_pointer(event.position, cx);
        let Some(press) = self.group_press.take() else {
            return;
        };
        self.group_drag_scroll = None;
        if press.state.active {
            cx.stop_propagation();
            let original: Vec<_> = self.snapshot.groups.iter().map(|g| g.id).collect();
            if let Some(order) =
                tp_core::group_drag::reordered(&original, press.state.id, press.before)
            {
                if order != original {
                    let result = operations::reorder_groups(&self.store, order);
                    self.refresh(result, cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn group_drag_overlay(&self, p: Palette) -> Option<gpui::AnyElement> {
        let press = self
            .group_press
            .as_ref()
            .filter(|press| press.state.active)?;
        let viewport = self.group_scroll.bounds();
        let selected = self.session.view == format!("group:{}", press.state.id);
        let left = tp_core::group_drag::floating_left(
            f32::from(press.rect.left()),
            press.start_x,
            press.state.x,
            f32::from(press.rect.size.width),
            f32::from(viewport.left()),
            f32::from(viewport.right()),
        );
        Some(
            div()
                .absolute()
                .left(px(left))
                .top(press.rect.top())
                .w(press.rect.size.width.min(viewport.size.width))
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .bg(if selected { p.group_soft } else { p.control })
                .text_color(if selected { p.group_text } else { p.group_idle })
                .when(selected, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                .shadow(vec![gpui::BoxShadow {
                    color: p.line.into(),
                    offset: gpui::point(px(0.), px(2.)),
                    blur_radius: px(8.),
                    spread_radius: px(0.),
                }])
                .opacity(0.7)
                .overflow_hidden()
                .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
                .child(press.label.clone())
                .into_any_element(),
        )
    }
}
