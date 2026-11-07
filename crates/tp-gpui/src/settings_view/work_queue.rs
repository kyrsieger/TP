//! A later appearance choice must not replace a pending cleanup confirmation.
#[derive(Default)]
pub(super) struct ThemeQueue(Option<String>);

impl ThemeQueue {
    pub fn push(&mut self, theme: &str) {
        self.0 = Some(theme.into());
    }

    pub fn is_pending(&self) -> bool {
        self.0.is_some()
    }

    pub fn take_ready(&mut self, saving: bool, confirming: bool) -> Option<String> {
        if saving || confirming {
            None
        } else {
            self.0.take()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ThemeQueue;

    #[test]
    fn save_completion_cannot_consume_choice_while_cleanup_confirmation_is_pending() {
        let mut queue = ThemeQueue::default();
        queue.push("ice");
        queue.push("olive");
        assert_eq!(queue.take_ready(true, false), None);
        assert_eq!(queue.take_ready(false, true), None);
        assert!(queue.is_pending());
        // Cancellation (including hiding) releases the confirmation. Preserve
        // the last choice, without replaying the earlier theme or deleting data.
        assert_eq!(queue.take_ready(false, false).as_deref(), Some("olive"));
        assert!(!queue.is_pending());
        assert_eq!(queue.take_ready(false, false), None);
    }

    #[test]
    fn confirmed_save_keeps_choice_until_the_write_finishes() {
        let mut queue = ThemeQueue::default();
        queue.push("clay");
        assert_eq!(queue.take_ready(false, true), None);
        assert_eq!(queue.take_ready(true, false), None);
        assert_eq!(queue.take_ready(false, false).as_deref(), Some("clay"));
    }
}
