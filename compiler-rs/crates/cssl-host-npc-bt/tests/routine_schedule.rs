// § tests/routine_schedule.rs — 12 archetypes + jitter determinism
// ════════════════════════════════════════════════════════════════════
// § I> 3 tests : all 12 archetypes generate · jitter-deterministic · sleep-anchor stable
// ════════════════════════════════════════════════════════════════════

use cssl_host_npc_bt::routines::{RoutineActivity, RoutineArchetype, daily_schedule};

#[test]
fn all_twelve_archetypes_generate_24h() {
    for arch in RoutineArchetype::all() {
        let s = daily_schedule(arch, 0xCAFE);
        assert_eq!(s.len(), 24);
        // hour-index monotonic 0..24
        for (i, b) in s.iter().enumerate() {
            assert_eq!(b.hour, i as u8);
        }
    }
}

#[test]
fn jitter_deterministic_per_seed() {
    let s1 = daily_schedule(RoutineArchetype::Worker, 0x42);
    let s2 = daily_schedule(RoutineArchetype::Worker, 0x42);
    assert_eq!(s1, s2);
    let s3 = daily_schedule(RoutineArchetype::Worker, 0x43);
    // Different seed → likely different schedule (jitter swaps differ).
    // We only assert determinism above ; this is informational.
    let same = s1 == s3;
    let _ = same; // not required
}

#[test]
fn sleep_anchor_zero_to_six_stable_per_jitter_design() {
    // Per impl, jitter swaps only when i >= 6, so hours 0..6 stay Sleep across seeds.
    for seed in [0_u64, 1, 0xDEAD, 0xBEEF].iter() {
        let s = daily_schedule(RoutineArchetype::Worker, *seed);
        for hb in &s[0..6] {
            assert_eq!(hb.activity, RoutineActivity::Sleep);
        }
    }
}
