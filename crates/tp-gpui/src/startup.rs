//! Show the panel automatically for diagnostic fixtures and hot-reload sessions.
pub fn show_panel(debug: bool, isolated: bool, hot_reload: bool, start_hidden: bool) -> bool {
    debug && (isolated || hot_reload) && !start_hidden
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_debug_fixtures_and_hot_reload_sessions_open_the_panel() {
        for isolated in [false, true] {
            for hot_reload in [false, true] {
                assert!(!super::show_panel(false, isolated, hot_reload, false));
                assert!(!super::show_panel(true, isolated, hot_reload, true));
            }
        }
        assert!(!super::show_panel(true, false, false, false));
        assert!(super::show_panel(true, true, false, false));
        assert!(super::show_panel(true, false, true, false));
    }
}
