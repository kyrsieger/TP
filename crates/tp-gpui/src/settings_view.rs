mod confirmation;
mod fonts;
mod language;
mod numbers;
mod position;
mod retention;
mod storage;
mod theme;
mod work_queue;
use crate::theme::Palette;
use gpui::{
    div, prelude::*, px, Context, Entity, EntityInputHandler, EventEmitter, Render, Subscription,
    Window,
};
use gpui_component::input::{Input, InputEvent, InputState};
use std::sync::Arc;
use tp_core::{i18n::choose, preferences::SaveOutcome, settings::Settings};
use tp_desktop::Store;

pub enum Event {
    #[cfg(debug_assertions)]
    ReleasePanel,
    Changed,
    Reposition,
    Close,
    Toast(String),
}
#[derive(Clone)]
enum Patch {
    BackgroundResident(bool),
    #[cfg(windows)]
    TaskbarIcon(bool),
    PositionMode(String),
    RememberPosition(bool, Option<(f64, f64)>),
    FixedPosition(String),
    Language(String),
    Theme(String),
    FontSize(u16),
    Font(String, tp_core::fonts::FontStyle),
    Capacity(u32),
    Retention(u16),
    Sensitive(u32),
    Trim(bool),
    RetainSearch(bool),
    RetainFilter(bool),
    Rename(String),
    Shortcut(String),
}
impl Patch {
    fn is_toggle(&self) -> bool {
        match self {
            Self::BackgroundResident(_) => true,
            #[cfg(windows)]
            Self::TaskbarIcon(_) => true,
            Self::RememberPosition(_, _)
            | Self::Trim(_)
            | Self::RetainSearch(_)
            | Self::RetainFilter(_) => true,
            _ => false,
        }
    }
    fn apply(&self, value: &mut Settings) {
        match self {
            Self::BackgroundResident(v) => value.background_resident = *v,
            #[cfg(windows)]
            Self::TaskbarIcon(v) => value.show_taskbar_icon = *v,
            Self::RememberPosition(enabled, position) => {
                value.remember_position = *enabled;
                if let Some((x, y)) = position {
                    value.panel_x = Some(*x);
                    value.panel_y = Some(*y);
                }
            }
            Self::PositionMode(v) => value.position_mode = v.clone(),
            Self::FixedPosition(v) => value.fixed_position = v.clone(),
            Self::Language(v) => value.language = v.clone(),
            Self::Theme(v) => value.theme = v.clone(),
            Self::FontSize(v) => value.font_size = *v,
            Self::Font(family, style) => {
                value.font_family = family.clone();
                value.font_style = style.clone();
            }
            Self::Capacity(v) => value.max_ordinary = *v,
            Self::Retention(v) => value.retention_days = *v,
            Self::Sensitive(v) => value.sensitive_seconds = *v,
            Self::Trim(v) => value.remove_trailing_newlines = *v,
            Self::RetainSearch(v) => value.retain_search_input = *v,
            Self::RetainFilter(v) => value.retain_type_filter = *v,
            Self::Rename(v) => value.rename_shortcut = v.clone(),
            Self::Shortcut(v) => value.shortcut = v.clone(),
        }
    }
}

pub struct SettingsView {
    theme_open: bool,
    theme_scroll: gpui::ScrollHandle,
    content_scroll: gpui::ScrollHandle,
    queued_theme: work_queue::ThemeQueue,
    language_open: bool,
    retention_open: bool,
    fixed_position_open: bool,
    toggle_focus: std::cell::RefCell<std::collections::HashMap<String, gpui::FocusHandle>>,
    cleanup: Option<Entity<crate::cleanup_view::CleanupView>>,
    cleanup_subscription: Option<Subscription>,
    cache_size: String,
    cache_request: u64,
    transfer: Option<Entity<crate::transfer_view::TransferView>>,
    transfer_subscription: Option<Subscription>,
    retained_for_work: bool,
    store: Arc<Store>,
    value: Settings,
    ordinary_count: usize,
    autostart: Option<bool>,
    autostart_loading: bool,
    font_bounds: [std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>; 2],
    font_focus: gpui::FocusHandle,
    font_style_focus: gpui::FocusHandle,
    font_style_scroll: gpui::ScrollHandle,
    font_styles: bool,
    font_catalog: Option<Arc<Vec<tp_core::fonts::FontFamily>>>,
    font_migrations: Vec<(String, tp_core::fonts::FontStyle)>,
    font_picker: Option<Entity<crate::font_picker::FontPicker>>,
    font_subscription: Option<Subscription>,
    numbers: Vec<Entity<InputState>>,
    sensitive_save: Option<gpui::Task<()>>,
    number_repeat: Option<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
    busy: bool,
    pending: Option<(Patch, usize)>,
    pending_focus: [gpui::FocusHandle; 2],
    shortcut_error: Option<String>,
    rename_error: Option<String>,
    shortcut_focus: gpui::FocusHandle,
    shortcut_draft: Option<String>,
    rename_focus: gpui::FocusHandle,
    rename_draft: Option<String>,
}
impl EventEmitter<Event> for SettingsView {}
impl SettingsView {
    pub fn new(
        store: Arc<Store>,
        snapshot: tp_core::Snapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let value = snapshot.settings;
        let mut numbers = Vec::new();
        let mut subscriptions = Vec::new();
        for (index, text) in [
            value.font_size.to_string(),
            value.max_ordinary.to_string(),
            value.sensitive_seconds.to_string(),
        ]
        .into_iter()
        .enumerate()
        {
            let input = cx.new(|cx| {
                let mut input = InputState::new(window, cx);
                input.set_value(text, window, cx);
                input
            });
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, _, event, window, cx| match event {
                    InputEvent::Change => {
                        if index == 2 {
                            this.schedule_sensitive_save(window, cx);
                        }
                        cx.notify();
                    }
                    InputEvent::Blur => {
                        this.number_repeat = None;
                        if index == 2 {
                            this.sensitive_save = None;
                        }
                        this.number(index, true, window, cx);
                    }
                    InputEvent::PressEnter { secondary: false } if index != 2 => {
                        this.number(index, true, window, cx);
                    }
                    _ => {}
                },
            ));
            numbers.push(input);
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { tp_desktop::autostart::enabled() })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.autostart_loading = false;
                match result {
                    Ok(value) => this.autostart = Some(value),
                    Err(error) => cx.emit(Event::Toast(error)),
                }
                cx.notify();
            });
        })
        .detach();
        let mut view = Self {
            theme_open: false,
            theme_scroll: gpui::ScrollHandle::new(),
            content_scroll: gpui::ScrollHandle::new(),
            queued_theme: Default::default(),
            language_open: false,
            retention_open: false,
            fixed_position_open: false,
            font_bounds: Default::default(),
            toggle_focus: Default::default(),
            cleanup: None,
            cleanup_subscription: None,
            cache_size: choose("计算中…", "Calculating…").into(),
            cache_request: 0,
            transfer: None,
            transfer_subscription: None,
            retained_for_work: false,
            store,
            value,
            autostart: None,
            autostart_loading: true,
            ordinary_count: snapshot
                .records
                .iter()
                .filter(|r| !r.favorite && r.group_id.is_none())
                .count(),
            font_focus: cx.focus_handle(),
            font_style_focus: cx.focus_handle(),
            font_style_scroll: gpui::ScrollHandle::new(),
            font_styles: false,
            font_catalog: None,
            font_migrations: Vec::new(),
            font_picker: None,
            font_subscription: None,
            numbers,
            sensitive_save: None,
            number_repeat: None,
            _subscriptions: subscriptions,
            busy: false,
            pending: None,
            pending_focus: std::array::from_fn(|_| cx.focus_handle()),
            shortcut_error: None,
            rename_error: None,
            shortcut_focus: cx.focus_handle(),
            shortcut_draft: None,
            rename_focus: cx.focus_handle(),
            rename_draft: None,
        };
        view.watch_language_focus(window, cx);
        view.load_font_catalog(window, cx);
        view.refresh_cache_size(window, cx);
        view
    }
    fn font_trigger_key(
        &mut self,
        styles_only: bool,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.is_held
            && matches!(
                event.keystroke.key.as_str(),
                "enter" | "space" | "up" | "down"
            )
        {
            cx.stop_propagation();
            window.prevent_default();
            let arrows = matches!(event.keystroke.key.as_str(), "up" | "down");
            if !(arrows && self.font_picker.is_some() && self.font_styles == styles_only) {
                self.open_fonts(styles_only, window, cx);
            }
            if styles_only && matches!(event.keystroke.key.as_str(), "up" | "down") {
                if let Some(picker) = &self.font_picker {
                    picker.update(cx, |picker, cx| {
                        picker.reveal_selected_after_load(window, cx)
                    });
                }
            }
        }
    }
    fn open_fonts(&mut self, styles_only: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        if styles_only && !self.has_font_styles() {
            return;
        }
        self.theme_open = false;
        self.language_open = false;
        self.retention_open = false;
        if self.font_picker.take().is_some() {
            self.font_subscription = None;
            if self.font_styles == styles_only {
                cx.notify();
                return;
            }
        }
        self.font_styles = styles_only;
        if styles_only {
            self.font_style_focus.focus(window);
        }
        let picker = cx.new(|cx| {
            crate::font_picker::FontPicker::new(
                self.value.theme.clone(),
                self.value.font_size,
                self.value.font_family.clone(),
                self.value.font_style.clone(),
                styles_only,
                self.font_catalog.clone().map(|fonts| {
                    (
                        fonts,
                        if styles_only {
                            self.font_style_scroll.clone()
                        } else {
                            gpui::ScrollHandle::new()
                        },
                    )
                }),
                self.font_bounds[usize::from(styles_only)].clone(),
                window,
                cx,
            )
        });
        self.font_subscription =
            Some(
                cx.subscribe_in(&picker, window, |this, _, event, window, cx| {
                    if let crate::font_picker::Event::Catalog(fonts) = event {
                        this.font_catalog = Some(fonts.clone());
                        this.reconcile_font(window, cx);
                        cx.notify();
                        return;
                    }
                    this.font_picker = None;
                    this.font_subscription = None;
                    if let crate::font_picker::Event::TabFromSearch { reverse } = event {
                        // The search sits after the family trigger in the original
                        // DOM. Tab closes it without applying the highlighted font.
                        // Use the adjacent control explicitly: deferred popup elements
                        // do not share the original DOM's sequential focus order.
                        this.close_font_search_for_tab(*reverse, window, cx);
                        return;
                    }
                    if this.font_styles {
                        this.font_style_focus.focus(window);
                    } else {
                        this.font_focus.focus(window);
                    }
                    if let crate::font_picker::Event::Select(family, style) = event {
                        this.save(Patch::Font(family.clone(), style.clone()), None, window, cx);
                    }
                    cx.notify();
                }),
            );
        self.font_picker = Some(picker);
        cx.notify();
    }
    fn set_autostart(&mut self, cx: &mut Context<Self>, window: &mut Window) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(current) = self.autostart else {
            return;
        };
        self.busy = true;
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    tp_desktop::autostart::set(&store, !current)?;
                    tp_desktop::autostart::enabled()
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(value) => this.autostart = Some(value),
                    Err(error) => cx.emit(Event::Toast(error)),
                }
                this.finish_settings_work(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn capture_shortcut(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.shortcut_focus.is_focused(window) || event.keystroke.key == "tab" {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        if self.busy || self.pending.is_some() || event.is_held {
            return;
        }
        let candidate = match event.keystroke.key.as_str() {
            "backspace" | "delete" => Some(String::new()),
            _ => crate::shortcuts::panel_key(&event.keystroke),
        };
        let Some(candidate) = candidate else {
            return;
        };
        self.shortcut_draft = Some(candidate.clone());
        self.shortcut_error = None;
        if !candidate.is_empty() && candidate == self.value.rename_shortcut {
            self.shortcut_error = Some(
                choose(
                    "此快捷键已用于记录重命名，请更换组合键。",
                    "This shortcut is used to rename records. Choose another combination.",
                )
                .into(),
            );
            cx.notify();
            return;
        }
        let mut next = self.value.clone();
        next.shortcut = candidate.clone();
        if let Err(error) = next.validate() {
            self.shortcut_error = Some(tp_core::i18n::tr(error).into());
            cx.notify();
            return;
        }
        self.save(Patch::Shortcut(candidate), None, window, cx);
    }
    fn capture_rename(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.rename_focus.is_focused(window) {
            return;
        }
        if event.keystroke.key == "tab" {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        if self.busy || self.pending.is_some() || event.is_held {
            return;
        }
        let candidate = match event.keystroke.key.as_str() {
            "backspace" | "delete" => Some(String::new()),
            _ => crate::shortcuts::rename_key(&event.keystroke),
        };
        let Some(candidate) = candidate else {
            return;
        };
        self.rename_draft = Some(candidate.clone());
        self.rename_error = None;
        if crate::shortcuts::rename_conflicts(&candidate, &self.value.shortcut) {
            self.rename_error = Some(
                choose(
                    "此快捷键已用于其他操作，请更换按键。",
                    "This shortcut is assigned to another action. Choose another key.",
                )
                .into(),
            );
            cx.notify();
            return;
        }
        let mut next = self.value.clone();
        next.rename_shortcut = candidate.clone();
        if let Err(error) = next.validate() {
            self.rename_error = Some(tp_core::i18n::tr(error).into());
            cx.notify();
            return;
        }
        self.save(Patch::Rename(candidate), None, window, cx);
    }
    fn schedule_sensitive_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sensitive_save = None;
        if self.busy
            || self.pending.is_some()
            || self.numbers[2].read(cx).value().as_str() == self.value.sensitive_seconds.to_string()
        {
            return;
        }
        self.sensitive_save = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(300))
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.number(2, false, window, cx);
            });
        }));
    }
    fn number(
        &mut self,
        index: usize,
        restore_invalid: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.pending.is_some() {
            return;
        }
        if self.numbers[index].update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        let input = self.numbers[index].read(cx).value().to_string();
        let current = [
            self.value.font_size.to_string(),
            self.value.max_ordinary.to_string(),
            self.value.sensitive_seconds.to_string(),
        ][index]
            .clone();
        if input == current {
            return;
        }
        let patch = numbers::setting_integer(&input, index).map(|number| match index {
            0 => Patch::FontSize(number as u16),
            1 => Patch::Capacity(number),
            _ => Patch::Sensitive(number),
        });
        if let Some(patch) = patch {
            self.save(patch, None, window, cx);
        } else if restore_invalid {
            cx.emit(Event::Toast(
                choose("请输入有效的设置值", "Enter a valid setting value").into(),
            ));
            self.reset_numbers(window, cx);
            cx.notify();
        }
    }
    fn reset_numbers(&self, window: &mut Window, cx: &mut Context<Self>) {
        for (input, value) in self.numbers.iter().zip([
            self.value.font_size.to_string(),
            self.value.max_ordinary.to_string(),
            self.value.sensitive_seconds.to_string(),
        ]) {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }
    fn save(
        &mut self,
        mut patch: Patch,
        confirmed: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if matches!(&patch, Patch::FixedPosition(position) if *position == self.value.fixed_position)
        {
            return;
        }
        if let Patch::RememberPosition(true, position) = &mut patch {
            match crate::native_window::geometry(window) {
                Ok(geometry) => *position = geometry.panel_x.zip(geometry.panel_y),
                Err(error) => {
                    cx.emit(Event::Toast(error));
                    cx.notify();
                    return;
                }
            }
        }
        if matches!(&patch, Patch::PositionMode(mode) if mode != "fixed") {
            self.fixed_position_open = false;
        }
        self.busy = true;
        self.pending = None;
        let store = self.store.clone();
        let work = patch.clone();
        let revision = cx.global::<crate::host::Host>().background.revision.clone();
        let native_effect = matches!(patch, Patch::Shortcut(_));
        #[cfg(windows)]
        let native_effect = native_effect || matches!(patch, Patch::TaskbarIcon(_));
        if native_effect {
            let result = crate::window_preferences::save_patch(
                &store,
                confirmed,
                |value| work.apply(value),
                window,
                cx,
            );
            self.finish_save(result, patch, window, cx);
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let outcome = tp_core::preferences::save_patch(
                        &store,
                        confirmed,
                        |value| work.apply(value),
                        |_, _| Ok(()),
                    )?;
                    if matches!(outcome, SaveOutcome::Saved { .. }) {
                        revision.fetch_add(1, std::sync::atomic::Ordering::Release);
                    }
                    Ok::<_, String>((outcome, store.snapshot()?))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_save(result, patch, window, cx);
            });
        })
        .detach();
        cx.notify();
    }
    fn finish_save(
        &mut self,
        result: Result<(SaveOutcome, tp_core::Snapshot), String>,
        patch: Patch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        match result {
            Ok((SaveOutcome::ConfirmationRequired { count }, _)) => {
                // Hiding cancels unconfirmed destructive settings changes, as
                // SettingsController.reset does in the original interface.
                if !self.retained_for_work {
                    self.pending = Some((patch, count));
                    self.pending_focus[0].focus(window);
                }
                self.language_open = false;
                self.theme_open = false;
                self.retention_open = false;
                self.fixed_position_open = false;
                self.font_picker = None;
                self.font_subscription = None;
            }
            Ok((SaveOutcome::Saved { .. }, snapshot)) => {
                self.value = snapshot.settings;
                if matches!(patch, Patch::Rename(_)) {
                    self.rename_draft = None;
                    self.rename_error = None;
                }
                if matches!(patch, Patch::Shortcut(_)) {
                    self.shortcut_draft = None;
                    self.shortcut_error = None;
                }
                self.ordinary_count = snapshot
                    .records
                    .iter()
                    .filter(|r| !r.favorite && r.group_id.is_none())
                    .count();
                tp_core::i18n::set_preference(&self.value.language);
                Palette::apply(&self.value, cx);
                self.reset_numbers(window, cx);
                self.refresh_cache_size(window, cx);
                if matches!(patch, Patch::FixedPosition(_)) {
                    cx.emit(Event::Reposition);
                } else {
                    cx.emit(Event::Changed);
                }
            }
            Err(error) => {
                let message = match &patch {
                    Patch::Rename(_) => {
                        let message = format!(
                            "{}{error}",
                            choose("无法保存重命名快捷键：", "Could not save rename shortcut: ")
                        );
                        self.rename_error = Some(message.clone());
                        message
                    }
                    Patch::Shortcut(shortcut) => {
                        let display = crate::shortcuts::display(shortcut);
                        let message = if shortcut.is_empty() {
                            format!(
                                "{}{error}",
                                choose("无法清除快捷键：", "Could not clear shortcut: ")
                            )
                        } else if error.to_lowercase().contains("already registered") {
                            format!(
                                "{display}{}",
                                choose(
                                    " 已被占用，请更换组合键或解除占用。",
                                    " is already in use. Choose another combination or release it."
                                )
                            )
                        } else {
                            format!(
                                "{}{display}{}{error}",
                                choose("无法启用 ", "Could not enable "),
                                choose("：", ": ")
                            )
                        };
                        self.shortcut_error = Some(message.clone());
                        message
                    }
                    _ => error,
                };
                cx.emit(Event::Toast(message));
                self.reset_numbers(window, cx);
            }
        }
        self.finish_settings_work(window, cx);
        cx.notify();
    }
    fn finish_settings_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self
            .queued_theme
            .take_ready(self.busy, self.pending.is_some())
        {
            self.choose_theme(&id, window, cx);
        }
        self.reconcile_font(window, cx);
        if self.retained_for_work && !self.busy {
            cx.emit(Event::Close);
        }
    }
    fn refresh_cache_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cache_request += 1;
        let request = self.cache_request;
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let _data = store.read_data()?;
                    tp_core::cleanup::cache_size_data(&store)
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.cache_request != request {
                    return;
                }
                this.cache_size = match result {
                    Ok(bytes) => {
                        let units = ["B", "KB", "MB", "GB", "TB"];
                        let mut size = bytes as f64;
                        let mut unit = 0;
                        while size >= 1024. && unit < units.len() - 1 {
                            size /= 1024.;
                            unit += 1;
                        }
                        if unit == 0 {
                            format!("{bytes} B")
                        } else {
                            format!("{size:.1} {}", units[unit])
                        }
                    }
                    Err(_) => choose("无法获取", "Unavailable").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }
    fn open_cleanup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.transfer.is_some() || self.cleanup.is_some()
        {
            return;
        }
        self.sensitive_save = None;
        let view = cx.new(|cx| {
            crate::cleanup_view::CleanupView::new(
                self.store.clone(),
                self.value.clone(),
                window,
                cx,
            )
        });
        self.cleanup_subscription =
            Some(
                cx.subscribe_in(&view, window, |this, _, event, window, cx| {
                    if let crate::cleanup_view::Event::Finished { all, result } = event {
                        match this.store.snapshot() {
                            Ok(snapshot) => {
                                this.ordinary_count = snapshot
                                    .records
                                    .iter()
                                    .filter(|r| !r.favorite && r.group_id.is_none())
                                    .count();
                                this.value = snapshot.settings;
                                this.reset_numbers(window, cx);
                            }
                            Err(error) => cx.emit(Event::Toast(error)),
                        }
                        match result {
                            Ok(count) => cx.emit(Event::Toast(format!(
                                "{} {count} {}",
                                choose("已清空", "Cleared"),
                                choose(
                                    if *all {
                                        "条记录及孤立附件"
                                    } else {
                                        "条普通记录及孤立附件"
                                    },
                                    if *all {
                                        "records and orphaned attachments"
                                    } else {
                                        "ordinary records and orphaned attachments"
                                    }
                                )
                            ))),
                            Err(error) => cx.emit(Event::Toast(format!(
                                "{}{error}",
                                choose("清空数据失败：", "Failed to clear data: ")
                            ))),
                        }
                        this.refresh_cache_size(window, cx);
                        cx.emit(Event::Changed);
                    }
                    this.cleanup = None;
                    this.cleanup_subscription = None;
                    if this.retained_for_work {
                        cx.emit(Event::Close);
                        return;
                    }
                    if let Some(focus) = this.toggle_focus.borrow().get("clear-cache") {
                        focus.focus(window);
                    }
                    cx.notify();
                }),
            );
        self.cleanup = Some(view);
        cx.notify();
    }
    fn open_transfer(&mut self, import: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.transfer.is_some() || self.cleanup.is_some()
        {
            return;
        }
        self.sensitive_save = None;
        let transfer = cx.new(|cx| {
            crate::transfer_view::TransferView::new(
                self.store.clone(),
                self.value.clone(),
                import,
                window,
                cx,
            )
        });
        self.transfer_subscription =
            Some(
                cx.subscribe_in(&transfer, window, move |this, _, event, window, cx| {
                    if let crate::transfer_view::Event::Failed(error) = event {
                        cx.emit(Event::Toast(error.clone()));
                    }
                    if matches!(event, crate::transfer_view::Event::Restored) {
                        match this.store.snapshot() {
                            Ok(snapshot) => {
                                this.ordinary_count = snapshot
                                    .records
                                    .iter()
                                    .filter(|r| !r.favorite && r.group_id.is_none())
                                    .count();
                                this.value = snapshot.settings;
                                this.reset_numbers(window, cx);
                            }
                            Err(error) => cx.emit(Event::Toast(format!(
                                "{}{error}",
                                choose(
                                    "记录已导入，刷新界面失败：",
                                    "Records imported, but refresh failed: "
                                )
                            ))),
                        }
                        this.refresh_cache_size(window, cx);
                        cx.emit(Event::Changed);
                    }
                    this.transfer = None;
                    this.transfer_subscription = None;
                    if this.retained_for_work {
                        cx.emit(Event::Close);
                        return;
                    }
                    let trigger = if import { "import-data" } else { "export-data" };
                    if let Some(focus) = this.toggle_focus.borrow().get(trigger) {
                        focus.focus(window);
                    }
                    cx.notify();
                }),
            );
        self.transfer = Some(transfer);
        cx.notify();
    }
    pub(crate) fn has_modal(&self) -> bool {
        self.pending.is_some() || self.cleanup.is_some() || self.transfer.is_some()
    }
    pub(crate) fn retain_on_hide(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let retain = self.busy
            || self.queued_theme.is_pending()
            || self
                .transfer
                .as_ref()
                .is_some_and(|view| view.read(cx).retain_on_hide())
            || self
                .cleanup
                .as_ref()
                .is_some_and(|view| view.read(cx).is_busy());
        self.retained_for_work = retain;
        self.number_repeat = None;
        self.sensitive_save = None;
        self.pending = None;
        self.theme_open = false;
        self.language_open = false;
        self.retention_open = false;
        self.fixed_position_open = false;
        self.font_styles = false;
        self.font_picker = None;
        self.font_subscription = None;
        if !self.busy && self.queued_theme.is_pending() {
            self.finish_settings_work(window, cx);
        }
        retain
    }

    pub(crate) fn focus_work(&self, window: &mut Window, cx: &Context<Self>) {
        if let Some(transfer) = &self.transfer {
            transfer.read(cx).focus(window);
        } else if let Some(cleanup) = &self.cleanup {
            cleanup.read(cx).focus(window);
        } else {
            self.font_focus.focus(window);
        }
    }
    pub(crate) fn sync_record_count(
        &mut self,
        snapshot: &tp_core::Snapshot,
        cx: &mut Context<Self>,
    ) {
        let count = snapshot
            .records
            .iter()
            .filter(|record| !record.favorite && record.group_id.is_none())
            .count();
        if self.ordinary_count != count {
            self.ordinary_count = count;
            cx.notify();
        }
    }
    pub(crate) fn sync_always_on_top(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.value.always_on_top != enabled {
            self.value.always_on_top = enabled;
            cx.notify();
        }
    }
    pub(crate) fn panel_escape_allowed(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.cleanup.is_some()
            || self.transfer.is_some()
            || self.shortcut_focus.is_focused(window)
            || self.rename_focus.is_focused(window)
            || self.font_picker.as_ref().is_some_and(|picker| {
                picker.update(cx, |picker, cx| picker.is_composing(window, cx))
            })
        {
            return false;
        }
        !self.numbers.iter().any(|input| {
            input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        })
    }
    pub(crate) fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_open {
            self.close_theme(window, cx);
        } else if self.language_open {
            self.language_open = false;
            if let Some(focus) = self.toggle_focus.borrow().get("language-trigger") {
                focus.focus(window);
            }
            cx.notify();
        } else if self.fixed_position_open {
            self.fixed_position_open = false;
            if let Some(focus) = self.toggle_focus.borrow().get("fixed-position-trigger") {
                focus.focus(window);
            }
            cx.notify();
        } else if self.retention_open {
            self.retention_open = false;
            cx.notify();
        } else if self.font_picker.take().is_some() {
            self.font_subscription = None;
            if self.font_styles {
                self.font_style_focus.focus(window);
            } else {
                self.font_focus.focus(window);
            }
            cx.notify();
        } else if self.pending.is_some() {
            self.resolve_pending(false, window, cx);
        } else {
            self.close(cx);
        }
    }
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        if !self.busy && self.transfer.is_none() && self.cleanup.is_none() {
            self.number_repeat = None;
            self.sensitive_save = None;
            self.pending = None;
            cx.emit(Event::Close);
        }
    }
    fn toggle_control(
        &self,
        id: String,
        checked: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry(id.clone())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        // The original fixed-position fieldset overrides disabled opacity for
        // all descendants, including the remember-position checkbox.
        let preserve_opacity = id == "remember-position";
        toggle_visual(id, checked, disabled, Palette::for_id(&self.value.theme))
            .when(preserve_opacity, |d| d.opacity(1.))
            .track_focus(&focus.clone().tab_index(0).tab_stop(!disabled))
            .on_click(move |_, window, _| {
                if !disabled {
                    focus.focus(window);
                }
            })
    }
    fn button(
        &self,
        id: String,
        label: String,
        selected: bool,
        cx: &mut Context<Self>,
        patch: Patch,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let disabled = self.busy
            || self.pending.is_some()
            || (self.value.position_mode != "fixed"
                && matches!(
                    patch,
                    Patch::FixedPosition(_) | Patch::RememberPosition(_, _)
                ));
        if patch.is_toggle() {
            return self
                .toggle_control(id, selected, disabled, cx)
                .on_click(cx.listener(move |this, event, window, cx| {
                    if toggle_click_allowed(event)
                        && !disabled
                        && !this.busy
                        && this.pending.is_none()
                    {
                        this.save(patch.clone(), None, window, cx);
                    }
                }))
                .into_any_element();
        }
        div()
            .id(gpui::SharedString::from(id))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(6.))
            .bg(if selected { p.soft } else { p.control })
            .text_color(if selected { p.accent } else { p.text })
            .when(!disabled, |d| d.cursor_pointer())
            .when(disabled, |d| d.opacity(0.45))
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                if !disabled && !this.busy && this.pending.is_none() {
                    this.save(patch.clone(), None, window, cx);
                }
            }))
            .into_any_element()
    }
    fn position_radio(
        &self,
        mode: &'static str,
        label: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.value.position_mode == mode;
        let disabled = self.busy
            || self.pending.is_some()
            || self.transfer.is_some()
            || self.cleanup.is_some();
        let p = Palette::for_id(&self.value.theme);
        let focus = self
            .toggle_focus
            .borrow_mut()
            .entry(format!("position-{mode}"))
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let narrow = window.viewport_size().width < px(360.);
        let scale = self.value.font_size as f32 / 13.;
        div()
            .id(gpui::SharedString::from(format!("position-{mode}")))
            .track_focus(&focus.clone().tab_index(0).tab_stop(selected && !disabled))
            .relative()
            .flex()
            .items_center()
            .gap(px(if narrow { 9. } else { 14. }))
            .min_h(px(44.))
            .px(px(if narrow { 10. } else { 14. }))
            .py(px(10.))
            .text_size(px(13. * scale))
            .line_height(px(19.5 * scale))
            .when(disabled, |d| d.opacity(0.55))
            .when(!disabled, |d| d.cursor_pointer())
            .when(mode == "cursor", |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(14.))
                        .right_0()
                        .h(px(1.))
                        .bg(p.line),
                )
            })
            .child(
                div()
                    .size(px(16.))
                    .flex_shrink_0()
                    .rounded_full()
                    .border_1()
                    .border_color(if selected { p.accent } else { p.control_border })
                    .bg(if selected { p.accent } else { p.control })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(selected, |d| {
                        d.child(div().size(px(6.)).rounded_full().bg(p.button_text))
                    }),
            )
            .child(label)
            .on_click(cx.listener(move |this, event, window, cx| {
                if !disabled && toggle_click_allowed(event) {
                    focus.focus(window);
                    if this.value.position_mode != mode {
                        this.save(Patch::PositionMode(mode.into()), None, window, cx);
                    }
                }
            }))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if disabled || event.keystroke.modifiers != gpui::Modifiers::default() {
                        return;
                    }
                    if matches!(
                        event.keystroke.key.as_str(),
                        "up" | "down" | "left" | "right"
                    ) {
                        let next = if mode == "fixed" { "cursor" } else { "fixed" };
                        let next_focus = this
                            .toggle_focus
                            .borrow()
                            .get(&format!("position-{next}"))
                            .cloned();
                        if let Some(focus) = next_focus {
                            focus.focus(window);
                        }
                        this.save(Patch::PositionMode(next.into()), None, window, cx);
                        cx.stop_propagation();
                        window.prevent_default();
                    }
                }),
            )
    }
    fn card(&self) -> gpui::Div {
        let p = Palette::for_id(&self.value.theme);
        div()
            .bg(p.surface)
            .border_1()
            .border_color(p.line)
            .rounded(px(10.))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::rgba(0x00000008).into(),
                offset: gpui::point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            }])
    }
    fn shortcut_control(
        &self,
        rename: bool,
        disabled: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let scale = self.value.font_size as f32 / 13.;
        let (focus, value, error) = if rename {
            (
                &self.rename_focus,
                self.rename_draft
                    .as_deref()
                    .unwrap_or(&self.value.rename_shortcut),
                &self.rename_error,
            )
        } else {
            (
                &self.shortcut_focus,
                self.shortcut_draft
                    .as_deref()
                    .unwrap_or(&self.value.shortcut),
                &self.shortcut_error,
            )
        };
        let label = if value.is_empty() {
            choose(
                if rename {
                    "点击后按快捷键"
                } else {
                    "点击后按组合键"
                },
                "Press a shortcut",
            )
            .to_owned()
        } else {
            crate::shortcuts::display(value)
        };
        let text: gpui::SharedString = label.clone().into();
        let width = (window
            .text_system()
            .shape_line(
                text,
                px(12. * scale),
                &[window.text_style().to_run(label.len())],
                None,
            )
            .width
            + px(16.))
        .max(px(80.))
        .min(px(160.));
        div()
            .w(width)
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(5.))
            .child(
                div()
                    .id(if rename {
                        "rename-shortcut"
                    } else {
                        "panel-shortcut"
                    })
                    .key_context("TPShortcutCapture")
                    .track_focus(&focus.clone().tab_index(0).tab_stop(!disabled))
                    .h(px(28.))
                    .w_full()
                    .px(px(7.))
                    .rounded(px(6.))
                    .bg(p.control)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12. * scale))
                    .border_1()
                    .border_color(if focus.is_focused(window) {
                        p.accent
                    } else {
                        p.control_border
                    })
                    .cursor_pointer()
                    .child(div().min_w_0().overflow_hidden().child(label))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if rename {
                            this.rename_focus.focus(window);
                        } else {
                            this.shortcut_focus.focus(window);
                        }
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |this, event, window, cx| {
                        if rename {
                            this.capture_rename(event, window, cx);
                        } else {
                            this.capture_shortcut(event, window, cx);
                        }
                    })),
            )
            .when_some(error.clone(), |d, error| {
                d.child(
                    div()
                        .w_full()
                        .text_size(px(11. * scale))
                        .line_height(px(15.4 * scale))
                        .text_color(p.danger)
                        .child(error),
                )
            })
    }
    fn card_row(
        &self,
        window: &Window,
        label: &str,
        hint: Option<&str>,
        separator: bool,
        control: impl IntoElement,
    ) -> impl IntoElement {
        self.card_row_content(window, label.to_owned(), hint, separator, control)
    }
    fn card_row_content(
        &self,
        window: &Window,
        label: impl IntoElement,
        hint: Option<&str>,
        separator: bool,
        control: impl IntoElement,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        let narrow = window.viewport_size().width < px(360.);
        let scale = self.value.font_size as f32 / 13.;
        div()
            .relative()
            .flex()
            .items_center()
            .gap(px(if narrow { 9. } else { 14. }))
            .min_h(px(44.))
            .px(px(if narrow { 10. } else { 14. }))
            .py(px(10.))
            .text_size(px(13. * scale))
            .line_height(px(19.5 * scale))
            .text_color(p.text)
            .when(separator, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(14.))
                        .right_0()
                        .h(px(1.))
                        .bg(p.line),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(label)
                    .when_some(hint, |d, hint| {
                        d.child(
                            div()
                                .mt(px(3.))
                                .text_size(px(12. * scale))
                                .line_height(px(18.6 * scale))
                                .text_color(p.muted)
                                .child(hint.to_owned()),
                        )
                    }),
            )
            .child(control)
    }
}
impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.has_font_styles() {
            // FontStyleSetting unmounts when the selected family has no styles.
            self.font_style_scroll = gpui::ScrollHandle::new();
        }
        let p = Palette::for_id(&self.value.theme);
        let disabled = self.busy
            || self.pending.is_some()
            || self.transfer.is_some()
            || self.cleanup.is_some();
        let wide = window.viewport_size().width >= px(760.);
        let narrow = window.viewport_size().width < px(360.);
        // Match .settings-content: the centered box includes its own padding.
        let mut content = div()
            .w_full()
            .max_w(px(620.))
            .mx_auto()
            .px(px(if wide {
                24.
            } else if narrow {
                10.
            } else {
                16.
            }))
            .pt(px(if wide { 28. } else { 20. }))
            .pb(px(if wide { 52. } else { 36. }));
        #[cfg(debug_assertions)]
        {
            let release_disabled =
                disabled || self.value.background_resident || self.queued_theme.is_pending();
            let focus = self
                .toggle_focus
                .borrow_mut()
                .entry("release-panel".into())
                .or_insert_with(|| cx.focus_handle())
                .clone();
            let scale = self.value.font_size as f32 / 13.;
            content = content.child(self.card().p(px(14.)).mb(px(if wide { 20. } else { 16. }))
                .flex().flex_col().gap(px(10.))
                .child(div().text_size(px(13. * scale)).font_weight(gpui::FontWeight::MEDIUM)
                    .child(choose("开发测试", "Development tools")))
                .child(div().text_size(px(12. * scale)).line_height(px(19.2 * scale)).text_color(p.muted).child(choose(
                    "立即隐藏面板、卸载内容并挂起页面，跳过一分钟等待。历史记录和设置会保留，可用快捷键或托盘重新唤出。",
                    "Hide the panel, unload its contents, and suspend the page immediately, skipping the one-minute wait. History and settings are kept. Reopen with the shortcut or tray icon.")))
                .when(self.value.background_resident, |d| d.child(div().text_size(px(12. * scale)).line_height(px(19.2 * scale)).text_color(p.muted)
                    .child(choose("请先关闭“后台常驻”再测试。", "Turn off “Keep running in background” before testing."))))
                .child(div().flex().justify_end().child(div().id("release-panel")
                    .track_focus(&focus.tab_index(0).tab_stop(!release_disabled))
                    .px(px(12.)).py(px(6.)).text_size(px(13. * scale)).rounded(px(6.)).border_1().border_color(p.control_border)
                    .bg(p.control).text_color(p.text)
                    .when(!release_disabled, |d| d.cursor_pointer().hover(move |d| d.bg(p.hover)))
                    .when(release_disabled, |d| d.opacity(0.5))
                    .child(choose("立即释放面板", "Release panel now"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !release_disabled && !this.busy && !this.has_modal() {
                            this.sensitive_save = None;
                            this.number_repeat = None;
                            cx.emit(Event::ReleasePanel);
                        }
                    })))));
        }
        let languages = self.language_control(disabled, cx);
        let mut general = self.card().child(self.card_row(
            window,
            choose("语言", "Language"),
            None,
            false,
            languages,
        ));
        general = general.child(
            self.card_row(
                window,
                choose("开机启动", "Launch at startup"),
                None,
                true,
                self.toggle_control(
                    "autostart".into(),
                    self.autostart == Some(true),
                    disabled || self.autostart.is_none(),
                    cx,
                )
                .tooltip({
                    let status = match self.autostart {
                        Some(true) => choose("开启", "On"),
                        Some(false) => choose("关闭", "Off"),
                        None if self.autostart_loading => choose("读取中…", "Loading…"),
                        None => choose("无法获取", "Unavailable"),
                    };
                    move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(status).build(window, cx)
                    }
                })
                .on_click(cx.listener(|this, event, window, cx| {
                    if toggle_click_allowed(event) {
                        this.set_autostart(cx, window);
                    }
                })),
            ),
        );
        general = general.child(self.card_row(
            window, choose("后台常驻", "Keep running in background"),
            Some(choose("开启后，关闭面板仍保留内容且不挂起，快捷键唤出更快，但会占用更多内存。", "Keep panel contents loaded when closed for faster reopening, at the cost of more memory.")),
            true,
            self.button("background-resident".into(), String::new(), self.value.background_resident, cx, Patch::BackgroundResident(!self.value.background_resident)),
        ));
        #[cfg(windows)]
        {
            general = general.child(self.card_row(window,
                choose("在任务栏显示图标", "Show taskbar icon"), Some(choose("关闭后仍可通过系统托盘或唤出快捷键打开 TP。", "When off, you can still open TP from the system tray or with its shortcut.")), true,
                self.button(
                    "taskbar-icon".into(),
                    choose(if self.value.show_taskbar_icon { "开启" } else { "关闭" },
                        if self.value.show_taskbar_icon { "On" } else { "Off" }).into(),
                    self.value.show_taskbar_icon, cx,
                    Patch::TaskbarIcon(!self.value.show_taskbar_icon),
                ),
            ));
        }
        content = content.child(general.mb(px(if wide { 20. } else { 16. })));
        let appearance = self
            .card()
            .child(self.card_row(
                window,
                choose("主题配色", "Color theme"),
                None,
                false,
                self.theme_control(disabled && !self.theme_open, window, cx),
            ))
            .child(self.card_row(
                window,
                choose("字号", "Font size"),
                None,
                true,
                self.number_control(0, disabled, window, cx),
            ))
            .child(
                self.card_row(
                    window,
                    choose("字体", "Font"),
                    None,
                    true,
                    div()
                        .id("font-trigger")
                        .relative()
                        .child(
                            gpui::canvas(
                                {
                                    let bounds = self.font_bounds[0].clone();
                                    let picker = self
                                        .font_picker
                                        .as_ref()
                                        .filter(|_| self.font_styles == false)
                                        .map(|picker| picker.entity_id());
                                    move |rect, _, cx| {
                                        if bounds.replace(rect) != rect {
                                            if let Some(picker) = picker {
                                                cx.notify(picker);
                                            }
                                        }
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                        .h(px(28.))
                        .px(px(9.))
                        .gap(px(8.))
                        .max_w(gpui::relative(0.5))
                        .min_w_0()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_1()
                        .rounded(px(6.))
                        .border_color(if self.font_picker.is_some() && !self.font_styles {
                            p.accent
                        } else {
                            p.control_border
                        })
                        .bg(if self.font_picker.is_some() && !self.font_styles {
                            p.surface
                        } else {
                            p.control
                        })
                        .hover(|d| d.bg(p.hover))
                        .text_size(px(13. * self.value.font_size as f32 / 13.))
                        .track_focus(&self.font_focus.clone().tab_index(0).tab_stop(!disabled))
                        .cursor_pointer()
                        .child(div().min_w_0().truncate().child(
                            if self.value.font_family.is_empty() {
                                choose("系统默认", "System default").to_owned()
                            } else {
                                self.value.font_family.clone()
                            },
                        ))
                        .child(crate::icons::icon("chevron", 12.).text_color(p.subtle))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_fonts(false, window, cx)),
                        )
                        .on_key_down(cx.listener(|this, event, window, cx| {
                            this.font_trigger_key(false, event, window, cx)
                        })),
                ),
            )
            .when(self.has_font_styles(), |appearance| {
                appearance.child(
                    self.card_row(
                        window,
                        choose("字重", "Font style"),
                        None,
                        true,
                        div()
                            .id("font-style-trigger")
                            .relative()
                            .child(
                                gpui::canvas(
                                    {
                                        let bounds = self.font_bounds[1].clone();
                                        let picker = self
                                            .font_picker
                                            .as_ref()
                                            .filter(|_| self.font_styles == true)
                                            .map(|picker| picker.entity_id());
                                        move |rect, _, cx| {
                                            if bounds.replace(rect) != rect {
                                                if let Some(picker) = picker {
                                                    cx.notify(picker);
                                                }
                                            }
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            )
                            .h(px(28.))
                            .px(px(9.))
                            .gap(px(8.))
                            .max_w(gpui::relative(0.5))
                            .min_w_0()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_1()
                            .rounded(px(6.))
                            .border_color(if self.font_picker.is_some() && self.font_styles {
                                p.accent
                            } else {
                                p.control_border
                            })
                            .bg(if self.font_picker.is_some() && self.font_styles {
                                p.surface
                            } else {
                                p.control
                            })
                            .hover(|d| d.bg(p.hover))
                            .text_size(px(13. * self.value.font_size as f32 / 13.))
                            .track_focus(
                                &self
                                    .font_style_focus
                                    .clone()
                                    .tab_index(0)
                                    .tab_stop(!disabled),
                            )
                            .cursor_pointer()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .child(self.value.font_style.name.clone()),
                            )
                            .child(crate::icons::icon("chevron", 12.).text_color(p.subtle))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_fonts(true, window, cx)
                                }),
                            )
                            .on_key_down(cx.listener(|this, event, window, cx| {
                                this.font_trigger_key(true, event, window, cx)
                            })),
                    ),
                )
            });
        content = content
            .child(appearance)
            .when_some(self.font_picker.clone(), |d, picker| d.child(picker));
        let shortcut_card = self.card()
            .child(self.card_row(window, choose("唤出快捷键", "Open panel shortcut"), None, false, self.shortcut_control(false, disabled, window, cx)))
            .child(self.card_row(window, choose("记录重命名快捷键", "Rename record shortcut"), None, true, self.shortcut_control(true, disabled, window, cx)))
            .child(div().px(px(14.)).py(px(10.)).border_t_1().border_color(p.line)
                .rounded_b(px(10.)).text_size(px(12. * self.value.font_size as f32 / 13.))
                .line_height(px(19.2 * self.value.font_size as f32 / 13.)).text_color(p.muted)
                .bg(gpui::Rgba { r:p.surface.r*0.7+p.bg.r*0.3, g:p.surface.g*0.7+p.bg.g*0.3, b:p.surface.b*0.7+p.bg.b*0.3, a:1. }).child(choose("默认为 F2，仅在记录列表中生效。可使用功能键或带修饰键的字母、数字；按 Backspace 或 Delete 清除。", "Defaults to F2 and works only in the record list. Use function keys or letters/numbers with modifiers. Press Backspace or Delete to clear.")))
;
        content = content.child(
            shortcut_card
                .mt(px(if wide { 20. } else { 16. }))
                .mb(px(if wide { 20. } else { 16. })),
        );
        let records = self
            .card()
            .child(
                self.card_row_content(
                    window,
                    div()
                        .flex()
                        .flex_wrap()
                        .items_baseline()
                        .gap_x(px(8.))
                        .when(narrow, |d| d.flex_col().flex_nowrap().items_start().gap_0())
                        .child(
                            div()
                                .min_w_0()
                                .when(narrow, |d| d.w_full())
                                .child(choose("普通记录数量上限", "Ordinary record limit")),
                        )
                        .child(div().text_color(p.subtle).child(format!(
                            "{}/{}",
                            self.ordinary_count, self.value.max_ordinary
                        ))),
                    None,
                    false,
                    self.number_control(1, disabled, window, cx),
                ),
            )
            .child(self.card_row(
                window,
                choose("普通记录保留时间", "Keep ordinary records for"),
                None,
                true,
                self.retention_control(disabled, cx),
            ))
            .child(self.card_row(
                window,
                choose("1Password 记录保留秒数", "Keep 1Password records (seconds)"),
                None,
                true,
                self.number_control(2, disabled, window, cx),
            ));
        content = content.child(records).child(div().mt(px(7.)).mx(px(10.))
            .text_size(px(12. * self.value.font_size as f32 / 13.)).line_height(px(19.2 * self.value.font_size as f32 / 13.)).text_color(p.muted)
            .child(choose("数量和时间上限只作用于普通记录。收藏或已分组的记录只能手动删除。", "Count and time limits apply only to ordinary records. Favorites and grouped records must be deleted manually.")));
        let behavior = self.card()
            .child(self.card_row(
                window, choose("保留搜索框输入", "Keep search text"),
                Some(choose("关闭后，每次面板关闭都会清空搜索框内的关键词。", "When off, the search field is cleared each time the panel closes.")), false,
                self.button("retain-search".into(), String::new(), self.value.retain_search_input, cx, Patch::RetainSearch(!self.value.retain_search_input)),
            ))
            .child(self.card_row(
                window, choose("保留类型筛选", "Keep type filter"),
                Some(choose("关闭后，每次面板关闭都会将类型筛选恢复为“全部”。", "When off, the type filter resets to “All” each time the panel closes.")), true,
                self.button("retain-filter".into(), String::new(), self.value.retain_type_filter, cx, Patch::RetainFilter(!self.value.retain_type_filter)),
            ))
            .child(self.card_row(
                window, choose("移除末尾空白", "Remove trailing whitespace"),
                Some(choose("复制后自动移除文本末尾的换行和空格，直接粘贴也生效。关闭后新复制的内容会保留原文。", "Remove trailing line breaks and spaces after copying, including direct paste. When off, newly copied text keeps its original whitespace.")), true,
                self.button("trim".into(), String::new(), self.value.remove_trailing_newlines, cx, Patch::Trim(!self.value.remove_trailing_newlines)),
            ));
        content = content.child(
            behavior
                .mt(px(if wide { 20. } else { 16. }))
                .mb(px(if wide { 20. } else { 16. })),
        );
        let mut position_content = self
            .card()
            .child(self.position_radio("fixed", choose("固定位置", "Fixed position"), window, cx))
            .child(self.position_radio("cursor", choose("跟随光标", "Follow cursor"), window, cx));
        position_content =
            position_content.child(div().border_t_1().border_color(p.line).child(self.card_row(
                window,
                choose("固定位置", "Fixed position"),
                None,
                false,
                self.fixed_position_control(disabled, window, cx),
            )));
        position_content = position_content.child(
            self.card_row(
                window,
                choose("记忆拖拽位置", "Remember dragged position"),
                None,
                true,
                self.button(
                    "remember-position".into(),
                    choose(
                        if self.value.remember_position {
                            "开启"
                        } else {
                            "关闭"
                        },
                        if self.value.remember_position {
                            "On"
                        } else {
                            "Off"
                        },
                    )
                    .into(),
                    self.value.remember_position,
                    cx,
                    Patch::RememberPosition(!self.value.remember_position, None),
                ),
            ),
        );
        content = content.child(position_content).child(
            div().mt(px(7.)).mx(px(10.)).text_size(px(12. * self.value.font_size as f32 / 13.))
                .line_height(px(19.2 * self.value.font_size as f32 / 13.)).text_color(p.muted)
                .child(choose("拖拽面板边缘调整大小；缩放光标出现时，双击左右边缘恢复默认宽度，双击上下边缘恢复默认高度，双击角落恢复两维。大小会在下次打开时恢复。",
                    "Drag panel edges to resize. With a resize cursor, double-click a side to reset width, the top or bottom to reset height, or a corner to reset both. The size is restored next time."))
        );
        content =
            content.child(
                self.storage_cards(disabled, cx)
                    .mt(px(if wide { 20. } else { 16. })),
            );
        content = content
            .child(
                self.card()
                    .mt(px(14.))
                    .px(px(14.))
                    .py(px(10.))
                    .min_h(px(42.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .text_color(p.muted)
                    .text_size(px(12. * self.value.font_size as f32 / 13.))
                    .child(choose("当前版本", "Current version"))
                    .child(env!("CARGO_PKG_VERSION")),
            )
            .child(
                self.card()
                    .mt(px(if wide { 20. } else { 16. }))
                    .overflow_hidden()
                    .child(crate::settings_reference::render(
                        &self.value,
                        window.viewport_size().width < px(360.),
                    )),
            );
        let mut root = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.text)
            .text_size(px(self.value.font_size as f32))
            .child(
                div()
                    .id("settings-content")
                    .track_scroll(&self.content_scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .border_t_1()
                    .border_color(p.line)
                    .bg(gpui::Rgba {
                        r: p.bg.r * 0.65 + p.control.r * 0.35,
                        g: p.bg.g * 0.65 + p.control.g * 0.35,
                        b: p.bg.b * 0.65 + p.control.b * 0.35,
                        a: 1.,
                    })
                    .child(content),
            )
            .child(
                div().absolute().inset_0().top(px(1.)).child(
                    gpui_component::scroll::Scrollbar::vertical(&self.content_scroll)
                        .id("settings-scrollbar")
                        .thin(px(7.))
                        .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                ),
            );
        if let Some((_, count)) = &self.pending {
            root = root.child(self.pending_dialog(*count, cx));
        }
        if let Some(cleanup) = &self.cleanup {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .top(px(-52.))
                    .child(cleanup.clone()),
            );
        }
        if let Some(transfer) = &self.transfer {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .top(px(-52.))
                    .child(transfer.clone()),
            );
        }
        crate::theme::typography(root, &self.value)
    }
}

/// HTML checkboxes toggle on Space, whereas GPUI also generates clicks for Enter.
fn toggle_click_allowed(event: &gpui::ClickEvent) -> bool {
    !matches!(event, gpui::ClickEvent::Keyboard(key) if key.button == gpui::KeyboardButton::Enter)
}

/// Original settings checkbox dimensions; the thumb position includes its 1px border inset.
fn toggle_visual(
    id: String,
    checked: bool,
    disabled: bool,
    p: Palette,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(gpui::SharedString::from(id))
        .relative()
        .flex_shrink_0()
        .w(px(38.))
        .h(px(23.))
        .border_1()
        .border_color(if checked { p.accent } else { p.control_border })
        .rounded(px(23.))
        .bg(if checked { p.accent } else { p.control })
        .when(disabled, |d| d.opacity(0.55))
        .when(!disabled, |d| {
            d.cursor_pointer().tab_index(0).focus(|d| {
                d.shadow(vec![gpui::BoxShadow {
                    color: p.accent.into(),
                    offset: gpui::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(2.),
                }])
            })
        })
        .child(
            div()
                .absolute()
                .top(px(1.))
                .left(px(if checked { 16. } else { 1. }))
                .size(px(19.))
                .rounded_full()
                .bg(if checked {
                    p.button_text
                } else {
                    gpui::rgb(0xFFFFFF)
                })
                .shadow(vec![gpui::BoxShadow {
                    color: gpui::rgba(0x00000033).into(),
                    offset: gpui::point(px(0.), px(1.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                }]),
        )
}
