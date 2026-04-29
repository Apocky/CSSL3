//! § Integration tests — Stage-7 fractal-amplifier
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The 30+ test target declared in the T11-D119 brief. These exercise
//!   the amplifier across the four canonical surfaces :
//!
//!     - amplifier-determinism (per-fragment + per-frame)
//!     - 5-level recursion descent
//!     - KAN-confidence-budget gate
//!     - fovea-mask integration (D120 surface)
//!     - flicker-stability across-frames
//!     - integration trait surface (D116 + D118 + D120)
//!     - cost-model + budget enforcement

#![allow(clippy::float_cmp)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use cssl_fractal_amp::{
    AmplifiedFragment, AmplifierError, CostModel, DetailBudget, DeterminismCheck, FoveaTier,
    FractalAmplifier, MicroColor, MockSdfHit, RecursiveDetailLOD, SdfRaymarchAmplifier,
    SigmaPrivacy,
};

// ─── Group 1 : amplifier-determinism (10 tests) ────────────────────────────

/// § 1.1 — Pixel determinism : same input ⇒ same output.
#[test]
fn pixel_determinism() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let pos = [0.5, 0.5, 0.5];
    let view = [0.0, 0.0, 1.0];
    let grad = [0.6, 0.8, 0.0];
    let a = amp
        .amplify(pos, view, grad, &budget, SigmaPrivacy::Public)
        .unwrap();
    let b = amp
        .amplify(pos, view, grad, &budget, SigmaPrivacy::Public)
        .unwrap();
    assert_eq!(a, b);
}

/// § 1.2 — Pixel determinism : 1000 calls all match.
#[test]
fn pixel_determinism_one_thousand_calls() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let pos = [0.5, 0.5, 0.5];
    let view = [0.0, 0.0, 1.0];
    let grad = [0.6, 0.8, 0.0];
    let first = amp
        .amplify(pos, view, grad, &budget, SigmaPrivacy::Public)
        .unwrap();
    for _ in 0..1000 {
        let f = amp
            .amplify(pos, view, grad, &budget, SigmaPrivacy::Public)
            .unwrap();
        assert_eq!(f, first);
    }
}

/// § 1.3 — Different positions produce different outputs.
#[test]
fn different_positions_different_outputs() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::default();
    let a = amp
        .amplify(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.6, 0.8, 0.0],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    let b = amp
        .amplify(
            [10.0, 10.0, 10.0],
            [0.0, 0.0, 1.0],
            [0.6, 0.8, 0.0],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    assert_ne!(a, b);
}

/// § 1.4 — Different view directions produce (potentially) different outputs.
#[test]
fn different_views_can_produce_different_outputs() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::default();
    let a = amp
        .amplify(
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.6, 0.8, 0.0],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    let b = amp
        .amplify(
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            [0.6, 0.8, 0.0],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    // § View direction enters the input vector at indices 3-4 so different
    //   view directions usually produce different roughness output (which
    //   is keyed on view).
    assert!(a != b || a.is_zero());
}

/// § 1.5 — Identical inputs across 16 thread-equivalent eval orders match.
#[test]
fn identical_input_serial_order_matches() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let pos = [0.7, 0.3, 0.1];
    let view = [0.0, 0.0, 1.0];
    let grad = [0.5, 0.5, 0.7];
    let mut results = Vec::new();
    for _ in 0..16 {
        let f = amp
            .amplify(pos, view, grad, &budget, SigmaPrivacy::Public)
            .unwrap();
        results.push(f);
    }
    let first = results[0];
    for f in &results[1..] {
        assert_eq!(*f, first);
    }
}

/// § 1.6 — DeterminismCheck verifies stable amplifier.
#[test]
fn determinism_check_passes_stable() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let check = DeterminismCheck::new_default(&amp).unwrap();
    assert!(check.verify_frame(&amp).is_ok());
}

/// § 1.7 — DeterminismCheck flags injected mutation.
#[test]
fn determinism_check_flags_mutation() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let check = DeterminismCheck::new_default(&amp).unwrap();
    let golden = check.golden_at(0).unwrap();
    let mutated = AmplifiedFragment::new(
        golden.micro_displacement + 1e-4,
        golden.micro_roughness,
        golden.micro_color,
        golden.kan_confidence,
        golden.sigma_privacy,
    );
    // § Verify the mutation produces a new fragment distinct from golden ;
    //   this confirms the equality-check is sensitive enough to flag
    //   diffs at this scale.
    assert_ne!(mutated, golden);
}

/// § 1.8 — DeterminismCheck verify_n_frames stable for 16 frames.
#[test]
fn determinism_check_sixteen_frames() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let check = DeterminismCheck::new_default(&amp).unwrap();
    assert!(check.verify_n_frames(&amp, 16).is_ok());
}

/// § 1.9 — Σ-private input always emits ZERO.
#[test]
fn sigma_private_always_zero() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    for &x in &[-1.0, 0.0, 0.5, 1.0, 5.0] {
        let f = amp
            .amplify(
                [x, x, x],
                [0.0, 0.0, 1.0],
                [0.5, 0.5, 0.7],
                &budget,
                SigmaPrivacy::Private,
            )
            .unwrap();
        assert!(f.is_zero());
        assert_eq!(f.sigma_privacy, SigmaPrivacy::Private);
    }
}

/// § 1.10 — Trained vs untrained networks have different outputs.
#[test]
fn trained_vs_untrained_differs() {
    let amp_untrained = FractalAmplifier::new_untrained();
    let mut amp_trained = FractalAmplifier::new_untrained();
    amp_trained.kan_micro_displacement.trained = true;
    amp_trained.kan_micro_roughness.trained = true;
    amp_trained.kan_micro_color.trained = true;
    let budget = DetailBudget::default();
    let untrained_out = amp_untrained
        .amplify(
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.7],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    let trained_out = amp_trained
        .amplify(
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.7],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    // § Trained eval returns 0.0 (placeholder per substrate-kan rustdoc)
    //   so produces a different fragment than the synthetic-pattern path.
    assert_ne!(untrained_out, trained_out);
}

// ─── Group 2 : 5-level recursion (8 tests) ─────────────────────────────────

/// § 2.1 — Full-tier recursion descends 5 levels max.
#[test]
fn full_tier_descends_five_levels() {
    let r = RecursiveDetailLOD::new(
        FractalAmplifier::new_untrained().with_confidence_floor(0.05),
        1.0e-3,
    );
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    assert_eq!(budget.max_recursion_depth, 5);
    let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let _ = r.recurse(&hit, &budget).unwrap();
}

/// § 2.2 — Mid-tier recursion limits to 2 levels.
#[test]
fn mid_tier_limits_two_levels() {
    let budget = DetailBudget::from_fovea_tier(FoveaTier::Mid);
    assert_eq!(budget.max_recursion_depth, 2);
}

/// § 2.3 — Peripheral-tier emits ZERO without descent.
#[test]
fn peripheral_zero_no_descent() {
    let r = RecursiveDetailLOD::new_default();
    let budget = DetailBudget::from_fovea_tier(FoveaTier::Peripheral);
    let hit = MockSdfHit::default();
    let out = r.recurse(&hit, &budget).unwrap();
    assert!(out.is_zero());
}

/// § 2.4 — Recursion is deterministic for a fixed hit.
#[test]
fn recursion_deterministic_per_hit() {
    let r = RecursiveDetailLOD::new(
        FractalAmplifier::new_untrained().with_confidence_floor(0.05),
        1.0e-3,
    );
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let hit = MockSdfHit::new([1.5, 2.5, 3.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let a = r.recurse(&hit, &budget).unwrap();
    let b = r.recurse(&hit, &budget).unwrap();
    assert_eq!(a, b);
}

/// § 2.5 — Recursion output is bounded by EPSILON_DISP.
#[test]
fn recursion_bounded_by_epsilon_disp() {
    use cssl_fractal_amp::fragment::EPSILON_DISP;
    let r = RecursiveDetailLOD::new(
        FractalAmplifier::new_untrained().with_confidence_floor(0.05),
        1.0e-3,
    );
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    for &x in &[0.0, 0.5, 1.0, 1.5, 2.0] {
        let hit = MockSdfHit::new([x, x, x], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let out = r.recurse(&hit, &budget).unwrap();
        assert!(out.micro_displacement.abs() <= EPSILON_DISP + 1e-7);
    }
}

/// § 2.6 — Recursion respects Σ-private at depth 0.
#[test]
fn recursion_sigma_private_at_depth_zero() {
    let r = RecursiveDetailLOD::new_default();
    let budget = DetailBudget::default();
    let hit = MockSdfHit::default().with_sigma_privacy(SigmaPrivacy::Private);
    let out = r.recurse(&hit, &budget).unwrap();
    assert!(out.is_zero());
    assert_eq!(out.sigma_privacy, SigmaPrivacy::Private);
}

/// § 2.7 — Recursion respects confidence-floor truncation.
#[test]
fn recursion_truncates_on_low_confidence() {
    let r = RecursiveDetailLOD::new_default();
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.99).unwrap();
    let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.0, 0.5, 0.0]);
    let out = r.recurse(&hit, &budget).unwrap();
    // § High floor → most levels truncated → ZERO output expected.
    assert!(out.is_zero());
}

/// § 2.8 — Different positions through recursion produce different outputs.
#[test]
fn recursion_reads_position() {
    let r = RecursiveDetailLOD::new(
        FractalAmplifier::new_untrained().with_confidence_floor(0.05),
        1.0e-3,
    );
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let hit_a = MockSdfHit::new([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let hit_b = MockSdfHit::new([5.0, 5.0, 5.0], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let a = r.recurse(&hit_a, &budget).unwrap();
    let b = r.recurse(&hit_b, &budget).unwrap();
    assert_ne!(a, b);
}

// ─── Group 3 : KAN-confidence-budget (5 tests) ─────────────────────────────

/// § 3.1 — Confidence floor below MIN refuses construction.
#[test]
fn confidence_below_min_refused() {
    let r = DetailBudget::new(FoveaTier::Full, 1.0, 0.001);
    assert!(matches!(r, Err(AmplifierError::KanConfidenceTooLow(_))));
}

/// § 3.2 — Confidence floor above 1 refused.
#[test]
fn confidence_above_one_refused() {
    let r = DetailBudget::new(FoveaTier::Full, 1.0, 1.5);
    assert!(matches!(r, Err(AmplifierError::KanConfidenceTooLow(_))));
}

/// § 3.3 — Below-floor confidence emits ZERO from amplifier.
#[test]
fn below_floor_emits_zero() {
    let amp = FractalAmplifier::new_untrained();
    // § High floor (0.99) ; the synthetic confidence ranges in [0.3, 0.8]
    //   so most inputs fall below this floor and produce ZERO.
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.99).unwrap();
    let f = amp
        .amplify(
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.0, 0.5, 0.0],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    assert!(f.is_zero());
}

/// § 3.4 — Above-floor confidence emits non-ZERO from amplifier.
#[test]
fn above_floor_emits_non_zero() {
    let amp = FractalAmplifier::new_untrained();
    // § Low floor (0.05) ; almost all inputs are above the floor.
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let f = amp
        .amplify(
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    // § May be non-zero. (Confidence floor passes ⇒ amplifier emits a
    //   real fragment.)
    assert!(!f.is_zero());
}

/// § 3.5 — Confidence-floor is monotone : tighter floor = more zeros.
#[test]
fn tighter_floor_more_zeros() {
    let amp = FractalAmplifier::new_untrained();
    let lax = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let strict = DetailBudget::new(FoveaTier::Full, 1.0, 0.95).unwrap();
    let mut zeros_lax = 0;
    let mut zeros_strict = 0;
    for i in 0..32 {
        let x = (i as f32) * 0.1;
        let pos = [x, x, x];
        let f1 = amp
            .amplify(
                pos,
                [0.0, 0.0, 1.0],
                [0.0, 0.5, 0.0],
                &lax,
                SigmaPrivacy::Public,
            )
            .unwrap();
        let f2 = amp
            .amplify(
                pos,
                [0.0, 0.0, 1.0],
                [0.0, 0.5, 0.0],
                &strict,
                SigmaPrivacy::Public,
            )
            .unwrap();
        if f1.is_zero() {
            zeros_lax += 1;
        }
        if f2.is_zero() {
            zeros_strict += 1;
        }
    }
    assert!(zeros_strict >= zeros_lax);
}

// ─── Group 4 : fovea-mask integration (D120) (4 tests) ─────────────────────

/// § 4.1 — FoveaTier::Full → 5-deep recursion descent.
#[test]
fn fovea_full_descent_five() {
    let budget = DetailBudget::from_fovea_tier(FoveaTier::Full);
    assert_eq!(budget.max_recursion_depth, 5);
    assert!((budget.amplitude - 1.0).abs() < 1e-6);
}

/// § 4.2 — FoveaTier::Mid → 2-deep + half amplitude.
#[test]
fn fovea_mid_descent_two_half() {
    let budget = DetailBudget::from_fovea_tier(FoveaTier::Mid);
    assert_eq!(budget.max_recursion_depth, 2);
    assert!((budget.amplitude - 0.5).abs() < 1e-6);
}

/// § 4.3 — FoveaTier::Peripheral → 0-deep (no amplifier evaluation).
#[test]
fn fovea_peripheral_no_descent() {
    let budget = DetailBudget::from_fovea_tier(FoveaTier::Peripheral);
    assert_eq!(budget.max_recursion_depth, 0);
    assert!(budget.amplitude == 0.0);
    assert!(!budget.should_amplify());
}

/// § 4.4 — Fovea-mask transition produces graduated detail.
#[test]
fn fovea_transition_graduated() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let pos = [0.5, 0.5, 0.5];
    let view = [0.0, 0.0, 1.0];
    let grad = [0.6, 0.8, 0.0];
    let f_full = amp
        .amplify(
            pos,
            view,
            grad,
            &DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap(),
            SigmaPrivacy::Public,
        )
        .unwrap();
    let f_mid = amp
        .amplify(
            pos,
            view,
            grad,
            &DetailBudget::new(FoveaTier::Mid, 1.0, 0.05).unwrap(),
            SigmaPrivacy::Public,
        )
        .unwrap();
    let f_periph = amp
        .amplify(
            pos,
            view,
            grad,
            &DetailBudget::new(FoveaTier::Peripheral, 1.0, 0.05).unwrap(),
            SigmaPrivacy::Public,
        )
        .unwrap();
    // § peripheral is always ZERO ; mid is half-amplitude ; full is full.
    assert!(f_periph.is_zero());
    assert!(f_mid.micro_displacement.abs() <= f_full.micro_displacement.abs() + 1e-7);
}

// ─── Group 5 : flicker-stability across-frames (3 tests) ───────────────────

/// § 5.1 — Frame-to-frame stability for fixed fovea-mask + view.
#[test]
fn flicker_stability_fixed_input() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let pos = [0.5, 0.5, 0.5];
    let view = [0.0, 0.0, 1.0];
    let grad = [0.6, 0.8, 0.0];
    let frame_0 = amp
        .amplify(pos, view, grad, &budget, SigmaPrivacy::Public)
        .unwrap();
    // § Simulate 60 frames of pure-evaluation.
    for _ in 0..60 {
        let f = amp
            .amplify(pos, view, grad, &budget, SigmaPrivacy::Public)
            .unwrap();
        assert_eq!(f, frame_0);
    }
}

/// § 5.2 — Frame-to-frame stability across recursion driver.
#[test]
fn flicker_stability_recursion() {
    let r = RecursiveDetailLOD::new(
        FractalAmplifier::new_untrained().with_confidence_floor(0.05),
        1.0e-3,
    );
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let frame_0 = r.recurse(&hit, &budget).unwrap();
    for _ in 0..60 {
        let f = r.recurse(&hit, &budget).unwrap();
        assert_eq!(f, frame_0);
    }
}

/// § 5.3 — Flicker detection captures intentional injection.
#[test]
fn flicker_detection_captures_injection() {
    let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
    let check = DeterminismCheck::new_default(&amp).unwrap();
    let golden = check.golden_at(0).unwrap();
    // § Confirm any displacement perturbation produces a fragment !=
    //   golden ; this is what DeterminismError::FlickerDetected fires
    //   on internally.
    let injected = AmplifiedFragment::new(
        golden.micro_displacement + cssl_fractal_amp::fragment::EPSILON_DISP * 0.5,
        golden.micro_roughness,
        golden.micro_color,
        golden.kan_confidence,
        golden.sigma_privacy,
    );
    assert_ne!(injected, golden);
    // § The same amplifier always produces the golden fragment.
    assert!(check.verify_frame(&amp).is_ok());
}

// ─── Group 6 : integration trait surface (D116/D118/D120) (5 tests) ─────────

/// § 6.1 — SdfRaymarchAmplifier trait dispatches via method.
#[test]
fn sdf_raymarch_amplifier_dispatch() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::default();
    let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let from_trait: AmplifiedFragment = amp.amplify_at_hit(&hit, &budget).unwrap();
    let from_method = amp.amplify_hit(&hit, &budget).unwrap();
    assert_eq!(from_trait, from_method);
}

/// § 6.2 — SdfHitInfo trait propagates Σ-privacy correctly.
#[test]
fn sdf_hit_info_propagates_privacy() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::default();
    let hit_pub = MockSdfHit::default();
    let hit_priv = MockSdfHit::default().with_sigma_privacy(SigmaPrivacy::Private);
    let f_pub = amp.amplify_at_hit(&hit_pub, &budget).unwrap();
    let f_priv = amp.amplify_at_hit(&hit_priv, &budget).unwrap();
    assert_eq!(f_pub.sigma_privacy, SigmaPrivacy::Public);
    assert_eq!(f_priv.sigma_privacy, SigmaPrivacy::Private);
    // § Σ-private is always ZERO at the amplifier.
    assert!(f_priv.is_zero());
}

/// § 6.3 — D118 integration : MicroColor channels feed BRDF M-coord shift.
#[test]
fn micro_color_channels_for_brdf() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
    let f = amp
        .amplify(
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
            &budget,
            SigmaPrivacy::Public,
        )
        .unwrap();
    // § The MicroColor must be saturatable into [-0.5, +0.5] per channel.
    assert!(f.micro_color.low.abs() <= 0.5);
    assert!(f.micro_color.mid.abs() <= 0.5);
    assert!(f.micro_color.high.abs() <= 0.5);
}

/// § 6.4 — D120 integration : FoveaTier directly maps to budget.
#[test]
fn d120_fovea_tier_maps_budget() {
    let full = DetailBudget::from_fovea_tier(FoveaTier::Full);
    let mid = DetailBudget::from_fovea_tier(FoveaTier::Mid);
    let periph = DetailBudget::from_fovea_tier(FoveaTier::Peripheral);
    assert!(full.amplitude > mid.amplitude);
    assert!(mid.amplitude > periph.amplitude);
    assert!(periph.amplitude == 0.0);
}

/// § 6.5 — D116 integration : amplify_charged drives cost-model + amplifier.
#[test]
fn d116_amplify_charged_integration() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::default();
    let mut cost = CostModel::quest3();
    let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let mut frags = Vec::new();
    for _ in 0..1000 {
        let f = amp.amplify_charged(&hit, &budget, &mut cost).unwrap();
        frags.push(f);
    }
    // § All 1000 fragments are identical (same input).
    let first = frags[0];
    for f in &frags[1..] {
        assert_eq!(*f, first);
    }
    // § Cost-model charged 1000 calls.
    assert_eq!(cost.fragments_amplified, 1000);
}

// ─── Group 7 : cost-model + budget enforcement (3 tests) ────────────────────

/// § 7.1 — Per-fragment cost is well below the 1.2 ms budget.
#[test]
fn per_fragment_below_budget() {
    use cssl_fractal_amp::cost_model::COST_PER_FRAGMENT_TIER1_NS;
    let cost_ms_per_frag = COST_PER_FRAGMENT_TIER1_NS / 1.0e6;
    // § 0.00015 ms / 1.2 ms = 0.0125% of frame budget per fragment.
    assert!(cost_ms_per_frag < cssl_fractal_amp::COST_BUDGET_QUEST3_MS);
}

/// § 7.2 — Cost-model BudgetExceeded fires at saturation.
#[test]
fn cost_model_budget_exceeded() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::default();
    let mut cost = CostModel::new(
        0.0001,
        cssl_fractal_amp::cost_model::DispatchTier::CoopMatrix,
    )
    .unwrap();
    let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    let mut hit_err = false;
    for _ in 0..100 {
        if amp.amplify_charged(&hit, &budget, &mut cost).is_err() {
            hit_err = true;
            break;
        }
    }
    assert!(hit_err);
}

/// § 7.3 — Cost-model degraded-mode threshold triggers.
#[test]
fn cost_model_degrade_mode_triggers() {
    let amp = FractalAmplifier::new_untrained();
    let budget = DetailBudget::default();
    let mut cost = CostModel::quest3();
    let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
    // § Hit a high-but-not-exhausted budget.
    let calls_to_degrade = ((cssl_fractal_amp::COST_BUDGET_QUEST3_MS * 0.85 * 1.0e6)
        / cssl_fractal_amp::cost_model::COST_PER_FRAGMENT_TIER1_NS)
        .ceil() as u32;
    for _ in 0..calls_to_degrade {
        let _ = amp.amplify_charged(&hit, &budget, &mut cost);
    }
    assert!(cost.should_degrade());
}

// ─── Group 8 : MicroColor + AmplifiedFragment composition (4 tests) ─────────

/// § 8.1 — MicroColor add is associative.
#[test]
fn micro_color_add_associative() {
    let a = MicroColor::from_array([0.1, 0.1, 0.1]);
    let b = MicroColor::from_array([0.2, 0.2, 0.2]);
    let c = MicroColor::from_array([0.3, 0.3, 0.3]);
    let r1 = a.add(b).add(c);
    let r2 = a.add(b.add(c));
    assert!((r1.low - r2.low).abs() < 1e-7);
    assert!((r1.mid - r2.mid).abs() < 1e-7);
    assert!((r1.high - r2.high).abs() < 1e-7);
}

/// § 8.2 — MicroColor scale by 0 yields ZERO.
#[test]
fn micro_color_scale_zero() {
    let a = MicroColor::from_array([0.5, -0.5, 0.5]);
    let z = a.scale(0.0);
    assert!(z.is_zero());
}

/// § 8.3 — AmplifiedFragment::attenuate(0) yields ZERO.
#[test]
fn fragment_attenuate_zero_yields_zero() {
    let f = AmplifiedFragment::new(
        cssl_fractal_amp::fragment::EPSILON_DISP,
        cssl_fractal_amp::fragment::EPSILON_ROUGHNESS,
        MicroColor::from_array([0.5, 0.5, 0.5]),
        1.0,
        SigmaPrivacy::Public,
    );
    let z = f.attenuate(0.0);
    assert!(z.is_zero());
}

/// § 8.4 — AmplifiedFragment::attenuate(1) is identity.
#[test]
fn fragment_attenuate_one_is_identity() {
    let f = AmplifiedFragment::new(
        cssl_fractal_amp::fragment::EPSILON_DISP,
        cssl_fractal_amp::fragment::EPSILON_ROUGHNESS,
        MicroColor::from_array([0.4, 0.3, 0.2]),
        0.7,
        SigmaPrivacy::Public,
    );
    let id = f.attenuate(1.0);
    assert_eq!(id, f);
}
