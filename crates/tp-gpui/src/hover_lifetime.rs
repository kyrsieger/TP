//! One-way cancellation for a single floating card and its delayed callbacks.
use std::cell::Cell;

#[derive(Default)]
pub struct HoverLifetime {
    closed: Cell<bool>,
    layout: Cell<u64>,
    leave: Cell<u64>,
}
impl HoverLifetime {
    #[track_caller]
    pub fn close(&self) {
        if !self.closed.get() && std::env::var_os("TP_DIAGNOSTIC_HOVER").is_some() {
            eprintln!("floating: close at {}", std::panic::Location::caller());
        }
        self.closed.set(true);
    }
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }
    pub fn queue_layout(&self) -> u64 {
        let ticket = self.layout.get().wrapping_add(1);
        self.layout.set(ticket);
        ticket
    }
    pub fn allows_layout(&self, ticket: u64) -> bool {
        !self.is_closed() && self.layout.get() == ticket
    }
    pub fn cancel_leave(&self) {
        self.leave.set(self.leave.get().wrapping_add(1));
    }
    pub fn queue_leave(&self) -> u64 {
        self.cancel_leave();
        self.leave.get()
    }
    pub fn allows_leave(&self, ticket: u64) -> bool {
        !self.is_closed() && self.leave.get() == ticket
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_rejects_queued_and_late_layouts() {
        let state = HoverLifetime::default();
        let queued = state.queue_layout();
        state.close();
        assert!(!state.allows_layout(queued));
        assert!(!state.allows_layout(state.queue_layout()));
        assert!(!state.allows_leave(state.queue_leave()));
    }
    #[test]
    fn only_latest_measurement_can_reposition_the_window() {
        let state = HoverLifetime::default();
        let old = state.queue_layout();
        let current = state.queue_layout();
        assert!(!state.allows_layout(old));
        assert!(state.allows_layout(current));
    }
    #[test]
    fn reenter_then_leave_does_not_reuse_the_first_deadline() {
        let state = HoverLifetime::default();
        let old = state.queue_leave();
        state.cancel_leave();
        assert!(!state.allows_leave(old));
        let current = state.queue_leave();
        assert!(!state.allows_leave(old));
        assert!(state.allows_leave(current));
    }
}
