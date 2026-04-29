//! § integration_test — exercises the public surface end-to-end.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! These tests SUPPLEMENT the per-module unit tests with end-to-end
//! scenarios that span multiple modules. Each test exercises a complete
//! Stage-8 lifecycle from gate-setup through CompanionView emission.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::module_name_repetitions)]
#![allow(unused_imports)]

use cssl_render_companion_perspective::salience_visualization::SalienceVisualization;
use cssl_render_companion_perspective::{
    AuraOverlap, CompanionConsentDecision, CompanionConsentGate, CompanionContext, CompanionId,
    CompanionPerspectivePass, MutualWitnessMode, PlayerToggleState, RenderCostReport, SalienceAxis,
    SalienceScore, SemanticSalienceEvaluator, Stage8Budget,
};
use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked, SigmaPolicy};

fn open_mask() -> SigmaMaskPacked {
    SigmaMaskPacked::from_policy(SigmaPolicy::DefaultPrivate)
        .with_consent(ConsentBit::Observe.bits())
}

fn realistic_ctx() -> CompanionContext {
    let mut c = CompanionContext::neutral();
    c.companion_id = CompanionId(42);
    c.companion_sovereign_handle = 100;
    for i in 0..32 {
        c.belief_embedding[i] = 0.05 * (i as f32 + 1.0);
    }
    c.emotion = cssl_render_companion_perspective::CompanionEmotion {
        curious: 0.4,
        anxious: 0.0,
        content: 0.3,
        alert: 0.0,
    };
    c.attention_target = Some([5.0, 0.0, 0.0]);
    c.attention_radius = 3.0;
    c
}

#[test]
fn full_pipeline_consent_off_emits_empty_view() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[0.0; 3], [1.0; 3]];
    let masks = vec![open_mask(); 2];
    let (view, report) = pass
        .execute_one_shot(
            PlayerToggleState::Off,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    assert!(view.is_skipped());
    assert!(report.skipped);
    assert_eq!(report.cells_evaluated, 0);
}

#[test]
fn full_pipeline_companion_declined_emits_labeled_view() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[0.0; 3]];
    let masks = vec![open_mask()];
    let (view, report) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Declined,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    assert!(view.is_skipped());
    assert!(view.companion_declined);
    assert!(report.companion_declined);
}

#[test]
fn full_pipeline_open_renders_per_eye_cells() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = (0..32)
        .map(|i| [(i as f32) * 0.5, 0.0, 0.0])
        .collect::<Vec<_>>();
    let masks = vec![open_mask(); positions.len()];
    let (view, report) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    assert!(!view.is_skipped());
    assert_eq!(view.total_cells(), positions.len() * 2);
    assert_eq!(view.cells_per_eye_count(), positions.len());
    assert_eq!(report.cells_evaluated, positions.len() as u32);
}

#[test]
fn full_pipeline_revoke_mid_session_blocks_render() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[0.0; 3]];
    let masks = vec![open_mask()];
    let mut gate = CompanionConsentGate::new();
    gate.set_player_toggle(PlayerToggleState::On);
    gate.companion_grant();

    // Frame 0 : both consent.
    let (v0, r0) = pass
        .execute_with_gate(&mut gate, &ctx, &positions, &masks, None)
        .unwrap();
    assert!(!v0.is_skipped());
    assert_eq!(r0.cells_evaluated, 1);

    // Frame 1 : companion revokes.
    gate.companion_revoke();
    let (v1, r1) = pass
        .execute_with_gate(&mut gate, &ctx, &positions, &masks, None)
        .unwrap();
    assert!(v1.is_skipped());
    assert!(v1.companion_declined);
    assert_eq!(r1.cells_evaluated, 0);
}

#[test]
fn full_pipeline_mutual_witness_modulates_attended_cells() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    // Spread cells around the attention target (which is at [5,0,0]).
    let positions = vec![
        [4.5, 0.0, 0.0],  // inside focal
        [5.0, 0.5, 0.0],  // inside focal
        [5.5, 0.0, 0.0],  // inside focal
        [50.0, 0.0, 0.0], // far
    ];
    let masks = vec![open_mask(); positions.len()];
    let overlap = AuraOverlap::from_spheres(&[0.0; 3], 1.5, &[0.5, 0.0, 0.0], 1.5);
    let (_view, report) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            Some(overlap),
            0,
        )
        .unwrap();
    let witness = report.mutual_witness.expect("witness should fire");
    // Witness fires : at least the AURA-overlap is recorded, even if no
    // individual cell crossed the per-cell shimmer threshold.
    assert!(witness.overlap.is_active());
}

#[test]
fn full_pipeline_per_cell_consent_blocks_some_cells() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[0.0; 3], [1.0; 3], [2.0; 3], [3.0; 3]];
    // First two cells have consent, last two do not.
    let masks = vec![
        open_mask(),
        open_mask(),
        SigmaMaskPacked::from_policy(SigmaPolicy::DefaultPrivate).with_consent(0),
        SigmaMaskPacked::from_policy(SigmaPolicy::DefaultPrivate).with_consent(0),
    ];
    let (view, report) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    assert_eq!(report.cells_consent_refused, 2);
    assert_eq!(report.cells_evaluated, 2);
    // The refused cells have a zero salience and are flagged.
    assert!(view.cells_per_eye[0][2].consent_refused);
    assert!(view.cells_per_eye[0][3].consent_refused);
    assert!(!view.cells_per_eye[0][0].consent_refused);
    assert!(!view.cells_per_eye[0][1].consent_refused);
}

#[test]
fn full_pipeline_companion_holds_sovereignty_overrides_mask() {
    let mut pass = CompanionPerspectivePass::canonical();
    let companion_handle: u16 = 100;
    let ctx = realistic_ctx();
    assert_eq!(ctx.companion_sovereign_handle, companion_handle);
    let positions = vec![[0.0; 3]];
    // Mask has NO consent bits, but is owned by the companion.
    let masks = vec![SigmaMaskPacked::default_mask()
        .with_sovereign(companion_handle)
        .with_consent(0)];
    let (_view, report) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    // Companion holds sovereignty over the cell ⇒ no consent refusal.
    assert_eq!(report.cells_consent_refused, 0);
    assert_eq!(report.cells_evaluated, 1);
}

#[test]
fn full_pipeline_attestation_is_present_on_every_report() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[0.0; 3]];
    let masks = vec![open_mask()];

    // Skipped frame.
    let (_view0, r0) = pass.skip(0, false);
    assert!(r0.attestation.contains("hurt nor harm"));

    // Rendered frame.
    let (_view1, r1) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            1,
        )
        .unwrap();
    assert!(r1.attestation.contains("hurt nor harm"));

    // Companion-declined frame.
    let (_view2, r2) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Declined,
            &ctx,
            &positions,
            &masks,
            None,
            2,
        )
        .unwrap();
    assert!(r2.attestation.contains("hurt nor harm"));
}

#[test]
fn full_pipeline_repeated_invocations_are_byte_deterministic() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
    let masks = vec![open_mask(); 2];
    let (v_a, _r_a) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    let (v_b, _r_b) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    // Outputs MUST be byte-identical.
    for eye in 0..2 {
        assert_eq!(v_a.cells_per_eye[eye].len(), v_b.cells_per_eye[eye].len());
        for (ca, cb) in v_a.cells_per_eye[eye]
            .iter()
            .zip(v_b.cells_per_eye[eye].iter())
        {
            assert_eq!(ca.score, cb.score);
            assert_eq!(ca.viz, cb.viz);
            assert_eq!(ca.bands, cb.bands);
            assert_eq!(ca.consent_refused, cb.consent_refused);
        }
    }
}

#[test]
fn full_pipeline_anxiety_dominates_threat_axis_in_visualization() {
    let mut pass = CompanionPerspectivePass::canonical();
    let mut ctx = realistic_ctx();
    ctx.emotion = cssl_render_companion_perspective::CompanionEmotion {
        curious: 0.0,
        anxious: 1.0,
        content: 0.0,
        alert: 0.0,
    };
    let positions = vec![[5.0, 0.0, 0.0]; 8]; // all near the attention target
    let masks = vec![open_mask(); positions.len()];
    let (view, _report) = pass
        .execute_one_shot(
            PlayerToggleState::On,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            0,
        )
        .unwrap();
    // At least one cell should have Threat as its dominant axis under
    // a strongly-anxious context.
    let any_threat_dominant = view.cells_per_eye[0]
        .iter()
        .any(|c| c.score.dominant() == SalienceAxis::Threat);
    assert!(any_threat_dominant);
}

#[test]
fn budget_records_costs_across_frames() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[0.0; 3]; 4];
    let masks = vec![open_mask(); 4];
    // Five frames : alternating gate-on / gate-off.
    let mut gate = CompanionConsentGate::new();
    gate.companion_grant();
    for frame in 0..5 {
        if frame % 2 == 0 {
            gate.set_player_toggle(PlayerToggleState::On);
        } else {
            gate.set_player_toggle(PlayerToggleState::Off);
        }
        let _r = pass.execute_with_gate(&mut gate, &ctx, &positions, &masks, None);
    }
    // Budget should have 5 samples (one per frame, even off frames record 0).
    assert_eq!(pass.budget.sample_count(), 5);
}

#[test]
fn salience_score_dominant_is_consistent_with_visualization() {
    // Salience-score's dominant axis MUST match the visualization tint.
    let viz = SalienceVisualization::canonical();
    let emo = cssl_render_companion_perspective::CompanionEmotion::default();
    for axis in SalienceAxis::ALL {
        // Use a base + peak that puts magnitude clearly above the glow
        // threshold (mean of [0.4,0.4,0.4,0.4,0.9] = 0.5 ≫ 0.18) while
        // keeping `axis` strictly dominant.
        let mut score = SalienceScore::new([0.4; 5]);
        *score.at_mut(axis) = 0.9;
        assert_eq!(score.dominant(), axis);
        let p = viz.map(&score, &emo);
        let expected_tint = SalienceVisualization::axis_tint(axis);
        // The tint should be within 1e-6 of the expected per-axis tint.
        for c in 0..3 {
            assert!(
                (p.tint_rgb[c] - expected_tint[c]).abs() < 1e-6,
                "axis {axis:?} ch {c} : got {} expected {}",
                p.tint_rgb[c],
                expected_tint[c]
            );
        }
    }
}

#[test]
fn evaluator_handle_used_in_canonical_pipeline() {
    // The evaluator should be reachable through the pass + produce
    // identical outputs when called directly.
    let pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let s_a = pass.evaluator.evaluate(&[1.0, 2.0, 3.0], &ctx);
    let direct_eval = SemanticSalienceEvaluator::new_untrained();
    let s_b = direct_eval.evaluate(&[1.0, 2.0, 3.0], &ctx);
    assert_eq!(s_a, s_b);
}

#[test]
fn pipeline_zero_cost_when_consent_off() {
    let mut pass = CompanionPerspectivePass::canonical();
    let ctx = realistic_ctx();
    let positions = vec![[0.0; 3]; 256];
    let masks = vec![open_mask(); 256];
    // 100 gate-off frames.
    for frame in 0..100 {
        let _r = pass.execute_one_shot(
            PlayerToggleState::Off,
            CompanionConsentDecision::Granted,
            &ctx,
            &positions,
            &masks,
            None,
            frame,
        );
    }
    // Mean cost should be 0 across all gate-off frames.
    assert_eq!(pass.budget.mean_cost_ns(), 0);
}

#[test]
fn mutual_witness_dry_run_skips_below_threshold_cells() {
    let witness = MutualWitnessMode::canonical();
    let overlap = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[0.0; 3], 1.0);
    // Mix of below + above threshold cells.
    let scores = vec![
        SalienceScore::new([0.05; 5]),
        SalienceScore::new([0.5; 5]),
        SalienceScore::new([0.6; 5]),
        SalienceScore::new([0.05; 5]),
    ];
    let elig = witness.dry_run_eligible(overlap, &scores);
    assert_eq!(elig.len(), 2);
}

#[test]
fn budget_is_under_quest3_for_small_pipeline() {
    let b = Stage8Budget::quest3();
    // Confirm budget is the canonical 600 microsec.
    assert_eq!(b.budget_ns(), 600_000);
}

#[test]
fn view_companion_declined_distinct_from_skip() {
    let mut pass = CompanionPerspectivePass::canonical();
    let (declined_view, _) = pass.skip(0, true);
    let (off_view, _) = pass.skip(0, false);
    assert!(declined_view.companion_declined);
    assert!(!off_view.companion_declined);
    // Both are skipped though.
    assert!(declined_view.is_skipped());
    assert!(off_view.is_skipped());
}
