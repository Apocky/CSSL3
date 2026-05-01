// § full_loop — combat → loot → craft → equip → cast end-to-end.
// ════════════════════════════════════════════════════════════════════
// § Coverage : composite driver `run_full_loop` + replay-bit-equal
//   determinism across two cold runs with the same seed.

use cssl_host_integration_tests::run_full_loop;

/// (a) Full loop from combat through cast emits coherent results : damage
///     dealt > 0, equipped-ok, cast-ok, mana-pool debited, looted gear
///     display-name non-empty.
#[test]
fn full_loop_completes_combat_through_cast() {
    let outcome = run_full_loop(0x1234_5678_9ABC_DEF0_u128, 8);
    assert!(
        outcome.damage_dealt > 0.0,
        "expected non-zero damage ; got {}",
        outcome.damage_dealt
    );
    assert!(
        !outcome.recovered_mats.is_empty(),
        "deconstruct must yield ≥ 1 recovered mat"
    );
    assert!(outcome.equipped_ok, "crafted-T1-weapon must equip cleanly");
    assert!(outcome.cast_ok, "minimal-fire-ray must cast at full mana");
    assert!(
        outcome.mana_after >= 0.0,
        "mana never negative ; got {}",
        outcome.mana_after
    );
    assert!(
        !outcome.looted_display_name.is_empty(),
        "looted gear must have a non-empty display-name"
    );
}

/// (b) Replay-bit-equal across 2 runs with the same seed.
#[test]
fn full_loop_replay_bit_equal_same_seed() {
    let seed: u128 = 0xFACE_C0DE_DEAD_BEEF_u128;
    let intel: u8 = 5;

    let a = run_full_loop(seed, intel);
    let b = run_full_loop(seed, intel);

    assert_eq!(
        a.damage_dealt, b.damage_dealt,
        "damage replay-bit-equal across same-seed runs"
    );
    assert_eq!(a.looted_rarity, b.looted_rarity);
    assert_eq!(a.recovered_mats, b.recovered_mats);
    assert_eq!(a.equipped_ok, b.equipped_ok);
    assert_eq!(a.cast_ok, b.cast_ok);
    assert_eq!(a.mana_after, b.mana_after);
    assert_eq!(a.looted_display_name, b.looted_display_name);
}
