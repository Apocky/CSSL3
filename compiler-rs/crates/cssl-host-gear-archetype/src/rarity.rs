//! § Rarity — 6-tier rarity ladder per GDDs/GEAR_RARITY_SYSTEM.csl § AXIOMS.
//!
//! Common → Uncommon → Rare → Epic → Legendary → Mythic
//!
//! Drop-floor binding (per § DROP-TABLES § CHEST-LOOT base-curve mob-tier-1) :
//!   Common 60% · Uncommon 28% · Rare 9% · Epic 2.5% · Legendary 0.49% · Mythic 0.01%
//!
//! `rarity_drop_floor(r)` returns the per-tier minimum probability used by the
//! drop-table sampler. Mythic ≤ 0.0001 (= 0.01%) per the GDD anti-spam invariant.
//!
//! Tier-bias for stat-rolling (per § STAT-ROLLING § rarity ↔ tier-bias) is encoded
//! as `(min_tier, max_tier)` pairs, used by `crate::stat_rolling`.

use serde::{Deserialize, Serialize};

// ───────────────────────────────────────────────────────────────────────
// § Rarity enum
// ───────────────────────────────────────────────────────────────────────

/// Six-tier rarity ladder. Ordered : Common < Uncommon < Rare < Epic < Legendary < Mythic.
///
/// `Ord` derived ; lower-discriminant = lower-rarity. Useful for `>=` floor-checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rarity {
    /// Tier-1 common drop. 60% base-rate (mob-tier-1). 0 glyph-slots.
    Common,
    /// Tier-2 uncommon. 28% base-rate. 0..1 glyph-slots.
    Uncommon,
    /// Tier-3 rare. 9% base-rate. 1 glyph-slot.
    Rare,
    /// Tier-4 epic. 2.5% base-rate. 1..2 glyph-slots.
    Epic,
    /// Tier-5 legendary. 0.49% base-rate. 2..3 glyph-slots. Bond-eligible.
    Legendary,
    /// Tier-6 mythic. ≤0.01% base-rate (anti-spam floor). 3 glyph-slots.
    /// Mythic = drop-only OR bond-locked ; transmute Legendary→Mythic FORBIDDEN.
    Mythic,
}

impl Rarity {
    /// All six tiers in canonical drop-floor order. Stable iteration for tests.
    #[must_use]
    pub const fn all() -> [Rarity; 6] {
        [
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Epic,
            Rarity::Legendary,
            Rarity::Mythic,
        ]
    }

    /// Stable name for audit-event payloads + serde-key uses.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Rarity::Common => "common",
            Rarity::Uncommon => "uncommon",
            Rarity::Rare => "rare",
            Rarity::Epic => "epic",
            Rarity::Legendary => "legendary",
            Rarity::Mythic => "mythic",
        }
    }

    /// True iff this rarity is bond-eligible (Legendary or Mythic per GDD § BOND).
    #[must_use]
    pub const fn is_bond_eligible(self) -> bool {
        matches!(self, Rarity::Legendary | Rarity::Mythic)
    }

    /// Tier-bias band : `(min_tier, max_tier)` ∈ ⟦1..6⟧ for stat-rolling.
    /// Per GDD § STAT-ROLLING § rarity ↔ tier-bias :
    ///   Common (1..2) · Uncommon (2..3) · Rare (3..4) · Epic (4..5) · Legendary (5..6) · Mythic (6..6)
    #[must_use]
    pub const fn tier_band(self) -> (u8, u8) {
        match self {
            Rarity::Common => (1, 2),
            Rarity::Uncommon => (2, 3),
            Rarity::Rare => (3, 4),
            Rarity::Epic => (4, 5),
            Rarity::Legendary => (5, 6),
            Rarity::Mythic => (6, 6),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § rarity_drop_floor
// ───────────────────────────────────────────────────────────────────────

/// Per-rarity drop-floor probability for mob-tier-1 base-curve. Per GDD :
///   Common 0.60 · Uncommon 0.28 · Rare 0.09 · Epic 0.025 · Legendary 0.0049 · Mythic 0.0001
///
/// Sums to 1.0 (modulo f32-precision ; tested in `rarity_drop_floor.rs`).
///
/// Anti-spam invariant : `rarity_drop_floor(Mythic) <= 0.0001` (M-2 balance-metric).
#[must_use]
pub fn rarity_drop_floor(r: Rarity) -> f32 {
    match r {
        Rarity::Common => 0.60,
        Rarity::Uncommon => 0.28,
        Rarity::Rare => 0.09,
        Rarity::Epic => 0.025,
        Rarity::Legendary => 0.0049,
        Rarity::Mythic => 0.0001,
    }
}
