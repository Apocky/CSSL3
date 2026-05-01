// § quality_tier : 6 quality-tiers + skill→tier-shift mapping
// ══════════════════════════════════════════════════════════════════
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § QUALITY-TIERS` :
//!
//! - Q1 Common      ×1.00 · 0 slot
//! - Q2 Fine        ×1.10 · 0..1 slots
//! - Q3 Superior    ×1.20 · 1..2 slots
//! - Q4 Master      ×1.30 · 2..3 slots
//! - Q5 Heroic      ×1.40 · 3 slots + named-author
//! - Q6 Legendary   ×1.50 · 3 slots + lineage-recorded + Audit<"legendary-craft", ω-step>
//!
//! § INVARIANT : stat-multiplier ≤ 1.50 (anti-power-creep). Higher-skill never
//! produces raw-damage-up beyond the tier-cap — only tier-shifts up.

use serde::{Deserialize, Serialize};

/// § QualityTier : 6-tier output classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QualityTier {
    Common,
    Fine,
    Superior,
    Master,
    Heroic,
    Legendary,
}

impl QualityTier {
    /// § stat_multiplier : per-tier stat-multiplier.
    ///
    /// ‼ INVARIANT : ≤ 1.50 always.
    #[must_use]
    pub fn stat_multiplier(self) -> f32 {
        match self {
            QualityTier::Common => 1.00,
            QualityTier::Fine => 1.10,
            QualityTier::Superior => 1.20,
            QualityTier::Master => 1.30,
            QualityTier::Heroic => 1.40,
            QualityTier::Legendary => 1.50,
        }
    }

    /// § as_index : 1..6 ordinal mapping (matches GDD Q1..Q6 numbering).
    #[must_use]
    pub fn as_index(self) -> u8 {
        match self {
            QualityTier::Common => 1,
            QualityTier::Fine => 2,
            QualityTier::Superior => 3,
            QualityTier::Master => 4,
            QualityTier::Heroic => 5,
            QualityTier::Legendary => 6,
        }
    }
}

/// § quality_tier_for_skill : deterministic skill+roll → quality-tier.
///
/// Per GDD § THRESHOLDS (P of next-tier · roll-down on miss · ¬ flat-fail) :
/// - skill ≥ 0   ⇒ Common (always)
/// - skill ≥ 20  ⇒ Fine        P 0.30
/// - skill ≥ 40  ⇒ Superior    P 0.25
/// - skill ≥ 60  ⇒ Master      P 0.20
/// - skill ≥ 80  ⇒ Heroic      P 0.15
/// - skill ≥ 100 ⇒ Legendary   P 0.10
///
/// `roll` ∈ ⟦0.0, 1.0⟩ is supplied by the caller (deterministic seeded RNG).
/// Lower `roll` = better outcome (matches probability-of-success convention).
///
/// `base_tier` is the recipe's nominal output_tier (1..6) ; this caps the
/// quality-tier a skill-roll can shift to (you can't produce Legendary from
/// a tier-1 base recipe — the GDD treats quality-tier as a per-craft modifier
/// over the recipe's structural tier).
#[must_use]
pub fn quality_tier_for_skill(skill: u8, base_tier: u8, roll: f32) -> QualityTier {
    let s = skill.min(100);
    let cap = base_tier.clamp(1, 6);

    // § Walk thresholds top-down ; first-match wins.
    let candidate = if s >= 100 && roll < 0.10 {
        QualityTier::Legendary
    } else if s >= 80 && roll < 0.15 {
        QualityTier::Heroic
    } else if s >= 60 && roll < 0.20 {
        QualityTier::Master
    } else if s >= 40 && roll < 0.25 {
        QualityTier::Superior
    } else if s >= 20 && roll < 0.30 {
        QualityTier::Fine
    } else {
        QualityTier::Common
    };

    // § Cap candidate at base_tier (anti-power-creep + structural-tier gate).
    let candidate_idx = candidate.as_index();
    if candidate_idx <= cap {
        candidate
    } else {
        match cap {
            1 => QualityTier::Common,
            2 => QualityTier::Fine,
            3 => QualityTier::Superior,
            4 => QualityTier::Master,
            5 => QualityTier::Heroic,
            _ => QualityTier::Legendary,
        }
    }
}
