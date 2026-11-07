//! Floating-preview placement ported from services/previews.ts.
//! All coordinates are physical pixels; content measurement is in logical pixels.
use crate::panel_geometry::WorkArea;
use dpi::{PhysicalPosition, PhysicalSize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub row_y: f64,
    pub width: f64,
    scale: f64,
    top: f64,
    bottom: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Placement {
    pub fn logical_width(self) -> f64 {
        self.width / self.scale
    }
    pub fn logical_max_height(self) -> f64 {
        (self.bottom - self.top - 2. * (12. * self.scale).round()) / self.scale
    }
    /// Called after the card and any media have finished measuring.
    pub fn measured(self, logical_height: f64) -> Bounds {
        let margin = (12. * self.scale).round();
        let height = (logical_height * self.scale)
            .ceil()
            .min(self.bottom - self.top - 2. * margin)
            .max(1.);
        let y = self
            .row_y
            .min(self.bottom - margin - height)
            .max(self.top + margin);
        Bounds {
            x: self.x,
            y,
            width: self.width,
            height,
        }
    }
}

pub fn place(
    area: &WorkArea,
    panel_position: PhysicalPosition<i32>,
    panel_size: PhysicalSize<u32>,
    scale: f64,
    logical_row_top: f64,
) -> Placement {
    let margin = (12. * scale).round();
    let gap = (2. * scale).round();
    let left = f64::from(area.position.x) + margin;
    let right = f64::from(area.position.x) + f64::from(area.size.width) - margin;
    let desired = (420. * scale).round();
    let minimum = (260. * scale).round();
    let panel_x = f64::from(panel_position.x);
    let right_space = right - panel_x - f64::from(panel_size.width) - gap;
    let left_space = panel_x - gap - left;
    let use_right = right_space >= desired || (left_space < desired && right_space >= left_space);
    let available = if use_right { right_space } else { left_space };
    let width = desired
        .min(if available >= minimum {
            available
        } else {
            right - left
        })
        .max(1.);
    let anchor = if use_right {
        panel_x + f64::from(panel_size.width) + gap
    } else {
        panel_x - gap - width
    };
    Placement {
        x: anchor.min(right - width).max(left),
        row_y: f64::from(panel_position.y) + (logical_row_top * scale).round(),
        width,
        scale,
        top: f64::from(area.position.y),
        bottom: f64::from(area.position.y) + f64::from(area.size.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area(x: i32, y: i32, width: u32, height: u32) -> WorkArea {
        WorkArea {
            position: PhysicalPosition::new(x, y),
            size: PhysicalSize::new(width, height),
        }
    }
    #[test]
    fn prefers_right_when_both_sides_fit_and_left_at_right_edge() {
        let monitor = area(0, 0, 1920, 1080);
        let right = place(
            &monitor,
            PhysicalPosition::new(700, 100),
            PhysicalSize::new(420, 600),
            1.,
            80.,
        );
        assert_eq!(
            right.measured(200.),
            Bounds {
                x: 1122.,
                y: 180.,
                width: 420.,
                height: 200.
            }
        );
        let left = place(
            &monitor,
            PhysicalPosition::new(1482, 100),
            PhysicalSize::new(420, 600),
            1.,
            80.,
        );
        assert_eq!(left.x, 1060.);
    }
    #[test]
    fn shrinks_to_usable_side_but_overlaps_panel_if_neither_side_is_wide_enough() {
        let monitor = area(0, 0, 1200, 800);
        let narrow = place(
            &monitor,
            PhysicalPosition::new(390, 50),
            PhysicalSize::new(420, 600),
            1.,
            100.,
        );
        assert_eq!((narrow.x, narrow.width), (812., 376.));
        let fallback = place(
            &area(0, 0, 700, 800),
            PhysicalPosition::new(140, 50),
            PhysicalSize::new(420, 600),
            1.,
            100.,
        );
        assert_eq!((fallback.x, fallback.width), (268., 420.));
        let tiny = place(
            &area(0, 0, 300, 600),
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(300, 600),
            1.,
            0.,
        );
        assert_eq!((tiny.x, tiny.width), (12., 276.));
    }
    #[test]
    fn negative_monitor_coordinates_and_scaling_keep_logical_content_size() {
        let placement = place(
            &area(-2560, -200, 2560, 1400),
            PhysicalPosition::new(-680, 0),
            PhysicalSize::new(630, 900),
            1.5,
            100.,
        );
        assert_eq!(placement.x, -1313.);
        assert_eq!(placement.logical_width(), 420.);
        assert_eq!(placement.logical_max_height(), 1364. / 1.5);
        assert_eq!(
            placement.measured(200.2),
            Bounds {
                x: -1313.,
                y: 150.,
                width: 630.,
                height: 301.
            }
        );
    }
    #[test]
    fn measured_height_is_ceil_scaled_then_clamped_and_row_anchor_stays_in_work_area() {
        let monitor = area(0, 40, 1920, 1000);
        let placement = place(
            &monitor,
            PhysicalPosition::new(100, 700),
            PhysicalSize::new(420, 600),
            1.,
            500.,
        );
        assert_eq!(placement.measured(200.).y, 828.);
        let tall = placement.measured(2000.);
        assert_eq!((tall.y, tall.height), (52., 976.));
        let above = place(
            &monitor,
            PhysicalPosition::new(100, -400),
            PhysicalSize::new(420, 600),
            1.,
            0.,
        );
        assert_eq!(above.measured(50.).y, 52.);
    }
}
