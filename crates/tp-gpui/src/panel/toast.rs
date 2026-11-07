use std::time::{Duration, Instant};

pub(super) const VISIBLE: Duration = Duration::from_millis(3500);
pub(super) const FADE: Duration = Duration::from_millis(180);

pub(super) struct Toast {
    pub message: String,
    shown: Instant,
    initial_opacity: f32,
}

impl Toast {
    pub fn new(message: String, previous: Option<&Self>, now: Instant) -> Self {
        Self {
            message,
            shown: now,
            initial_opacity: previous.map_or(0., |previous| previous.opacity(now)),
        }
    }
    pub fn opacity(&self, now: Instant) -> f32 {
        let age = now.saturating_duration_since(self.shown);
        if age >= VISIBLE {
            1. - ease((age - VISIBLE).as_secs_f32() / FADE.as_secs_f32())
        } else {
            self.initial_opacity
                + (1. - self.initial_opacity) * ease(age.as_secs_f32() / FADE.as_secs_f32())
        }
    }
    pub fn animating(&self, now: Instant) -> bool {
        let age = now.saturating_duration_since(self.shown);
        (age < FADE && self.initial_opacity < 1.) || (VISIBLE..VISIBLE + FADE).contains(&age)
    }
}

// CSS `ease`: cubic-bezier(.25, .1, .25, 1). Solve x before evaluating y.
fn ease(progress: f32) -> f32 {
    if progress <= 0. {
        return 0.;
    }
    if progress >= 1. {
        return 1.;
    }
    let (mut low, mut high) = (0., 1.);
    for _ in 0..18 {
        let t = (low + high) / 2.;
        let x = 0.75 * (1. - t) * t + t * t * t;
        if x < progress {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) / 2.;
    0.3 * (1. - t) * (1. - t) * t + 3. * (1. - t) * t * t + t * t * t
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toast_lifetime_includes_both_transitions() {
        let now = Instant::now();
        let toast = Toast::new("error".into(), None, now);
        assert_eq!(toast.opacity(now), 0.);
        assert!((toast.opacity(now + FADE / 2) - 0.8024).abs() < 0.001);
        assert_eq!(toast.opacity(now + FADE), 1.);
        assert!(!toast.animating(now + FADE));
        assert!(toast.animating(now + VISIBLE));
        assert_eq!(toast.opacity(now + VISIBLE + FADE), 0.);
        assert!(!toast.animating(now + VISIBLE + FADE));
    }
    #[test]
    fn replacing_message_preserves_opacity_and_restarts_visibility() {
        let now = Instant::now();
        let first = Toast::new("first".into(), None, now);
        for age in [FADE / 2, Duration::from_secs(2), VISIBLE + FADE / 2] {
            let replaced = now + age;
            let second = Toast::new("second".into(), Some(&first), replaced);
            assert_eq!(second.opacity(replaced), first.opacity(replaced));
            assert_eq!(
                second.opacity(replaced + VISIBLE - Duration::from_millis(1)),
                1.
            );
            assert_eq!(second.opacity(replaced + VISIBLE + FADE), 0.);
        }
    }
}
