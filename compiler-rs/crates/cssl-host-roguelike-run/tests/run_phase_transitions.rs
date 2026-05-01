// § run_phase_transitions ← state-machine integration tests
// ════════════════════════════════════════════════════════════════════
// § I> Hub → BiomeSelect → Floor → BossArena → Reward path
// § I> invalid-transitions caught
// ════════════════════════════════════════════════════════════════════

use cssl_host_roguelike_run::run_state::{RunPhase, RunState, RunStateErr};
use cssl_host_roguelike_run::Biome;

#[test]
fn full_happy_path_hub_to_reward() {
    let mut s = RunState::genesis(0xCAFE_F00D, 1);
    assert!(matches!(s.phase, RunPhase::Hub));

    s.enter_biome_select().unwrap();
    assert!(matches!(s.phase, RunPhase::BiomeSelect));

    s.descend_into(Biome::Crypt, 3).unwrap();
    assert!(matches!(s.phase, RunPhase::Floor { idx: 1, biome: Biome::Crypt }));
    assert_eq!(s.depth, 1);

    s.advance_floor().unwrap(); // 1 → 2
    assert!(matches!(s.phase, RunPhase::Floor { idx: 2, .. }));

    s.advance_floor().unwrap(); // 2 → 3 (==floor_count) → BossArena
    assert!(matches!(s.phase, RunPhase::BossArena { biome: Biome::Crypt }));

    s.boss_cleared().unwrap();
    assert!(matches!(s.phase, RunPhase::Reward { biome: Biome::Crypt }));
}

#[test]
fn cannot_advance_floor_from_hub() {
    let mut s = RunState::genesis(1, 1);
    let err = s.advance_floor().unwrap_err();
    assert!(matches!(err, RunStateErr::InvalidTransition { .. }));
}

#[test]
fn cannot_descend_from_hub_directly() {
    let mut s = RunState::genesis(1, 1);
    let err = s.descend_into(Biome::Crypt, 5).unwrap_err();
    assert!(matches!(err, RunStateErr::InvalidTransition { .. }));
}

#[test]
fn floor_count_clamped_to_3_to_12() {
    let mut s = RunState::genesis(1, 1);
    s.enter_biome_select().unwrap();
    s.descend_into(Biome::EndlessSpire, 99).unwrap();
    assert_eq!(s.floor_count, 12);

    let mut s2 = RunState::genesis(2, 2);
    s2.enter_biome_select().unwrap();
    s2.descend_into(Biome::Crypt, 1).unwrap();
    assert_eq!(s2.floor_count, 3);
}

#[test]
fn reward_can_loop_back_to_biome_select() {
    let mut s = RunState::genesis(7, 7);
    s.enter_biome_select().unwrap();
    s.descend_into(Biome::Crypt, 3).unwrap();
    s.advance_floor().unwrap();
    s.advance_floor().unwrap();
    s.boss_cleared().unwrap();
    assert!(matches!(s.phase, RunPhase::Reward { .. }));

    // Reward → BiomeSelect (DAG-junction next-pick).
    s.enter_biome_select().unwrap();
    assert!(matches!(s.phase, RunPhase::BiomeSelect));
}
