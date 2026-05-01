// § stamina_economy.rs — 4 tests on regen / drain / clamp / starvation
// ════════════════════════════════════════════════════════════════════

#![allow(clippy::float_cmp)]

use cssl_host_combat_sim::stamina::{RegenMode, StaminaAction, StaminaPool};

#[test]
fn try_consume_succeeds_when_above_cost() {
    let mut p = StaminaPool::new(100.0);
    assert!(p.try_consume(StaminaAction::LightAttack)); // cost 18
    assert!((p.current - 82.0).abs() < 1e-3);
}

#[test]
fn post_action_delay_suppresses_regen() {
    let mut p = StaminaPool::new(100.0);
    assert!(p.try_consume(StaminaAction::LightAttack));
    let after_drain = p.current;
    p.set_regen_mode(RegenMode::Idle);
    p.tick(0.1); // 100ms < 350ms ; no regen
    assert!((p.current - after_drain).abs() < 1e-3);
}

#[test]
fn capacity_clamps_at_max() {
    let p = StaminaPool::new(500.0); // GDD cap 200
    assert!((p.capacity - 200.0).abs() < 1e-3);
}

#[test]
fn forced_idle_when_starved() {
    let mut p = StaminaPool::new(50.0);
    let _ = p.drain_raw(100.0); // underflow ⇒ clamped 0
    assert!(p.is_starved());
    assert_eq!(p.current, 0.0);
}
