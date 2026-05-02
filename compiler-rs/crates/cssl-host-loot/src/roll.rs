//! § roll — drop-event flow per W13-8 spec
//!
//! Spec drop-event flow :
//!   1. Combat-encounter ends → generate-loot-roll
//!   2. KAN-bias-rolls rarity (within-public-distribution)
//!   3. Affix-rolls within-rarity-pool (cosmetic-only)
//!   4. Σ-Chain-anchor drop-event (immutable-history · sovereign-revocable)
//!   5. Player-pickup → inventory + Akashic-attribution
//!
//! This module covers steps 1–3. Step 4 lives in [`crate::event`]. Step 5
//! is the consumer's responsibility (e.g. inventory crate).

use cssl_host_gear_archetype::{DetRng, Rarity};
use serde::{Deserialize, Serialize};

use crate::affix::{
    AffixCategory, AttributionAffix, AudioAffix, LootAffix, ParticleAffix, VisualAffix,
};
use crate::bias::{KanBiasConsent, KanBiasVector};
use crate::distribution::DropRateDistribution;
use crate::item::{LootItem, LootSeason};

// ───────────────────────────────────────────────────────────────────────
// § LootRollError — narrow set
// ───────────────────────────────────────────────────────────────────────

/// Errors that can arise during a loot roll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LootRollError {
    /// Distribution not normalized (sum outside [0.999, 1.001]).
    NotNormalized,
    /// Caller attempted to specify a tier-skip purchase — REJECTED structurally
    /// to enforce the no-pay-for-power axiom.
    PayForTierSkipAttempted,
}

impl core::fmt::Display for LootRollError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LootRollError::NotNormalized => f.write_str("drop-distribution not normalized"),
            LootRollError::PayForTierSkipAttempted => {
                f.write_str("pay-for-tier-skip rejected (no-pay-for-power axiom)")
            }
        }
    }
}

impl std::error::Error for LootRollError {}

// ───────────────────────────────────────────────────────────────────────
// § LootContext — per-call state
// ───────────────────────────────────────────────────────────────────────

/// Per-call drop context. Influences affix-rolls but NEVER rarity-distribution
/// outside the public-curve. (Bias modulates rarity ; context informs affix-pool.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootContext {
    /// Opaque WeaponKind code attached to the dropped item.
    pub weapon_kind_code: u32,
    /// Season-tag at drop-time.
    pub season: LootSeason,
    /// Optional creator-attribution string for AttributionAffix (≤64 bytes).
    pub creator_name: Option<String>,
    /// Biome-origin id for AttributionAffix.
    pub biome_origin: u8,
}

impl LootContext {
    /// Default context for "combat-encounter ends" event. Empty creator-name,
    /// generic biome.
    #[must_use]
    pub fn default_for_combat_end() -> Self {
        Self {
            weapon_kind_code: 0,
            season: LootSeason::BOOTSTRAP,
            creator_name: None,
            biome_origin: 0,
        }
    }
}

impl Default for LootContext {
    fn default() -> Self {
        Self::default_for_combat_end()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § sample_rarity_with_bias
// ───────────────────────────────────────────────────────────────────────

/// Sample a rarity from the bias-modulated distribution.
///
/// **Pipeline** :
///   1. Apply [`KanBiasVector::apply_to`] under the consent-gate (default-deny).
///   2. Renormalize to sum = 1.0.
///   3. Inverse-CDF roll with [`DetRng`] seeded from `seed`.
///
/// Deterministic per `seed` + `(distribution, bias, consent)`.
#[must_use]
pub fn sample_rarity_with_bias(
    base: &DropRateDistribution,
    bias: &KanBiasVector,
    consent: &KanBiasConsent,
    seed: u128,
) -> Rarity {
    let modulated = bias.apply_to(base, consent);
    let mut rng = DetRng::new(seed);
    let r = rng.next_f32();
    let rarities = Rarity::all();
    let mut cum = 0.0_f32;
    for (i, p) in modulated.rates.iter().enumerate() {
        cum += *p;
        if r < cum {
            return rarities[i];
        }
    }
    // Defensive fallback — if floating-point sums slightly < 1.0, return Common.
    Rarity::Common
}

// ───────────────────────────────────────────────────────────────────────
// § rarity_affix_count_band — affix-count by rarity (cosmetic richness)
// ───────────────────────────────────────────────────────────────────────

/// Cosmetic-richness band : `(min_affix_count, max_affix_count)` per rarity.
/// Higher rarities ship MORE affixes — the **only** thing rarity buys you.
/// Q-06 Apocky-canonical 2026-05-01 : extended to 8 tiers.
#[must_use]
const fn rarity_affix_count_band(r: Rarity) -> (u8, u8) {
    match r {
        Rarity::Common => (0, 1),
        Rarity::Uncommon => (1, 2),
        Rarity::Rare => (2, 3),
        Rarity::Epic => (3, 4),
        Rarity::Legendary => (4, 6),
        Rarity::Mythic => (6, 8),
        Rarity::Prismatic => (8, 10), // Q-06 NEW · multi-element-resonance
        Rarity::Chaotic => (10, 12),  // Q-06 NEW · Σ-mask wildcard pool
    }
}

// ───────────────────────────────────────────────────────────────────────
// § affix-pool — per-category candidates
// ───────────────────────────────────────────────────────────────────────

/// Visual-affix pool sample. Deterministic per seed.
fn roll_visual_affix(rng: &mut DetRng) -> VisualAffix {
    // 4 sub-variants ; pick by next_u32_below(4).
    let pick = rng.next_u32_below(4) as u8;
    match pick {
        0 => {
            // Random RGB packed into u32 (top byte zero).
            let color = rng.next_u32_below(0x00FF_FFFE_u32 + 1);
            VisualAffix::TracerColor(color)
        }
        1 => VisualAffix::MuzzleFlash(rng.next_u32_below(256) as u16),
        2 => VisualAffix::ImpactParticle(rng.next_u32_below(256) as u16),
        _ => VisualAffix::SkinPattern(rng.next_u32_below(256) as u16),
    }
}

fn roll_audio_affix(rng: &mut DetRng) -> AudioAffix {
    let pick = rng.next_u32_below(3) as u8;
    match pick {
        0 => AudioAffix::FireSound(rng.next_u32_below(256) as u16),
        1 => AudioAffix::ReloadClink(rng.next_u32_below(256) as u16),
        _ => AudioAffix::IdleHum(rng.next_u32_below(256) as u16),
    }
}

fn roll_particle_affix(rng: &mut DetRng) -> ParticleAffix {
    let pick = rng.next_u32_below(3) as u8;
    match pick {
        0 => ParticleAffix::CasingEject(rng.next_u32_below(256) as u16),
        1 => ParticleAffix::Trail(rng.next_u32_below(256) as u16),
        _ => ParticleAffix::HolsterEffect(rng.next_u32_below(256) as u16),
    }
}

fn roll_attribution_affix(rng: &mut DetRng, ctx: &LootContext) -> AttributionAffix {
    let pick = rng.next_u32_below(3) as u8;
    match pick {
        0 => {
            let name = ctx.creator_name.clone().unwrap_or_else(|| String::from("anon"));
            // Bound to 64 bytes per spec.
            let bounded = if name.len() > 64 {
                name.chars().take(64).collect::<String>()
            } else {
                name
            };
            AttributionAffix::CreatorName(bounded)
        }
        1 => AttributionAffix::SeasonTag(ctx.season.0),
        _ => AttributionAffix::BiomeOrigin(ctx.biome_origin),
    }
}

/// Roll a single cosmetic affix, picking a category uniformly (per call ; the
/// affix-bag may end up biased toward visual due to count-distribution).
fn roll_one_affix(rng: &mut DetRng, ctx: &LootContext) -> LootAffix {
    let cats = AffixCategory::all();
    let pick_idx = rng.next_u32_below(cats.len() as u32) as usize;
    match cats[pick_idx] {
        AffixCategory::Visual => LootAffix::Visual(roll_visual_affix(rng)),
        AffixCategory::Audio => LootAffix::Audio(roll_audio_affix(rng)),
        AffixCategory::Particle => LootAffix::Particle(roll_particle_affix(rng)),
        AffixCategory::Attribution => LootAffix::Attribution(roll_attribution_affix(rng, ctx)),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § roll_loot — top-level entry point
// ───────────────────────────────────────────────────────────────────────

/// Top-level loot-roll. Deterministic per `seed`. Honors the
/// COSMETIC-ONLY-AXIOM by construction (only cosmetic affixes can be produced).
///
/// **Pipeline** :
///   1. [`sample_rarity_with_bias`] picks a rarity (Σ-mask-gated KAN-bias).
///   2. Affix-count is drawn uniformly within the rarity's band.
///   3. Each affix is rolled from the cosmetic pool.
#[must_use]
pub fn roll_loot(
    base: &DropRateDistribution,
    consent: &KanBiasConsent,
    ctx: &LootContext,
    seed: u128,
) -> LootItem {
    let bias = KanBiasVector::zero();
    roll_loot_with_bias(base, &bias, consent, ctx, seed)
}

/// Variant accepting an explicit bias vector.
#[must_use]
pub fn roll_loot_with_bias(
    base: &DropRateDistribution,
    bias: &KanBiasVector,
    consent: &KanBiasConsent,
    ctx: &LootContext,
    seed: u128,
) -> LootItem {
    let rarity = sample_rarity_with_bias(base, bias, consent, seed);
    // Re-derive affix-rng from a sub-seed to keep rarity + affix rolls independent.
    let affix_seed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15_9E37_79B9_7F4A_7C15_u128);
    let mut rng = DetRng::new(affix_seed);

    let (lo, hi) = rarity_affix_count_band(rarity);
    let count = if hi <= lo {
        u32::from(lo)
    } else {
        let span = u32::from(hi - lo) + 1;
        u32::from(lo) + rng.next_u32_below(span)
    };

    let mut affixes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        affixes.push(roll_one_affix(&mut rng, ctx));
    }

    LootItem::new(rarity, ctx.weapon_kind_code, affixes, ctx.season, seed)
}
