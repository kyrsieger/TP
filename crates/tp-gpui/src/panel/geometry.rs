use super::*;
use std::time::{Duration, Instant};

impl Panel {
    pub(super) fn place_window(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.geometry_save = None;
        self.geometry_dirty = false;
        self.placing_until = Instant::now() + Duration::from_millis(350);
        if let Err(error) = crate::native_window::place(window, &self.snapshot.settings) {
            self.show_toast(error, cx);
        }
    }

    pub(super) fn bounds_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Windows can notify unchanged bounds during activation / z-order changes.
        // Closing the hover window must not cancel the large preview it just opened.
        let geometry = (window.bounds(), window.scale_factor());
        if self.observed_geometry == geometry {
            return;
        }
        self.observed_geometry = geometry;
        self.preview.hide();
        self.floating_owner_changed(window, cx);
        if self.hidden || Instant::now() < self.placing_until {
            return;
        }
        self.geometry_dirty = true;
        self.geometry_save = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = this.update_in(cx, |this, window, _| this.persist_geometry(window));
        }));
    }

    pub(super) fn persist_geometry(&mut self, window: &Window) {
        if !self.geometry_dirty {
            return;
        }
        self.geometry_dirty = false;
        let Ok(value) = crate::native_window::geometry(window) else {
            return;
        };
        // Merge geometry under the store lock; never write a stale settings-page snapshot.
        let result = (|| -> Result<(), String> {
            let _data = self.store.read_data()?;
            let conn = self.store.db.lock().map_err(|e| e.to_string())?;
            let geometry = (
                value.panel_x,
                value.panel_y,
                value.panel_width,
                value.panel_height,
            );
            tp_core::preferences::persist_panel_geometry(&conn, value)?;
            self.snapshot.settings.panel_x = geometry.0;
            self.snapshot.settings.panel_y = geometry.1;
            self.snapshot.settings.panel_width = geometry.2;
            self.snapshot.settings.panel_height = geometry.3;
            Ok(())
        })();
        // Retain last valid dimensions on transient native/validation errors, as the original does.
        if let Err(error) = result {
            eprintln!("Failed to save panel geometry: {error}");
        }
    }
}
