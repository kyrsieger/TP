use super::{Geometry, Rect};

/// AppKit starts a Window Server drag asynchronously and may omit mouse-up events.
#[derive(Default)]
pub(super) struct WindowDrag {
    active: bool,
}

impl WindowDrag {
    pub fn begin(&mut self, geometry: &mut Geometry) {
        geometry.move_to(geometry.x, geometry.y);
        self.active = true;
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn sync_frame(&self, geometry: &mut Geometry, visible: &mut Rect, actual: Rect) -> bool {
        if actual == *visible {
            return false;
        }
        // The native window contains only the visible crop. Preserve the full image's
        // offset within it instead of replacing its origin with the window origin.
        geometry.move_to(
            geometry.x + actual.x - visible.x,
            geometry.y + actual.y - visible.y,
        );
        *visible = actual;
        true
    }

    pub fn finish_if_released(&mut self, primary_button_pressed: bool) -> bool {
        if self.active && !primary_button_pressed {
            self.active = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(
            2400,
            1600,
            1.,
            Rect {
                x: 0.,
                y: 0.,
                w: 1200.,
                h: 800.,
            },
        )
    }

    #[test]
    fn asynchronous_moves_keep_the_full_image_offset_until_button_release() {
        let mut geometry = geometry();
        geometry.scale = 1.;
        geometry.move_to(-600., -400.);
        let mut visible = geometry.rect().intersect(geometry.work);
        let mut drag = WindowDrag::default();
        drag.begin(&mut geometry);

        // performWindowDragWithEvent returns before the first movement.
        let before = visible;
        assert!(!drag.sync_frame(&mut geometry, &mut visible, before));
        assert!(!drag.finish_if_released(true));
        assert!(drag.active());

        let first = Rect {
            x: 80.,
            y: -25.,
            ..visible
        };
        assert!(drag.sync_frame(&mut geometry, &mut visible, first));
        let second = Rect {
            x: 140.,
            y: -60.,
            ..visible
        };
        assert!(drag.sync_frame(&mut geometry, &mut visible, second));
        assert_eq!((geometry.x, geometry.y), (-460., -460.));
        assert_eq!(visible, second);

        // A timer can observe the released button even if AppKit sends no mouse-up.
        assert!(drag.finish_if_released(false));
        assert!(!drag.active());
        assert!(!drag.finish_if_released(false));
        assert_eq!(
            geometry.rect().intersect(geometry.work),
            Rect {
                x: 0.,
                y: 0.,
                w: 1200.,
                h: 800.
            }
        );
    }

    #[test]
    fn duplicate_notifications_do_not_move_the_image_or_cancel_later_zoom() {
        let mut geometry = geometry();
        let mut visible = geometry.rect();
        let drag = WindowDrag::default();
        let actual = Rect {
            x: visible.x + 100.,
            y: visible.y + 30.,
            ..visible
        };
        assert!(drag.sync_frame(&mut geometry, &mut visible, actual));
        geometry.zoom(1.25, (actual.x + 50., actual.y + 50.));
        let target = geometry.target;
        assert!(!drag.sync_frame(&mut geometry, &mut visible, actual));
        assert_eq!(geometry.target, target);
        assert_eq!((geometry.x, geometry.y), (actual.x, actual.y));
    }

    #[test]
    fn dragging_cancels_pending_zoom_and_accepts_a_move_to_another_screen() {
        let mut geometry = geometry();
        let mut visible = geometry.rect();
        geometry.zoom(2., (300., 200.));
        let mut drag = WindowDrag::default();
        drag.begin(&mut geometry);
        assert_eq!(geometry.target, geometry.scale);
        let actual = Rect {
            x: -1100.,
            y: 50.,
            ..visible
        };
        drag.sync_frame(&mut geometry, &mut visible, actual);
        geometry.work = Rect {
            x: -1200.,
            y: 0.,
            w: 1200.,
            h: 800.,
        };
        drag.finish_if_released(false);
        assert_eq!(geometry.rect(), actual);
        assert_eq!(geometry.rect().intersect(geometry.work), actual);
    }
}
