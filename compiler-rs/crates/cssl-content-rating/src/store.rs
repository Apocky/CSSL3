//! § store — in-memory RatingStore.
//!
//! Stage-0 self-sufficient. Server-side persistence lives in
//! `cssl-supabase/migrations/0028_content_rating.sql` ; the same data-shape
//! flows in both directions. The store enforces :
//!   1. Σ-mask cap-gate at submit-time (CAP_RATE bit MUST be set)
//!   2. one-rating-per-(rater, content) — re-submit overwrites
//!   3. revoke = withdrawn-row + recompute aggregate
//!   4. read APIs return AggregateView (k-anon enforced) for non-rater readers

use crate::aggregate::AggregateView;
use crate::kan_bridge::QualitySignal;
use crate::rating::{Rating, RatingError};
use crate::review::Review;
use crate::CAP_RATE;
use std::collections::BTreeMap;

/// § StoreError — submit / revoke / lookup failure modes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("rating cap-gate failed : sigma_mask=0b{had:08b} ; need CAP_RATE=0b{required:08b}")]
    CapDenied { required: u8, had: u8 },
    #[error(transparent)]
    Rating(#[from] RatingError),
}

/// § RatingStore — BTreeMap<storage_key, (Rating, Option<Review>)>.
///
/// `BTreeMap` keeps iteration deterministic across replays — the substrate
/// invariant for cocreative-bias loops.
#[derive(Debug, Default, Clone)]
pub struct RatingStore {
    /// `storage_key` → (rating, optional review)
    rows: BTreeMap<u64, (Rating, Option<Review>)>,
}

impl RatingStore {
    /// § new — empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: BTreeMap::new(),
        }
    }

    /// § len — number of stored rows (incl. withdrawn).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// § is_empty — convenience.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// § submit — add or overwrite a rating row. Optional review attaches.
    /// Cap-gate : `rating.sigma_mask & CAP_RATE` MUST be set ; the validator
    /// in `Rating::new` already enforces this, so we only re-check
    /// defensively in case the caller skipped construction.
    pub fn submit(
        &mut self,
        rating: Rating,
        review: Option<Review>,
    ) -> Result<(), StoreError> {
        if rating.sigma_mask & CAP_RATE == 0 {
            return Err(StoreError::CapDenied {
                required: CAP_RATE,
                had: rating.sigma_mask,
            });
        }
        let key = rating.storage_key();
        self.rows.insert(key, (rating, review));
        Ok(())
    }

    /// § get_for_rater — return the rater's own row (private detail-view).
    /// The rater themselves can ALWAYS see their own row regardless of k.
    #[must_use]
    pub fn get_for_rater(
        &self,
        rater_pubkey_hash: u64,
        content_id: u32,
    ) -> Option<&(Rating, Option<Review>)> {
        self.rows
            .values()
            .find(|(r, _)| r.rater_pubkey_hash == rater_pubkey_hash && r.content_id == content_id)
    }

    /// § revoke — sovereign-revoke flow.
    ///
    ///   1. Replace the row with a `withdrawn()` Rating sentinel.
    ///   2. Drop the review-body if present.
    ///   3. Aggregate recompute happens lazily on next `aggregate_for` read.
    ///
    /// Idempotent : re-revoking a withdrawn row is a no-op.
    pub fn revoke(
        &mut self,
        rater_pubkey_hash: u64,
        content_id: u32,
        sigma_mask: u8,
        ts_minutes_since_epoch: u32,
    ) -> Result<(), StoreError> {
        let withdrawn = Rating::withdrawn(
            rater_pubkey_hash,
            content_id,
            sigma_mask,
            ts_minutes_since_epoch,
        )?;
        let key = withdrawn.storage_key();
        self.rows.insert(key, (withdrawn, None));
        Ok(())
    }

    /// § ratings_for — all rows (incl. withdrawn) for a content_id. Sorted
    /// by storage_key for deterministic order.
    #[must_use]
    pub fn ratings_for(&self, content_id: u32) -> Vec<Rating> {
        self.rows
            .values()
            .filter(|(r, _)| r.content_id == content_id)
            .map(|(r, _)| *r)
            .collect()
    }

    /// § aggregate_for — k-anon-enforced AggregateView for a content_id.
    #[must_use]
    pub fn aggregate_for(&self, content_id: u32) -> AggregateView {
        let rs = self.ratings_for(content_id);
        AggregateView::from_ratings(content_id, &rs)
    }

    /// § quality_signal_for — distill aggregate → KAN-bias-loop signal.
    /// Returns `None` when aggregate is `Hidden` (k-floor not met).
    #[must_use]
    pub fn quality_signal_for(&self, content_id: u32) -> Option<QualitySignal> {
        let rs = self.ratings_for(content_id);
        QualitySignal::from_ratings(content_id, &rs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::{tag_index, TagBitset};
    use crate::{CAP_AGGREGATE_PUBLIC, K_FLOOR_SINGLE};

    fn flag(name: &str) -> TagBitset {
        let mut t = TagBitset::EMPTY;
        t.set(tag_index(name).expect(name));
        t
    }

    fn submit_basic(s: &mut RatingStore, rater: u64, content: u32, stars: u8) {
        let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
        let r = Rating::new(rater, content, stars, flag("fun"), mask, 1_000, 200).expect("valid");
        s.submit(r, None).expect("submit ok");
    }

    #[test]
    fn submit_then_get_for_rater_roundtrips() {
        let mut s = RatingStore::new();
        submit_basic(&mut s, 1, 7, 5);
        let row = s
            .get_for_rater(1, 7)
            .expect("rater should see their own row");
        assert_eq!(row.0.stars, 5);
    }

    #[test]
    fn submit_overwrites_same_rater_content_pair() {
        let mut s = RatingStore::new();
        submit_basic(&mut s, 1, 7, 3);
        submit_basic(&mut s, 1, 7, 5);
        assert_eq!(s.len(), 1);
        assert_eq!(s.get_for_rater(1, 7).unwrap().0.stars, 5);
    }

    #[test]
    fn aggregate_hidden_for_lone_rater() {
        let mut s = RatingStore::new();
        submit_basic(&mut s, 1, 7, 5);
        let agg = s.aggregate_for(7);
        // distinct=1 < K_FLOOR_SINGLE=5
        assert!(matches!(
            agg.visibility,
            crate::aggregate::AggregateVisibility::Hidden
        ));
    }

    #[test]
    fn revoke_drops_aggregate_below_floor() {
        let mut s = RatingStore::new();
        for i in 0..K_FLOOR_SINGLE {
            submit_basic(&mut s, u64::from(i) + 1, 7, 5);
        }
        let agg_before = s.aggregate_for(7);
        assert!(agg_before.visibility.publicly_visible());

        // Revoke ONE → drops below floor → Hidden
        s.revoke(1, 7, CAP_RATE | CAP_AGGREGATE_PUBLIC, 2_000)
            .expect("revoke ok");
        let agg_after = s.aggregate_for(7);
        assert!(matches!(
            agg_after.visibility,
            crate::aggregate::AggregateVisibility::Hidden
        ));
    }

    #[test]
    fn revoke_is_idempotent() {
        let mut s = RatingStore::new();
        submit_basic(&mut s, 1, 7, 5);
        s.revoke(1, 7, CAP_RATE, 2_000).expect("first revoke ok");
        s.revoke(1, 7, CAP_RATE, 3_000).expect("second revoke ok");
        let row = s.get_for_rater(1, 7).expect("withdrawn row visible");
        assert!(row.0.is_withdrawn());
    }

    #[test]
    fn submit_rejects_zero_sigma_mask_via_rating_construction() {
        // Rating::new will reject sigma_mask=0 ; verify error surfaces.
        let err = Rating::new(1, 7, 5, TagBitset::EMPTY, 0, 1, 0)
            .expect_err("sigma_mask=0 must reject");
        assert!(matches!(err, RatingError::CapRateMissing(_)));
    }

    #[test]
    fn ratings_for_filters_to_content_id() {
        let mut s = RatingStore::new();
        submit_basic(&mut s, 1, 7, 5);
        submit_basic(&mut s, 2, 8, 4);
        let only_seven = s.ratings_for(7);
        assert_eq!(only_seven.len(), 1);
        assert_eq!(only_seven[0].content_id, 7);
    }

    #[test]
    fn quality_signal_emitted_when_k_floor_met() {
        let mut s = RatingStore::new();
        for i in 0..K_FLOOR_SINGLE {
            submit_basic(&mut s, u64::from(i) + 1, 7, 5);
        }
        let sig = s.quality_signal_for(7).expect("k met → signal emits");
        assert_eq!(sig.content_id, 7);
        assert_eq!(sig.distinct_rater_count, K_FLOOR_SINGLE);
    }

    #[test]
    fn quality_signal_suppressed_below_k_floor() {
        let mut s = RatingStore::new();
        submit_basic(&mut s, 1, 7, 5);
        assert!(s.quality_signal_for(7).is_none());
    }
}
