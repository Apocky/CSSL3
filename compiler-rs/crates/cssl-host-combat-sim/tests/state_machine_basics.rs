// § state_machine_basics.rs — 4 tests on transition table-correctness
// ════════════════════════════════════════════════════════════════════

use cssl_host_combat_sim::{CombatInput, CombatState, CombatTransition};

#[test]
fn idle_block_hold_goes_to_block() {
    let next = CombatTransition::step(CombatState::Idle, CombatInput::BlockHold, 0.016);
    assert_eq!(next, CombatState::Block);
}

#[test]
fn block_release_returns_to_idle() {
    let next = CombatTransition::step(CombatState::Block, CombatInput::BlockRelease, 0.016);
    assert_eq!(next, CombatState::Idle);
}

#[test]
fn parry_window_incoming_hit_triggers_counter() {
    let next = CombatTransition::step(
        CombatState::ParryWindow,
        CombatInput::IncomingHit,
        0.016,
    );
    assert_eq!(next, CombatState::Counter);
}

#[test]
fn iframes_invulnerable_predicate() {
    assert!(CombatState::IFrames.is_invulnerable());
    assert!(!CombatState::Idle.is_invulnerable());
}
