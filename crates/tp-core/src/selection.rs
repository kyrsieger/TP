//! Range selection follows the same anchor and wrap rules as the original panel.
use std::collections::HashSet;

#[derive(Default)]
pub struct Selection {
    pub current: Option<String>,
    pub ids: HashSet<String>,
    anchor: Option<String>,
}

impl Selection {
    pub fn select(&mut self, visible: &[String], id: &str, extend: bool) {
        let Some(target) = visible.iter().position(|item| item == id) else {
            return;
        };
        let anchor = extend
            .then(|| {
                self.anchor
                    .as_ref()
                    .and_then(|id| visible.iter().position(|item| item == id))
            })
            .flatten();
        let start = anchor.unwrap_or(target);
        self.ids = visible[start.min(target)..=start.max(target)]
            .iter()
            .cloned()
            .collect();
        self.current = Some(id.into());
        if anchor.is_none() {
            self.anchor = Some(id.into());
        }
    }

    pub fn step(&mut self, visible: &[String], direction: isize, extend: bool) -> Option<usize> {
        if visible.is_empty() {
            return None;
        }
        let index = self
            .current
            .as_ref()
            .and_then(|id| visible.iter().position(|item| item == id));
        let next = match index {
            None => 0,
            Some(index) if extend => {
                (index as isize + direction).clamp(0, visible.len() as isize - 1) as usize
            }
            Some(index) => (index as isize + direction).rem_euclid(visible.len() as isize) as usize,
        };
        self.select(visible, &visible[next], extend);
        Some(next)
    }

    pub fn reconcile(&mut self, visible: &[String]) {
        self.ids.retain(|id| visible.contains(id));
        if self.current.as_ref().is_none_or(|id| !visible.contains(id)) {
            self.current = visible
                .iter()
                .find(|id| self.ids.contains(*id))
                .or(visible.first())
                .cloned();
        }
        if let Some(id) = &self.current {
            if !self.ids.contains(id) {
                self.ids = HashSet::from([id.clone()]);
            }
        }
        if self.anchor.as_ref().is_none_or(|id| !visible.contains(id)) {
            self.anchor = self.current.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids() -> Vec<String> {
        ["a", "b", "c", "d"].map(String::from).to_vec()
    }
    #[test]
    fn range_can_expand_contract_and_cross_the_anchor() {
        let ids = ids();
        let mut s = Selection::default();
        s.select(&ids, "b", false);
        s.select(&ids, "d", true);
        assert_eq!(s.ids.len(), 3);
        s.select(&ids, "c", true);
        assert_eq!(s.ids.len(), 2);
        s.select(&ids, "a", true);
        assert_eq!(s.ids, HashSet::from(["a".into(), "b".into()]));
        s.select(&ids, "d", false);
        assert_eq!(s.ids, HashSet::from(["d".into()]));
    }
    #[test]
    fn navigation_wraps_but_shift_navigation_stops_at_edges() {
        let ids = ids();
        let mut s = Selection::default();
        assert_eq!(s.step(&ids, -1, false), Some(0));
        assert_eq!(s.step(&ids, -1, false), Some(3));
        assert_eq!(s.step(&ids, 1, true), Some(3));
        assert_eq!(s.step(&ids, 1, false), Some(0));
    }
    #[test]
    fn deleting_or_filtering_records_reconciles_current_and_anchor() {
        let ids = ids();
        let mut s = Selection::default();
        s.select(&ids, "b", false);
        s.select(&ids, "d", true);
        s.reconcile(&ids[..3]);
        assert_eq!(s.current.as_deref(), Some("b"));
        assert_eq!(s.ids.len(), 2);
        s.reconcile(&ids[..1]);
        assert_eq!(s.current.as_deref(), Some("a"));
        s.reconcile(&[]);
        assert!(s.ids.is_empty());
        assert!(s.current.is_none());
    }
}
