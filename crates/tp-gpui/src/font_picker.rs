use crate::theme::Palette;
use gpui::{
    div, prelude::*, px, Context, Entity, EntityInputHandler, EventEmitter, Render, ScrollHandle,
    Subscription, Window,
};
use gpui_component::input::{Input, InputEvent, InputState};
use tp_core::{
    font_search::FontSearchIndex,
    fonts::{FontFamily, FontStyle},
    i18n::choose,
};
pub enum Event {
    Catalog(std::sync::Arc<Vec<FontFamily>>),
    Select(String, FontStyle),
    Close,
    TabFromSearch { reverse: bool },
}

fn family_candidate_index(options: &[(String, FontStyle)], candidate: &str) -> usize {
    options
        .iter()
        .position(|(family, _)| family == candidate)
        .unwrap_or(0)
}
pub struct FontPicker {
    anchor: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    menu_focus: gpui::FocusHandle,
    font_size: u16,
    theme: String,
    family: String,
    style: FontStyle,
    styles_only: bool,
    fonts: Vec<FontFamily>,
    index: Option<FontSearchIndex>,
    search: Entity<InputState>,
    active: usize,
    active_family: String,
    requested_style_index: Option<usize>,
    scroll: ScrollHandle,
    reveal_pending: std::rc::Rc<std::cell::Cell<bool>>,
    error: Option<String>,
    loading: bool,
    catalog_loaded: bool,
    #[cfg(debug_assertions)]
    diagnostic_failed_once: bool,
    _subscription: Subscription,
}
impl EventEmitter<Event> for FontPicker {}
impl FontPicker {
    pub(crate) fn search_focused(&self, window: &Window, cx: &Context<Self>) -> bool {
        use gpui::Focusable;
        !self.styles_only && self.search.read(cx).focus_handle(cx).is_focused(window)
    }
    pub(crate) fn style_navigation(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> (usize, Option<usize>) {
        (
            self.options(cx).len(),
            self.menu_focus.is_focused(window).then_some(self.active),
        )
    }
    pub(crate) fn focus_style(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.requested_style_index = Some(index);
        self.active = index.min(self.options(cx).len().saturating_sub(1));
        self.menu_focus.focus(window);
        self.scroll.scroll_to_item(self.active);
        self.reveal_pending.set(true);
        cx.notify();
    }
    pub(crate) fn reveal_selected_after_load(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.requested_style_index = None;
        self.active = self
            .options(cx)
            .iter()
            .position(|(_, style)| *style == self.style)
            .unwrap_or(0);
        self.scroll.scroll_to_item(self.active);
        self.menu_focus.focus(window);
        self.reveal_pending.set(true);
        cx.notify();
    }
    pub(crate) fn is_composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        !self.styles_only
            && self.search.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
    }
    pub fn new(
        theme: String,
        font_size: u16,
        family: String,
        style: FontStyle,
        styles_only: bool,
        retained_catalog: Option<(std::sync::Arc<Vec<FontFamily>>, ScrollHandle)>,
        anchor: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(if styles_only {
                choose("搜索字体样式", "Search font styles")
            } else {
                choose("字体名、拼音或首字母", "Font name, pinyin, or initials")
            })
        });
        let subscription = cx.subscribe_in(&search, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.active = 0;
                this.active_family = this
                    .options(cx)
                    .first()
                    .map(|(family, _)| family.clone())
                    .unwrap_or_default();
                this.scroll.scroll_to_item(0);
                this.reveal_pending.set(!this.styles_only);
                cx.notify();
            }
        });
        let menu_focus = cx.focus_handle();
        if !styles_only {
            search.update(cx, |input, cx| input.focus(window, cx));
        }
        let mut picker = Self {
            menu_focus,
            font_size,
            anchor,
            theme,
            active_family: family.clone(),
            family,
            style,
            styles_only,
            fonts: vec![],
            index: None,
            search,
            active: 0,
            requested_style_index: None,
            scroll: ScrollHandle::new(),
            reveal_pending: Default::default(),
            error: None,
            loading: false,
            catalog_loaded: false,
            #[cfg(debug_assertions)]
            diagnostic_failed_once: false,
            _subscription: subscription,
        };
        if let Some((fonts, scroll)) = retained_catalog {
            // The original style component uses its parent's already loaded
            // catalog and stays mounted while hidden. Preserve its scroll offset.
            picker.fonts = (*fonts).clone();
            picker.catalog_loaded = true;
            picker.scroll = scroll;
        }
        // Even before a successful enumeration, the original offers System
        // default and the configured family. Refresh failures retain this list.
        picker.index = Some(FontSearchIndex::new(&picker.fonts, &picker.family));
        picker.active = picker
            .options(cx)
            .iter()
            .position(|(family, style)| {
                *family == picker.family && (!styles_only || *style == picker.style)
            })
            .unwrap_or(0);
        if !styles_only {
            picker.reveal_pending.set(true);
        }
        if !styles_only || !picker.catalog_loaded {
            picker.load(window, cx);
        }
        picker
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        let selected = self.family.clone();
        let selected_style = self.style.clone();
        #[cfg(debug_assertions)]
        let diagnostic_delay = if std::env::var_os("TP_DIAGNOSTIC_DATA_DIR").is_some() {
            std::env::var("TP_DIAGNOSTIC_FONT_DELAY_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
                .min(30_000)
        } else {
            0
        };
        #[cfg(debug_assertions)]
        let diagnostic_failure = {
            let enabled = !self.diagnostic_failed_once
                && std::env::var_os("TP_DIAGNOSTIC_DATA_DIR").is_some()
                && std::env::var_os("TP_DIAGNOSTIC_FONT_FAILURE_ONCE").is_some();
            self.diagnostic_failed_once |= enabled;
            enabled
        };
        // NSFontManager requires the main thread; DirectWrite can enumerate on a worker.
        #[cfg(target_os = "macos")]
        let enumerated = tp_desktop::fonts::get_system_fonts();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    #[cfg(debug_assertions)]
                    if diagnostic_delay > 0 {
                        eprintln!("Diagnostic font refresh delay started: {diagnostic_delay}ms");
                        std::thread::sleep(std::time::Duration::from_millis(diagnostic_delay));
                    }
                    #[cfg(debug_assertions)]
                    if diagnostic_failure {
                        return Err("Diagnostic font enumeration failure".to_owned());
                    }
                    #[cfg(windows)]
                    let enumerated = tp_desktop::fonts::get_system_fonts();
                    let fonts = enumerated?;
                    let (selected, _) = tp_core::fonts::resolve(&fonts, &selected, &selected_style);
                    let index = FontSearchIndex::new(&fonts, &selected);
                    Ok::<_, String>((fonts, index))
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.loading = false;
                match result {
                    Ok((fonts, index)) => {
                        this.catalog_loaded = true;
                        let (family, style) =
                            tp_core::fonts::resolve(&fonts, &this.family, &this.style);
                        this.family = family;
                        this.style = style;
                        cx.emit(Event::Catalog(std::sync::Arc::new(fonts.clone())));
                        this.fonts = fonts;
                        this.index = Some(index);
                        let options = this.options(cx);
                        this.active = if this.styles_only {
                            options
                                .iter()
                                .position(|(_, style)| *style == this.style)
                                .unwrap_or(0)
                        } else {
                            // Refresh must preserve the user's current candidate,
                            // not jump back to the saved setting. Keep its identity
                            // even when temporarily absent, just like React's active.
                            family_candidate_index(&options, &this.active_family)
                        };
                        if this.styles_only {
                            if let Some(index) = this.requested_style_index {
                                this.active = index.min(options.len().saturating_sub(1));
                            }
                        }
                        if !this.styles_only || this.reveal_pending.get() {
                            this.scroll.scroll_to_item(this.active);
                        }
                        this.reveal_pending
                            .set(this.reveal_pending.get() || !this.styles_only);
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
                #[cfg(debug_assertions)]
                if diagnostic_delay > 0 {
                    eprintln!("Diagnostic font refresh result applied");
                }
            });
        })
        .detach();
    }
    fn menu_width(&self, window: &Window) -> gpui::Pixels {
        // Measure the complete catalog, never the current search result.
        let labels: Vec<String> = if self.styles_only {
            self.fonts
                .iter()
                .find(|font| font.name == self.family)
                .map(|font| font.styles.iter().map(|style| style.name.clone()).collect())
                .unwrap_or_else(|| vec![self.style.name.clone()])
        } else {
            // Match searchFontFamilies(index, "") in FontSetting. The index
            // includes a configured family missing from the system catalog;
            // measuring only `fonts` would omit that potentially longest label.
            self.index
                .as_ref()
                .map(|index| index.search(""))
                .unwrap_or_default()
                .into_iter()
                .map(|name| {
                    if name.is_empty() {
                        choose("系统默认", "System default").to_owned()
                    } else {
                        name
                    }
                })
                .collect()
        };
        let normal = window.text_style();
        let mut semibold = normal.clone();
        semibold.font_weight = gpui::FontWeight::SEMIBOLD;
        let size = px(12. * self.font_size as f32 / 13.);
        let longest = labels
            .iter()
            .flat_map(|label| {
                [&normal, &semibold].into_iter().map(move |style| {
                    window
                        .text_system()
                        .shape_line(
                            label.to_owned().into(),
                            size,
                            &[style.to_run(label.len())],
                            None,
                        )
                        .width
                })
            })
            .fold(px(0.), |a, b| a.max(b));
        let viewport_width = window.viewport_size().width;
        let outer_padding = if viewport_width >= px(760.) {
            48.
        } else if viewport_width < px(360.) {
            20.
        } else {
            32.
        };
        let row_padding = if viewport_width < px(360.) { 20. } else { 28. };
        let available =
            (viewport_width.min(px(620.)) - px(outer_padding + row_padding + 2.)).max(px(1.));
        px((f32::from(longest).ceil() + 40.).max(if self.styles_only { 0. } else { 180. }))
            .min(available)
    }

    fn options(&self, cx: &Context<Self>) -> Vec<(String, FontStyle)> {
        let query = self.search.read(cx).value();
        if self.styles_only {
            let styles = self
                .fonts
                .iter()
                .find(|f| f.name == self.family)
                .map(|f| f.styles.clone())
                .unwrap_or_else(|| vec![self.style.clone()]);
            let needle = query.to_lowercase();
            styles
                .into_iter()
                .filter(|s| s.name.to_lowercase().contains(&needle))
                .map(|s| (self.family.clone(), s))
                .collect()
        } else {
            self.index
                .as_ref()
                .map(|index| {
                    index
                        .search(&query)
                        .into_iter()
                        .map(|family| {
                            let style = self
                                .fonts
                                .iter()
                                .find(|f| f.name == family)
                                .map(FontFamily::default_style)
                                .unwrap_or_default();
                            (family, style)
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
    }
    fn keys(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.styles_only
            && self.search.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        {
            return;
        }
        let options = self.options(cx);
        match event.keystroke.key.as_str() {
            "escape" => cx.emit(Event::Close),
            "tab" if !self.styles_only => cx.emit(Event::TabFromSearch {
                reverse: event.keystroke.modifiers.shift,
            }),
            "tab" => return,
            "up" | "down" if !options.is_empty() => {
                self.active = (self.active as isize
                    + if event.keystroke.key == "up" { -1 } else { 1 })
                .rem_euclid(options.len() as isize) as usize;
                self.scroll.scroll_to_item(self.active);
                if !self.styles_only {
                    self.active_family = options[self.active].0.clone();
                }
            }
            "home" | "end" if self.styles_only && !options.is_empty() => {
                self.active = if event.keystroke.key == "home" {
                    0
                } else {
                    options.len() - 1
                };
                self.scroll.scroll_to_item(self.active);
            }
            "enter" | "space"
                if !event.is_held && (self.styles_only || event.keystroke.key == "enter") =>
            {
                if let Some((family, style)) = options.get(self.active) {
                    cx.emit(Event::Select(family.clone(), style.clone()));
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }
}
impl Render for FontPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_id(&self.theme);
        let scale = self.font_size as f32 / 13.;
        let anchor = self.anchor.get();
        let viewport = window.viewport_size();
        let below = (viewport.height - px(8.) - anchor.bottom() - px(5.)).max(px(0.));
        let above = (anchor.top() - px(5.) - px(90.)).max(px(0.));
        let upwards = below < px(180.) && above > below;
        let room = if upwards { above } else { below }.max(px(60.));
        let list_height = if self.styles_only {
            // CSS max-height: 240px includes the menu's border and padding.
            (room.min(px(240.)) - px(12.)).max(px(30.))
        } else {
            (room - px(53.)).max(px(30.)).min(px(240.))
        };
        let width = self.menu_width(window);
        let x = anchor
            .right()
            .max(width + px(8.))
            .min(viewport.width - px(8.));
        let y = if upwards {
            anchor.top() - px(5.)
        } else {
            anchor.bottom() + px(5.)
        };
        let mut list = div()
            .id("font-options")
            .max_h(list_height)
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        let options = self.options(cx);
        let no_options = options.is_empty();
        for (index, (family, style)) in options.into_iter().enumerate() {
            let active =
                index == self.active && (!self.styles_only || self.menu_focus.is_focused(window));
            let selected = family == self.family && (!self.styles_only || style == self.style);
            let label = if self.styles_only {
                style.name.clone()
            } else if family.is_empty() {
                choose("系统默认", "System default").into()
            } else {
                family.clone()
            };
            let hover_family = family.clone();
            list = list.child(
                div()
                    .id(index)
                    .px(px(9.))
                    .py(px(5.))
                    .min_h(px(30.))
                    .rounded(px(6.))
                    .text_size(px(12. * scale))
                    .line_height(px(18. * scale))
                    .truncate()
                    .tooltip({
                        let label = label.clone();
                        move |window, cx| {
                            let width = (window.viewport_size().width - px(48.)).max(px(1.));
                            let label = label.clone();
                            gpui_component::tooltip::Tooltip::element(move |_, _| {
                                div().max_w(width).whitespace_normal().child(label.clone())
                            })
                            .build(window, cx)
                        }
                    })
                    .when(selected && !self.styles_only, |d| {
                        d.font_weight(gpui::FontWeight::SEMIBOLD)
                    })
                    .text_color(if active || (selected && self.styles_only) {
                        p.group_text
                    } else if selected {
                        p.accent
                    } else {
                        p.muted
                    })
                    .when(self.styles_only, |d| {
                        d.hover(|d| d.bg(p.soft).text_color(p.group_text))
                    })
                    .cursor_pointer()
                    .when(active || (selected && self.styles_only), |d| d.bg(p.soft))
                    .child(label)
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered && !this.styles_only {
                            // A catalog refresh can precede the next paint. Resolve
                            // the rendered row by identity, not its stale index.
                            if let Some(current) = this
                                .options(cx)
                                .iter()
                                .position(|(family, _)| *family == hover_family)
                            {
                                this.active = current;
                                this.active_family = hover_family.clone();
                                cx.notify();
                            }
                        }
                    }))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(Event::Select(family.clone(), style.clone()))
                    })),
            );
        }
        let pending = self.reveal_pending.clone();
        let scroll = self.scroll.clone();
        let active = self.active;
        let ready = self.catalog_loaded || !self.loading;
        let picker = cx.entity().downgrade();
        let mut panel = div()
            .id("font-popup")
            .key_context("FontPicker")
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                // The trigger owns its toggle. Closing here first would make its
                // following click reopen the same menu.
                if !this.anchor.get().contains(&event.position) {
                    cx.emit(Event::Close);
                }
            }))
            .when(self.styles_only, |d| d.track_focus(&self.menu_focus))
            .capture_key_down(cx.listener(Self::keys))
            .border_1()
            .border_color(p.line)
            .rounded(px(9.))
            .w(width)
            .shadow(vec![Palette::popup_shadow()])
            .occlude()
            .bg(p.surface)
            .text_color(p.text)
            .p(px(if self.styles_only { 5. } else { 6. }))
            .when(!self.styles_only, |d| d.child(Input::new(&self.search)))
            .child(
                div()
                    .relative()
                    .when(!self.styles_only, |d| d.mt(px(5.)))
                    .child(list)
                    .child(
                        gpui::canvas(
                            move |_, _, cx| {
                                if ready
                                    && pending.get()
                                    && scroll.bounds_for_item(active).is_some()
                                {
                                    pending.set(false);
                                    scroll.scroll_to_item(active);
                                    cx.defer(move |cx| {
                                        let _ = picker.update(cx, |_, cx| cx.notify());
                                    });
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(
                        div().absolute().inset_0().child(
                            gpui_component::scroll::Scrollbar::vertical(&self.scroll)
                                .id("font-scrollbar")
                                .thin(px(7.))
                                .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                        ),
                    ),
            );
        // Match the shared .font-status element outside the scrollable list.
        let status = |label| {
            div()
                .mt(px(5.))
                .py(px(6.))
                .px(px(4.))
                .w_full()
                .text_center()
                .text_size(px(12. * scale))
                .text_color(p.muted)
                .child(label)
        };
        if self.error.is_some() {
            panel = panel.child(
                status(choose(
                    "读取字体失败，点击重试",
                    "Could not load fonts. Click to retry",
                ))
                .id("retry-fonts")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.load(window, cx);
                    cx.notify();
                })),
            );
        } else if self.loading && !self.catalog_loaded {
            panel = panel.child(status(choose("正在读取系统字体…", "Loading system fonts…")));
        } else if no_options {
            panel = panel.child(status(choose("没有匹配的字体", "No matching fonts")));
        }
        gpui::deferred(
            gpui::anchored()
                .anchor(if upwards {
                    gpui::Corner::BottomRight
                } else {
                    gpui::Corner::TopRight
                })
                .position(gpui::point(x, y))
                .snap_to_window_with_margin(gpui::Edges {
                    top: px(90.),
                    right: px(8.),
                    bottom: px(8.),
                    left: px(8.),
                })
                .child(panel),
        )
        .priority(8)
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;

    fn options(names: &[&str]) -> Vec<(String, FontStyle)> {
        names
            .iter()
            .map(|name| ((*name).into(), FontStyle::default()))
            .collect()
    }

    #[test]
    fn refresh_tracks_candidate_identity_across_reordering_not_saved_font() {
        let before = options(&["", "Arial", "Segoe UI"]);
        let candidate = before[1].0.clone(); // User navigated away from saved Segoe UI.
        let after = options(&["", "Calibri", "Segoe UI", "Arial"]);
        let active = family_candidate_index(&after, &candidate);
        assert_eq!(after[active].0, "Arial"); // Enter still selects the same family.
        assert_eq!(active, 3);
    }

    #[test]
    fn absent_candidate_falls_back_without_forgetting_its_identity() {
        let candidate = "Arial";
        assert_eq!(
            family_candidate_index(&options(&["", "Segoe UI"]), candidate),
            0
        );
        assert_eq!(
            family_candidate_index(&options(&["", "Arial", "Segoe UI"]), candidate),
            1
        );
        assert_eq!(family_candidate_index(&[], candidate), 0);
    }

    #[test]
    fn refreshed_filtered_results_do_not_replace_the_users_candidate() {
        let after = options(&["Arial", "Arial Narrow", "Arial Unicode MS"]);
        assert_eq!(family_candidate_index(&after, "Arial Narrow"), 1);
        assert_eq!(family_candidate_index(&options(&[""]), ""), 0);
    }
}
