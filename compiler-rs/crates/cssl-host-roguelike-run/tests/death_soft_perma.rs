// § death_soft_perma ← soft-perma carryover invariants
// ════════════════════════════════════════════════════════════════════
// § I> 50%-Echoes-scaled-by-floor kept on soft-death
// § I> hard-mode zeros all carryover
// § I> ECHOES_CAP enforced (¬ pathological-grind)
// ════════════════════════════════════════════════════════════════════

use cssl_host_roguelike_run::death::apply_death_penalty;
use cssl_host_roguelike_run::run_state::{RunPhase, RunState};

#[test]
fn echoes_carryover_50pct_at_full_depth() {
    let mut s = RunState::genesis(0xCAFE, 1);
    s.echoes_in_run = 800;
    s.floor_count = 4;
    s.depth = 4; // full-depth multiplier = 1.0
    let out = apply_death_penalty(&s, false, false);
    assert_eq!(out.carryover.echoes_carried, 400); // 800/2 × 1.0
    assert!(matches!(out.state.phase, RunPhase::Death));
    assert_eq!(out.state.echoes_in_run, 0);
}

#[test]
fn echoes_carryover_scales_by_partial_depth() {
    let mut s = RunState::genesis(0xCAFE, 2);
    s.echoes_in_run = 1000;
    s.floor_count = 10;
    s.depth = 3; // 30% depth → carry ≈ 500 × 0.3 = 150 (256-fixedpoint)
    let out = apply_death_penalty(&s, false, false);
    // (500 × ((3*256)/10)) / 256 = (500 × 76) / 256 = 38000/256 = 148
    assert!(
        (140..=160).contains(&out.carryover.echoes_carried),
        "got {}",
        out.carryover.echoes_carried
    );
}

#[test]
fn hard_perma_kills_all_carryover() {
    let mut s = RunState::genesis(0x42, 99);
    s.echoes_in_run = 5000;
    s.floor_count = 8;
    s.depth = 8;
    let out = apply_death_penalty(&s, true, false);
    assert_eq!(out.carryover.echoes_carried, 0);
    assert!(out.hard_perma);
    assert!(!out.carryover.mercy_used);
}

#[test]
fn cap_prevents_pathological_grind() {
    let mut s = RunState::genesis(0xABCD, 1);
    // Massive in-run echoes — should cap at 100k carry-over.
    s.echoes_in_run = 10_000_000;
    s.floor_count = 5;
    s.depth = 5;
    let out = apply_death_penalty(&s, false, false);
    assert_eq!(out.carryover.echoes_carried, 100_000); // ECHOES_CAP
}

#[test]
fn mercy_recorded_on_soft_perma_when_available() {
    let mut s = RunState::genesis(1, 1);
    s.echoes_in_run = 100;
    s.floor_count = 3;
    s.depth = 2;
    let out = apply_death_penalty(&s, false, true);
    assert!(out.carryover.mercy_used);
    assert!(!out.hard_perma);
}
