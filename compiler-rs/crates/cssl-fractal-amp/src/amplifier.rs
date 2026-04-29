//! § amplifier — FractalAmplifier (KAN-driven per-fragment evaluator)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The per-fragment evaluator that turns coarse-SDF samples into
//!   sub-pixel detail per `07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.3`.
//!   Wraps three KAN-networks borrowed from cssl-substrate-kan :
//!
//!     - `KAN_micro_displacement      : KanNetwork<7, 1>`  (scalar disp)
//!     - `KAN_micro_roughness         : KanNetwork<7, 1>`  (scalar rough)
//!     - `KAN_micro_color_perturbation: KanNetwork<7, 3>`  (3-band tint)
//!
//!   The seven-input vector is packed exactly as
//!   `07_KAN_RUNTIME_SHADING § IX § canonical-call-site-signature` :
//!
//!     `[pos.xyz | view.xy_proj | grad.norm_2D]`
//!
//!   ⇒ pos.x, pos.y, pos.z, view.x, view.y, grad_norm, grad_z.
//!
//!   Sigma-private inputs short-circuit to `AmplifiedFragment::ZERO`
//!   BEFORE any input vector is constructed. This is the
//!   `00_EXOTICISM § V.3 (d) sovereignty` check.
//!
//! § DETERMINISM
//!   The amplifier is a pure function of `(world_pos, view_dir,
//!   base_sdf_grad, budget, kan_weights)`. No frame-counter, no time, no
//!   seed-by-pixel-id. Pinned by the
//!   `tests::amplifier_is_deterministic_per_input` test.

use crate::budget::DetailBudget;
use crate::cost_model::CostModel;
use crate::fragment::{AmplifiedFragment, MicroColor};
use crate::sdf_trait::{SdfHitInfo, SdfRaymarchAmplifier};
use crate::sigma_mask::SigmaPrivacy;
use cssl_substrate_kan::KanNetwork;

/// § The canonical KAN-amplifier input dimension. Per
///   `07_KAN_RUNTIME_SHADING § II § variant-table § micro-displacement` the
///   input is 7-D : pos.xyz + view.xy_proj + grad.norm_2D.
pub const KAN_AMPLIFIER_INPUT_DIM: usize = 7;

/// § Output dim for the micro-displacement network (scalar disp).
pub const MICRO_DISPLACEMENT_OUTPUT_DIM: usize = 1;

/// § Output dim for the micro-roughness network (scalar rough).
pub const MICRO_ROUGHNESS_OUTPUT_DIM: usize = 1;

/// § Output dim for the micro-color-perturbation network (3-band tint).
pub const MICRO_COLOR_OUTPUT_DIM: usize = 3;

/// § The default KAN-confidence floor — derived from a heuristic that
///   atmospheric-loss-equivalent confidence is ~0.2 at typical M7 scene
///   complexity. Below this the amplifier emits ZERO regardless of
///   tier.
pub const DEFAULT_KAN_CONFIDENCE_FLOOR: f32 = 0.2;

/// § Errors that the amplifier can return.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AmplifierError {
    /// § Per-frame Stage-7 budget exhausted ; `06_RENDERING_PIPELINE
    ///   § Stage-7` "1.2ms @ Quest-3 budget".
    #[error("Stage-7 budget exceeded : {used_ms:.4} ms used > {budget_ms:.4} ms allowed")]
    BudgetExceeded {
        /// § The configured budget in milliseconds.
        budget_ms: f32,
        /// § The cumulative cost at the moment of failure.
        used_ms: f32,
    },
    /// § The KAN-confidence is below the configured floor — the
    ///   amplifier refuses to fire.
    #[error("KAN-confidence {0:.4} is below the configured floor")]
    KanConfidenceTooLow(f32),
    /// § The view-distance is invalid (negative or NaN).
    #[error("view-distance {0:.4} is invalid")]
    InvalidViewDistance(f32),
    /// § The configured budget is invalid (≤0 or NaN).
    #[error("budget {0:.4} ms is invalid")]
    InvalidBudget(f32),
    /// § The configured KAN-network shape doesn't match the canonical
    ///   amplifier shape.
    #[error("KAN-network shape mismatch : expected {expected}, got {actual}")]
    KanShapeMismatch {
        /// § The expected shape literal.
        expected: &'static str,
        /// § The actual shape literal.
        actual: &'static str,
    },
}

/// § The per-fragment fractal amplifier. Wraps three KAN-networks (the
///   canonical micro-displacement, micro-roughness, micro-color
///   triple) plus an optional cost-model for runtime budget tracking.
pub struct FractalAmplifier {
    /// § The micro-displacement KAN-network. `KanNetwork<7, 1>`.
    pub kan_micro_displacement: KanNetwork<7, 1>,
    /// § The micro-roughness KAN-network. `KanNetwork<7, 1>`.
    pub kan_micro_roughness: KanNetwork<7, 1>,
    /// § The micro-color-perturbation KAN-network. `KanNetwork<7, 3>`.
    pub kan_micro_color: KanNetwork<7, 3>,
    /// § Default confidence floor when the budget doesn't supply one.
    pub default_confidence_floor: f32,
}

impl FractalAmplifier {
    /// § Construct with three pre-trained KAN-networks. Use
    ///   [`Self::new_untrained`] to build placeholder networks (whose
    ///   shape-correct outputs are a deterministic synthetic detail
    ///   pattern keyed by the input vector).
    #[must_use]
    pub fn new(
        kan_micro_displacement: KanNetwork<7, 1>,
        kan_micro_roughness: KanNetwork<7, 1>,
        kan_micro_color: KanNetwork<7, 3>,
    ) -> Self {
        Self {
            kan_micro_displacement,
            kan_micro_roughness,
            kan_micro_color,
            default_confidence_floor: DEFAULT_KAN_CONFIDENCE_FLOOR,
        }
    }

    /// § Construct with three new-untrained KAN-networks. The amplifier
    ///   emits a deterministic synthetic detail pattern that is purely
    ///   a function of the 7-D input vector, so the amplifier still
    ///   exercises the determinism + recursion + budget paths even
    ///   without trained weights.
    #[must_use]
    pub fn new_untrained() -> Self {
        Self::new(
            KanNetwork::<7, 1>::new_untrained(),
            KanNetwork::<7, 1>::new_untrained(),
            KanNetwork::<7, 3>::new_untrained(),
        )
    }

    /// § Override the default confidence floor.
    #[must_use]
    pub fn with_confidence_floor(mut self, floor: f32) -> Self {
        self.default_confidence_floor = floor.clamp(0.0, 1.0);
        self
    }

    /// § Pack the 7-D input vector for the KAN-amplifier. Layout :
    ///
    ///     [pos.x, pos.y, pos.z, view.x, view.y, grad_norm, grad.z]
    ///
    ///   This is the spec-mandated layout from
    ///   `07_KAN_RUNTIME_SHADING § IX § canonical-call-site-signature`.
    ///   The view's z-component is dropped (replaced by the gradient's
    ///   z) because for surface-displacement the view-direction's
    ///   axial component contributes only via the angle factor that
    ///   the gradient already encodes.
    #[must_use]
    pub fn pack_input(
        world_pos: [f32; 3],
        view_dir: [f32; 3],
        base_sdf_grad: [f32; 3],
    ) -> [f32; KAN_AMPLIFIER_INPUT_DIM] {
        let grad_norm = (base_sdf_grad[0].powi(2) + base_sdf_grad[1].powi(2)).sqrt();
        [
            world_pos[0],
            world_pos[1],
            world_pos[2],
            view_dir[0],
            view_dir[1],
            grad_norm,
            base_sdf_grad[2],
        ]
    }

    /// § Synthetic detail-pattern for the new-untrained KAN networks.
    ///   This is a deterministic function of the 7-D input that
    ///   exercises the amplifier's pipeline without requiring trained
    ///   weights. Production deployments swap the networks via
    ///   [`Self::new`] ; this synthetic exists for tests + scaffolding.
    ///
    ///   The synthetic uses smooth analytic functions (no RNG, no hash)
    ///   so the determinism contract is preserved : same input ⇒ same
    ///   output, every frame, every host.
    #[must_use]
    fn synthetic_displacement(input: &[f32; KAN_AMPLIFIER_INPUT_DIM]) -> f32 {
        // § Smooth sin/cos pattern keyed on (pos.xyz, grad_norm).
        let p = input[0] * 17.31 + input[1] * 7.91 + input[2] * 11.13;
        let g = input[5] * 5.31;
        // § amplitude bounded into ~[-1, 1] before downstream saturation.
        (p + g).sin() * 0.7
    }

    /// § Synthetic micro-roughness pattern.
    #[must_use]
    fn synthetic_roughness(input: &[f32; KAN_AMPLIFIER_INPUT_DIM]) -> f32 {
        let p = input[0] * 13.07 + input[1] * 19.23 + input[2] * 23.41;
        let v = input[3] * 4.13 + input[4] * 4.17;
        (p + v).cos() * 0.5
    }

    /// § Synthetic micro-color pattern (3-band).
    #[must_use]
    fn synthetic_color(input: &[f32; KAN_AMPLIFIER_INPUT_DIM]) -> [f32; 3] {
        let p = input[0] * 29.81 + input[1] * 31.27 + input[2] * 37.13;
        [
            (p).sin() * 0.4,
            (p + 2.094).sin() * 0.4,
            (p + 4.188).sin() * 0.4,
        ]
    }

    /// § Synthetic confidence pattern. Higher confidence at high
    ///   surface curvature (encoded as grad_norm) and at close
    ///   view-distance (the budget already pre-scales by distance).
    #[must_use]
    fn synthetic_confidence(input: &[f32; KAN_AMPLIFIER_INPUT_DIM]) -> f32 {
        // § grad_norm in [0, ~1.4] for unit-grad surfaces ; map into
        //   [0, 1] confidence.
        let g = input[5];
        // § confidence is a smooth ramp on grad_norm with floor 0.3.
        (0.3 + g * 0.5).clamp(0.0, 1.0)
    }

    /// § The core amplifier evaluation. Pure function of the input
    ///   tuple ; no side effects (no RNG, no time, no global state
    ///   mutation). Returns AmplifiedFragment::ZERO for Σ-private or
    ///   peripheral-skip inputs.
    pub fn amplify(
        &self,
        world_pos: [f32; 3],
        view_dir: [f32; 3],
        base_sdf_grad: [f32; 3],
        budget: &DetailBudget,
        sigma_privacy: SigmaPrivacy,
    ) -> Result<AmplifiedFragment, AmplifierError> {
        // § Σ-private gate : must fire BEFORE any input vector touches
        //   the KAN-network. This is the load-bearing
        //   `00_EXOTICISM § V.3 (d) sovereignty` check.
        if sigma_privacy.is_private() {
            return Ok(AmplifiedFragment::ZERO.with_privacy(SigmaPrivacy::Private));
        }

        // § Peripheral-skip / far-distance early-out.
        if !budget.should_amplify() {
            return Ok(AmplifiedFragment::ZERO);
        }

        let input = Self::pack_input(world_pos, view_dir, base_sdf_grad);

        // § Either evaluate the trained KAN-networks (production path)
        //   OR emit the synthetic deterministic pattern (test +
        //   scaffolding path). The branch is keyed on `trained` flag.
        let raw_disp = if self.kan_micro_displacement.trained {
            self.kan_micro_displacement.eval(&input)[0]
        } else {
            Self::synthetic_displacement(&input)
        };
        let raw_rough = if self.kan_micro_roughness.trained {
            self.kan_micro_roughness.eval(&input)[0]
        } else {
            Self::synthetic_roughness(&input)
        };
        let raw_color = if self.kan_micro_color.trained {
            self.kan_micro_color.eval(&input)
        } else {
            Self::synthetic_color(&input)
        };
        // § Confidence comes from the synthetic floor for now ; a
        //   future trained variant will emit confidence as the second
        //   output of a 7→2 network.
        let raw_conf = Self::synthetic_confidence(&input);

        // § Apply budget amplitude to all spatial-bound outputs.
        let disp = budget.apply_amplitude(raw_disp);
        let rough = budget.apply_amplitude(raw_rough);
        let color = MicroColor::from_array([
            budget.apply_amplitude(raw_color[0]),
            budget.apply_amplitude(raw_color[1]),
            budget.apply_amplitude(raw_color[2]),
        ]);

        // § Confidence floor check : if the per-fragment confidence is
        //   below the budget's floor, emit ZERO (the amplifier
        //   "didn't fire").
        if !budget.confidence_passes(raw_conf) {
            return Ok(AmplifiedFragment::ZERO);
        }

        Ok(AmplifiedFragment::new(
            disp,
            rough,
            color,
            raw_conf,
            sigma_privacy,
        ))
    }

    /// § Convenience wrapper that consumes a `&dyn SdfHitInfo` directly.
    pub fn amplify_hit<H: SdfHitInfo>(
        &self,
        hit: &H,
        budget: &DetailBudget,
    ) -> Result<AmplifiedFragment, AmplifierError> {
        self.amplify(
            hit.world_pos(),
            hit.view_dir(),
            hit.base_sdf_grad(),
            budget,
            hit.sigma_privacy(),
        )
    }

    /// § Combined trait-glue for the cssl-render-v2 (D116) integration :
    ///   evaluate amplification AND charge the cost-model. This is what
    ///   the per-frame walker calls.
    pub fn amplify_charged<H: SdfHitInfo>(
        &self,
        hit: &H,
        budget: &DetailBudget,
        cost: &mut CostModel,
    ) -> Result<AmplifiedFragment, AmplifierError> {
        // § Σ-private + peripheral-skip do NOT charge the cost-model
        //   (no KAN-network was evaluated).
        if hit.sigma_privacy().is_private() || !budget.should_amplify() {
            return self.amplify_hit(hit, budget);
        }
        // § Charge first ; if budget is exceeded the early-error
        //   returns BEFORE the amplifier evaluates.
        cost.charge_one()?;
        self.amplify_hit(hit, budget)
    }
}

impl Default for FractalAmplifier {
    fn default() -> Self {
        Self::new_untrained()
    }
}

impl SdfRaymarchAmplifier for FractalAmplifier {
    fn amplify_at_hit<H: SdfHitInfo>(
        &self,
        hit: &H,
        budget: &DetailBudget,
    ) -> Result<AmplifiedFragment, AmplifierError> {
        self.amplify_hit(hit, budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::FoveaTier;
    use crate::sdf_trait::MockSdfHit;

    /// § Synthetic detail-pattern is deterministic per input.
    #[test]
    fn synthetic_disp_deterministic() {
        let i = [0.5, 1.0, 1.5, 0.0, 0.0, 1.0, 0.0];
        let a = FractalAmplifier::synthetic_displacement(&i);
        let b = FractalAmplifier::synthetic_displacement(&i);
        assert_eq!(a, b);
    }

    /// § pack_input produces the spec-mandated 7-D vector layout.
    #[test]
    fn pack_input_layout() {
        let pos = [1.0, 2.0, 3.0];
        let view = [0.1, 0.2, 0.3];
        let grad = [0.6, 0.8, 0.0];
        let v = FractalAmplifier::pack_input(pos, view, grad);
        assert_eq!(v[0], 1.0);
        assert_eq!(v[1], 2.0);
        assert_eq!(v[2], 3.0);
        assert_eq!(v[3], 0.1);
        assert_eq!(v[4], 0.2);
        // § grad_norm of (0.6, 0.8, _) = 1.0.
        assert!((v[5] - 1.0).abs() < 1e-6);
        assert_eq!(v[6], 0.0);
    }

    /// § Σ-private input emits ZERO with private classification.
    #[test]
    fn sigma_private_emits_zero() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::default();
        let r = amp
            .amplify(
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0],
                &budget,
                SigmaPrivacy::Private,
            )
            .unwrap();
        assert!(r.is_zero());
        assert_eq!(r.sigma_privacy, SigmaPrivacy::Private);
    }

    /// § Peripheral-tier emits ZERO regardless of input.
    #[test]
    fn peripheral_emits_zero() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::from_fovea_tier(FoveaTier::Peripheral);
        let r = amp
            .amplify(
                [0.5, 0.5, 0.5],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0],
                &budget,
                SigmaPrivacy::Public,
            )
            .unwrap();
        assert!(r.is_zero());
    }

    /// § Far-distance emits ZERO regardless of fovea-tier.
    #[test]
    fn far_distance_emits_zero() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::new(FoveaTier::Full, 100.0, 0.5).unwrap();
        let r = amp
            .amplify(
                [0.5, 0.5, 0.5],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0],
                &budget,
                SigmaPrivacy::Public,
            )
            .unwrap();
        assert!(r.is_zero());
    }

    /// § Foveal full-budget emits non-ZERO output for non-degenerate inputs.
    #[test]
    fn foveal_emits_non_zero() {
        let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.05);
        let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
        let r = amp
            .amplify(
                [0.5, 0.5, 0.5],
                [0.0, 0.0, 1.0],
                [0.5, 0.5, 0.0],
                &budget,
                SigmaPrivacy::Public,
            )
            .unwrap();
        // § Some component should be non-zero.
        assert!(!r.is_zero());
    }

    /// § Determinism : same inputs ⇒ same outputs across two calls.
    #[test]
    fn amplifier_is_deterministic_per_input() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::default();
        let inputs = ([1.0, 2.0, 3.0], [0.0, 0.0, 1.0], [0.6, 0.8, 0.0]);
        let a = amp
            .amplify(inputs.0, inputs.1, inputs.2, &budget, SigmaPrivacy::Public)
            .unwrap();
        let b = amp
            .amplify(inputs.0, inputs.1, inputs.2, &budget, SigmaPrivacy::Public)
            .unwrap();
        assert_eq!(a, b);
    }

    /// § Different inputs ⇒ different outputs (i.e. amplifier actually
    /// reads the input).
    #[test]
    fn different_inputs_different_outputs() {
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
                [1.0, 1.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.6, 0.8, 0.0],
                &budget,
                SigmaPrivacy::Public,
            )
            .unwrap();
        assert_ne!(a, b);
    }

    /// § Trait-glue : amplify_hit calls amplify with the right unpacked args.
    #[test]
    fn amplify_hit_unpacks_correctly() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::default();
        let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let from_hit = amp.amplify_hit(&hit, &budget).unwrap();
        let from_direct = amp
            .amplify(
                hit.world_pos,
                hit.view_dir,
                hit.base_sdf_grad,
                &budget,
                SigmaPrivacy::Public,
            )
            .unwrap();
        assert_eq!(from_hit, from_direct);
    }

    /// § amplify_charged charges the cost-model on success.
    #[test]
    fn charged_increments_cost_model() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::default();
        let mut cost = CostModel::quest3();
        let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        amp.amplify_charged(&hit, &budget, &mut cost).unwrap();
        assert_eq!(cost.fragments_amplified, 1);
    }

    /// § amplify_charged does NOT charge for Σ-private input.
    #[test]
    fn charged_skips_for_private() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::default();
        let mut cost = CostModel::quest3();
        let hit = MockSdfHit::default().with_sigma_privacy(SigmaPrivacy::Private);
        amp.amplify_charged(&hit, &budget, &mut cost).unwrap();
        assert_eq!(cost.fragments_amplified, 0);
    }

    /// § amplify_charged does NOT charge for peripheral skip.
    #[test]
    fn charged_skips_for_peripheral() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::from_fovea_tier(FoveaTier::Peripheral);
        let mut cost = CostModel::quest3();
        let hit = MockSdfHit::default();
        amp.amplify_charged(&hit, &budget, &mut cost).unwrap();
        assert_eq!(cost.fragments_amplified, 0);
    }

    /// § with_confidence_floor mutates the floor.
    #[test]
    fn with_confidence_floor() {
        let amp = FractalAmplifier::new_untrained().with_confidence_floor(0.7);
        assert!((amp.default_confidence_floor - 0.7).abs() < 1e-6);
    }

    /// § Default amplifier has untrained networks.
    #[test]
    fn default_is_untrained() {
        let amp = FractalAmplifier::default();
        assert!(!amp.kan_micro_displacement.trained);
        assert!(!amp.kan_micro_roughness.trained);
        assert!(!amp.kan_micro_color.trained);
    }

    /// § Output respects EPSILON_DISP saturation.
    #[test]
    fn output_respects_epsilon_disp() {
        use crate::fragment::EPSILON_DISP;
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::default();
        for &x in &[0.1, 0.5, 1.0, 2.0, 5.0] {
            let r = amp
                .amplify(
                    [x, x, x],
                    [0.0, 0.0, 1.0],
                    [0.5, 0.5, 0.0],
                    &budget,
                    SigmaPrivacy::Public,
                )
                .unwrap();
            assert!(r.micro_displacement.abs() <= EPSILON_DISP + 1e-7);
        }
    }

    /// § SdfRaymarchAmplifier trait dispatches to amplify_hit.
    #[test]
    fn sdf_raymarch_amplifier_trait_dispatches() {
        let amp = FractalAmplifier::new_untrained();
        let budget = DetailBudget::default();
        let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let from_trait = amp.amplify_at_hit(&hit, &budget).unwrap();
        let from_method = amp.amplify_hit(&hit, &budget).unwrap();
        assert_eq!(from_trait, from_method);
    }
}
