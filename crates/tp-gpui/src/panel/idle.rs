use std::time::{Duration, Instant};

const RELEASE_DELAY: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct IdleContent {
    deadline: Option<Instant>,
    pub released: bool,
}

impl IdleContent {
    pub fn at_start(visible: bool) -> Self {
        Self {
            deadline: None,
            released: !visible,
        }
    }
    pub fn needs_snapshot(&self, hidden: bool) -> bool {
        !hidden || !self.released
    }
    #[cfg(debug_assertions)]
    pub fn expire_delay(&mut self, now: Instant) {
        // Only shorten an already scheduled release.
        if self.deadline.is_some() {
            self.deadline = Some(now);
        }
    }
    pub fn hide(&mut self, now: Instant) {
        if !self.released && self.deadline.is_none() {
            self.deadline = Some(now + RELEASE_DELAY);
        }
    }
    pub fn show(&mut self) {
        self.deadline = None;
        self.released = false;
    }
    pub fn release_due(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.deadline = None;
            self.released = true;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_start_defers_history_until_first_open() {
        let mut idle = IdleContent::at_start(false);
        assert!(!idle.needs_snapshot(true));
        idle.hide(Instant::now());
        assert!(!idle.needs_snapshot(true));
        idle.show();
        assert!(idle.needs_snapshot(false));
        assert!(IdleContent::at_start(true).needs_snapshot(false));
    }
    #[test]
    fn completed_background_work_does_not_reload_released_content() {
        let now = Instant::now();
        let mut idle = IdleContent::default();
        assert!(idle.needs_snapshot(false));
        idle.hide(now);
        assert!(idle.needs_snapshot(true));
        assert!(idle.release_due(now + Duration::from_secs(30)));
        // Multiple task completions remain deferred until the panel reopens.
        assert!(!idle.needs_snapshot(true));
        idle.hide(now + Duration::from_secs(31));
        assert!(!idle.needs_snapshot(true));
        idle.show();
        assert!(idle.needs_snapshot(true));
        assert!(idle.needs_snapshot(false));
    }
    #[test]
    #[cfg(debug_assertions)]
    fn debug_release_requires_a_pending_hide() {
        let now = Instant::now();
        let mut idle = IdleContent::default();
        idle.expire_delay(now);
        assert!(!idle.release_due(now));
        idle.hide(now);
        idle.expire_delay(now);
        assert!(idle.release_due(now));
        idle.show();
        idle.hide(now);
        idle.show();
        idle.expire_delay(now);
        assert!(!idle.release_due(now));
    }
    #[test]
    fn idle_grace_period_is_not_extended_by_repeated_hides() {
        let now = Instant::now();
        let mut idle = IdleContent::default();
        idle.hide(now);
        idle.hide(now + Duration::from_secs(20));
        assert!(!idle.release_due(now + Duration::from_secs(29)));
        assert!(idle.release_due(now + Duration::from_secs(30)));
        assert!(idle.released);
        assert!(!idle.release_due(now + Duration::from_secs(31)));
    }
    #[test]
    fn reopening_cancels_release_and_next_hide_starts_a_new_delay() {
        let now = Instant::now();
        let mut idle = IdleContent::default();
        idle.hide(now);
        idle.show();
        assert!(!idle.release_due(now + Duration::from_secs(30)));
        idle.hide(now + Duration::from_secs(35));
        assert!(!idle.release_due(now + Duration::from_secs(64)));
        assert!(idle.release_due(now + Duration::from_secs(65)));
        idle.show();
        assert!(!idle.released);
    }
}
