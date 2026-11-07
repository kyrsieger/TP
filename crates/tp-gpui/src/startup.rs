//! Normal launches stay in the tray; isolated development fixtures may show UI.
pub fn show_panel(debug: bool, isolated: bool, start_hidden: bool) -> bool {
    debug && isolated && !start_hidden
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_isolated_debug_launches_can_show_the_panel_automatically() {
        for isolated in [false, true] {
            for hidden in [false, true] {
                assert!(!super::show_panel(false, isolated, hidden));
            }
        }
        assert!(!super::show_panel(true, false, false));
        assert!(super::show_panel(true, true, false));
        assert!(!super::show_panel(true, true, true));
    }
}
