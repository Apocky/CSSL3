//! § rarity_drop_floor tests — Q-06 8-tier canonical (Apocky 2026-05-01).
//!
//! 8-tier drop-curve : 60/25/10/4/0.9/0.09/0.009/0.001 sums to 100.000% (bps-exact).

use cssl_host_gear_archetype::{rarity_drop_floor, Rarity};

#[test]
fn mythic_drop_floor_at_or_below_anti_spam_invariant() {
    // Q-06 : Mythic ≤ 0.001 (= 0.1%) guaranteed-not-time-gated.
    let p = rarity_drop_floor(Rarity::Mythic);
    assert!(p <= 0.001, "mythic drop-floor {p} exceeds 0.001");
    assert!(p > 0.0, "mythic drop-floor must be positive ; floor not gate");
}

#[test]
fn prismatic_drop_floor_at_or_below_anti_spam_invariant() {
    // Q-06 NEW : Prismatic ≤ 0.0001 (= 0.01%) anti-spam.
    let p = rarity_drop_floor(Rarity::Prismatic);
    assert!(p <= 0.0001, "prismatic drop-floor {p} exceeds 0.0001");
    assert!(p > 0.0, "prismatic drop-floor must be positive");
}

#[test]
fn chaotic_drop_floor_most_rare() {
    // Q-06 NEW : Chaotic ≤ 0.00001 (= 0.001%) most-rare anti-spam.
    let p = rarity_drop_floor(Rarity::Chaotic);
    assert!(p <= 0.00001, "chaotic drop-floor {p} exceeds 0.00001");
    assert!(p > 0.0, "chaotic drop-floor must be positive");
}

#[test]
fn drop_floors_strictly_descending() {
    // Common → Chaotic must monotonically decrease (Q-06 8-tier).
    let common = rarity_drop_floor(Rarity::Common);
    let uncommon = rarity_drop_floor(Rarity::Uncommon);
    let rare = rarity_drop_floor(Rarity::Rare);
    let epic = rarity_drop_floor(Rarity::Epic);
    let legendary = rarity_drop_floor(Rarity::Legendary);
    let mythic = rarity_drop_floor(Rarity::Mythic);
    let prismatic = rarity_drop_floor(Rarity::Prismatic);
    let chaotic = rarity_drop_floor(Rarity::Chaotic);
    assert!(common > uncommon);
    assert!(uncommon > rare);
    assert!(rare > epic);
    assert!(epic > legendary);
    assert!(legendary > mythic);
    assert!(mythic > prismatic);    // Q-06 NEW
    assert!(prismatic > chaotic);   // Q-06 NEW
}

#[test]
fn drop_floors_sum_to_unity_within_tolerance() {
    // Q-06 8-tier sum check : 0.60+0.25+0.10+0.04+0.009+0.0009+0.00009+0.00001
    //                      = 0.99999... (within 1e-4 of 1.0).
    let total: f32 = Rarity::all().iter().copied().map(rarity_drop_floor).sum();
    let diff = (total - 1.0).abs();
    assert!(diff < 1e-4, "drop-floor sum {total} not near 1.0 (diff {diff})");
}

#[test]
fn rarity_all_has_eight_tiers() {
    // Q-06 canonical : 8 tiers (Apocky 2026-05-01).
    assert_eq!(Rarity::all().len(), 8, "Rarity::all() must yield 8 tiers post-Q-06");
}

#[test]
fn drop_only_tiers_match_q06_canonical() {
    // Q-06 : Mythic + Prismatic + Chaotic are drop-only-or-bond.
    assert!(Rarity::Mythic.is_drop_only());
    assert!(Rarity::Prismatic.is_drop_only());
    assert!(Rarity::Chaotic.is_drop_only());
    // Lower tiers transmutable.
    assert!(!Rarity::Common.is_drop_only());
    assert!(!Rarity::Legendary.is_drop_only());
}

#[test]
fn bond_eligibility_extended_to_q06_tiers() {
    // Q-06 : bond extends to Mythic + Prismatic + Chaotic.
    assert!(Rarity::Legendary.is_bond_eligible());
    assert!(Rarity::Mythic.is_bond_eligible());
    assert!(Rarity::Prismatic.is_bond_eligible());
    assert!(Rarity::Chaotic.is_bond_eligible());
    // Lower tiers : not bond-eligible.
    assert!(!Rarity::Common.is_bond_eligible());
    assert!(!Rarity::Epic.is_bond_eligible());
}
