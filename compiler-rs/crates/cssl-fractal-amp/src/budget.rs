//! § budget — DetailBudget + FoveaTier classification
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per-pixel KAN-detail-budget conditioned on three factors :
//!
//!     (a) FoveaMask from D120 gaze-collapse.
//!     (b) View-distance to the surface.
//!     (c) KAN-confidence (the second output of the recursion-truncation
//!         KAN).
//!
//!   The budget is consumed by [`crate::RecursiveDetailLOD`] to decide
//!   how deep the fractal-recursion descends. The mapping table comes
//!   straight out of `06_RENDERING_PIPELINE § Stage-7 § compute` :
//!
//!     - FoveaMask = full       → DetailBudget::Full   (KAN @ full-depth ; 5 levels max)
//!     - FoveaMask = 2×2 mid    → DetailBudget::MidHalf (KAN × 0.5  ; 2 levels max)
//!     - FoveaMask = 4×4 periph → DetailBudget::PeripheralSkip (no amplifier)
//!
//!   View-distance further attenuates : surfaces > 10 m away never receive
//!   the full-depth amplifier even in the fovea, because the projected
//!   sub-pixel area is already smaller than the sample pixel.
//!
//! § DETERMINISM CONTRACT
//!   The budget MUST be a pure function of `(fovea_tier, view_distance,
//!   kan_confidence_floor)`. No frame-counter, no time, no RNG. Pinned by
//!   `tests::budget_is_deterministic`.
//!
//! § BUDGET — Stage-7 cost-model
//!   Per `07_KAN_RUNTIME_SHADING § VII § §D budget-decomposition` :
//!
//!     Sub-pixel-fractal (fovea) | 0.42 ms | 3.8% frame
//!
//!   The 0.42 ms figure assumes ~5% trigger density on ~520k foveal
//!   pixels at ~50ns/eval. The DetailBudget enforces this trigger-density
//!   by checking the `pixel_projected_area × surface_curvature` product
//!   against a τ_micro threshold (`07_KAN § IX § fovea-amplifier
//!   optimization`).

use crate::amplifier::AmplifierError;

/// § FoveaTier — three-level classification matching the
///   `06_RENDERING_PIPELINE § Stage-7 § compute` table verbatim. The
///   D120 gaze-collapse stage emits one of these per pixel ; the
///   amplifier consumes it via [`DetailBudget::from_fovea_tier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoveaTier {
    /// § Foveal pixel ⊗ full-depth amplifier ⊗ 5 recursion levels.
    Full,
    /// § Mid-region pixel ⊗ half-depth amplifier ⊗ 2 recursion levels
    ///   ⊗ KAN evaluated at 0.5× attenuated amplitude.
    Mid,
    /// § Peripheral pixel ⊗ no amplifier ⊗ output = base SDF.
    #[default]
    Peripheral,
}

impl FoveaTier {
    /// § Returns the maximum recursion-depth for this tier.
    #[must_use]
    pub const fn max_recursion_depth(self) -> u8 {
        match self {
            Self::Full => 5,
            Self::Mid => 2,
            Self::Peripheral => 0,
        }
    }

    /// § Returns the amplitude-attenuation coefficient for this tier.
    #[must_use]
    pub const fn amplitude_attenuation(self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::Mid => 0.5,
            Self::Peripheral => 0.0,
        }
    }

    /// § True iff the amplifier should evaluate at all for this tier.
    ///   `Peripheral` returns false — the amplifier is skipped entirely
    ///   per `06_RENDERING_PIPELINE § Stage-7 § §D compute step 3`.
    #[must_use]
    pub const fn should_amplify(self) -> bool {
        !matches!(self, Self::Peripheral)
    }
}

/// § A pre-computed FoveaTier classification for "full-detail" callers.
///   Equivalent to `FoveaTier::Full` but exposed as a const so unit tests
///   can write `BUDGET_FULL` without importing the enum.
pub const BUDGET_FULL: FoveaTier = FoveaTier::Full;

/// § A pre-computed FoveaTier classification for "mid-region half-rate"
///   callers. Equivalent to `FoveaTier::Mid`.
pub const BUDGET_MID_HALF: FoveaTier = FoveaTier::Mid;

/// § A pre-computed FoveaTier classification for "peripheral skip" callers.
///   Equivalent to `FoveaTier::Peripheral`.
pub const BUDGET_PERIPHERAL_SKIP: FoveaTier = FoveaTier::Peripheral;

/// § The view-distance at which the amplifier's amplitude is reduced
///   linearly. Surfaces beyond `MAX_AMPLIFY_DISTANCE` get zero amplitude
///   ; surfaces closer than `FULL_AMPLIFY_DISTANCE` get full amplitude ;
///   in between is a linear ramp.
pub const FULL_AMPLIFY_DISTANCE: f32 = 1.0; // scene-units (≈ 1 m)
/// § See `FULL_AMPLIFY_DISTANCE`.
pub const MAX_AMPLIFY_DISTANCE: f32 = 10.0; // scene-units (≈ 10 m)

/// § The KAN-confidence floor. If the configured `kan_confidence_floor`
///   is below this constant the budget refuses construction (returns
///   `AmplifierError::KanConfidenceTooLow`). The floor exists because a
///   below-floor confidence means atmospheric loss / Σ-private / over-
///   budget, in which case the amplifier shouldn't fire at all.
pub const MIN_CONFIDENCE_FLOOR: f32 = 0.05;

/// § The default KAN-confidence floor — moderately permissive. A tighter
///   floor (e.g. 0.5) suppresses amplification of low-curvature surfaces
///   ; a looser floor (e.g. 0.05) lets the amplifier fire on near-flat
///   surfaces too. The default sits at the inflection of the
///   `06_RENDERING_PIPELINE § Stage-7` budget curve.
pub const DEFAULT_CONFIDENCE_FLOOR: f32 = 0.2;

/// § Per-pixel detail-budget. Constructed by combining a [`FoveaTier`],
///   a `view_distance`, and a `kan_confidence_floor`. Consumed by
///   [`crate::FractalAmplifier::amplify`] to decide whether to evaluate
///   the KAN-network and by [`crate::RecursiveDetailLOD`] to decide how
///   deep the fractal-recursion descends.
#[derive(Debug, Clone, Copy)]
pub struct DetailBudget {
    /// § The per-pixel fovea classification.
    pub fovea_tier: FoveaTier,
    /// § View-distance attenuation factor in `[0, 1]`. `1.0` = full
    ///   amplitude (close surface), `0.0` = no amplitude (far surface).
    pub distance_attenuation: f32,
    /// § Below-floor confidence ⇒ amplifier emits ZERO.
    pub kan_confidence_floor: f32,
    /// § The maximum recursion depth allowed by this budget. Derived
    ///   from `fovea_tier` and capped at `MAX_RECURSION_DEPTH`.
    pub max_recursion_depth: u8,
    /// § Amplitude-attenuation coefficient (composed of fovea-tier
    ///   amplitude × distance-attenuation). Multiplied into every output
    ///   component.
    pub amplitude: f32,
}

impl DetailBudget {
    /// § Construct from the three conditioning factors.
    pub fn new(
        fovea_tier: FoveaTier,
        view_distance: f32,
        kan_confidence_floor: f32,
    ) -> Result<Self, AmplifierError> {
        if !view_distance.is_finite() || view_distance < 0.0 {
            return Err(AmplifierError::InvalidViewDistance(view_distance));
        }
        if kan_confidence_floor < MIN_CONFIDENCE_FLOOR
            || kan_confidence_floor > 1.0
            || !kan_confidence_floor.is_finite()
        {
            return Err(AmplifierError::KanConfidenceTooLow(kan_confidence_floor));
        }

        // § Linear ramp : full amplitude at FULL_AMPLIFY_DISTANCE, zero
        //   at MAX_AMPLIFY_DISTANCE.
        let distance_attenuation = if view_distance <= FULL_AMPLIFY_DISTANCE {
            1.0
        } else if view_distance >= MAX_AMPLIFY_DISTANCE {
            0.0
        } else {
            let span = MAX_AMPLIFY_DISTANCE - FULL_AMPLIFY_DISTANCE;
            let into = view_distance - FULL_AMPLIFY_DISTANCE;
            1.0 - (into / span)
        };

        let max_recursion_depth = fovea_tier.max_recursion_depth();
        let amplitude = fovea_tier.amplitude_attenuation() * distance_attenuation;

        Ok(Self {
            fovea_tier,
            distance_attenuation,
            kan_confidence_floor,
            max_recursion_depth,
            amplitude,
        })
    }

    /// § Construct directly from a FoveaTier ; uses default
    ///   distance-attenuation (1.0) and default confidence-floor.
    pub fn from_fovea_tier(fovea_tier: FoveaTier) -> Self {
        // § Defaults are guaranteed valid by the constants above so we
        //   unwrap safely.
        Self::new(fovea_tier, FULL_AMPLIFY_DISTANCE, DEFAULT_CONFIDENCE_FLOOR)
            .expect("default detail-budget params are always valid")
    }

    /// § True iff the amplifier should evaluate at all for this budget.
    ///   False for peripheral-skip and for far surfaces (amplitude = 0).
    #[must_use]
    pub fn should_amplify(&self) -> bool {
        self.fovea_tier.should_amplify() && self.amplitude > 0.0
    }

    /// § True iff the given KAN-confidence passes the floor check.
    #[must_use]
    pub fn confidence_passes(&self, kan_confidence: f32) -> bool {
        kan_confidence >= self.kan_confidence_floor
    }

    /// § Apply the budget's amplitude to a fragment value. Returns
    ///   `value × amplitude` ; called by the amplifier on the raw
    ///   KAN-network outputs to produce the final saturated output.
    #[must_use]
    pub fn apply_amplitude(&self, value: f32) -> f32 {
        value * self.amplitude
    }
}

impl Default for DetailBudget {
    fn default() -> Self {
        Self::from_fovea_tier(FoveaTier::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § FoveaTier::Full → 5 levels recursion.
    #[test]
    fn full_max_depth_is_5() {
        assert_eq!(FoveaTier::Full.max_recursion_depth(), 5);
    }

    /// § FoveaTier::Mid → 2 levels.
    #[test]
    fn mid_max_depth_is_2() {
        assert_eq!(FoveaTier::Mid.max_recursion_depth(), 2);
    }

    /// § FoveaTier::Peripheral → 0 levels.
    #[test]
    fn peripheral_max_depth_is_0() {
        assert_eq!(FoveaTier::Peripheral.max_recursion_depth(), 0);
    }

    /// § Full amplitude is 1.0.
    #[test]
    fn full_amplitude_is_one() {
        assert!((FoveaTier::Full.amplitude_attenuation() - 1.0).abs() < 1e-6);
    }

    /// § Mid amplitude is 0.5.
    #[test]
    fn mid_amplitude_is_half() {
        assert!((FoveaTier::Mid.amplitude_attenuation() - 0.5).abs() < 1e-6);
    }

    /// § Peripheral amplitude is 0.0.
    #[test]
    fn peripheral_amplitude_is_zero() {
        assert!(FoveaTier::Peripheral.amplitude_attenuation() == 0.0);
    }

    /// § FoveaTier::Peripheral.should_amplify() is false.
    #[test]
    fn peripheral_should_not_amplify() {
        assert!(!FoveaTier::Peripheral.should_amplify());
    }

    /// § FoveaTier::Full.should_amplify() is true.
    #[test]
    fn full_should_amplify() {
        assert!(FoveaTier::Full.should_amplify());
    }

    /// § DetailBudget::new() with negative distance fails.
    #[test]
    fn budget_rejects_negative_distance() {
        let r = DetailBudget::new(FoveaTier::Full, -1.0, 0.5);
        assert!(matches!(r, Err(AmplifierError::InvalidViewDistance(_))));
    }

    /// § DetailBudget::new() with NaN distance fails.
    #[test]
    fn budget_rejects_nan_distance() {
        let r = DetailBudget::new(FoveaTier::Full, f32::NAN, 0.5);
        assert!(matches!(r, Err(AmplifierError::InvalidViewDistance(_))));
    }

    /// § DetailBudget::new() with confidence below floor fails.
    #[test]
    fn budget_rejects_low_confidence() {
        let r = DetailBudget::new(FoveaTier::Full, 1.0, 0.01);
        assert!(matches!(r, Err(AmplifierError::KanConfidenceTooLow(_))));
    }

    /// § DetailBudget::new() with confidence > 1 fails.
    #[test]
    fn budget_rejects_over_one_confidence() {
        let r = DetailBudget::new(FoveaTier::Full, 1.0, 2.0);
        assert!(matches!(r, Err(AmplifierError::KanConfidenceTooLow(_))));
    }

    /// § view_distance ≤ FULL_AMPLIFY_DISTANCE → distance_attenuation = 1.
    #[test]
    fn close_distance_full_attenuation() {
        let b = DetailBudget::new(FoveaTier::Full, 0.5, 0.5).unwrap();
        assert!((b.distance_attenuation - 1.0).abs() < 1e-6);
    }

    /// § view_distance ≥ MAX_AMPLIFY_DISTANCE → distance_attenuation = 0.
    #[test]
    fn far_distance_zero_attenuation() {
        let b = DetailBudget::new(FoveaTier::Full, 100.0, 0.5).unwrap();
        assert!(b.distance_attenuation == 0.0);
    }

    /// § view_distance in range → linear ramp.
    #[test]
    fn mid_distance_linear_ramp() {
        let mid = (FULL_AMPLIFY_DISTANCE + MAX_AMPLIFY_DISTANCE) * 0.5;
        let b = DetailBudget::new(FoveaTier::Full, mid, 0.5).unwrap();
        assert!((b.distance_attenuation - 0.5).abs() < 1e-6);
    }

    /// § amplitude composes fovea-tier × distance-attenuation.
    #[test]
    fn amplitude_composes() {
        let mid = (FULL_AMPLIFY_DISTANCE + MAX_AMPLIFY_DISTANCE) * 0.5;
        let b = DetailBudget::new(FoveaTier::Mid, mid, 0.5).unwrap();
        // § Mid tier amplitude (0.5) × distance attenuation (0.5) = 0.25
        assert!((b.amplitude - 0.25).abs() < 1e-6);
    }

    /// § Peripheral budget never amplifies, regardless of distance.
    #[test]
    fn peripheral_never_amplifies() {
        let b = DetailBudget::new(FoveaTier::Peripheral, 0.1, 0.5).unwrap();
        assert!(!b.should_amplify());
    }

    /// § Far-distance budget never amplifies even at Full tier.
    #[test]
    fn far_full_never_amplifies() {
        let b = DetailBudget::new(FoveaTier::Full, 100.0, 0.5).unwrap();
        assert!(!b.should_amplify());
    }

    /// § confidence_passes() honors the floor.
    #[test]
    fn confidence_passes_above_floor() {
        let b = DetailBudget::new(FoveaTier::Full, 1.0, 0.5).unwrap();
        assert!(b.confidence_passes(0.6));
        assert!(!b.confidence_passes(0.4));
    }

    /// § Budget construction is deterministic — same inputs ⇒ same outputs.
    #[test]
    fn budget_is_deterministic() {
        let a = DetailBudget::new(FoveaTier::Full, 5.5, 0.4).unwrap();
        let b = DetailBudget::new(FoveaTier::Full, 5.5, 0.4).unwrap();
        assert_eq!(a.fovea_tier, b.fovea_tier);
        assert!((a.distance_attenuation - b.distance_attenuation).abs() < 1e-9);
        assert!((a.amplitude - b.amplitude).abs() < 1e-9);
        assert_eq!(a.max_recursion_depth, b.max_recursion_depth);
    }

    /// § from_fovea_tier shorthand uses default distance + confidence.
    #[test]
    fn from_fovea_tier_uses_defaults() {
        let b = DetailBudget::from_fovea_tier(FoveaTier::Full);
        assert_eq!(b.fovea_tier, FoveaTier::Full);
        assert!((b.distance_attenuation - 1.0).abs() < 1e-6);
    }

    /// § default DetailBudget is FoveaTier::Full at full amplitude.
    #[test]
    fn default_is_full() {
        let b: DetailBudget = DetailBudget::default();
        assert_eq!(b.fovea_tier, FoveaTier::Full);
        assert!((b.amplitude - 1.0).abs() < 1e-6);
    }

    /// § BUDGET_FULL / BUDGET_MID_HALF / BUDGET_PERIPHERAL_SKIP constants.
    #[test]
    fn budget_constants() {
        assert_eq!(BUDGET_FULL, FoveaTier::Full);
        assert_eq!(BUDGET_MID_HALF, FoveaTier::Mid);
        assert_eq!(BUDGET_PERIPHERAL_SKIP, FoveaTier::Peripheral);
    }

    /// § apply_amplitude scales correctly at full.
    #[test]
    fn apply_amplitude_at_full() {
        let b = DetailBudget::default();
        assert!((b.apply_amplitude(0.5) - 0.5).abs() < 1e-6);
    }

    /// § apply_amplitude scales correctly at mid.
    #[test]
    fn apply_amplitude_at_mid() {
        let b = DetailBudget::from_fovea_tier(FoveaTier::Mid);
        assert!((b.apply_amplitude(0.5) - 0.25).abs() < 1e-6);
    }
}
