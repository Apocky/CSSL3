//! § Integration tests : mise_en_abyme — Stage-9 recursive-witness rendering
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Test-suite covering :
//!     - recursion-bounded behaviour (HARD cap is enforced)
//!     - KAN-attenuation correctness (energy decay, no runaway)
//!     - budget-respected (cost-model + frame-stats)
//!     - Companion-eye mutual-witness (path-V.5 composition)
//!     - mirror-corridor doesn't-runaway (the load-bearing safety property)
//!     - anti-surveillance (cross-region mirror is blocked)
//!     - Σ-Sovereign creature-eye consent gate

#![allow(clippy::field_reassign_with_default)]
// Test fixtures often start with
// Default + tweak ; the struct-literal
// form would force naming all defaults.
#![allow(clippy::float_cmp)]
// Exact equality is intentional in energy-conservation
// assertions where the spec demands ZERO output (no
// FP-drift at the consent-redaction gate).
#![allow(clippy::needless_update)]
#![allow(clippy::too_many_arguments)]

use cssl_render_v2::mise_en_abyme::companion::ConstantSemanticFrameProvider;
use cssl_render_v2::mise_en_abyme::confidence::{
    KanConfidence, KanConfidenceInputs, MIN_CONFIDENCE,
};
use cssl_render_v2::mise_en_abyme::cost::{MiseEnAbymeCostModel, RuntimePlatform};
use cssl_render_v2::mise_en_abyme::mirror::{
    MirrorDetectionThreshold, MirrorSurface, MirrornessChannel,
};
use cssl_render_v2::mise_en_abyme::pass::{
    MiseEnAbymePass, MiseEnAbymePassConfig, RecursionDepthBudget,
};
use cssl_render_v2::mise_en_abyme::probe::{ConstantProbe, FixedHit};
use cssl_render_v2::mise_en_abyme::radiance::MiseEnAbymeRadiance;
use cssl_render_v2::mise_en_abyme::region::{RegionBoundary, RegionId, RegionPolicy};
use cssl_render_v2::mise_en_abyme::{
    CompanionEyeWitness, RECURSION_DEPTH_HARD_CAP, STAGE9_BUDGET_QUEST3_US,
};

use cssl_substrate_kan::{KanMaterial, EMBEDDING_DIM};
use cssl_substrate_projections::vec::Vec3;

// ─────────────────────────────────────────────────────────────────────────
// § Test fixtures
// ─────────────────────────────────────────────────────────────────────────

fn mirror_material(mirrorness: f32) -> KanMaterial {
    let mut emb = [0.0_f32; EMBEDDING_DIM];
    emb[7] = mirrorness;
    KanMaterial::creature_morphology(emb)
}

fn planar_mirror(mirrorness: f32, region: RegionId) -> MirrorSurface {
    MirrorSurface {
        position: Vec3::ZERO,
        normal: Vec3::Y,
        mirrorness,
        roughness: 1.0 - mirrorness,
        region_id: region,
        curvature: 0.0,
    }
}

fn fixture_hit(mirrorness: f32, region: RegionId, atmosphere: f32) -> FixedHit {
    FixedHit {
        position: Vec3::new(0.0, 1.0, 0.0),
        gradient: Vec3::Y,
        curvature: 0.05,
        material: mirror_material(mirrorness),
        region_id: region,
        atmosphere,
    }
}

fn cornea_pass_config(threshold: MirrorDetectionThreshold) -> MiseEnAbymePassConfig {
    let mut cfg = MiseEnAbymePassConfig::default();
    cfg.mirrorness_channel = MirrornessChannel::CorneaAxis7;
    cfg.threshold = threshold;
    cfg
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : recursion-bounded behaviour
// ─────────────────────────────────────────────────────────────────────────

/// § The recursion HARD cap is exactly 5, per the dispatch ticket.
#[test]
fn integration_recursion_hard_cap_is_five() {
    assert_eq!(RECURSION_DEPTH_HARD_CAP, 5);
}

/// § RecursionDepthBudget refuses to advance past HARD_CAP.
#[test]
fn integration_recursion_budget_refuses_past_hard_cap() {
    let mut b = RecursionDepthBudget::new();
    for _ in 0..RECURSION_DEPTH_HARD_CAP {
        b = b.try_advance().unwrap();
    }
    assert!(b.try_advance().is_err());
}

/// § A mirror corridor with always-mirror probe terminates at HARD_CAP
///   OR via KAN-confidence — never runs away.
#[test]
fn integration_mirror_corridor_doesnt_runaway() {
    let region = RegionId(7);
    let mut p = MiseEnAbymePass::new(cornea_pass_config(MirrorDetectionThreshold::default()));
    let mirror = planar_mirror(0.95, region);
    let probe = ConstantProbe::always_hit(fixture_hit(0.95, region, 0.0));
    let base = MiseEnAbymeRadiance::splat(1.0);
    p.begin_frame();
    let r = p
        .recurse_at_mirror(
            mirror,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe,
            None,
            None,
        )
        .unwrap();
    let stats = p.end_frame();
    // § Energy bounded by linear envelope :
    //   sum_d=0^HARD_CAP attenuation(d) ≤ sum_d=0^5 0.7^d ≈ 2.5
    //   then × base.total_energy().
    assert!(r.total_energy() < base.total_energy() * 5.0);
    // § Some pixel terminated (either KAN or hard-cap).
    assert!(stats.kan_terminate_pixels + stats.hard_cap_pixels >= 1);
}

/// § max_depth = 0 still does no recursion (0 means "primary only").
#[test]
fn integration_max_depth_zero_no_recursion() {
    let mut cfg = cornea_pass_config(MirrorDetectionThreshold::default());
    cfg.max_depth = 0;
    let mut p = MiseEnAbymePass::new(cfg);
    let mirror = planar_mirror(0.95, RegionId(7));
    let probe = ConstantProbe::always_hit(fixture_hit(0.95, RegionId(7), 0.0));
    let base = MiseEnAbymeRadiance::splat(1.0);
    p.begin_frame();
    let _ = p.recurse_at_mirror(
        mirror,
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
        &base,
        &probe,
        None,
        None,
    );
    let stats = p.end_frame();
    // § max_depth=0 still processes the primary mirror (one bounce) but does
    //   not advance.
    assert!(stats.bounces <= 1);
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : KAN-attenuation correctness
// ─────────────────────────────────────────────────────────────────────────

/// § Attenuation decays monotonically with depth.
#[test]
fn integration_kan_attenuation_decays_with_depth() {
    let k = KanConfidence::analytic_default();
    let mut last = 1.0_f32;
    for d in 0..=RECURSION_DEPTH_HARD_CAP {
        let a = k
            .evaluate(KanConfidenceInputs::new(d, 0.0, 0.0))
            .attenuation;
        assert!(a <= last + 1e-5);
        last = a;
    }
}

/// § Attenuation is bounded in [0, 1] for all valid inputs.
#[test]
fn integration_kan_attenuation_bounded_unit_interval() {
    let k = KanConfidence::analytic_default();
    for d in 0..=RECURSION_DEPTH_HARD_CAP {
        for ri in 0..=10 {
            for ai in 0..=10 {
                let r = ri as f32 / 10.0;
                let a = ai as f32 / 10.0;
                let o = k.evaluate(KanConfidenceInputs::new(d, r, a));
                assert!((0.0..=1.0).contains(&o.attenuation));
            }
        }
    }
}

/// § Below MIN_CONFIDENCE, recursion truncates (should_continue = false).
#[test]
fn integration_kan_below_min_confidence_truncates() {
    let k = KanConfidence::analytic_default().with_base_falloff(0.3);
    let o = k.evaluate(KanConfidenceInputs::new(3, 0.5, 0.5));
    if o.attenuation < MIN_CONFIDENCE {
        assert!(!o.should_continue);
    }
}

/// § Energy-conservation : MiseEnAbymeRadiance accumulates within a linear
///   envelope. (Tests the safety property the spec says is "load-bearing"
///   for "no-runaway-light-amplification".)
#[test]
fn integration_energy_conservation_no_amplification() {
    let mut acc = MiseEnAbymeRadiance::ZERO;
    let contribution = MiseEnAbymeRadiance::splat(1.0);
    let total_attenuation = 0.7 + 0.7 * 0.7 + 0.7 * 0.7 * 0.7; // 1.813
    acc.accumulate(0.7, &contribution);
    acc.accumulate(0.49, &contribution);
    acc.accumulate(0.343, &contribution);
    let envelope = total_attenuation * contribution.total_energy();
    assert!(acc.total_energy() < envelope + 1e-2);
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : budget-respected
// ─────────────────────────────────────────────────────────────────────────

/// § Quest-3 stage-9 budget is 0.8ms = 800us per spec.
#[test]
fn integration_stage9_budget_quest3_is_800us() {
    assert_eq!(STAGE9_BUDGET_QUEST3_US, 800);
    assert_eq!(RuntimePlatform::Quest3.budget_us(), 800);
}

/// § Cost-model fits a moderate workload.
#[test]
fn integration_cost_model_moderate_workload_fits_quest3() {
    let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
    // § A typical scene has ~1000 mirror-touched pixels at depth 3.
    assert!(m.fits_in_budget(1000, 3));
}

/// § Cost-model exhausts on absurd workload.
#[test]
fn integration_cost_model_absurd_workload_exhausts() {
    let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
    assert!(!m.fits_in_budget(1_000_000_000, 5));
}

/// § Cost-model suggest_depth gracefully decreases under load.
#[test]
fn integration_cost_model_suggest_depth_decreases_under_load() {
    let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
    let d_light = m.suggest_depth(10);
    let d_heavy = m.suggest_depth(100_000);
    assert!(d_light >= d_heavy);
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : Companion-eye mutual-witness
// ─────────────────────────────────────────────────────────────────────────

/// § Companion-eye reflection composes in the path-V.5 semantic frame
///   when consent + presence are granted.
#[test]
fn integration_companion_eye_mutual_witness_consenting() {
    let region = RegionId(2);
    let semantic_frame = MiseEnAbymeRadiance::splat(0.7);
    let provider = ConstantSemanticFrameProvider {
        frame: Some(semantic_frame),
    };
    let companion = CompanionEyeWitness::new(11, true, true);

    let mut p = MiseEnAbymePass::new(cornea_pass_config(MirrorDetectionThreshold::default()));
    let cornea = planar_mirror(0.85, region);
    let probe = ConstantProbe::always_miss();
    let base = MiseEnAbymeRadiance::ZERO;
    p.begin_frame();
    let r = p
        .recurse_at_mirror(
            cornea,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe,
            Some(&companion),
            Some(&provider),
        )
        .unwrap();
    // § The reflection should carry a non-zero contribution from the
    //   semantic frame.
    assert!(r.total_energy() > 0.0);
}

/// § Companion-eye reflection is REDACTED when consent is declined ;
///   the eye-of-the-sovereign closes.
#[test]
fn integration_companion_eye_redacted_when_decline() {
    let region = RegionId(2);
    let provider = ConstantSemanticFrameProvider {
        frame: Some(MiseEnAbymeRadiance::splat(0.9)),
    };
    let companion = CompanionEyeWitness::new(11, false, true); // declined

    let mut p = MiseEnAbymePass::new(cornea_pass_config(MirrorDetectionThreshold::default()));
    let cornea = planar_mirror(0.85, region);
    let probe = ConstantProbe::always_miss();
    let base = MiseEnAbymeRadiance::ZERO;
    p.begin_frame();
    let r = p
        .recurse_at_mirror(
            cornea,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe,
            Some(&companion),
            Some(&provider),
        )
        .unwrap();
    let stats = p.end_frame();
    // § The reflection is empty.
    assert_eq!(r.total_energy(), 0.0);
    // § The eye-redaction was logged.
    assert_eq!(stats.eye_redactions, 1);
}

/// § Companion-eye is REDACTED when the companion is absent from the region.
#[test]
fn integration_companion_eye_redacted_when_absent() {
    let region = RegionId(2);
    let provider = ConstantSemanticFrameProvider {
        frame: Some(MiseEnAbymeRadiance::splat(0.9)),
    };
    let companion = CompanionEyeWitness::new(11, true, false); // absent

    let mut p = MiseEnAbymePass::new(cornea_pass_config(MirrorDetectionThreshold::default()));
    let cornea = planar_mirror(0.85, region);
    let probe = ConstantProbe::always_miss();
    let base = MiseEnAbymeRadiance::ZERO;
    p.begin_frame();
    let r = p
        .recurse_at_mirror(
            cornea,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe,
            Some(&companion),
            Some(&provider),
        )
        .unwrap();
    let stats = p.end_frame();
    assert_eq!(r.total_energy(), 0.0);
    assert_eq!(stats.eye_redactions, 1);
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : anti-surveillance (cross-region blocked)
// ─────────────────────────────────────────────────────────────────────────

/// § Cross-region mirror chaining is blocked under SameRegionOnly policy.
#[test]
fn integration_cross_region_mirror_blocked() {
    let mut cfg = cornea_pass_config(MirrorDetectionThreshold::default());
    cfg.region_boundary = RegionBoundary::from_policy(RegionPolicy::SameRegionOnly);
    let mut p = MiseEnAbymePass::new(cfg);
    let mirror_in_a = planar_mirror(0.95, RegionId(7));
    // The probe returns hits in a DIFFERENT region — this should be blocked.
    let cross_hit = fixture_hit(0.95, RegionId(8), 0.0);
    let probe = ConstantProbe::always_hit(cross_hit);
    let base = MiseEnAbymeRadiance::splat(1.0);
    p.begin_frame();
    let _ = p.recurse_at_mirror(
        mirror_in_a,
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
        &base,
        &probe,
        None,
        None,
    );
    let stats = p.end_frame();
    // § At least one surveillance block fired.
    assert!(stats.surveillance_blocks >= 1);
}

/// § Cross-region mirror chaining IS permitted under SameRegionOrPublic
///   policy when destination is PUBLIC.
#[test]
fn integration_public_destination_permitted() {
    let mut cfg = cornea_pass_config(MirrorDetectionThreshold::default());
    cfg.region_boundary = RegionBoundary::from_policy(RegionPolicy::SameRegionOrPublic);
    let mut p = MiseEnAbymePass::new(cfg);
    let mirror_in_private = planar_mirror(0.95, RegionId(7));
    let public_hit = fixture_hit(0.95, RegionId::PUBLIC, 0.0);
    let probe = ConstantProbe::always_hit(public_hit);
    let base = MiseEnAbymeRadiance::splat(1.0);
    p.begin_frame();
    let _ = p.recurse_at_mirror(
        mirror_in_private,
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
        &base,
        &probe,
        None,
        None,
    );
    let stats = p.end_frame();
    // § No surveillance block — PUBLIC destination is allowed.
    assert_eq!(stats.surveillance_blocks, 0);
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : MiseEnAbymeRadiance correctness
// ─────────────────────────────────────────────────────────────────────────

/// § The spec's `MiseEnAbymeRadiance<2, 16>` shape is honored.
#[test]
fn integration_radiance_shape_2x16() {
    let r = MiseEnAbymeRadiance::ZERO;
    assert_eq!(r.bands.len(), 2);
    assert_eq!(r.bands[0].len(), 16);
    assert_eq!(core::mem::size_of::<MiseEnAbymeRadiance>(), 2 * 16 * 4);
}

/// § Radiance::splat fills all entries.
#[test]
fn integration_radiance_splat_fills_all() {
    let r = MiseEnAbymeRadiance::splat(0.5);
    for eye in 0..2 {
        for band in 0..16 {
            assert!((r.bands[eye][band] - 0.5).abs() < 1e-6);
        }
    }
}

/// § Radiance::is_finite catches NaN.
#[test]
fn integration_radiance_is_finite_catches_nan() {
    let mut r = MiseEnAbymeRadiance::ZERO;
    assert!(r.is_finite());
    r.bands[1][3] = f32::NAN;
    assert!(!r.is_finite());
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : MirrorSurface reflect direction
// ─────────────────────────────────────────────────────────────────────────

/// § Mirror-floor reflects upward incidence into upward outgoing.
///   (Ray going down +Y direction at a +Y normal mirror reflects to -Y.)
#[test]
fn integration_mirror_floor_reflection_law() {
    let m = planar_mirror(1.0, RegionId(0));
    // § Ray heading directly into the mirror normal → reflects to its
    //   opposite.
    let r = m.reflect_direction(Vec3::Y);
    assert!((r.y - (-1.0)).abs() < 1e-5);
}

/// § A perfect mirror at the world origin reflects camera at (0,5,0)
///   into (0,-5,0).
#[test]
fn integration_mirror_position_reflection() {
    let m = planar_mirror(1.0, RegionId(0));
    let p = m.reflect_position(Vec3::new(0.0, 5.0, 0.0));
    assert!((p.y - (-5.0)).abs() < 1e-5);
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests : End-to-end small-scene integration
// ─────────────────────────────────────────────────────────────────────────

/// § Smoke-test : run a full frame with a single planar mirror, no
///   recursion (probe always misses). Verify per-frame stats are
///   sensible. On a miss the atmospheric extinction zeroes out the
///   bounce attenuation by spec design — the recursion bounces 1 ray
///   then naturally terminates with zero energy from atmospheric loss.
#[test]
fn integration_e2e_single_mirror_miss() {
    let mut p = MiseEnAbymePass::substrate_default();
    let mirror = planar_mirror(0.9, RegionId(7));
    let probe = ConstantProbe::always_miss();
    let base = MiseEnAbymeRadiance::splat(0.5);
    p.begin_frame();
    let r = p
        .recurse_at_mirror(
            mirror,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe,
            None,
            None,
        )
        .unwrap();
    let stats = p.end_frame();
    // § Result is finite (no NaN/inf — energy-conservation post-condition).
    assert!(r.is_finite());
    // § Exactly one bounce was processed.
    assert_eq!(stats.bounces, 1);
    // § Total energy is bounded (atmospheric loss is the spec's
    //   designed termination — see `miss_probe_returns_zero_with_full
    //   _attenuation_loss` for the load-bearing assertion).
    assert!(r.total_energy() <= base.total_energy());
}

/// § Smoke-test : full frame with companion-eye consenting. Verify the
///   semantic frame contributes to the output.
#[test]
fn integration_e2e_companion_eye_consenting() {
    let region = RegionId(2);
    let semantic_frame = MiseEnAbymeRadiance::splat(0.6);
    let provider = ConstantSemanticFrameProvider {
        frame: Some(semantic_frame),
    };
    let companion = CompanionEyeWitness::new(11, true, true);

    let mut cfg = cornea_pass_config(MirrorDetectionThreshold::default());
    cfg.mirrorness_channel = MirrornessChannel::CorneaAxis7;
    let mut p = MiseEnAbymePass::new(cfg);
    let cornea = planar_mirror(0.7, region);
    let probe = ConstantProbe::always_miss();
    let base = MiseEnAbymeRadiance::ZERO;
    p.begin_frame();
    let r = p
        .recurse_at_mirror(
            cornea,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe,
            Some(&companion),
            Some(&provider),
        )
        .unwrap();
    let _ = p.end_frame();
    assert!(r.total_energy() > 0.0);
}

/// § Reproducibility : the same inputs produce the same output across
///   two independent passes (deterministic recursion).
#[test]
fn integration_reproducibility_deterministic() {
    let mut p1 = MiseEnAbymePass::new(cornea_pass_config(MirrorDetectionThreshold::default()));
    let mut p2 = MiseEnAbymePass::new(cornea_pass_config(MirrorDetectionThreshold::default()));
    let mirror = planar_mirror(0.9, RegionId(7));
    let hit = fixture_hit(0.9, RegionId(7), 0.0);
    let probe1 = ConstantProbe::always_hit(hit.clone());
    let probe2 = ConstantProbe::always_hit(hit);
    let base = MiseEnAbymeRadiance::splat(1.0);

    p1.begin_frame();
    let r1 = p1
        .recurse_at_mirror(
            mirror,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe1,
            None,
            None,
        )
        .unwrap();
    p1.end_frame();

    p2.begin_frame();
    let r2 = p2
        .recurse_at_mirror(
            mirror,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            &base,
            &probe2,
            None,
            None,
        )
        .unwrap();
    p2.end_frame();

    assert!(r1.approx_eq(&r2, 1e-5));
}
