use super::*;
use std::{cell::Cell, rc::Rc};

pub(super) struct FloatingPreview {
    from_keyboard: bool,
    owner_bounds: gpui::Bounds<gpui::Pixels>,
    owner_scale: f32,
    lifetime: Rc<crate::hover_lifetime::HoverLifetime>,
    id: String,
    open_large: Rc<Cell<bool>>,
    window: gpui::WindowHandle<crate::hover_card::HoverCard>,
    inside: Rc<Cell<bool>>,
    over_record: Rc<Cell<bool>>,
}
impl Panel {
    pub(super) fn close_keyboard_preview(&mut self, cx: &mut Context<Self>) {
        if self.keyboard_preview_pending.is_some()
            || self
                .floating
                .as_ref()
                .is_some_and(|preview| preview.from_keyboard)
        {
            self.close_floating(cx);
        }
    }
    pub(super) fn floating_owner_changed(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.floating.as_ref().is_some_and(|preview| {
            preview.owner_bounds != window.bounds() || preview.owner_scale != window.scale_factor()
        }) {
            self.close_floating(cx);
        }
    }
    pub(super) fn poll_floating(&mut self, cx: &mut Context<Self>) {
        if self.floating.as_ref().is_some_and(|preview| {
            preview.lifetime.is_closed() || !self.visible_ids().contains(&preview.id)
        }) {
            self.close_floating(cx);
        }
        let request = self.floating.as_ref().and_then(|preview| {
            preview
                .open_large
                .replace(false)
                .then(|| preview.id.clone())
        });
        if let Some(id) = request {
            if !self.hidden
                && self.settings_view.is_none()
                && self.selection.current.as_ref() == Some(&id)
            {
                self.open_large_preview(id, cx);
            }
        }
    }

    pub(super) fn open_large_preview(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(record) = self.snapshot.records.iter().find(|record| record.id == id) else {
            return;
        };
        if !matches!(record.kind.as_str(), "image" | "video") {
            return;
        }
        let error = if record.kind == "video" && !record.preview && record.files.len() < 2 {
            Some(choose("首帧预览不可用", "First-frame preview unavailable").into())
        } else {
            self.close_floating(cx);
            self.preview.open(id).err()
        };
        if let Some(error) = error {
            self.show_toast(error, cx);
        }
        cx.notify();
    }
    #[track_caller]
    pub(super) fn close_floating(&mut self, cx: &mut Context<Self>) {
        self.keyboard_preview_pending = None;
        self.hover_task = None;
        if let Some(preview) = self.floating.take() {
            preview.lifetime.close();
            preview.over_record.set(false);
            let _ = preview
                .window
                .update(cx, |_, window, _| window.remove_window());
        }
    }
    pub(super) fn request_floating(
        &mut self,
        id: String,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_floating(cx);
        if self.preview.is_active()
            || self.settings_view.is_some()
            || self.record_menu.is_some()
            || self.hidden
            || self.record_edit.is_some()
            || self.group_edit.is_some()
        {
            return;
        }
        if keyboard {
            if let Some(index) = self.visible_ids().iter().position(|item| *item == id) {
                self.scroll.scroll_to_item(index);
                self.keyboard_preview_pending = Some(id);
                cx.notify();
            }
            return;
        }
        self.hover_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(300))
                .await;
            let _ = this.update_in(cx, move |this, window, cx| {
                if this.hidden
                    || this.preview.is_active()
                    || this.settings_view.is_some()
                    || this.record_menu.is_some()
                    || this.record_edit.is_some()
                    || this.group_edit.is_some()
                {
                    return;
                }
                if this.hovered_record.as_deref() != Some(id.as_str()) {
                    return;
                }
                if let Err(error) = this.open_floating(id, false, window, cx) {
                    this.show_toast(error, cx);
                    cx.notify();
                }
            });
        }));
    }
    // Called after the rendered list has resolved the keyboard reveal and its
    // scroll dismissal. A zero-duration timer can run before that layout.
    pub(super) fn finish_keyboard_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A callback from the previous frame may precede the newly requested
        // render. Leave the request queued until update has applied its reveal.
        if self.scroll.has_pending_reveal() {
            return;
        }
        let Some(id) = self.keyboard_preview_pending.take() else {
            return;
        };
        if self.hidden
            || self.preview.is_active()
            || self.settings_view.is_some()
            || self.record_menu.is_some()
            || self.record_edit.is_some()
            || self.group_edit.is_some()
            || self.selection.current.as_ref() != Some(&id)
        {
            return;
        }
        self.hover_task = None;
        self.hovered_record = Some(id.clone());
        if std::env::var_os("TP_DIAGNOSTIC_HOVER").is_some() {
            eprintln!("floating: keyboard reveal settled");
        }
        if let Err(error) = self.open_floating(id, true, window, cx) {
            self.show_toast(error, cx);
        }
    }
    fn open_floating(
        &mut self,
        id: String,
        from_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(index) = self.visible_ids().iter().position(|item| *item == id) else {
            return Ok(());
        };
        let Some(bounds) = self.scroll.bounds_for_item(index) else {
            return Ok(());
        };
        let Some(record) = self.snapshot.records.iter().find(|r| r.id == id).cloned() else {
            return Ok(());
        };
        if std::env::var_os("TP_DIAGNOSTIC_HOVER").is_some() {
            eprintln!("floating: opening");
        }
        let row_top = f64::from(f32::from(bounds.top() + self.scroll.offset().y));
        let anchor = crate::native_window::hover_preview::anchor(window, row_top)?;
        let size = gpui::size(
            px(anchor.placement.logical_width() as f32),
            px(anchor.placement.logical_max_height() as f32),
        );
        let settings = self.snapshot.settings.clone();
        let search = self.session.search.clone();
        let group = self
            .snapshot
            .groups
            .iter()
            .find(|g| Some(g.id) == record.group_id)
            .map(|g| g.name.clone());
        let store = self.store.clone();
        let inside = Rc::new(Cell::new(false));
        let over_record = Rc::new(Cell::new(true));
        let hover_inside = inside.clone();
        let hover_record = over_record.clone();
        let open_large = Rc::new(Cell::new(false));
        let request_large = open_large.clone();
        let lifetime = Rc::new(crate::hover_lifetime::HoverLifetime::default());
        let card_lifetime = lifetime.clone();
        let display_id = window.display(cx).map(|display| display.id());
        let handle = cx
            .open_window(
                gpui::WindowOptions {
                    window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(
                        gpui::point(px(0.), px(0.)),
                        size,
                    ))),
                    titlebar: None,
                    focus: false,
                    show: false,
                    is_resizable: false,
                    is_minimizable: false,
                    is_movable: false,
                    kind: gpui::WindowKind::Normal,
                    display_id,
                    ..Default::default()
                },
                move |window, cx| {
                    cx.new(|cx| {
                        crate::hover_card::HoverCard::new(
                            store,
                            record,
                            settings,
                            search,
                            group,
                            anchor,
                            hover_inside,
                            hover_record,
                            request_large,
                            card_lifetime,
                            window,
                            cx,
                        )
                    })
                },
            )
            .map_err(|error| error.to_string())?;
        let any_handle: gpui::AnyWindowHandle = handle.into();
        any_handle
            .update(cx, |_, window, cx| window.draw(cx).clear())
            .map_err(|error| error.to_string())?;
        self.floating = Some(FloatingPreview {
            from_keyboard,
            owner_bounds: window.bounds(),
            owner_scale: window.scale_factor(),
            lifetime,
            id,
            open_large,
            window: handle,
            inside,
            over_record,
        });
        Ok(())
    }
    pub(super) fn leave_floating(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hover_task = None;
        let Some(preview) = &self.floating else {
            return;
        };
        preview.over_record.set(false);
        let inside = preview.inside.clone();
        self.hover_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(220))
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if !inside.get() {
                    this.close_floating(cx);
                }
            });
        }));
    }
}
