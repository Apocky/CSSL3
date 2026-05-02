//! § Rarity — 8-tier rarity ladder per Apocky-canonical Q-06 (2026-05-01).
//!
//! Common → Uncommon → Rare → Epic → Legendary → Mythic → Prismatic → Chaotic
//!
//! verbatim Apocky : "Add mythic, prismatic, chaotic, in that order of ascending rarity."
//!
//! Drop-curve (per Q-06 canonical · supersedes prior 5/6-tier) :
//!   Common 60% · Uncommon 25% · Rare 10% · Epic 4% · Legendary 0.9%
//!   · Mythic 0.09% · Prismatic 0.009% · Chaotic 0.001%
//!   sum = 100.000% (sums-to-unity exact at-bps-resolution)
//!
//! Glyph-slot table (per Q-06 + GDD § GLYPH-SLOTS extension) :
//!   Common 0 · Uncommon 0..1 · Rare 1 · Epic 1..2 · Legendary 2..3
//!   · Mythic 3..4 · Prismatic 4..5 · Chaotic 5..6
//!
//! `rarity_drop_floor(r)` returns the per-tier minimum probability used by the
//! drop-table sampler. Anti-spam invariants preserved per-tier.
//!
//! Tier-bias for stat-rolling (per § STAT-ROLLING § rarity ↔ tier-bias) extends :
//!   Mythic (6..6) · Prismatic (7..7) · Chaotic (8..8)
//!
//! § Q-06 propagation : 6-variant → 8-variant enum extension. Old 6-tier callers
//! continue to work — the enum is append-only at the high-rarity end.

use serde::{Deserialize, Serialize};

// ───────────────────────────────────────────────────────────────────────
// § Rarity enum (8-tier canonical · Apocky-Q-06 2026-05-01)
// ───────────────────────────────────────────────────────────────────────

/// Eight-tier rarity ladder. Ordered :
/// Common < Uncommon < Rare < Epic < Legendary < Mythic < Prismatic < Chaotic.
///
/// `Ord` derived ; lower-discriminant = lower-rarity. Useful for `>=` floor-checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rarity {
    /// Tier-1 common drop. 60.000% base-rate. 0 glyph-slots.
    Common,
    /// Tier-2 uncommon. 25.000% base-rate. 0..1 glyph-slots.
    Uncommon,
    /// Tier-3 rare. 10.000% base-rate. 1 glyph-slot.
    Rare,
    /// Tier-4 epic. 4.000% base-rate. 1..2 glyph-slots.
    Epic,
    /// Tier-5 legendary. 0.900% base-rate. 2..3 glyph-slots. Bond-eligible.
    Legendary,
    /// Tier-6 mythic. 0.090% base-rate. 3..4 glyph-slots. Bond-eligible.
    /// Mythic = drop-only OR bond-locked ; transmute Legendary→Mythic FORBIDDEN.
    Mythic,
    /// Tier-7 prismatic (NEW · Apocky-Q-06 2026-05-01). 0.009% base-rate.
    /// 4..5 glyph-slots. Bond-eligible. drop-only · multi-element-resonance.
    /// Transmute Mythic→Prismatic FORBIDDEN (drop-only-or-bond).
    Prismatic,
    /// Tier-8 chaotic (NEW · Apocky-Q-06 2026-05-01). 0.001% base-rate.
    /// 5..6 glyph-slots. Bond-eligible. drop-only · wildcard-affix-pool.
    /// Transmute Prismatic→Chaotic FORBIDDEN (drop-only-or-bond).
    /// Σ-mask randomizes affix-pool from-ALL-tiers per cell.
    Chaotic,
}

impl Rarity {
    /// All eight tiers in canonical drop-floor order. Stable iteration for tests.
    #[must_use]
    pub const fn all() -> [Rarity; 8] {
        [
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Epic,
            Rarity::Legendary,
            Rarity::Mythic,
            Rarity::Prismatic,
            Rarity::Chaotic,
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
            Rarity::Prismatic => "prismatic",
            Rarity::Chaotic => "chaotic",
        }
    }

    /// True iff this rarity is bond-eligible (Legendary+ per GDD § BOND).
    /// Q-06 extends bond-eligibility to Mythic + Prismatic + Chaotic.
    #[must_use]
    pub const fn is_bond_eligible(self) -> bool {
        matches!(
            self,
            Rarity::Legendary | Rarity::Mythic | Rarity::Prismatic | Rarity::Chaotic
        )
    }

    /// True iff this rarity is drop-only (no transmute path leads here).
    /// Mythic + Prismatic + Chaotic are drop-only (or bond-locked).
    /// Q-06 : Mythic→Prismatic FORBIDDEN · Prismatic→Chaotic FORBIDDEN.
    #[must_use]
    pub const fn is_drop_only(self) -> bool {
        matches!(self, Rarity::Mythic | Rarity::Prismatic | Rarity::Chaotic)
    }

    /// Tier-bias band : `(min_tier, max_tier)` ∈ ⟦1..8⟧ for stat-rolling.
    /// Per GDD § STAT-ROLLING § rarity ↔ tier-bias (Q-06 extension) :
    ///   Common (1..2) · Uncommon (2..3) · Rare (3..4) · Epic (4..5)
    ///   · Legendary (5..6) · Mythic (6..6) · Prismatic (7..7) · Chaotic (8..8)
    #[must_use]
    pub const fn tier_band(self) -> (u8, u8) {
        match self {
            Rarity::Common => (1, 2),
            Rarity::Uncommon => (2, 3),
            Rarity::Rare => (3, 4),
            Rarity::Epic => (4, 5),
            Rarity::Legendary => (5, 6),
            Rarity::Mythic => (6, 6),
            Rarity::Prismatic => (7, 7),
            Rarity::Chaotic => (8, 8),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § rarity_drop_floor (Q-06 8-tier canonical)
// ───────────────────────────────────────────────────────────────────────

/// Per-rarity drop-floor probability per Apocky-Q-06 canonical (2026-05-01).
/// Drop-curve sums to 1.000 exactly at-bps-resolution :
///   60.000% + 25.000% + 10.000% + 4.000% + 0.900% + 0.090% + 0.009% + 0.001% = 100.000%
///
/// f32-stored rates approximate within 1e-4 tolerance ; tested in `rarity_drop_floor.rs`.
///
/// Anti-spam invariants per-tier :
///   `rarity_drop_floor(Mythic)    <= 0.001`   (Q-06)
///   `rarity_drop_floor(Prismatic) <= 0.0001`  (Q-06 NEW)
///   `rarity_drop_floor(Chaotic)   <= 0.00001` (Q-06 NEW · most-rare)
#[must_use]
pub fn rarity_drop_floor(r: Rarity) -> f32 {
    match r {
        Rarity::Common => 0.60,
        Rarity::Uncommon => 0.25,
        Rarity::Rare => 0.10,
        Rarity::Epic => 0.04,
        Rarity::Legendary => 0.009,
        Rarity::Mythic => 0.0009,
        Rarity::Prismatic => 0.00009,
        Rarity::Chaotic => 0.00001,
    }
}
