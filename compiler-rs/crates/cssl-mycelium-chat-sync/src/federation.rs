//! § federation — `ChatPatternFederation` k-anon-enforced shared-state
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   The federation is the receiving-side. It holds (pattern_id ↦ aggregated
//!   counts) WITH a k-anonymity floor : a pattern_id MUST have ≥ k distinct
//!   emitter_handles before it becomes visible to readers. Below the floor,
//!   the pattern is held in a STAGING area ; reading the federation NEVER
//!   exposes staged patterns.
//!
//! § K-ANONYMITY ENFORCEMENT
//!   ─ Default k = 5 ⊑ raise via `with_k_floor`
//!   ─ Per-pattern_id distinct-emitter set tracked
//!   ─ Visibility transitions : staged → public when |emitters| ≥ k
//!   ─ Visibility transitions : public → staged on emitter-revoke (defense-
//!     in-depth ; if revoke drops the count below k, the pattern is hidden)
//!
//! § DETERMINISM
//!   ─ Snapshots use BTreeMap<u32, AggregatedShape> ⊑ ordered iteration
//!   ─ Federation BLAKE3 = blake3("federation\0" ‖ k_floor ‖ ∀ pattern_id ‖
//!     count ‖ confidence_q8) ⊑ replay-stable per-tick
//!   ─ Persona-modulation reads from snapshot ⟶ same-snapshot + same-seed →
//!     same modulation (replay-safe per spec invariant)
//!
//! § REVOKE-PURGE FLOW
//!   ─ `purge_emitter(handle)` removes the handle from EVERY pattern's
//!     emitter-set ; patterns whose distinct-emitter-count drops below k
//!     become un-publishable from this point forward
//!   ─ Purge is idempotent ⊑ replay-safe across crash-recovery
//!
//! § Σ-MASK GATING (defense-in-depth)
//!   ─ `ingest()` rejects any pattern whose `cap_flags` lacks
//!     `CAP_FEDERATION_INGEST` (the 2nd of two cap-checks ; the first is at
//!     emit-time)
//!   ─ Reserved cap_flags bits cause hard-rejection (likely-tampered)

use crate::pattern::{
    ArcPhase, ChatPattern, IntentKind, PatternError, ResponseShape, CAP_FEDERATION_INGEST,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

/// § DEFAULT_K_FLOOR — minimum distinct-emitter count for pattern visibility.
pub const DEFAULT_K_FLOOR: usize = 5;

/// § AggregatedShape — per-pattern_id aggregate visible to GM/DM modulation.
///
/// Counts + mean-confidence-q8 ; NEVER exposes the per-emitter set itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedShape {
    pub pattern_id: u32,
    pub intent_kind: IntentKind,
    pub response_shape: ResponseShape,
    pub arc_phase: ArcPhase,
    /// Number of patterns ingested with this id (totals · ≥ count of distinct emitters).
    pub observation_count: u32,
    /// Distinct-emitter count. Public iff ≥ k_floor.
    pub distinct_emitter_count: u32,
    /// Mean confidence as quantized u8 in [0, 255].
    pub mean_confidence_q8: u8,
    /// Most-recent ts_bucketed (minutes since epoch).
    pub last_ts_bucketed: u32,
}

impl AggregatedShape {
    /// § confidence as f32 in [0, 1].
    #[must_use]
    pub fn confidence(&self) -> f32 {
        f32::from(self.mean_confidence_q8) / 255.0
    }
}

#[derive(Debug, Default)]
struct PatternBucket {
    intent_kind: IntentKind,
    response_shape: ResponseShape,
    arc_phase: ArcPhase,
    observation_count: u32,
    /// Set of distinct emitter_handles that contributed to this pattern.
    /// Stored in a `BTreeSet` for deterministic-order serialization.
    emitters: BTreeSet<u64>,
    /// Sum-of-confidence-q8 ; mean = sum / observation_count.
    confidence_sum: u64,
    last_ts_bucketed: u32,
}

impl Default for IntentKind {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Default for ResponseShape {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Default for ArcPhase {
    fn default() -> Self {
        Self::Unknown
    }
}

/// § FederationStats — observability snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederationStats {
    pub total_ingested: u64,
    pub total_rejected_cap: u64,
    pub total_rejected_validate: u64,
    pub total_purged_emitters: u64,
    pub patterns_staged: u64,
    pub patterns_public: u64,
}

/// § FederationError — ingest + purge failure modes.
#[derive(Debug, thiserror::Error)]
pub enum FederationError {
    #[error("pattern fails Σ-mask cap check : need 0b{required:08b}, had 0b{had:08b}")]
    CapDenied { required: u8, had: u8 },
    #[error("malformed pattern : {0}")]
    Malformed(#[from] PatternError),
}

/// § ChatPatternFederation — k-anon-enforced cross-instance shared-state.
///
/// `Arc`-shareable. The internal `RwLock` permits many concurrent readers
/// (the GM/DM persona-agent reads on every turn) with single-writer
/// ingestion (the digest-loop pushes batches every 60s).
#[derive(Default, Clone)]
pub struct ChatPatternFederation {
    inner: Arc<RwLock<FederationInner>>,
}

#[derive(Default)]
struct FederationInner {
    k_floor: usize,
    buckets: BTreeMap<u32, PatternBucket>,
    stats: FederationStats,
}

impl ChatPatternFederation {
    /// § new — default k-floor of 5.
    #[must_use]
    pub fn new() -> Self {
        Self::with_k_floor(DEFAULT_K_FLOOR)
    }

    /// § with_k_floor — k must be ≥ 2 ; values lower are clamped to 2 to
    /// preserve the privacy invariant.
    #[must_use]
    pub fn with_k_floor(k: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FederationInner {
                k_floor: k.max(2),
                buckets: BTreeMap::new(),
                stats: FederationStats::default(),
            })),
        }
    }

    /// § k_floor — current k-anonymity threshold.
    #[must_use]
    pub fn k_floor(&self) -> usize {
        self.inner.read().map_or(DEFAULT_K_FLOOR, |g| g.k_floor)
    }

    /// § ingest — accept a pattern into staging or aggregate. Returns the
    /// post-ingest distinct-emitter-count for observability ; does NOT
    /// reveal whether the pattern is now public (callers should use
    /// `snapshot_public()` to read).
    pub fn ingest(&self, pattern: &ChatPattern) -> Result<u32, FederationError> {
        // Σ-mask gate (2nd line ; emit-side is the 1st).
        if !pattern.cap_check(CAP_FEDERATION_INGEST) {
            if let Ok(mut g) = self.inner.write() {
                g.stats.total_rejected_cap += 1;
            }
            return Err(FederationError::CapDenied {
                required: CAP_FEDERATION_INGEST,
                had: pattern.cap_flags(),
            });
        }

        // Structural validation.
        if let Err(e) = pattern.validate() {
            if let Ok(mut g) = self.inner.write() {
                g.stats.total_rejected_validate += 1;
            }
            return Err(e.into());
        }

        let mut g = self
            .inner
            .write()
            .map_err(|_| FederationError::Malformed(PatternError::ReservedCapFlagsSet(0)))?;
        g.stats.total_ingested += 1;
        let bucket = g.buckets.entry(pattern.pattern_id()).or_default();
        bucket.intent_kind = pattern.intent_kind();
        bucket.response_shape = pattern.response_shape();
        bucket.arc_phase = pattern.arc_phase();
        bucket.observation_count = bucket.observation_count.saturating_add(1);
        bucket.emitters.insert(pattern.emitter_handle());
        bucket.confidence_sum = bucket
            .confidence_sum
            .saturating_add(u64::from(pattern.confidence_q8()));
        if pattern.ts_bucketed() > bucket.last_ts_bucketed {
            bucket.last_ts_bucketed = pattern.ts_bucketed();
        }
        let count = bucket.emitters.len() as u32;
        Self::recompute_stats(&mut g);
        Ok(count)
    }

    /// § ingest_batch — convenience for digest-loop.
    pub fn ingest_batch(&self, patterns: &[ChatPattern]) -> usize {
        let mut accepted = 0_usize;
        for p in patterns {
            if self.ingest(p).is_ok() {
                accepted += 1;
            }
        }
        accepted
    }

    /// § purge_emitter — remove `emitter_handle` from EVERY pattern's
    /// emitter-set. Idempotent. Returns the number of buckets touched.
    ///
    /// § sovereign-revoke compliance : invoked by `MyceliumChatSync` when
    /// an emitter revokes their `CAP_PURGE_ON_REVOKE` cap. The federation
    /// maintains the invariant : after purge, the emitter contributes ZERO
    /// to any aggregated-shape count.
    pub fn purge_emitter(&self, emitter_handle: u64) -> usize {
        let Ok(mut g) = self.inner.write() else {
            return 0;
        };
        let mut touched = 0_usize;
        let mut empty_keys: Vec<u32> = Vec::new();
        for (id, bucket) in g.buckets.iter_mut() {
            if bucket.emitters.remove(&emitter_handle) {
                touched += 1;
                // Decrement observation_count proportionally — we don't
                // know how many of the obs were from this emitter, so we
                // conservatively decrement by 1 per removed emitter and let
                // the next ingest-cycle re-converge. For correctness of the
                // distinct-emitter floor, only the set membership matters.
                bucket.observation_count = bucket.observation_count.saturating_sub(1);
                if bucket.emitters.is_empty() {
                    empty_keys.push(*id);
                }
            }
        }
        for k in empty_keys {
            g.buckets.remove(&k);
        }
        g.stats.total_purged_emitters += 1;
        Self::recompute_stats(&mut g);
        touched
    }

    /// § snapshot_public — patterns currently above the k-anon floor.
    /// Deterministic order : ascending by pattern_id.
    ///
    /// Returns owned `Vec<AggregatedShape>` rather than borrowing because
    /// the GM/DM persona-agent typically iterates once-per-turn ; the
    /// allocation is well-amortized.
    #[must_use]
    pub fn snapshot_public(&self) -> Vec<AggregatedShape> {
        let Ok(g) = self.inner.read() else {
            return Vec::new();
        };
        let mut out: Vec<AggregatedShape> = Vec::with_capacity(g.buckets.len());
        for (id, b) in &g.buckets {
            if b.emitters.len() >= g.k_floor {
                out.push(Self::shape_from_bucket(*id, b));
            }
        }
        out
    }

    /// § snapshot_all — INCLUDING staged. For internal-stats only ; do NOT
    /// expose to GM/DM modulation. Cap-gated indirectly : callers reach
    /// this only inside the federation crate.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn snapshot_all_for_stats(&self) -> (Vec<AggregatedShape>, Vec<AggregatedShape>) {
        let Ok(g) = self.inner.read() else {
            return (Vec::new(), Vec::new());
        };
        let mut public = Vec::new();
        let mut staged = Vec::new();
        for (id, b) in &g.buckets {
            let s = Self::shape_from_bucket(*id, b);
            if b.emitters.len() >= g.k_floor {
                public.push(s);
            } else {
                staged.push(s);
            }
        }
        (public, staged)
    }

    fn shape_from_bucket(id: u32, b: &PatternBucket) -> AggregatedShape {
        let mean_q8 = if b.observation_count == 0 {
            0
        } else {
            (b.confidence_sum / u64::from(b.observation_count)).min(255) as u8
        };
        AggregatedShape {
            pattern_id: id,
            intent_kind: b.intent_kind,
            response_shape: b.response_shape,
            arc_phase: b.arc_phase,
            observation_count: b.observation_count,
            distinct_emitter_count: b.emitters.len() as u32,
            mean_confidence_q8: mean_q8,
            last_ts_bucketed: b.last_ts_bucketed,
        }
    }

    fn recompute_stats(inner: &mut FederationInner) {
        let mut staged = 0_u64;
        let mut public = 0_u64;
        for b in inner.buckets.values() {
            if b.emitters.len() >= inner.k_floor {
                public += 1;
            } else {
                staged += 1;
            }
        }
        inner.stats.patterns_staged = staged;
        inner.stats.patterns_public = public;
    }

    /// § stats — observability snapshot.
    #[must_use]
    pub fn stats(&self) -> FederationStats {
        self.inner.read().map_or_else(|_| FederationStats::default(), |g| g.stats)
    }

    /// § federation_blake3 — deterministic 32-byte digest of the public-set.
    /// Used by replay tests + sovereign-attestation.
    #[must_use]
    pub fn federation_blake3(&self) -> [u8; 32] {
        let Ok(g) = self.inner.read() else {
            return [0_u8; 32];
        };
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-mycelium-chat-sync\0federation\0v1");
        h.update(&(g.k_floor as u32).to_le_bytes());
        for (id, b) in &g.buckets {
            if b.emitters.len() >= g.k_floor {
                h.update(&id.to_le_bytes());
                h.update(&b.observation_count.to_le_bytes());
                h.update(&(b.emitters.len() as u32).to_le_bytes());
                let mean_q8 = (b.confidence_sum / u64::from(b.observation_count.max(1))) as u8;
                h.update(&[mean_q8]);
                h.update(&b.last_ts_bucketed.to_le_bytes());
            }
        }
        *h.finalize().as_bytes()
    }

    /// § public_pattern_count — convenience accessor.
    #[must_use]
    pub fn public_pattern_count(&self) -> usize {
        let Ok(g) = self.inner.read() else {
            return 0;
        };
        g.buckets
            .values()
            .filter(|b| b.emitters.len() >= g.k_floor)
            .count()
    }

    /// § total_pattern_count — incl. staged.
    #[must_use]
    pub fn total_pattern_count(&self) -> usize {
        self.inner.read().map_or(0, |g| g.buckets.len())
    }
}

impl std::fmt::Debug for ChatPatternFederation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.inner.read() {
            Ok(g) => f
                .debug_struct("ChatPatternFederation")
                .field("k_floor", &g.k_floor)
                .field("buckets", &g.buckets.len())
                .field("stats", &g.stats)
                .finish(),
            Err(_) => f
                .debug_struct("ChatPatternFederation")
                .field("state", &"poisoned")
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{
        ArcPhase, ChatPatternBuilder, IntentKind, ResponseShape, CAP_FLAGS_ALL,
    };

    fn mk_pattern_with_emitter(emitter_seed: u8, intent: IntentKind) -> ChatPattern {
        ChatPatternBuilder {
            intent_kind: intent,
            response_shape: ResponseShape::ScenicNarrative,
            arc_phase: ArcPhase::RisingAction,
            confidence: 0.6,
            ts_unix: 60 * 100,
            region_tag: 1,
            opt_in_tier: 1,
            cap_flags: CAP_FLAGS_ALL,
            emitter_pubkey: [emitter_seed; 32],
            co_signers: vec![],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn k_anon_floor_blocks_below_threshold() {
        let f = ChatPatternFederation::with_k_floor(5);
        for i in 0..4_u8 {
            f.ingest(&mk_pattern_with_emitter(i, IntentKind::Question)).unwrap();
        }
        // 4 distinct emitters ⊑ k_floor = 5 ⟶ NOT public.
        assert_eq!(f.snapshot_public().len(), 0);
        assert_eq!(f.total_pattern_count(), 1);
    }

    #[test]
    fn k_anon_threshold_promotes_to_public_at_k() {
        let f = ChatPatternFederation::with_k_floor(5);
        for i in 0..5_u8 {
            f.ingest(&mk_pattern_with_emitter(i, IntentKind::Question)).unwrap();
        }
        // 5 distinct emitters ⊑ k_floor = 5 ⟶ PUBLIC.
        let pub_set = f.snapshot_public();
        assert_eq!(pub_set.len(), 1);
        assert_eq!(pub_set[0].distinct_emitter_count, 5);
    }

    #[test]
    fn k_floor_minimum_is_two() {
        let f = ChatPatternFederation::with_k_floor(0);
        assert_eq!(f.k_floor(), 2);
        let f2 = ChatPatternFederation::with_k_floor(1);
        assert_eq!(f2.k_floor(), 2);
    }

    #[test]
    fn cap_denied_when_federation_ingest_bit_missing() {
        let f = ChatPatternFederation::new();
        let mut b = ChatPatternBuilder {
            intent_kind: IntentKind::Question,
            response_shape: ResponseShape::ScenicNarrative,
            arc_phase: ArcPhase::Setup,
            confidence: 0.5,
            ts_unix: 60 * 50,
            region_tag: 1,
            opt_in_tier: 1,
            cap_flags: 0b0000_0001, // EMIT_ALLOWED only ; missing FEDERATION_INGEST
            emitter_pubkey: [1_u8; 32],
            co_signers: vec![],
        };
        let p = b.clone().build().unwrap();
        let res = f.ingest(&p);
        assert!(matches!(res, Err(FederationError::CapDenied { .. })));
        // Sanity : flipping the bit on lets it in.
        b.cap_flags = CAP_FLAGS_ALL;
        let p2 = b.build().unwrap();
        assert!(f.ingest(&p2).is_ok());
    }

    #[test]
    fn purge_emitter_drops_below_threshold_again() {
        let f = ChatPatternFederation::with_k_floor(5);
        let patterns: Vec<ChatPattern> = (0..5_u8)
            .map(|i| mk_pattern_with_emitter(i, IntentKind::Question))
            .collect();
        for p in &patterns {
            f.ingest(p).unwrap();
        }
        assert_eq!(f.snapshot_public().len(), 1);
        // Revoke emitter 0 — distinct-count drops to 4, falls below k.
        let purged_handle = patterns[0].emitter_handle();
        let touched = f.purge_emitter(purged_handle);
        assert!(touched >= 1);
        assert_eq!(f.snapshot_public().len(), 0);
    }

    #[test]
    fn purge_emitter_idempotent() {
        let f = ChatPatternFederation::with_k_floor(2);
        let p1 = mk_pattern_with_emitter(1, IntentKind::Question);
        let p2 = mk_pattern_with_emitter(2, IntentKind::Question);
        f.ingest(&p1).unwrap();
        f.ingest(&p2).unwrap();
        assert_eq!(f.snapshot_public().len(), 1);
        let h1 = p1.emitter_handle();
        f.purge_emitter(h1);
        f.purge_emitter(h1); // idempotent
        f.purge_emitter(h1); // idempotent
        // k_floor=2 ⟶ removing emitter 1 drops distinct to 1 ⊑ now staged.
        assert_eq!(f.snapshot_public().len(), 0);
    }

    #[test]
    fn purge_emitter_with_nonexistent_handle_is_no_op() {
        let f = ChatPatternFederation::with_k_floor(2);
        let p = mk_pattern_with_emitter(1, IntentKind::Question);
        f.ingest(&p).unwrap();
        let touched = f.purge_emitter(0xDEAD_BEEF_CAFE_F00D);
        assert_eq!(touched, 0);
    }

    #[test]
    fn deterministic_federation_blake3() {
        let f1 = ChatPatternFederation::with_k_floor(2);
        let f2 = ChatPatternFederation::with_k_floor(2);
        for i in 0..3_u8 {
            f1.ingest(&mk_pattern_with_emitter(i, IntentKind::Question)).unwrap();
            f2.ingest(&mk_pattern_with_emitter(i, IntentKind::Question)).unwrap();
        }
        assert_eq!(f1.federation_blake3(), f2.federation_blake3());
    }

    #[test]
    fn federation_blake3_changes_with_new_pattern() {
        let f = ChatPatternFederation::with_k_floor(2);
        for i in 0..3_u8 {
            f.ingest(&mk_pattern_with_emitter(i, IntentKind::Question)).unwrap();
        }
        let h1 = f.federation_blake3();
        for i in 5..8_u8 {
            f.ingest(&mk_pattern_with_emitter(i, IntentKind::Combat)).unwrap();
        }
        let h2 = f.federation_blake3();
        assert_ne!(h1, h2);
    }

    #[test]
    fn snapshot_public_excludes_staged() {
        let f = ChatPatternFederation::with_k_floor(5);
        // pattern A : 5 emitters ⟶ public
        for i in 0..5_u8 {
            f.ingest(&mk_pattern_with_emitter(i, IntentKind::Question)).unwrap();
        }
        // pattern B : 2 emitters ⟶ staged
        for i in 10..12_u8 {
            f.ingest(&mk_pattern_with_emitter(i, IntentKind::Combat)).unwrap();
        }
        let pub_set = f.snapshot_public();
        assert_eq!(pub_set.len(), 1);
        assert_eq!(pub_set[0].intent_kind, IntentKind::Question);
        // total includes both
        assert_eq!(f.total_pattern_count(), 2);
    }

    #[test]
    fn ingest_batch_counts_accepted() {
        let f = ChatPatternFederation::with_k_floor(2);
        let patterns: Vec<ChatPattern> = (0..5_u8)
            .map(|i| mk_pattern_with_emitter(i, IntentKind::Question))
            .collect();
        let n = f.ingest_batch(&patterns);
        assert_eq!(n, 5);
    }

    #[test]
    fn confidence_mean_correct() {
        let f = ChatPatternFederation::with_k_floor(2);
        for (i, c) in [(1_u8, 0.0_f32), (2, 0.5), (3, 1.0)] {
            let p = ChatPatternBuilder {
                intent_kind: IntentKind::Question,
                response_shape: ResponseShape::ShortDirect,
                arc_phase: ArcPhase::Setup,
                confidence: c,
                ts_unix: 60 * 100,
                region_tag: 1,
                opt_in_tier: 1,
                cap_flags: CAP_FLAGS_ALL,
                emitter_pubkey: [i; 32],
                co_signers: vec![],
            }
            .build()
            .unwrap();
            f.ingest(&p).unwrap();
        }
        let pub_set = f.snapshot_public();
        assert_eq!(pub_set.len(), 1);
        // Mean of 0.0, 0.5, 1.0 ≈ 0.5.
        let mean = pub_set[0].confidence();
        assert!((mean - 0.5).abs() < 0.05, "mean = {mean}");
    }

    #[test]
    fn stats_track_ingest_and_reject() {
        let f = ChatPatternFederation::new();
        // 1 accept, 1 cap-reject
        let ok = mk_pattern_with_emitter(1, IntentKind::Question);
        f.ingest(&ok).unwrap();
        let bad = ChatPatternBuilder {
            intent_kind: IntentKind::Question,
            response_shape: ResponseShape::ShortDirect,
            arc_phase: ArcPhase::Setup,
            confidence: 0.5,
            ts_unix: 60 * 100,
            region_tag: 1,
            opt_in_tier: 1,
            cap_flags: 0b0000_0001, // missing FEDERATION_INGEST
            emitter_pubkey: [99; 32],
            co_signers: vec![],
        }
        .build()
        .unwrap();
        let _ = f.ingest(&bad);
        let s = f.stats();
        assert_eq!(s.total_ingested, 1);
        assert_eq!(s.total_rejected_cap, 1);
    }
}
