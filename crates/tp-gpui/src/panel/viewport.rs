use gpui::{point, px, Bounds, Pixels, Point, ScrollHandle};
use std::time::Instant;
use std::{cell::RefCell, ops::Range, rc::Rc};
use tp_core::list_layout::ListLayout;

pub(super) const LIST_TOP_PADDING: f32 = 2.;
pub(super) const LIST_BOTTOM_PADDING: f32 = 6.;
const LIST_VERTICAL_PADDING: f32 = LIST_TOP_PADDING + LIST_BOTTOM_PADDING;

fn clamped_top(top: f32, total: f32, height: f32) -> f32 {
    top.clamp(0., (total + LIST_VERTICAL_PADDING - height).max(0.))
}

struct ReturnAnimation {
    start: f32,
    started: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn first_open_starts_at_top_before_viewport_has_measured_bounds() {
        let viewport = Viewport::new();
        viewport.scroll_to_item(0);
        viewport.update(ListLayout::new(std::iter::repeat_n(("text", false), 33)));
        assert_eq!(viewport.top(), 0.);
        assert_eq!(viewport.top_item(), 0);
        assert!(!viewport.has_pending_reveal());
    }

    #[test]
    fn bottom_position_retains_padding_and_clamps_after_resize_or_removal() {
        assert_eq!(clamped_top(1256., 1728., 480.), 1256.);
        assert_eq!(clamped_top(2000., 1728., 480.), 1256.);
        assert_eq!(clamped_top(1256., 1728., 600.), 1136.);
        assert_eq!(clamped_top(1256., 96., 480.), 0.);
        assert_eq!(clamped_top(-10., 1728., 480.), 0.);
        let layout = ListLayout::new(std::iter::repeat_n(("text", false), 36));
        assert_eq!(layout.reveal(35, 0., 480. - LIST_VERTICAL_PADDING), 1256.);
    }

    #[test]
    fn return_curve_matches_original_duration_and_quadratic_easing() {
        let now = Instant::now();
        // 100 px => 320 + sqrt(100) * 12 = 440 ms.
        let animation = ReturnAnimation {
            start: 100.,
            started: now,
        };
        for (ms, expected) in [(0, 100.), (110, 87.5), (220, 50.), (330, 12.5), (440, 0.)] {
            let (actual, running) = animation.position(now + Duration::from_millis(ms));
            assert!((actual - expected).abs() < 0.001);
            assert_eq!(running, ms < 440);
        }
        let far = ReturnAnimation {
            start: 10000.,
            started: now,
        };
        assert_eq!(far.position(now + Duration::from_millis(700)), (0., false));
    }

    #[test]
    fn interrupted_return_cannot_overwrite_manual_or_navigation_position() {
        let viewport = Viewport::new();
        let now = Instant::now();
        viewport.handle.set_offset(point(px(0.), px(-1000.)));
        viewport.return_to_top(false, now);
        assert!(viewport.advance_return(now + Duration::from_millis(200)));
        viewport.cancel_return();
        viewport.handle.set_offset(point(px(0.), px(-600.)));
        assert!(!viewport.advance_return(now + Duration::from_secs(1)));
        assert_eq!(viewport.top(), 600.);
        viewport.return_to_top(false, now);
        viewport.scroll_to_item(3);
        assert!(!viewport.advance_return(now + Duration::from_secs(1)));
        viewport.return_to_top(true, now);
        assert_eq!(viewport.top(), 0.);
        assert!(viewport.pending.borrow().is_none());
        assert!(!viewport.advance_return(now));
    }
}
impl ReturnAnimation {
    fn position(&self, now: Instant) -> (f32, bool) {
        let duration = (320. + self.start.sqrt() * 12.).min(700.);
        let progress = (now.duration_since(self.started).as_secs_f32() * 1000. / duration).min(1.);
        let eased = if progress < 0.5 {
            2. * progress * progress
        } else {
            1. - 2. * (1. - progress) * (1. - progress)
        };
        (self.start * (1. - eased), progress < 1.)
    }
}

#[derive(Clone, Default)]
pub(super) struct Viewport {
    pub handle: ScrollHandle,
    layout: Rc<RefCell<ListLayout>>,
    pending: Rc<RefCell<Option<usize>>>,
    returning: Rc<RefCell<Option<ReturnAnimation>>>,
}
impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&self, layout: ListLayout) {
        *self.layout.borrow_mut() = layout;
        let height = self.height();
        let mut top = self.top();
        if let Some(index) = self.pending.borrow_mut().take() {
            // Before the first paint the viewport has no measured height.
            // Revealing row zero in a zero-height area aligns its bottom and
            // clips the first row. A reset to the first row always means top.
            top = if index == 0 {
                0.
            } else {
                self.layout
                    .borrow()
                    .reveal(index, top, (height - LIST_VERTICAL_PADDING).max(0.))
            };
        }
        top = clamped_top(top, self.layout.borrow().total(), height);
        self.handle.set_offset(point(px(0.), px(-top)));
        if cfg!(debug_assertions) && std::env::var_os("TP_DIAGNOSTIC_LAYOUT").is_some() {
            eprintln!(
                "list layout: top={top} height={height} total={} mounted={:?}",
                self.layout.borrow().count(),
                self.range()
            );
        }
    }
    fn height(&self) -> f32 {
        f32::from(self.handle.bounds().size.height).max(1.)
    }
    fn top(&self) -> f32 {
        (-f32::from(self.handle.offset().y)).max(0.)
    }
    pub fn show_back_to_top(&self) -> bool {
        self.layout
            .borrow()
            .back_to_top_offset((self.height() - LIST_VERTICAL_PADDING).max(0.))
            .is_some_and(|threshold| threshold + LIST_TOP_PADDING - self.top() < self.height())
    }
    pub fn range(&self) -> Range<usize> {
        self.layout.borrow().visible(
            (self.top() - LIST_TOP_PADDING).max(0.),
            (self.height() - LIST_VERTICAL_PADDING).max(0.),
            180.,
        )
    }
    pub fn top_item(&self) -> usize {
        // RecordList.visibleRecords excludes one pixel at either viewport edge;
        // row offsets start after the two-pixel top padding.
        self.layout
            .borrow()
            .visible(
                self.top() + 1. - LIST_TOP_PADDING,
                (self.height() - 2.).max(0.),
                0.,
            )
            .start
    }
    pub fn bottom_item(&self) -> usize {
        self.layout
            .borrow()
            .visible(
                self.top() + 1. - LIST_TOP_PADDING,
                (self.height() - 2.).max(0.),
                0.,
            )
            .end
            .saturating_sub(1)
    }
    pub fn scroll_to_item(&self, index: usize) {
        self.cancel_return();
        *self.pending.borrow_mut() = Some(index);
    }
    pub fn has_pending_reveal(&self) -> bool {
        self.pending.borrow().is_some()
    }
    pub fn cancel_return(&self) {
        self.returning.borrow_mut().take();
    }
    pub fn return_to_top(&self, reduced_motion: bool, now: Instant) {
        self.cancel_return();
        self.pending.borrow_mut().take();
        if reduced_motion {
            self.handle.set_offset(point(px(0.), px(0.)));
        } else {
            *self.returning.borrow_mut() = Some(ReturnAnimation {
                start: self.top(),
                started: now,
            });
        }
    }
    pub fn advance_return(&self, now: Instant) -> bool {
        let mut returning = self.returning.borrow_mut();
        let Some(animation) = returning.as_ref() else {
            return false;
        };
        let (top, running) = animation.position(now);
        self.handle.set_offset(point(px(0.), px(-top)));
        if !running {
            returning.take();
        }
        running
    }
    pub fn offset(&self) -> Point<Pixels> {
        self.handle.offset()
    }
    // Wheel dispatch can expose an out-of-bounds offset until prepaint clamps
    // it. Compare only the position that can actually be displayed.
    pub fn displayed_offset(&self) -> Point<Pixels> {
        let offset = self.handle.offset();
        let max = self.handle.max_offset();
        point(
            offset.x.clamp(-max.width.max(px(0.)), px(0.)),
            offset.y.clamp(-max.height.max(px(0.)), px(0.)),
        )
    }
    pub fn bounds_for_item(&self, index: usize) -> Option<Bounds<Pixels>> {
        let layout = self.layout.borrow();
        if index >= layout.count() {
            return None;
        }
        let mut bounds = self.handle.bounds();
        bounds.origin.y += px(LIST_TOP_PADDING + layout.offsets[index]);
        bounds.size.height = px(layout.offsets[index + 1] - layout.offsets[index]);
        Some(bounds)
    }
}
