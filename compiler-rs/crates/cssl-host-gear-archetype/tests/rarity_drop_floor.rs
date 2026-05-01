//! § rarity_drop_floor tests — per GDD § DROP-TABLES § CHEST-LOOT base-curve.

use cssl_host_gear_archetype::{rarity_drop_floor, Rarity};

#[test]
fn mythic_drop_floor_at_or_below_anti_spam_invariant() {
    // GDD M-2 : Mythic ≤ 0.01% guaranteed-not-time-gated.
    let p = rarity_drop_floor(Rarity::Mythic);
    assert!(p <= 0.0001, "mythic drop-floor {p} exceeds 0.0001");
    assert!(p > 0.0, "mythic drop-floor must be positive ; floor not gate");
}

#[test]
fn drop_floors_strictly_descending() {
    // Common → Mythic must monotonically decrease.
    let common = rarity_drop_floor(Rarity::Common);
    let uncommon = rarity_drop_floor(Rarity::Uncommon);
    let rare = rarity_drop_floor(Rarity::Rare);
    let epic = rarity_drop_floor(Rarity::Epic);
    let legendary = rarity_drop_floor(Rarity::Legendary);
    let mythic = rarity_drop_floor(Rarity::Mythic);
    assert!(common > uncommon);
    assert!(uncommon > rare);
    assert!(rare > epic);
    assert!(epic > legendary);
    assert!(legendary > mythic);
}

#[test]
fn drop_floors_sum_to_unity_within_tolerance() {
    let total: f32 = Rarity::all().iter().copied().map(rarity_drop_floor).sum();
    let diff = (total - 1.0).abs();
    assert!(diff < 1e-4, "drop-floor sum {total} not near 1.0 (diff {diff})");
}
