//! § Stat-rolling — deterministic + replay-bit-equal per GDD § STAT-ROLLING.
//!
//! Algorithm :
//!   seed = (drop-source-id ⊕ player-id ⊕ ts ⊕ world-seed) mod u128
//!   per-affix-roll-range = tier-curve table [tier-1..6]
//!   rarity ↔ tier-bias :
//!     Common (1..2) · Uncommon (2..3) · Rare (3..4) · Epic (4..5)
//!     Legendary (5..6) · Mythic (6..6)
//!   final-stat = base × (1 + Σ-affix-percents) clamped-to-class-max
//!
//! `DetRng` is a SplitMix64-style PRNG (Steele · Lea · Flood) — public-domain,
//! deterministic, branch-free. Suitable for replay-bit-equal across hosts.
//!
//! Re-roll = NEW-seed-event ; `audit_emit` triggered at the call-site (see
//! `crate::upgrade::reroll_affix`).

// Per-rarity match-arms with identical bodies are intentional (per GDD §
// rarity ↔ tier-bias) — preserved for readability + future-divergence per-tier.
#![allow(clippy::match_same_arms)]
// Roll-fraction maths : `mul_add` is a fastmath suggestion ; we keep the
// classic `lo + span × frac` shape for replay-bit-equal across hardware that
// may not all expose FMA the same way.
#![allow(clippy::suboptimal_flops)]
// `(bits as f32) / (1u32 << 24) as f32` — 24-bit mantissa fits f32 exactly
// by construction (we shifted to ensure ≤ 24 bits) ; cast is precision-safe.
#![allow(clippy::cast_precision_loss)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::affixes::{AffixDescriptor, AffixKind, Prefix, Suffix};
use crate::base::{BaseItem, ItemClass};
use crate::rarity::Rarity;
use crate::slots::StatKind;

// ───────────────────────────────────────────────────────────────────────
// § DetRng — SplitMix64 PRNG
// ───────────────────────────────────────────────────────────────────────

/// Deterministic 64-bit PRNG. SplitMix64-style. Public-domain. Branch-free.
/// Seeded from u128 by XOR-fold of (high ⊕ low).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetRng {
    state: u64,
}

impl DetRng {
    /// Construct from u128 seed via XOR-fold.
    #[must_use]
    pub const fn new(seed: u128) -> Self {
        let folded = ((seed >> 64) as u64) ^ (seed as u64);
        // Avoid all-zero state (SplitMix64 produces 0,0,0,... if state=0).
        let state = if folded == 0 { 0x9E37_79B9_7F4A_7C15 } else { folded };
        Self { state }
    }

    /// Construct from a raw u64 state (for sub-RNG branching off a parent's u64).
    #[must_use]
    pub const fn from_state(state: u64) -> Self {
        let s = if state == 0 { 0x9E37_79B9_7F4A_7C15 } else { state };
        Self { state: s }
    }

    /// Advance + return next u64. SplitMix64 reference implementation.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform f32 ∈ [0.0, 1.0). High-24 bits of next u64 → mantissa.
    pub fn next_f32(&mut self) -> f32 {
        let bits = (self.next_u64() >> 40) as u32; // 24-bit mantissa
        (bits as f32) / ((1u32 << 24) as f32)
    }

    /// Uniform u32 ∈ [0, n). Rejects high-pad to avoid modulo-bias.
    pub fn next_u32_below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let limit = u32::MAX - (u32::MAX % n);
        loop {
            let v = self.next_u64() as u32;
            if v < limit {
                return v % n;
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § RolledAffix
// ───────────────────────────────────────────────────────────────────────

/// One rolled affix-instance : descriptor + final f32 value + seed-of-roll.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RolledAffix {
    /// Source descriptor ; carries kind + stat-kind + range.
    pub descriptor: AffixDescriptor,
    /// Final rolled value (clamped to descriptor.range).
    pub value: f32,
    /// Tier-band that produced this roll (1..=6). Audit-trace.
    pub tier: u8,
    /// Per-roll seed. NEW-seed on reroll.
    pub seed: u64,
}

// ───────────────────────────────────────────────────────────────────────
// § tier-curve  — per-tier sub-range of affix range
// ───────────────────────────────────────────────────────────────────────

/// Tier-curve fractions per GDD § STAT-ROLLING § per-affix-roll-range.
///   tier-1 : 0..20%  · tier-2 : 20..40% · tier-3 : 40..60%
///   tier-4 : 60..75% · tier-5 : 75..90% · tier-6 : 90..100%
#[must_use]
pub const fn tier_curve(tier: u8) -> (f32, f32) {
    match tier {
        1 => (0.00, 0.20),
        2 => (0.20, 0.40),
        3 => (0.40, 0.60),
        4 => (0.60, 0.75),
        5 => (0.75, 0.90),
        6 => (0.90, 1.00),
        _ => (0.00, 0.20), // clamp invalid → tier-1
    }
}

// ───────────────────────────────────────────────────────────────────────
// § roll_affix
// ───────────────────────────────────────────────────────────────────────

/// Roll one affix at the given tier. Deterministic per `rng` state.
///
/// Returns the rolled f32, clamped to (range_min, range_max). Mythic-floor
/// (`tier == 6`) is max-roll-floor : returns the high-end of the tier-curve
/// sub-range (top of 90..100%) deterministically — caller passes Mythic via
/// `roll_gear` which feeds `tier=6`.
pub fn roll_affix(rng: &mut DetRng, affix: &AffixDescriptor, tier: u8) -> f32 {
    let (range_lo, range_hi) = affix.range;
    let span = range_hi - range_lo;
    let (curve_lo, curve_hi) = tier_curve(tier);
    let curve_span = curve_hi - curve_lo;

    if tier == 6 {
        // Mythic max-roll-floor : top of 90..100% — deterministic.
        let frac = curve_lo + curve_span; // = 1.0
        return range_lo + span * frac;
    }

    // Uniform within the tier-curve sub-range.
    let frac = curve_lo + rng.next_f32() * curve_span;
    let raw = range_lo + span * frac;

    // Clamp to range. (curve_hi never exceeds 1.0 so raw ≤ range_hi.)
    if raw < range_lo { range_lo } else if raw > range_hi { range_hi } else { raw }
}

// ───────────────────────────────────────────────────────────────────────
// § roll_gear  — full Gear assembly from base + rarity + seed
// ───────────────────────────────────────────────────────────────────────

/// Roll a complete Gear instance from `base` × `rarity` × `seed`.
/// Deterministic + replay-bit-equal : same args → bit-identical output.
///
/// Affix-count : Common 1P+1S · Uncommon 1P+1S · Rare 2P+1S · Epic 2P+2S
/// · Legendary 2P+2S · Mythic 2P+2S (capped to `base.allowed_affixes`).
pub fn roll_gear(seed: u128, base: &BaseItem, rarity: Rarity) -> crate::gear::Gear {
    let mut rng = DetRng::new(seed);

    // Tier-band selection — deterministic per rarity.
    let (tier_lo, tier_hi) = rarity.tier_band();
    let tier = if tier_hi == tier_lo {
        tier_lo
    } else {
        // Pick within band uniformly. Mythic locked to 6 above.
        let span = (tier_hi - tier_lo + 1) as u32;
        tier_lo + (rng.next_u32_below(span) as u8)
    };

    // Affix counts per rarity (Q-06 8-tier · ≤ allowed_affixes).
    // Higher rarities ship MORE affixes — Prismatic + Chaotic at 3+3 = 6
    // require base.allowed_affixes ≥ 6 to fully populate.
    let (n_pre, n_suf) = match rarity {
        Rarity::Common => (1u8, 1u8),
        Rarity::Uncommon => (1, 1),
        Rarity::Rare => (2, 1),
        Rarity::Epic => (2, 2),
        Rarity::Legendary => (2, 2),
        Rarity::Mythic => (2, 2),
        Rarity::Prismatic => (3, 3), // Q-06 NEW · 6 affixes
        Rarity::Chaotic => (3, 3),   // Q-06 NEW · 6 affixes (Σ-mask wildcard pool)
    };
    let total = (n_pre + n_suf).min(base.allowed_affixes);
    let n_pre_eff = n_pre.min(total);
    let n_suf_eff = total - n_pre_eff;

    // Pick prefixes deterministically (rejection-sample to avoid duplicates).
    let prefix_pool = Prefix::all();
    let suffix_pool = Suffix::all();
    let mut chosen_prefixes: Vec<Prefix> = Vec::with_capacity(n_pre_eff as usize);
    while chosen_prefixes.len() < n_pre_eff as usize {
        let idx = rng.next_u32_below(prefix_pool.len() as u32) as usize;
        let p = prefix_pool[idx];
        if !chosen_prefixes.contains(&p) {
            chosen_prefixes.push(p);
        }
    }
    let mut chosen_suffixes: Vec<Suffix> = Vec::with_capacity(n_suf_eff as usize);
    while chosen_suffixes.len() < n_suf_eff as usize {
        let idx = rng.next_u32_below(suffix_pool.len() as u32) as usize;
        let s = suffix_pool[idx];
        if !chosen_suffixes.contains(&s) {
            chosen_suffixes.push(s);
        }
    }

    // Roll each affix.
    let prefixes: Vec<RolledAffix> = chosen_prefixes
        .iter()
        .map(|p| {
            let mut desc = p.descriptor();
            desc.tier_band = tier;
            let s = rng.next_u64();
            let value = roll_affix(&mut DetRng::from_state(s.wrapping_add(1)), &desc, tier);
            RolledAffix {
                descriptor: desc,
                value,
                tier,
                seed: s,
            }
        })
        .collect();

    let suffixes: Vec<RolledAffix> = chosen_suffixes
        .iter()
        .map(|s_kind| {
            let mut desc = s_kind.descriptor();
            desc.tier_band = tier;
            let s = rng.next_u64();
            let value = roll_affix(&mut DetRng::from_state(s.wrapping_add(1)), &desc, tier);
            RolledAffix {
                descriptor: desc,
                value,
                tier,
                seed: s,
            }
        })
        .collect();

    // Glyph-slots count. Inherit seed → glyph_slots roll.
    let glyph_count = crate::glyph_slots::roll_glyph_slots(seed, rarity);
    let glyph_slots: Vec<crate::glyph_slots::GlyphSlot> =
        (0..glyph_count).map(|_| crate::glyph_slots::GlyphSlot::empty()).collect();

    crate::gear::Gear {
        slot: base.slot,
        base: base.clone(),
        rarity,
        prefixes,
        suffixes,
        glyph_slots,
        item_level: 1,
        bound_to_player: false,
        seed,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § clamp_to_class_max
// ───────────────────────────────────────────────────────────────────────

/// Clamp every stat in `stats` to `base × class_max_multiplier()`. Returns
/// a NEW BTreeMap ; input untouched. For stats whose `base_stats` lack an
/// entry (affix-only stats e.g. FireDamage), the multiplier is applied
/// against the affix-derived value as `[0, multiplier × value]` no-op clamp.
#[must_use]
pub fn clamp_to_class_max(
    stats: &BTreeMap<StatKind, f32>,
    item_class: ItemClass,
    base_stats: &BTreeMap<StatKind, f32>,
) -> BTreeMap<StatKind, f32> {
    let mult = item_class.class_max_multiplier();
    let mut out = BTreeMap::new();
    for (k, v) in stats {
        let cap = base_stats
            .get(k)
            // Class-max-clamp : base × multiplier (anti-power-creep).
            // Affix-only stats (no base) → INFINITY pass-through.
            .map_or(f32::INFINITY, |b| (b.abs() * mult).max(b.abs()));
        let clamped = if v.is_sign_negative() {
            // Negative stats (e.g. -weight) : clamp on absolute magnitude.
            if -v > cap { -cap } else { *v }
        } else if *v > cap {
            cap
        } else {
            *v
        };
        out.insert(*k, clamped);
    }
    out
}

// ───────────────────────────────────────────────────────────────────────
// § Affix → Prefix/Suffix lookup helpers (used by `crate::upgrade::reroll`)
// ───────────────────────────────────────────────────────────────────────

/// Reverse-lookup : stat-kind → Prefix variant (first match by descriptor.stat_kind).
#[must_use]
pub fn prefix_for_descriptor(d: &AffixDescriptor) -> Option<Prefix> {
    if d.kind != AffixKind::Prefix {
        return None;
    }
    Prefix::all()
        .into_iter()
        .find(|p| p.descriptor().stat_kind == d.stat_kind)
}

/// Reverse-lookup : stat-kind → Suffix variant.
#[must_use]
pub fn suffix_for_descriptor(d: &AffixDescriptor) -> Option<Suffix> {
    if d.kind != AffixKind::Suffix {
        return None;
    }
    Suffix::all()
        .into_iter()
        .find(|s| s.descriptor().stat_kind == d.stat_kind)
}
