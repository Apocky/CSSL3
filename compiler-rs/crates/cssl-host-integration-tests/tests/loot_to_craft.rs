// § loot_to_craft — gear-loot → craft-graph deconstruct → craft-attempt.
// ════════════════════════════════════════════════════════════════════
// § Coverage : material flow from a dropped Gear through deconstruct, then
//   the recovered materials feeding a craft-attempt with skill-curve check.

use cssl_host_craft_graph as craft;
use cssl_host_gear_archetype as gear;

use cssl_host_integration_tests::{
    craft_a_t1_weapon, deconstruct_a_crafted_item, material_to_basemat,
};

/// (a) Dropped Gear → craft-graph CraftedItem → deconstruct returns mats
///     consistent with the Gear's base-mat.
#[test]
fn dropped_gear_feeds_deconstruct() {
    let ctx = gear::DropContext {
        mob_tier: 4,
        biome: gear::Biome::Forge,
        magic_find: 0.0,
    };
    let g = gear::roll_drop(&ctx, 0xFEED_FACE, Some(gear::GearSlot::MainHand))
        .expect("roll_drop never returns None for non-empty ctx");
    // Map gear::BaseMat → craft::Material so we can route through deconstruct.
    let mat = match g.base.base_mat {
        gear::BaseMat::Iron => craft::Material::Iron,
        gear::BaseMat::Silver => craft::Material::Silver,
        gear::BaseMat::Mithril => craft::Material::Mithril,
        gear::BaseMat::Adamant => craft::Material::Adamant,
        gear::BaseMat::Voidsteel => craft::Material::Voidsteel,
        gear::BaseMat::Soulbound => craft::Material::Soulalloy,
    };
    let result = deconstruct_a_crafted_item(mat, 4, 50, 50);
    assert!(
        !result.returned_mats.is_empty(),
        "deconstruct must return ≥ 1 material from a non-empty lineage"
    );
    // Anti-frustration : recovery_fraction ∈ [0.30, 0.70].
    assert!(
        (0.30..=0.70).contains(&result.recovery_fraction),
        "recovery_fraction {} outside GDD-clamped range",
        result.recovery_fraction
    );
    // Returned mat == fed mat (lineage preservation).
    assert_eq!(result.returned_mats[0].0, mat);
}

/// (b) Deconstructed materials feed a craft attempt : the recovered Iron
///     mat is sufficient for the R05 sword-T1 recipe.
#[test]
fn deconstructed_mats_feed_craft_attempt() {
    let result = deconstruct_a_crafted_item(craft::Material::Iron, 6, 70, 80);
    let recovered_iron: u8 = result
        .returned_mats
        .iter()
        .filter(|(m, _)| *m == craft::Material::Iron)
        .map(|&(_, n)| n)
        .sum();
    assert!(recovered_iron >= 1, "expect ≥ 1 Iron from a 6-Iron lineage");

    // Pull R05 (sword-T1) from default-graph and confirm Iron count satisfies
    // the recipe's ingredient requirement.
    let g = craft::default_recipe_graph();
    let r05 = g.get(5).expect("R05 sword-T1 must exist in default recipes");
    let need_iron: u8 = r05
        .ingredients
        .iter()
        .filter(|(m, _)| *m == craft::Material::Iron)
        .map(|&(_, n)| n)
        .sum();
    assert!(
        recovered_iron >= need_iron,
        "{recovered_iron} Iron recovered ; recipe needs {need_iron}"
    );
}

/// (c) Craft-output rarity behaves correctly with the skill-curve : at skill
///     0, no tier-shift probability ; at skill 80+, non-zero shift-prob into
///     tier-5. Validates the GDD anti-power-creep rule (≤ 1.50× cap implicit
///     in tier-cap discipline).
#[test]
fn craft_output_rarity_from_skill_curve() {
    // Skill = 0 : zero tier-shift probability for any target tier.
    assert!(
        craft::quality_tier_shift_prob(0, 5).abs() < f32::EPSILON,
        "skill 0 must yield zero tier-shift probability"
    );
    // Skill = 80, target = 5 : nonzero per GDD threshold table.
    let p = craft::quality_tier_shift_prob(80, 5);
    assert!(
        p > 0.0 && p <= 0.30,
        "skill 80 → target 5 must give p ∈ (0, 0.30] ; got {p}"
    );

    // Cross-cargo wiring : craft a T1 weapon at Common rarity using the
    // material_to_basemat coercion ; confirm its rarity-floor matches.
    let mat = material_to_basemat(craft::Material::Iron);
    assert_eq!(mat, gear::BaseMat::Iron);
    let g = craft_a_t1_weapon(0xABC_DEF, gear::Rarity::Common);
    assert!(
        g.rarity >= g.base.base_mat.rarity_floor(),
        "crafted gear rarity {:?} below mat-floor {:?}",
        g.rarity,
        g.base.base_mat.rarity_floor()
    );
}
