use super::*;
use gpui::{img, Image, ImageFormat, ObjectFit, RenderImage};

pub(super) enum Thumbnail {
    Loading { request: u64, retry_if_failed: bool },
    Ready(Arc<RenderImage>),
    Failed,
}

impl Thumbnail {
    pub(super) fn release_image(&self, window: &mut Window) {
        if let Self::Ready(image) = self {
            let _ = window.drop_image(image.clone());
        }
    }
}

pub(super) fn has_preview(record: &tp_core::records::Record) -> bool {
    record.preview
        || (record.kind == "image" && !record.files.is_empty() && record.status != "pending")
}

pub(super) fn record_note(record: &tp_core::records::Record) -> &'static str {
    match record.status.as_str() {
        "pending" => choose("保存副本中", "Saving copy"),
        "missing" => choose("文件已不可用", "File unavailable"),
        "copy_failed" => choose("副本保存失败", "Could not save copy"),
        _ if record.expires_at.is_some() => choose("敏感记录", "Sensitive record"),
        _ => "",
    }
}
impl Panel {
    pub(super) fn clear_thumbnails(&mut self, window: &mut Window) {
        for (_, thumbnail) in self.thumbnails.drain() {
            thumbnail.release_image(window);
        }
    }

    pub(super) fn retry_failed_thumbnails(&mut self) {
        self.thumbnails.retain(|_, thumbnail| match thumbnail {
            Thumbnail::Loading {
                retry_if_failed, ..
            } => {
                // The source may have become available while this request was running.
                *retry_if_failed = true;
                true
            }
            Thumbnail::Ready(_) => true,
            Thumbnail::Failed => false,
        });
    }

    pub(super) fn media_content(
        &mut self,
        record: &tp_core::records::Record,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let id = &record.id;
        let has_preview = has_preview(record);
        if has_preview && !self.thumbnails.contains_key(id) {
            self.thumbnail_request = self.thumbnail_request.wrapping_add(1);
            let request = self.thumbnail_request;
            self.thumbnails.insert(
                id.clone(),
                Thumbnail::Loading {
                    request,
                    retry_if_failed: false,
                },
            );
            let store = self.store.clone();
            let id = id.clone();
            let renderer = cx.svg_renderer();
            cx.spawn(async move |panel, cx| {
                let request_id = id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let bytes = tp_desktop::media::preview_png(
                            &store,
                            request_id,
                            Some("thumbnail".into()),
                            None,
                        )?;
                        // Own the decoded image so identical bytes in another row or
                        // in a hover preview cannot evict this row's image.
                        Image::from_bytes(ImageFormat::Png, bytes)
                            .to_image_data(renderer)
                            .map_err(|error| error.to_string())
                    })
                    .await;
                let _ = panel.update(cx, |panel, cx| {
                    let Some(Thumbnail::Loading {
                        request: current,
                        retry_if_failed,
                    }) = panel.thumbnails.get(&id)
                    else {
                        return;
                    };
                    // A row may have left the mounted range and started a newer request.
                    if *current != request {
                        return;
                    }
                    match result {
                        Ok(image) => {
                            panel.thumbnails.insert(id, Thumbnail::Ready(image));
                        }
                        Err(_) if *retry_if_failed => {
                            panel.thumbnails.remove(&id);
                        }
                        Err(_) => {
                            panel.thumbnails.insert(id, Thumbnail::Failed);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let p = Palette::for_id(&self.snapshot.settings.theme);
        let font_scale = self.snapshot.settings.font_size as f32 / 13.;
        let unavailable = if record.kind == "video" {
            choose("首帧预览不可用", "First-frame preview unavailable")
        } else {
            choose("图片预览不可用", "Image preview unavailable")
        };
        let preview = match self.thumbnails.get(id) {
            Some(Thumbnail::Ready(image)) => img(image.clone())
                .w_full()
                .h_full()
                .max_h(px(160.))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            state => div()
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.subtle)
                .text_size(px(12. * font_scale))
                .child(if matches!(state, Some(Thumbnail::Loading { .. })) {
                    choose("加载中", "Loading")
                } else if matches!(state, Some(Thumbnail::Failed)) {
                    choose("预览图不可用", "Preview unavailable")
                } else {
                    unavailable
                })
                .into_any_element(),
        };
        let show_play = record.kind == "video"
            && has_preview
            && !matches!(self.thumbnails.get(id), Some(Thumbnail::Failed));
        let hovered = self.hovered_record.as_deref() == Some(id.as_str());
        let note = record_note(record);
        let query = self.session.search.to_lowercase();
        let body_match = !query.is_empty()
            && std::iter::once(record.body.as_str())
                .chain(record.files.iter().map(|f| f.original.as_str()))
                .any(|text| text.to_lowercase().contains(&query));
        let editing = self
            .record_edit
            .as_ref()
            .is_some_and(|edit| edit.id == record.id);
        let overlay = gpui::Rgba {
            a: 0.9,
            ..p.surface
        };
        div()
            .absolute()
            .left(px(28.))
            .right(px(28.))
            .top_0()
            .bottom_0()
            .min_w_0()
            .when(editing, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(-18.))
                        .right(px(-18.))
                        .top(px(5.))
                        .child(self.record_input(&record.id, window, cx).unwrap()),
                )
            })
            .when(!editing && !record.title.is_empty(), |d| {
                d.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "media-title:{}",
                            record.id
                        )))
                        .tooltip(record_display::full_text_tooltip(record.title.clone()))
                        .absolute()
                        .top(px(10.))
                        .left_0()
                        .right(px(if hovered {
                            104.
                        } else {
                            actions::record_indicator_width(record, p)
                        }))
                        .text_size(px(12.5 * font_scale))
                        .line_height(px(17. * font_scale))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(if self.selection.ids.contains(&record.id) {
                            p.selected
                        } else {
                            p.text
                        })
                        .truncate()
                        .child(crate::highlight::text(
                            record.title.clone(),
                            &self.session.search,
                            p,
                        )),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(if editing || !record.title.is_empty() {
                        42.
                    } else {
                        10.
                    }))
                    .bottom(px(10.))
                    .left_0()
                    .right_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .rounded(px(6.))
                    .child(preview)
                    .when(show_play, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(gpui::relative(0.5))
                                .left(gpui::relative(0.5))
                                .mt(px(-10.))
                                .ml(px(-10.))
                                .size(px(20.))
                                .rounded_full()
                                .bg(gpui::rgba(0x171c28bf))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    crate::icons::icon("play-fill", 10.)
                                        .text_color(gpui::rgb(0xffffff)),
                                ),
                        )
                    }),
            )
            .when(!editing && body_match, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(if note.is_empty() { 9. } else { 32. }))
                        .px(px(5.))
                        .py(px(3.))
                        .bg(overlay)
                        .rounded(px(4.))
                        .text_color(p.text)
                        .text_size(px(12. * font_scale))
                        .child(crate::highlight::excerpt(
                            tp_core::browse::record_list_text(
                                record,
                                &self.session.search,
                                self.snapshot.settings.remove_trailing_newlines,
                            ),
                            self.session.search.clone(),
                            p,
                            12. * font_scale,
                            17. * font_scale,
                        )),
                )
            })
            .when(!editing && !note.is_empty(), |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .bottom(px(9.))
                        .px(px(5.))
                        .py(px(3.))
                        .rounded(px(4.))
                        .bg(overlay)
                        .text_size(px(10. * font_scale))
                        .text_color(p.muted)
                        .child(note),
                )
            })
            .into_any_element()
    }
}
