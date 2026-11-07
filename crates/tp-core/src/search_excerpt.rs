//! Width-dependent excerpts around the first search hit, using the host's font measurements.
pub struct PreparedExcerpt {
    text: String,
    boundaries: Vec<usize>,
    hit: Option<(usize, usize)>,
}

fn largest(limit: usize, mut fits: impl FnMut(usize) -> bool) -> usize {
    let (mut low, mut high) = (0, limit.min(1));
    while high < limit && fits(high) {
        low = high;
        high = high.saturating_mul(2).min(limit);
    }
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

impl PreparedExcerpt {
    pub fn new(text: String, query: &str) -> Self {
        let mut boundaries: Vec<_> = text.char_indices().map(|(offset, _)| offset).collect();
        boundaries.push(text.len());
        let hit = crate::search_highlight::ranges(&text, query)
            .first()
            .map(|r| {
                (
                    boundaries.binary_search(&r.start).unwrap(),
                    boundaries.binary_search(&r.end).unwrap(),
                )
            });
        Self {
            text,
            boundaries,
            hit,
        }
    }

    pub fn fit(&self, width: f32, mut measure: impl FnMut(&str) -> f32) -> String {
        let Some((first, last)) = self.hit else {
            return self.text.clone();
        };
        if width <= 0. {
            return String::new();
        }
        let count = self.boundaries.len() - 1;
        let slice = |start, end| &self.text[self.boundaries[start]..self.boundaries[end]];
        let excerpt = |start, end| {
            format!(
                "{}{}{}",
                if start > 0 { "…" } else { "" },
                slice(start, end),
                if end < count { "…" } else { "" }
            )
        };
        // Probe short prefixes exponentially; do not shape megabytes to fit one row.
        if largest(count, |size| measure(slice(0, size)) <= width) == count {
            return self.text.clone();
        }
        let preceding = largest(first, |size| measure(&excerpt(first - size, last)) <= width);
        let mut start = if preceding == first {
            0
        } else {
            first
                - largest(first, |size| {
                    measure(slice(first - size, first)) <= width / 3.
                })
        };
        if measure(&excerpt(start, last)) > width {
            start = first - preceding;
        }
        let mut end = last
            + largest(count - last, |size| {
                measure(&excerpt(start, last + size)) <= width
            });
        start -= largest(start, |size| measure(&excerpt(start - size, end)) <= width);
        end += largest(count - end, |size| {
            measure(&excerpt(start, end + size)) <= width
        });
        excerpt(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn width(text: &str) -> f32 {
        text.chars().count() as f32
    }
    #[test]
    fn fits_short_text_and_reveals_late_hit_without_splitting_unicode() {
        let short = PreparedExcerpt::new("中文😀hit".into(), "HIT");
        assert_eq!(short.fit(6., width), "中文😀hit");
        assert_eq!(short.fit(0., width), "");
        let long = PreparedExcerpt::new("abcdefghijklmnop😀TARGET后续文字".into(), "target");
        let result = long.fit(14., width);
        assert!(result.contains("TARGET"));
        assert!(width(&result) <= 14.);
        assert!(result.starts_with('…'));
        assert_eq!(
            PreparedExcerpt::new("İABC".into(), "i").fit(2., width),
            "İ…"
        );
        assert_eq!(
            PreparedExcerpt::new("unchanged".into(), "missing").fit(1., width),
            "unchanged"
        );
    }
    #[test]
    fn long_body_never_measures_a_large_candidate_for_a_narrow_row() {
        let text = format!("{}needle{}", "x".repeat(1_000_000), "z".repeat(1_000_000));
        let prepared = PreparedExcerpt::new(text, "needle");
        let mut largest_probe = 0;
        let result = prepared.fit(40., |s| {
            largest_probe = largest_probe.max(s.len());
            width(s)
        });
        assert!(result.contains("needle"));
        assert!(width(&result) <= 40.);
        assert!(largest_probe < 100);
    }
}
