use std::time::{Duration, Instant};

pub const DURATION: Duration = Duration::from_millis(240);

pub struct Running<T> {
    pub started: Instant,
    pub index: usize,
    pub incoming: T,
}

/// Mirrors the original carousel's current / desired / running lifecycle.
pub struct Carousel<T> {
    desired: usize,
    current: usize,
    ready: bool,
    running: Option<Running<T>>,
}
impl<T> Default for Carousel<T> {
    fn default() -> Self {
        Self {
            desired: 0,
            current: 0,
            ready: false,
            running: None,
        }
    }
}
impl<T> Carousel<T> {
    pub fn desired(&self) -> usize {
        self.desired
    }
    pub fn current(&self) -> usize {
        self.current
    }
    pub fn running(&self) -> Option<&Running<T>> {
        self.running.as_ref()
    }
    pub fn select(&mut self, index: usize, count: usize) -> bool {
        if index >= count || index == self.desired {
            return false;
        }
        self.desired = index;
        true
    }
    pub fn load_index(&self) -> Option<usize> {
        if self.running.is_some() {
            None
        } else {
            Some(if self.ready { self.desired } else { 0 })
        }
    }
    pub fn loaded(
        &mut self,
        index: usize,
        incoming: T,
        reduced_motion: bool,
        now: Instant,
    ) -> Option<(usize, T)> {
        if self.load_index() != Some(index) {
            return None;
        }
        if self.ready && index != self.current && !reduced_motion {
            self.running = Some(Running {
                started: now,
                index,
                incoming,
            });
            None
        } else {
            self.current = index;
            self.ready = true;
            Some((index, incoming))
        }
    }
    pub fn finish(&mut self, now: Instant) -> Option<(usize, T)> {
        if !self
            .running
            .as_ref()
            .is_some_and(|run| now.saturating_duration_since(run.started) >= DURATION)
        {
            return None;
        }
        let run = self.running.take().unwrap();
        self.current = run.index;
        Some((run.index, run.incoming))
    }
}

/// CSS cubic-bezier(.22,.61,.36,1), solving x(t) before evaluating y(t).
fn progress(fraction: f64) -> f64 {
    if fraction <= 0. {
        return 0.;
    }
    if fraction >= 1. {
        return 1.;
    }
    let curve = |t: f64, a: f64, b: f64| {
        3. * (1. - t).powi(2) * t * a + 3. * (1. - t) * t * t * b + t.powi(3)
    };
    let (mut low, mut high) = (0., 1.);
    for _ in 0..24 {
        let middle = (low + high) / 2.;
        if curve(middle, 0.22, 0.36) < fraction {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve((low + high) / 2., 0.61, 1.)
}

pub fn offsets(elapsed: Duration, forward: bool) -> (f32, f32) {
    let eased = progress(elapsed.as_secs_f64() / DURATION.as_secs_f64()) as f32;
    let direction = if forward { 1. } else { -1. };
    (-direction * eased, direction * (1. - eased))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_choices_finish_the_running_slide_then_use_only_latest_target() {
        let now = Instant::now();
        let mut carousel = Carousel::default();
        assert_eq!(carousel.loaded(0, "first", false, now), Some((0, "first")));
        assert!(carousel.select(1, 8));
        assert_eq!(carousel.loaded(1, "second", false, now), None);
        carousel.select(2, 8);
        carousel.select(5, 8);
        assert_eq!(carousel.load_index(), None);
        assert_eq!(carousel.current(), 0);
        assert_eq!(
            carousel.finish(now + DURATION - Duration::from_millis(1)),
            None
        );
        assert_eq!(carousel.finish(now + DURATION), Some((1, "second")));
        assert_eq!(carousel.load_index(), Some(5));
        assert_eq!(
            carousel.loaded(2, "late skipped page", false, now + DURATION),
            None
        );
        assert_eq!(carousel.loaded(5, "latest", false, now + DURATION), None);
        assert_eq!(carousel.finish(now + DURATION * 2), Some((5, "latest")));
        assert_eq!(carousel.current(), carousel.desired());
    }
    #[test]
    fn reversing_during_transition_does_not_overwrite_the_incoming_slide() {
        let now = Instant::now();
        let mut carousel = Carousel::default();
        carousel.loaded(0, 0, false, now);
        carousel.select(1, 2);
        carousel.loaded(1, 1, false, now);
        carousel.select(0, 2);
        assert_eq!(carousel.running().unwrap().index, 1);
        assert_eq!(carousel.finish(now + DURATION), Some((1, 1)));
        carousel.loaded(0, 0, false, now + DURATION);
        assert_eq!(carousel.finish(now + DURATION * 2), Some((0, 0)));
    }
    #[test]
    fn initial_page_settles_before_early_selection_and_reduced_motion_commits_errors() {
        let now = Instant::now();
        let mut carousel = Carousel::default();
        assert!(carousel.select(3, 4));
        assert!(!carousel.select(4, 4));
        assert_eq!(carousel.load_index(), Some(0));
        assert_eq!(carousel.loaded(3, Ok("too early"), false, now), None);
        assert_eq!(
            carousel.loaded(0, Ok("first"), false, now),
            Some((0, Ok("first")))
        );
        assert_eq!(carousel.load_index(), Some(3));
        assert_eq!(
            carousel.loaded(3, Err("unavailable"), true, now),
            Some((3, Err("unavailable")))
        );
        assert!(carousel.running().is_none());
        assert_eq!(carousel.current(), 3);
    }
    #[test]
    fn css_curve_uses_time_coordinate_not_bezier_parameter() {
        // At Bezier parameter .5, x=.3425 and y=.72875.
        assert!((progress(0.3425) - 0.72875).abs() < 0.000001);
    }
    #[test]
    fn both_directions_keep_slides_one_frame_width_apart() {
        for forward in [true, false] {
            for millis in [0, 20, 80, 160, 240, 400] {
                let (outgoing, incoming) = offsets(Duration::from_millis(millis), forward);
                assert!(((incoming - outgoing).abs() - 1.).abs() < 0.00001);
            }
        }
        assert_eq!(offsets(Duration::ZERO, true), (0., 1.));
        assert_eq!(offsets(DURATION, true), (-1., 0.));
        assert_eq!(offsets(DURATION, false), (1., 0.));
    }
}
