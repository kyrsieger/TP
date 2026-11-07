use super::*;

impl SettingsView {
    pub(crate) fn font_style_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.font_styles {
            return false;
        }
        let Some(picker) = self.font_picker.clone() else {
            return false;
        };
        let (count, focused) = picker.update(cx, |picker, cx| picker.style_navigation(window, cx));
        if count == 0 {
            return false;
        }
        let next = match (focused, reverse) {
            (Some(index), false) if index + 1 < count => Some(index + 1),
            (Some(index), true) if index > 0 => Some(index - 1),
            (None, false) if self.font_style_focus.is_focused(window) => Some(0),
            (None, true) if self.shortcut_focus.is_focused(window) => Some(count - 1),
            _ => None,
        };
        if let Some(next) = next {
            picker.update(cx, |picker, cx| picker.focus_style(next, window, cx));
        } else if focused.is_some() {
            if reverse {
                self.font_style_focus.focus(window);
            } else {
                self.shortcut_focus.focus(window);
            }
            picker.update(cx, |_, cx| cx.notify());
        } else {
            return false;
        }
        cx.notify();
        true
    }

    pub(crate) fn font_search_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .font_picker
            .as_ref()
            .is_some_and(|picker| picker.update(cx, |picker, cx| picker.search_focused(window, cx)))
        {
            return false;
        }
        self.close_font_search_for_tab(reverse, window, cx);
        true
    }

    pub(super) fn close_font_search_for_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.font_picker = None;
        self.font_subscription = None;
        if reverse {
            self.font_focus.focus(window);
        } else if self.has_font_styles() {
            self.font_style_focus.focus(window);
        } else {
            self.shortcut_focus.focus(window);
        }
        cx.notify();
    }

    pub(super) fn has_font_styles(&self) -> bool {
        self.font_catalog.as_ref().is_some_and(|fonts| {
            let (family, _) =
                tp_core::fonts::resolve(fonts, &self.value.font_family, &self.value.font_style);
            !family.is_empty()
                && fonts
                    .iter()
                    .any(|font| font.name == family && font.styles.len() > 1)
        })
    }

    pub(super) fn load_font_catalog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // NSFontManager belongs on the main thread; DirectWrite enumeration can
        // run on a worker. Also load while the menu is closed, as FontSetting does.
        #[cfg(target_os = "macos")]
        let result = tp_desktop::fonts::get_system_fonts();
        cx.spawn_in(window, async move |this, cx| {
            #[cfg(windows)]
            let result = cx
                .background_executor()
                .spawn(async { tp_desktop::fonts::get_system_fonts() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                // A picker opened in the meantime may already have refreshed the catalog.
                if this.font_catalog.is_none() {
                    if let Ok(fonts) = result {
                        this.font_catalog = Some(Arc::new(fonts));
                        this.reconcile_font(window, cx);
                        cx.notify();
                    }
                }
                // Opening the picker offers the original loading/error/retry UI.
            });
        })
        .detach();
    }

    pub(super) fn reconcile_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.value.font_family.is_empty() {
            return;
        }
        let Some(fonts) = &self.font_catalog else {
            return;
        };
        let original = (
            self.value.font_family.clone(),
            self.value.font_style.clone(),
        );
        let resolved = tp_core::fonts::resolve(fonts, &original.0, &original.1);
        if original != resolved && !self.font_migrations.contains(&original) {
            // Match the original component's once-per-selection migration guard,
            // including on save failure, so an unavailable face cannot retry forever.
            self.font_migrations.push(original);
            self.save(Patch::Font(resolved.0, resolved.1), None, window, cx);
        }
    }
}
