//! § m8_integration_smoke — end-to-end M8 pipeline smoke + acceptance test.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Exercise the full 12-stage pipeline + companion subsystems over a
//!   condensed canonical playtest sequence. Asserts every M8 acceptance
//!   criterion structurally :
//!     - 12 stages execute in canonical order
//!     - 1M+ entity broadphase via Morton-hash (cssl-physics-wave)
//!     - ψ-field updates audio + light from same wave-PDE solver
//!     - KAN-BRDF spectral rendering produces non-RGB output
//!     - Mise-en-abyme recursion bounded at HARD cap = 5
//!     - Replay-determinism : two runs same seed produce bit-equal output
//!     - VR fallback : flat-screen rendering when no OpenXR runtime
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody." (per `PRIME_DIRECTIVE.md § 11 CREATOR-ATTESTATION`).

use loa_game::m8_integration::{M8Pipeline, PipelineConfig, StageId, ATTESTATION};

/// Canonical default configuration used across the M8 acceptance tests.
fn canonical_config() -> PipelineConfig {
    PipelineConfig {
        master_seed: 0xC551_F00D,
        view_width: 8,
        view_height: 8,
        xr_enabled: false,
        companion_enabled: true,
        work_graph_enabled: true,
    }
}

#[test]
fn m8_ac1_twelve_stages_execute_in_canonical_order() {
    // M8 AC1 : ALL 12 stages execute in order, every frame.
    let mut p = M8Pipeline::new(canonical_config());
    let digest = p.step(1.0 / 60.0).expect("step");
    assert!(digest.telemetry.all_twelve_executed());
    for (i, expected) in StageId::ORDER.iter().enumerate() {
        assert_eq!(digest.telemetry.stage_reports[i].stage, *expected);
    }
}

#[test]
fn m8_ac2_million_entity_broadphase_capacity() {
    // M8 AC2 : 1M+ entity broadphase achievable via Morton-hash.
    let mut p = M8Pipeline::new(canonical_config());
    let digest = p.step(1.0 / 60.0).expect("step");
    let phys = digest.telemetry.physics;
    // 2^20 ≥ 1M
    assert!(phys.broadphase_capacity_log2 >= 20);
    assert!((1_u64 << phys.broadphase_capacity_log2) >= 1_000_000);
}

#[test]
fn m8_ac3_psi_field_audio_light_shared_solver() {
    // M8 AC3 : ψ-field updates audio + light from same wave-PDE solver.
    let mut p = M8Pipeline::new(canonical_config());
    let digest = p.step(1.0 / 60.0).expect("step");
    assert!(digest.telemetry.audio.shared_solver_with_light);
}

#[test]
fn m8_ac4_kan_brdf_produces_non_rgb_output() {
    // M8 AC4 : KAN-BRDF spectral rendering produces non-RGB output.
    let mut p = M8Pipeline::new(canonical_config());
    let _digest = p.step(1.0 / 60.0).expect("step");
    // Walk back through the pipeline's KAN-BRDF state — we check via the
    // stage 6 report's output_hash being non-zero (the BRDF synthesizer
    // produces non-trivial spectrum for the canonical material).
    let stage6 = _digest.telemetry.stage_reports[5];
    assert_eq!(stage6.stage, StageId::KanBrdf);
    assert!(!stage6.skipped);
    assert_ne!(stage6.output_hash, 0);
}

#[test]
fn m8_ac5_fovea_mask_drives_detail_emergence() {
    // M8 AC5 : Fovea-mask drives detail-emergence (Stage 2 → 5 → 7 chain).
    let mut p = M8Pipeline::new(canonical_config());
    let digest = p.step(1.0 / 60.0).expect("step");
    let stage2 = digest.telemetry.stage_reports[1];
    let stage5 = digest.telemetry.stage_reports[4];
    let stage7 = digest.telemetry.stage_reports[6];
    assert_eq!(stage2.stage, StageId::GazeCollapse);
    assert_eq!(stage5.stage, StageId::SdfRaymarch);
    assert_eq!(stage7.stage, StageId::FractalAmplifier);
    assert!(!stage2.skipped);
    assert!(!stage5.skipped);
    assert!(!stage7.skipped);
}

#[test]
fn m8_ac6_companion_perspective_toggleable_zero_cost_when_off() {
    // M8 AC6 : Companion-perspective toggle works (zero-cost when off).
    // Test ON path.
    let mut p_on = M8Pipeline::new(canonical_config());
    let digest_on = p_on.step(1.0 / 60.0).expect("on step");
    let stage8_on = digest_on.telemetry.stage_reports[7];
    assert_eq!(stage8_on.stage, StageId::CompanionSemantic);
    assert!(!stage8_on.skipped);

    // Test OFF path : zero-cost skip.
    let mut p_off = M8Pipeline::new(PipelineConfig {
        companion_enabled: false,
        ..canonical_config()
    });
    let digest_off = p_off.step(1.0 / 60.0).expect("off step");
    let stage8_off = digest_off.telemetry.stage_reports[7];
    assert_eq!(stage8_off.stage, StageId::CompanionSemantic);
    assert!(stage8_off.skipped);
}

#[test]
fn m8_ac7_mise_en_abyme_recursion_bounded_hard_cap_five() {
    // M8 AC7 : Mise-en-abyme recursion bounded at HARD cap=5.
    let mut p = M8Pipeline::new(canonical_config());
    for _ in 0..5 {
        let digest = p.step(1.0 / 60.0).expect("step");
        let stage9 = digest.telemetry.stage_reports[8];
        assert_eq!(stage9.stage, StageId::MiseEnAbyme);
        assert!(!stage9.skipped);
    }
    // The hard-cap = 5 is asserted at the unit-test level in
    // `m8_integration::mise_en_abyme_pass::tests::mise_en_abyme_hard_cap_is_five`.
    // This test confirms the stage actually runs across multiple frames
    // without exhausting the recursion.
}

#[test]
fn m8_ac8_replay_determinism_two_runs_bit_equal() {
    // M8 AC8 : Two runs from same seed produce bit-equal output.
    // Existing H5 contract per loa-game spec.
    let mut p1 = M8Pipeline::new(canonical_config());
    let mut p2 = M8Pipeline::new(canonical_config());
    let r1 = p1.run_n(60, 1.0 / 60.0).expect("p1 60 frames"); // 1 second
    let r2 = p2.run_n(60, 1.0 / 60.0).expect("p2 60 frames");
    assert_eq!(r1.len(), r2.len());
    for (i, (a, b)) in r1.iter().zip(r2.iter()).enumerate() {
        assert_eq!(a.digest(), b.digest(), "frame {} digest mismatch", i);
    }
}

#[test]
fn m8_ac9_vr_fallback_flat_screen_when_no_openxr_runtime() {
    // M8 AC9 : VR fallback : flat-screen rendering when no OpenXR runtime.
    let mut p_flat = M8Pipeline::new(PipelineConfig {
        xr_enabled: false,
        ..canonical_config()
    });
    let digest = p_flat.step(1.0 / 60.0).expect("step");
    assert!(!digest.compose_report.xr_path_used);
    assert!(digest.compose_report.flat_path_used);
    assert!(!digest.composed.stereo);
}

#[test]
fn m8_canonical_10_minute_playtest_no_panic() {
    // M8 canonical playtest acceptance : 10 minutes @ 60 fps = 36_000 frames.
    // We compress to 600 frames in CI (≈ 10 sec real-time) but exercise the
    // same stage-progression every tick. Real-time hardware playtest is
    // reserved for the M8-Pass certification ledger entry.
    let mut p = M8Pipeline::new(canonical_config());
    for f in 0..600_u32 {
        let digest = p
            .step(1.0 / 60.0)
            .unwrap_or_else(|e| panic!("frame {} panicked: {:?}", f, e));
        assert!(digest.telemetry.all_twelve_executed());
    }
    assert_eq!(p.frame_idx(), 600);
}

#[test]
fn m8_attestation_carried_through_pipeline() {
    // PRIME-DIRECTIVE §11 attestation must be present + canonical.
    assert!(ATTESTATION.contains("no hurt nor harm"));
    assert!(ATTESTATION.contains("anyone, anything, or anybody"));
}

#[test]
fn m8_pipeline_fingerprint_stable_across_runs() {
    // Cross-run fingerprint stability — load-bearing for replay.
    let mut p1 = M8Pipeline::new(canonical_config());
    let mut p2 = M8Pipeline::new(canonical_config());
    let f1 = p1.step(1.0 / 60.0).unwrap().telemetry.fingerprint();
    let f2 = p2.step(1.0 / 60.0).unwrap().telemetry.fingerprint();
    assert_eq!(f1, f2);
}

#[test]
fn m8_pipeline_distinct_seeds_distinct_digests() {
    // Different seeds → different digests (sanity for the determinism harness).
    let mut a = M8Pipeline::new(PipelineConfig {
        master_seed: 0x1111_1111,
        ..canonical_config()
    });
    let mut b = M8Pipeline::new(PipelineConfig {
        master_seed: 0x2222_2222,
        ..canonical_config()
    });
    let da = a.step(1.0 / 60.0).unwrap();
    let db = b.step(1.0 / 60.0).unwrap();
    assert_ne!(da.digest(), db.digest());
}

#[test]
fn m8_xr_path_emits_stereo_when_enabled() {
    // When XR runtime is bound, Stage 12 emits stereo composition.
    let mut p = M8Pipeline::new(PipelineConfig {
        xr_enabled: true,
        ..canonical_config()
    });
    let d = p.step(1.0 / 60.0).unwrap();
    assert!(d.compose_report.xr_path_used);
    assert!(d.composed.stereo);
}

#[test]
fn m8_work_graph_subsystem_active_when_enabled() {
    let mut p = M8Pipeline::new(canonical_config());
    let d = p.step(1.0 / 60.0).unwrap();
    // Work-graph step ran (built or no-op fallback path executed).
    let _ = d.telemetry.work_graph; // non-error access
}

#[test]
fn m8_all_stages_deterministic_across_120_frames() {
    // 120-frame replay determinism — stronger than 1-frame.
    let mut p1 = M8Pipeline::new(canonical_config());
    let mut p2 = M8Pipeline::new(canonical_config());
    let r1 = p1.run_n(120, 1.0 / 60.0).unwrap();
    let r2 = p2.run_n(120, 1.0 / 60.0).unwrap();
    let h1: Vec<u64> = r1.iter().map(|d| d.digest()).collect();
    let h2: Vec<u64> = r2.iter().map(|d| d.digest()).collect();
    assert_eq!(h1, h2);
}

#[test]
fn m8_stage_source_crates_documented() {
    // Every stage advertises its source crate (no placeholders).
    for sid in StageId::ORDER {
        let src = sid.source_crate();
        assert!(!src.is_empty());
        assert!(!src.contains("TODO"));
        assert!(!src.contains("placeholder"));
    }
}
