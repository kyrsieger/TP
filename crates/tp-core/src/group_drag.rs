//! Pointer-independent rules ported from use-group-drag.ts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Press {
    pub id: i64,
    start: (f32, f32),
    started_ms: u64,
    pub x: f32,
    pub active: bool,
}

impl Press {
    pub fn new(id: i64, x: f32, y: f32, now_ms: u64) -> Self {
        Self {
            id,
            start: (x, y),
            started_ms: now_ms,
            x,
            active: false,
        }
    }

    /// False cancels a press that moved too far before the hold timer fired.
    pub fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        self.x = x;
        self.active || (x - self.start.0).hypot(y - self.start.1) <= 6.
    }

    /// Resolve the hold deadline before processing a newly delivered pointer event.
    pub fn pointer_at(&mut self, x: f32, y: f32, now_ms: u64) -> bool {
        self.tick(now_ms);
        self.pointer_move(x, y)
    }

    pub fn tick(&mut self, now_ms: u64) -> bool {
        if now_ms.saturating_sub(self.started_ms) >= 200 {
            self.active = true;
        }
        self.active
    }
}

/// DOM/layout order is significant; the dragged item cannot be its own target.
pub fn insertion_before(
    dragged: i64,
    x: f32,
    bounds: impl IntoIterator<Item = (i64, f32, f32)>,
) -> Option<i64> {
    bounds
        .into_iter()
        .find_map(|(id, left, width)| (id != dragged && x < left + width / 2.).then_some(id))
}

pub fn reordered(ids: &[i64], dragged: i64, before: Option<i64>) -> Option<Vec<i64>> {
    if !ids.contains(&dragged) || before == Some(dragged) {
        return None;
    }
    let mut order: Vec<_> = ids.iter().copied().filter(|id| *id != dragged).collect();
    let index = before
        .and_then(|id| order.iter().position(|item| *item == id))
        .unwrap_or(order.len());
    order.insert(index, dragged);
    Some(order)
}

pub fn edge_step(x: f32, left: f32, right: f32) -> f32 {
    if x < left + 28. {
        -8.
    } else if x > right - 28. {
        8.
    } else {
        0.
    }
}

pub fn floating_left(
    start_left: f32,
    start_x: f32,
    x: f32,
    width: f32,
    left: f32,
    right: f32,
) -> f32 {
    (start_left + x - start_x).min(right - width).max(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_threshold_and_early_motion_match_original() {
        let mut press = Press::new(1, 10., 20., 1000);
        assert!(!press.tick(1199));
        assert!(press.pointer_move(16., 20.));
        assert!(!press.pointer_move(16., 21.));
        let mut held = Press::new(1, 10., 20., 1000);
        assert!(held.tick(1200));
        assert!(held.pointer_move(400., -100.));
    }

    #[test]
    fn pointer_delivery_resolves_deadline_without_waiting_for_animation_tick() {
        let mut early = Press::new(1, 10., 20., 1000);
        assert!(!early.pointer_at(30., 20., 1199));
        assert!(!early.active);
        let mut held = Press::new(1, 10., 20., 1000);
        assert!(held.pointer_at(30., 20., 1200));
        assert!(held.active);
        assert!(held.pointer_at(400., -100., 1201));
    }

    #[test]
    fn insertion_excludes_source_and_midpoint_advances_to_next_item() {
        let bounds = [(1, 10., 40.), (2, 55., 60.), (3, 120., 40.)];
        assert_eq!(insertion_before(1, 84., bounds), Some(2));
        assert_eq!(insertion_before(1, 85., bounds), Some(3));
        assert_eq!(insertion_before(1, 140., bounds), None);
        assert_eq!(reordered(&[1, 2, 3], 1, Some(3)), Some(vec![2, 1, 3]));
        assert_eq!(reordered(&[1, 2, 3], 1, None), Some(vec![2, 3, 1]));
        assert_eq!(reordered(&[1, 2, 3], 4, None), None);
    }

    #[test]
    fn edge_and_floating_label_stay_within_original_limits() {
        assert_eq!(edge_step(27., 0., 400.), -8.);
        assert_eq!(edge_step(28., 0., 400.), 0.);
        assert_eq!(edge_step(372., 0., 400.), 0.);
        assert_eq!(edge_step(373., 0., 400.), 8.);
        assert_eq!(floating_left(50., 60., -100., 80., 0., 400.), 0.);
        assert_eq!(floating_left(50., 60., 500., 80., 0., 400.), 320.);
        assert_eq!(floating_left(0., 0., 50., 500., 0., 400.), 0.);
    }
}
