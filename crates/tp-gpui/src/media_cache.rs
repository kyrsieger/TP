//! A hover preview retains its current page, neighbours, and requested page.
use std::collections::{HashMap, HashSet};

pub struct MediaCache<T> {
    ready: HashMap<usize, T>,
    loading: HashSet<usize>,
}
impl<T> Default for MediaCache<T> {
    fn default() -> Self {
        Self {
            ready: HashMap::new(),
            loading: HashSet::new(),
        }
    }
}
impl<T> MediaCache<T> {
    pub fn get(&self, index: usize) -> Option<&T> {
        self.ready.get(&index)
    }
    pub fn begin(&mut self, index: usize) -> bool {
        !self.ready.contains_key(&index) && self.loading.insert(index)
    }
    pub fn retain(&mut self, current: usize, desired: usize) -> Vec<T> {
        let removed: Vec<_> = self
            .ready
            .keys()
            .copied()
            .filter(|index| index.abs_diff(current) > 1 && *index != desired)
            .collect();
        removed
            .into_iter()
            .filter_map(|index| self.ready.remove(&index))
            .collect()
    }
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.loading.clear();
        self.ready.drain().map(|(_, value)| value)
    }
    pub fn complete(&mut self, index: usize, value: T, current: usize, desired: usize) {
        self.loading.remove(&index);
        if index.abs_diff(current) <= 1 || index == desired {
            self.ready.insert(index, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_to_a_prefetch_reuses_the_inflight_request() {
        let mut cache = MediaCache::default();
        assert!(cache.begin(1));
        assert!(!cache.begin(1));
        cache.complete(1, "next", 0, 1);
        assert_eq!(cache.get(1), Some(&"next"));
        assert!(!cache.begin(1));
    }
    #[test]
    fn late_distant_results_are_discarded_and_can_be_requested_again() {
        let mut cache = MediaCache::default();
        assert!(cache.begin(2));
        cache.complete(2, "stale", 20, 21);
        assert!(cache.get(2).is_none());
        assert!(cache.begin(2));
    }
    #[test]
    fn only_neighbours_and_desired_page_survive_pruning_including_errors() {
        let mut cache = MediaCache::default();
        for index in 0..4 {
            cache.complete(index, Err::<(), _>("unavailable"), 1, 3);
        }
        assert_eq!(cache.retain(3, 9).len(), 2);
        assert!(cache.get(0).is_none());
        assert!(cache.get(1).is_none());
        assert_eq!(cache.get(2), Some(&Err("unavailable")));
        assert!(!cache.begin(2));
        cache.complete(9, Ok(()), 3, 9);
        assert_eq!(cache.get(9), Some(&Ok(())));
    }

    #[test]
    fn eviction_returns_assets_once_and_close_drains_retained_pages() {
        let mut cache = MediaCache::default();
        for index in 0..4 {
            cache.complete(index, index, 1, 3);
        }
        let mut removed = cache.retain(3, 9);
        removed.sort();
        assert_eq!(removed, vec![0, 1]);
        assert!(cache.retain(3, 9).is_empty());
        assert!(cache.begin(9));
        let mut retained: Vec<_> = cache.drain().collect();
        retained.sort();
        assert_eq!(retained, vec![2, 3]);
        assert!(cache.drain().next().is_none());
        assert!(cache.begin(9));
    }
}
