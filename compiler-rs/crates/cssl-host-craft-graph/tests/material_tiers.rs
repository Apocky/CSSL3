// § tests : material-pool tier-correctness + 33-mat-floor
// ══════════════════════════════════════════════════════════════════
use cssl_host_craft_graph::material::{
    all_materials, material_category, material_tier, Material, MaterialCategory,
};

#[test]
fn t_at_least_33_materials() {
    // GDD § MATERIAL-POOL : 33 base mats · 6 categories.
    let mats = all_materials();
    assert!(mats.len() >= 33, "GDD requires ≥ 33 materials, found {}", mats.len());
}

#[test]
fn t_tier_bounds() {
    for &m in &all_materials() {
        let t = material_tier(m);
        assert!((1..=6).contains(&t), "tier {t} out of bounds for {m:?}");
    }
}

#[test]
fn t_known_tier_anchors_match_gdd() {
    // Per GDD § MATERIAL-POOL spot-checks :
    assert_eq!(material_tier(Material::Iron), 1);
    assert_eq!(material_tier(Material::Soulalloy), 6); // METAL T6
    assert_eq!(material_tier(Material::Voidweave), 5); // CLOTH T5
    assert_eq!(material_tier(Material::Catalystgem), 6); // GEM T6
    assert_eq!(material_tier(Material::Phaseether), 6); // CATALYST T6
    assert_eq!(material_category(Material::Iron), MaterialCategory::Metal);
    assert_eq!(material_category(Material::Phaseether), MaterialCategory::Catalyst);
    assert_eq!(material_category(Material::Spidersilk), MaterialCategory::Cloth);
}
