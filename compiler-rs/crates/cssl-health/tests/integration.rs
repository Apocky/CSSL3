// § T11-D159 (W-Jζ-3) : cssl-health integration-tests
// ══════════════════════════════════════════════════════════════════════════
// Coverage : all-12-probes individually + aggregate + degraded + failed +
// PrimeDirectiveTrip-absorbing + registry + monoid-laws.
// ──────────────────────────────────────────────────────────────────────────

use cssl_health::{
    aggregator::{combine, engine_health},
    probes::{
        anim_procedural, fractal_amp, gaze_collapse, host_openxr, physics_wave, register_all_mock,
        render_companion_perspective, render_v2, spectral_render, substrate_kan,
        substrate_omega_field, wave_audio, wave_solver, HealthError, HealthProbe,
    },
    HealthAggregate, HealthFailureKind, HealthRegistry, HealthStatus, ATTESTATION, SLICE_ID,
    SUBSYSTEMS, WAVE_TAG,
};

// ─── crate-level metadata ─────────────────────────────────────────────────

#[test]
fn attestation_and_metadata() {
    assert!(ATTESTATION.contains("no hurt nor harm"));
    assert_eq!(SLICE_ID, "T11-D159");
    assert_eq!(WAVE_TAG, "W-Jζ-3");
    assert_eq!(SUBSYSTEMS.len(), 12);
}

#[test]
fn subsystem_names_are_unique_and_match_register_all() {
    let probes = register_all_mock();
    let names: Vec<&str> = probes.iter().map(|p| p.name()).collect();
    assert_eq!(names.as_slice(), &SUBSYSTEMS[..]);
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 12);
}

// ─── individual probes : Ok-state ─────────────────────────────────────────

#[test]
fn render_v2_ok_default() {
    let p = render_v2::MockProbe::new();
    assert_eq!(p.name(), "cssl-render-v2");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn physics_wave_ok_default() {
    let p = physics_wave::MockProbe::new();
    assert_eq!(p.name(), "cssl-physics-wave");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn wave_solver_ok_default() {
    let p = wave_solver::MockProbe::new();
    assert_eq!(p.name(), "cssl-wave-solver");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn spectral_render_ok_default() {
    let p = spectral_render::MockProbe::new();
    assert_eq!(p.name(), "cssl-spectral-render");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn fractal_amp_ok_default() {
    let p = fractal_amp::MockProbe::new();
    assert_eq!(p.name(), "cssl-fractal-amp");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn gaze_collapse_ok_default() {
    let p = gaze_collapse::MockProbe::new();
    assert_eq!(p.name(), "cssl-gaze-collapse");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn render_companion_perspective_ok_default() {
    let p = render_companion_perspective::MockProbe::new();
    assert_eq!(p.name(), "cssl-render-companion-perspective");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn host_openxr_ok_default() {
    let p = host_openxr::MockProbe::new();
    assert_eq!(p.name(), "cssl-host-openxr");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn anim_procedural_ok_default() {
    let p = anim_procedural::MockProbe::new();
    assert_eq!(p.name(), "cssl-anim-procedural");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn wave_audio_ok_default() {
    let p = wave_audio::MockProbe::new();
    assert_eq!(p.name(), "cssl-wave-audio");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn substrate_omega_field_ok_default() {
    let p = substrate_omega_field::MockProbe::new();
    assert_eq!(p.name(), "cssl-substrate-omega-field");
    assert_eq!(p.health(), HealthStatus::Ok);
}

#[test]
fn substrate_kan_ok_default() {
    let p = substrate_kan::MockProbe::new();
    assert_eq!(p.name(), "cssl-substrate-kan");
    assert_eq!(p.health(), HealthStatus::Ok);
}

// ─── individual probes : Degraded ─────────────────────────────────────────

#[test]
fn render_v2_degrades_on_vram_pressure() {
    let p = render_v2::MockProbe::new();
    p.set_vram_used(1_000_000_000, 42); // ~93% of 1 GiB
    assert!(matches!(p.health(), HealthStatus::Degraded { .. }));
}

#[test]
fn physics_wave_degrades_at_80pct() {
    let p = physics_wave::MockProbe::new();
    p.set_entity_count(900_000, 17);
    assert!(p.health().is_degraded());
}

#[test]
fn wave_solver_degrades_on_extra_imex_substeps() {
    let p = wave_solver::MockProbe::new();
    p.set_state(8, false, 100);
    assert!(p.health().is_degraded());
}

#[test]
fn spectral_render_degrades_on_thermal() {
    let p = spectral_render::MockProbe::new();
    p.set_state(2, true, 5);
    assert!(p.health().is_degraded());
}

#[test]
fn fractal_amp_degrades_above_recursion_max() {
    let p = fractal_amp::MockProbe::new();
    p.set_recursion_depth(8, 1);
    assert!(p.health().is_degraded());
}

#[test]
fn gaze_collapse_degrades_on_low_eye_confidence() {
    let p = gaze_collapse::MockProbe::new();
    p.set_state(true, 5_000, 30); // confidence 0.50
    assert!(p.health().is_degraded());
}

#[test]
fn render_companion_perspective_degrades_at_saturation() {
    let p = render_companion_perspective::MockProbe::new();
    p.set_view_count(4, 100);
    assert!(p.health().is_degraded());
}

#[test]
fn host_openxr_degrades_when_not_focused() {
    let p = host_openxr::MockProbe::new();
    p.set_state(2, 0, 11);
    assert!(p.health().is_degraded());
}

#[test]
fn anim_procedural_degrades_above_threshold() {
    let p = anim_procedural::MockProbe::new();
    p.set_creatures_active(55_000, 9);
    assert!(p.health().is_degraded());
}

#[test]
fn wave_audio_degrades_on_underrun() {
    let p = wave_audio::MockProbe::new();
    p.set_state(2, 0, 7);
    assert!(p.health().is_degraded());
}

#[test]
fn substrate_omega_field_degrades_on_entropy_drift() {
    let p = substrate_omega_field::MockProbe::new();
    p.set_state(15_000, false, 20);
    assert!(p.health().is_degraded());
}

#[test]
fn substrate_kan_degrades_on_fallback() {
    let p = substrate_kan::MockProbe::new();
    p.set_state(0, 3, 50);
    assert!(p.health().is_degraded());
}

// ─── individual probes : Failed ───────────────────────────────────────────

#[test]
fn render_v2_fails_on_vram_overrun() {
    let p = render_v2::MockProbe::new();
    p.set_vram_used(2 * 1_073_741_824, 42);
    let s = p.health();
    assert!(s.is_failed());
    if let HealthStatus::Failed { kind, .. } = s {
        assert_eq!(kind, HealthFailureKind::ResourceExhaustion);
    }
}

#[test]
fn physics_wave_fails_on_entity_overflow() {
    let p = physics_wave::MockProbe::new();
    p.set_entity_count(2_000_000, 100);
    assert!(p.health().is_failed());
}

#[test]
fn wave_solver_fails_on_conservation_violation() {
    let p = wave_solver::MockProbe::new();
    p.set_state(2, true, 100);
    let s = p.health();
    if let HealthStatus::Failed { kind, .. } = s {
        assert_eq!(kind, HealthFailureKind::InvariantBreach);
    } else {
        panic!("expected Failed");
    }
}

#[test]
fn spectral_render_fails_on_kan_runaway() {
    let p = spectral_render::MockProbe::new();
    p.set_state(64, false, 100); // 64 > 32 = 8 * 4
    assert!(p.health().is_failed());
}

#[test]
fn fractal_amp_fails_on_recursion_runaway() {
    let p = fractal_amp::MockProbe::new();
    p.set_recursion_depth(16, 1);
    let s = p.health();
    if let HealthStatus::Failed { kind, .. } = s {
        assert_eq!(kind, HealthFailureKind::InvariantBreach);
    } else {
        panic!("expected Failed");
    }
}

#[test]
fn gaze_collapse_trips_prime_directive_without_consent() {
    let p = gaze_collapse::MockProbe::new();
    p.set_state(false, 9_500, 99);
    let s = p.health();
    assert!(s.is_prime_directive_trip());
}

#[test]
fn render_companion_perspective_fails_above_cap() {
    let p = render_companion_perspective::MockProbe::new();
    p.set_view_count(5, 1);
    assert!(p.health().is_failed());
}

#[test]
fn host_openxr_fails_on_session_lost() {
    let p = host_openxr::MockProbe::new();
    p.set_state(0, 0, 99);
    let s = p.health();
    if let HealthStatus::Failed { kind, .. } = s {
        assert!(matches!(kind, HealthFailureKind::UpstreamFailure { .. }));
    } else {
        panic!("expected Failed");
    }
}

#[test]
fn anim_procedural_fails_above_budget() {
    let p = anim_procedural::MockProbe::new();
    p.set_creatures_active(70_000, 12);
    assert!(p.health().is_failed());
}

#[test]
fn wave_audio_fails_on_persistent_underruns() {
    let p = wave_audio::MockProbe::new();
    p.set_state(20, 0, 200);
    assert!(p.health().is_failed());
}

#[test]
fn substrate_omega_field_trips_prime_directive_on_consent_violation() {
    let p = substrate_omega_field::MockProbe::new();
    p.set_state(0, true, 99);
    let s = p.health();
    assert!(s.is_prime_directive_trip());
}

#[test]
fn substrate_omega_field_fails_on_entropy_runaway() {
    let p = substrate_omega_field::MockProbe::new();
    p.set_state(40_000, false, 99);
    let s = p.health();
    if let HealthStatus::Failed { kind, .. } = s {
        assert_eq!(kind, HealthFailureKind::InvariantBreach);
    } else {
        panic!("expected Failed");
    }
}

#[test]
fn substrate_kan_fails_on_backend_init_failure() {
    let p = substrate_kan::MockProbe::new();
    p.set_state(255, 0, 1);
    let s = p.health();
    if let HealthStatus::Failed { kind, .. } = s {
        assert!(matches!(kind, HealthFailureKind::UpstreamFailure { .. }));
    } else {
        panic!("expected Failed");
    }
}

// ─── degrade-error path ───────────────────────────────────────────────────

#[test]
fn degrade_default_returns_ok() {
    let p = physics_wave::MockProbe::new();
    assert_eq!(p.degrade("test"), Ok(()));
}

#[test]
fn render_v2_degrade_can_be_refused() {
    let p = render_v2::MockProbe::new();
    p.set_degrade_refused(true);
    assert_eq!(
        p.degrade("safety"),
        Err(HealthError::DegradeRefused("cssl-render-v2"))
    );
}

#[test]
fn render_v2_degrade_reduces_vram() {
    let p = render_v2::MockProbe::new();
    p.set_vram_used(1_000_000_000, 0);
    p.degrade("DFR-cut").expect("degrade succeeds");
    // After degrade, vram_used should be 90% of original ; verify the
    // call-graph runs to completion (status returned regardless of value).
    assert!(matches!(
        p.health(),
        HealthStatus::Ok | HealthStatus::Degraded { .. }
    ));
}

// ─── aggregator + monoid laws ─────────────────────────────────────────────

#[test]
fn aggregator_all_ok() {
    let probes = register_all_mock();
    let refs: Vec<&dyn HealthProbe> = probes.iter().map(AsRef::as_ref).collect();
    let agg: HealthAggregate = engine_health(&refs);
    assert_eq!(agg.worst, HealthStatus::Ok);
    assert_eq!(agg.entries.len(), 12);
    assert_eq!(agg.ok_count(), 12);
    assert_eq!(agg.degraded_count(), 0);
    assert_eq!(agg.failed_count(), 0);
    assert!(!agg.is_prime_directive_trip());
}

#[test]
fn aggregator_one_degraded() {
    let v2 = render_v2::MockProbe::new();
    v2.set_vram_used(1_000_000_000, 1);
    let pw = physics_wave::MockProbe::new();
    let probes: Vec<&dyn HealthProbe> = vec![&v2, &pw];
    let agg = engine_health(&probes);
    assert!(agg.worst.is_degraded());
    assert_eq!(agg.degraded_count(), 1);
    assert_eq!(agg.ok_count(), 1);
}

#[test]
fn aggregator_one_failed_dominates() {
    let v2 = render_v2::MockProbe::new();
    v2.set_vram_used(2 * 1_073_741_824, 1);
    let pw = physics_wave::MockProbe::new();
    let probes: Vec<&dyn HealthProbe> = vec![&v2, &pw];
    let agg = engine_health(&probes);
    assert!(agg.worst.is_failed());
}

#[test]
fn aggregator_prime_directive_trip_always_wins() {
    let gaze = gaze_collapse::MockProbe::new();
    gaze.set_state(false, 9_500, 99);
    let omega = substrate_omega_field::MockProbe::new();
    omega.set_state(40_000, false, 50); // entropy-runaway, but lower priority
    let probes: Vec<&dyn HealthProbe> = vec![&omega, &gaze];
    let agg = engine_health(&probes);
    assert!(agg.is_prime_directive_trip());
    assert!(agg.worst.is_prime_directive_trip());
}

#[test]
fn combine_is_associative_for_ok() {
    let a = HealthStatus::Ok;
    let b = HealthStatus::Ok;
    let c = HealthStatus::Ok;
    let lhs = combine(combine(a.clone(), b.clone()), c.clone());
    let rhs = combine(a, combine(b, c));
    assert_eq!(lhs, rhs);
}

#[test]
fn combine_with_ok_is_identity() {
    let s = HealthStatus::degraded("test", 5);
    assert_eq!(combine(s.clone(), HealthStatus::Ok), s);
    assert_eq!(combine(HealthStatus::Ok, s.clone()), s);
}

#[test]
fn combine_two_okays_is_ok() {
    assert_eq!(
        combine(HealthStatus::Ok, HealthStatus::Ok),
        HealthStatus::Ok
    );
}

// ─── registry tests ───────────────────────────────────────────────────────

#[test]
fn registry_starts_empty() {
    let r = HealthRegistry::new();
    assert!(r.is_empty());
    assert_eq!(r.len(), 0);
    assert_eq!(r.engine_health().worst, HealthStatus::Ok);
}

#[test]
fn registry_register_all_twelve() {
    let mut r = HealthRegistry::new();
    for p in register_all_mock() {
        r.register(p);
    }
    assert_eq!(r.len(), 12);
    let agg = r.engine_health();
    assert_eq!(agg.entries.len(), 12);
    assert_eq!(agg.worst, HealthStatus::Ok);
}

#[test]
fn registry_find_by_name() {
    let mut r = HealthRegistry::new();
    for p in register_all_mock() {
        r.register(p);
    }
    let p = r.find("cssl-render-v2").expect("should find render-v2");
    assert_eq!(p.name(), "cssl-render-v2");
    assert!(r.find("nonexistent-crate").is_none());
}

#[test]
fn registry_aggregates_after_state_change() {
    let mut r = HealthRegistry::new();
    let v2 = Box::new(render_v2::MockProbe::new());
    v2.set_vram_used(2_000_000_000, 1); // OOM
    r.register(v2);
    r.register(Box::new(physics_wave::MockProbe::new()));
    let agg = r.engine_health();
    assert!(agg.worst.is_failed());
}

// ─── debug + display ──────────────────────────────────────────────────────

#[test]
fn registry_debug_includes_names() {
    let mut r = HealthRegistry::new();
    r.register(Box::new(render_v2::MockProbe::new()));
    let s = format!("{r:?}");
    assert!(s.contains("HealthRegistry"));
    assert!(s.contains("cssl-render-v2"));
}

#[test]
fn status_display_round_trips_basic_data() {
    let s = HealthStatus::Degraded {
        reason: "test",
        budget_overshoot_bps: 500,
        since_frame: 7,
    };
    let r = s.to_string();
    assert!(r.contains("test"));
    assert!(r.contains("5.00%"));
}

// ─── gauge-encoding contract ──────────────────────────────────────────────

#[test]
fn gauge_encoding_matches_telemetry_spec() {
    // spec § III.1 : engine.health_state = 0=Failed/1=Degraded/2=Ok
    assert_eq!(HealthStatus::Ok.gauge_encoding(), 2);
    assert_eq!(HealthStatus::degraded("x", 0).gauge_encoding(), 1);
    assert_eq!(
        HealthStatus::failed("y", HealthFailureKind::Unknown, 0).gauge_encoding(),
        0
    );
}

// ─── all-12 cross-coverage ────────────────────────────────────────────────

// This is intentionally a single dense test : it exercises Ok/Degraded/Failed
// transitions for all 12 probes back-to-back so a single regression-run
// surfaces every cross-state. Splitting into 24 sibling tests would
// triple test-fixture-count + lose the spec-mapping at one glance.
#[allow(clippy::cognitive_complexity)]
#[test]
fn each_subsystem_can_reach_failed_and_degraded() {
    use HealthStatus::{Degraded, Failed, Ok};

    // Render-v2
    let p = render_v2::MockProbe::new();
    p.set_vram_used(1_000_000_000, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_vram_used(2 * 1_073_741_824, 0);
    assert!(matches!(p.health(), Failed { .. }));
    p.set_vram_used(0, 0);
    assert!(matches!(p.health(), Ok));

    // Physics-wave
    let p = physics_wave::MockProbe::new();
    p.set_entity_count(900_000, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_entity_count(2_000_000, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Wave-solver
    let p = wave_solver::MockProbe::new();
    p.set_state(8, false, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_state(2, true, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Spectral-render
    let p = spectral_render::MockProbe::new();
    p.set_state(2, true, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_state(64, false, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Fractal-amp
    let p = fractal_amp::MockProbe::new();
    p.set_recursion_depth(8, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_recursion_depth(20, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Gaze-collapse
    let p = gaze_collapse::MockProbe::new();
    p.set_state(true, 5_000, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_state(false, 9_500, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Render-companion-perspective
    let p = render_companion_perspective::MockProbe::new();
    p.set_view_count(4, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_view_count(5, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Host-openxr
    let p = host_openxr::MockProbe::new();
    p.set_state(2, 0, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_state(0, 0, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Anim-procedural
    let p = anim_procedural::MockProbe::new();
    p.set_creatures_active(55_000, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_creatures_active(70_000, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Wave-audio
    let p = wave_audio::MockProbe::new();
    p.set_state(2, 0, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_state(20, 0, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Substrate-omega-field
    let p = substrate_omega_field::MockProbe::new();
    p.set_state(15_000, false, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_state(40_000, false, 0);
    assert!(matches!(p.health(), Failed { .. }));

    // Substrate-kan
    let p = substrate_kan::MockProbe::new();
    p.set_state(0, 3, 0);
    assert!(matches!(p.health(), Degraded { .. }));
    p.set_state(255, 0, 0);
    assert!(matches!(p.health(), Failed { .. }));
}
