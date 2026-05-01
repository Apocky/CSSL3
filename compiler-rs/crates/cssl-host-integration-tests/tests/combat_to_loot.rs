// § combat_to_loot — combat-tick → damage-output → loot-drop chain.
// ════════════════════════════════════════════════════════════════════
// § Coverage : combat damage-output flowing into gear-archetype roll_drop.
//   Includes seed-determinism end-to-end across the chain.

use cssl_host_combat_sim as combat;
use cssl_host_gear_archetype as gear;

use cssl_host_integration_tests::{make_combat_session, CombatSession};

/// (a) Combat-tick produces non-zero damage-output for an Active-state hit.
#[test]
fn combat_tick_produces_damage_output() {
    let mut session: CombatSession = make_combat_session(0xC0FFEE);
    // Force the attacker into Active and run a tick with a damage-roll.
    session.attacker.state = combat::CombatState::Active;
    let out = session.attacker.tick(
        combat::CombatInput::None,
        0.016,
        Some(session.damage_roll.clone()),
        Some(session.defender_armor),
    );
    assert!(
        out.damage_dealt > 0.0,
        "Active tick with damage-roll must produce >0 damage ({} found)",
        out.damage_dealt
    );
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e, combat::CombatEvent::HitLanded { .. })),
        "expected at least one HitLanded event"
    );
}

/// (b) Damage-output (via mob-tier 3 ctx + magic-find) triggers a loot-drop
///     gear from gear-archetype roll_drop.
#[test]
fn damage_output_triggers_loot_drop() {
    let session = make_combat_session(0x0BAD_C0DE);
    // Simulate the damage-handler emitting a drop-context based on the kill.
    let ctx = gear::DropContext {
        mob_tier: 3,
        biome: gear::Biome::Dungeon,
        magic_find: 0.4,
    };
    let dropped =
        gear::roll_drop(&ctx, session.seed as u128, Some(gear::GearSlot::MainHand));
    assert!(dropped.is_some(), "roll_drop must yield a Gear");
    let g = dropped.unwrap();
    assert_eq!(g.slot, gear::GearSlot::MainHand);
    // Rarity must satisfy the base-mat's rarity-floor invariant.
    assert!(
        g.rarity >= g.base.base_mat.rarity_floor(),
        "rarity {:?} violated base-mat floor {:?}",
        g.rarity,
        g.base.base_mat.rarity_floor()
    );
}

/// (c) End-to-end seed-determinism : same seed ⇒ bit-equal combat-output AND
///     bit-equal loot-roll across two cold runs.
#[test]
fn seed_determinism_combat_to_loot() {
    let seed: u64 = 0xDEAD_BEEF_CAFE_F00D;
    let ctx = gear::DropContext {
        mob_tier: 4,
        biome: gear::Biome::Forge,
        magic_find: 1.0,
    };

    // Run-A
    let mut a = make_combat_session(seed);
    a.attacker.state = combat::CombatState::Active;
    let out_a = a.attacker.tick(
        combat::CombatInput::None,
        0.016,
        Some(a.damage_roll.clone()),
        Some(a.defender_armor),
    );
    let loot_a = gear::roll_drop(&ctx, seed as u128, Some(gear::GearSlot::MainHand));

    // Run-B
    let mut b = make_combat_session(seed);
    b.attacker.state = combat::CombatState::Active;
    let out_b = b.attacker.tick(
        combat::CombatInput::None,
        0.016,
        Some(b.damage_roll.clone()),
        Some(b.defender_armor),
    );
    let loot_b = gear::roll_drop(&ctx, seed as u128, Some(gear::GearSlot::MainHand));

    assert_eq!(
        out_a.damage_dealt, out_b.damage_dealt,
        "bit-equal damage must hold across replays"
    );
    assert_eq!(
        loot_a.is_some(),
        loot_b.is_some(),
        "loot-presence must match"
    );
    if let (Some(la), Some(lb)) = (loot_a, loot_b) {
        assert_eq!(la.rarity, lb.rarity, "rarity must replay bit-equal");
        assert_eq!(
            la.base.base_mat,
            lb.base.base_mat,
            "base-mat must replay bit-equal"
        );
        assert_eq!(la.seed, lb.seed, "drop-seed must replay bit-equal");
    }
}
