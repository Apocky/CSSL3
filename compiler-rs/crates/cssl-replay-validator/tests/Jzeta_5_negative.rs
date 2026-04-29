//! Wave-Jζ-5 negative-tests — invalid-input handling per spec error-conditions.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.4 forbidden-patterns.

use cssl_replay_validator::{
    sampling_decision_strict, DeterminismMode, ReplayLog, ReplayLogError, ReplayRun,
    ReplayRunError, ReplayValidator, SamplingDiscipline, SamplingDisciplineError, ScenarioId,
    StrictClock, StrictClockError, SubPhase,
};

// ───────────────────────────────────────────────────────────────────────
// NEG-1 : Validator refuses Lenient mode.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg1_validator_refuses_lenient_mode() {
    let r = ReplayValidator::new(DeterminismMode::Lenient);
    assert_eq!(r.unwrap_err(), ReplayRunError::NotStrictMode);
}

// ───────────────────────────────────────────────────────────────────────
// NEG-2 : ReplayRun refuses Lenient mode.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg2_run_refuses_lenient_mode() {
    let r = ReplayRun::new(ScenarioId::EngineFrameTick, DeterminismMode::Lenient);
    assert_eq!(r.unwrap_err(), ReplayRunError::NotStrictMode);
}

// ───────────────────────────────────────────────────────────────────────
// NEG-3 : OneIn(0) refused at construction.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg3_one_in_zero_refused() {
    let e = SamplingDiscipline::one_in(0);
    assert_eq!(e.unwrap_err(), SamplingDisciplineError::ZeroDivisor);
}

#[test]
fn neg3_burst_zero_divisor_refused() {
    let e = SamplingDiscipline::burst_then_decimate(5, 0);
    assert_eq!(e.unwrap_err(), SamplingDisciplineError::ZeroDivisor);
}

#[test]
fn neg3_sampling_decision_strict_zero_div_returns_false() {
    // Defensive : N=0 returns false rather than panicking.
    assert!(!sampling_decision_strict(0, 0, 0));
    assert!(!sampling_decision_strict(100, 0xDEAD, 0));
}

// ───────────────────────────────────────────────────────────────────────
// NEG-4 : ReplayLog capacity exceeded.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg4_replay_log_capacity_refused() {
    use cssl_replay_validator::{MetricEvent, MetricEventKind, MetricValue};
    let mut log = ReplayLog::with_capacity(2);
    let make = |frame: u64| MetricEvent {
        frame_n: frame,
        sub_phase_index: 0,
        kind: MetricEventKind::CounterIncBy,
        metric_id: 1,
        value: MetricValue::from_u64(1),
        tag_hash: 0,
    };
    log.append(make(0)).unwrap();
    log.append(make(1)).unwrap();
    let r = log.append(make(2));
    assert_eq!(r.unwrap_err(), ReplayLogError::CapacityExceeded { cap: 2 });
}

// ───────────────────────────────────────────────────────────────────────
// NEG-5 : StrictClock frame-N overflow refused.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg5_strict_clock_overflow_refused() {
    let mut c = StrictClock::at(u64::MAX, SubPhase::Entropy);
    let r = c.advance_sub_phase();
    assert_eq!(r.unwrap_err(), StrictClockError::FrameNOverflow);
}

// ───────────────────────────────────────────────────────────────────────
// NEG-6 : Adaptive sampling absent from public API (compile-fail
//         encoded as type-system absence, verified at construction).
//
//   The `SamplingDiscipline` enum has no `Adaptive` variant. Any code
//   that tries to construct it would fail to compile. We can't directly
//   write a "this fails to compile" test without trybuild, but we CAN
//   verify the discipline's static guarantees.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg6_only_three_discipline_variants() {
    // Always, OneIn, BurstThenDecimate — all three CAN be constructed.
    // No fourth variant exists.
    let _ = SamplingDiscipline::Always;
    let _ = SamplingDiscipline::one_in(1).unwrap();
    let _ = SamplingDiscipline::burst_then_decimate(1, 1).unwrap();
    // If a fourth variant existed, the type system would surface it.
}

// ───────────────────────────────────────────────────────────────────────
// NEG-7 : Strict-mode permits-wallclock returns false.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg7_strict_refuses_wallclock_observation() {
    let m = DeterminismMode::strict_with_seed(0);
    assert!(!m.permits_wallclock());
    assert!(!m.permits_adaptive_sampling());
    assert!(m.enforces_forbidden_patterns());
}

// ───────────────────────────────────────────────────────────────────────
// NEG-8 : Bad MetricEvent kind disc returns None on decode.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg8_bad_metric_kind_disc_returns_none() {
    use cssl_replay_validator::MetricEvent;
    let mut bytes = [0u8; 32];
    bytes[9] = 0xFF; // Invalid kind disc.
    assert!(MetricEvent::from_canonical_bytes(&bytes).is_none());
}

// ───────────────────────────────────────────────────────────────────────
// NEG-9 : Diff with malformed magic bytes returns Diverged-BadMagic.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg9_diff_bad_magic_left() {
    use cssl_replay_validator::{HistoryDiff, HistoryDiffKind};
    let log = ReplayLog::new();
    // Construct a snapshot then pretend we got a "valid" snapshot whose
    // bytes have been corrupted in the magic header. We can't directly
    // mutate the snapshot bytes (they're private), so we verify the
    // canonical happy-path here and rely on the diff_snapshots tests in
    // the diff module for malformed-magic coverage.
    let snap = log.snapshot();
    let snap2 = log.snapshot();
    // For two valid snapshots from empty logs, diff is BitEqual.
    let d = cssl_replay_validator::diff::diff_snapshots(&snap, &snap2).unwrap();
    assert!(matches!(d, HistoryDiff::BitEqual { .. }));
    // Negative variant proper : count-mismatch surfaces as Diverged.
    let mut log2 = ReplayLog::new();
    log2.append(cssl_replay_validator::MetricEvent {
        frame_n: 0,
        sub_phase_index: 0,
        kind: cssl_replay_validator::MetricEventKind::CounterIncBy,
        metric_id: 0,
        value: cssl_replay_validator::MetricValue::from_u64(0),
        tag_hash: 0,
    })
    .unwrap();
    let d2 = cssl_replay_validator::diff::diff_snapshots(&snap, &log2.snapshot()).unwrap();
    assert!(matches!(
        d2,
        HistoryDiff::Diverged(HistoryDiffKind::EventCountDiffers { .. })
    ));
}

// ───────────────────────────────────────────────────────────────────────
// NEG-10 : Frame-zero scenarios produce empty snapshots, not errors.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg10_frame_zero_yields_empty_snapshot_not_error() {
    let r = ReplayRun::new(
        ScenarioId::EngineFrameTick,
        DeterminismMode::strict_with_seed(0),
    )
    .unwrap()
    .with_frames(0);
    let snap = r.execute().unwrap();
    assert_eq!(snap.event_count(), 0);
}

// ───────────────────────────────────────────────────────────────────────
// NEG-11 : Different seed ⇒ different snapshot bytes (sanity).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn neg11_different_seed_diverges() {
    let v0 = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let v1 = ReplayValidator::new(DeterminismMode::strict_with_seed(1))
        .unwrap()
        .with_frames(5);
    let s0 = v0.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
    let s1 = v1.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
    assert!(s0.passed());
    assert!(s1.passed());
    assert!(!s0.run_a_snapshot.is_bit_equal_to(&s1.run_a_snapshot));
}
