// Context menus initially focus their container, not an action.
pub(super) fn next_item(active: Option<usize>, count: usize, up: bool) -> usize {
    match active {
        None => {
            if up {
                count - 1
            } else {
                0
            }
        }
        Some(index) => (index + if up { count - 1 } else { 1 }) % count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_arrow_enters_menu_and_subsequent_arrows_wrap() {
        for count in [1, 2, 4] {
            assert_eq!(next_item(None, count, false), 0);
            assert_eq!(next_item(None, count, true), count - 1);
            assert_eq!(next_item(Some(count - 1), count, false), 0);
            assert_eq!(next_item(Some(0), count, true), count - 1);
        }
    }
}
