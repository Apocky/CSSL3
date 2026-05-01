//! § stat_rolling_seeded tests — replay-bit-equal · clamp-to-class-max ·
//!   tier-curve · Mythic max-roll-floor.

#![allow(clippy::suboptimal_flops)]

use cssl_host_gear_archetype::{
    clamp_to_class_max, roll_affix, roll_gear, tier_curve, BaseItem, BaseMat, DetRng, GearSlot,
    ItemClass, Prefix, Rarity, StatKind,
};
use std::collections::BTreeMap;

#[test]
fn roll_gear_replay_bit_equal_round_trip() {
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Mithril, 25.0, 1.2);
    let g1 = roll_gear(0xCAFE_BEEF_DEAD_F00D_u128, &base, Rarity::Rare);
    let g2 = roll_gear(0xCAFE_BEEF_DEAD_F00D_u128, &base, Rarity::Rare);
    assert_eq!(g1, g2, "same seed/base/rarity must produce bit-equal Gear");
    // Serde round-trip stable.
    let j = serde_json::to_string(&g1).expect("serde");
    let back: cssl_host_gear_archetype::Gear = serde_json::from_str(&j).expect("deser");
    assert_eq!(g1, back);
}

#[test]
fn roll_gear_distinct_seeds_diverge() {
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Iron, 10.0, 1.0);
    let g1 = roll_gear(1, &base, Rarity::Rare);
    let g2 = roll_gear(2, &base, Rarity::Rare);
    // At minimum, prefix or suffix or value must differ across seeds 1 vs 2.
    let same = g1 == g2;
    assert!(!same, "distinct seeds (1, 2) should not produce identical gear");
}

#[test]
fn mythic_tier_six_always_max_roll_floor() {
    // GDD : Mythic always tier-6 ; Mythic max-roll-floor.
    // Every rolled affix value ≥ tier-6-curve-low × range-span + range-lo.
    let base = BaseItem::armor(GearSlot::Helm, BaseMat::Soulbound, 50.0);
    let g = roll_gear(0xABCD_1234_u128, &base, Rarity::Mythic);
    let (curve_lo, curve_hi) = tier_curve(6);
    for r in g.prefixes.iter().chain(g.suffixes.iter()) {
        assert_eq!(r.tier, 6, "Mythic affix tier must be 6, got {}", r.tier);
        let (lo, hi) = r.descriptor.range;
        let expected_floor = lo + (hi - lo) * curve_lo;
        let _ = curve_hi;
        // Mythic uses the deterministic top of curve (1.0) ; value ≥ floor.
        assert!(
            r.value >= expected_floor - 1e-3,
            "mythic affix value {} below tier-6 floor {expected_floor}",
            r.value
        );
    }
}

#[test]
fn clamp_to_class_max_caps_at_one_point_five_x() {
    // Anti-power-creep cap : ≤ 1.50 × base.
    let mut base_stats = BTreeMap::new();
    base_stats.insert(StatKind::Damage, 100.0_f32);
    let mut stats = BTreeMap::new();
    stats.insert(StatKind::Damage, 200.0_f32); // 2.0× base — should clamp to 150.
    let clamped = clamp_to_class_max(&stats, ItemClass::Weapon, &base_stats);
    let v = clamped.get(&StatKind::Damage).copied().unwrap();
    assert!(
        (v - 150.0).abs() < 1e-3,
        "expected clamp to 150.0 (1.5× base), got {v}"
    );

    // Sub-cap value passes through.
    stats.insert(StatKind::Damage, 120.0);
    let clamped = clamp_to_class_max(&stats, ItemClass::Weapon, &base_stats);
    let v = clamped.get(&StatKind::Damage).copied().unwrap();
    assert!((v - 120.0).abs() < 1e-3);

    // Affix-only stat (no base) passes through unbounded.
    stats.insert(StatKind::FireDamage, 0.5);
    let clamped = clamp_to_class_max(&stats, ItemClass::Weapon, &base_stats);
    let v = clamped.get(&StatKind::FireDamage).copied().unwrap();
    assert!((v - 0.5).abs() < 1e-3);

    // Sanity : roll_affix bounded.
    let mut rng = DetRng::new(99);
    for _ in 0..50 {
        let d = Prefix::Burning.descriptor();
        let v = roll_affix(&mut rng, &d, 3);
        assert!(v >= d.range.0 && v <= d.range.1, "rolled out of range");
    }
}
