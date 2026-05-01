// § tests : transmutation · catalyst-stack ≤ 0.95 cap · skill-gate · cross-cat
// ══════════════════════════════════════════════════════════════════
use cssl_host_craft_graph::alchemy::{
    catalyst_multiplier, transmute_tier, TransmuteErr,
};
use cssl_host_craft_graph::material::Material;

#[test]
fn t_transmute_success_clamp_at_0_95() {
    // GDD : Voidessence + Soulflux ⇒ stack 3.0× · clamp ≤ 0.95.
    // input_tier=2, skill=80 ⇒ base = 0.20 + 0.005×80 = 0.60 ; ×3.0 = 1.80 ; clamp 0.95.
    let mult = catalyst_multiplier(&[Material::Voidessence, Material::Soulflux]);
    assert!((mult - 3.0).abs() < 1e-6);

    let r = transmute_tier(2, 3, mult, 80, 0.50, false).expect("should succeed");
    assert!((r.success_prob - 0.95).abs() < 1e-6, "success_prob clamped to 0.95");
}

#[test]
fn t_transmute_skill_gate_blocks() {
    // GDD : transmute-skill ≥ 20×N. For input_tier=3, gate=60. Skill=10 ⇒ block.
    let err = transmute_tier(3, 3, 1.0, 10, 0.50, false).unwrap_err();
    assert_eq!(err, TransmuteErr::SkillBelowGate);
}

#[test]
fn t_transmute_cross_category_blocked() {
    // GDD § TRANSMUTATION § forbidden : ¬ transmute consumables ↔ equipment.
    let err = transmute_tier(2, 3, 1.5, 100, 0.10, true).unwrap_err();
    assert_eq!(err, TransmuteErr::CrossCategoryBlock);

    // Insufficient inputs branch.
    let err2 = transmute_tier(2, 2, 1.0, 100, 0.10, false).unwrap_err();
    assert_eq!(err2, TransmuteErr::InsufficientInputs);
}
