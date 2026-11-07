//! The original panel's variable-height layout, independent of mounted rows.
use std::ops::Range;

#[derive(Clone, Default)]
pub struct ListLayout {
    pub offsets: Vec<f32>,
}
impl ListLayout {
    pub fn new<'a>(rows: impl IntoIterator<Item = (&'a str, bool)>) -> Self {
        let mut offsets = vec![0.];
        for (kind, named) in rows {
            let height = if matches!(kind, "image" | "video") {
                if named {
                    212.
                } else {
                    180.
                }
            } else {
                48.
            };
            offsets.push(offsets.last().unwrap() + height);
        }
        Self { offsets }
    }
    pub fn count(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }
    pub fn total(&self) -> f32 {
        self.offsets.last().copied().unwrap_or(0.)
    }
    fn first_after(&self, position: f32) -> usize {
        self.offsets
            .get(1..)
            .unwrap_or_default()
            .partition_point(|bottom| *bottom <= position)
    }
    pub fn visible(&self, top: f32, height: f32, overscan: f32) -> Range<usize> {
        let start = self.first_after((top - overscan).max(0.));
        let bottom = (top + height + overscan).max(0.);
        let mut end = self.first_after(bottom);
        if end < self.count() && self.offsets[end] < bottom {
            end += 1;
        }
        start..end.max(start)
    }
    pub fn reveal(&self, index: usize, top: f32, height: f32) -> f32 {
        if index >= self.count() {
            return top;
        }
        if self.offsets[index] < top {
            self.offsets[index]
        } else if self.offsets[index + 1] > top + height {
            (self.offsets[index + 1] - height).max(0.)
        } else {
            top
        }
    }
    pub fn back_to_top_offset(&self, height: f32) -> Option<f32> {
        let capacity = self.first_after(height);
        (capacity + 9 < self.count()).then(|| self.offsets[capacity + 9])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_rows_and_partial_edges_match_original_layout() {
        let l = ListLayout::new([
            ("text", false),
            ("image", false),
            ("file", false),
            ("video", false),
            ("link", false),
        ]);
        assert_eq!(l.offsets, [0., 48., 228., 276., 456., 504.]);
        assert_eq!(l.visible(48., 180., 0.), 1..2);
        assert_eq!(l.visible(227., 50., 0.), 1..4);
        let l = ListLayout::new([
            ("text", true),
            ("image", true),
            ("video", false),
            ("video", true),
        ]);
        assert_eq!(l.offsets, [0., 48., 260., 440., 652.]);
        assert_eq!(l.reveal(3, 0., 400.), 252.);
    }
    #[test]
    fn five_thousand_records_only_mount_viewport_and_overscan() {
        let l = ListLayout::new(std::iter::repeat_n(("text", false), 5000));
        for top in [0., 240., 120000., 239520.] {
            let r = l.visible(top, 480., 180.);
            assert!(r.len() <= 19);
            assert!(l.offsets[r.start] <= top);
            assert!(l.offsets[r.end] >= (top + 480.).min(l.total()));
        }
        assert_eq!(ListLayout::new([]).visible(0., 480., 180.), 0..0);
    }
    #[test]
    fn navigation_reveals_distant_rows_and_retains_visible_neighbors() {
        let l =
            ListLayout::new((0..100).map(|i| (if i % 2 == 0 { "text" } else { "image" }, false)));
        let top = l.reveal(99, 0., 400.);
        assert_eq!(top, l.total() - 400.);
        assert!(l.visible(top, 400., 180.).contains(&99));
        assert_eq!(l.reveal(98, top, 400.), top);
        assert_eq!(l.reveal(0, top, 400.), 0.);
        let l = ListLayout::new((0..100).map(|i| (if i == 1 { "image" } else { "text" }, false)));
        assert_eq!(l.back_to_top_offset(276.), Some(l.offsets[12]));
    }
}
