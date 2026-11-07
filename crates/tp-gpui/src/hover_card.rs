//! Floating record card, separate from the native full-size media viewer.
use crate::{
    highlight,
    native_window::hover_preview::Anchor,
    theme::{self, Palette},
};
use gpui::{div, prelude::*, px, Context, Render, Window};
use std::{cell::Cell, rc::Rc, sync::Arc};
use tp_core::{i18n::choose, records::Record, settings::Settings};

// Match preview.css and the carousel's fitImage: preserve intrinsic size,
// constrain both dimensions, and only shrink. Card padding/border total 38 px;
// single media adds 16 px of padding, while the carousel has none.
fn preview_image_size(width: u32, height: u32, card_width: f32, multiple: bool) -> (f32, f32) {
    let available_width = (card_width - if multiple { 38. } else { 54. }).max(1.);
    let available_height = if multiple { 320. } else { 520. };
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let scale = 1_f32
        .min(available_width / width)
        .min(available_height / height);
    (width * scale, height * scale)
}

// media-indicators.ts keeps 8 px of breathing room when revealing a page.
fn indicator_scroll_left(index: usize, count: usize, viewport: f32, current: f32) -> f32 {
    let left = 3. + index as f32 * 18.;
    let right = left + 18.;
    let next = if left < current {
        left - 8.
    } else if right > current + viewport {
        right - viewport + 8.
    } else {
        current
    };
    next.clamp(0., (count as f32 * 18. + 6. - viewport).max(0.))
}

#[cfg(test)]
mod image_size_tests {
    use super::{indicator_scroll_left, preview_image_size};

    #[test]
    fn page_indicators_reveal_both_edges_and_preserve_visible_positions() {
        assert_eq!(indicator_scroll_left(0, 30, 100., 0.), 0.);
        assert_eq!(indicator_scroll_left(5, 30, 100., 0.), 19.);
        assert_eq!(indicator_scroll_left(5, 30, 100., 19.), 19.);
        assert_eq!(indicator_scroll_left(0, 30, 100., 19.), 0.);
        assert_eq!(indicator_scroll_left(29, 30, 100., 0.), 446.);
        assert_eq!(indicator_scroll_left(2, 3, 100., 0.), 0.);
    }

    #[test]
    fn small_images_keep_intrinsic_dimensions_in_both_preview_modes() {
        for multiple in [false, true] {
            assert_eq!(preview_image_size(24, 16, 400., multiple), (24., 16.));
        }
    }

    #[test]
    fn landscape_and_portrait_images_fit_without_distortion() {
        assert_eq!(preview_image_size(692, 346, 400., false), (346., 173.));
        assert_eq!(preview_image_size(100, 1040, 400., false), (50., 520.));
        assert_eq!(preview_image_size(724, 362, 400., true), (362., 181.));
        assert_eq!(preview_image_size(100, 640, 400., true), (50., 320.));
    }
}

#[derive(Clone)]
struct CachedMedia {
    image: Arc<gpui::Image>,
    width: u32,
    height: u32,
}

impl CachedMedia {
    fn release_asset(&self, cx: &mut gpui::App) {
        gpui::ImageSource::from(self.image.clone()).remove_asset(cx);
    }
}

pub struct HoverCard {
    lifetime: Rc<crate::hover_lifetime::HoverLifetime>,
    record: Record,
    store: Arc<tp_desktop::Store>,
    media: crate::media_transition::Carousel<Result<CachedMedia, String>>,
    indicator_scroll: gpui::ScrollHandle,
    reveal_indicator: bool,
    media_generation: u64,
    media_cache: crate::media_cache::MediaCache<Result<CachedMedia, String>>,
    media_hovered: bool,
    page_focus: Vec<gpui::FocusHandle>,
    keyboard_focus: bool,
    last_wheel: Option<std::time::Instant>,
    focus: gpui::FocusHandle,
    body_text: gpui::Entity<crate::selectable_text::SelectableText>,
    body_scroll: gpui::ScrollHandle,
    outer_scroll: gpui::ScrollHandle,
    file_scroll: gpui::ScrollHandle,
    open_large: Rc<Cell<bool>>,
    settings: Settings,
    search: String,
    group: Option<String>,
    anchor: Rc<Anchor>,
    measured: Rc<Cell<f32>>,
    pub inside: Rc<Cell<bool>>,
    pub over_record: Rc<Cell<bool>>,
    image: Option<Arc<gpui::Image>>,
    image_size: (f32, f32),
    media_error: bool,
    file_text: Option<gpui::Entity<crate::selectable_text::SelectableText>>,
    file_text_font: Option<String>,
    file_paths: Vec<gpui::Entity<crate::selectable_text::SelectableText>>,
    file_error: Option<String>,
    was_active: bool,
    _blur: gpui::Subscription,
}
impl HoverCard {
    pub fn new(
        store: Arc<tp_desktop::Store>,
        record: Record,
        settings: Settings,
        search: String,
        group: Option<String>,
        anchor: Anchor,
        inside: Rc<Cell<bool>>,
        over_record: Rc<Cell<bool>>,
        open_large: Rc<Cell<bool>>,
        lifetime: Rc<crate::hover_lifetime::HoverLifetime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.set_window_title(choose("TP 预览", "TP Preview"));
        cx.on_release(|this, cx| {
            for media in this.media_cache.drain().flatten() {
                media.release_asset(cx);
            }
        })
        .detach();
        let focus = cx.focus_handle();
        focus.focus(window);
        let blur = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                // Activation can contain a transient deactivation while Windows
                // raises a borderless window. Verify after that event sequence.
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(30))
                        .await;
                    let _ = this.update_in(cx, |this, window, _| {
                        if !window.is_window_active() {
                            this.lifetime.close();
                            crate::native_window::hide(window);
                        }
                    });
                })
                .detach();
            }
        });
        let body_scroll = gpui::ScrollHandle::new();
        let body_text = cx.new(|cx| {
            let text = crate::selectable_text::SelectableText::new(
                tp_core::browse::text_for_use(&record.body, settings.remove_trailing_newlines)
                    .to_owned(),
                search.clone(),
                Palette::for_id(&settings.theme),
                window,
                cx,
            );
            if record.kind == "text" {
                text.reveal_search_in(body_scroll.clone())
            } else {
                text
            }
        });
        let file_paths =
            tp_core::records::preview_paths(&record.files, record.asset_path.as_deref())
                .into_iter()
                .map(|path| {
                    cx.new(|cx| {
                        crate::selectable_text::SelectableText::file_path(
                            path.to_owned(),
                            search.clone(),
                            Palette::for_id(&settings.theme),
                            window,
                            cx,
                        )
                    })
                })
                .collect();
        let page_focus = (0..record.files.len())
            .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
            .collect();
        let mut card = Self {
            lifetime,
            record,
            store,
            media: Default::default(),
            indicator_scroll: gpui::ScrollHandle::new(),
            reveal_indicator: true,
            media_generation: 0,
            media_cache: Default::default(),
            media_hovered: false,
            page_focus,
            keyboard_focus: false,
            last_wheel: None,
            focus,
            body_text,
            body_scroll,
            outer_scroll: gpui::ScrollHandle::new(),
            file_scroll: gpui::ScrollHandle::new(),
            open_large,
            settings,
            search,
            group,
            anchor: Rc::new(anchor),
            measured: Rc::new(Cell::new(0.)),
            inside,
            over_record,
            image: None,
            image_size: (0., 0.),
            media_error: false,
            file_text: None,
            file_text_font: None,
            file_paths,
            file_error: None,
            was_active: false,
            _blur: blur,
        };
        card.load_content(window, cx);
        card
    }

    fn select_media(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.media.select(index, self.record.files.len()) {
            return;
        }
        self.load_content(window, cx);
        cx.notify();
    }

    fn show_cached_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.media.load_index() else {
            return;
        };
        let Some(result) = self.media_cache.get(index).cloned() else {
            return;
        };
        if let Some((index, result)) = self.media.loaded(
            index,
            result,
            crate::native_window::reduced_motion(),
            std::time::Instant::now(),
        ) {
            self.commit_media(index, result, window, cx);
            if self.media.desired() != self.media.current() {
                self.load_content(window, cx);
            }
        }
        cx.notify();
    }

    fn commit_media(
        &mut self,
        index: usize,
        result: Result<CachedMedia, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reveal_indicator = true;
        self.image = None;
        self.image_size = (0., 0.);
        self.media_error = result.is_err();
        if let Ok(media) = result {
            self.image_size = preview_image_size(
                media.width,
                media.height,
                self.anchor.placement.logical_width() as f32,
                self.record.files.len() > 1,
            );
            self.image = Some(media.image);
        }
        for media in self
            .media_cache
            .retain(index, self.media.desired())
            .into_iter()
            .flatten()
        {
            media.release_asset(cx);
        }
        let count = self.record.files.len().max(1);
        if index > 0 {
            self.request_media(index - 1, window, cx);
        }
        if index + 1 < count {
            self.request_media(index + 1, window, cx);
        }
        cx.notify();
        window.defer(cx, |window, cx| {
            window.draw(cx).clear();
            window.refresh();
        });
    }

    fn request_media(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.media_cache.begin(index) {
            return;
        }
        let store = self.store.clone();
        let id = self.record.id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let bytes = tp_desktop::media::preview_png(
                        &store,
                        id,
                        Some("preview".into()),
                        Some(index),
                    )?;
                    // Probe dimensions off the UI thread as well; malformed images use
                    // the same cached unavailable state as a failed source read.
                    let decoded = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
                    Ok::<_, String>((bytes, decoded.width(), decoded.height()))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.lifetime.is_closed() {
                    return;
                }
                let result = result.map(|(bytes, width, height)| CachedMedia {
                    image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)),
                    width,
                    height,
                });
                this.media_cache.complete(
                    index,
                    result,
                    this.media.current(),
                    this.media.desired(),
                );
                // A prefetch or late result must never replace the requested page.
                if this.media.load_index() == Some(index) {
                    this.show_cached_media(window, cx);
                }
            });
        })
        .detach();
    }

    fn load_content(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.record.kind.as_str(), "image" | "video") {
            let Some(index) = self.media.load_index() else {
                return;
            };
            for media in self
                .media_cache
                .retain(self.media.current(), self.media.desired())
                .into_iter()
                .flatten()
            {
                media.release_asset(cx);
            }
            if self.media_cache.get(index).is_some() {
                self.show_cached_media(window, cx);
            } else {
                self.request_media(index, window, cx);
            }
            return;
        }

        if self.record.kind != "file" || self.record.files.len() != 1 {
            return;
        }
        self.media_generation = self.media_generation.wrapping_add(1);
        let generation = self.media_generation;
        let store = self.store.clone();
        let id = self.record.id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { tp_desktop::media::file_text_preview(&store, id, 0) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.lifetime.is_closed() || this.media_generation != generation {
                    return;
                }
                match result {
                    Ok(text) => {
                        if text.is_some() {
                            this.file_text_font =
                                Some(theme::file_preview_font_family(&this.settings, window));
                        }
                        this.file_text = text.map(|text| {
                            cx.new(|cx| {
                                crate::selectable_text::SelectableText::new(
                                    text,
                                    String::new(),
                                    Palette::for_id(&this.settings.theme),
                                    window,
                                    cx,
                                )
                            })
                        })
                    }
                    Err(error) => this.file_error = Some(error),
                }
                cx.notify();
                window.defer(cx, |window, cx| {
                    window.draw(cx).clear();
                    window.refresh();
                });
            });
        })
        .detach();
    }
}
impl Render for HoverCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some((index, incoming)) = self.media.finish(std::time::Instant::now()) {
            self.commit_media(index, incoming, window, cx);
            if self.media.desired() != self.media.current() {
                self.load_content(window, cx);
            }
        }
        let p = Palette::for_id(&self.settings.theme);
        let scale = self.settings.font_size as f32 / 13.;
        let body = tp_core::browse::text_for_use(
            &self.record.body,
            self.settings.remove_trailing_newlines,
        )
        .to_owned();
        let mut card = div()
            .id("floating-card")
            .track_focus(&self.focus)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.keyboard_focus = false;
                    this.focus.focus(window);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let modifiers = event.keystroke.modifiers;
                if key == "tab"
                    && this.media_hovered
                    && this.page_focus.len() > 1
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.platform
                {
                    this.keyboard_focus = true;
                    if modifiers.shift {
                        window.focus_prev();
                    } else {
                        window.focus_next();
                    }
                    if let Some(index) = this
                        .page_focus
                        .iter()
                        .position(|focus| focus.is_focused(window))
                    {
                        this.indicator_scroll.scroll_to_item(index);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }
                if key == "a"
                    && modifiers.control
                    && !modifiers.alt
                    && !modifiers.platform
                    && !modifiers.shift
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
                if key == "space" && matches!(this.record.kind.as_str(), "image" | "video") {
                    window.prevent_default();
                    cx.stop_propagation();
                    if !event.is_held {
                        this.open_large.set(true);
                    }
                }
            }))
            .relative()
            .w_full()
            .p(px(18.))
            .border_1()
            .border_color(p.line)
            .rounded(px(8.))
            .bg(p.surface)
            .text_color(p.text)
            .when(self.record.kind == "text", |card| {
                card.flex()
                    .flex_col()
                    .max_h(px(self.anchor.placement.logical_max_height() as f32))
            });
        if !self.record.title.is_empty() {
            card = card.child(
                div()
                    .flex_shrink_0()
                    .mb(px(12.))
                    .text_size(px(13. * scale))
                    .line_height(px(19.5 * scale))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(highlight::text(self.record.title.clone(), &self.search, p)),
            );
        }
        if matches!(self.record.kind.as_str(), "text" | "link") {
            let body = div()
                .id("preview-text")
                .p(px(12.))
                .rounded(px(7.))
                .bg(p.control)
                .text_size(px(12.5 * scale))
                .line_height(px(20. * scale))
                // Only .text-card gets a bounded inner scroller in preview.css.
                // Links expand naturally and scroll with the outer card.
                .when(self.record.kind == "text", |body| {
                    body.track_scroll(&self.body_scroll)
                        .min_h_0()
                        .max_h(px(320.))
                        .overflow_y_scroll()
                })
                .child(self.body_text.clone());
            card = if self.record.kind == "text" {
                card.child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .max_h(px(320.))
                        .child(body)
                        .child(
                            div().absolute().inset_0().child(
                                gpui_component::scroll::Scrollbar::vertical(&self.body_scroll)
                                    .id("preview-text-scrollbar")
                                    .thin(px(7.))
                                    .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                            ),
                        ),
                )
            } else {
                card.child(body)
            };
        }
        if self.record.kind == "color" {
            let color = gpui::Rgba::try_from(body.trim()).unwrap_or(p.surface);
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .pt(px(12.))
                    .pb(px(20.))
                    .text_size(px(13. * scale))
                    .line_height(px(20.8 * scale))
                    .child(
                        div()
                            .relative()
                            .size(px(112.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(color)
                            .child(
                                gpui::canvas(
                                    |_, _, _| (),
                                    |bounds, _, window, _| {
                                        // CSS outline is outside the 112px disc; a border
                                        // would reduce the fill or change its layout size.
                                        let outline = gpui::Rgba {
                                            a: 0.12,
                                            ..gpui::rgb(0x171c28)
                                        };
                                        window.paint_quad(
                                            gpui::outline(
                                                bounds.dilate(px(1.)),
                                                outline,
                                                gpui::BorderStyle::Solid,
                                            )
                                            .corner_radii(px(57.))
                                            .border_widths(px(1.)),
                                        );
                                    },
                                )
                                .absolute()
                                .inset_0()
                                .size_full(),
                            ),
                    )
                    .child(self.body_text.clone()),
            );
        }
        if matches!(self.record.kind.as_str(), "image" | "video") {
            let multiple = self.record.files.len() > 1;
            let mut media = div()
                .id("hover-media")
                .relative()
                .min_h(px(120.))
                .h(px(if multiple {
                    320.
                } else {
                    (self.image_size.1 + 16.).max(120.)
                }))
                .mb(px(14.))
                .p(px(8.))
                .rounded(px(7.))
                .bg(p.control)
                .flex()
                .items_center()
                .justify_center();
            let slide = |image: Option<Arc<gpui::Image>>, size: (f32, f32), error: bool| {
                let mut content = div().flex().items_center().justify_center().size_full();
                if let Some(image) = image {
                    content = content.child(
                        gpui::img(image)
                            .w(px(size.0))
                            .h(px(size.1))
                            .flex_shrink_0()
                            .object_fit(gpui::ObjectFit::Contain),
                    );
                } else if error {
                    content = content.child(
                        div()
                            .p(px(20.))
                            .text_center()
                            .text_color(p.subtle)
                            .text_size(px(12. * scale))
                            .child(if self.record.kind == "video" {
                                choose("首帧预览不可用", "First-frame preview unavailable")
                            } else {
                                choose("预览图不可用", "Preview unavailable")
                            }),
                    );
                }
                content
            };
            if let Some(transition) = self.media.running() {
                let width = (self.anchor.placement.logical_width() as f32 - 38.).max(1.);
                let (outgoing, incoming) = crate::media_transition::offsets(
                    transition.started.elapsed(),
                    transition.index > self.media.current(),
                );
                let (next_image, next_size) = match &transition.incoming {
                    Ok(next) => (
                        Some(next.image.clone()),
                        preview_image_size(
                            next.width,
                            next.height,
                            self.anchor.placement.logical_width() as f32,
                            true,
                        ),
                    ),
                    Err(_) => (None, (0., 0.)),
                };
                media = media
                    .overflow_hidden()
                    .child(
                        slide(self.image.clone(), self.image_size, self.media_error)
                            .absolute()
                            .top_0()
                            .left(px(width * outgoing))
                            .w(px(width)),
                    )
                    .child(
                        slide(next_image, next_size, transition.incoming.is_err())
                            .absolute()
                            .top_0()
                            .left(px(width * incoming))
                            .w(px(width)),
                    );
                window.request_animation_frame();
            } else {
                media = media.child(slide(self.image.clone(), self.image_size, self.media_error));
            }
            if multiple {
                media = media
                    .p(px(0.))
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        this.media_hovered = *hovered;
                        this.reveal_indicator = *hovered;
                        cx.notify();
                    }))
                    .on_scroll_wheel(cx.listener(
                        |this, event: &gpui::ScrollWheelEvent, window, cx| {
                            let delta = f32::from(event.delta.pixel_delta(px(20.)).y);
                            if delta == 0. {
                                return;
                            }
                            cx.stop_propagation();
                            let now = std::time::Instant::now();
                            if this
                                .last_wheel
                                .is_some_and(|last| now.duration_since(last).as_millis() < 180)
                            {
                                return;
                            }
                            this.last_wheel = Some(now);
                            let index = if delta < 0. {
                                (this.media.desired() + 1).min(this.record.files.len() - 1)
                            } else {
                                this.media.desired().saturating_sub(1)
                            };
                            this.select_media(index, window, cx);
                        },
                    ));
                if self.media_hovered {
                    let mut indicators = div()
                        .id("media-pages")
                        .flex()
                        .items_center()
                        .h(px(22.))
                        .px(px(3.))
                        .border_1()
                        .border_color(gpui::Rgba {
                            a: 0.1,
                            ..gpui::rgb(0x64748B)
                        })
                        .rounded_full()
                        .bg(p.indicator_bg)
                        .max_w_full()
                        .track_scroll(&self.indicator_scroll)
                        .overflow_x_scroll();
                    for index in 0..self.record.files.len() {
                        let active = index == self.media.current();
                        indicators = indicators.child(
                            div()
                                .id(("media-page", index))
                                .group("media-page-button")
                                .track_focus(&self.page_focus[index])
                                .relative()
                                .size(px(18.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .rounded_full()
                                .child(
                                    div()
                                        .w(px(if active { 14. } else { 6. }))
                                        .h(px(6.))
                                        .rounded_full()
                                        .bg(if active {
                                            p.indicator_active
                                        } else {
                                            p.indicator_dot
                                        })
                                        .group_hover("media-page-button", |dot| {
                                            dot.bg(p.indicator_active)
                                        }),
                                )
                                .when(
                                    self.keyboard_focus
                                        && self.page_focus[index].is_focused(window),
                                    |button| {
                                        button.child(
                                            div()
                                                .absolute()
                                                .inset_0()
                                                .rounded_full()
                                                .border_2()
                                                .border_color(p.indicator_active),
                                        )
                                    },
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.keyboard_focus = false;
                                        this.page_focus[index].focus(window);
                                        cx.notify();
                                    }),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.select_media(index, window, cx);
                                })),
                        );
                    }
                    if self.reveal_indicator {
                        self.reveal_indicator = false;
                        let scroll = self.indicator_scroll.clone();
                        let index = self.media.current();
                        let count = self.record.files.len();
                        // Run after layout has populated the scroll viewport.
                        indicators = indicators.child(
                            gpui::canvas(
                                move |_, window, cx| {
                                    let scroll = scroll.clone();
                                    window.defer(cx, move |window, _| {
                                        let viewport =
                                            (f32::from(scroll.bounds().size.width) - 2.).max(1.);
                                        let offset = scroll.offset();
                                        let left = indicator_scroll_left(
                                            index,
                                            count,
                                            viewport,
                                            -f32::from(offset.x),
                                        );
                                        if (left + f32::from(offset.x)).abs() > 0.01 {
                                            scroll.set_offset(gpui::point(px(-left), offset.y));
                                            window.refresh();
                                        }
                                    });
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_0(),
                        );
                    }
                    media = media.child(
                        div()
                            .absolute()
                            .bottom(px(12.))
                            .left(px(8.))
                            .right(px(8.))
                            .flex()
                            .justify_center()
                            .child(indicators),
                    );
                }
            }
            card = card.child(media);
        }
        if let Some(group) = &self.group {
            card = card.child(
                div()
                    .mt(px(14.))
                    .flex_shrink_0()
                    .flex()
                    .gap(px(10.))
                    .text_size(px(12. * scale))
                    .line_height(px(18. * scale))
                    .child(
                        div()
                            .w(px(64.))
                            .flex_shrink_0()
                            .text_color(p.subtle)
                            .child(choose("分组", "Group")),
                    )
                    .child(div().flex_1().min_w_0().child(group.clone())),
            );
        }
        if !self.file_paths.is_empty() {
            let multiple = self.file_paths.len() > 1;
            let mut files = div()
                .py(px(10.))
                .px(px(12.))
                .rounded(px(7.))
                .bg(p.control)
                .text_size(px(12. * scale))
                .line_height(px(18. * scale));
            for (i, path) in self.file_paths.iter().enumerate() {
                files = files.child(
                    div()
                        .when(i > 0, |d| d.mt(px(10.)))
                        .flex()
                        .items_start()
                        .when(multiple, |d| {
                            d.child(
                                div()
                                    .w(px(17.))
                                    .flex_shrink_0()
                                    .text_color(p.muted)
                                    .child("•"),
                            )
                        })
                        .child(div().flex_1().min_w_0().child(path.clone())),
                );
            }
            if let Some(text) = &self.file_text {
                files = files.child(
                    div()
                        .relative()
                        .mt(px(10.))
                        .child(
                            div()
                                .id("file-text")
                                .when_some(self.file_text_font.clone(), |block, family| {
                                    block.font_family(family)
                                })
                                .p(px(10.))
                                .max_h(px(320.))
                                .overflow_y_scroll()
                                .track_scroll(&self.file_scroll)
                                .rounded(px(6.))
                                .bg(p.surface)
                                .child(text.clone()),
                        )
                        .child(
                            div().absolute().inset_0().child(
                                gpui_component::scroll::Scrollbar::vertical(&self.file_scroll)
                                    .id("file-text-scrollbar")
                                    .thin(px(7.))
                                    .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                            ),
                        ),
                );
            }
            if let Some(error) = &self.file_error {
                files = files.child(div().mt(px(8.)).text_color(p.subtle).child(format!(
                    "{}{}",
                    choose("文本预览不可用：", "Text preview unavailable: "),
                    error
                )));
            }
            card = card.child(files);
        }
        let status = match self.record.status.as_str() {
            "missing" => choose("文件已不可用", "File unavailable"),
            "copy_failed" => choose("副本保存失败", "Could not save copy"),
            _ => "",
        };
        if !status.is_empty() {
            card = card.child(
                div()
                    .mt(px(12.))
                    .text_size(px(12. * scale))
                    .line_height(px(18. * scale))
                    .text_color(p.warning)
                    .child(status),
            );
        }
        let anchor = self.anchor.clone();
        let measured = self.measured.clone();
        let lifetime = self.lifetime.clone();
        let ready = !matches!(self.record.kind.as_str(), "image" | "video")
            || self.image.is_some()
            || self.media_error;
        // Measure the visible card outside its scroll content. Absolute probes
        // inside a padded scroller contribute to GPUI's content extent.
        let mut card = div()
            .id("floating-viewport")
            .relative()
            .w_full()
            .child(
                div()
                    .id("preview-app")
                    .max_h(px(self.anchor.placement.logical_max_height() as f32))
                    .overflow_y_scroll()
                    .track_scroll(&self.outer_scroll)
                    .child(card),
            )
            .child(
                div().absolute().inset_0().child(
                    gpui_component::scroll::Scrollbar::vertical(&self.outer_scroll)
                        .id("preview-outer-scrollbar")
                        .thin(px(7.))
                        .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                ),
            );
        card = card.child(
            gpui::canvas(
                move |bounds, window, cx| {
                    let height = f32::from(bounds.size.height);
                    if std::env::var_os("TP_DIAGNOSTIC_HOVER").is_some() {
                        eprintln!("floating: measured {height}, ready={ready}");
                    }
                    if !lifetime.is_closed()
                        && ready
                        && height > 0.
                        && measured.replace(height) != height
                    {
                        let lifetime = lifetime.clone();
                        let ticket = lifetime.queue_layout();
                        let anchor = anchor.clone();
                        let native = raw_window_handle::HasWindowHandle::window_handle(window)
                            .map(|handle| handle.as_raw());
                        let handle = window.window_handle();
                        // Native resize synchronously calls GPUI's bounds callback. Run
                        // without borrowing the window, so its viewport can update too.
                        cx.spawn(async move |cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(1))
                                .await;
                            if !lifetime.allows_layout(ticket) {
                                return;
                            }
                            let Ok(native) = native else {
                                return;
                            };
                            if handle.update(cx, |_, _, _| ()).is_err() {
                                return;
                            }
                            let result = anchor.reveal(native, height as f64);
                            if std::env::var_os("TP_DIAGNOSTIC_HOVER").is_some() {
                                eprintln!("floating: reveal {result:?}");
                            }
                            let _ = handle.update(cx, |_, window, _| {
                                let _ = crate::native_window::set_taskbar_icon(window, false);
                                window.refresh();
                            });
                        })
                        .detach();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .size_full(),
        );
        card = card.on_hover(cx.listener(|this, hovered, window, cx| {
            this.inside.set(*hovered);
            this.lifetime.cancel_leave();
            if !hovered {
                let inside = this.inside.clone();
                let record = this.over_record.clone();
                let lifetime = this.lifetime.clone();
                let ticket = lifetime.queue_leave();
                cx.spawn_in(window, async move |_, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(220))
                        .await;
                    if lifetime.allows_leave(ticket) && !inside.get() && !record.get() {
                        lifetime.close();
                        let _ = cx.update(|window, _| crate::native_window::hide(window));
                    }
                })
                .detach();
            }
        }));
        theme::typography(card, &self.settings)
    }
}
