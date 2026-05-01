//! § glyph_slot_distribution tests — per-rarity counts match GDD § GLYPH-SLOTS.

use cssl_host_gear_archetype::{
    glyph_slots_for_rarity, glyph_slots_lower_bound, roll_glyph_slots, Rarity,
};

#[test]
fn glyph_slot_bounds_per_rarity() {
    // Common 0 ; Uncommon 0..1 ; Rare 1 ; Epic 1..2 ; Legendary 2..3 ; Mythic 3.
    assert_eq!(glyph_slots_lower_bound(Rarity::Common), 0);
    assert_eq!(glyph_slots_for_rarity(Rarity::Common), 0);

    assert_eq!(glyph_slots_lower_bound(Rarity::Uncommon), 0);
    assert_eq!(glyph_slots_for_rarity(Rarity::Uncommon), 1);

    assert_eq!(glyph_slots_lower_bound(Rarity::Rare), 1);
    assert_eq!(glyph_slots_for_rarity(Rarity::Rare), 1);

    assert_eq!(glyph_slots_lower_bound(Rarity::Epic), 1);
    assert_eq!(glyph_slots_for_rarity(Rarity::Epic), 2);

    assert_eq!(glyph_slots_lower_bound(Rarity::Legendary), 2);
    assert_eq!(glyph_slots_for_rarity(Rarity::Legendary), 3);

    assert_eq!(glyph_slots_lower_bound(Rarity::Mythic), 3);
    assert_eq!(glyph_slots_for_rarity(Rarity::Mythic), 3);
}

#[test]
fn roll_glyph_slots_within_band() {
    for seed in 0u128..200 {
        for r in Rarity::all() {
            let n = roll_glyph_slots(seed, r);
            let lo = glyph_slots_lower_bound(r);
            let hi = glyph_slots_for_rarity(r);
            assert!(
                n >= lo && n <= hi,
                "rarity {r:?} seed {seed} produced {n} outside [{lo}, {hi}]"
            );
        }
    }
}

#[test]
fn roll_glyph_slots_replay_bit_equal() {
    // Same seed × rarity → same count.
    for seed in [0u128, 1, 42, 0xDEADBEEFu128, u128::MAX] {
        for r in Rarity::all() {
            let a = roll_glyph_slots(seed, r);
            let b = roll_glyph_slots(seed, r);
            assert_eq!(a, b, "non-deterministic for seed {seed} rarity {r:?}");
        }
    }
}
