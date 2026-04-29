//! Wave-Jζ-5 property-tests — invariant-based via deterministic iteration-grid.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI + AC-5..AC-9.
//!
//! § DISCIPLINE
//!
//!   No `proptest` crate ; the tests iterate over deterministic small
//!   grids of inputs to verify the same invariants. The advantage : the
//!   tests themselves are bit-deterministic (every run identical) — which
//!   is exactly the property under test.

use cssl_replay_validator::{
    sampling_decision_strict, strict_ns, DeterminismMode, MetricEvent, MetricEventKind,
    MetricValue, ReplayLog, ReplayValidator, SamplingDiscipline, ScenarioId, SubPhase,
};

// ───────────────────────────────────────────────────────────────────────
// PROP-1 : Counter same-sequence → bit-equal snapshots.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_counter_same_sequence_bit_equal() {
    // Iterate over a deterministic value-grid.
    let value_grids: &[&[u64]] = &[
        &[],
        &[0],
        &[0xFF],
        &[1, 2, 3],
        &[0, u64::MAX, 1, 0xC0FFEE],
        &[0xDEAD, 0xBEEF, 0xCAFE, 0xF00D, 0xC0DE],
    ];
    for grid in value_grids {
        let mut la = ReplayLog::new();
        let mut lb = ReplayLog::new();
        for (i, v) in grid.iter().enumerate() {
            let ev = MetricEvent {
                frame_n: i as u64,
                sub_phase_index: 0,
                kind: MetricEventKind::CounterIncBy,
                metric_id: 1,
                value: MetricValue::from_u64(*v),
                tag_hash: 0,
            };
            la.append(ev).unwrap();
            lb.append(ev).unwrap();
        }
        assert!(la.snapshot().is_bit_equal_to(&lb.snapshot()));
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-2 : Gauge bit-pattern round-trip.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_gauge_set_bit_pattern_round_trip() {
    let values: &[f64] = &[
        0.0,
        -0.0,
        1.0,
        -1.0,
        f64::EPSILON,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        std::f64::consts::PI,
        std::f64::consts::E,
        1e-300,
        1e300,
    ];
    for &v in values {
        let mv = MetricValue::from_f64(v);
        assert_eq!(mv.as_bits(), v.to_bits());
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-3 : Canonical-bytes round-trip across input grid.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_canonical_bytes_round_trip() {
    let frame_grid: &[u64] = &[0, 1, 100, FRAME_SAFE_HIGH, u32::MAX as u64];
    let phase_grid: &[u8] = &[0, 1, 2, 3, 4, 5, 6];
    let metric_id_grid: &[u32] = &[0, 1, 0xDEAD, u32::MAX];
    let value_bits_grid: &[u64] = &[0, 0xFF, 0xDEAD_BEEF_CAFE_F00D, u64::MAX];
    let tag_hash_grid: &[u64] = &[0, 0xABCD, u64::MAX];

    for &frame_n in frame_grid {
        for &sub_phase_index in phase_grid {
            for &metric_id in metric_id_grid {
                for &value_bits in value_bits_grid {
                    for &tag_hash in tag_hash_grid {
                        let ev = MetricEvent {
                            frame_n,
                            sub_phase_index,
                            kind: MetricEventKind::CounterIncBy,
                            metric_id,
                            value: MetricValue::from_u64(value_bits),
                            tag_hash,
                        };
                        let bytes = ev.to_canonical_bytes();
                        let back = MetricEvent::from_canonical_bytes(&bytes).unwrap();
                        assert_eq!(ev, back);
                    }
                }
            }
        }
    }
}

const FRAME_SAFE_HIGH: u64 = 100_000;

// ───────────────────────────────────────────────────────────────────────
// PROP-4 : Strict-ns associativity with phase-offset.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_strict_ns_associative_w_phase_offset() {
    for frame_n in (0..1000u64).step_by(7) {
        let p0 = SubPhase::Collapse;
        let p1 = SubPhase::Compose;
        let ns0 = strict_ns(frame_n, p0);
        let ns1 = strict_ns(frame_n, p1);
        assert_eq!(ns1 - ns0, 8_000_000);
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-5 : Sampling deterministic (any frame_n + any tag_hash).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_sampling_deterministic() {
    let frame_grid: &[u64] = &[0, 1, 100, 1_000_000, u64::MAX / 2];
    let tag_grid: &[u64] = &[0, 0xAA, 0xC0FFEE, u64::MAX];
    let n_grid: &[u32] = &[1, 2, 3, 5, 17, 100];
    for &n in n_grid {
        let s = SamplingDiscipline::one_in(n).unwrap();
        for &f in frame_grid {
            for &h in tag_grid {
                let a = s.should_sample(f, h);
                let b = s.should_sample(f, h);
                assert_eq!(a, b);
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-6 : Sampling-strict matches OneIn(N) over grid.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_sampling_strict_matches_one_in() {
    for n in 1u32..15 {
        let s = SamplingDiscipline::one_in(n).unwrap();
        for f in 0u64..50 {
            for h in 0u64..15 {
                assert_eq!(s.should_sample(f, h), sampling_decision_strict(f, h, n));
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-7 : Sampling decimation density — over a window of N frames,
//          exactly one frame triggers (when tag_hash = 0).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_sampling_decimated_density_in_bounds() {
    for n in 2u32..20 {
        let s = SamplingDiscipline::one_in(n).unwrap();
        let count = (0..n)
            .map(|f| s.should_sample(u64::from(f), 0) as u32)
            .sum::<u32>();
        assert_eq!(count, 1, "OneIn({n}) yielded {count} hits in window");
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-8..12 : Replay-roundtrip across all five canonical scenarios.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_replay_roundtrip_engine_frame_tick() {
    for &seed in &[0u64, 1, 42, 0xC0FFEE, 0xDEAD, u64::MAX / 3] {
        for frames in (1u64..15).step_by(3) {
            let v = ReplayValidator::new(DeterminismMode::strict_with_seed(seed))
                .unwrap()
                .with_frames(frames);
            let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
            assert!(
                outcome.passed(),
                "seed={seed} frames={frames} did not pass"
            );
        }
    }
}

#[test]
fn prop_replay_roundtrip_omega_phases() {
    for &seed in &[0u64, 1, 42, 0xC0FFEE] {
        for frames in (1u64..8).step_by(2) {
            let v = ReplayValidator::new(DeterminismMode::strict_with_seed(seed))
                .unwrap()
                .with_frames(frames);
            let outcome = v.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
            assert!(outcome.passed());
        }
    }
}

#[test]
fn prop_replay_roundtrip_render_stages() {
    for &seed in &[0u64, 7, 0xDEAD] {
        for frames in (1u64..6).step_by(2) {
            let v = ReplayValidator::new(DeterminismMode::strict_with_seed(seed))
                .unwrap()
                .with_frames(frames);
            let outcome = v.run_scenario(ScenarioId::RenderStageDistribution).unwrap();
            assert!(outcome.passed());
        }
    }
}

#[test]
fn prop_replay_roundtrip_tier_counts() {
    for &seed in &[0u64, 0xC0FFEE, u64::MAX / 7] {
        for frames in (1u64..10).step_by(3) {
            let v = ReplayValidator::new(DeterminismMode::strict_with_seed(seed))
                .unwrap()
                .with_frames(frames);
            let outcome = v.run_scenario(ScenarioId::EntityTierCounts).unwrap();
            assert!(outcome.passed());
        }
    }
}

#[test]
fn prop_replay_roundtrip_sampling() {
    for &seed in &[0u64, 1, 42] {
        for frames in (1u64..30).step_by(7) {
            let v = ReplayValidator::new(DeterminismMode::strict_with_seed(seed))
                .unwrap()
                .with_frames(frames);
            let outcome = v.run_scenario(ScenarioId::SamplingDecimation).unwrap();
            assert!(outcome.passed());
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-13 : Event-count formula — engine-frame-tick yields N events for N frames.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_event_count_engine_frame_tick() {
    for frames in 1u64..20 {
        let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
            .unwrap()
            .with_frames(frames);
        let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
        assert_eq!(outcome.run_a_snapshot.event_count() as u64, frames);
    }
}

#[test]
fn prop_event_count_omega_six_per_frame() {
    for frames in 1u64..10 {
        let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
            .unwrap()
            .with_frames(frames);
        let outcome = v.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
        assert_eq!(outcome.run_a_snapshot.event_count() as u64, frames * 6);
    }
}

#[test]
fn prop_event_count_render_twelve_per_frame() {
    for frames in 1u64..6 {
        let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
            .unwrap()
            .with_frames(frames);
        let outcome = v.run_scenario(ScenarioId::RenderStageDistribution).unwrap();
        assert_eq!(outcome.run_a_snapshot.event_count() as u64, frames * 12);
    }
}

#[test]
fn prop_event_count_tier_four_per_frame() {
    for frames in 1u64..15 {
        let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
            .unwrap()
            .with_frames(frames);
        let outcome = v.run_scenario(ScenarioId::EntityTierCounts).unwrap();
        assert_eq!(outcome.run_a_snapshot.event_count() as u64, frames * 4);
    }
}

// ───────────────────────────────────────────────────────────────────────
// PROP-14 : Different seeds → different snapshot bytes (independence).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn prop_seed_diverges_snapshot_bytes() {
    let seed_pairs: &[(u64, u64)] = &[
        (0, 1),
        (0, 2),
        (1, 100),
        (0xC0FFEE, 0xDEAD),
        (u64::MAX, u64::MAX - 1),
    ];
    for &(a, b) in seed_pairs {
        let v_a = ReplayValidator::new(DeterminismMode::strict_with_seed(a))
            .unwrap()
            .with_frames(5);
        let v_b = ReplayValidator::new(DeterminismMode::strict_with_seed(b))
            .unwrap()
            .with_frames(5);
        let s_a = v_a.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
        let s_b = v_b.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
        assert!(s_a.passed());
        assert!(s_b.passed());
        assert!(!s_a.run_a_snapshot.is_bit_equal_to(&s_b.run_a_snapshot));
    }
}
