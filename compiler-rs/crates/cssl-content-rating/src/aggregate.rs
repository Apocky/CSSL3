//! § aggregate — k-anonymized aggregate per content_id.

use crate::rating::Rating;
use crate::tags::{TAG_NAMES, TAG_TOTAL};
use crate::{CAP_AGGREGATE_PUBLIC, K_FLOOR_SINGLE, K_FLOOR_TRENDING};
use serde::{Deserialize, Serialize};

/// § AggregateVisibility — three-state visibility under k-floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AggregateVisibility {
    /// `< K_FLOOR_SINGLE` distinct raters → invisible to non-rater readers.
    #[default]
    Hidden,
    /// `≥ K_FLOOR_SINGLE` raters → visible on content page.
    Visible,
    /// `≥ K_FLOOR_TRENDING` raters → eligible for trending-rank influence.
    Trending,
}

impl AggregateVisibility {
    /// § from_count — bucket distinct-rater count → visibility.
    #[must_use]
    pub fn from_count(distinct_raters: u32) -> Self {
        if distinct_raters >= K_FLOOR_TRENDING {
            Self::Trending
        } else if distinct_raters >= K_FLOOR_SINGLE {
            Self::Visible
        } else {
            Self::Hidden
        }
    }

    /// § publicly_visible — true iff aggregate exposes details to non-raters.
    #[must_use]
    pub fn publicly_visible(&self) -> bool {
        !matches!(self, Self::Hidden)
    }

    /// § eligible_for_trending — true iff content can sway trending rank.
    #[must_use]
    pub fn eligible_for_trending(&self) -> bool {
        matches!(self, Self::Trending)
    }
}

/// § AggregateView — public-readable view per content_id (post-k-floor).
///
/// IMPORTANT : when `visibility == Hidden` only `content_id +
/// distinct_rater_count + visibility` are exposed. The `mean_stars_q8` and
/// `tag_counts` fields are zeroed defensively so a malformed serializer
/// cannot leak below-floor data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateView {
    pub content_id: u32,
    /// Distinct (non-withdrawn) raters that contributed.
    pub distinct_rater_count: u32,
    /// Mean stars * 51 → quantized u8 in [0, 255]. Zero when Hidden.
    pub mean_stars_q8: u8,
    /// Per-tag distinct-rater count. All-zero when Hidden.
    pub tag_counts: [u32; TAG_TOTAL],
    /// Visibility tier (Hidden / Visible / Trending).
    pub visibility: AggregateVisibility,
}

impl AggregateView {
    /// § for_content_only — zero-data shell used when `Hidden`.
    #[must_use]
    pub fn for_content_only(content_id: u32, distinct_rater_count: u32) -> Self {
        Self {
            content_id,
            distinct_rater_count,
            mean_stars_q8: 0,
            tag_counts: [0u32; TAG_TOTAL],
            visibility: AggregateVisibility::from_count(distinct_rater_count),
        }
    }

    /// § from_ratings — build an aggregate from a slice of ratings ALREADY
    /// pre-filtered to one `content_id`. Withdrawn rows (stars==0) are
    /// excluded from the public count. Rows missing `CAP_AGGREGATE_PUBLIC`
    /// are excluded from BOTH the count and the means/tag-counts (so a
    /// moderator-suppressed row cannot tip the k-floor either way).
    #[must_use]
    pub fn from_ratings(content_id: u32, ratings: &[Rating]) -> Self {
        let mut sum_stars: u64 = 0;
        let mut count_for_aggregate: u32 = 0;
        let mut tag_counts = [0u32; TAG_TOTAL];

        for r in ratings {
            if r.content_id != content_id {
                continue;
            }
            if r.is_withdrawn() {
                continue;
            }
            if r.sigma_mask & CAP_AGGREGATE_PUBLIC == 0 {
                continue;
            }
            sum_stars += u64::from(r.stars);
            count_for_aggregate += 1;
            for (i, _) in r.tags_bitset.iter_set() {
                tag_counts[i] += 1;
            }
        }

        let visibility = AggregateVisibility::from_count(count_for_aggregate);
        if !visibility.publicly_visible() {
            return Self::for_content_only(content_id, count_for_aggregate);
        }

        let mean_stars = if count_for_aggregate == 0 {
            0u64
        } else {
            sum_stars * 51 / u64::from(count_for_aggregate)
        };
        let mean_stars_q8 = mean_stars.min(255) as u8;

        Self {
            content_id,
            distinct_rater_count: count_for_aggregate,
            mean_stars_q8,
            tag_counts,
            visibility,
        }
    }

    /// § top_tags — top `n` tags by count ; ties broken by canonical-order
    /// (earlier index first). Returns names + counts. Empty when Hidden.
    #[must_use]
    pub fn top_tags(&self, n: usize) -> Vec<(&'static str, u32)> {
        if !self.visibility.publicly_visible() {
            return Vec::new();
        }
        let mut indexed: Vec<(usize, u32)> = self
            .tag_counts
            .iter()
            .enumerate()
            .filter(|(_, &c)| c > 0)
            .map(|(i, &c)| (i, c))
            .collect();
        // Sort descending by count ; stable so earlier-index wins on ties.
        indexed.sort_by(|a, b| b.1.cmp(&a.1));
        indexed
            .into_iter()
            .take(n)
            .map(|(i, c)| (TAG_NAMES[i], c))
            .collect()
    }

    /// § mean_stars — recover the mean as f32 in [0, 5].
    #[must_use]
    pub fn mean_stars(&self) -> f32 {
        f32::from(self.mean_stars_q8) / 51.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::{tag_index, TagBitset};
    use crate::CAP_RATE;

    fn rating(rater: u64, content: u32, stars: u8, mask: u8, tags: TagBitset) -> Rating {
        Rating::new(rater, content, stars, tags, mask, 1_000, 200).expect("valid")
    }

    fn flag(name: &str) -> TagBitset {
        let mut t = TagBitset::EMPTY;
        t.set(tag_index(name).expect(name));
        t
    }

    #[test]
    fn aggregate_hidden_below_k_floor_single() {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let mut rs = Vec::new();
        for i in 0..(K_FLOOR_SINGLE - 1) {
            rs.push(rating(u64::from(i) + 1, 7, 4, mask, TagBitset::EMPTY));
        }
        let agg = AggregateView::from_ratings(7, &rs);
        assert!(matches!(agg.visibility, AggregateVisibility::Hidden));
        assert_eq!(agg.mean_stars_q8, 0);
        assert!(agg.tag_counts.iter().all(|&c| c == 0));
        assert_eq!(agg.distinct_rater_count, K_FLOOR_SINGLE - 1);
    }

    #[test]
    fn aggregate_visible_at_k_floor_single() {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let rs: Vec<Rating> = (0..K_FLOOR_SINGLE)
            .map(|i| rating(u64::from(i) + 1, 7, 5, mask, TagBitset::EMPTY))
            .collect();
        let agg = AggregateView::from_ratings(7, &rs);
        assert!(matches!(agg.visibility, AggregateVisibility::Visible));
        // mean = 5 → q8 = 255
        assert_eq!(agg.mean_stars_q8, 255);
        assert_eq!(agg.distinct_rater_count, K_FLOOR_SINGLE);
    }

    #[test]
    fn aggregate_trending_at_k_floor_trending() {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let rs: Vec<Rating> = (0..K_FLOOR_TRENDING)
            .map(|i| rating(u64::from(i) + 1, 7, 4, mask, flag("fun")))
            .collect();
        let agg = AggregateView::from_ratings(7, &rs);
        assert!(matches!(agg.visibility, AggregateVisibility::Trending));
        assert!(agg.visibility.eligible_for_trending());
        let top = agg.top_tags(3);
        assert_eq!(top[0].0, "fun");
    }

    #[test]
    fn aggregate_excludes_withdrawn_from_count() {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let mut rs: Vec<Rating> = (0..(K_FLOOR_SINGLE - 1))
            .map(|i| rating(u64::from(i) + 1, 7, 4, mask, TagBitset::EMPTY))
            .collect();
        // 5th row is withdrawn → does NOT count, stays Hidden.
        rs.push(Rating::withdrawn(99, 7, mask, 1_000).expect("withdraw ok"));
        let agg = AggregateView::from_ratings(7, &rs);
        assert!(matches!(agg.visibility, AggregateVisibility::Hidden));
    }

    #[test]
    fn aggregate_excludes_rows_missing_cap_aggregate_public() {
        let mask_pub = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let mask_priv = CAP_RATE; // missing CAP_AGGREGATE_PUBLIC
        let mut rs: Vec<Rating> = (0..K_FLOOR_SINGLE)
            .map(|i| rating(u64::from(i) + 1, 7, 4, mask_priv, TagBitset::EMPTY))
            .collect();
        // Five private rows → all dropped → still Hidden.
        let agg = AggregateView::from_ratings(7, &rs);
        assert!(matches!(agg.visibility, AggregateVisibility::Hidden));
        // Add 5 public → flips to Visible.
        for i in 100..(100 + K_FLOOR_SINGLE) {
            rs.push(rating(u64::from(i), 7, 4, mask_pub, TagBitset::EMPTY));
        }
        let agg2 = AggregateView::from_ratings(7, &rs);
        assert!(matches!(agg2.visibility, AggregateVisibility::Visible));
        assert_eq!(agg2.distinct_rater_count, K_FLOOR_SINGLE);
    }

    #[test]
    fn top_tags_returns_descending_by_count() {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let mut rs: Vec<Rating> = Vec::new();
        // 6 raters tag fun
        for i in 0u64..6u64 {
            rs.push(rating(i + 1, 7, 5, mask, flag("fun")));
        }
        // 2 raters tag novel
        for i in 6u64..8u64 {
            rs.push(rating(i + 1, 7, 4, mask, flag("novel")));
        }
        let agg = AggregateView::from_ratings(7, &rs);
        let top = agg.top_tags(2);
        assert_eq!(top[0], ("fun", 6));
        assert_eq!(top[1], ("novel", 2));
    }

    #[test]
    fn aggregate_filters_other_content_ids() {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let mut rs: Vec<Rating> = (0..K_FLOOR_SINGLE)
            .map(|i| rating(u64::from(i) + 1, 7, 5, mask, TagBitset::EMPTY))
            .collect();
        rs.push(rating(99, 999, 1, mask, TagBitset::EMPTY));
        let agg = AggregateView::from_ratings(7, &rs);
        assert_eq!(agg.distinct_rater_count, K_FLOOR_SINGLE);
        // 7's mean unchanged at 5 → 255 q8.
        assert_eq!(agg.mean_stars_q8, 255);
    }
}
