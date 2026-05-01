// § determinism_replay.rs — 4 tests on seed+input → output bit-equal cross-call
// ════════════════════════════════════════════════════════════════════
// § per GDD axiom : combat-tick = pure-deterministic ; replay-bit-equal
// ════════════════════════════════════════════════════════════════════

#![allow(clippy::precedence, clippy::float_cmp, clippy::cast_possible_truncation)]

use cssl_host_combat_sim::damage_types::{ArmorClass, DamageRoll, DamageType};
use cssl_host_combat_sim::seed::DeterministicRng;
use cssl_host_combat_sim::state_machine::CombatInput;
use cssl_host_combat_sim::tick::CombatTick;
use cssl_host_combat_sim::weapons::WeaponArchetype;

fn run_sequence(seed: u64) -> Vec<u32> {
    let mut t = CombatTick::new(WeaponArchetype::Sword, ArmorClass::FleshBeast, seed);
    let inputs = [
        CombatInput::AttackPress,
        CombatInput::TickElapsed,
        CombatInput::TickElapsed,
        CombatInput::AttackPress,
        CombatInput::TickElapsed,
    ];
    let mut signature: Vec<u32> = Vec::new();
    for inp in inputs {
        let out = t.tick(inp, 0.016, None, None);
        // Encode (state, event-count, dmg) into a stable u32 signature
        let sig = (t.state as u32) ^ ((out.events.len() as u32) << 8) ^ (out.damage_dealt as u32);
        signature.push(sig);
        signature.push(t.rng.state() as u32 ^ ((t.rng.state() >> 32) as u32));
    }
    signature
}

#[test]
fn rng_same_seed_bit_equal() {
    let mut a = DeterministicRng::new(0xCAFE_BABE_DEAD_BEEF);
    let mut b = DeterministicRng::new(0xCAFE_BABE_DEAD_BEEF);
    let av: Vec<u64> = (0..256).map(|_| a.next_u64()).collect();
    let bv: Vec<u64> = (0..256).map(|_| b.next_u64()).collect();
    assert_eq!(av, bv);
}

#[test]
fn rng_different_seeds_diverge() {
    let mut a = DeterministicRng::new(1);
    let mut b = DeterministicRng::new(2);
    let mut diverged = false;
    for _ in 0..64 {
        if a.next_u64() != b.next_u64() {
            diverged = true;
            break;
        }
    }
    assert!(diverged, "different seeds must produce different streams");
}

#[test]
fn tick_replay_bit_equal_two_calls() {
    let s1 = run_sequence(42);
    let s2 = run_sequence(42);
    assert_eq!(s1, s2, "replay bit-equal axiom violated");
}

#[test]
fn damage_roll_deterministic_no_rng() {
    let roll = DamageRoll::single(DamageType::Slash, 30.0);
    let a = roll.apply(ArmorClass::FleshBeast);
    let b = roll.apply(ArmorClass::FleshBeast);
    assert_eq!(a.to_bits(), b.to_bits());
}
