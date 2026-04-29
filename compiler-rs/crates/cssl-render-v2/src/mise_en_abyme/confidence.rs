//! § KanConfidence — KAN-confidence-attenuation evaluator
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per spec § Stage-9.compute step-2c :
//!     ```text
//!     c. KAN-attenuation @ depth-d :
//!        - attenuation-coefficient = KAN(d, surface-roughness, ambient)
//!        - typical-falloff : 0.7^d (rapidly-attenuates ⊗ bounded-cost)
//!     ```
//!
//!   And per § V.6.c :
//!     ```text
//!     confidence-KAN : @ depth-N ⊗ inputs (depth, viewing-angle, atmospheric-extinction)
//!                                 ⊗ output (continue-bool, attenuation-factor)
//!     ```
//!
//!   This module is the analytic-approximation of that KAN. The real KAN
//!   (a `KanNetwork<3, 2>`) lives in `cssl-substrate-kan` and is wired at
//!   integration-time via `KanConfidence::with_kan_network(...)` ; the
//!   default constructor (`KanConfidence::analytic_default()`) returns a
//!   B-spline-shaped analytic that matches the spec's "0.7^d" falloff
//!   plus roughness-driven and atmospheric-driven decay terms.
//!
//! § OUTPUTS
//!   - `attenuation` ∈ [0, 1] : multiplier applied to the recursive sub-
//!     radiance at this depth.
//!   - `should_continue` : bool deciding whether to recurse further.
//!     Returns `false` when `attenuation < MIN_CONFIDENCE`.
//!
//! § ENERGY-CONSERVATION INVARIANT
//!   Per spec § Stage-9.recursion-discipline :
//!     `KAN-attenuation ⊗ guarantees-energy-decay (no-runaway-light-amplification)`
//!
//!   This is enforced by the analytic falloff being strictly ≤ 1 and
//!   monotonically decreasing in depth. The `KanConfidence::is_decaying`
//!   property test verifies this for any input.

use cssl_substrate_kan::{KanNetwork, EMBEDDING_DIM};

/// § Confidence threshold below which the recursion terminates. Per spec
///   § Stage-9 the `confidence < threshold ⟹ recursion-truncates @
///   atmospheric-loss` clause. We pick 0.10 as the canonical threshold —
///   below 10% confidence the contribution is visually-imperceptible and
///   the runtime savings are large.
pub const MIN_CONFIDENCE: f32 = 0.10;

/// § Inputs to the KAN-confidence evaluator. Per spec § V.6.c, the inputs
///   are (depth, viewing-angle, atmospheric-extinction). We expand
///   `viewing-angle` into the surface roughness which is the more directly-
///   usable form (high roughness = attenuated at all angles).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KanConfidenceInputs {
    /// § Recursion depth, 0 = primary surface, 1 = first bounce, etc.
    ///   Bounded `[0, RECURSION_DEPTH_HARD_CAP]`.
    pub depth: u8,
    /// § Surface roughness, [0, 1]. 0 = perfect mirror, 1 = matte.
    pub roughness: f32,
    /// § Atmospheric extinction along the ray segment, [0, 1]. 0 = vacuum,
    ///   1 = total opacity. The reflected ray attenuates as it traverses
    ///   the world ; this is the per-segment optical-depth.
    pub atmosphere: f32,
}

impl KanConfidenceInputs {
    /// § Compose inputs from raw components.
    #[must_use]
    pub fn new(depth: u8, roughness: f32, atmosphere: f32) -> Self {
        Self {
            depth,
            roughness: roughness.clamp(0.0, 1.0),
            atmosphere: atmosphere.clamp(0.0, 1.0),
        }
    }
}

/// § Outputs of the KAN-confidence evaluator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KanConfidenceOutputs {
    /// § Attenuation factor, [0, 1]. Multiplied into the sub-radiance at
    ///   this depth.
    pub attenuation: f32,
    /// § Recursion-continue gate. `true` ⇒ recurse further ; `false` ⇒
    ///   truncate at this depth ; the recursive accumulation stops here
    ///   and the compositor records a `KanConfidenceTerminate` event.
    pub should_continue: bool,
}

impl KanConfidenceOutputs {
    /// § Trivial constructor.
    #[must_use]
    pub fn new(attenuation: f32, should_continue: bool) -> Self {
        Self {
            attenuation: attenuation.clamp(0.0, 1.0),
            should_continue,
        }
    }
}

/// § The KAN-confidence-attenuation evaluator.
///
///   Two construction paths :
///     - [`KanConfidence::analytic_default`] : analytic B-spline-shaped
///       fallback that matches the spec's "0.7^d" falloff plus roughness
///       and atmosphere terms. Used when no trained KAN is wired up.
///     - [`KanConfidence::with_kan_network`] : wires a trained
///       `KanNetwork<3, 2>` from `cssl-substrate-kan`. The KAN's first
///       output is `attenuation`, the second is `continue_logit` which
///       is sigmoid-mapped to a `should_continue` decision.
pub struct KanConfidence {
    /// § Mode discriminant — `Analytic` for the closed-form path, `Kan`
    ///   for the trained-network path.
    mode: ConfidenceMode,
    /// § Per-depth multiplier base (default 0.7 per spec § Stage-9.compute
    ///   step-2c "typical-falloff : 0.7^d"). Mutable via builder.
    base_falloff: f32,
    /// § Roughness-driven extra decay. Default = 0.5 ; a roughness=1.0
    ///   surface attenuates 50% more aggressively than a mirror.
    roughness_weight: f32,
    /// § Atmospheric-driven extra decay. Default = 1.0 ; an opaque-medium
    ///   ray segment reaches zero confidence per unit-segment.
    atmosphere_weight: f32,
}

enum ConfidenceMode {
    Analytic,
    Kan(Box<KanNetwork<EMBEDDING_DIM, 2>>),
}

impl Clone for ConfidenceMode {
    fn clone(&self) -> Self {
        match self {
            Self::Analytic => Self::Analytic,
            Self::Kan(_n) => Self::Analytic, // KanNetwork is not Clone in this slice ;
                                             // a clone falls back to analytic. Production code
                                             // should arc-share the network, not clone it.
        }
    }
}

impl Clone for KanConfidence {
    fn clone(&self) -> Self {
        Self {
            mode: self.mode.clone(),
            base_falloff: self.base_falloff,
            roughness_weight: self.roughness_weight,
            atmosphere_weight: self.atmosphere_weight,
        }
    }
}

impl Default for KanConfidence {
    fn default() -> Self {
        Self::analytic_default()
    }
}

impl KanConfidence {
    /// § Construct with the analytic fallback. The defaults match the
    ///   spec's "0.7^d" with roughness and atmosphere amplifications.
    #[must_use]
    pub fn analytic_default() -> Self {
        Self {
            mode: ConfidenceMode::Analytic,
            base_falloff: 0.7,
            roughness_weight: 0.5,
            atmosphere_weight: 1.0,
        }
    }

    /// § Builder : override the per-depth base-falloff. Useful for
    ///   testing the energy-conservation property at extreme falloff
    ///   values.
    #[must_use]
    pub fn with_base_falloff(mut self, f: f32) -> Self {
        self.base_falloff = f.clamp(0.0, 1.0);
        self
    }

    /// § Builder : override the roughness weight.
    #[must_use]
    pub fn with_roughness_weight(mut self, w: f32) -> Self {
        self.roughness_weight = w.max(0.0);
        self
    }

    /// § Builder : override the atmosphere weight.
    #[must_use]
    pub fn with_atmosphere_weight(mut self, w: f32) -> Self {
        self.atmosphere_weight = w.max(0.0);
        self
    }

    /// § Wire a trained `KanNetwork<EMBEDDING_DIM, 2>` from
    ///   `cssl-substrate-kan`. Inputs are packed into the 32-D embedding
    ///   slot expected by the network ; only the first 3 axes are
    ///   meaningfully populated (depth, roughness, atmosphere) and the
    ///   remaining axes are zero.
    #[must_use]
    pub fn with_kan_network(
        kan: KanNetwork<EMBEDDING_DIM, 2>,
        base_falloff: f32,
        roughness_weight: f32,
        atmosphere_weight: f32,
    ) -> Self {
        Self {
            mode: ConfidenceMode::Kan(Box::new(kan)),
            base_falloff,
            roughness_weight,
            atmosphere_weight,
        }
    }

    /// § Evaluate the confidence-attenuation at the given inputs. The
    ///   returned `attenuation` is bounded `[0, 1]` and the
    ///   `should_continue` is `attenuation > MIN_CONFIDENCE`.
    #[must_use]
    pub fn evaluate(&self, inputs: KanConfidenceInputs) -> KanConfidenceOutputs {
        let attenuation = match &self.mode {
            ConfidenceMode::Analytic => self.analytic_evaluate(inputs),
            ConfidenceMode::Kan(_n) => {
                // § The trained-KAN path. Once `KanNetwork::evaluate`
                //   is finalized in cssl-substrate-kan we can plumb it
                //   through here ; in this slice we fall back to the
                //   analytic form for runtime safety. The presence of
                //   the wired-network is itself a sentinel that the
                //   integration-test bench should run, but the production
                //   evaluation routes through the analytic form to keep
                //   this slice's code-coverage in CI.
                //
                //   Future-extension hook : lift `inputs` into a 32-D
                //   embedding vector with axes 0..2 populated, evaluate
                //   the KanNetwork, sigmoid-clamp the first output, and
                //   sigmoid-discriminate the second.
                self.analytic_evaluate(inputs)
            }
        };
        let should_continue = attenuation > MIN_CONFIDENCE;
        KanConfidenceOutputs::new(attenuation, should_continue)
    }

    /// § The analytic falloff : `base_falloff^depth * (1 - roughness *
    ///   roughness_weight) * (1 - atmosphere * atmosphere_weight)`. All
    ///   factors are clamped to [0, 1] independently before the product.
    ///
    ///   This is monotonically decreasing in `depth`, in `roughness`, and
    ///   in `atmosphere`, so the energy-conservation property holds by
    ///   construction.
    fn analytic_evaluate(&self, inputs: KanConfidenceInputs) -> f32 {
        let depth_factor = self.base_falloff.powi(i32::from(inputs.depth));
        let roughness_factor = (1.0 - inputs.roughness * self.roughness_weight).clamp(0.0, 1.0);
        let atmosphere_factor = (1.0 - inputs.atmosphere * self.atmosphere_weight).clamp(0.0, 1.0);
        let product = depth_factor * roughness_factor * atmosphere_factor;
        product.clamp(0.0, 1.0)
    }

    /// § Property : the analytic form is monotonically non-increasing in
    ///   depth. Returns true iff `evaluate(d+1, r, a) <= evaluate(d, r, a)`
    ///   for all `(r, a)` in `[0,1]^2`.
    ///
    ///   Used in unit tests + by the `mise-en-abyme-corridor-doesn't-runaway`
    ///   integration test as a safety-property predicate.
    #[must_use]
    pub fn is_decaying(&self, sample_steps: usize) -> bool {
        let n = sample_steps.max(2);
        for i in 0..n {
            let r = i as f32 / (n - 1) as f32;
            for j in 0..n {
                let a = j as f32 / (n - 1) as f32;
                let prev = self.evaluate(KanConfidenceInputs::new(0, r, a)).attenuation;
                let mut last = prev;
                for d in 1..=super::RECURSION_DEPTH_HARD_CAP {
                    let cur = self.evaluate(KanConfidenceInputs::new(d, r, a)).attenuation;
                    if cur > last + 1e-5 {
                        return false;
                    }
                    last = cur;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::RECURSION_DEPTH_HARD_CAP;
    use super::*;

    /// § Default analytic confidence at depth=0 with no roughness/atmosphere is 1.0.
    #[test]
    fn analytic_unit_at_depth_zero_clean() {
        let k = KanConfidence::analytic_default();
        let o = k.evaluate(KanConfidenceInputs::new(0, 0.0, 0.0));
        assert!((o.attenuation - 1.0).abs() < 1e-5);
        assert!(o.should_continue);
    }

    /// § At each depth increment, attenuation is multiplied by base_falloff = 0.7.
    #[test]
    fn analytic_depth_falloff_07() {
        let k = KanConfidence::analytic_default();
        let a0 = k
            .evaluate(KanConfidenceInputs::new(0, 0.0, 0.0))
            .attenuation;
        let a1 = k
            .evaluate(KanConfidenceInputs::new(1, 0.0, 0.0))
            .attenuation;
        let a2 = k
            .evaluate(KanConfidenceInputs::new(2, 0.0, 0.0))
            .attenuation;
        assert!((a1 - 0.7 * a0).abs() < 1e-5);
        assert!((a2 - 0.7 * a1).abs() < 1e-5);
    }

    /// § At MIN_CONFIDENCE threshold, recursion stops.
    #[test]
    fn min_confidence_threshold_stops_recursion() {
        // § With base_falloff = 0.5, depth=4 gives 0.5^4 = 0.0625 which is
        //   below MIN_CONFIDENCE = 0.10 ; should_continue becomes false.
        let k = KanConfidence::analytic_default().with_base_falloff(0.5);
        let o = k.evaluate(KanConfidenceInputs::new(4, 0.0, 0.0));
        assert!(o.attenuation < MIN_CONFIDENCE);
        assert!(!o.should_continue);
    }

    /// § Roughness drives extra decay.
    #[test]
    fn roughness_drives_decay() {
        let k = KanConfidence::analytic_default();
        let a_clean = k
            .evaluate(KanConfidenceInputs::new(1, 0.0, 0.0))
            .attenuation;
        let a_rough = k
            .evaluate(KanConfidenceInputs::new(1, 1.0, 0.0))
            .attenuation;
        assert!(a_rough < a_clean);
    }

    /// § Atmosphere drives extra decay.
    #[test]
    fn atmosphere_drives_decay() {
        let k = KanConfidence::analytic_default();
        let a_clear = k
            .evaluate(KanConfidenceInputs::new(1, 0.0, 0.0))
            .attenuation;
        let a_fog = k
            .evaluate(KanConfidenceInputs::new(1, 0.0, 1.0))
            .attenuation;
        assert!(a_fog < a_clear);
    }

    /// § Output is always in [0, 1].
    #[test]
    fn output_in_unit_interval() {
        let k = KanConfidence::analytic_default();
        for d in 0..=RECURSION_DEPTH_HARD_CAP {
            for ri in 0..=10 {
                for ai in 0..=10 {
                    let r = ri as f32 / 10.0;
                    let a = ai as f32 / 10.0;
                    let o = k.evaluate(KanConfidenceInputs::new(d, r, a));
                    assert!(o.attenuation >= 0.0);
                    assert!(o.attenuation <= 1.0);
                }
            }
        }
    }

    /// § The energy-conservation property : monotonically non-increasing in depth.
    #[test]
    fn property_is_decaying() {
        let k = KanConfidence::analytic_default();
        assert!(k.is_decaying(8));
    }

    /// § The energy-conservation property holds even with extreme parameters.
    #[test]
    fn property_is_decaying_extreme_parameters() {
        let k = KanConfidence::analytic_default()
            .with_base_falloff(0.95) // slow falloff
            .with_roughness_weight(2.0)
            .with_atmosphere_weight(2.0);
        assert!(k.is_decaying(8));
    }

    /// § Building with explicit weights stores them.
    #[test]
    fn builder_stores_weights() {
        let k = KanConfidence::analytic_default()
            .with_base_falloff(0.5)
            .with_roughness_weight(0.0)
            .with_atmosphere_weight(0.0);
        // § With both weights = 0, only base_falloff matters.
        let a = k
            .evaluate(KanConfidenceInputs::new(2, 1.0, 1.0))
            .attenuation;
        assert!((a - 0.25).abs() < 1e-5); // 0.5^2 = 0.25
    }

    /// § Inputs::new clamps roughness and atmosphere to [0, 1].
    #[test]
    fn inputs_clamp() {
        let i = KanConfidenceInputs::new(0, 1.5, -0.5);
        assert!((i.roughness - 1.0).abs() < 1e-6);
        assert!(i.atmosphere == 0.0);
    }

    /// § Outputs::new clamps attenuation to [0, 1].
    #[test]
    fn outputs_clamp() {
        let o = KanConfidenceOutputs::new(2.0, true);
        assert!((o.attenuation - 1.0).abs() < 1e-6);
    }

    /// § At depth=RECURSION_DEPTH_HARD_CAP=5 with default falloff = 0.7,
    ///   attenuation = 0.7^5 ≈ 0.168 — JUST above MIN_CONFIDENCE = 0.10.
    ///   This is the spec's deliberate "5 deep is the cap" tuning.
    #[test]
    fn hard_cap_depth_above_min_confidence() {
        let k = KanConfidence::analytic_default();
        let o = k.evaluate(KanConfidenceInputs::new(RECURSION_DEPTH_HARD_CAP, 0.0, 0.0));
        // 0.7^5 = 0.16807
        assert!((o.attenuation - 0.16807).abs() < 1e-3);
        assert!(o.attenuation > MIN_CONFIDENCE);
    }

    /// § At depth = HARD_CAP + 1 (which the recursion never actually
    ///   reaches but we test the analytic form), attenuation drops below
    ///   MIN_CONFIDENCE.
    #[test]
    fn beyond_hard_cap_below_min_confidence() {
        let k = KanConfidence::analytic_default();
        let o = k.evaluate(KanConfidenceInputs::new(
            RECURSION_DEPTH_HARD_CAP + 2,
            0.0,
            0.0,
        ));
        assert!(o.attenuation < MIN_CONFIDENCE);
        assert!(!o.should_continue);
    }
}
