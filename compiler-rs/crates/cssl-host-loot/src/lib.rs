// § T11-W13-LOOT : cssl-host-loot — 6-tier rarity loot-drop · COSMETIC-ONLY-AXIOM
// §§ spec : Labyrinth of Apocalypse/systems/loot_drop.csl
// §§ thesis : "looting = fashion-show ¬ power-curve" · rarity gates aesthetic-richness ¬ stats
// §§ axioms (t∞) :
//     A-1 ¬ stat-modifying-affixes (no +damage / +reload-speed / +accuracy)
//     A-2 ¬ pay-for-tier-skip (no purchase-pathway in LootRoll)
//     A-3 ✓ DPS-tier IDENTICAL across rarities (verifiable via attest_no_pay_for_power)
//     A-4 ✓ KAN-bias Σ-mask-gated default-deny (player opt-in explicit)
//     A-5 ✓ KAN-bias-update-cap (no runaway amplification)
//     A-6 ✓ Σ-Chain anchor every drop (sovereign-revocable + immutable-history)
//     A-7 ✓ public drop-rates : Common 60 · Uncommon 25 · Rare 10 · Epic 4 · Legendary 0.9 · Mythic 0.1
//
// §§ scope this crate :
//     - LootAffix sum-type (Visual · Audio · Particle · Attribution) — NO stat-variant
//     - LootItem with Rarity-tier from cssl-host-gear-archetype + cosmetic-affix bag
//     - DropRateDistribution::PUBLIC canonical 6-tier curve
//     - KanBiasVector + Σ-mask consent-gated apply
//     - LootRoll : KAN-bias rolls rarity → affix-rolls within-rarity-pool
//     - LootDropEvent → cssl-host-sigma-chain SigmaEvent anchor (kind=LootDrop)
//     - attest_no_pay_for_power(item) structural-attestation
//
// §§ ATTESTATION (PRIME_DIRECTIVE.md § 11) :
//     There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
//! # cssl-host-loot
//!
//! 6-tier rarity loot-drop with **COSMETIC-ONLY-AXIOM** structurally enforced.
//!
//! ## Axiom : looting = fashion-show, not power-curve
//!
//! Rarity gates **aesthetic richness only** — never stats, never DPS, never
//! reload-speed. Per-tier DPS is identical across the 6 rarities ; rarity
//! buys you tracer-color, muzzle-flash, particle-effects, and creator-attribution
//! — nothing that changes balance.
//!
//! ### Public drop-rates (W13-8 spec)
//!
//! | Rarity      | Rate    |
//! |-------------|---------|
//! | Common      | 60%     |
//! | Uncommon    | 25%     |
//! | Rare        | 10%     |
//! | Epic        | 4%      |
//! | Legendary   | 0.9%    |
//! | Mythic      | 0.1%    |
//!
//! These rates are PUBLIC ; player-facing UI exposes them verbatim. No hidden
//! pity-timers, no manipulated drop-tables, no tier-skip purchase pathway.
//!
//! ## COSMETIC-ONLY-AXIOM enforcement
//!
//! [`LootAffix`] is a sum-type with **only four cosmetic variants** :
//!
//! - [`LootAffix::Visual`]      — tracer-color · muzzle-flash · impact-particle · weapon-skin-pattern
//! - [`LootAffix::Audio`]       — fire-sound · reload-clink · idle-hum
//! - [`LootAffix::Particle`]    — casing-eject · trail · holster-effect
//! - [`LootAffix::Attribution`] — creator-name · season-tag · biome-origin
//!
//! There is **no** `LootAffix::StatBuff(...)` variant. The type-system makes a
//! `+10% damage` affix unrepresentable. [`attest_no_pay_for_power`] walks the
//! variants and returns `true` for every shipped item — the proof is structural
//! (the absence of the variant), not behavioral.
//!
//! ## KAN-bias integration
//!
//! Per-player [`KanBiasVector`] tunes rarity-distribution toward aesthetic
//! preference (e.g. player likes blue-tracers → mild bias toward Visual variants
//! that ship blue-tracers). Σ-mask-gated **default-deny** ; player must explicitly
//! opt-in via [`KanBiasConsent::granted`]. Bias-update-cap (`MAX_BIAS_DELTA`)
//! prevents runaway amplification regardless of inputs.
//!
//! ## Σ-Chain anchoring
//!
//! Every drop emits a [`LootDropEvent`] which serializes to a
//! [`cssl_host_sigma_chain::SigmaEvent`] of kind [`EventKind::LootDrop`].
//! The drop becomes part of immutable history — the player can sovereign-revoke
//! via the Σ-Chain `forget-myself` axiom but cannot rewrite past drops.
//!
//! ## Quick-start
//!
//! ```no_run
//! use cssl_host_loot::{
//!     roll_loot, attest_no_pay_for_power, DropRateDistribution, KanBiasConsent, LootContext,
//! };
//! use cssl_host_gear_archetype::Rarity;
//!
//! let dist = DropRateDistribution::PUBLIC;
//! let consent = KanBiasConsent::denied();             // default-deny
//! let ctx = LootContext::default_for_combat_end();
//! let item = roll_loot(&dist, &consent, &ctx, 0xDEAD_BEEF_BAD_F00D_u128);
//! assert!(attest_no_pay_for_power(&item));             // structural-true
//! assert!(item.rarity >= Rarity::Common);
//! ```

pub mod affix;
pub mod attest;
pub mod bias;
pub mod distribution;
pub mod event;
pub mod item;
pub mod roll;

pub use affix::{
    AffixCategory, AttributionAffix, AudioAffix, LootAffix, ParticleAffix, VisualAffix,
};
pub use attest::{attest_no_pay_for_power, PayForPowerError};
pub use bias::{KanBiasConsent, KanBiasVector, BIAS_DIM, MAX_BIAS_DELTA};
pub use distribution::{DropRateDistribution, PUBLIC_DROP_RATES};
pub use event::{anchor_drop_to_sigma_chain, LootDropEvent};
pub use item::{LootItem, LootSeason};
pub use roll::{roll_loot, sample_rarity_with_bias, LootContext, LootRollError};

/// Crate-tag for transparency/audit-stream identification.
pub const LOOT_CRATE_TAG: &str = "cssl-host-loot/0.1.0";
