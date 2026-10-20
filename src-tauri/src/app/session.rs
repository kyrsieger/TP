use serde::{Deserialize, Serialize};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default, Debug, PartialEq, Eq)]
pub(super) enum PanelIdlePhase {
    #[default]
    Awake,
    Clearing,
    Suspending,
    Suspended,
    Retained,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PanelSession {
    pub(super) view: String,
    pub(super) kind_filter: String,
    pub(super) search: String,
}

impl Default for PanelSession {
    fn default() -> Self {
        Self {
            view: "default".into(),
            kind_filter: "all".into(),
            search: String::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct PanelState {
    pub(super) ready: bool,
    pub(super) open_requested: bool,
    pub(super) idle_phase: PanelIdlePhase,
    pub(super) generation: u64,
    pub(super) hidden_at: Option<Instant>,
    pub(super) background_resident: bool,
    pub(super) reload_before_open: bool,
    pub(super) session: PanelSession,
    pub(super) shortcut_error: Option<String>,
}

impl PanelState {
    pub(super) fn set_background_resident(&mut self, enabled: bool) {
        if self.background_resident == enabled {
            return;
        }
        self.background_resident = enabled;
        // Invalidate queued release acknowledgements and native suspend callbacks.
        self.generation = self.generation.wrapping_add(1);
        self.idle_phase = PanelIdlePhase::Awake;
        if self.hidden_at.is_some() {
            // Turning residency off starts a fresh one-minute retention period.
            self.hidden_at = Some(Instant::now());
        }
    }

    pub(super) fn begin_reload_for_open(&mut self) -> bool {
        if !self.ready || !self.reload_before_open {
            return false;
        }
        self.document_loading(false);
        self.open_requested = true;
        true
    }

    pub(super) fn document_loading(&mut self, visible: bool) {
        self.reload_before_open = false;
        self.ready = false;
        self.open_requested |= visible;
        // A new document cannot acknowledge an old document's release request.
        self.generation = self.generation.wrapping_add(1);
        if self.hidden_at.is_some() {
            self.idle_phase = PanelIdlePhase::Awake;
        }
    }

    pub(super) fn document_ready(&mut self, visible: bool) -> Option<u64> {
        self.ready = true;
        let opening = self.open_requested || visible;
        self.open_requested = false;
        if opening {
            self.cancel_idle();
            Some(self.generation)
        } else {
            None
        }
    }

    pub(super) fn cancel_idle(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.hidden_at = None;
        self.idle_phase = PanelIdlePhase::Awake;
    }

    pub(super) fn mark_hidden(&mut self) -> bool {
        if self.hidden_at.is_some() {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.hidden_at = Some(Instant::now());
        self.idle_phase = PanelIdlePhase::Awake;
        true
    }

    pub(super) fn idle_due(&self, now: Instant) -> bool {
        self.ready
            && !self.background_resident
            && !self.open_requested
            && self.idle_phase == PanelIdlePhase::Awake
            && self
                .hidden_at
                .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(60))
    }

    pub(super) fn begin_release(&mut self, now: Instant) -> Option<u64> {
        if !self.idle_due(now) {
            return None;
        }
        self.idle_phase = PanelIdlePhase::Clearing;
        Some(self.generation)
    }

    #[cfg(debug_assertions)]
    pub(super) fn expire_idle_delay(&mut self) {
        if let Some(hidden_at) = self.hidden_at.as_mut() {
            *hidden_at = Instant::now() - Duration::from_secs(60);
        }
    }

    pub(super) fn begin_suspend(&mut self, generation: u64) -> bool {
        if generation != self.generation
            || self.background_resident
            || self.idle_phase != PanelIdlePhase::Clearing
            || self.hidden_at.is_none()
            || self.open_requested
        {
            return false;
        }
        self.idle_phase = PanelIdlePhase::Suspending;
        true
    }

    pub(super) fn finish_suspend(&mut self, generation: u64, succeeded: bool) -> bool {
        if generation != self.generation || self.idle_phase != PanelIdlePhase::Suspending {
            return false;
        }
        self.idle_phase = if succeeded {
            PanelIdlePhase::Suspended
        } else {
            PanelIdlePhase::Retained
        };
        true
    }

    pub(super) fn accepts_history_events(&self) -> bool {
        self.ready && self.hidden_at.is_none() && self.idle_phase == PanelIdlePhase::Awake
    }

    pub(super) fn clear_hidden_search(&mut self, retain_search_input: bool) {
        if self.hidden_at.is_some() && !retain_search_input {
            self.session.search.clear();
        }
    }

    pub(super) fn clear_hidden_type_filter(&mut self, retain_type_filter: bool) {
        if self.hidden_at.is_some() && !retain_type_filter {
            self.session.kind_filter = "all".into();
        }
    }
}

#[derive(Default)]
pub(super) struct PanelLifecycle {
    pub(super) state: Mutex<PanelState>,
    pub(super) shortcut: Mutex<String>,
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
