// § weapon_stats.rs — 3 tests on archetype-table integrity
// ════════════════════════════════════════════════════════════════════

use cssl_host_combat_sim::weapons::{stats_for, RangeClass, SpecialMoveId, WeaponArchetype};

#[test]
fn spear_long_range_with_thrust_hold() {
    let s = stats_for(WeaponArchetype::Spear);
    assert_eq!(s.range_class, RangeClass::Long);
    assert_eq!(s.special_move_id, SpecialMoveId::ThrustHold);
    assert!((s.reach - 2.4).abs() < 1e-3);
}

#[test]
fn shield_fist_special_is_shield_bash() {
    let s = stats_for(WeaponArchetype::ShieldFist);
    assert_eq!(s.special_move_id, SpecialMoveId::ShieldBash);
}

#[test]
fn all_eight_archetypes_distinct_specials() {
    let archs = [
        WeaponArchetype::Sword,
        WeaponArchetype::Axe,
        WeaponArchetype::Spear,
        WeaponArchetype::Dagger,
        WeaponArchetype::Bow,
        WeaponArchetype::Staff,
        WeaponArchetype::Hammer,
        WeaponArchetype::ShieldFist,
    ];
    let mut specials: Vec<_> = archs.iter().map(|a| stats_for(*a).special_move_id).collect();
    specials.sort_by_key(|m| format!("{m:?}"));
    specials.dedup();
    assert_eq!(specials.len(), 8, "expected 8 distinct special-moves, got {specials:?}");
}
