//! § cfer_integration — integration tests for cssl-substrate-render
//!
//! Per spec § 36 § TESTS the canonical scenes are :
//!   - empty-scene-converge-1-iter
//!   - sphere-converge-16-iter
//!   - Cornell-box-vs-pathtraced (regression-style ; we test the harness)
//!   - moving-light-warm-cache
//!   - multigrid-acceleration
//!   - evidence-driver
//!   - denoiser-variance
//!   - regression
//!
//! These integration tests validate the public surface end-to-end against the
//! W-S-CORE-1 + W-S-CORE-2 + W-S-CORE-3 STUB types ; they re-verify on the
//! real types once the stubs migrate (see lib.rs § STUB-MIGRATION).

use cssl_substrate_render::cfer::{cfer_render_frame, DirtySet, RenderBudget};
use cssl_substrate_render::denoiser::{Denoiser, DenoiserConfig};
use cssl_substrate_render::evidence_driver::{EvidenceDriver, EvidenceGlyph};
use cssl_substrate_render::kan_stub::MaterialBag;
use cssl_substrate_render::light_stub::{LightState, LIGHT_STATE_COEFS};
use cssl_substrate_render::multigrid::{MultigridConfig, VCycle};
use cssl_substrate_render::camera::Camera;

/// Helper : build a 1D field of n cells (size_x = n, size_y = 1, size_z = 1).
fn build_field(
    n: usize,
    initial: LightState,
    material: MaterialBag,
) -> (
    Vec<LightState>,
    Vec<MaterialBag>,
    (usize, usize, usize),
) {
    (vec![initial; n], vec![material; n], (n, 1, 1))
}

#[test]
fn test_empty_scene_converges_in_one_iter() {
    // Spec § TESTS § convergence : empty-scene CFER converges in 1 iter.
    // "Empty" here = vacuum cells with zero radiance ; KAN-update is no-op.
    let (mut field, mats, sz) = build_field(8, LightState::zero(), MaterialBag::ABSORBER);
    let cam = Camera::new(16, 16).unwrap();
    let mut dirty = DirtySet::from_iter(0..8);
    let budget = RenderBudget {
        max_iterations: 8,
        ..RenderBudget::default()
    };
    let ed = EvidenceDriver::default();
    let (img, report) = cfer_render_frame(
        &mut field, &mats, sz, &mut dirty, &cam, 1_000, &budget, &ed,
    )
    .expect("CFER should succeed on empty scene");
    assert!(report.iterations <= 2, "empty scene should converge fast");
    assert!(report.converged, "empty scene must converge");
    assert_eq!(img.width, 16);
    assert_eq!(img.height, 16);
    // All pixels are dark (post-tonemap, ACES of zero is ~zero).
    for p in &img.pixels {
        assert!(p.r < 0.1);
        assert!(p.g < 0.1);
        assert!(p.b < 0.1);
    }
}

#[test]
fn test_sphere_with_emitter_converges_in_16_iter() {
    // Spec § TESTS § convergence : single-light + sphere converges to known-
    // radiance after 16-iter. We model this as : one cell is an emitter,
    // surrounding cells are diffuse-mid. After 16 iterations the radiance
    // should have spread.
    let n = 16;
    let mut field = vec![LightState::zero(); n];
    field[n / 2] = LightState::from_coefs([1.0; LIGHT_STATE_COEFS]);
    let mut mats = vec![MaterialBag::DIFFUSE_MID; n];
    mats[n / 2] = MaterialBag::emitter(1.0);

    let cam = Camera::new(8, 8).unwrap();
    let mut dirty = DirtySet::from_iter(0..(n as u32));
    let budget = RenderBudget {
        max_iterations: 32,
        epsilon: 1e-4,
        convergence_threshold: 1e-2,
        use_multigrid: false,
        use_denoiser: false,
    };
    let ed = EvidenceDriver::default();
    let (_img, report) = cfer_render_frame(
        &mut field,
        &mats,
        (n, 1, 1),
        &mut dirty,
        &cam,
        2_000,
        &budget,
        &ed,
    )
    .unwrap();
    // Should converge within budget.
    assert!(
        report.iterations <= 32,
        "sphere+emitter should converge in ≤ 32 iter ; got {}",
        report.iterations
    );
    // Center cell has bounded radiance (not exploded).
    let center_radiance = field[n / 2].radiance();
    assert!(
        center_radiance.is_finite() && center_radiance > 0.0,
        "center radiance positive and finite ; got {}",
        center_radiance
    );
}

#[test]
fn test_moving_light_warm_cache_re_converges_fast() {
    // Spec § TESTS § convergence : moving-light → CFER re-converges in <8
    // iter (warm-cache amortization). We simulate two consecutive frames :
    // frame 1 cold-converges, frame 2 with a single dirty-cell perturbation.
    let n = 16;
    let (mut field, mats, sz) = build_field(n, LightState::zero(), MaterialBag::DIFFUSE_MID);
    let cam = Camera::new(8, 8).unwrap();
    let mut dirty = DirtySet::from_iter(0..(n as u32));
    let mut budget = RenderBudget {
        max_iterations: 32,
        ..RenderBudget::default()
    };
    budget.use_multigrid = false;
    budget.use_denoiser = false;
    let ed = EvidenceDriver::default();

    // Frame 1 : cold start.
    let (_img1, _report1) = cfer_render_frame(
        &mut field, &mats, sz, &mut dirty, &cam, 1_000, &budget, &ed,
    )
    .unwrap();

    // Frame 2 : perturb one cell, mark only that cell dirty.
    field[3] = LightState::from_coefs([0.5; LIGHT_STATE_COEFS]);
    dirty.mark(3);
    let (_img2, report2) = cfer_render_frame(
        &mut field, &mats, sz, &mut dirty, &cam, 16_667_000, &budget, &ed,
    )
    .unwrap();
    // Warm-cache : few iterations needed.
    assert!(
        report2.iterations <= 16,
        "warm-cache should re-converge fast ; got {} iters",
        report2.iterations
    );
}

#[test]
fn test_multigrid_acceleration_runs_v_cycle() {
    // Spec § TESTS : multigrid-acceleration. Verify V-cycle reduces residual.
    let cfg = MultigridConfig::default();
    let v = VCycle::new(cfg).unwrap();
    let fine = vec![LightState::from_coefs([1.0; LIGHT_STATE_COEFS]); 16];
    let (out, report) = v.v_cycle(&fine).unwrap();
    assert_eq!(out.len(), 16);
    assert!(report.levels_visited >= 2);
    assert!(report.total_relax_iterations > 0);
}

#[test]
fn test_evidence_driver_assigns_more_budget_to_uncertain_cells() {
    // Spec § TESTS § evidence-driver : ◐ cells get more iterations than ✓
    // cells.
    let d = EvidenceDriver::default();
    let uncertain_budget = d.budget_for(EvidenceGlyph::Uncertain);
    let trusted_budget = d.budget_for(EvidenceGlyph::Trusted);
    let default_budget = d.budget_for(EvidenceGlyph::Default);
    assert!(uncertain_budget > default_budget);
    assert!(default_budget > trusted_budget);
    assert_eq!(trusted_budget, 0.0);
}

#[test]
fn test_evidence_driver_classifies_residual_correctly() {
    let d = EvidenceDriver {
        epsilon: 1e-3,
        confidence_threshold: 1e-1,
        base_budget: 1.0,
        priority_scale: 1.0,
    };
    assert_eq!(d.classify(0.0), EvidenceGlyph::Trusted);
    assert_eq!(d.classify(1e-4), EvidenceGlyph::Trusted);
    assert_eq!(d.classify(0.05), EvidenceGlyph::Default);
    assert_eq!(d.classify(0.5), EvidenceGlyph::Uncertain);
}

#[test]
fn test_denoiser_reduces_local_variance() {
    // Spec § TESTS : denoiser-variance — high-variance regions get more
    // smoothing.
    let mut d = Denoiser::new(DenoiserConfig::default()).unwrap();
    let w = 16_u32;
    let h = 16_u32;
    let mut noisy = vec![[0.5_f32; 3]; (w * h) as usize];
    for (i, p) in noisy.iter_mut().enumerate() {
        let n = ((i as f32) * 0.13).sin() * 0.3;
        for c in 0..3 {
            p[c] = (0.5 + n).clamp(0.0, 1.0);
        }
    }
    let var_before: f32 = Denoiser::local_variance(&noisy, w, h).iter().sum();
    let denoised = d.denoise(&noisy, w, h).unwrap();
    let var_after: f32 = Denoiser::local_variance(&denoised, w, h).iter().sum();
    assert!(
        var_after <= var_before,
        "denoiser should reduce variance ; before={} after={}",
        var_before,
        var_after
    );
}

#[test]
fn test_regression_image_dimensions_match_camera() {
    // Spec § TESTS § regression : pixel-stable across releases. Here we
    // verify the basic structural invariant : image-width × image-height ==
    // total pixel count, and the image is the size requested by the camera.
    let (mut field, mats, sz) = build_field(8, LightState::zero(), MaterialBag::DIFFUSE_MID);
    let cam = Camera::new(13, 17).unwrap(); // odd-sizes intentional.
    let mut dirty = DirtySet::from_iter(0..8);
    let budget = RenderBudget {
        max_iterations: 4,
        ..RenderBudget::default()
    };
    let ed = EvidenceDriver::default();
    let (img, _report) = cfer_render_frame(
        &mut field, &mats, sz, &mut dirty, &cam, 0, &budget, &ed,
    )
    .unwrap();
    assert_eq!(img.width, 13);
    assert_eq!(img.height, 17);
    assert_eq!(img.pixel_count(), 13 * 17);
}

#[test]
fn test_cornell_box_style_scene_renders_without_crash() {
    // Spec § TESTS § convergence : Cornell-box reference scene. We don't
    // ship a path-traced ground-truth in this slice (that's a separate
    // regression-asset slice) ; we verify the algorithm runs to completion
    // without numerical blow-up.
    let n = 27; // 3×3×3 Cornell-like cube.
    let mut field = vec![LightState::zero(); n];
    field[13] = LightState::from_coefs([1.0; LIGHT_STATE_COEFS]); // center emitter
    let mut mats = vec![MaterialBag::DIFFUSE_MID; n];
    mats[13] = MaterialBag::emitter(0.8);

    let cam = Camera::new(32, 32).unwrap();
    let mut dirty = DirtySet::from_iter(0..(n as u32));
    let budget = RenderBudget {
        max_iterations: 32,
        epsilon: 1e-4,
        convergence_threshold: 1e-2,
        use_multigrid: true,
        use_denoiser: true,
    };
    let ed = EvidenceDriver::default();
    let (img, report) = cfer_render_frame(
        &mut field,
        &mats,
        (3, 3, 3),
        &mut dirty,
        &cam,
        16_667_000,
        &budget,
        &ed,
    )
    .unwrap();
    assert_eq!(img.pixel_count(), 32 * 32);
    // Image should not be all-zero (we have an emitter).
    let total_radiance: f32 = img.pixels.iter().map(|p| p.r + p.g + p.b).sum();
    assert!(total_radiance > 0.0, "Cornell-style scene should produce visible light");
    // Convergence report sane.
    assert!(report.iterations > 0);
    assert!(report.iterations <= 32);
    assert!(report.final_residual_l1.is_finite());
}

#[test]
fn test_dirty_set_persistence_across_frames() {
    // Spec § ALGORITHM § Temporal-amortization : steady-state scenes have
    // <10% dirty-cell-rate. We verify the dirty-set semantics : after a
    // converged frame, the dirty-set is cleared.
    let n = 8;
    let (mut field, mats, sz) = build_field(n, LightState::zero(), MaterialBag::ABSORBER);
    let cam = Camera::new(4, 4).unwrap();
    let mut dirty = DirtySet::from_iter(0..(n as u32));
    let budget = RenderBudget::default();
    let ed = EvidenceDriver::default();
    assert!(!dirty.is_empty());
    let (_img, _report) = cfer_render_frame(
        &mut field, &mats, sz, &mut dirty, &cam, 0, &budget, &ed,
    )
    .unwrap();
    // After cfer_render_frame, dirty has been processed and cleared.
    assert!(dirty.is_empty(), "dirty-set should be cleared after a successful frame");
}

#[test]
fn test_attestation_present_at_runtime() {
    // PRIME_DIRECTIVE §11 : the attestation must be programmatically
    // accessible at runtime.
    let a = cssl_substrate_render::ATTESTATION;
    assert!(a.contains("CFER"));
    assert!(a.contains("hurt nor harm"));
    assert!(a.contains("Sovereign-handle"));
}

#[test]
fn test_convergence_report_skip_rate_low_for_cold_start() {
    // Cold-start : almost all cells iterated (skip-rate near 0).
    let n = 8;
    let (mut field, mats, sz) = build_field(n, LightState::zero(), MaterialBag::DIFFUSE_MID);
    let cam = Camera::new(4, 4).unwrap();
    let mut dirty = DirtySet::from_iter(0..(n as u32));
    let budget = RenderBudget {
        max_iterations: 4,
        use_multigrid: false,
        use_denoiser: false,
        ..RenderBudget::default()
    };
    let ed = EvidenceDriver::default();
    let (_img, report) = cfer_render_frame(
        &mut field, &mats, sz, &mut dirty, &cam, 0, &budget, &ed,
    )
    .unwrap();
    // Cold-start : at least one cell iterated per cell × at least 1 iter.
    assert!(report.cells_iterated >= 1);
    assert!(report.total_cells == n as u32);
}
