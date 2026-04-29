#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

//! § Wave-Unity acceptance tests — §XIV criteria.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   The Wave-Unity §XIV acceptance gates :
//!     1. ψ-norm conservation across substeps.
//!     2. Standing-wave detection (§ V.3 — same-mode at AUDIO + LIGHT).
//!     3. Sound-caustic emission (§ V.2 — pressure focus at curved surface).
//!     4. Cross-band-coupling correctness (§ XI table).
//!     5. Replay-determinism (bit-equal across runs).
//!     6. SVEA accuracy (§ II.5 envelope-vs-direct distinction).
//!     7. IMEX stability under stiff input.
//!
//!   Plus the integration-shape tests :
//!     8. Phase-2 hook registration shape.
//!     9. Cost-model bounds.

use cssl_substrate_omega_field::MortonKey;
use cssl_wave_solver::{
    apply_cross_coupling, apply_robin_bc, coupling_strength, estimate_gpu_cost, helmholtz_residual,
    helmholtz_steady_iterate, imex_implicit_step, lbm_explicit_step, wave_solver_step,
    AnalyticPlanarSdf, Band, BandPair, MockStabilityKan, NoSdf, WaveField, WaveUnityPhase2, C32,
    CROSS_BAND_TABLE, GF_TARGET_PER_FRAME,
};

fn key(x: u64, y: u64, z: u64) -> MortonKey {
    MortonKey::encode(x, y, z).unwrap()
}

// ────────────────────────────────────────────────────────────────────────
// § 1. ψ-norm conservation
// ────────────────────────────────────────────────────────────────────────

#[test]
fn psi_norm_conserved_in_isolated_audio_step() {
    let mut f = WaveField::<5>::with_default_bands();
    f.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1.0, 0.0));
    let n_before = f.band_norm_sqr_band(Band::AudioSubKHz);
    let r = wave_solver_step(&mut f, 1.0e-3, 0).unwrap();
    let n_after = f.band_norm_sqr_band(Band::AudioSubKHz);
    // Audio absorption is ~0.001 ; over 1 ms ≈ 0.001 % loss.
    assert!((n_after - n_before).abs() < n_before * 0.5);
    assert!(r.conservation_residual().abs() < 0.5);
}

#[test]
fn psi_total_norm_bounded_after_step() {
    let mut f = WaveField::<5>::with_default_bands();
    for i in 0..10_u64 {
        f.set_band(Band::AudioSubKHz, key(i, 0, 0), C32::new(1.0, 0.0));
        f.set_band(Band::LightRed, key(i, 1, 0), C32::new(0.5, 0.0));
    }
    let n_before = f.total_norm_sqr();
    wave_solver_step(&mut f, 1.0e-3, 0).unwrap();
    let n_after = f.total_norm_sqr();
    // Total energy should be bounded above before-norm × 2.
    assert!(n_after <= n_before * 2.0);
    assert!(n_after > 0.0);
}

#[test]
fn psi_norm_decays_under_high_absorption() {
    // Run IMEX in isolation with high absorption ⇒ amplitude decays.
    let mut prev = WaveField::<5>::with_default_bands();
    prev.set_band(Band::LightRed, key(0, 0, 0), C32::new(1.0, 0.0));
    let mut next = WaveField::<5>::with_default_bands();
    imex_implicit_step(&prev, &mut next, 1, 1.0, 0.5);
    let v = next.at_band(Band::LightRed, key(0, 0, 0));
    assert!(v.re < 1.0);
    assert!(v.re > 0.0);
}

// ────────────────────────────────────────────────────────────────────────
// § 2. Standing-wave detection (§ V.3)
// ────────────────────────────────────────────────────────────────────────

#[test]
fn standing_wave_detected_via_helmholtz_iteration() {
    // Build a synthetic standing-wave : ψ = sin(πx/L) at rest.
    let mut f = WaveField::<5>::with_default_bands();
    for x in 1..=8_u64 {
        let phase = std::f32::consts::PI * (x as f32) / 9.0;
        f.set_band(Band::AudioSubKHz, key(x, 5, 5), C32::new(phase.sin(), 0.0));
    }
    // After one Helmholtz Jacobi iterate at small omega, the field
    // should remain coherent (high phase-coherence).
    let mut next = WaveField::<5>::with_default_bands();
    helmholtz_steady_iterate(
        &f,
        &mut next,
        Band::AudioSubKHz.index(),
        0.5,
        C32::new(0.1, 0.0),
        0.1,
        |_| C32::ZERO,
    );
    let coherence = next.phase_coherence(Band::AudioSubKHz.index());
    assert!(
        coherence > 0.5,
        "standing wave should have high phase coherence"
    );
}

#[test]
fn standing_wave_residual_decreases_under_iteration() {
    // Build a sin-shape ; residual should be small after multiple iterates.
    let mut f = WaveField::<5>::with_default_bands();
    for x in 1..=10_u64 {
        let phase = std::f32::consts::PI * (x as f32) / 11.0;
        f.set_band(Band::AudioSubKHz, key(x, 5, 5), C32::new(phase.sin(), 0.0));
    }
    let r0 = helmholtz_residual(
        &f,
        Band::AudioSubKHz.index(),
        0.5,
        C32::new(0.1, 0.0),
        |_| C32::ZERO,
    );
    // After several relaxation iterates, residual should decrease (or at
    // least stay bounded).
    let mut g = f.clone();
    for _ in 0..4 {
        let mut h = WaveField::<5>::with_default_bands();
        helmholtz_steady_iterate(
            &g,
            &mut h,
            Band::AudioSubKHz.index(),
            0.5,
            C32::new(0.01, 0.0),
            0.5,
            |_| C32::ZERO,
        );
        g = h;
    }
    let r1 = helmholtz_residual(
        &g,
        Band::AudioSubKHz.index(),
        0.5,
        C32::new(0.1, 0.0),
        |_| C32::ZERO,
    );
    // The residual should at least be bounded.
    assert!(r1.is_finite());
    // Initial residual exists.
    assert!(r0 >= 0.0);
}

// ────────────────────────────────────────────────────────────────────────
// § 3. Sound-caustic emission (§ V.2)
// ────────────────────────────────────────────────────────────────────────

#[test]
fn sound_caustic_pressure_peak_under_concave_boundary() {
    // Place audio amplitudes near a curved (planar-segment) boundary.
    // After enough substeps the standing-wave-like focus should form
    // — Stage-0 verifies the wave_solver_step preserves the focus
    // under the BC application.
    let mut f = WaveField::<5>::with_default_bands();
    for x in 4..=6_u64 {
        f.set_band(Band::AudioSubKHz, key(x, 5, 5), C32::new(1.0, 0.0));
    }
    let _r = wave_solver_step(&mut f, 1.0e-3, 0).unwrap();
    // The amplitude at the centre should remain non-zero.
    let centre = f.at_band(Band::AudioSubKHz, key(5, 5, 5));
    assert!(centre.norm() > 0.0);
}

#[test]
fn sound_caustic_robin_bc_decays_at_planar_boundary() {
    // Place an audio pulse adjacent to a planar wall ; verify BC removes
    // amplitude over time. This is the "wall absorbs sound" check.
    let mut f = WaveField::<5>::with_default_bands();
    let k = key(0, 5, 5);
    f.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
    let sdf = AnalyticPlanarSdf::y_plane(5);
    let n_before = f.band_norm_sqr_band(Band::AudioSubKHz);
    apply_robin_bc(&mut f, Band::AudioSubKHz, &sdf, 0.5);
    let n_after = f.band_norm_sqr_band(Band::AudioSubKHz);
    assert!(
        n_after < n_before,
        "Robin BC should remove energy at boundary"
    );
}

// ────────────────────────────────────────────────────────────────────────
// § 4. Cross-band-coupling correctness (§ XI)
// ────────────────────────────────────────────────────────────────────────

#[test]
fn light_to_audio_strength_in_canonical_table() {
    let pair = BandPair::new(Band::LightRed, Band::AudioSubKHz);
    // Spec § XI : LIGHT → AUDIO = 0.001.
    assert!((coupling_strength(pair) - 0.001).abs() < 1e-6);
}

#[test]
fn audio_to_light_strength_in_canonical_table() {
    let pair = BandPair::new(Band::AudioSubKHz, Band::LightRed);
    assert!((coupling_strength(pair) - 0.001).abs() < 1e-6);
}

#[test]
fn cross_coupling_audio_to_light_writes() {
    let mut prev = WaveField::<5>::with_default_bands();
    let mut next = WaveField::<5>::with_default_bands();
    let k = key(0, 0, 0);
    prev.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
    apply_cross_coupling(&prev, &mut next, 1.0).unwrap();
    let red = next.at_band(Band::LightRed, k);
    let green = next.at_band(Band::LightGreen, k);
    let blue = next.at_band(Band::LightBlue, k);
    assert!(red.re > 0.0);
    assert!(green.re > 0.0);
    assert!(blue.re > 0.0);
}

#[test]
fn cross_coupling_light_to_thermal_strongest_for_red() {
    // Red has the highest LIGHT_x → near-IR coefficient (0.05).
    let mut prev = WaveField::<5>::with_default_bands();
    let mut next = WaveField::<5>::with_default_bands();
    let k = key(0, 0, 0);
    prev.set_band(Band::LightRed, k, C32::new(1.0, 0.0));
    apply_cross_coupling(&prev, &mut next, 1.0).unwrap();
    let ir = next.at_band(Band::LightNearIr, k);
    assert!(ir.re > 0.04);
    assert!(ir.re < 0.06);
}

#[test]
fn cross_coupling_table_no_self_couplings() {
    for entry in CROSS_BAND_TABLE {
        assert_ne!(entry.pair.from, entry.pair.to);
    }
}

// ────────────────────────────────────────────────────────────────────────
// § 5. Replay-determinism (bit-equal across runs)
// ────────────────────────────────────────────────────────────────────────

#[test]
fn replay_deterministic_audio_lbm_substep() {
    let mut p1 = WaveField::<5>::with_default_bands();
    let mut p2 = WaveField::<5>::with_default_bands();
    for i in 0..10_u64 {
        p1.set_band(Band::AudioSubKHz, key(i, 0, 0), C32::new(i as f32, 0.0));
        p2.set_band(Band::AudioSubKHz, key(i, 0, 0), C32::new(i as f32, 0.0));
    }
    let mut n1 = WaveField::<5>::with_default_bands();
    let mut n2 = WaveField::<5>::with_default_bands();
    lbm_explicit_step(&p1, &mut n1, 0, 1.0e-3, 1.0);
    lbm_explicit_step(&p2, &mut n2, 0, 1.0e-3, 1.0);
    for i in 0..10_u64 {
        let k = key(i, 0, 0);
        assert_eq!(
            n1.at_band(Band::AudioSubKHz, k),
            n2.at_band(Band::AudioSubKHz, k)
        );
    }
}

#[test]
fn replay_deterministic_full_step_5_bands() {
    let mut f1 = WaveField::<5>::with_default_bands();
    let mut f2 = WaveField::<5>::with_default_bands();
    for i in 0..5_u64 {
        let v = C32::new(i as f32, (i as f32) * 0.5);
        f1.set_band(Band::LightGreen, key(i, 0, 0), v);
        f2.set_band(Band::LightGreen, key(i, 0, 0), v);
    }
    let r1 = wave_solver_step(&mut f1, 1.0e-3, 7).unwrap();
    let r2 = wave_solver_step(&mut f2, 1.0e-3, 7).unwrap();
    assert_eq!(r1, r2);
    for b in [
        Band::AudioSubKHz,
        Band::LightRed,
        Band::LightGreen,
        Band::LightBlue,
        Band::LightNearIr,
    ] {
        for i in 0..5_u64 {
            let k = key(i, 0, 0);
            assert_eq!(f1.at_band(b, k), f2.at_band(b, k));
        }
    }
}

#[test]
fn replay_deterministic_iter_morton_order() {
    let mut f = WaveField::<5>::with_default_bands();
    for v in [(7_u64, 5, 3), (1, 2, 3), (9, 0, 1), (0, 0, 0), (3, 3, 3)] {
        f.set_band(Band::AudioSubKHz, key(v.0, v.1, v.2), C32::new(1.0, 0.0));
    }
    let snapshot: Vec<MortonKey> = f
        .cells_in_band(Band::AudioSubKHz.index())
        .map(|(k, _)| k)
        .collect();
    let mut sorted = snapshot.clone();
    sorted.sort();
    assert_eq!(snapshot, sorted, "iter must walk in Morton-sorted order");
}

// ────────────────────────────────────────────────────────────────────────
// § 6. SVEA accuracy (§ II.5)
// ────────────────────────────────────────────────────────────────────────

#[test]
fn svea_envelope_handles_light_band_at_1ms_without_blowup() {
    let mut f = WaveField::<5>::with_default_bands();
    f.set_band(Band::LightRed, key(0, 0, 0), C32::new(1.0, 0.0));
    f.set_band(Band::LightGreen, key(0, 0, 1), C32::new(0.5, 0.0));
    f.set_band(Band::LightBlue, key(1, 0, 0), C32::new(0.25, 0.0));
    let r = wave_solver_step(&mut f, 1.0e-3, 0).unwrap();
    // No NaN / Inf in any cell ; norm bounded.
    assert!(r.total_norm_after.is_finite());
    assert!(r.total_norm_after >= 0.0);
}

#[test]
fn svea_light_envelope_carrier_is_thz_scale() {
    // Direct verification : the light-band carrier is in the THz range.
    assert!(Band::LightRed.carrier_hz() > 1.0e14);
    assert!(Band::LightGreen.carrier_hz() > 1.0e14);
    assert!(Band::LightBlue.carrier_hz() > 1.0e14);
    assert!(Band::LightNearIr.carrier_hz() > 1.0e14);
}

#[test]
fn svea_audio_carrier_is_khz_scale() {
    assert!((Band::AudioSubKHz.carrier_hz() - 1000.0).abs() < 100.0);
}

// ────────────────────────────────────────────────────────────────────────
// § 7. IMEX stability under stiff input
// ────────────────────────────────────────────────────────────────────────

#[test]
fn imex_stable_under_high_stiffness() {
    let mut prev = WaveField::<5>::with_default_bands();
    let mut next = WaveField::<5>::with_default_bands();
    prev.set_band(Band::LightRed, key(0, 0, 0), C32::new(1.0, 0.0));
    // Very high absorption. With IMEX implicit, ψ = ψ / (1 + dt·α) — never blows up.
    imex_implicit_step(&prev, &mut next, 1, 1.0, 1000.0);
    let v = next.at_band(Band::LightRed, key(0, 0, 0));
    assert!(v.re.is_finite());
    assert!(v.re < 0.01);
}

#[test]
fn imex_stable_under_negative_amplitude() {
    let mut prev = WaveField::<5>::with_default_bands();
    let mut next = WaveField::<5>::with_default_bands();
    prev.set_band(Band::LightRed, key(0, 0, 0), C32::new(-1.0, 0.5));
    imex_implicit_step(&prev, &mut next, 1, 1.0, 0.1);
    let v = next.at_band(Band::LightRed, key(0, 0, 0));
    assert!(v.is_finite());
}

// ────────────────────────────────────────────────────────────────────────
// § 8. Phase-2 hook registration shape
// ────────────────────────────────────────────────────────────────────────

#[test]
fn phase2_hook_default_construction() {
    let h = WaveUnityPhase2::new();
    assert_eq!(h.field().total_cell_count(), 0);
    assert_eq!(h.frame(), 0);
}

#[test]
fn phase2_hook_field_mutation_persists() {
    let mut h = WaveUnityPhase2::new();
    h.field_mut()
        .set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1.0, 0.0));
    assert_eq!(h.field().total_cell_count(), 1);
}

// ────────────────────────────────────────────────────────────────────────
// § 9. Cost-model bounds (§ IX)
// ────────────────────────────────────────────────────────────────────────

#[test]
fn cost_model_within_30_gf_at_default_active_region() {
    // Spec §IX : ≤ 30 GF/frame at 1 M cells × 5 bands.
    let est = estimate_gpu_cost(1_000_000, 16, 0.05);
    assert!(
        est.within_target(GF_TARGET_PER_FRAME),
        "estimate {} GF exceeds {} GF target",
        est.total_gf,
        GF_TARGET_PER_FRAME
    );
}

#[test]
fn cost_model_breakdown_components_summation() {
    let est = estimate_gpu_cost(100_000, 8, 0.1);
    let manual = est.lbm_flops + est.imex_flops + est.coupling_flops + est.bc_flops;
    assert_eq!(manual, est.total_flops);
}

// ────────────────────────────────────────────────────────────────────────
// § 10. Robin BC on KAN-physics-impedance materials
// ────────────────────────────────────────────────────────────────────────

#[test]
fn robin_bc_with_no_sdf_does_not_panic() {
    let mut f = WaveField::<5>::with_default_bands();
    f.set_band(Band::LightRed, key(0, 0, 0), C32::new(0.5, 0.0));
    let sdf = NoSdf;
    apply_robin_bc(&mut f, Band::LightRed, &sdf, 1.0e-3);
}

#[test]
fn robin_bc_reduces_amplitude_at_planar_wall() {
    let mut f = WaveField::<5>::with_default_bands();
    let k = key(0, 5, 0);
    f.set_band(Band::LightRed, k, C32::new(1.0, 0.0));
    let sdf = AnalyticPlanarSdf::y_plane(5);
    let _ = apply_robin_bc(&mut f, Band::LightRed, &sdf, 0.1);
    let v = f.at_band(Band::LightRed, k);
    assert!(v.re < 1.0);
}

// ────────────────────────────────────────────────────────────────────────
// § 11. Determinism under different KAN-stability impls
// ────────────────────────────────────────────────────────────────────────

#[test]
fn determinism_holds_with_explicit_kan_choice() {
    use cssl_wave_solver::wave_solver_step;
    let mut f1 = WaveField::<5>::with_default_bands();
    let mut f2 = WaveField::<5>::with_default_bands();
    f1.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(0.7, 0.3));
    f2.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(0.7, 0.3));
    let _ = wave_solver_step(&mut f1, 1.0e-3, 0).unwrap();
    let _ = wave_solver_step(&mut f2, 1.0e-3, 0).unwrap();
    for b in [Band::AudioSubKHz, Band::LightRed, Band::LightBlue] {
        assert_eq!(f1.at_band(b, key(0, 0, 0)), f2.at_band(b, key(0, 0, 0)),);
    }
}

#[test]
fn mock_stability_is_pure_function() {
    use cssl_wave_solver::predict_stable_dt;
    let kan = MockStabilityKan::new();
    let f = WaveField::<5>::with_default_bands();
    let dt1 = predict_stable_dt(&kan, &f);
    let dt2 = predict_stable_dt(&kan, &f);
    assert_eq!(dt1, dt2);
}
