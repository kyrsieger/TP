use super::*;

// HTML number input syntax, before applying the settings' integer/range rules.
// https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#valid-floating-point-number
fn html_number(text: &str) -> Option<f64> {
    let mut bytes = text.as_bytes().iter().copied().peekable();
    if bytes.peek() == Some(&b'-') {
        bytes.next();
    }
    let mut digits = 0;
    while bytes.peek().is_some_and(u8::is_ascii_digit) {
        bytes.next();
        digits += 1;
    }
    if bytes.peek() == Some(&b'.') {
        bytes.next();
        let mut fraction = 0;
        while bytes.peek().is_some_and(u8::is_ascii_digit) {
            bytes.next();
            fraction += 1;
        }
        if fraction == 0 {
            return None;
        }
        digits += fraction;
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.peek(), Some(b'e' | b'E')) {
        bytes.next();
        if matches!(bytes.peek(), Some(b'+' | b'-')) {
            bytes.next();
        }
        let mut exponent = 0;
        while bytes.peek().is_some_and(u8::is_ascii_digit) {
            bytes.next();
            exponent += 1;
        }
        if exponent == 0 {
            return None;
        }
    }
    if bytes.next().is_some() {
        return None;
    }
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

pub(super) fn setting_integer(text: &str, index: usize) -> Option<u32> {
    let value = html_number(text)?;
    let (min, max) = limits(index);
    (value.fract() == 0. && value >= min && value <= max).then_some(value as u32)
}
fn limits(index: usize) -> (f64, f64) {
    if index == 0 {
        (10., 20.)
    } else {
        (1., u32::MAX as f64)
    }
}
// Unit step with an integer minimum, matching HTML stepUp/stepDown. In
// particular, a step must never move in the opposite direction to repair a bound.
fn stepped(text: &str, index: usize, up: bool) -> Option<u32> {
    let before = html_number(text).unwrap_or(0.);
    let next = if up {
        before.floor() + 1.
    } else {
        before.ceil() - 1.
    };
    let (min, max) = limits(index);
    let next = next.clamp(min, max);
    if (up && next < before) || (!up && next > before) {
        None
    } else {
        Some(next as u32)
    }
}

impl SettingsView {
    fn step_number(&mut self, index: usize, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.has_modal() {
            return;
        }
        if self.numbers[index].update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        let draft = self.numbers[index].read(cx).value();
        if let Some(next) = stepped(&draft, index, up) {
            self.numbers[index].update(cx, |input, cx| {
                input.set_value(next.to_string(), window, cx)
            });
            if index == 2 {
                self.schedule_sensitive_save(window, cx);
            }
        }
        cx.notify();
    }

    fn number_step_button(
        &self,
        index: usize,
        up: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = Palette::for_id(&self.value.theme);
        div()
            .id((if up { "number-up" } else { "number-down" }, index))
            .w(px(14.))
            .h(px(9.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(6.))
            .line_height(px(9.))
            .text_color(p.muted)
            .when(!disabled, |d| {
                d.hover(|d| d.bg(p.control).text_color(p.text))
            })
            .child(if up { "▲" } else { "▼" })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if disabled {
                        return;
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    use gpui::Focusable;
                    this.numbers[index].read(cx).focus_handle(cx).focus(window);
                    this.step_number(index, up, window, cx);
                    this.number_repeat = Some(cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(400))
                            .await;
                        loop {
                            if !this
                                .update_in(cx, |this, window, cx| {
                                    if !window.is_window_active()
                                        || !this.numbers[index]
                                            .read(cx)
                                            .focus_handle(cx)
                                            .is_focused(window)
                                        || this.has_modal()
                                    {
                                        return false;
                                    }
                                    this.step_number(index, up, window, cx);
                                    true
                                })
                                .unwrap_or(false)
                            {
                                break;
                            }
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(80))
                                .await;
                        }
                    }));
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.number_repeat = None;
                }),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.number_repeat = None;
                }),
            )
            .on_hover(cx.listener(|this, hovered, _, _| {
                if !hovered {
                    this.number_repeat = None;
                }
            }))
    }

    pub(super) fn number_control(
        &self,
        index: usize,
        disabled: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use gpui::Focusable;
        let p = Palette::for_id(&self.value.theme);
        let size = px(12. * self.value.font_size as f32 / 13.);
        let value = self.numbers[index].read(cx).value();
        let mut style = window.text_style();
        let mut features = style.font_features.tag_value_list().to_vec();
        features.retain(|(tag, _)| tag != "tnum");
        features.push(("tnum".into(), 1));
        style.font_features = gpui::FontFeatures(Arc::new(features));
        let measured = window
            .text_system()
            .shape_line(value.clone(), size, &[style.to_run(value.len())], None)
            .width;
        let width = if index == 0 {
            (measured + px(36.)).max(px(80.)).min(px(160.))
        } else {
            px(110.)
        };
        let focused = self.numbers[index]
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let mut input = Input::new(&self.numbers[index])
            .appearance(false)
            .disabled(disabled)
            .pl(((width - px(2.) - measured) / 2.).max(px(7.)))
            .pr(px(7.))
            .py_0()
            .text_size(size)
            .text_color(p.text);
        input
            .text_style()
            .get_or_insert_with(Default::default)
            .font_features = Some(style.font_features);
        div()
            .id(("setting-number", index))
            .relative()
            .w(width)
            .h(px(28.))
            .flex_shrink_0()
            .rounded(px(6.))
            .border_1()
            .border_color(if focused { p.accent } else { p.control_border })
            .bg(p.surface)
            .when(focused, |d| {
                d.shadow(vec![gpui::BoxShadow {
                    color: p.accent.into(),
                    offset: gpui::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                }])
            })
            .child(gpui::Styled::h(input, px(26.)))
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(5.))
                    .w(px(14.))
                    .h(px(18.))
                    .flex()
                    .flex_col()
                    .child(self.number_step_button(index, true, disabled, cx))
                    .child(self.number_step_button(index, false, disabled, cx)),
            )
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if disabled
                        || event.keystroke.modifiers.modified()
                        || !matches!(event.keystroke.key.as_str(), "up" | "down")
                    {
                        return;
                    }
                    if this.numbers[index].update(cx, |input, cx| {
                        input.marked_text_range(window, cx).is_some()
                    }) {
                        return;
                    }
                    this.step_number(index, event.keystroke.key == "up", window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_integer_decimal_and_exponent_forms_without_relaxing_bounds() {
        for input in ["14", "14.0", "1.4e1", "140E-1", "0014", ".14e+2"] {
            assert_eq!(setting_integer(input, 0), Some(14), "{input}");
        }
        for input in [
            "", " ", " 14", "+14", "14.", "1e", "0xE", "NaN", "inf", "14.5", "9", "21", "1e999",
            "１４",
        ] {
            assert_eq!(setting_integer(input, 0), None, "{input}");
        }
        assert_eq!(setting_integer("4294967295.0", 1), Some(u32::MAX));
        assert_eq!(setting_integer("4294967296", 1), None);
        assert_eq!(setting_integer("0", 2), None);
    }
    #[test]
    fn stepping_snaps_fraction_and_respects_direction_outside_bounds() {
        assert_eq!(stepped("14.2", 0, true), Some(15));
        assert_eq!(stepped("14.2", 0, false), Some(14));
        assert_eq!(stepped("1.4e1", 0, true), Some(15));
        assert_eq!(stepped("25", 0, true), None);
        assert_eq!(stepped("25", 0, false), Some(20));
        assert_eq!(stepped("9", 0, false), None);
        assert_eq!(stepped("9", 0, true), Some(10));
        assert_eq!(stepped("", 0, true), Some(10));
        assert_eq!(stepped("", 0, false), None);
        assert_eq!(stepped("20", 0, true), Some(20));
        assert_eq!(stepped("4294967295", 1, true), Some(u32::MAX));
    }
}
