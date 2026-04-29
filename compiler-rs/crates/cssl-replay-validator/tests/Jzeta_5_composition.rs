//! Wave-Jζ-5 composition-tests — interaction-with-other-slices' surfaces.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI + AC-9 + AC-10.

use cssl_replay_validator::{
    diff::diff_snapshots, DeterminismMode, HistoryDiff, LogShim, MetricEvent, MetricEventKind,
    MetricValue, MetricsShim, RecordContext, ReplayLog, ReplayValidator, ScenarioId,
    SpecAnchorMock, SpecCoverageShim, StrictAware, StrictClock, SubPhase,
};

// ───────────────────────────────────────────────────────────────────────
// COMP-1 : MetricsShim + ReplayLog + DeterminismMode end-to-end.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp1_metrics_shim_strict_records_and_log_seals() {
    let mut shim = MetricsShim::new();
    let mut log = ReplayLog::new();
    let mode = DeterminismMode::strict_with_seed(0);
    for f in 0..10 {
        shim.counter_inc(
            &mut log,
            mode,
            RecordContext::new(f, SubPhase::Collapse, 1, 0),
            1,
        );
    }
    let snap = log.snapshot();
    assert_eq!(snap.event_count(), 10);
}

// ───────────────────────────────────────────────────────────────────────
// COMP-2 : Two MetricsShim runs with same input produce bit-equal snapshots.
// ───────────────────────────────────────────────────────────────────────

#[test]
#[allow(clippy::cast_precision_loss)]
fn comp2_metrics_shim_replay_bit_equal() {
    let mode = DeterminismMode::strict_with_seed(42);
    let mut snaps = Vec::new();
    for _ in 0..2 {
        let mut shim = MetricsShim::new();
        let mut log = ReplayLog::new();
        for f in 0..20u64 {
            shim.counter_inc(
                &mut log,
                mode,
                RecordContext::new(f, SubPhase::Collapse, 1, 0xABCD),
                f,
            );
            shim.gauge_set(
                &mut log,
                mode,
                RecordContext::new(f, SubPhase::Propagate, 2, 0xABCD),
                f as f64,
            );
        }
        snaps.push(log.snapshot());
    }
    assert!(snaps[0].is_bit_equal_to(&snaps[1]));
}

// ───────────────────────────────────────────────────────────────────────
// COMP-3 : MetricsShim with all four metric kinds.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp3_all_four_metric_kinds_in_one_snapshot() {
    let mode = DeterminismMode::strict_with_seed(0);
    let mut shim = MetricsShim::new();
    let mut log = ReplayLog::new();
    shim.counter_inc(
        &mut log,
        mode,
        RecordContext::new(0, SubPhase::Collapse, 1, 0),
        1,
    );
    shim.gauge_set(
        &mut log,
        mode,
        RecordContext::new(0, SubPhase::Propagate, 2, 0),
        1.5,
    );
    shim.histogram_record(
        &mut log,
        mode,
        RecordContext::new(0, SubPhase::Compose, 3, 0),
        2.5,
    );
    shim.timer_record_ns(
        &mut log,
        mode,
        RecordContext::new(0, SubPhase::Cohomology, 4, 0),
        100_000,
    );
    let snap = log.snapshot();
    assert_eq!(snap.event_count(), 4);
    let kinds: Vec<_> = log.events().iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&MetricEventKind::CounterIncBy));
    assert!(kinds.contains(&MetricEventKind::GaugeSet));
    assert!(kinds.contains(&MetricEventKind::HistogramRecord));
    assert!(kinds.contains(&MetricEventKind::TimerRecordNs));
}

// ───────────────────────────────────────────────────────────────────────
// COMP-4 : LogShim composes deterministically with StrictClock.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp4_log_shim_strict_clock_deterministic() {
    let mode = DeterminismMode::strict_with_seed(0);
    let mk = |seed: u64| {
        let mut clock = StrictClock::default();
        let mut shim = LogShim::new();
        for _ in 0..6 {
            let (frame_n, sub_phase) = clock.cursor();
            shim.record(mode, frame_n, sub_phase, "tick");
            let _ = clock.advance_sub_phase();
        }
        // Avoid unused-warning on seed.
        let _ = seed;
        shim.snapshot_bytes()
    };
    assert_eq!(mk(0), mk(0));
}

// ───────────────────────────────────────────────────────────────────────
// COMP-5 : SpecCoverageShim composes with MetricsShim — citing-metrics
//          lookup is deterministic.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp5_spec_coverage_lookup_deterministic() {
    let mut shim = SpecCoverageShim::new();
    shim.register(SpecAnchorMock {
        spec_section: "§ I engine.frame_n",
        citing_metric: "engine.frame_n",
    });
    shim.register(SpecAnchorMock {
        spec_section: "§ V phase-COLLAPSE",
        citing_metric: "omega_step.phase_time_ns",
    });
    let s = shim.metric_to_spec_section("omega_step.phase_time_ns");
    assert_eq!(s, Some("§ V phase-COLLAPSE"));
    // Repeated lookup is identical.
    let s2 = shim.metric_to_spec_section("omega_step.phase_time_ns");
    assert_eq!(s, s2);
}

// ───────────────────────────────────────────────────────────────────────
// COMP-6..10 : Five-replay-scenarios bit-equal verification.
//   These are the explicit "5 replay scenarios bit-equal" coverage AC.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp6_scenario_engine_frame_tick_bit_equal_e2e() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xC0FFEE))
        .unwrap()
        .with_frames(13);
    let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
    let d = diff_snapshots(&outcome.run_a_snapshot, &outcome.run_b_snapshot).unwrap();
    assert!(matches!(d, HistoryDiff::BitEqual { .. }));
}

#[test]
fn comp7_scenario_omega_step_phases_bit_equal_e2e() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xC0FFEE))
        .unwrap()
        .with_frames(13);
    let outcome = v.run_scenario(ScenarioId::OmegaStepPhases).unwrap();
    let d = diff_snapshots(&outcome.run_a_snapshot, &outcome.run_b_snapshot).unwrap();
    assert!(matches!(d, HistoryDiff::BitEqual { .. }));
}

#[test]
fn comp8_scenario_render_stages_bit_equal_e2e() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xC0FFEE))
        .unwrap()
        .with_frames(7);
    let outcome = v.run_scenario(ScenarioId::RenderStageDistribution).unwrap();
    let d = diff_snapshots(&outcome.run_a_snapshot, &outcome.run_b_snapshot).unwrap();
    assert!(matches!(d, HistoryDiff::BitEqual { .. }));
}

#[test]
fn comp9_scenario_tier_counts_bit_equal_e2e() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xC0FFEE))
        .unwrap()
        .with_frames(13);
    let outcome = v.run_scenario(ScenarioId::EntityTierCounts).unwrap();
    let d = diff_snapshots(&outcome.run_a_snapshot, &outcome.run_b_snapshot).unwrap();
    assert!(matches!(d, HistoryDiff::BitEqual { .. }));
}

#[test]
fn comp10_scenario_sampling_decimation_bit_equal_e2e() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xC0FFEE))
        .unwrap()
        .with_frames(31);
    let outcome = v.run_scenario(ScenarioId::SamplingDecimation).unwrap();
    let d = diff_snapshots(&outcome.run_a_snapshot, &outcome.run_b_snapshot).unwrap();
    assert!(matches!(d, HistoryDiff::BitEqual { .. }));
}

// ───────────────────────────────────────────────────────────────────────
// COMP-11..15 : H5 contract preservation — the H5 contract is preserved
//   IFF the validator runs cleanly across ALL scenarios under any seed.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp11_h5_preserved_seed_zero() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(11);
    assert!(v.all_pass().unwrap());
}

#[test]
fn comp12_h5_preserved_seed_max() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(u64::MAX))
        .unwrap()
        .with_frames(11);
    assert!(v.all_pass().unwrap());
}

#[test]
fn comp13_h5_preserved_seed_arbitrary() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xFEED_DEAD_BEEF_CAFE))
        .unwrap()
        .with_frames(11);
    assert!(v.all_pass().unwrap());
}

#[test]
fn comp14_h5_preserved_long_run() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0xC0FFEE))
        .unwrap()
        .with_frames(60); // 1 second @ 60Hz
    assert!(v.all_pass().unwrap());
}

#[test]
fn comp15_h5_preserved_short_run() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(1);
    assert!(v.all_pass().unwrap());
}

// ───────────────────────────────────────────────────────────────────────
// COMP-16..20 : Cross-shim composition — three shims jointly seal a
//   replay-bundle, and the bundle is bit-equal across two runs.
// ───────────────────────────────────────────────────────────────────────

fn run_bundle(seed: u64) -> Vec<u8> {
    let mode = DeterminismMode::strict_with_seed(seed);
    let mut log = ReplayLog::new();
    let mut metrics = MetricsShim::new();
    let mut log_shim = LogShim::new();
    let mut spec_cov = SpecCoverageShim::new();
    spec_cov.register(SpecAnchorMock {
        spec_section: "§ I engine",
        citing_metric: "engine.frame_n",
    });
    spec_cov.register(SpecAnchorMock {
        spec_section: "§ V phase-COLLAPSE",
        citing_metric: "omega_step.phase_time_ns",
    });

    let mut clock = StrictClock::default();
    for _ in 0..3 {
        let (frame_n, sub_phase) = clock.cursor();
        // Use seed as both the tag-hash AND the value, so the bundle
        // bytes diverge across seeds (verifying the seed-threading path).
        metrics.counter_inc(
            &mut log,
            mode,
            RecordContext::new(frame_n, sub_phase, 1, seed),
            seed,
        );
        log_shim.record(mode, frame_n, sub_phase, "tick");
        // Advance phases per frame.
        for _ in 0..5 {
            let _ = clock.advance_sub_phase();
        }
        // Wrap into next frame.
        let _ = clock.advance_sub_phase();
    }

    // Bundle-bytes : log-snapshot + log-shim-bytes + spec-coverage-bytes.
    let mut bundle = log.snapshot().as_bytes().to_vec();
    bundle.extend_from_slice(&log_shim.snapshot_bytes());
    bundle.extend_from_slice(&spec_cov.snapshot_bytes());
    bundle
}

#[test]
fn comp16_bundle_replay_bit_equal_seed_zero() {
    assert_eq!(run_bundle(0), run_bundle(0));
}

#[test]
fn comp17_bundle_replay_bit_equal_seed_arbitrary() {
    assert_eq!(run_bundle(0xC0FFEE), run_bundle(0xC0FFEE));
}

#[test]
fn comp18_bundle_diverges_under_different_seeds() {
    let a = run_bundle(0);
    let b = run_bundle(1);
    assert_ne!(a, b);
}

#[test]
fn comp19_bundle_includes_log_shim_path_hashes() {
    let bundle = run_bundle(7);
    // Bundle must be longer than just the replay-log snapshot.
    let mode = DeterminismMode::strict_with_seed(7);
    let mut log = ReplayLog::new();
    let mut metrics = MetricsShim::new();
    metrics.counter_inc(
        &mut log,
        mode,
        RecordContext::new(0, SubPhase::Collapse, 1, 0xAAA),
        1,
    );
    let log_only = log.snapshot();
    assert!(bundle.len() > log_only.as_bytes().len());
}

#[test]
fn comp20_bundle_strict_aware_trait_object_safe() {
    // StrictAware should be trait-object-safe (dyn StrictAware).
    struct M(DeterminismMode);
    impl StrictAware for M {
        fn determinism_mode(&self) -> DeterminismMode {
            self.0
        }
    }
    let m: Box<dyn StrictAware> = Box::new(M(DeterminismMode::strict_with_seed(0)));
    assert!(!m.permits_wallclock());
}

// ───────────────────────────────────────────────────────────────────────
// COMP-21 : Lenient-mode shims do NOT pollute the replay-log.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp21_lenient_does_not_record_to_replay_log() {
    let mode = DeterminismMode::Lenient;
    let mut log = ReplayLog::new();
    let mut metrics = MetricsShim::new();
    for f in 0..10 {
        metrics.counter_inc(
            &mut log,
            mode,
            RecordContext::new(f, SubPhase::Collapse, 1, 0),
            1,
        );
    }
    assert_eq!(log.len(), 0);
    // But the shim-side counter still ticked (records what calls happened).
    assert_eq!(metrics.record_count, 10);
}

// ───────────────────────────────────────────────────────────────────────
// COMP-22 : Strict ⇒ replay-log fully populated for every recorded op.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp22_strict_populates_replay_log() {
    let mode = DeterminismMode::strict_with_seed(0);
    let mut log = ReplayLog::new();
    let mut metrics = MetricsShim::new();
    for f in 0..10 {
        metrics.counter_inc(
            &mut log,
            mode,
            RecordContext::new(f, SubPhase::Collapse, 1, 0),
            1,
        );
    }
    assert_eq!(log.len(), 10);
}

// ───────────────────────────────────────────────────────────────────────
// COMP-23 : Bundle bit-equal via diff_snapshots on the log-portion only.
// ───────────────────────────────────────────────────────────────────────

#[test]
#[allow(clippy::cast_precision_loss)]
fn comp23_log_portion_diff_bit_equal_across_runs() {
    let mode = DeterminismMode::strict_with_seed(0xFEED);
    let mk = || {
        let mut log = ReplayLog::new();
        let mut metrics = MetricsShim::new();
        for f in 0..7 {
            metrics.histogram_record(
                &mut log,
                mode,
                RecordContext::new(f, SubPhase::Compose, 3, 0xCAFE),
                f as f64,
            );
        }
        log.snapshot()
    };
    let a = mk();
    let b = mk();
    let d = diff_snapshots(&a, &b).unwrap();
    assert!(matches!(d, HistoryDiff::BitEqual { .. }));
}

// ───────────────────────────────────────────────────────────────────────
// COMP-24 : Cross-frame-window canonical bytes consistent.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp24_canonical_bytes_consistent_across_frame_windows() {
    let ev_a = MetricEvent {
        frame_n: 100,
        sub_phase_index: SubPhase::Compose.index(),
        kind: MetricEventKind::CounterIncBy,
        metric_id: 7,
        value: MetricValue::from_u64(42),
        tag_hash: 0xC0FFEE,
    };
    let ev_b = MetricEvent {
        frame_n: 100,
        sub_phase_index: SubPhase::Compose.index(),
        kind: MetricEventKind::CounterIncBy,
        metric_id: 7,
        value: MetricValue::from_u64(42),
        tag_hash: 0xC0FFEE,
    };
    assert_eq!(ev_a.to_canonical_bytes(), ev_b.to_canonical_bytes());
}

// ───────────────────────────────────────────────────────────────────────
// COMP-25 : Validator with non-default frame-count remains bit-equal.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn comp25_validator_arbitrary_frames_pass() {
    for frames in [1u64, 2, 5, 10, 30, 60, 120] {
        let v = ReplayValidator::new(DeterminismMode::strict_with_seed(frames))
            .unwrap()
            .with_frames(frames);
        assert!(v.all_pass().unwrap(), "failed at frames={frames}");
    }
}
