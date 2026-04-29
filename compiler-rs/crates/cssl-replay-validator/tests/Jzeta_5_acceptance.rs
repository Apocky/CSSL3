//! Wave-Jζ-5 acceptance-tests — maps directly to spec § VI.1-VI.4 + AC-1..AC-12.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.

use cssl_replay_validator::{
    sampling_decision_strict, strict_ns, sub_phase_offset_ns, DeterminismMode,
    DeterminismModeKind, MetricEvent, MetricEventKind, MetricValue, ReplayLog, ReplayRun,
    ReplayValidator, SamplingDiscipline, ScenarioId, StrictClock, SubPhase, FRAME_NS,
};

// ───────────────────────────────────────────────────────────────────────
// AC-1 : Determinism-strict mode records to replay-log instead of perturbing.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac1_strict_engages_replay_log() {
    assert!(DeterminismMode::strict_with_seed(0).engages_replay_log());
}

#[test]
fn ac1_lenient_does_not_engage_replay_log() {
    assert!(!DeterminismMode::Lenient.engages_replay_log());
}

#[test]
fn ac1_strict_kind_disc() {
    assert_eq!(
        DeterminismMode::strict_with_seed(123).kind(),
        DeterminismModeKind::Strict
    );
}

#[test]
fn ac1_lenient_kind_disc() {
    assert_eq!(DeterminismMode::Lenient.kind(), DeterminismModeKind::Lenient);
}

#[test]
fn ac1_seed_carried_in_strict() {
    assert_eq!(
        DeterminismMode::strict_with_seed(0xCAFE).seed(),
        Some(0xCAFE)
    );
}

// ───────────────────────────────────────────────────────────────────────
// AC-2 : Strict-clock primitives — monotonic_ns ↦ (frame_n × FRAME_NS) + offset.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac2_strict_ns_frame0_collapse_zero() {
    assert_eq!(strict_ns(0, SubPhase::Collapse), 0);
}

#[test]
fn ac2_strict_ns_frame1_collapse_eq_frame_ns() {
    assert_eq!(strict_ns(1, SubPhase::Collapse), FRAME_NS);
}

#[test]
fn ac2_strict_ns_frame100_propagate_eq_100_frame_ns_plus_4ms() {
    assert_eq!(
        strict_ns(100, SubPhase::Propagate),
        100 * FRAME_NS + 4_000_000
    );
}

#[test]
fn ac2_strict_ns_no_wallclock_leak_repeatable() {
    // Read multiple times — must be exactly the same.
    for _ in 0..100 {
        assert_eq!(strict_ns(42, SubPhase::Compose), 42 * FRAME_NS + 8_000_000);
    }
}

#[test]
fn ac2_strict_ns_saturates_at_u64_max() {
    assert_eq!(strict_ns(u64::MAX, SubPhase::Entropy), u64::MAX);
}

// ───────────────────────────────────────────────────────────────────────
// AC-3 : Sub-phase ns-offset deterministic per § V phase-ordering.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac3_phase_ordering_collapse_zero() {
    assert_eq!(sub_phase_offset_ns(SubPhase::Collapse), 0);
}

#[test]
fn ac3_phase_ordering_propagate_4ms() {
    assert_eq!(sub_phase_offset_ns(SubPhase::Propagate), 4_000_000);
}

#[test]
fn ac3_phase_ordering_compose_8ms() {
    assert_eq!(sub_phase_offset_ns(SubPhase::Compose), 8_000_000);
}

#[test]
fn ac3_phase_ordering_cohomology_10ms() {
    assert_eq!(sub_phase_offset_ns(SubPhase::Cohomology), 10_000_000);
}

#[test]
fn ac3_phase_ordering_agency_12ms() {
    assert_eq!(sub_phase_offset_ns(SubPhase::Agency), 12_000_000);
}

#[test]
fn ac3_phase_ordering_entropy_14ms() {
    assert_eq!(sub_phase_offset_ns(SubPhase::Entropy), 14_000_000);
}

#[test]
fn ac3_phase_ordering_frame_end_eq_frame_ns() {
    assert_eq!(sub_phase_offset_ns(SubPhase::FrameEnd), FRAME_NS);
}

// ───────────────────────────────────────────────────────────────────────
// AC-4 : Histogram boundaries are `&'static [f64]` (compile-time).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac4_histogram_record_event_canonical_bytes_stable() {
    // The "static boundary" property is enforced in cssl-metrics (D157).
    // What we verify here : a HistogramRecord event has stable canonical bytes.
    let ev = MetricEvent {
        frame_n: 1,
        sub_phase_index: SubPhase::Compose.index(),
        kind: MetricEventKind::HistogramRecord,
        metric_id: 7,
        value: MetricValue::from_f64(1.5),
        tag_hash: 0,
    };
    let a = ev.to_canonical_bytes();
    let b = ev.to_canonical_bytes();
    assert_eq!(a, b);
}

// ───────────────────────────────────────────────────────────────────────
// AC-5 : Counter monotonic-ops are commutative-saturating-monoid ⇒ deterministic.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac5_counter_inc_by_canonical_bytes_stable() {
    let ev = MetricEvent {
        frame_n: 0,
        sub_phase_index: 0,
        kind: MetricEventKind::CounterIncBy,
        metric_id: 1,
        value: MetricValue::from_u64(7),
        tag_hash: 0,
    };
    let a = ev.to_canonical_bytes();
    let b = ev.to_canonical_bytes();
    assert_eq!(a, b);
}

#[test]
fn ac5_two_logs_with_same_inc_sequence_bit_equal() {
    let mut la = ReplayLog::new();
    let mut lb = ReplayLog::new();
    for i in 0..100u64 {
        let ev = MetricEvent {
            frame_n: i,
            sub_phase_index: 0,
            kind: MetricEventKind::CounterIncBy,
            metric_id: 1,
            value: MetricValue::from_u64(i),
            tag_hash: 0,
        };
        la.append(ev).unwrap();
        lb.append(ev).unwrap();
    }
    let snap_a = la.snapshot();
    let snap_b = lb.snapshot();
    assert!(snap_a.is_bit_equal_to(&snap_b));
}

// ───────────────────────────────────────────────────────────────────────
// AC-6 : Multi-thread aggregation — for THIS shim, single-thread,
//        deterministic-merge-order per appended events.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac6_merge_order_preserved() {
    let mut log = ReplayLog::new();
    log.append(MetricEvent {
        frame_n: 0,
        sub_phase_index: 0,
        kind: MetricEventKind::CounterIncBy,
        metric_id: 1,
        value: MetricValue::from_u64(0xA),
        tag_hash: 0,
    })
    .unwrap();
    log.append(MetricEvent {
        frame_n: 0,
        sub_phase_index: 0,
        kind: MetricEventKind::CounterIncBy,
        metric_id: 1,
        value: MetricValue::from_u64(0xB),
        tag_hash: 0,
    })
    .unwrap();
    let evs = log.events();
    assert_eq!(evs[0].value.as_u64(), 0xA);
    assert_eq!(evs[1].value.as_u64(), 0xB);
}

// ───────────────────────────────────────────────────────────────────────
// AC-7 : Sampling — OneIn(N) keyed-on frame_n (deterministic).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac7_sampling_deterministic_repeat() {
    let s = SamplingDiscipline::one_in(5).unwrap();
    let a: Vec<_> = (0..50).map(|f| s.should_sample(f, 0)).collect();
    let b: Vec<_> = (0..50).map(|f| s.should_sample(f, 0)).collect();
    assert_eq!(a, b);
}

#[test]
#[allow(non_snake_case)]
fn ac7_sampling_decision_strict_matches_oneIn() {
    let s = SamplingDiscipline::one_in(7).unwrap();
    for f in 0..100u64 {
        for h in 0..5u64 {
            assert_eq!(s.should_sample(f, h), sampling_decision_strict(f, h, 7));
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// AC-8 : Forbidden patterns refused (Adaptive / monotonic_ns / etc).
//
//   "Adaptive sampling" is enforced by ABSENCE from `SamplingDiscipline`.
//   "monotonic_ns direct-call" is enforced by ABSENCE from public API.
//
//   These are compile-time properties — the tests here verify the public
//   surface does NOT expose those constructions.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac8_one_in_zero_refused() {
    assert!(SamplingDiscipline::one_in(0).is_err());
}

#[test]
fn ac8_burst_then_decimate_zero_refused() {
    assert!(SamplingDiscipline::burst_then_decimate(3, 0).is_err());
}

#[test]
fn ac8_strict_mode_refuses_wallclock() {
    let m = DeterminismMode::strict_with_seed(0);
    assert!(!m.permits_wallclock());
    assert!(!m.permits_adaptive_sampling());
    assert!(m.enforces_forbidden_patterns());
}

// ───────────────────────────────────────────────────────────────────────
// AC-9 : Validator runs two replay-runs ⇒ bit-equal metric-histories.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac9_validator_engine_frame_tick_bit_equal() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
    assert!(outcome.passed());
    assert!(outcome
        .run_a_snapshot
        .is_bit_equal_to(&outcome.run_b_snapshot));
}

#[test]
fn ac9_validator_omega_phases_bit_equal() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let outcome = v.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
    assert!(outcome.passed());
}

#[test]
fn ac9_validator_render_stages_bit_equal() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let outcome = v.run_scenario(ScenarioId::RenderStageDistribution).unwrap();
    assert!(outcome.passed());
}

#[test]
fn ac9_validator_tier_counts_bit_equal() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let outcome = v.run_scenario(ScenarioId::EntityTierCounts).unwrap();
    assert!(outcome.passed());
}

#[test]
fn ac9_validator_sampling_bit_equal() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let outcome = v.run_scenario(ScenarioId::SamplingDecimation).unwrap();
    assert!(outcome.passed());
}

#[test]
fn ac9_validator_all_5_scenarios_bit_equal() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(7);
    let outcomes = v.run_all_scenarios().unwrap();
    assert_eq!(outcomes.len(), 5);
    for o in &outcomes {
        assert!(o.passed(), "scenario {:?} not bit-equal", o.scenario);
    }
}

// ───────────────────────────────────────────────────────────────────────
// AC-10 : H5 contract preserved — existing-omega_step bit-determinism.
//
//   We don't have access to the loa-game H5 harness in this crate, so
//   the contract is preserved INDIRECTLY : we ensure the validator
//   itself is deterministic when given the same seed. If a future
//   integration breaks the H5 contract, that breakage will surface in
//   the loa-game H5 harness — this acceptance test is the "metrics-side
//   guarantee" piece.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac10_validator_deterministic_across_invocations() {
    let v_a = ReplayValidator::new(DeterminismMode::strict_with_seed(0xDEAD))
        .unwrap()
        .with_frames(10);
    let v_b = ReplayValidator::new(DeterminismMode::strict_with_seed(0xDEAD))
        .unwrap()
        .with_frames(10);
    let s_a = v_a.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
    let s_b = v_b.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
    assert!(s_a.passed());
    assert!(s_b.passed());
    assert!(s_a
        .run_a_snapshot
        .is_bit_equal_to(&s_b.run_a_snapshot));
}

// ───────────────────────────────────────────────────────────────────────
// AC-11 : omega_step.replay_determinism_check{kind=fail} = 0.
//
//   Encoded here as : when the validator runs all scenarios, ZERO of
//   them report divergence.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac11_zero_failures_when_strict() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(8);
    let outcomes = v.run_all_scenarios().unwrap();
    let fail_count = outcomes.iter().filter(|o| !o.passed()).count();
    assert_eq!(fail_count, 0);
}

// ───────────────────────────────────────────────────────────────────────
// AC-12 : Metric-history bit-equal across replay runs.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn ac12_metric_history_bit_equal_explicit() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xFEED))
        .unwrap()
        .with_frames(5);
    let outcome = v.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
    assert_eq!(
        outcome.run_a_snapshot.as_bytes(),
        outcome.run_b_snapshot.as_bytes()
    );
}

#[test]
fn ac12_content_hash_equal_when_bit_equal() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
    assert_eq!(
        outcome.run_a_snapshot.content_hash(),
        outcome.run_b_snapshot.content_hash()
    );
}

// ───────────────────────────────────────────────────────────────────────
// Strict-clock cursor advancement
// ───────────────────────────────────────────────────────────────────────

#[test]
fn strict_clock_six_phases_then_wraps_frame() {
    let mut c = StrictClock::at_frame(0);
    for _ in 0..6 {
        c.advance_sub_phase().unwrap();
    }
    assert_eq!(c.cursor(), (1, SubPhase::Collapse));
}

#[test]
fn strict_clock_jump_to_resets_cursor() {
    let mut c = StrictClock::default();
    c.jump_to(99, SubPhase::Agency);
    assert_eq!(c.cursor(), (99, SubPhase::Agency));
    assert_eq!(c.now_ns(), 99 * FRAME_NS + 12_000_000);
}

#[test]
fn run_engine_frame_tick_yields_frame_count_events() {
    let r = ReplayRun::new(ScenarioId::EngineFrameTick, DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(13);
    let snap = r.execute().unwrap();
    assert_eq!(snap.event_count(), 13);
}
