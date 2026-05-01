// § tests : skill-scaling = quality-tier-shift ; ≤ 1.50× cap (anti-power-creep)
// ══════════════════════════════════════════════════════════════════
use cssl_host_craft_graph::quality_tier::{quality_tier_for_skill, QualityTier};
use cssl_host_craft_graph::skill::{apply_xp, quality_tier_shift_prob, CraftSkill};

#[test]
fn t_quality_tier_shift_prob_bounded() {
    // GDD § THRESHOLDS : highest probability is 0.30 (Fine @ skill 20).
    // Note : function returns highest-tier-shift probability the skill enables ;
    // for skill ≥ 20 with target_tier ≥ 2, returns 0.30.
    for skill in 0u8..=100u8 {
        for target in 1u8..=6u8 {
            let p = quality_tier_shift_prob(skill, target);
            assert!((0.0..=0.30).contains(&p), "P {p} out of [0, 0.30] @ skill={skill} target={target}");
        }
    }
}

#[test]
fn t_quality_tier_never_exceeds_1_50_multiplier() {
    // ‼ INVARIANT : stat-multiplier ≤ 1.50 (anti-power-creep, GDD § INVARIANT).
    // Even with skill-100 + lowest-roll, Legendary caps at 1.50×.
    for skill in 0u8..=100u8 {
        for tier in 1u8..=6u8 {
            for roll_int in 0..100 {
                let roll = roll_int as f32 / 100.0;
                let qt = quality_tier_for_skill(skill, tier, roll);
                assert!(qt.stat_multiplier() <= 1.50, "multiplier exceeded 1.50 @ skill={} tier={} roll={}", skill, tier, roll);
            }
        }
    }
}

#[test]
fn t_quality_tier_thresholds_per_gdd() {
    // GDD : skill ≥ 0 ⇒ Common ; skill ≥ 20 ⇒ Fine @ P 0.30 ; etc.
    // High-roll always lands Common (fail-down).
    assert_eq!(quality_tier_for_skill(50, 6, 0.99), QualityTier::Common);
    // skill 20 + low-roll + tier-2-cap = Fine.
    assert_eq!(quality_tier_for_skill(20, 6, 0.05), QualityTier::Fine);
    // skill 100 + low-roll + tier-6-cap = Legendary.
    assert_eq!(quality_tier_for_skill(100, 6, 0.05), QualityTier::Legendary);
    // base-tier cap : skill 100 + tier-1-recipe = Common (capped down).
    assert_eq!(quality_tier_for_skill(100, 1, 0.05), QualityTier::Common);
}

#[test]
fn t_apply_xp_diminishing_returns() {
    // GDD § XP-CURVE : higher skill → less XP per craft.
    let s0 = CraftSkill::with_level(0);
    let s50 = CraftSkill::with_level(50);

    // Same recipe-tier ; skill-50 should gain less XP than skill-0.
    let s0_after = apply_xp(s0, 6);
    let s50_after = apply_xp(s50, 6);

    // Compute pure XP delta (ignore level-up consumption from s0).
    // For diminishing-returns, low-skill input level should match expected curve.
    // Level-0 : numerator = 6 × 10 × 25 = 1500 ; denom = 25 ; xp = 60.
    // Level-50 : numerator = 1500 ; denom = 75 ; xp = 20.
    assert!(s0_after.xp >= 60 || s0_after.level > 0, "skill-0 should gain ≥ 60 xp or level-up");
    assert_eq!(s50_after.xp, 20, "skill-50 should gain exactly 20 xp from tier-6 recipe");
    assert_eq!(s50_after.level, 50, "skill-50 should not level-up from one tier-6 craft");
}
