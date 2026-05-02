//! § wired_content — content rating + moderation + playtest wired into loa-host.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W16-WIREUP-CONTENT (W12-7 · W12-10 · W12-11 → loa-host event-loop)
//!
//! § ROLE
//!   Pre-allocates the three content-pipeline stores :
//!     - `RatingStore`     (W12-7) for player-submitted ratings + reviews.
//!     - `ModerationStore` (W12-11) for flag-handling + curator decisions.
//!     - `PlaytestState`   (W12-10) for automated-GM playtest queue.
//!
//!   Per-frame `tick(state, dt_ms, ingest)` drains a pending-ingest queue
//!   into the appropriate store. Cap-gated : every mutation requires the
//!   per-store cap (default-deny ; the wrapped crates structurally
//!   enforce the cap-bits at submit-time).
//!
//! § Σ-CAP-GATE attestation
//!   - default-deny : RatingStore::submit checks `CAP_RATE` ; failure is
//!     non-fatal (the host counts denials).
//!   - k-anonymity : aggregates honor `K_FLOOR_SINGLE = 5` ; this slice
//!     does NOT bypass the floor.
//!   - sovereign-revoke : both stores expose revoke-paths ; this slice
//!     re-exports the helpers but does NOT call them automatically.
//!
//! § ATTESTATION
//!   ¬ harm · ¬ surveillance · ¬ profiling-individual-players.
//!   Aggregate-only above k=5/k=10 floors.
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(clippy::module_name_repetitions)]

pub use cssl_content_rating::{
    AggregateView, AggregateVisibility, Rating, RatingStore, Review,
    StoreError as RatingStoreError, TagBitset, CAP_AGGREGATE_PUBLIC, CAP_RATE,
    K_FLOOR_SINGLE, K_FLOOR_TRENDING,
};

pub use cssl_content_moderation::{
    CapPolicy as ModCapPolicy, FlagRecord, ModerationAggregate, ModerationStore,
    StoreError as ModStoreError, MOD_CAP_AGGREGATE_READ, MOD_CAP_FLAG_SUBMIT,
};

pub use cssl_host_playtest_agent::{
    PlayTestReport, QualitySignal, SovereignDecline, DEFAULT_MAX_TURNS, DEFAULT_TIMEOUT_SECS,
};

/// § Per-frame ingest bundle the host fills from MCP / network / on-disk feeds.
/// Every field is OPTIONAL ; a frame with no ingest is a no-op.
#[derive(Debug, Clone, Default)]
pub struct ContentIngest {
    /// Optional rating to submit this frame.
    pub rating: Option<Rating>,
    /// Optional review to attach to the rating above.
    pub review: Option<Review>,
    /// Σ-cap : rating-submit gated. Default-deny.
    pub allow_rating: bool,

    /// Optional flag-record to submit this frame.
    pub flag: Option<FlagRecord>,
    /// Cap-policy snapshot for the flagger ; required by ModerationStore::submit_flag.
    pub flagger_cap: Option<ModCapPolicy>,
    /// Σ-cap : moderation-flag-submit gated. Default-deny.
    pub allow_flag: bool,
    /// Server timestamp (seconds since epoch ; used by ModerationStore).
    pub now_secs: u32,

    /// Optional playtest-quality-signal to ingest.
    pub quality_signal: Option<QualitySignal>,
}

/// § Persistent content-pipeline state.
pub struct ContentState {
    pub ratings: RatingStore,
    pub moderation: ModerationStore,
    /// Stage-0 quality-signal accumulator (KAN-bias feed).
    pub quality_log: Vec<QualitySignal>,
    /// Counters.
    pub ratings_accepted: u64,
    pub ratings_denied: u64,
    pub flags_accepted: u64,
    pub flags_denied: u64,
    pub quality_signals_ingested: u64,
}

impl Default for ContentState {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ratings: RatingStore::new(),
            moderation: ModerationStore::new(),
            quality_log: Vec::new(),
            ratings_accepted: 0,
            ratings_denied: 0,
            flags_accepted: 0,
            flags_denied: 0,
            quality_signals_ingested: 0,
        }
    }
}

/// § Per-frame tick — drain the ingest bundle into the stores.
/// Cap-gating : every submit checks `allow_*` BEFORE attempting the
/// store-call ; a denied frame counts up the `*_denied` counter and
/// returns without mutating the store.
pub fn tick(state: &mut ContentState, _dt_ms: f32, ingest: ContentIngest) {
    // § Rating ingest.
    if let Some(rating) = ingest.rating {
        if ingest.allow_rating {
            match state.ratings.submit(rating, ingest.review) {
                Ok(()) => state.ratings_accepted = state.ratings_accepted.saturating_add(1),
                Err(_) => state.ratings_denied = state.ratings_denied.saturating_add(1),
            }
        } else {
            state.ratings_denied = state.ratings_denied.saturating_add(1);
        }
    }

    // § Flag ingest.
    if let (Some(flag), Some(cap)) = (ingest.flag, ingest.flagger_cap) {
        if ingest.allow_flag {
            match state.moderation.submit_flag(cap, flag, ingest.now_secs) {
                Ok(()) => state.flags_accepted = state.flags_accepted.saturating_add(1),
                Err(_) => state.flags_denied = state.flags_denied.saturating_add(1),
            }
        } else {
            state.flags_denied = state.flags_denied.saturating_add(1);
        }
    }

    // § Quality-signal ingest (always allowed ; the wrapped playtest-agent
    // crate already gated cap-bits at session-start ; this is just a feed).
    if let Some(sig) = ingest.quality_signal {
        state.quality_log.push(sig);
        state.quality_signals_ingested = state.quality_signals_ingested.saturating_add(1);
    }
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_empty() {
        let s = ContentState::new();
        assert_eq!(s.ratings.len(), 0);
        assert_eq!(s.ratings_accepted, 0);
        assert_eq!(s.ratings_denied, 0);
        assert_eq!(s.flags_accepted, 0);
        assert_eq!(s.flags_denied, 0);
    }

    #[test]
    fn tick_no_ingest_no_change() {
        let mut s = ContentState::new();
        tick(&mut s, 16.6, ContentIngest::default());
        assert_eq!(s.ratings.len(), 0);
        assert_eq!(s.ratings_accepted, 0);
    }

    fn make_rating(content_id: u32) -> Rating {
        use cssl_content_rating::TagBitset;
        Rating::new(
            0xDEAD_BEEF_u64,
            content_id,
            4,
            TagBitset::from_bits(0b0001),
            CAP_RATE,
            0,
            128,
        )
        .expect("test rating constructs")
    }

    fn make_quality_signal() -> QualitySignal {
        QualitySignal {
            total_q8: 200,
            safety_q8: 240,
            fun_q8: 200,
            balance_q8: 180,
            polish_q8: 220,
            is_publishable: 1,
            cosmetic_attest: 1,
            crash_count: 0,
            softlock_count: 0,
            determinism_ok: 1,
            protocol_version: 1,
        }
    }

    #[test]
    fn rating_submit_default_deny_increments_denials() {
        let mut s = ContentState::new();
        let rating = make_rating(1234);
        let ingest = ContentIngest {
            rating: Some(rating),
            allow_rating: false, // CAP DENIED
            ..Default::default()
        };
        tick(&mut s, 16.6, ingest);
        assert_eq!(s.ratings_accepted, 0);
        assert_eq!(s.ratings_denied, 1);
    }

    #[test]
    fn rating_submit_with_cap_accepted() {
        let mut s = ContentState::new();
        let rating = make_rating(1234);
        let ingest = ContentIngest {
            rating: Some(rating),
            allow_rating: true,
            ..Default::default()
        };
        tick(&mut s, 16.6, ingest);
        assert_eq!(s.ratings_accepted, 1);
        assert_eq!(s.ratings.len(), 1);
    }

    #[test]
    fn rating_with_missing_cap_bit_in_mask_denied() {
        use cssl_content_rating::TagBitset;
        // sigma_mask=0 means the rating itself doesn't have CAP_RATE.
        // Rating::new should reject this — verify.
        let result = Rating::new(0xDEAD_BEEF, 1234, 4, TagBitset::from_bits(0b0001), 0, 0, 128);
        assert!(result.is_err());
    }

    #[test]
    fn quality_signal_ingest_appends_to_log() {
        let mut s = ContentState::new();
        let sig = make_quality_signal();
        let ingest = ContentIngest {
            quality_signal: Some(sig),
            ..Default::default()
        };
        tick(&mut s, 16.6, ingest);
        assert_eq!(s.quality_log.len(), 1);
        assert_eq!(s.quality_signals_ingested, 1);
    }

    #[test]
    fn cap_constants_are_re_exported() {
        assert_eq!(CAP_RATE, 0x01);
        assert_eq!(K_FLOOR_SINGLE, 5);
        assert_eq!(K_FLOOR_TRENDING, 10);
        assert_eq!(MOD_CAP_FLAG_SUBMIT, 0x01);
    }

    #[test]
    fn multiple_ratings_accumulate_counters() {
        use cssl_content_rating::TagBitset;
        let mut s = ContentState::new();
        for i in 0..3u64 {
            let rating = Rating::new(
                i,
                1234,
                4,
                TagBitset::from_bits(0b0001),
                CAP_RATE,
                0,
                128,
            )
            .unwrap();
            tick(
                &mut s,
                16.6,
                ContentIngest {
                    rating: Some(rating),
                    allow_rating: true,
                    ..Default::default()
                },
            );
        }
        assert_eq!(s.ratings_accepted, 3);
        assert_eq!(s.ratings.len(), 3);
    }
}
