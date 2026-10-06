use serde::{Deserialize, Serialize};
use std::{sync::Mutex, time::Instant};
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
    pub(super) releasing: bool,
    pub(super) hidden_at: Option<Instant>,
    pub(super) session: PanelSession,
    pub(super) shortcut_error: Option<String>,
}

impl PanelState {
    pub(super) fn clear_hidden_search(&mut self, retain_search_input: bool) {
        if self.hidden_at.is_some() && !retain_search_input {
            self.session.search.clear();
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
