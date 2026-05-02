// § T11-W13-GACHA : cssl-host-gacha — root module
// ════════════════════════════════════════════════════════════════════
// § I> Purpose : transparency-first gacha-system
//   ⊑ Pull-modes : Single (1) · TenPull (10 + bonus) · HundredPull (100 + bonus)
//   ⊑ Drop-rates DISCLOSED publicly :
//       Common 60.0% · Uncommon 25.0% · Rare 10.0% · Epic 4.0% · Legendary 0.9% · Mythic 0.1%
//   ⊑ Pity-system : guaranteed-Mythic within PITY_THRESHOLD (=90) pulls (publicly-known)
//   ⊑ Sovereign-7d-refund : full refund · pull-cancelled · cosmetics-removed · automated-API
//   ⊑ Σ-Chain-anchor every-pull for-attribution-immutable-history
//
// § PRIME-DIRECTIVE attestations (structurally-encoded · constants exposed) :
//   1. ¬ pay-for-power      (cosmetic-only-axiom)
//   2. ¬ near-miss-anim     (no "almost-there!" UI feedback)
//   3. ¬ countdown-FOMO     (no time-limited exclusive-power)
//   4. ¬ exclusive-cosmetic-AT-ALL (all eventually-attainable)
//   5. ¬ loss-aversion-framing
//   6. ¬ social-comparison  (no "X just won!" UI)
//   7. ¬ celebrity-endorsement
//   8. ¬ in-game-grind-loop for-pull-currency (only Stripe OR gift-from-friend)
//   9. transparency-mandate (all drop-rates + pity publicly-disclosed)
//  10. sovereign-revocable (player-pubkey-tied · 7d full-refund window)
//
// § I> deterministic : BTreeMap-keyed banner-tables · DetRng-seed for replay-equivalence
// § I> safety : forbid(unsafe_code) · no panics in lib · all-failures via Result
// ════════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod banner;
pub mod pity;
pub mod pull;
pub mod refund;
pub mod rng;
pub mod sigma_anchor;
pub mod transparency;

pub use banner::{Banner, BannerErr, DropRateTable, Rarity};
pub use pity::{PityCounter, PityErr, PITY_THRESHOLD};
pub use pull::{run_pull, PullErr, PullMode, PullOutcome, PullRequest, PullResult};
pub use refund::{run_refund, RefundErr, RefundOutcome, RefundRequest, RefundWindow, REFUND_WINDOW_SECS};
pub use rng::{DetRng, derive_seed_from_pubkey};
pub use sigma_anchor::{SigmaAnchor, SigmaAnchorErr, SIGMA_ANCHOR_VERSION};
pub use transparency::{
    AttestationFlags, PredatoryPatternAttestation, ATTESTATIONS_COUNT,
};

/// § PRIME-DIRECTIVE banner — structural attestation per global preferences.
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • violation=bug • no-override-exists • cosmetic-only-axiom";

/// Crate version (matches Cargo.toml) — surfaced for receipt-headers + audit-emit.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Spec-anchor — the .csl spec this crate implements.
pub const SPEC_ANCHOR: &str = "Labyrinth of Apocalypse/systems/gacha.csl";

/// Public drop-rate disclosure threshold attestation : these MUST be re-verified
/// any time the drop-rate-table is modified ; the constants below are the
/// canonical reference values used by the public-disclosure banner schema.
pub mod canonical_drop_rates {
    /// Common rarity — 60.0% (60_000 / 100_000 basis-points).
    pub const COMMON_BPS: u32 = 60_000;
    /// Uncommon — 25.0% (25_000 / 100_000).
    pub const UNCOMMON_BPS: u32 = 25_000;
    /// Rare — 10.0% (10_000 / 100_000).
    pub const RARE_BPS: u32 = 10_000;
    /// Epic — 4.0% (4_000 / 100_000).
    pub const EPIC_BPS: u32 = 4_000;
    /// Legendary — 0.9% (900 / 100_000).
    pub const LEGENDARY_BPS: u32 = 900;
    /// Mythic — 0.1% (100 / 100_000).
    pub const MYTHIC_BPS: u32 = 100;
    /// Total basis-points (must sum to 100_000 = 100.0%).
    pub const TOTAL_BPS: u32 = 100_000;

    /// Compile-time invariant : sum-of-rates == TOTAL_BPS.
    pub const SUM_INVARIANT: u32 = COMMON_BPS
        + UNCOMMON_BPS
        + RARE_BPS
        + EPIC_BPS
        + LEGENDARY_BPS
        + MYTHIC_BPS;

    /// Static assertion via const-eval : the sum MUST equal TOTAL_BPS.
    /// If this expression is ever non-zero the const-eval fails to build.
    const _: () = assert!(SUM_INVARIANT == TOTAL_BPS);
}

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn prime_directive_banner_includes_cosmetic_axiom() {
        assert!(PRIME_DIRECTIVE_BANNER.contains("cosmetic-only-axiom"));
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
    }

    #[test]
    fn version_matches_cargo() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }

    #[test]
    fn spec_anchor_points_at_loa_systems() {
        assert_eq!(SPEC_ANCHOR, "Labyrinth of Apocalypse/systems/gacha.csl");
    }

    #[test]
    fn canonical_drop_rates_sum_to_100pct() {
        use canonical_drop_rates::*;
        assert_eq!(SUM_INVARIANT, TOTAL_BPS);
        assert_eq!(TOTAL_BPS, 100_000);
    }

    #[test]
    fn canonical_mythic_is_one_in_thousand() {
        // 0.1% = 1-in-1000. Documented publicly · pity-system kicks in at 90.
        assert_eq!(canonical_drop_rates::MYTHIC_BPS, 100);
    }
}
