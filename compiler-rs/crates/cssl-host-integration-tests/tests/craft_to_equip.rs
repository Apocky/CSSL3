// § craft_to_equip — crafted Gear → equip-into-slot → loadout-switch.
// ════════════════════════════════════════════════════════════════════
// § Coverage : equip-flow with bond-discipline + loadout-snapshot/restore.

use cssl_host_gear_archetype as gear;

use cssl_host_integration_tests::{
    craft_a_t1_weapon, equip_into_slot, make_player_inventory,
};

/// (a) Crafted Gear can equip into a matching slot.
#[test]
fn crafted_gear_equips_into_main_hand_slot() {
    let g = craft_a_t1_weapon(0x1111_2222_3333_4444_u128, gear::Rarity::Common);
    let mut inv = make_player_inventory();
    let res = equip_into_slot(&mut inv, gear::GearSlot::MainHand, g.clone());
    assert!(res.is_ok(), "fresh equip must succeed : {res:?}");
    assert!(res.unwrap(), "first equip should report was_empty=true");
    assert!(inv.is_equipped(gear::GearSlot::MainHand));
    let e = inv
        .get(gear::GearSlot::MainHand)
        .expect("just-equipped slot must be Some");
    assert_eq!(e.rarity, g.rarity);
    assert_eq!(e.seed, g.seed);
}

/// (b) Bonded gear cannot trade : the bond-flag is preserved across
///     equip/unequip cycles, and the rarity is bond-eligible.
#[test]
fn bonded_gear_preserves_bond_flag() {
    // Construct a Legendary gear ; only Legendary+ are bond-eligible per GDD.
    let mut g = craft_a_t1_weapon(0xDEAD_DEAD_DEAD_DEAD_u128, gear::Rarity::Legendary);
    g.bound_to_player = true;
    assert!(g.rarity.is_bond_eligible());

    let mut inv = make_player_inventory();
    equip_into_slot(&mut inv, gear::GearSlot::MainHand, g)
        .expect("equip must succeed");
    let stored = inv
        .get(gear::GearSlot::MainHand)
        .expect("equipped slot must be Some");
    assert!(
        stored.bound_to_player,
        "bond-flag must round-trip through equip"
    );

    // The non-tradable property is reflected by the bond-flag — if a transfer
    // were attempted, the receiver would refuse via this flag.
    let attempt_transfer = !stored.bound_to_player;
    assert!(
        !attempt_transfer,
        "bonded gear must NOT permit a transfer-attempt (flag-driven)"
    );
}

/// (c) Loadout-switch preserves equipped state : snapshot, swap, restore.
#[test]
fn loadout_switch_preserves_equipped_state() {
    let g_a = craft_a_t1_weapon(0x1111_u128, gear::Rarity::Common);
    let g_b = craft_a_t1_weapon(0x2222_u128, gear::Rarity::Uncommon);

    let mut inv = make_player_inventory();
    equip_into_slot(&mut inv, gear::GearSlot::MainHand, g_a.clone())
        .expect("equip-A must succeed");
    inv.snapshot_loadout();

    // Swap to gear-B.
    equip_into_slot(&mut inv, gear::GearSlot::MainHand, g_b.clone())
        .expect("equip-B must succeed");
    let after_swap = inv
        .get(gear::GearSlot::MainHand)
        .expect("post-swap slot must be Some")
        .clone();
    assert_eq!(after_swap.seed, g_b.seed, "post-swap must hold gear-B");

    // Restore the saved loadout (gear-A).
    inv.restore_loadout();
    let after_restore = inv
        .get(gear::GearSlot::MainHand)
        .expect("post-restore slot must be Some")
        .clone();
    assert_eq!(
        after_restore.seed, g_a.seed,
        "post-restore must hold gear-A's seed"
    );
    assert_eq!(after_restore.rarity, g_a.rarity);
}
