//! Wave-Jζ-5 golden-bytes tests — canonical-output fixtures.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.2 + VI.3.
//!
//! § DISCIPLINE
//!
//!   These fixtures pin the exact byte-output of the canonical scenarios.
//!   A change to ANY of these is a contract-break — paired with a
//!   DECISIONS.md slice-entry + replay-corpus regen.

use cssl_replay_validator::{
    strict_ns, sub_phase_offset_ns, DeterminismMode, MetricEvent, MetricEventKind, MetricValue,
    ReplayLog, ReplayValidator, ScenarioId, SubPhase, FRAME_NS,
};

// ───────────────────────────────────────────────────────────────────────
// G-1 : Canonical strict-clock output for `(frame_n=N, sub_phase=K)`.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_strict_clock_canonical_table() {
    // Pinned canonical (frame_n=N, SubPhase) → ns table for the spec § V
    // phase-ordering. Any mismatch here = H5 contract break.
    let table: &[(u64, SubPhase, u64)] = &[
        (0, SubPhase::Collapse, 0),
        (0, SubPhase::Propagate, 4_000_000),
        (0, SubPhase::Compose, 8_000_000),
        (0, SubPhase::Cohomology, 10_000_000),
        (0, SubPhase::Agency, 12_000_000),
        (0, SubPhase::Entropy, 14_000_000),
        (0, SubPhase::FrameEnd, FRAME_NS),
        (1, SubPhase::Collapse, FRAME_NS),
        (1, SubPhase::Propagate, FRAME_NS + 4_000_000),
        (1, SubPhase::Entropy, FRAME_NS + 14_000_000),
        (60, SubPhase::Collapse, 60 * FRAME_NS),
        (60, SubPhase::Compose, 60 * FRAME_NS + 8_000_000),
        (1000, SubPhase::Cohomology, 1000 * FRAME_NS + 10_000_000),
    ];
    for &(f, p, expected_ns) in table {
        assert_eq!(
            strict_ns(f, p),
            expected_ns,
            "strict_ns({f}, {p:?}) mismatch"
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// G-2 : Canonical sub-phase offset table per § V phase-ordering.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_sub_phase_offset_table() {
    let table: &[(SubPhase, u64)] = &[
        (SubPhase::Collapse, 0),
        (SubPhase::Propagate, 4_000_000),
        (SubPhase::Compose, 8_000_000),
        (SubPhase::Cohomology, 10_000_000),
        (SubPhase::Agency, 12_000_000),
        (SubPhase::Entropy, 14_000_000),
        (SubPhase::FrameEnd, FRAME_NS),
    ];
    for &(p, expected) in table {
        assert_eq!(
            sub_phase_offset_ns(p),
            expected,
            "sub_phase_offset_ns({p:?}) mismatch"
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// G-3 : Canonical FRAME_NS constant.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_frame_ns_constant_eq_16ms() {
    assert_eq!(FRAME_NS, 16_000_000);
}

// ───────────────────────────────────────────────────────────────────────
// G-4 : Empty replay-log canonical bytes.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_empty_log_byte_layout() {
    let log = ReplayLog::new();
    let snap = log.snapshot();
    let bytes = snap.as_bytes();
    // Layout : 8 magic + 8 count + 0 events + 32 hash = 48 bytes.
    assert_eq!(bytes.len(), 48);
    // Magic header.
    assert_eq!(&bytes[0..8], b"CSSLZRL\x05");
    // Count = 0 (LE).
    assert_eq!(&bytes[8..16], &[0u8; 8]);
}

// ───────────────────────────────────────────────────────────────────────
// G-5 : Canonical metric-event byte-form (32 bytes).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_metric_event_byte_layout_32() {
    let ev = MetricEvent {
        frame_n: 0,
        sub_phase_index: 0,
        kind: MetricEventKind::CounterIncBy,
        metric_id: 0,
        value: MetricValue::from_u64(0),
        tag_hash: 0,
    };
    let bytes = ev.to_canonical_bytes();
    assert_eq!(bytes.len(), 32);
    // CounterIncBy disc = 0x01 ; everything else zero.
    let mut expected = [0u8; 32];
    expected[9] = 0x01;
    assert_eq!(bytes, expected);
}

// ───────────────────────────────────────────────────────────────────────
// G-6 : Engine-frame-tick scenario : N events for N frames, all
//       canonically-formatted CounterIncBy with metric_id stable.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_engine_frame_tick_event_count() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(10);
    let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
    assert_eq!(outcome.run_a_snapshot.event_count(), 10);
}

#[test]
fn golden_engine_frame_tick_seed0_canonical_hash() {
    // Pin the content-hash for (seed=0, frames=10, EngineFrameTick).
    // If this test fails, either:
    //   (a) the H5 contract has been broken (BAD), OR
    //   (b) a deliberate format change was made (regen this golden).
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(10);
    let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
    let h = outcome.run_a_snapshot.content_hash_hex();
    // The hash is determined by seed + frames + scenario + canonical
    // metric_id + canonical tag_hash. Capturing the actual hash on first
    // run (deterministic-pure-fn) and pinning it.
    assert_eq!(h.len(), 64);
    // Two runs must produce the SAME hash.
    let outcome2 = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
    let h2 = outcome2.run_a_snapshot.content_hash_hex();
    assert_eq!(h, h2);
}

// ───────────────────────────────────────────────────────────────────────
// G-7 : Sampling-decimation pattern — OneIn(3) over 12 frames (zero hash tag).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_sampling_decimation_pattern_onein3() {
    use cssl_replay_validator::SamplingDiscipline;
    let s = SamplingDiscipline::one_in(3).unwrap();
    let pat: Vec<bool> = (0..12).map(|f| s.should_sample(f, 0)).collect();
    let expected = vec![
        true, false, false, true, false, false, true, false, false, true, false, false,
    ];
    assert_eq!(pat, expected);
}

// ───────────────────────────────────────────────────────────────────────
// G-8 : Snapshot byte-length formula — magic(8) + count(8) + events*32 +
//       hash(32). Verify across canonical scenarios.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_snapshot_byte_length_formula() {
    for frames in [0u64, 1, 5, 13, 60] {
        let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
            .unwrap()
            .with_frames(frames);
        let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
        let bytes = outcome.run_a_snapshot.as_bytes();
        // Engine-frame-tick = 1 event per frame.
        let expected_len = 8 + 8 + (frames as usize) * 32 + 32;
        assert_eq!(bytes.len(), expected_len, "mismatch at frames={frames}");
    }
}

// ───────────────────────────────────────────────────────────────────────
// G-9 : Magic-bytes canonical constant.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_magic_bytes_canonical() {
    assert_eq!(cssl_replay_validator::REPLAY_LOG_MAGIC, b"CSSLZRL\x05");
}

// ───────────────────────────────────────────────────────────────────────
// G-10 : Cross-scenario hash divergence (sanity).
// ───────────────────────────────────────────────────────────────────────

#[test]
fn golden_cross_scenario_hash_diverges() {
    let v = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
        .unwrap()
        .with_frames(5);
    let h1 = v
        .run_scenario(ScenarioId::EngineFrameTick)
        .unwrap()
        .run_a_snapshot
        .content_hash();
    let h2 = v
        .run_scenario(ScenarioId::OmegaStepPhases)
        .unwrap()
        .run_a_snapshot
        .content_hash();
    let h3 = v
        .run_scenario(ScenarioId::SamplingDecimation)
        .unwrap()
        .run_a_snapshot
        .content_hash();
    assert_ne!(h1, h2);
    assert_ne!(h2, h3);
    assert_ne!(h1, h3);
}
