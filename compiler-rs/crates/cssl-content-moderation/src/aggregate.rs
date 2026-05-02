//! § aggregate — k-anon-aware ModerationAggregate.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THRESHOLDS  (PRIME-DIRECTIVE-CONST · SOVEREIGN-NUMBERED)
//!   T1 : SINGLE-FLAG-PRIVATE       — 1 flag visible only-to-flagger + admin
//!   T2 : AUTHOR-AGGREGATE-FLOOR    — author sees aggregate @ ≥ 3 flags
//!   T3 : NEEDS-REVIEW-THRESHOLD    — ≥ 10 distinct-flaggers AND
//!                                    severity-weighted-score ≥ 75
//!                                    ⟶ "needs-review" (NOT hidden)
//!
//! § NO-SHADOWBAN ATTESTATION
//!   visible_to_author bit FLIPS at T2 unconditionally — no algorithmic
//!   delay · no time-decay · no engagement-bait deflation.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::record::{FlagKind, FlagRecord};

/// T2 — author sees aggregate-summary at this floor.
pub const K_AUTHOR_AGGREGATE_FLOOR: u32 = 3;
/// T3 — distinct-flagger count for needs-review.
pub const K_NEEDS_REVIEW_DISTINCT: u32 = 10;
/// T3 — severity-weighted-score for needs-review (sum of severities / 10).
pub const K_NEEDS_REVIEW_WEIGHTED: u32 = 75;

/// § ModerationAggregate — k-anon-aware author-visible summary.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModerationAggregate {
    pub content_id: u32,
    pub total_flags: u32,
    pub distinct_flaggers: u32,
    pub severity_weighted: u32,
    pub per_kind_counts: [u32; 8],
    pub needs_review: bool,
    pub visible_to_author: bool,
    pub last_flag_ts: u32,
}

impl ModerationAggregate {
    /// Compute aggregate from a slice of FlagRecord entries · enforces k-anon.
    /// All records MUST share the same content_id (caller-side invariant).
    pub fn compute(flags: &[FlagRecord]) -> Self {
        if flags.is_empty() {
            return Self {
                content_id: 0,
                total_flags: 0,
                distinct_flaggers: 0,
                severity_weighted: 0,
                per_kind_counts: [0; 8],
                needs_review: false,
                visible_to_author: false,
                last_flag_ts: 0,
            };
        }
        let content_id = flags[0].content_id();
        let total = flags.len() as u32;
        let mut handles: HashSet<u64> = HashSet::with_capacity(flags.len());
        let mut per_kind = [0u32; 8];
        let mut weighted: u32 = 0;
        let mut last_ts: u32 = 0;
        for f in flags {
            handles.insert(f.flagger_pubkey_hash());
            per_kind[f.flag_kind() as usize] = per_kind[f.flag_kind() as usize].saturating_add(1);
            weighted = weighted.saturating_add(f.severity() as u32 / 10);
            if f.ts() > last_ts {
                last_ts = f.ts();
            }
        }
        let distinct = handles.len() as u32;
        let visible_to_author = total >= K_AUTHOR_AGGREGATE_FLOOR;
        let needs_review =
            distinct >= K_NEEDS_REVIEW_DISTINCT && weighted >= K_NEEDS_REVIEW_WEIGHTED;
        Self {
            content_id,
            total_flags: total,
            distinct_flaggers: distinct,
            severity_weighted: weighted,
            per_kind_counts: per_kind,
            needs_review,
            visible_to_author,
            last_flag_ts: last_ts,
        }
    }

    /// Per-kind helper — author-visible breakdown.
    pub fn count_for_kind(&self, kind: FlagKind) -> u32 {
        self.per_kind_counts[kind as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(handle: u64, kind: FlagKind, severity: u8, ts: u32) -> FlagRecord {
        FlagRecord::pack(handle, 7, kind, severity, 0x03, ts, 0, 0).unwrap()
    }

    #[test]
    fn empty_aggregate_invisible() {
        let a = ModerationAggregate::compute(&[]);
        assert!(!a.visible_to_author);
        assert!(!a.needs_review);
    }

    #[test]
    fn t2_floor_at_three_flags() {
        let flags = vec![
            mk(1, FlagKind::Spam, 10, 100),
            mk(2, FlagKind::Spam, 10, 101),
        ];
        let a = ModerationAggregate::compute(&flags);
        assert!(!a.visible_to_author, "2 flags below T2 floor");

        let mut three = flags.clone();
        three.push(mk(3, FlagKind::Spam, 10, 102));
        let a3 = ModerationAggregate::compute(&three);
        assert!(a3.visible_to_author, "3 flags == T2 floor");
        assert_eq!(a3.distinct_flaggers, 3);
    }

    #[test]
    fn t3_needs_review_requires_both() {
        // 10 distinct flaggers but low severity ⟶ NOT needs-review.
        let mut low = Vec::new();
        for i in 0..10u64 {
            low.push(mk(i + 1, FlagKind::Spam, 10, 100));
        }
        let a = ModerationAggregate::compute(&low);
        assert_eq!(a.distinct_flaggers, 10);
        // weighted = 10 * (10/10) = 10 < 75
        assert!(!a.needs_review);
        // 10 distinct flaggers + severity 80 each ⟶ weighted=80 ⟶ needs-review.
        let mut hi = Vec::new();
        for i in 0..10u64 {
            hi.push(mk(i + 1, FlagKind::PrimeDirectiveViolation, 80, 200));
        }
        let a2 = ModerationAggregate::compute(&hi);
        assert!(a2.needs_review);
        assert!(a2.visible_to_author);
    }
}
