//! § cssl-host-battle-pass — seasonal-pass progression aggregator.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Tracks 100-tier seasonal battle-pass progression with two parallel
//!   tracks : `Free` (always-available · cosmetic-only) and `Premium`
//!   (Stripe-purchased · cosmetic-only · faster-pull but-NOT-exclusive).
//!   The pass enforces PRIME-DIRECTIVE-aligned anti-FOMO + sovereign-
//!   revocability rules :
//!
//! ```text
//!     • Free track is 100% accessible without payment.
//!     • Both tracks reward COSMETICS ONLY · ¬ stat-affixes · ¬ XP-boost.
//!     • Expired rewards are re-purchasable post-season at gift-cost.
//!     • Player may PAUSE progression-tracking via Σ-cap.
//!     • 14-day pro-rated refund window for Premium purchase.
//!     • Mycelium federation of progress is default-DENY · cap-grant required.
//!     • Progress accumulates across seasons · ¬ resetting.
//! ```
//!
//! § DESIGN
//!   - Pure data-model + state-machine. Zero outbound side-effects.
//!     Persistence + Stripe-call lives in `cssl-edge` endpoints + the
//!     `cssl-supabase/migrations/0032_battle_pass.sql` schema.
//!   - Determinism : every serde-exposed map is `BTreeMap` so JSON
//!     output is stable + diffable.
//!   - The XP-curve in [`xp::xp_required_for_tier`] is monotone-increasing
//!     and sub-quadratic in the early tiers (anti-grind) and gently
//!     accelerating in the later tiers (gradual-but-not-FOMO).
//!
//! § PRIME-DIRECTIVE binding
//!   - [`SeasonRules::FREE_TRACK_ALWAYS_INCLUDED`] is `true` and used by
//!     [`SeasonRules::validate`] to fail-closed any deserialization that
//!     attempts to set it false.
//!   - [`SeasonRules::COSMETIC_ONLY`] is `true` and rewards carrying any
//!     non-cosmetic effect-kind are rejected at construction-time.
//!   - [`refund::pro_rate_refund_cents`] returns the prorated refund-cents
//!     within the 14-day window. After-window → 0-cents (no refund).
//!   - [`mycelium::default_federation_decision`] is `Deny`.
//!
//! § DEPENDENCIES
//!   `serde` + `serde_json` only. Mirrors the simplicity-discipline of
//!   `cssl-host-attestation` and `cssl-host-coherence-proof`.

#![forbid(unsafe_code)]

pub mod mycelium;
pub mod progression;
pub mod refund;
pub mod reward;
pub mod rules;
pub mod season;
pub mod xp;

pub use mycelium::{default_federation_decision, FederationCap, FederationDecision};
pub use progression::{
    BattlePassErr, PauseState, Progression, ProgressionEvent, ProgressionUpdate,
};
pub use refund::{pro_rate_refund_cents, PurchaseReceipt, RefundDecision, RefundErr};
pub use reward::{Reward, RewardKind, RewardTrack};
pub use rules::SeasonRules;
pub use season::{Season, SeasonErr, SeasonId, SeasonStatus};
pub use xp::{
    cumulative_xp_for_tier, tier_for_cumulative_xp, xp_required_for_tier, MAX_TIER, MIN_TIER,
};

/// § PRIME-DIRECTIVE banner ← attest structurally.
pub const PRIME_DIRECTIVE_BANNER: &str =
    "cosmetic-channel-only · ¬ pay-for-power · ¬ FOMO · sovereign-revocable";

/// Crate version (matches Cargo.toml) — surfaced for receipt headers.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Spec-anchor : the LoA system this crate implements.
pub const SPEC_ANCHOR: &str = "Labyrinth of Apocalypse/systems/battle_pass.csl";

/// Database-schema anchor : the SQL migration this crate mirrors.
pub const SCHEMA_ANCHOR: &str = "cssl-supabase/migrations/0032_battle_pass.sql";

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn prime_directive_banner_includes_no_pay_for_power() {
        assert!(PRIME_DIRECTIVE_BANNER.contains("¬ pay-for-power"));
    }

    #[test]
    fn version_matches_cargo() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }

    #[test]
    fn anchors_nonempty() {
        assert!(SPEC_ANCHOR.contains("battle_pass.csl"));
        assert!(SCHEMA_ANCHOR.contains("0032_battle_pass.sql"));
    }
}
