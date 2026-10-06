use super::*;

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
