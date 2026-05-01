// § damage_affinity.rs — 3 tests on 9×8 affinity-matrix
// ════════════════════════════════════════════════════════════════════

use cssl_host_combat_sim::damage_types::{
    affinity_table, apply_affinity, ArmorClass, DamageRoll, DamageType, AFFINITY_COLS,
    AFFINITY_ROWS,
};

#[test]
fn affinity_table_dimensions_9x8() {
    let t = affinity_table();
    assert_eq!(t.len(), 9);
    assert_eq!(t.len(), AFFINITY_ROWS);
    assert_eq!(t[0].len(), 8);
    assert_eq!(t[0].len(), AFFINITY_COLS);
}

#[test]
fn fire_strong_against_cloth() {
    // ClothLight × Fire = Wk (1.5×)
    let d = apply_affinity(20.0, DamageType::Fire, ArmorClass::ClothLight);
    assert!((d - 30.0).abs() < 1e-3);
}

#[test]
fn composite_roll_sums_components() {
    let mut roll = DamageRoll::default();
    roll.components.push((DamageType::Slash, 21.0)); // 70%
    roll.components.push((DamageType::Fire, 9.0)); // 30%
    // PlateHeavy : Slash=Rs(0.6), Fire=Rs(0.6) ⇒ 21*0.6 + 9*0.6 = 18.0
    let d = roll.apply(ArmorClass::PlateHeavy);
    assert!((d - 18.0).abs() < 1e-3);
}
