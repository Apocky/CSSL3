//! § kan_bridge — distill aggregate → QualitySignal → sibling W12-3
//! cssl-self-authoring-kan as bias-axes input.
//!
//! § THESIS
//!   Each k-anon-public rating-aggregate produces ONE QualitySignal that
//!   the KAN-substrate ingests as bias-axes. The KAN never sees individual
//!   rows ; only the post-k-floor aggregate axes.
//!
//! § BIAS AXES
//!   ─ stars_q8           : mean stars × 51 → [0, 255]
//!   ─ remix_worthy_count : raters who tagged remix-worthy
//!   ─ runtime_stable_q8  : proportion tagged runtime-stable * 255
//!   ─ welcoming_q8       : proportion tagged welcoming * 255
//!   ─ warning_count      : raters with ≤ 2★ AND no runtime-stable tag
//!     → these signal "recalibrate this content's source-axes" to the KAN

use crate::aggregate::{AggregateView, AggregateVisibility};
use crate::rating::Rating;
use crate::tags::tag_index;
use crate::CAP_AGGREGATE_PUBLIC;
use serde::{Deserialize, Serialize};

/// § QualitySignal — bias-axes vector for KAN-substrate consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualitySignal {
    pub content_id: u32,
    pub distinct_rater_count: u32,
    pub stars_q8: u8,
    pub remix_worthy_count: u32,
    pub runtime_stable_q8: u8,
    pub welcoming_q8: u8,
    pub warning_count: u32,
}

impl QualitySignal {
    /// § from_ratings — distill ratings (one content_id) → signal.
    /// Returns `None` when the aggregate is `Hidden` (k-floor not met) ;
    /// the KAN must NEVER receive sub-k-floor signals.
    #[must_use]
    pub fn from_ratings(content_id: u32, ratings: &[Rating]) -> Option<Self> {
        let agg = AggregateView::from_ratings(content_id, ratings);
        if !matches!(
            agg.visibility,
            AggregateVisibility::Visible | AggregateVisibility::Trending
        ) {
            return None;
        }
        Some(Self::derive(content_id, ratings, &agg))
    }

    /// § from_aggregate — when the caller already has the aggregate, derive
    /// the signal directly (avoids re-aggregation).
    #[must_use]
    pub fn from_aggregate(
        content_id: u32,
        ratings: &[Rating],
        aggregate: &AggregateView,
    ) -> Option<Self> {
        if !matches!(
            aggregate.visibility,
            AggregateVisibility::Visible | AggregateVisibility::Trending
        ) {
            return None;
        }
        Some(Self::derive(content_id, ratings, aggregate))
    }

    fn derive(content_id: u32, ratings: &[Rating], agg: &AggregateView) -> Self {
        let i_remix = tag_index("remix-worthy").expect("remix-worthy is canonical");
        let i_stable = tag_index("runtime-stable").expect("runtime-stable is canonical");
        let i_welcoming = tag_index("welcoming").expect("welcoming is canonical");

        let mut remix_worthy_count: u32 = 0;
        let mut runtime_stable_count: u32 = 0;
        let mut welcoming_count: u32 = 0;
        let mut warning_count: u32 = 0;
        let mut public_count: u32 = 0;

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
            public_count += 1;
            if r.tags_bitset.contains(i_remix) {
                remix_worthy_count += 1;
            }
            if r.tags_bitset.contains(i_stable) {
                runtime_stable_count += 1;
            }
            if r.tags_bitset.contains(i_welcoming) {
                welcoming_count += 1;
            }
            // Warning = low stars AND no runtime-stable tag → KAN recalibrate.
            if r.stars <= 2 && !r.tags_bitset.contains(i_stable) {
                warning_count += 1;
            }
        }

        let runtime_stable_q8 = if public_count == 0 {
            0u8
        } else {
            (u64::from(runtime_stable_count) * 255 / u64::from(public_count)).min(255) as u8
        };
        let welcoming_q8 = if public_count == 0 {
            0u8
        } else {
            (u64::from(welcoming_count) * 255 / u64::from(public_count)).min(255) as u8
        };

        Self {
            content_id,
            distinct_rater_count: agg.distinct_rater_count,
            stars_q8: agg.mean_stars_q8,
            remix_worthy_count,
            runtime_stable_q8,
            welcoming_q8,
            warning_count,
        }
    }

    /// § is_strong_positive — heuristic for KAN-bias amplification.
    /// 5★ + ≥ 1 remix-worthy → strong positive signal.
    #[must_use]
    pub fn is_strong_positive(&self) -> bool {
        self.stars_q8 >= 230 && self.remix_worthy_count > 0
    }

    /// § needs_recalibration — heuristic for KAN-source axis recalibration.
    /// Multiple low-star + no-runtime-stable warnings → recalibrate source.
    #[must_use]
    pub fn needs_recalibration(&self) -> bool {
        self.warning_count >= 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::{tag_index, TagBitset};
    use crate::{CAP_RATE, K_FLOOR_SINGLE};

    fn rating(rater: u64, content: u32, stars: u8, tag: Option<&str>) -> Rating {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let mut tags = TagBitset::EMPTY;
        if let Some(name) = tag {
            tags.set(tag_index(name).unwrap());
        }
        Rating::new(rater, content, stars, tags, mask, 1_000, 200).expect("valid")
    }

    #[test]
    fn signal_suppressed_below_k_floor() {
        let rs = vec![rating(1, 7, 5, Some("remix-worthy"))];
        assert!(QualitySignal::from_ratings(7, &rs).is_none());
    }

    #[test]
    fn signal_derived_at_k_floor() {
        let mut rs: Vec<Rating> = (0..K_FLOOR_SINGLE)
            .map(|i| rating(u64::from(i) + 1, 7, 5, Some("remix-worthy")))
            .collect();
        // Ensure all 5 raters have remix-worthy tag.
        let sig = QualitySignal::from_ratings(7, &rs).expect("k met");
        assert_eq!(sig.distinct_rater_count, K_FLOOR_SINGLE);
        assert_eq!(sig.remix_worthy_count, K_FLOOR_SINGLE);
        // 5 stars → mean=5 → q8=255
        assert_eq!(sig.stars_q8, 255);
        assert!(sig.is_strong_positive());
        assert!(!sig.needs_recalibration());
        // touch rs to silence unused-mut
        rs.push(rating(99, 999, 1, None));
    }

    #[test]
    fn signal_warning_count_for_low_stars_no_stable_tag() {
        // 5 raters, all 1-star, none tagged runtime-stable
        let rs: Vec<Rating> = (0..K_FLOOR_SINGLE)
            .map(|i| rating(u64::from(i) + 1, 7, 1, None))
            .collect();
        let sig = QualitySignal::from_ratings(7, &rs).expect("k met");
        assert_eq!(sig.warning_count, K_FLOOR_SINGLE);
        assert!(sig.needs_recalibration());
        assert!(!sig.is_strong_positive());
    }

    #[test]
    fn signal_runtime_stable_q8_proportional() {
        // 5 raters ; 4 tag runtime-stable
        let mut rs: Vec<Rating> = Vec::new();
        for i in 0u64..4u64 {
            rs.push(rating(i + 1, 7, 5, Some("runtime-stable")));
        }
        rs.push(rating(99, 7, 5, None));
        let sig = QualitySignal::from_ratings(7, &rs).expect("k met");
        // 4/5 = 80% ; q8 = 4*255/5 = 204
        assert_eq!(sig.runtime_stable_q8, 204);
    }

    #[test]
    fn signal_welcoming_q8_proportional() {
        let mut rs: Vec<Rating> = Vec::new();
        for i in 0..K_FLOOR_SINGLE {
            rs.push(rating(u64::from(i) + 1, 7, 5, Some("welcoming")));
        }
        let sig = QualitySignal::from_ratings(7, &rs).expect("k met");
        // 5/5 = 255
        assert_eq!(sig.welcoming_q8, 255);
    }
}
