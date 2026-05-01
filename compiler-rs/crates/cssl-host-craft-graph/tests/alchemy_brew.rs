// § tests : alchemy brew · catalyst-stack ≤ 0.95 cap · cross-category-block
// ══════════════════════════════════════════════════════════════════
use cssl_host_craft_graph::alchemy::{brew, BrewErr};
use cssl_host_craft_graph::material::Material;

#[test]
fn t_brew_basic_success() {
    // R30 : Vial + Saltpeter + Quartz → Potion<HP+25>. Skill 0, no required-skill.
    let reagents = vec![(Material::Saltpeter, 1), (Material::Quartz, 1)];
    let r = brew(&reagents, &[], 0, 30, 0, 0.99).expect("R30 brew should succeed");
    assert!(!r.brewed_effect.is_empty());
    assert!(r.magnitude >= 25);
    assert_eq!(r.recipe_id, 30);
}

#[test]
fn t_brew_low_skill_explodes() {
    // GDD : skill < required ⇒ 50% bottle-explodes. Caller-roll < 0.50 ⇒ explosion.
    let reagents = vec![(Material::Mithrilshade, 1), (Material::Drakeskin, 1)];
    let err = brew(&reagents, &[], 10, 34, 40, 0.10).unwrap_err();
    assert_eq!(err, BrewErr::BottleExploded);

    // Roll ≥ 0.50 ⇒ skill-too-low (no explosion).
    let err2 = brew(&reagents, &[], 10, 34, 40, 0.75).unwrap_err();
    assert_eq!(err2, BrewErr::SkillTooLow);
}

#[test]
fn t_brew_cross_category_block_pure_metal() {
    // All-metal reagent list = equipment-domain ⇒ blocked (sanity-check).
    let reagents = vec![(Material::Iron, 1), (Material::Silver, 1)];
    let err = brew(&reagents, &[], 50, 99, 0, 0.99).unwrap_err();
    assert_eq!(err, BrewErr::CrossCategoryBlock);
}
