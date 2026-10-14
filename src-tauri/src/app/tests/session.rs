use super::*;

#[test]
fn type_filter_retention_clears_only_hidden_filters_including_late_session_saves() {
    for hidden in [true, false] {
        for retain in [true, false] {
            for phase in [PanelIdlePhase::Awake, PanelIdlePhase::Suspended] {
                let mut state = PanelState {
                    hidden_at: hidden.then(Instant::now),
                    idle_phase: phase,
                    session: PanelSession {
                        view: "favorite".into(),
                        kind_filter: "image".into(),
                        search: "keyword".into(),
                    },
                    ..PanelState::default()
                };
                state.clear_hidden_type_filter(retain);
                assert_eq!(
                    state.session.kind_filter,
                    if hidden && !retain { "all" } else { "image" }
                );
                assert_eq!(state.session.search, "keyword");
                assert_eq!(state.session.view, "favorite");
                state.session.kind_filter = "video".into();
                state.clear_hidden_type_filter(retain);
                assert_eq!(
                    state.session.kind_filter,
                    if hidden && !retain { "all" } else { "video" }
                );
            }
        }
    }
}

#[test]
fn resident_panel_never_releases_and_disabling_restarts_the_idle_deadline() {
    let mut state = PanelState {
        ready: true,
        ..PanelState::default()
    };
    state.set_background_resident(true);
    state.mark_hidden();
    let hidden_at = state.hidden_at.unwrap();
    assert_eq!(
        state.begin_release(hidden_at + Duration::from_secs(3600)),
        None
    );
    assert!(!state.accepts_history_events());
    state.session.search = "keyword".into();
    state.clear_hidden_search(false);
    assert_eq!(state.session.search, "");
    state.hidden_at = Some(hidden_at - Duration::from_secs(120));
    state.set_background_resident(false);
    let restarted = state.hidden_at.unwrap();
    assert!(restarted >= hidden_at);
    assert!(!state.idle_due(restarted + Duration::from_secs(59)));
    assert!(state
        .begin_release(restarted + Duration::from_secs(60))
        .is_some());
}

#[test]
fn enabling_residency_invalidates_release_and_suspend_even_after_disabling_again() {
    for suspending in [false, true] {
        let mut state = PanelState {
            ready: true,
            ..PanelState::default()
        };
        state.mark_hidden();
        let old = state
            .begin_release(state.hidden_at.unwrap() + Duration::from_secs(60))
            .unwrap();
        if suspending {
            assert!(state.begin_suspend(old));
        }
        state.set_background_resident(true);
        assert!(!state.begin_suspend(old));
        assert!(!state.finish_suspend(old, true));
        assert_eq!(state.idle_phase, PanelIdlePhase::Awake);
        state.set_background_resident(false);
        let current = state
            .begin_release(state.hidden_at.unwrap() + Duration::from_secs(60))
            .unwrap();
        assert!(!state.begin_suspend(old));
        assert!(state.begin_suspend(current));
    }
}

#[test]
fn residency_survives_navigation_and_repeated_visibility_cycles() {
    let mut state = PanelState {
        ready: true,
        ..PanelState::default()
    };
    state.set_background_resident(true);
    for _ in 0..3 {
        state.cancel_idle();
        state.mark_hidden();
        state.document_loading(false);
        assert!(state.document_ready(false).is_none());
        assert!(!state.idle_due(state.hidden_at.unwrap() + Duration::from_secs(3600)));
    }
}

#[test]
fn hidden_search_retention_preserves_other_session_state_and_rejects_late_search() {
    for hidden in [true, false] {
        for retain in [true, false] {
            let mut state = PanelState::default();
            state.hidden_at = hidden.then(Instant::now);
            state.session = PanelSession {
                view: "favorite".into(),
                kind_filter: "image".into(),
                search: "keyword".into(),
            };
            state.clear_hidden_search(retain);
            assert_eq!(
                state.session.search,
                if hidden && !retain { "" } else { "keyword" }
            );
            assert_eq!(state.session.view, "favorite");
            assert_eq!(state.session.kind_filter, "image");
            state.session.search = "late keyword".into();
            state.clear_hidden_search(retain);
            assert_eq!(
                state.session.search,
                if hidden && !retain {
                    ""
                } else {
                    "late keyword"
                }
            );
        }
    }
}

#[test]
fn idle_suspension_waits_one_minute_and_repeated_hides_keep_the_original_deadline() {
    let mut state = PanelState {
        ready: true,
        ..PanelState::default()
    };
    assert!(state.mark_hidden());
    let hidden_at = state.hidden_at.unwrap();
    assert!(!state.mark_hidden());
    assert_eq!(state.hidden_at, Some(hidden_at));
    assert_eq!(
        state.begin_release(hidden_at + Duration::from_secs(59)),
        None
    );
    let generation = state
        .begin_release(hidden_at + Duration::from_secs(60))
        .unwrap();
    assert_eq!(
        state.begin_release(hidden_at + Duration::from_secs(61)),
        None
    );
    assert!(state.begin_suspend(generation));
    assert!(state.finish_suspend(generation, true));
    assert_eq!(state.idle_phase, PanelIdlePhase::Suspended);
    assert_eq!(state.hidden_at, Some(hidden_at));
    assert!(!state.accepts_history_events());
    state.session.search = "late search".into();
    state.clear_hidden_search(false);
    assert_eq!(state.session.search, "");
}

#[test]
fn reopening_during_suspension_rejects_the_late_completion_and_starts_a_new_idle_period() {
    let mut state = PanelState {
        ready: true,
        ..PanelState::default()
    };
    state.mark_hidden();
    let generation = state
        .begin_release(state.hidden_at.unwrap() + Duration::from_secs(60))
        .unwrap();
    assert!(state.begin_suspend(generation));
    state.cancel_idle();
    assert!(!state.finish_suspend(generation, true));
    assert_eq!(state.idle_phase, PanelIdlePhase::Awake);
    assert!(state.accepts_history_events());
    assert!(state.mark_hidden());
    let next = state
        .begin_release(state.hidden_at.unwrap() + Duration::from_secs(60))
        .unwrap();
    assert_ne!(generation, next);
    assert!(!state.finish_suspend(generation, true));
    assert_eq!(state.idle_phase, PanelIdlePhase::Clearing);
    assert!(state.begin_suspend(next));
    assert!(state.finish_suspend(next, true));
}

#[test]
fn unsupported_suspension_retains_the_hidden_panel_without_repeated_requests() {
    let mut state = PanelState {
        ready: true,
        ..PanelState::default()
    };
    state.mark_hidden();
    let now = state.hidden_at.unwrap() + Duration::from_secs(60);
    let generation = state.begin_release(now).unwrap();
    assert!(state.begin_suspend(generation));
    assert!(state.finish_suspend(generation, false));
    assert_eq!(state.idle_phase, PanelIdlePhase::Retained);
    assert_eq!(state.begin_release(now + Duration::from_secs(60)), None);
    assert!(!state.accepts_history_events());
    state.cancel_idle();
    assert!(state.accepts_history_events());
}

#[test]
fn startup_and_hidden_changes_never_refresh_the_page_and_pending_open_cancels_idle_work() {
    let mut state = PanelState::default();
    assert!(!state.accepts_history_events());
    state.ready = true;
    assert!(state.accepts_history_events());
    state.mark_hidden();
    assert!(!state.accepts_history_events());
    state.open_requested = true;
    assert_eq!(
        state.begin_release(state.hidden_at.unwrap() + Duration::from_secs(60)),
        None
    );
}

#[test]
fn release_ack_must_match_the_current_hidden_period_and_precede_suspension() {
    let mut state = PanelState {
        ready: true,
        ..PanelState::default()
    };
    state.mark_hidden();
    assert!(!state.begin_suspend(state.generation));
    let old = state
        .begin_release(state.hidden_at.unwrap() + Duration::from_secs(60))
        .unwrap();
    state.cancel_idle();
    assert!(!state.begin_suspend(old));
    state.mark_hidden();
    let current = state
        .begin_release(state.hidden_at.unwrap() + Duration::from_secs(60))
        .unwrap();
    assert!(!state.begin_suspend(old));
    assert!(state.begin_suspend(current));
    assert!(!state.begin_suspend(current));
}

#[test]
fn visible_document_reload_reopens_without_another_shortcut_and_hidden_reload_stays_hidden() {
    let mut state = PanelState {
        ready: true,
        ..PanelState::default()
    };
    state.document_loading(true);
    assert!(!state.ready);
    assert!(state.open_requested);
    assert!(state.document_ready(true).is_some());
    assert!(!state.open_requested);
    state.mark_hidden();
    let hidden_at = state.hidden_at;
    let old = state
        .begin_release(hidden_at.unwrap() + Duration::from_secs(60))
        .unwrap();
    state.document_loading(false);
    assert!(state.document_ready(false).is_none());
    assert_eq!(state.hidden_at, hidden_at);
    assert!(!state.begin_suspend(old));
    assert!(state.idle_due(hidden_at.unwrap() + Duration::from_secs(61)));
}
