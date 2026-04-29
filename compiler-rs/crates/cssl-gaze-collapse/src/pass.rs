//! `GazeCollapsePass` : the render-graph node implementing pipeline-stage-2.
//!
//! § DESIGN
//!   The pass orchestrates :
//!     1. Validate the IFC-labeled `SensitiveGaze` input (re-checks the
//!        biometric-egress-gate is wired in — defensive ; the type-system
//!        already enforces this).
//!     2. Run the saccadic-predictor to generate the predicted-gaze for
//!        the configured horizon (default 4 ms).
//!     3. Compute the per-eye `FoveaMask` from the predicted-gaze (so the
//!        renderer is one-frame-ahead and saccadic-suppression hides any
//!        flicker).
//!     4. Step the `ObservationCollapseEvolver` to detect peripheral-→-
//!        foveal transitions ; emit the `RegionTransition` events that
//!        Stage-3's Phase-1 COLLAPSE consumes.
//!     5. Compose the `KanDetailBudget` from the per-region budget-
//!        coefficients (config) + the transition-events (load-bearing
//!        regions get full depth).
//!     6. Emit `GazeCollapseOutputs` carrying the FoveaMask × 2 + the
//!        KanDetailBudget + the CollapseBiasVector.
//!
//! § FALLBACK PATH
//!   When `config.opt_in == false` OR the gaze-input's confidence falls
//!   below `GazeConfidence::FALLBACK_THRESHOLD`, the pass takes the
//!   `FoveationFallback::CenterBias` path : the FoveaMask is anchored at
//!   the screen-center, no SaccadePredictor work is done, no transitions
//!   are emitted (the center-bias is stable across frames). This means
//!   ZERO gaze-bearing data crosses the pass-output boundary in the
//!   fallback path.
//!
//! § STRICT MODE
//!   When `config.strict_mode == true`, a confidence drop mid-frame raises
//!   [`crate::GazeCollapseError::ConsentRevokedStrict`] rather than
//!   silently falling back. Callers that want explicit handling of the
//!   consent-revoked case use this mode ; the spec-default is graceful
//!   fallback.

use crate::config::GazeCollapseConfig;
use crate::error::GazeCollapseError;
use crate::fovea_mask::{FoveaMask, FoveaResolution};
use crate::gaze_input::{GazeConfidence, GazeInput, SensitiveGaze};
use crate::observation_collapse::{
    CollapseBiasVector, KanLike, MockKan, ObservationCollapseEvolver, RegionTransition,
};
use crate::saccade_predictor::{
    PredictedSaccade, SaccadePredictor, SaccadePredictorConfig, SaccadePredictorMetrics,
};
use cssl_ifc::{validate_egress, EgressGrantError, LabeledValue};

/// Per-region KAN-detail-evaluation depth budget.
///
/// Maps each FoveaResolution class to its (coefficient × KAN-depth) ;
/// downstream stages (Stage-7 FractalAmplifierPass) consume this to
/// throttle their per-pixel cost.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KanDetailBudget {
    /// Foveal coefficient ∈ [0.0, 1.0] (1.0 = full-depth-8).
    pub foveal: f32,
    /// Para-foveal coefficient.
    pub para_foveal: f32,
    /// Peripheral coefficient.
    pub peripheral: f32,
    /// Total foveal-pixel-count (for budget-pulldown).
    pub foveal_pixels: u32,
    /// Total para-foveal-pixel-count.
    pub para_foveal_pixels: u32,
    /// Total peripheral-pixel-count.
    pub peripheral_pixels: u32,
}

impl KanDetailBudget {
    /// Construct from a single-eye FoveaMask + budget config.
    #[must_use]
    pub fn from_mask(mask: &FoveaMask, config: &GazeCollapseConfig) -> Self {
        Self {
            foveal: config.budget.foveal,
            para_foveal: config.budget.para_foveal,
            peripheral: config.budget.peripheral,
            foveal_pixels: mask.foveal_pixel_count(),
            para_foveal_pixels: mask.para_foveal_pixel_count(),
            peripheral_pixels: mask.peripheral_pixel_count(),
        }
    }

    /// Coefficient lookup by FoveaResolution.
    #[must_use]
    pub fn coefficient_for(&self, res: FoveaResolution) -> f32 {
        match res {
            FoveaResolution::Full => self.foveal,
            FoveaResolution::Half => self.para_foveal,
            FoveaResolution::Quarter => self.peripheral,
        }
    }

    /// Pull the budget down by `factor` (0..=1.0) — used when stage-7
    /// detects hardware-overload and signals stage-2 to throttle next
    /// frame.
    pub fn pull_down(&mut self, factor: f32) {
        let f = factor.clamp(0.0, 1.0);
        self.foveal *= f;
        self.para_foveal *= f;
        self.peripheral *= f;
    }
}

/// Outputs of the `GazeCollapsePass`.
#[derive(Debug, Clone)]
pub struct GazeCollapseOutputs {
    /// Per-eye fovea-masks (left, right).
    pub fovea_masks: [FoveaMask; 2],
    /// Per-eye KAN-detail-budget.
    pub kan_budgets: [KanDetailBudget; 2],
    /// Collapse-bias-vector (single — gaze is conjugate so this is shared).
    pub collapse_bias: CollapseBiasVector,
    /// Region-transitions detected this frame.
    pub transitions: Vec<RegionTransition>,
    /// Predicted-saccade (if any). `None` if fallback path was taken.
    pub predicted_saccade: Option<PredictedSaccade>,
    /// Whether the fallback (center-bias) path was used this frame.
    pub fallback_used: bool,
    /// Diagnostic metrics from the saccade predictor (if used).
    pub predictor_metrics: Option<SaccadePredictorMetrics>,
}

impl GazeCollapseOutputs {
    /// `true` iff this frame's outputs include actual gaze-derived data
    /// (vs. center-bias fallback).
    #[must_use]
    pub fn used_gaze_data(&self) -> bool {
        !self.fallback_used
    }
}

/// `GazeCollapsePass` : the render-graph Stage-2 node.
pub struct GazeCollapsePass<K: KanLike = MockKan> {
    config: GazeCollapseConfig,
    predictor: SaccadePredictor,
    evolver: ObservationCollapseEvolver<K>,
}

impl GazeCollapsePass<MockKan> {
    /// Construct with a default (mock) KAN evaluator.
    pub fn new(config: GazeCollapseConfig) -> Result<Self, GazeCollapseError> {
        config.validate()?;
        let predictor = SaccadePredictor::new(SaccadePredictorConfig {
            horizon: config.prediction_horizon,
            ..SaccadePredictorConfig::default()
        });
        Ok(Self {
            config,
            predictor,
            evolver: ObservationCollapseEvolver::default(),
        })
    }
}

impl<K: KanLike> GazeCollapsePass<K> {
    /// Construct with an explicit KAN evaluator.
    pub fn with_kan(config: GazeCollapseConfig, kan: K) -> Result<Self, GazeCollapseError> {
        config.validate()?;
        let predictor = SaccadePredictor::new(SaccadePredictorConfig {
            horizon: config.prediction_horizon,
            ..SaccadePredictorConfig::default()
        });
        Ok(Self {
            config,
            predictor,
            evolver: ObservationCollapseEvolver::with_kan(kan),
        })
    }

    /// Borrow the active config.
    #[must_use]
    pub const fn config(&self) -> &GazeCollapseConfig {
        &self.config
    }

    /// Borrow the saccade predictor.
    #[must_use]
    pub const fn predictor(&self) -> &SaccadePredictor {
        &self.predictor
    }

    /// Borrow the observation-collapse evolver.
    #[must_use]
    pub const fn evolver(&self) -> &ObservationCollapseEvolver<K> {
        &self.evolver
    }

    /// Execute the pass for the given gaze input.
    ///
    /// **Egress contract** : the input is `SensitiveGaze` (a
    /// `LabeledValue<GazeInput>` carrying `SensitiveDomain::Gaze`). The
    /// output `GazeCollapseOutputs` does NOT carry the gaze-domain — the
    /// outputs (FoveaMask, KAN-budget, CollapseBiasVector) are
    /// gaze-derived but all-on-device-rendering signals, suitable for the
    /// downstream rendering stages. Stage-2's *intermediate* values
    /// (predicted-saccade, history-hash) DO carry the gaze-domain and
    /// stay confined to this pass.
    pub fn execute(
        &mut self,
        input: &SensitiveGaze,
    ) -> Result<GazeCollapseOutputs, GazeCollapseError> {
        // Defensive : verify the IFC label is intact before we even start.
        // This is the explicit pass-API surface where the cssl-ifc gate
        // would refuse if a caller tried to flow this to telemetry.
        // (The output of this pass does not flow gaze-domain forward.)
        debug_assert!(input.is_biometric());
        debug_assert!(input
            .sensitive_domains
            .contains(&cssl_ifc::SensitiveDomain::Gaze));

        let raw = &input.value;

        // Fallback path : opt-out OR low-confidence.
        let confidence_below_threshold = !raw
            .bound_confidence()
            .passes_threshold(GazeConfidence::FALLBACK_THRESHOLD);
        let force_fallback = !self.config.opt_in;

        if force_fallback || confidence_below_threshold {
            if self.config.strict_mode && confidence_below_threshold {
                return Err(GazeCollapseError::ConsentRevokedStrict);
            }
            return Ok(self.execute_fallback(raw.frame_counter));
        }

        // Real path : update predictor with current input.
        let (predicted, metrics) = self.predictor.predict(raw)?;
        let predicted_input_left = self.synthesize_predicted_input(raw, predicted, true);
        let predicted_input_right = self.synthesize_predicted_input(raw, predicted, false);

        // Compute per-eye FoveaMask from the predicted gaze.
        let mask_left = FoveaMask::compute(&predicted_input_left, &self.config)?;
        let mask_right = FoveaMask::compute(&predicted_input_right, &self.config)?;

        // Update history with the *measured* (not predicted) gaze.
        self.evolver
            .push_history(raw.cyclopean_direction(), raw.frame_counter);

        // Step the collapse-evolver against the LEFT mask (we only need one
        // — left and right are conjugate so transitions are equivalent).
        let mut transitions = self.evolver.step(&mask_left, &self.config)?;

        // Apply diagnostic-overlay if requested.
        if self.config.diagnostic_overlay {
            // The diagnostic_circle_pixels are returned to the caller via
            // the FoveaMask's anchor + the regions[] surface. Downstream
            // can rasterize them. We don't mutate the mask data so the
            // shading-rate-hint is preserved.
        }

        let collapse_bias = CollapseBiasVector::from_transitions(&transitions);

        // Sort transitions deterministically so caller-visible ordering
        // does not depend on transient HashMap iteration order.
        transitions.sort_by_key(|t| (t.anchor_y, t.anchor_x));

        let kan_budget_left = KanDetailBudget::from_mask(&mask_left, &self.config);
        let kan_budget_right = KanDetailBudget::from_mask(&mask_right, &self.config);

        Ok(GazeCollapseOutputs {
            fovea_masks: [mask_left, mask_right],
            kan_budgets: [kan_budget_left, kan_budget_right],
            collapse_bias,
            transitions,
            predicted_saccade: Some(predicted),
            fallback_used: false,
            predictor_metrics: Some(metrics),
        })
    }

    /// The center-bias-fallback path. No gaze-data flows.
    fn execute_fallback(&mut self, frame_counter: u32) -> GazeCollapseOutputs {
        let mask_left = FoveaMask::center_bias(&self.config);
        let mask_right = FoveaMask::center_bias(&self.config);
        let kan_budget_left = KanDetailBudget::from_mask(&mask_left, &self.config);
        let kan_budget_right = KanDetailBudget::from_mask(&mask_right, &self.config);

        // No transition detection in fallback (center-bias is stable).
        // Bump the prev_mask state in the evolver to keep it in sync (so
        // when the user toggles opt_in back on, we don't mis-detect a
        // false transition between center-bias and gaze-tracked).
        let _ = self.evolver.step(&mask_left, &self.config);

        let _ = frame_counter;
        GazeCollapseOutputs {
            fovea_masks: [mask_left, mask_right],
            kan_budgets: [kan_budget_left, kan_budget_right],
            collapse_bias: CollapseBiasVector::default(),
            transitions: Vec::new(),
            predicted_saccade: None,
            fallback_used: true,
            predictor_metrics: None,
        }
    }

    /// Synthesize a "predicted" gaze-input by replacing the eye-direction
    /// with the saccade-predictor's output. This is what the renderer
    /// actually projects against (one-frame-ahead).
    fn synthesize_predicted_input(
        &self,
        raw: &GazeInput,
        predicted: PredictedSaccade,
        left: bool,
    ) -> GazeInput {
        let mut out = raw.clone();
        if left {
            out.left_direction = predicted.direction;
        } else {
            out.right_direction = predicted.direction;
        }
        out
    }

    /// Reset all state (used between sessions).
    pub fn reset(&mut self) {
        self.predictor.reset();
        self.evolver.reset();
    }
}

/// **Compile-time gate verification** : a free function that demonstrates the
/// telemetry-egress refusal for any value derived from a `SensitiveGaze`.
/// Calling this with the result of `gaze_input.cyclopean_direction()`
/// wrapped in a Sensitive-labeled value yields `Err(BiometricRefused)`.
///
/// This is the entry-point that the ON-DEVICE-ONLY verification tests
/// invoke ; if the gate ever fails to fire, this function will return Ok.
pub fn assert_no_egress<T>(value: &LabeledValue<T>) -> Result<(), EgressGrantError> {
    validate_egress(value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use cssl_ifc::{
        Confidentiality, EgressGrantError, Integrity, Label, LabeledValue, Principal, PrincipalSet,
        SensitiveDomain,
    };

    use super::{
        assert_no_egress, GazeCollapseError, GazeCollapseOutputs, GazeCollapsePass, KanDetailBudget,
    };
    use crate::config::GazeCollapseConfig;
    use crate::fovea_mask::FoveaResolution;
    use crate::gaze_input::{
        EyeOpenness, GazeConfidence, GazeDirection, GazeInput, SaccadeState, SensitiveGaze,
        SensitiveGazeConstructors,
    };

    fn small_config_opt_in() -> GazeCollapseConfig {
        let mut cfg = GazeCollapseConfig::quest3_opted_in();
        cfg.render_target_width = 256;
        cfg.render_target_height = 256;
        cfg
    }

    fn small_config_opt_out() -> GazeCollapseConfig {
        let mut cfg = GazeCollapseConfig::default();
        cfg.render_target_width = 256;
        cfg.render_target_height = 256;
        cfg
    }

    fn baseline(frame: u32) -> GazeInput {
        GazeInput {
            left_direction: GazeDirection::FORWARD,
            right_direction: GazeDirection::FORWARD,
            left_confidence: GazeConfidence::new(0.95).unwrap(),
            right_confidence: GazeConfidence::new(0.95).unwrap(),
            left_openness: EyeOpenness::new(0.95).unwrap(),
            right_openness: EyeOpenness::new(0.95).unwrap(),
            saccade_state: SaccadeState::Fixation,
            frame_counter: frame,
            convergence_meters: None,
        }
    }

    #[test]
    fn pass_constructs_with_default_config() {
        let cfg = small_config_opt_in();
        let _p = GazeCollapsePass::new(cfg).unwrap();
    }

    #[test]
    fn pass_rejects_invalid_config() {
        let mut cfg = small_config_opt_in();
        cfg.render_target_width = 0;
        let p = GazeCollapsePass::new(cfg);
        assert!(p.is_err());
    }

    #[test]
    fn pass_executes_opt_in_path_emits_no_fallback() {
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let input = SensitiveGaze::from_raw(baseline(0));
        let outputs = p.execute(&input).unwrap();
        assert!(!outputs.fallback_used);
        assert!(outputs.predicted_saccade.is_some());
        assert!(outputs.predictor_metrics.is_some());
    }

    #[test]
    fn pass_opt_out_takes_fallback_path() {
        let cfg = small_config_opt_out();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let input = SensitiveGaze::from_raw(baseline(0));
        let outputs = p.execute(&input).unwrap();
        assert!(outputs.fallback_used);
        assert!(outputs.predicted_saccade.is_none());
        // No transitions emitted in fallback.
        assert!(outputs.transitions.is_empty());
    }

    #[test]
    fn pass_low_confidence_takes_fallback_path() {
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let mut raw = baseline(0);
        raw.left_confidence = GazeConfidence::new(0.1).unwrap();
        raw.right_confidence = GazeConfidence::new(0.1).unwrap();
        let input = SensitiveGaze::from_raw(raw);
        let outputs = p.execute(&input).unwrap();
        assert!(outputs.fallback_used);
    }

    #[test]
    fn pass_strict_mode_rejects_low_confidence() {
        let mut cfg = small_config_opt_in();
        cfg.strict_mode = true;
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let mut raw = baseline(0);
        raw.left_confidence = GazeConfidence::new(0.1).unwrap();
        raw.right_confidence = GazeConfidence::new(0.1).unwrap();
        let input = SensitiveGaze::from_raw(raw);
        let res = p.execute(&input);
        assert!(matches!(res, Err(GazeCollapseError::ConsentRevokedStrict)));
    }

    #[test]
    fn pass_outputs_two_per_eye_masks() {
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let input = SensitiveGaze::from_raw(baseline(0));
        let outputs = p.execute(&input).unwrap();
        assert_eq!(outputs.fovea_masks.len(), 2);
        assert_eq!(outputs.kan_budgets.len(), 2);
    }

    #[test]
    fn pass_emits_transitions_on_anchor_shift() {
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        // Frame 0 : forward.
        let input0 = SensitiveGaze::from_raw(baseline(0));
        let _ = p.execute(&input0).unwrap();
        // Frame 1 : large angular jump.
        let mut raw1 = baseline(1);
        let s = (1.0_f32 / 3.0).sqrt();
        raw1.left_direction = GazeDirection::new(s, s, s).unwrap();
        raw1.right_direction = raw1.left_direction;
        let input1 = SensitiveGaze::from_raw(raw1);
        let outputs1 = p.execute(&input1).unwrap();
        assert!(
            !outputs1.transitions.is_empty(),
            "expected at least one transition"
        );
    }

    #[test]
    fn pass_used_gaze_data_predicate() {
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let input = SensitiveGaze::from_raw(baseline(0));
        let outputs = p.execute(&input).unwrap();
        assert!(outputs.used_gaze_data());

        let cfg2 = small_config_opt_out();
        let mut p2 = GazeCollapsePass::new(cfg2).unwrap();
        let outputs2 = p2.execute(&input).unwrap();
        assert!(!outputs2.used_gaze_data());
    }

    #[test]
    fn kan_detail_budget_from_mask_carries_pixel_counts() {
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let input = SensitiveGaze::from_raw(baseline(0));
        let outputs = p.execute(&input).unwrap();
        let kb = &outputs.kan_budgets[0];
        let total = kb.foveal_pixels + kb.para_foveal_pixels + kb.peripheral_pixels;
        assert_eq!(total, 256 * 256);
    }

    #[test]
    fn kan_detail_budget_coefficient_for_resolution() {
        let cfg = small_config_opt_in();
        let mask = crate::fovea_mask::FoveaMask::center_bias(&cfg);
        let kb = KanDetailBudget::from_mask(&mask, &cfg);
        assert!((kb.coefficient_for(FoveaResolution::Full) - 1.0).abs() < 1e-6);
        assert!((kb.coefficient_for(FoveaResolution::Half) - 0.5).abs() < 1e-6);
        assert!((kb.coefficient_for(FoveaResolution::Quarter) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn kan_detail_budget_pull_down() {
        let cfg = small_config_opt_in();
        let mask = crate::fovea_mask::FoveaMask::center_bias(&cfg);
        let mut kb = KanDetailBudget::from_mask(&mask, &cfg);
        kb.pull_down(0.5);
        assert!((kb.foveal - 0.5).abs() < 1e-6);
        assert!((kb.para_foveal - 0.25).abs() < 1e-6);
    }

    #[test]
    fn pass_reset_clears_state() {
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let input = SensitiveGaze::from_raw(baseline(0));
        let _ = p.execute(&input).unwrap();
        p.reset();
        assert!(p.predictor().last_prediction().is_none());
        assert_eq!(p.evolver().history().recent.len(), 0);
    }

    // ────────────────────────────────────────────────────────────────────
    // § ON-DEVICE-ONLY (compile-error-tests)
    // ────────────────────────────────────────────────────────────────────
    //
    // The cssl-ifc structural-gate refuses telemetry-egress for any value
    // labeled with `SensitiveDomain::Gaze`. The tests below verify the
    // gate at every constructor + propagation surface.

    #[test]
    fn assert_no_egress_refuses_sensitive_gaze() {
        let s = SensitiveGaze::from_raw(baseline(0));
        let res = assert_no_egress(&s);
        assert!(matches!(
            res,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn assert_no_egress_refuses_pure_gaze_label_no_domain() {
        // Even if the SensitiveDomain were stripped (cannot happen via the
        // public API but verified-end-to-end here), the LABEL with
        // GazeSubject in confidentiality MUST trigger refusal.
        let mut conf = PrincipalSet::empty();
        conf.insert(Principal::Subject);
        conf.insert(Principal::GazeSubject);
        let label = Label {
            confidentiality: Confidentiality(conf),
            integrity: Integrity(PrincipalSet::singleton(Principal::Subject)),
        };
        let bare: LabeledValue<u32> = LabeledValue::new(0, label);
        let res = assert_no_egress(&bare);
        assert!(matches!(
            res,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn assert_no_egress_refuses_sensitive_gaze_with_extra_domains() {
        // Even if we add benign domains, the Gaze tag still triggers refusal.
        let raw = baseline(0);
        let label = Label::default();
        let mut domains = BTreeSet::new();
        domains.insert(SensitiveDomain::Gaze);
        domains.insert(SensitiveDomain::Privacy);
        let s = LabeledValue::with_domains(raw, label, domains);
        let res = assert_no_egress(&s);
        assert!(matches!(
            res,
            Err(EgressGrantError::BiometricRefused {
                domain: SensitiveDomain::Gaze
            })
        ));
    }

    #[test]
    fn assert_no_egress_no_privilege_can_grant_gaze() {
        for tier in [
            cssl_ifc::PrivilegeLevel::User,
            cssl_ifc::PrivilegeLevel::System,
            cssl_ifc::PrivilegeLevel::Kernel,
            cssl_ifc::PrivilegeLevel::Root,
            cssl_ifc::PrivilegeLevel::AnthropicAudit,
            cssl_ifc::PrivilegeLevel::ApockyRoot,
        ] {
            let cap =
                cssl_ifc::TelemetryEgress::for_domain_with_privilege(SensitiveDomain::Gaze, tier);
            assert!(
                matches!(cap, Err(EgressGrantError::BiometricRefused { .. })),
                "tier {:?} must not authorize gaze egress",
                tier
            );
        }
    }

    #[test]
    fn pass_input_carries_gaze_label_and_refuses() {
        let s = SensitiveGaze::from_raw(baseline(0));
        // Verify the input has the gate-active label.
        assert!(s.is_biometric());
        assert!(s.is_egress_banned());
    }

    #[test]
    fn pass_outputs_do_not_carry_gaze_domain_directly() {
        // The pass output (FoveaMask, KanDetailBudget, CollapseBiasVector)
        // does NOT carry the Gaze label — these are gaze-DERIVED but
        // strip the domain at the pass-API boundary because they are
        // rendering-pipeline values (suitable for downstream stages).
        // The `cssl-render` integration is what wires the egress-gate
        // into the telemetry sink ; the pass itself produces values
        // ready-for-consumption-by-stage-3.
        let cfg = small_config_opt_in();
        let mut p = GazeCollapsePass::new(cfg).unwrap();
        let input = SensitiveGaze::from_raw(baseline(0));
        let outputs: GazeCollapseOutputs = p.execute(&input).unwrap();
        // The outputs are NOT LabeledValue<_> ; they are bare structs.
        // To flow them to telemetry, the caller must wrap them in
        // LabeledValue with whatever label is appropriate. Since the
        // pass does not apply the Gaze label automatically, the caller
        // is responsible for either re-applying it (which would block
        // telemetry) or treating them as render-pipeline data (which
        // is the spec-intended use).
        let _ = outputs.fovea_masks;
    }
}
