//! § DropTable — per-context drop-distribution per GDD § DROP-TABLES.
//!
//! `DropContext{mob_tier, biome, magic_find}` modulates the base-curve.
//!   base-curve mob-tier-1 : Common 60% · Uncommon 28% · Rare 9% · Epic 2.5%
//!                            · Legendary 0.49% · Mythic 0.01%
//!   mob-tier-N : shifts curve up by tier-N × 0.05 weight (per GDD).
//!   magic_find : multiplies non-Common probabilities ; renormalizes.
//!
//! `roll_drop(ctx, seed) -> Option<Gear>` returns `None` if the roll-fraction
//! lands above the cumulative distribution (no-drop) — but for non-empty
//! contexts the implementation guarantees `Some` (every call yields a Gear).
//! `None` is reserved for explicit no-drop slots (future-extension).

use serde::{Deserialize, Serialize};

use crate::base::{BaseItem, BaseMat};
use crate::gear::Gear;
use crate::rarity::{rarity_drop_floor, Rarity};
use crate::slots::GearSlot;
use crate::stat_rolling::{roll_gear, DetRng};

// ───────────────────────────────────────────────────────────────────────
// § Biome
// ───────────────────────────────────────────────────────────────────────

/// Biome-tag for drop-table modulation. Closed-set ; future-extended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Biome {
    /// Generic dungeon — neutral drop-curve.
    Dungeon,
    /// Crypt — Shadow/Frost-bias.
    Crypt,
    /// Forge — Fire-bias.
    Forge,
    /// Sanctum — Light-bias.
    Sanctum,
    /// Abyss — Void/Shadow-bias.
    Abyss,
}

impl Biome {
    /// Stable name for audit-payloads.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Biome::Dungeon => "dungeon",
            Biome::Crypt => "crypt",
            Biome::Forge => "forge",
            Biome::Sanctum => "sanctum",
            Biome::Abyss => "abyss",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § DropContext
// ───────────────────────────────────────────────────────────────────────

/// Drop-context modulating the base-curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DropContext {
    /// Mob-tier ∈ [1..N]. Each step shifts curve up by 0.05 weight.
    pub mob_tier: u8,
    /// Biome — not drop-curve-affecting yet ; reserved for future-bias.
    pub biome: Biome,
    /// Magic-find ∈ [0.0, 1.0+] — multiplies non-Common probabilities.
    pub magic_find: f32,
}

impl DropContext {
    /// Trash-mob mob-tier-1 dungeon ; magic_find = 0.
    #[must_use]
    pub const fn trivial() -> Self {
        Self { mob_tier: 1, biome: Biome::Dungeon, magic_find: 0.0 }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § distribution_for_context  — per-rarity probabilities (sum to 1.0)
// ───────────────────────────────────────────────────────────────────────

/// Compute per-rarity drop-probabilities for the given context.
/// Returns `[Common, Uncommon, Rare, Epic, Legendary, Mythic]` summing to ~1.0.
///
/// Algorithm :
///   1. Start with base-curve floors.
///   2. Apply `mob_tier × 0.05` upshift : each Mythic+/Legendary/etc. tier
///      gets +0.05 × (mob_tier - 1) weight, subtracted from Common.
///   3. Apply `magic_find` : multiply non-Common rarities by (1 + magic_find).
///   4. Renormalize so Σ = 1.0.
///   5. Mythic-floor preserved : Mythic ≥ rarity_drop_floor(Mythic).
#[must_use]
pub fn distribution_for_context(ctx: &DropContext) -> [f32; 6] {
    let rarities = Rarity::all();
    let mut probs = [0.0f32; 6];
    for (i, r) in rarities.iter().enumerate() {
        probs[i] = rarity_drop_floor(*r);
    }
    // mob-tier upshift : take from Common, give to higher tiers proportional
    // to their floors. Cap upshift at 0.50 to avoid Common-collapse.
    let upshift = (ctx.mob_tier.saturating_sub(1) as f32 * 0.05).min(0.50);
    if upshift > 0.0 {
        let take = upshift.min(probs[0] - 0.05); // keep Common ≥ 0.05
        let take = take.max(0.0);
        probs[0] -= take;
        // Distribute proportional to current non-Common probs.
        let nc_sum: f32 = probs[1..].iter().sum();
        if nc_sum > 0.0 {
            for v in probs.iter_mut().skip(1) {
                *v += take * (*v / nc_sum);
            }
        }
    }
    // magic_find : multiply non-Common.
    let mf = ctx.magic_find.clamp(0.0, 5.0);
    if mf > 0.0 {
        for v in probs.iter_mut().skip(1) {
            *v *= mf + 1.0;
        }
    }
    // Renormalize.
    let sum: f32 = probs.iter().sum();
    if sum > 0.0 {
        for v in &mut probs {
            *v /= sum;
        }
    }
    // Mythic-floor : ensure >= 0.0001 (anti-spam invariant ; floor-not-time-gated).
    if probs[5] < 0.0001 {
        let deficit = 0.0001 - probs[5];
        probs[5] = 0.0001;
        // Take deficit from Common.
        probs[0] = (probs[0] - deficit).max(0.0);
    }
    probs
}

// ───────────────────────────────────────────────────────────────────────
// § sample_rarity  — seeded inverse-CDF
// ───────────────────────────────────────────────────────────────────────

/// Sample a rarity from the context's distribution. Deterministic per seed.
#[must_use]
pub fn sample_rarity(ctx: &DropContext, seed: u128) -> Rarity {
    let probs = distribution_for_context(ctx);
    let mut rng = DetRng::new(seed);
    let r = rng.next_f32();
    let mut cum = 0.0;
    for (i, p) in probs.iter().enumerate() {
        cum += *p;
        if r < cum {
            return Rarity::all()[i];
        }
    }
    Rarity::Common // fallback — should be unreachable post-renorm
}

// ───────────────────────────────────────────────────────────────────────
// § default_base_for_rarity  — pick a base-mat consistent with rarity
// ───────────────────────────────────────────────────────────────────────

/// Pick a default base-material whose rarity-floor ≤ rolled-rarity.
/// Prefers exact-floor match : Mythic→Soulbound · Legendary→Voidsteel · etc.
#[must_use]
pub fn default_base_for_rarity(r: Rarity) -> BaseMat {
    match r {
        Rarity::Common => BaseMat::Iron,
        Rarity::Uncommon => BaseMat::Silver,
        Rarity::Rare => BaseMat::Mithril,
        Rarity::Epic => BaseMat::Adamant,
        Rarity::Legendary => BaseMat::Voidsteel,
        Rarity::Mythic => BaseMat::Soulbound,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § roll_drop  — top-level entry point
// ───────────────────────────────────────────────────────────────────────

/// Top-level drop-roller. `slot_hint` selects the gear-slot ; defaults to
/// MainHand if none-given. Returns Some(Gear) — `None` reserved for explicit
/// no-drop tables (future-extension).
#[must_use]
pub fn roll_drop(ctx: &DropContext, seed: u128, slot_hint: Option<GearSlot>) -> Option<Gear> {
    let rarity = sample_rarity(ctx, seed);
    let mat = default_base_for_rarity(rarity);
    let slot = slot_hint.unwrap_or(GearSlot::MainHand);
    // Pick a base by slot class. Default-class heuristic :
    //   weapon-slots → Weapon · armor-slots → Armor · ring/amulet → Jewelry · trinket → Trinket.
    let base = match slot {
        GearSlot::MainHand | GearSlot::OffHand => BaseItem::weapon(slot, mat, 10.0, 1.0),
        GearSlot::RingA | GearSlot::RingB | GearSlot::Amulet => BaseItem::jewelry(slot, mat, 50.0),
        GearSlot::Trinket => BaseItem::trinket(slot, mat, 3.0),
        _ => BaseItem::armor(slot, mat, 20.0),
    };
    Some(roll_gear(seed, &base, rarity))
}
