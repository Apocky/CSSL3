// § skill : CraftSkill + diminishing-returns XP + tier-shift probability
// ══════════════════════════════════════════════════════════════════
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § SKILL-SCALING` :
//!
//! - Skill ∈ ⟦0, 100⟧, clamp on overflow.
//! - XP-curve : `xp_gain = (action_tier × 10) / (1 + skill/25)` ; diminishing.
//! - Threshold : `100 × (skill+1)²` xp to advance one level.
//! - skill-scaling = quality-tier-shift, NOT raw-stat-multiplier.
//! - INVARIANT : effective stat-multiplier ≤ 1.50 (anti-power-creep).

use serde::{Deserialize, Serialize};

/// § CraftSkill : per-axis skill state ∈ ⟦0, 100⟧ + accumulated XP.
///
/// GDD § AXES distinguishes craft-skill / deconstruct-skill / alchemy-skill /
/// enchant-skill — this struct represents one axis. The caller maintains
/// per-axis instances.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CraftSkill {
    pub level: u8,
    pub xp: u32,
}

impl Default for CraftSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl CraftSkill {
    /// § new : level=0, xp=0.
    #[must_use]
    pub fn new() -> Self {
        Self { level: 0, xp: 0 }
    }

    /// § with_level : preset for tests + character-loading.
    #[must_use]
    pub fn with_level(level: u8) -> Self {
        Self {
            level: level.min(100),
            xp: 0,
        }
    }

    /// § threshold : xp required to advance from level → level+1.
    /// Per GDD : `100 × (skill+1)²`.
    #[must_use]
    pub fn threshold(&self) -> u32 {
        let next = u32::from(self.level) + 1;
        100u32.saturating_mul(next.saturating_mul(next))
    }
}

/// § quality_tier_shift_prob : probability that skill produces a tier-shift on craft.
///
/// Per GDD § THRESHOLDS, this is the per-tier "P of next-tier" probability.
/// Returns the BEST tier-shift probability the skill enables, capped at the
/// target_tier (no tier-shift past requested item-tier).
///
/// Returns ∈ [0.0, 0.30] (matches highest GDD threshold = Fine @ 0.30).
#[must_use]
pub fn quality_tier_shift_prob(skill: u8, target_tier: u8) -> f32 {
    let s = skill.min(100);
    let cap = target_tier.clamp(1, 6);

    // § Walk top-down ; the highest-tier shift the skill enables AND that the
    // target_tier permits wins.
    if s >= 100 && cap >= 6 {
        0.10
    } else if s >= 80 && cap >= 5 {
        0.15
    } else if s >= 60 && cap >= 4 {
        0.20
    } else if s >= 40 && cap >= 3 {
        0.25
    } else if s >= 20 && cap >= 2 {
        0.30
    } else {
        0.0
    }
}

/// § apply_xp : grant XP for a craft action ; level up if threshold reached.
///
/// Per GDD § XP-CURVE : `xp_gain = (action_tier × 10) / (1 + skill/25)`.
/// Diminishing-returns : higher skill = less XP per craft.
///
/// Skill clamps at 100. XP carries over across level-ups.
#[must_use]
pub fn apply_xp(mut skill: CraftSkill, recipe_tier: u8) -> CraftSkill {
    let tier = u32::from(recipe_tier.clamp(1, 6));
    // Use integer arithmetic for determinism : numerator scaled by 25.
    let numerator = tier.saturating_mul(10).saturating_mul(25);
    let denominator = 25u32.saturating_add(u32::from(skill.level));
    let xp_gain = numerator / denominator.max(1);

    skill.xp = skill.xp.saturating_add(xp_gain);

    // § Level-up loop : drain xp through thresholds.
    while skill.level < 100 {
        let threshold = skill.threshold();
        if skill.xp >= threshold {
            skill.xp -= threshold;
            skill.level += 1;
        } else {
            break;
        }
    }

    skill
}
