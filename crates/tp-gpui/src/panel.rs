use crate::theme::Palette;
mod actions;
mod context_menu;
mod floating;
mod geometry;
mod group_drag;
mod groups;
mod idle;
mod move_menu;
mod record_display;
mod record_menu;
mod resize;
mod thumbnails;
mod titlebar;
mod toast;
mod viewport;
use gpui::Focusable;
gpui::actions!(tp_panel, [FocusSearch, NextGroup, PreviousGroup]);
use gpui::{
    div, prelude::*, px, Context, Entity, EntityInputHandler, FocusHandle, IntoElement,
    KeyDownEvent, ScrollHandle, Subscription, Window,
};
use gpui_component::input::{Input, InputEvent, InputState};
use std::sync::{atomic::Ordering, Arc};
use tp_core::{browse::PanelSession, i18n::choose, operations, Snapshot};
use tp_desktop::Store;

pub struct Panel {
    store: Arc<Store>,
    snapshot: Snapshot,
    settings_view: Option<Entity<crate::settings_view::SettingsView>>,
    settings_subscription: Option<Subscription>,
    available_update: Option<crate::updates::Release>,
    session: PanelSession,
    hidden: bool,
    idle: idle::IdleContent,
    hovered_record: Option<String>,
    floating: Option<floating::FloatingPreview>,
    hover_task: Option<gpui::Task<()>>,
    keyboard_preview_pending: Option<String>,
    geometry_save: Option<gpui::Task<()>>,
    resize_task: Option<gpui::Task<()>>,
    geometry_dirty: bool,
    observed_geometry: (gpui::Bounds<gpui::Pixels>, f32),
    placing_until: std::time::Instant,
    search: Entity<InputState>,
    search_placeholder: &'static str,
    selection: tp_core::selection::Selection,
    list_focus: FocusHandle,
    panel_focus: FocusHandle,
    move_trigger_focus: FocusHandle,
    focused_move_trigger: Option<String>,
    group_focus: FocusHandle,
    focused_group_view: String,
    group_space_pending: Option<String>,
    scroll: viewport::Viewport,
    observed_scroll_offset: gpui::Point<gpui::Pixels>,
    group_scroll: ScrollHandle,
    group_press: Option<group_drag::GroupPress>,
    group_click_suppressed: bool,
    group_bounds:
        std::rc::Rc<std::cell::RefCell<std::collections::HashMap<i64, gpui::Bounds<gpui::Pixels>>>>,
    group_drag_scroll: Option<gpui::Task<()>>,
    filter_open: bool,
    filter_trigger_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    move_menu: Option<move_menu::MoveMenu>,
    keyboard_move_pending: Option<move_menu::PendingMove>,
    move_trigger_bounds:
        std::rc::Rc<std::cell::RefCell<Option<(String, gpui::Bounds<gpui::Pixels>)>>>,
    group_edit: Option<groups::GroupEditor>,
    group_menu: Option<groups::GroupMenu>,
    record_edit: Option<record_menu::RecordEditor>,
    record_menu: Option<record_menu::RecordMenu>,
    thumbnails: std::collections::HashMap<String, thumbnails::Thumbnail>,
    thumbnail_request: u64,
    preview: crate::preview::Preview,
    toast: Option<toast::Toast>,
    toast_task: Option<gpui::Task<()>>,
    pending_startup_error: Option<String>,
    _subscriptions: Vec<Subscription>,
    _poll: gpui::Task<()>,
}

impl Panel {
    fn show_toast(&mut self, message: String, cx: &mut Context<Self>) {
        self.toast = Some(toast::Toast::new(
            message,
            self.toast.as_ref(),
            std::time::Instant::now(),
        ));
        self.toast_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(toast::VISIBLE).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                return;
            }
            cx.background_executor().timer(toast::FADE).await;
            let _ = this.update(cx, |this, cx| {
                this.toast = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(crate) fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.group_space_pending = None;
        self.keyboard_move_pending = None;
        self.scroll.cancel_return();
        cx.stop_active_drag(window);
        self.cancel_group_press(cx);
        self.persist_geometry(window);
        self.geometry_save = None;
        self.resize_task = None;
        self.hidden = true;
        self.available_update = None;
        self.idle.hide(std::time::Instant::now());
        self.close_floating(cx);
        self.hovered_record = None;
        self.preview.hide();
        let retain_work = self.settings_view.clone().is_some_and(|view| {
            view.update(cx, |settings, cx| settings.retain_on_hide(window, cx))
        });
        if !retain_work {
            self.settings_view = None;
            self.settings_subscription = None;
        }
        self.filter_open = false;
        self.move_menu = None;
        self.focused_move_trigger = None;
        self.group_menu = None;
        self.record_menu = None;
        self.group_edit = None;
        self.record_edit = None;
        self.clear_hidden_browse(window, cx);
        crate::native_window::hide(window);
        cx.notify();
    }
    fn clear_hidden_browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.hidden {
            return;
        }
        self.session.clear_hidden(
            self.snapshot.settings.retain_search_input,
            self.snapshot.settings.retain_type_filter,
        );
        if self.search.read(cx).value().as_str() != self.session.search {
            let search = self.session.search.clone();
            self.search
                .update(cx, |input, cx| input.set_value(search, window, cx));
        }
        self.reconcile();
    }
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.idle.show();
        self.persist_geometry(window);
        self.preview.hide();
        self.close_floating(cx);
        self.refresh(Ok(()), cx);
        self.clear_hidden_browse(window, cx);
        self.hidden = false;
        if let Some(error) = self.pending_startup_error.take() {
            self.show_toast(error, cx);
        }
        self.session.view = "default".into();
        self.selection = Default::default();
        self.reconcile();
        self.scroll.scroll_to_item(0);
        self.panel_focus.focus(window);
        if let Some(settings) = self.settings_view.clone() {
            settings.update(cx, |settings, cx| settings.focus_work(window, cx));
        }
        if let Err(error) =
            crate::native_window::set_always_on_top(window, self.snapshot.settings.always_on_top)
        {
            self.show_toast(error, cx);
        }
        self.place_window(window, cx);
        crate::native_window::show(window);
        if let Err(error) =
            crate::native_window::set_taskbar_icon(window, self.snapshot.settings.show_taskbar_icon)
        {
            self.show_toast(error, cx);
        }
        cx.notify();
    }
    fn cycle_group(&mut self, direction: isize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(settings) = self.settings_view.clone() {
            if settings.read(cx).has_modal() {
                cx.stop_propagation();
                return;
            }
            settings.update(cx, |settings, cx| {
                settings.leave_language_for_tab(window, cx)
            });
            if settings.update(cx, |settings, cx| {
                settings.font_search_tab(direction < 0, window, cx)
                    || settings.font_style_tab(direction < 0, window, cx)
                    || settings.fixed_position_tab(direction < 0, window, cx)
                    || settings.theme_tab(direction < 0, window, cx)
                    || settings.retention_tab(direction < 0, window, cx)
            }) {
                return;
            }
            if direction < 0 {
                window.focus_prev();
            } else {
                window.focus_next();
            }
            return;
        }
        if self.record_edit.take().is_some() {
            self.list_focus.focus(window);
            cx.notify();
            return;
        }
        self.record_menu = None;
        if self.group_edit.take().is_some() {
            self.list_focus.focus(window);
            cx.notify();
            return;
        }
        self.group_menu = None;
        if self.move_menu.take().is_some() {
            self.list_focus.focus(window);
        }
        if self.search.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        let mut views = vec!["default".to_owned(), "favorite".to_owned()];
        views.extend(
            self.snapshot
                .groups
                .iter()
                .map(|g| format!("group:{}", g.id)),
        );
        let index = views
            .iter()
            .position(|v| *v == self.session.view)
            .unwrap_or(0);
        let next = (index as isize + direction).rem_euclid(views.len() as isize) as usize;
        self.session.view = views[next].clone();
        self.filter_open &= self.session.view == "default";
        self.group_scroll.scroll_to_item(next);
        self.focus_group(self.session.view.clone(), window);
        self.record_edit = None;
        self.selection = Default::default();
        self.reconcile();
        self.scroll.scroll_to_item(0);
        cx.notify();
    }
    fn visible_ids(&self) -> Vec<String> {
        self.session
            .filtered(&self.snapshot.records)
            .iter()
            .map(|r| r.id.clone())
            .collect()
    }

    fn reconcile(&mut self) {
        if let Some(id) = self
            .session
            .view
            .strip_prefix("group:")
            .and_then(|v| v.parse::<i64>().ok())
        {
            if !self.snapshot.groups.iter().any(|g| g.id == id) {
                self.session.view = "default".into();
            }
        }
        // Match PanelController.refresh: resolve a removed group first, then
        // reconcile the editor and selection against the resulting view.
        let visible = self.visible_ids();
        if self
            .record_edit
            .as_ref()
            .is_some_and(|edit| !visible.contains(&edit.id))
        {
            self.record_edit = None;
        }
        self.selection.reconcile(&visible);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(settings) = self.settings_view.clone() {
            if event.keystroke.key == "escape"
                && !self.search.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
                && settings.update(cx, |settings, cx| settings.panel_escape_allowed(window, cx))
            {
                if self.filter_open {
                    self.filter_open = false;
                    cx.notify();
                } else {
                    settings.update(cx, |settings, cx| settings.escape(window, cx));
                }
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        if let Some(press) = &self.group_press {
            if event.keystroke.key == "escape" {
                self.cancel_group_press(cx);
                cx.stop_propagation();
                window.prevent_default();
                return;
            }
            if press.state.active {
                cx.stop_propagation();
                window.prevent_default();
                return;
            }
        }
        if self.record_edit_key(event, window, cx) || self.record_menu_key(event, window, cx) {
            return;
        }
        if self.group_edit_key(event, window, cx) || self.group_menu_key(event, window, cx) {
            return;
        }
        if self.move_key(event, window, cx) {
            return;
        }
        if self.move_trigger_key(event, window, cx) {
            return;
        }
        // Never intercept candidate selection or confirmation while an IME is composing.
        if self.search.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        let editing = self.search.read(cx).focus_handle(cx).is_focused(window);
        if (self.list_focus.is_focused(window) || self.move_trigger_focus.is_focused(window))
            && self.rename_key_matches(event)
        {
            if !event.is_held {
                if let Some(id) = self.selection.current.clone() {
                    self.start_record_edit(id, window, cx);
                }
            }
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if self.filter_open && key == "escape" {
            self.filter_open = false;
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
            return;
        }
        // The original panel handles navigation regardless of modifiers. Only
        // operation shortcuts (digits, favorite, pin, move, delete and space)
        // exclude Alt/platform modifiers.
        if (m.alt || m.platform)
            && !matches!(
                key,
                "up" | "down" | "left" | "right" | "tab" | "enter" | "escape"
            )
        {
            return;
        }
        match key {
            "space" if self.group_focus.is_focused(window) && !m.control && !m.shift => {
                if !event.is_held {
                    self.group_space_pending = Some(self.focused_group_view.clone());
                }
            }
            "right" if !editing => {
                if !event.is_held {
                    if let Some(id) = self.selection.current.clone() {
                        self.request_floating(id, true, window, cx);
                    }
                }
            }

            "left" if !editing => {
                if !event.is_held {
                    if let Some(id) = self.selection.current.clone() {
                        self.open_large_preview(id, cx);
                    }
                }
            }
            "space" if !editing && !m.control && !m.shift => {
                if !event.is_held {
                    if let Some(id) = self.selection.current.clone() {
                        self.open_large_preview(id, cx);
                    }
                }
            }
            "g" if m.control && !m.shift => {
                if !event.is_held {
                    if let Some(trigger) = self.selection.current.clone() {
                        let ids: Vec<_> = self
                            .visible_ids()
                            .into_iter()
                            .filter(|id| self.selection.ids.contains(id))
                            .collect();
                        if let Some(index) = self.visible_ids().iter().position(|id| *id == trigger)
                        {
                            self.scroll.scroll_to_item(index);
                        }
                        self.close_floating(cx);
                        self.keyboard_move_pending = Some(move_menu::PendingMove {
                            ids,
                            trigger,
                            view: self.session.view.clone(),
                            search: self.session.search.clone(),
                            kind_filter: self.session.kind_filter.clone(),
                        });
                    }
                }
            }
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
                if !m.shift && (!editing || m.control) =>
            {
                if !event.is_held {
                    let index = self.scroll.top_item() + key.parse::<usize>().unwrap() - 1;
                    if index <= self.scroll.bottom_item() {
                        if let Some(id) = self.visible_ids().get(index).cloned() {
                            self.apply(id, window, cx);
                        }
                    }
                }
            }
            "up" | "down" => {
                self.scroll.cancel_return();
                self.close_keyboard_preview(cx);
                self.keyboard_move_pending = None;
                let ids = self.visible_ids();
                if let Some(index) =
                    self.selection
                        .step(&ids, if key == "up" { -1 } else { 1 }, m.shift)
                {
                    self.scroll.scroll_to_item(index);
                    self.list_focus.focus(window);
                }
            }
            "tab" => self.cycle_group(if m.shift { -1 } else { 1 }, window, cx),
            "enter" => {
                if let Some(id) = self.selection.current.clone() {
                    self.apply(id, window, cx);
                } else if self.group_focus.is_focused(window) && !event.is_held {
                    self.activate_focused_group(cx);
                }
            }
            "escape" => {
                if !self.session.search.is_empty() {
                    self.search
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.session.search.clear();
                    self.reconcile();
                    self.list_focus.focus(window);
                } else {
                    self.hide(window, cx);
                }
            }
            "d" | "p" if m.control && !m.shift => {
                if !event.is_held {
                    let selected: Vec<_> = self
                        .session
                        .filtered(&self.snapshot.records)
                        .into_iter()
                        .filter(|r| self.selection.ids.contains(&r.id))
                        .collect();
                    let operation = if key == "d" {
                        operations::BatchOperation::SetFavorite {
                            favorite: !selected.iter().all(|r| r.favorite),
                        }
                    } else {
                        operations::BatchOperation::SetPinned {
                            pinned: !selected.iter().all(|r| r.pinned_at.is_some()),
                        }
                    };
                    let ids = selected.iter().map(|r| r.id.clone()).collect();
                    let result = operations::batch_records(&self.store, ids, operation);
                    self.refresh(result, cx);
                }
            }
            "delete" if cfg!(windows) && !editing && !m.control && !m.shift => {
                self.delete_selected(event.is_held, cx)
            }
            "backspace" if !editing && m.control && !m.shift => {
                self.delete_selected(event.is_held, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }

    fn delete_selected(&mut self, held: bool, cx: &mut Context<Self>) {
        if held {
            return;
        }
        let result = operations::batch_records(
            &self.store,
            self.selection.ids.iter().cloned().collect(),
            operations::BatchOperation::Delete,
        );
        self.refresh(result, cx);
    }

    fn apply(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let result = tp_desktop::clipboard::write_record(&self.store, id);
        if result.is_ok() {
            let target = self
                .store
                .paste_target
                .lock()
                .ok()
                .and_then(|mut target| target.take());
            if let Some(target) = target {
                if !self.snapshot.settings.always_on_top {
                    self.hide(window, cx);
                }
                cx.background_executor()
                    .spawn(async move {
                        tp_desktop::platform::paste_to_target(target);
                    })
                    .detach();
            }
        }
        self.refresh(result, cx);
    }

    pub fn new(
        store: Arc<Store>,
        snapshot: Snapshot,
        show_on_start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(choose("搜索 · Ctrl+F", "Search · Ctrl+F"))
        });
        let subscription = cx.subscribe_in(&search, window, |this, input, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.close_floating(cx);
                // A user browse change cancels editing even when the same
                // record still matches the new search (unlike data refresh).
                this.record_edit = None;
                this.session.search = input.read(cx).value().to_string();
                this.clear_hidden_browse(window, cx);
                this.reconcile();
                this.scroll.scroll_to_item(0);
                cx.notify();
            }
        });
        let group_focus = cx.focus_handle();
        let group_blur_subscription = cx.on_blur(&group_focus, window, |this, _, _| {
            // Key-up may be delivered elsewhere. Do not retain an armed
            // button across focus changes, even if focus later returns.
            this.group_space_pending = None;
        });
        let bounds_subscription = cx.observe_window_bounds(window, Self::bounds_changed);
        let activation_subscription = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.group_space_pending = None;
                this.cancel_group_press(cx);
            }
        });
        let mut last_revision = 0;
        let poll = cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            if this
                .update_in(cx, |this, window, cx| {
                    let host = cx.global::<crate::host::Host>();
                    let revision = host.background.revision.load(Ordering::Acquire);
                    let (open, quit) = host.poll();
                    if quit {
                        this.persist_geometry(window);
                        this.preview.hide();
                        this.close_floating(cx);
                        cx.quit();
                        return;
                    }
                    if open {
                        this.show(window, cx);
                    }
                    if last_revision != revision && this.idle.needs_snapshot(this.hidden) {
                        last_revision = revision;
                        this.refresh(Ok(()), cx);
                        this.clear_hidden_browse(window, cx);
                    }
                    if this.hidden && this.idle.release_due(std::time::Instant::now()) {
                        this.close_floating(cx);
                        this.clear_thumbnails(window);
                        this.snapshot.records = Vec::new();
                        this.snapshot.groups = Vec::new();
                        this.selection = Default::default();
                        this.scroll
                            .update(tp_core::list_layout::ListLayout::new(std::iter::empty()));
                        cx.notify();
                    }
                    this.poll_floating(cx);
                    match this.preview.poll() {
                        Ok(true) => {
                            crate::native_window::activate(window);
                            this.list_focus.focus(window);
                        }
                        Ok(false) => {}
                        Err(error) => {
                            this.show_toast(error, cx);
                            cx.notify();
                        }
                    }
                })
                .is_err()
            {
                break;
            }
        });
        let placement_error = crate::native_window::place(window, &snapshot.settings).err();
        let taskbar_error =
            crate::native_window::set_taskbar_icon(window, snapshot.settings.show_taskbar_icon)
                .err();
        let startup_error =
            crate::native_window::set_always_on_top(window, snapshot.settings.always_on_top)
                .err()
                .or(taskbar_error)
                .or(placement_error)
                .or_else(|| cx.global::<crate::host::Host>().startup_error.clone());
        let mut panel = Self {
            preview: crate::preview::Preview::new(
                store.clone(),
                crate::native_window::owner(window),
            ),
            store,
            snapshot,
            settings_view: None,
            available_update: None,
            resize_task: None,
            settings_subscription: None,
            session: PanelSession::default(),
            hidden: !show_on_start,
            idle: idle::IdleContent::at_start(show_on_start),
            hovered_record: None,
            floating: None,
            hover_task: None,
            keyboard_preview_pending: None,
            geometry_save: None,
            geometry_dirty: false,
            observed_geometry: (window.bounds(), window.scale_factor()),
            placing_until: std::time::Instant::now() + std::time::Duration::from_millis(350),
            search,
            search_placeholder: choose("搜索 · Ctrl+F", "Search · Ctrl+F"),
            selection: Default::default(),
            list_focus: cx.focus_handle(),
            panel_focus: cx.focus_handle(),
            move_trigger_focus: cx.focus_handle(),
            focused_move_trigger: None,
            group_focus,
            focused_group_view: "default".into(),
            group_space_pending: None,
            scroll: viewport::Viewport::new(),
            observed_scroll_offset: Default::default(),
            group_scroll: ScrollHandle::new(),
            group_press: None,
            group_click_suppressed: false,
            group_bounds: Default::default(),
            group_drag_scroll: None,
            filter_open: false,
            filter_trigger_bounds: Default::default(),
            move_menu: None,
            keyboard_move_pending: None,
            move_trigger_bounds: Default::default(),
            group_edit: None,
            group_menu: None,
            record_edit: None,
            record_menu: None,
            thumbnails: Default::default(),
            thumbnail_request: 0,
            toast: None,
            toast_task: None,
            pending_startup_error: startup_error,
            _subscriptions: vec![
                subscription,
                group_blur_subscription,
                bounds_subscription,
                activation_subscription,
            ],
            _poll: poll,
        };
        panel.reconcile();
        if show_on_start {
            panel.panel_focus.focus(window);
            if let Some(error) = panel.pending_startup_error.take() {
                panel.show_toast(error, cx);
            }
        }
        panel
    }

    fn refresh(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.keyboard_move_pending = None;
        self.scroll.cancel_return();
        if let Err(error) = result {
            self.show_toast(error, cx);
        }
        // Retained settings/import/cleanup tasks may finish after the idle
        // release. Keep their completion/error handling, but do not repopulate
        // the hidden list. show() resets idle before reading the latest store.
        if !self.idle.needs_snapshot(self.hidden) {
            cx.notify();
            return;
        }
        match self.store.snapshot() {
            Ok(snapshot) => {
                let previous = &self.snapshot.settings;
                let next = &snapshot.settings;
                if previous.language != next.language {
                    tp_core::i18n::set_preference(&next.language);
                }
                if previous.theme != next.theme
                    || previous.font_size != next.font_size
                    || previous.font_family != next.font_family
                    || previous.font_style != next.font_style
                {
                    Palette::apply(next, cx);
                }
                self.preview
                    .reconcile(snapshot.records.iter().map(|record| record.id.as_str()));
                self.retry_failed_thumbnails();
                self.snapshot = snapshot;
                if let Some(settings) = &self.settings_view {
                    settings.update(cx, |settings, cx| {
                        settings.sync_record_count(&self.snapshot, cx);
                        settings.sync_always_on_top(self.snapshot.settings.always_on_top, cx)
                    });
                }
            }
            Err(error) => self.show_toast(error, cx),
        }
        self.reconcile();
        cx.notify();
    }

    // Observe the actual offset, including keyboard reveal, animation and layout
    // clamping. A wheel event at the list boundary is not a scroll.
    fn dismiss_scrolled_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let offset = self.scroll.displayed_offset();
        if offset == self.observed_scroll_offset {
            return false;
        }
        if cfg!(debug_assertions) && std::env::var_os("TP_DIAGNOSTIC_LAYOUT").is_some() {
            eprintln!(
                "list offset changed: {:?} -> {:?}; menu={:?}",
                self.observed_scroll_offset,
                offset,
                self.record_menu.as_ref().map(|menu| menu.scroll_offset)
            );
        }
        self.observed_scroll_offset = offset;
        let keyboard_preview = self.keyboard_preview_pending.take();
        self.close_floating(cx);
        self.keyboard_preview_pending = keyboard_preview;
        if self.move_menu.is_some() {
            self.close_move(window, cx);
        }
        // A delayed layout notification must not dismiss a menu opened at the
        // already-current position (same rule as RecordList.onScroll).
        if self
            .record_menu
            .as_ref()
            .is_some_and(|menu| menu.scroll_offset != offset)
        {
            self.record_menu = None;
            self.list_focus.focus(window);
        }
        true
    }

    fn tab(
        &self,
        id: String,
        label: String,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.session.view == id;
        let back_to_top = selected && self.scroll.show_back_to_top();
        let group_id = id
            .strip_prefix("group:")
            .and_then(|v| v.parse::<i64>().ok());
        let dragging = self
            .group_press
            .as_ref()
            .is_some_and(|press| press.state.active && Some(press.state.id) == group_id);
        div()
            .id(gpui::SharedString::from(id.clone()))
            .h(px(26.))
            .flex_shrink_0()
            .px(px(10.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
            .cursor_pointer()
            .bg(if selected { p.group_soft } else { p.control })
            .text_color(if selected { p.group_text } else { p.group_idle })
            .when(selected, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
            // GPUI permits one hover refinement per element, including during dragging.
            .hover(move |d| {
                d.bg(if dragging {
                    gpui::rgba(0)
                } else {
                    p.group_soft
                })
            })
            .when_some(
                group_id.filter(|_| self.group_edit.is_none() && self.record_edit.is_none()),
                |d, group_id| {
                    let measured = self.group_bounds.clone();
                    let active = self.group_press.as_ref().filter(|press| press.state.active);
                    let source = active.is_some_and(|press| press.state.id == group_id);
                    let before = active.is_some_and(|press| press.before == Some(group_id));
                    let last = self
                        .snapshot
                        .groups
                        .last()
                        .is_some_and(|g| g.id == group_id);
                    let at_end = last && active.is_some_and(|press| press.before.is_none());
                    d.relative()
                        .when(source, |d| d.bg(gpui::rgba(0)).text_color(gpui::rgba(0)))
                        .child(
                            gpui::canvas(
                                move |rect, _, _| {
                                    measured.borrow_mut().insert(group_id, rect);
                                },
                                move |rect, _, window, _| {
                                    if before || at_end {
                                        let x = if before {
                                            rect.left() - px(3.)
                                        } else {
                                            rect.right() + px(1.)
                                        };
                                        window.paint_quad(
                                            gpui::fill(
                                                gpui::Bounds::new(
                                                    gpui::point(x, rect.top() + px(3.)),
                                                    gpui::size(px(2.), px(20.)),
                                                ),
                                                p.group_text,
                                            )
                                            .corner_radii(px(1.)),
                                        );
                                    }
                                },
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.begin_group_press(group_id, event.position, window, cx);
                            }),
                        )
                },
            )
            .child(
                div()
                    .relative()
                    .child(div().when(back_to_top, |d| d.opacity(0.)).child(label))
                    .when(back_to_top, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    crate::icons::icon("arrow-up-to-line", 15.)
                                        .text_color(p.group_text),
                                ),
                        )
                    }),
            )
            .when(back_to_top, |d| {
                d.tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new(choose("返回顶部", "Back to top"))
                        .build(window, cx)
                })
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if this.group_press.is_some() {
                        this.cancel_group_press(cx);
                        cx.stop_propagation();
                        return;
                    }
                    if let Some(id) = group_id {
                        this.open_group_menu(id, event.position, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if this.group_click_suppressed && event.click_count() > 0 {
                        cx.stop_propagation();
                        return;
                    }
                    this.focus_group(id.clone(), window);
                    if back_to_top {
                        this.scroll.return_to_top(
                            crate::native_window::reduced_motion(),
                            std::time::Instant::now(),
                        );
                        cx.notify();
                        return;
                    }
                    this.session.view = id.clone();
                    this.record_edit = None;
                    this.filter_open &= this.session.view == "default";
                    this.selection = Default::default();
                    this.reconcile();
                    this.scroll.scroll_to_item(0);
                    if event.click_count() == 2 {
                        if let Some(id) = group_id {
                            this.start_group_edit(Some(id), window, cx);
                        }
                    }
                    cx.notify();
                }),
            )
    }
}

impl_view_render!(Panel);

impl Panel {
    #[cfg_attr(all(debug_assertions, feature = "hot-reload"), inline(never))]
    fn render_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.scroll.advance_return(std::time::Instant::now()) {
            window.request_animation_frame();
        }
        self.dismiss_scrolled_overlays(window, cx);
        let placeholder = choose("搜索 · Ctrl+F", "Search · Ctrl+F");
        if self.search_placeholder != placeholder {
            self.search_placeholder = placeholder;
            self.search.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx);
            });
        }
        if self.hidden && self.idle.released {
            return div().size_full().into_any_element();
        }
        *self.move_trigger_bounds.borrow_mut() = None;
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let mut tabs = div()
            .id("groups")
            .track_focus(&self.group_focus)
            .on_key_up(cx.listener(Self::group_key_up))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(5.))
            .h(px(38.))
            .pl(px(14.))
            .pr(px(10.))
            .pb(px(4.))
            .overflow_x_scroll()
            .track_scroll(&self.group_scroll)
            .child(self.tab("default".into(), choose("默认", "Default").into(), p, cx))
            .child(self.tab("favorite".into(), choose("收藏", "Favorites").into(), p, cx));
        for group in &self.snapshot.groups {
            tabs = if let Some(input) = self.group_input(Some(group.id), window, cx) {
                tabs.child(input)
            } else {
                tabs.child(self.tab(format!("group:{}", group.id), group.name.clone(), p, cx))
            };
        }
        tabs = if let Some(input) = self.group_input(None, window, cx) {
            tabs.child(input)
        } else {
            tabs.child(
                div()
                    .id("add-group")
                    .size(px(26.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(p.control)
                    .hover(|d| d.bg(p.group_soft))
                    .cursor_pointer()
                    .child(crate::icons::icon("plus", 14.).text_color(p.group_idle))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.start_group_edit(None, window, cx)),
                    ),
            )
        };
        let filter_disabled = self.session.view != "default";
        let filter_active = !filter_disabled && self.session.kind_filter != "all";
        let filter_color = if filter_disabled {
            p.disabled
        } else if filter_active {
            p.accent
        } else {
            p.subtle
        };
        let filter_icon = if filter_active {
            self.session.kind_filter.as_str()
        } else {
            "filter"
        };
        let filter_bounds = self.filter_trigger_bounds.clone();
        let filter_control = div()
            .relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| filter_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .flex()
            .flex_shrink_0()
            .h(px(28.))
            .rounded(px(7.))
            .when(filter_active, |d| d.bg(p.soft))
            .child(
                div()
                    .id("filter")
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .when(!filter_disabled, |d| {
                        d.cursor_pointer()
                            .hover(|d| d.bg(if filter_active { p.soft_hover } else { p.soft }))
                    })
                    .child(crate::icons::icon(filter_icon, 14.).text_color(filter_color))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.session.view == "default" {
                            this.filter_open = !this.filter_open;
                            cx.notify();
                        }
                    })),
            )
            .when(filter_active, |d| {
                d.child(
                    div()
                        .id("clear-filter")
                        .rounded_r(px(7.))
                        .w(px(20.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|d| d.bg(p.soft_hover))
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new(choose(
                                "清除筛选",
                                "Clear filter",
                            ))
                            .build(window, cx)
                        })
                        .child(crate::icons::icon("x", 11.).text_color(p.accent))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.session.kind_filter = "all".into();
                            this.record_edit = None;
                            this.filter_open = false;
                            this.reconcile();
                            this.scroll.scroll_to_item(0);
                            cx.notify();
                        })),
                )
            });
        let search_focus = self.search.read(cx).focus_handle(cx);
        let search_focused = search_focus.contains_focused(window, cx);
        let toolbar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.))
            .h(px(52.))
            .pl(px(14.))
            .pr(px(10.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if search_focused {
                        p.accent
                    } else {
                        gpui::rgba(0x00000000)
                    })
                    .bg(if search_focused { p.surface } else { p.control })
                    .id("search-box")
                    .track_focus(&search_focus)
                    .child(crate::icons::icon("search", 13.).text_color(p.subtle))
                    .child(div().flex_1().min_w_0().child({
                        let mut input = Input::new(&self.search)
                            .appearance(false)
                            .p_0()
                            .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.));
                        input.style().size.height = Some(px(28.).into());
                        input
                    }))
                    .when(!self.session.search.is_empty(), |d| {
                        d.child(
                            div()
                                .id("clear-search")
                                .size(px(18.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .hover(|d| d.bg(p.soft))
                                .tooltip(|window, cx| {
                                    gpui_component::tooltip::Tooltip::new(choose(
                                        "清空搜索",
                                        "Clear search",
                                    ))
                                    .build(window, cx)
                                })
                                .child(crate::icons::icon("x", 13.).text_color(p.subtle))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.search.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                        input.focus(window, cx);
                                    });
                                })),
                        )
                    }),
            )
            .child(filter_control);
        let pinned = self.snapshot.settings.always_on_top;
        let toolbar = toolbar.child(
            div()
                .id("pin-panel")
                .group("pin-panel")
                .size(px(28.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .when(pinned, |d| d.bg(p.soft))
                .hover(|d| d.bg(p.soft))
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(if pinned {
                        choose("取消置顶", "Unpin")
                    } else {
                        choose("置顶", "Pin")
                    })
                    .build(window, cx)
                })
                .child(
                    crate::icons::icon("pin", 15.)
                        .text_color(if pinned { p.accent } else { p.subtle })
                        .group_hover("pin-panel", |d| d.text_color(p.accent)),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    let result = crate::window_preferences::save_patch(
                        &this.store,
                        None,
                        |value| value.always_on_top = !value.always_on_top,
                        window,
                        cx,
                    )
                    .and_then(|(outcome, _)| match outcome {
                        tp_core::preferences::SaveOutcome::Saved { .. } => Ok(()),
                        // A topmost-only patch never changes retention or capacity.
                        tp_core::preferences::SaveOutcome::ConfirmationRequired { .. } => {
                            Err(choose("设置未保存", "Settings were not saved").into())
                        }
                    });
                    this.refresh(result, cx);
                })),
        );
        let toolbar = toolbar.child(
            div()
                .id("open-settings")
                .size(px(28.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .cursor_pointer()
                .when(self.settings_view.is_some(), |d| d.bg(p.soft))
                .hover(|d| d.bg(p.soft))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new(choose("设置", "Settings"))
                        .build(window, cx)
                })
                .child(crate::icons::icon("gear", 15.).text_color(
                    if self.settings_view.is_some() {
                        p.accent
                    } else {
                        p.subtle
                    },
                ))
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(settings) = this.settings_view.clone() {
                        settings.update(cx, |settings, cx| settings.close(cx));
                        return;
                    }
                    this.preview.hide();
                    this.close_floating(cx);
                    this.move_menu = None;
                    this.record_menu = None;
                    this.group_menu = None;
                    this.record_edit = None;
                    this.group_edit = None;
                    let store = this.store.clone();
                    let snapshot = match store.snapshot() {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            this.show_toast(error, cx);
                            cx.notify();
                            return;
                        }
                    };
                    let available_update = this.available_update.clone();
                    let view = cx.new(|cx| {
                        crate::settings_view::SettingsView::new(
                            store,
                            snapshot,
                            available_update,
                            window,
                            cx,
                        )
                    });
                    this.settings_subscription = Some(cx.subscribe_in(
                        &view,
                        window,
                        |this, _, event, window, cx| match event {
                            #[cfg(debug_assertions)]
                            crate::settings_view::Event::ReleasePanel => {
                                this.hide(window, cx);
                                this.idle.expire_delay(std::time::Instant::now());
                            }
                            crate::settings_view::Event::Toast(message) => {
                                this.show_toast(message.clone(), cx)
                            }
                            crate::settings_view::Event::Changed => this.refresh(Ok(()), cx),
                            crate::settings_view::Event::UpdateAvailable(release) => {
                                this.available_update = Some(release.clone());
                            }
                            crate::settings_view::Event::Restored => {
                                this.clear_thumbnails(window);
                                this.refresh(Ok(()), cx);
                            }
                            crate::settings_view::Event::Reposition => {
                                this.refresh(Ok(()), cx);
                                this.place_window(window, cx);
                                cx.notify();
                            }
                            crate::settings_view::Event::Close => {
                                this.settings_view = None;
                                this.settings_subscription = None;
                                this.refresh(Ok(()), cx);
                                this.panel_focus.focus(window);
                            }
                        },
                    ));
                    this.settings_view = Some(view);
                    cx.notify();
                })),
        );
        let records = self.session.filtered(&self.snapshot.records);
        let layout = tp_core::list_layout::ListLayout::new(records.iter().map(|r| {
            (
                r.kind.as_str(),
                !r.title.is_empty() || self.record_edit.as_ref().is_some_and(|e| e.id == r.id),
            )
        }));
        self.scroll.update(layout.clone());
        let range = self.scroll.range();
        // A removed action button must not leave the window with a detached
        // focus path. The original document receives keys once it is hidden.
        if self.move_trigger_focus.is_focused(window)
            && !self.focused_move_trigger.as_ref().is_some_and(|id| {
                self.hovered_record.as_ref() == Some(id)
                    && records[range.clone()].iter().any(|record| &record.id == id)
            })
        {
            self.focused_move_trigger = None;
            self.panel_focus.focus(window);
        }
        let mut list = div()
            .id("records")
            .track_focus(&self.list_focus)
            .track_scroll(&self.scroll.handle)
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                this.keyboard_move_pending = None;
                this.keyboard_preview_pending = None;
                this.scroll.cancel_return();
                cx.notify();
            }))
            .capture_any_mouse_down(cx.listener(|this, _, _, _| {
                this.keyboard_move_pending = None;
                this.keyboard_preview_pending = None;
                this.scroll.cancel_return();
            }))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .mr(px(7.))
            .pl(px(8.))
            .pr(px(3.))
            .pt(px(viewport::LIST_TOP_PADDING))
            .pb(px(viewport::LIST_BOTTOM_PADDING));
        if records.is_empty() {
            list = list.child(
                div()
                    .py(px(55.))
                    .px(px(16.))
                    .text_center()
                    .text_size(px(self.snapshot.settings.font_size as f32))
                    .text_color(p.subtle)
                    .child(choose("这里还没有记录", "No records yet")),
            );
        }
        let first_visible = self.scroll.top_item();
        let back_to_top = self.scroll.show_back_to_top();
        let rendered_range = range.clone();
        // Programmatic scrolling settles in prepaint. Re-render labels after
        // layout only if the first visible row changed.
        let scroll = self.scroll.clone();
        let panel = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let offset_changed = panel
                .update(cx, |this, cx| {
                    let changed = this.dismiss_scrolled_overlays(window, cx);
                    this.finish_keyboard_move(window, cx);
                    this.finish_keyboard_preview(window, cx);
                    changed
                })
                .unwrap_or(false);
            if scroll.top_item() != first_visible
                || scroll.range() != rendered_range
                || scroll.show_back_to_top() != back_to_top
                || offset_changed
            {
                let _ = panel.update(cx, |_, cx| cx.notify());
            }
        });
        list = list.child(div().h(px(layout.offsets[range.start])).flex_shrink_0());
        let selected_rows: std::collections::HashSet<usize> = records
            .iter()
            .enumerate()
            .filter(|(_, record)| self.selection.ids.contains(&record.id))
            .map(|(index, _)| index)
            .collect();
        let font_scale = self.snapshot.settings.font_size as f32 / 13.;
        let mounted: Vec<_> = records
            .into_iter()
            .enumerate()
            .skip(range.start)
            .take(range.len())
            .map(|(i, r)| (i, r.clone()))
            .collect();
        self.thumbnails.retain(|id, thumbnail| {
            let keep = mounted
                .iter()
                .any(|(_, record)| &record.id == id && thumbnails::has_preview(record));
            if !keep {
                thumbnail.release_image(window);
            }
            keep
        });
        for (index, record) in mounted {
            let id = record.id.clone();
            let context_id = id.clone();
            let hover_id = id.clone();
            let editing = self.record_edit.as_ref().is_some_and(|edit| edit.id == id);
            if editing && !matches!(record.kind.as_str(), "image" | "video") {
                list = list.child(
                    div()
                        .id(gpui::SharedString::from(id.clone()))
                        .h(px(48.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .child(self.record_input(&id, window, cx).unwrap()),
                );
                continue;
            }
            let selected = self.selection.ids.contains(&id);
            let media = matches!(record.kind.as_str(), "image" | "video");
            let body = tp_core::browse::record_list_text(
                &record,
                &self.session.search,
                self.snapshot.settings.remove_trailing_newlines,
            );
            let content = if matches!(record.kind.as_str(), "image" | "video") {
                self.media_content(&record, window, cx)
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap(px(if record.title.is_empty() { 6. } else { 8. }))
                    .text_size(px(12.5 * font_scale))
                    .line_height(px(17. * font_scale))
                    .when(record.title.is_empty(), |d| {
                        d.when_some(
                            record_display::marker(&record, selected, font_scale, p),
                            |d, marker| d.child(marker),
                        )
                    })
                    .when(!record.title.is_empty(), |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .flex_shrink_0()
                                .max_w_full()
                                .min_w_0()
                                .overflow_hidden()
                                .when_some(
                                    record_display::marker(&record, selected, font_scale, p),
                                    |d, marker| d.child(marker),
                                )
                                .child(
                                    div()
                                        .id(gpui::SharedString::from(format!(
                                            "record-title:{}",
                                            record.id
                                        )))
                                        .tooltip(record_display::full_text_tooltip(
                                            record.title.clone(),
                                        ))
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(if selected { p.selected } else { p.text })
                                        .child(crate::highlight::text(
                                            record.title.clone(),
                                            &self.session.search,
                                            p,
                                        )),
                                ),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_color(if !record.title.is_empty() {
                                p.muted
                            } else if selected {
                                p.selected
                            } else {
                                p.text
                            })
                            .child(if self.session.search.is_empty() {
                                crate::highlight::text(body, "", p).into_any_element()
                            } else {
                                crate::highlight::excerpt(
                                    body,
                                    self.session.search.clone(),
                                    p,
                                    12.5 * font_scale,
                                    17. * font_scale,
                                )
                                .into_any_element()
                            }),
                    )
                    .when(!thumbnails::record_note(&record).is_empty(), |d| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .max_w(gpui::relative(0.45))
                                .truncate()
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_size(px(10. * font_scale))
                                .text_color(p.subtle)
                                .child(thumbnails::record_note(&record)),
                        )
                    })
                    .into_any_element()
            };
            list = list.child(
                div()
                    .id(gpui::SharedString::from(id.clone()))
                    .on_hover(cx.listener(move |this, hovered, window, cx| {
                        // Revealing the keyboard target can move a different row
                        // under a stationary pointer before its post-layout callback.
                        if this.keyboard_preview_pending.is_some() {
                            return;
                        }
                        if *hovered {
                            this.hovered_record = Some(hover_id.clone());
                            this.request_floating(hover_id.clone(), false, window, cx);
                        } else if this.hovered_record.as_deref() == Some(hover_id.as_str()) {
                            this.hovered_record = None;
                            this.leave_floating(window, cx);
                        }
                        cx.notify();
                    }))
                    .h(px(layout.offsets[index + 1] - layout.offsets[index]))
                    .flex()
                    .items_center()
                    .relative()
                    .pl(px(4.))
                    .pr(px(6.))
                    .rounded(px(8.))
                    .when(selected, |d| d.bg(p.soft).text_color(p.selected))
                    .hover(|d| d.bg(if selected { p.soft } else { p.hover }))
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.open_record_menu(context_id.clone(), event.position, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if editing {
                                return;
                            }
                            this.selection.select(
                                &this.visible_ids(),
                                &id,
                                event.modifiers().shift,
                            );
                            this.list_focus.focus(window);
                            if event.click_count() == 2 {
                                this.apply(id.clone(), window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .w(px(20.))
                            .flex_shrink_0()
                            .text_center()
                            .text_size(px(10. * font_scale))
                            .text_color(p.subtle)
                            .when(media, |d| d.absolute().left(px(4.)).top(px(12.)))
                            .child(
                                if !editing && index >= first_visible && index < first_visible + 9 {
                                    (index - first_visible + 1).to_string()
                                } else {
                                    String::new()
                                },
                            ),
                    )
                    .child(content)
                    .when(
                        !selected
                            && index + 1 < layout.count()
                            && !selected_rows.contains(&(index + 1)),
                        |row| {
                            row.child(
                                div()
                                    .absolute()
                                    .left(px(28.))
                                    .right(px(6.))
                                    .bottom_0()
                                    .h(px(1.))
                                    .bg(p.separator),
                            )
                        },
                    )
                    .when(!editing, |row| {
                        row.child(self.record_actions(&record, media, p, cx))
                    }),
            );
        }
        list = list.child(
            div()
                .h(px(layout.total() - layout.offsets[range.end]))
                .flex_shrink_0(),
        );
        let mut shell = div()
            .track_focus(&self.panel_focus)
            .key_context("TP")
            .capture_key_down(cx.listener(Self::key_down))
            .capture_any_mouse_down(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                if event.button == gpui::MouseButton::Left && this.group_press.is_none() {
                    this.group_click_suppressed = false;
                }
                if event.button == gpui::MouseButton::Right && this.group_press.is_some() {
                    this.cancel_group_press(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                this.update_group_pointer(event.position, cx)
            }))
            .capture_any_mouse_up(cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                this.finish_group_press(event, cx)
            }))
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                    this.finish_group_press(event, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &NextGroup, window, cx| this.cycle_group(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousGroup, window, cx| this.cycle_group(-1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                if this.move_menu.is_none()
                    && this.group_edit.is_none()
                    && this.record_edit.is_none()
                    // The original settings keyboard handler returns before
                    // the panel-wide Ctrl+F branch, even without a modal.
                    && this.settings_view.is_none()
                {
                    this.group_menu = None;
                    this.record_menu = None;
                    this.search.update(cx, |input, cx| input.focus(window, cx));
                    cx.notify();
                }
            }))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.text)
            .text_size(px(self.snapshot.settings.font_size as f32))
            .child(self.titlebar(cx))
            .child(toolbar)
            .when(self.settings_view.is_none(), |d| {
                d.child(tabs).child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(list.h_full())
                        .child(
                            div().absolute().inset_0().mr(px(7.)).child(
                                gpui_component::scroll::Scrollbar::vertical(&self.scroll.handle)
                                    .id("record-scrollbar")
                                    .thin(px(7.))
                                    .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                            ),
                        ),
                )
            })
            .when_some(self.settings_view.clone(), |d, settings| {
                d.child(div().flex_1().min_h_0().child(settings))
            });
        if let Some(overlay) = self.group_drag_overlay(p) {
            shell = shell.child(overlay);
        }
        if self.filter_open
            && !self
                .settings_view
                .as_ref()
                .is_some_and(|view| view.read(cx).has_modal())
        {
            let kinds = [
                ("all", "全部", "All"),
                ("text", "文本", "Text"),
                ("image", "图片", "Images"),
                ("video", "视频", "Videos"),
                ("file", "文件", "Files"),
                ("link", "链接", "Links"),
                ("color", "颜色", "Colors"),
            ];
            // CSS min-width:128px; border/padding + item padding + icon + gap.
            let menu_width = kinds
                .iter()
                .map(|(_, zh, en)| {
                    crate::theme::label_width(window, &self.snapshot.settings, choose(zh, en), 12.)
                        + px(51.)
                })
                .fold(px(128.), |width, next| width.max(next));
            let mut menu = div()
                .id("filter-menu")
                .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    // The trigger toggles on click; closing it here would reopen
                    // the menu on mouse-up. Other clicks retain their normal action.
                    if !this.filter_trigger_bounds.get().contains(&event.position) {
                        this.filter_open = false;
                        cx.notify();
                    }
                }))
                .absolute()
                // 30px titlebar + 12px toolbar centering + 35px trigger offset.
                .top(px(77.))
                .right(px(86.))
                .w(menu_width)
                .p(px(5.))
                .border_1()
                .border_color(p.line)
                .rounded(px(9.))
                .bg(p.surface)
                .shadow(vec![Palette::popup_shadow()]);
            for (kind, zh, en) in kinds {
                menu = menu.child(
                    div()
                        .id(kind)
                        .h(px(30.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .gap(px(9.))
                        .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
                        .text_color(p.muted)
                        .hover(|d| d.bg(p.hover))
                        .child(
                            crate::icons::icon(if kind == "all" { "filter" } else { kind }, 14.)
                                .text_color(if self.session.kind_filter == kind {
                                    p.accent
                                } else {
                                    p.muted
                                }),
                        )
                        .child(choose(zh, en))
                        .when(self.session.kind_filter == kind, |d| {
                            d.bg(p.soft).text_color(p.accent)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.session.kind_filter = kind.into();
                            this.record_edit = None;
                            this.filter_open = false;
                            this.reconcile();
                            this.scroll.scroll_to_item(0);
                            cx.notify();
                        })),
                );
            }
            shell = shell.child(menu.occlude());
        }
        if let Some(menu) = self.render_move(window, cx) {
            shell = shell.child(menu);
        }
        if let Some(menu) = self.render_group_menu(window, cx) {
            shell = shell.child(menu);
        }
        if let Some(menu) = self.render_record_menu(window, cx) {
            shell = shell.child(menu);
        }
        shell = shell.child(self.resize_edges(window, cx));
        if let Some(toast) = &self.toast {
            let now = std::time::Instant::now();
            if toast.animating(now) {
                window.request_animation_frame();
            }
            let theme = self.snapshot.settings.theme.as_str();
            let background = if theme == "indigo" {
                gpui::rgb(0x273149)
            } else if ["forest-night", "graphite", "violet", "charcoal"].contains(&theme) {
                p.soft
            } else {
                p.selected
            };
            shell = shell.child(
                div()
                    .absolute()
                    .left(px(16.))
                    .right(px(16.))
                    .bottom(px(16.))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(8.))
                    .bg(background)
                    .text_color(gpui::rgb(0xFFFFFF))
                    .text_size(px(12. * self.snapshot.settings.font_size as f32 / 13.))
                    .opacity(toast.opacity(now))
                    .child(toast.message.clone()),
            );
        }
        crate::theme::typography(shell, &self.snapshot.settings).into_any_element()
    }
}
