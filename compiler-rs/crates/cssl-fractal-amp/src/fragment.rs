//! § AmplifiedFragment — the per-fragment output of the Stage-7 amplifier
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The output struct that [`crate::FractalAmplifier::amplify`] produces.
//!   Three components per the V.3 surface declared in
//!   `07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.3 (c)` :
//!
//!     `(micro-disp, micro-roughness, micro-spectral-tint)`
//!
//!   Fed-into : ray-march-bisection-refine + KAN-BRDF evaluation. The
//!   bisection-refine consumes `micro_displacement` to perturb the hit
//!   point along the surface normal ; the BRDF consumes `micro_roughness`
//!   as an additive surface-roughness shift and `micro_color` as a
//!   spectral-tint pre-shift to the M-coord input vector.
//!
//! § DETERMINISM
//!   The struct contains scalar floats only. No trailing
//!   `frame_counter` / `time` / `seed` field — that would violate the
//!   V.3 (d) reversibility row "same-input → same-output ⊗
//!   frame-deterministic". The struct's `Default` is the no-op fragment
//!   (all-zero amplification ; effectively "no detail emerged here"),
//!   which is what peripheral-skip and Σ-private branches return.

use crate::sigma_mask::SigmaPrivacy;

/// § A 3-channel micro-spectral-tint perturbation. Spec name :
///   `micro-spectral-tint` per `07_AESTHETIC/00_EXOTICISM § V.3 (c)`. The
///   three channels are the LOW / MID / HIGH spectral-bin tints that the
///   downstream KAN-BRDF (D118) adds to its M-coord pre-shift. Values are
///   bounded to `[-0.5, +0.5]` per channel so a fully-saturated tint can
///   shift the M-coord by at most half-a-cell — never a full cell, which
///   would be a discontinuity that tears across BRDF-LUT boundaries.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MicroColor {
    /// § Low-band tint contribution (≈ 380-505 nm spectral bin).
    pub low: f32,
    /// § Mid-band tint contribution (≈ 505-630 nm spectral bin).
    pub mid: f32,
    /// § High-band tint contribution (≈ 630-750 nm spectral bin).
    pub high: f32,
}

impl MicroColor {
    /// § The all-zero (no-tint) micro-color. This is the result for
    ///   peripheral-skip and Σ-private fragments.
    pub const ZERO: Self = Self {
        low: 0.0,
        mid: 0.0,
        high: 0.0,
    };

    /// § Construct from a `[f32; 3]` (the canonical KAN-output layout).
    #[must_use]
    pub const fn from_array(arr: [f32; 3]) -> Self {
        Self {
            low: arr[0],
            mid: arr[1],
            high: arr[2],
        }
    }

    /// § Saturate each channel into `[-0.5, +0.5]`. Called by
    ///   [`crate::FractalAmplifier::amplify`] to enforce the discontinuity
    ///   guard described in this module's rustdoc.
    #[must_use]
    pub fn saturated(self) -> Self {
        Self {
            low: self.low.clamp(-0.5, 0.5),
            mid: self.mid.clamp(-0.5, 0.5),
            high: self.high.clamp(-0.5, 0.5),
        }
    }

    /// § True iff every channel is exactly zero. Useful for the
    ///   peripheral-skip fast path.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.low == 0.0 && self.mid == 0.0 && self.high == 0.0
    }

    /// § Channel-wise add (used by `RecursiveDetailLOD` to accumulate
    ///   tint contributions across recursion levels).
    #[must_use]
    pub fn add(self, other: Self) -> Self {
        Self {
            low: self.low + other.low,
            mid: self.mid + other.mid,
            high: self.high + other.high,
        }
    }

    /// § Channel-wise scale (used by recursion attenuation).
    #[must_use]
    pub fn scale(self, k: f32) -> Self {
        Self {
            low: self.low * k,
            mid: self.mid * k,
            high: self.high * k,
        }
    }
}

/// § The amplified fragment that Stage-7 emits per pixel-fragment. Three
///   components matching `07_AESTHETIC/00_EXOTICISM § V.3 (c)` plus a
///   confidence scalar that the recursion driver uses to decide whether
///   to truncate.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AmplifiedFragment {
    /// § Sub-pixel surface-normal-direction displacement, in scene units.
    ///   Bounded to `[-EPSILON_DISP, +EPSILON_DISP]` after saturation.
    pub micro_displacement: f32,
    /// § Sub-pixel additive surface-roughness shift, dimensionless.
    ///   Bounded to `[-0.25, +0.25]` (the BRDF's roughness axis is
    ///   `[0, 1]` so this is a quarter-axis-shift max).
    pub micro_roughness: f32,
    /// § Three-channel sub-pixel spectral-tint perturbation. Bounded
    ///   per-channel to `[-0.5, +0.5]`.
    pub micro_color: MicroColor,
    /// § Recursion-truncation confidence in `[0, 1]`. Output by the
    ///   amplifier alongside the other three components ; consumed by
    ///   `RecursiveDetailLOD` to decide whether to descend another level.
    ///   `1.0` = highest confidence, definitely descend ; `0.0` = atmospheric
    ///   loss / Σ-private / over-budget, do not descend further.
    pub kan_confidence: f32,
    /// § Σ-privacy classification of the fragment that produced this
    ///   amplified output. Recorded so the consumer can audit
    ///   "does this fragment leak across an Σ-private region ?" at the
    ///   composition step.
    pub sigma_privacy: SigmaPrivacy,
}

/// § Maximum surface-normal displacement magnitude (scene units). Per
///   `07_AESTHETIC/00_EXOTICISM § V.3 (a)` the displacement is "sub-pixel"
///   ; we encode that as `epsilon = 1e-3` scene-units which at the
///   canonical ~1 m / scene-unit scale = 1 mm = below the smallest
///   visible pixel-projected feature at typical Quest-3 view-distance.
///   The amplifier saturates outputs into `[-EPSILON_DISP, +EPSILON_DISP]`.
pub const EPSILON_DISP: f32 = 1.0e-3;

/// § Maximum surface-roughness shift. The BRDF roughness axis is
///   `[0, 1]` so a `±0.25` shift is one quarter of the axis — enough
///   for visible micro-roughness without crossing the
///   "rough-glossy" / "smooth-matte" boundary in the KAN-BRDF LUT.
pub const EPSILON_ROUGHNESS: f32 = 0.25;

impl AmplifiedFragment {
    /// § The no-op (all-zero) amplified fragment. Used by the
    ///   peripheral-skip path (FoveaTier::Peripheral) and the Σ-private
    ///   path. `kan_confidence` is 0.0 here so the recursion driver
    ///   immediately truncates.
    pub const ZERO: Self = Self {
        micro_displacement: 0.0,
        micro_roughness: 0.0,
        micro_color: MicroColor::ZERO,
        kan_confidence: 0.0,
        sigma_privacy: SigmaPrivacy::Public,
    };

    /// § Construct an amplified fragment with explicit values, with
    ///   automatic per-channel saturation to enforce the discontinuity
    ///   guards declared in this module's constants.
    #[must_use]
    pub fn new(
        micro_displacement: f32,
        micro_roughness: f32,
        micro_color: MicroColor,
        kan_confidence: f32,
        sigma_privacy: SigmaPrivacy,
    ) -> Self {
        Self {
            micro_displacement: micro_displacement.clamp(-EPSILON_DISP, EPSILON_DISP),
            micro_roughness: micro_roughness.clamp(-EPSILON_ROUGHNESS, EPSILON_ROUGHNESS),
            micro_color: micro_color.saturated(),
            kan_confidence: kan_confidence.clamp(0.0, 1.0),
            sigma_privacy,
        }
    }

    /// § Mark this fragment as Σ-private. Σ-private fragments retain
    ///   their amplification values BUT downstream consumers MUST refuse
    ///   to leak them across the public boundary. The amplifier itself
    ///   emits ZERO for Σ-private inputs ; this constructor is for the
    ///   composition step that re-attaches privacy after a sub-call.
    #[must_use]
    pub fn with_privacy(mut self, sigma_privacy: SigmaPrivacy) -> Self {
        self.sigma_privacy = sigma_privacy;
        self
    }

    /// § True iff this fragment is the no-op all-zero fragment. Equivalent
    ///   to `*self == ZERO` but cheaper since it does not derive PartialEq
    ///   on the entire struct.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.micro_displacement == 0.0
            && self.micro_roughness == 0.0
            && self.micro_color.is_zero()
            && self.kan_confidence == 0.0
    }

    /// § Recursion attenuation. Multiplies displacement / roughness /
    ///   color tint by the attenuation coefficient and reduces
    ///   confidence by the same factor (one-step accumulator for the
    ///   `RecursiveDetailLOD` driver).
    #[must_use]
    pub fn attenuate(self, k: f32) -> Self {
        let kk = k.clamp(0.0, 1.0);
        Self {
            micro_displacement: self.micro_displacement * kk,
            micro_roughness: self.micro_roughness * kk,
            micro_color: self.micro_color.scale(kk),
            kan_confidence: self.kan_confidence * kk,
            sigma_privacy: self.sigma_privacy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § AmplifiedFragment::ZERO has no displacement / roughness / color.
    #[test]
    fn zero_is_no_op() {
        let f = AmplifiedFragment::ZERO;
        assert_eq!(f.micro_displacement, 0.0);
        assert_eq!(f.micro_roughness, 0.0);
        assert_eq!(f.micro_color, MicroColor::ZERO);
        assert_eq!(f.kan_confidence, 0.0);
        assert!(f.is_zero());
    }

    /// § new() saturates micro-displacement into the EPSILON_DISP band.
    #[test]
    fn new_saturates_displacement_high() {
        let f = AmplifiedFragment::new(1.0, 0.0, MicroColor::ZERO, 0.5, SigmaPrivacy::Public);
        assert_eq!(f.micro_displacement, EPSILON_DISP);
    }

    /// § new() saturates negative micro-displacement.
    #[test]
    fn new_saturates_displacement_low() {
        let f = AmplifiedFragment::new(-1.0, 0.0, MicroColor::ZERO, 0.5, SigmaPrivacy::Public);
        assert_eq!(f.micro_displacement, -EPSILON_DISP);
    }

    /// § new() saturates micro-roughness into the EPSILON_ROUGHNESS band.
    #[test]
    fn new_saturates_roughness() {
        let f = AmplifiedFragment::new(0.0, 1.0, MicroColor::ZERO, 0.5, SigmaPrivacy::Public);
        assert_eq!(f.micro_roughness, EPSILON_ROUGHNESS);
    }

    /// § new() saturates per-channel micro-color to ±0.5.
    #[test]
    fn new_saturates_color() {
        let f = AmplifiedFragment::new(
            0.0,
            0.0,
            MicroColor {
                low: 1.0,
                mid: -1.0,
                high: 0.5,
            },
            0.5,
            SigmaPrivacy::Public,
        );
        assert_eq!(f.micro_color.low, 0.5);
        assert_eq!(f.micro_color.mid, -0.5);
        assert_eq!(f.micro_color.high, 0.5);
    }

    /// § new() clamps confidence to [0, 1].
    #[test]
    fn new_clamps_confidence() {
        let f = AmplifiedFragment::new(0.0, 0.0, MicroColor::ZERO, 2.0, SigmaPrivacy::Public);
        assert_eq!(f.kan_confidence, 1.0);
        let f = AmplifiedFragment::new(0.0, 0.0, MicroColor::ZERO, -1.0, SigmaPrivacy::Public);
        assert_eq!(f.kan_confidence, 0.0);
    }

    /// § attenuate(k) scales all four components by k.
    #[test]
    fn attenuate_scales_components() {
        let f = AmplifiedFragment::new(
            EPSILON_DISP,
            EPSILON_ROUGHNESS,
            MicroColor::from_array([0.4, 0.4, 0.4]),
            1.0,
            SigmaPrivacy::Public,
        );
        let half = f.attenuate(0.5);
        assert!((half.micro_displacement - EPSILON_DISP * 0.5).abs() < 1e-6);
        assert!((half.micro_roughness - EPSILON_ROUGHNESS * 0.5).abs() < 1e-6);
        assert!((half.micro_color.low - 0.2).abs() < 1e-6);
        assert!((half.kan_confidence - 0.5).abs() < 1e-6);
    }

    /// § attenuate clamps the coefficient into [0, 1].
    #[test]
    fn attenuate_clamps_coefficient() {
        let f = AmplifiedFragment::new(
            EPSILON_DISP,
            EPSILON_ROUGHNESS,
            MicroColor::from_array([0.4, 0.4, 0.4]),
            1.0,
            SigmaPrivacy::Public,
        );
        // § over-1 attenuation collapses to identity (no amplification).
        let over = f.attenuate(2.0);
        assert_eq!(over.micro_displacement, f.micro_displacement);
        // § negative attenuation collapses to zero.
        let neg = f.attenuate(-1.0);
        assert!(neg.is_zero());
    }

    /// § with_privacy preserves all numeric components.
    #[test]
    fn with_privacy_preserves_numerics() {
        let f = AmplifiedFragment::new(
            0.5e-3,
            0.1,
            MicroColor::from_array([0.1, 0.2, 0.3]),
            0.8,
            SigmaPrivacy::Public,
        );
        let p = f.with_privacy(SigmaPrivacy::Private);
        assert_eq!(p.micro_displacement, f.micro_displacement);
        assert_eq!(p.micro_roughness, f.micro_roughness);
        assert_eq!(p.micro_color, f.micro_color);
        assert_eq!(p.kan_confidence, f.kan_confidence);
        assert_eq!(p.sigma_privacy, SigmaPrivacy::Private);
    }

    /// § MicroColor::add is channel-wise.
    #[test]
    fn micro_color_add_channelwise() {
        let a = MicroColor::from_array([0.1, 0.2, 0.3]);
        let b = MicroColor::from_array([0.05, 0.05, 0.05]);
        let c = a.add(b);
        assert!((c.low - 0.15).abs() < 1e-6);
        assert!((c.mid - 0.25).abs() < 1e-6);
        assert!((c.high - 0.35).abs() < 1e-6);
    }

    /// § MicroColor::scale is channel-wise.
    #[test]
    fn micro_color_scale_channelwise() {
        let a = MicroColor::from_array([0.2, 0.4, 0.6]);
        let b = a.scale(0.5);
        assert!((b.low - 0.1).abs() < 1e-6);
        assert!((b.mid - 0.2).abs() < 1e-6);
        assert!((b.high - 0.3).abs() < 1e-6);
    }

    /// § MicroColor::ZERO satisfies is_zero().
    #[test]
    fn micro_color_zero_is_zero() {
        assert!(MicroColor::ZERO.is_zero());
    }

    /// § MicroColor::saturated clamps each channel independently.
    #[test]
    fn micro_color_saturate_independent() {
        let raw = MicroColor {
            low: 0.7,
            mid: 0.0,
            high: -0.7,
        };
        let s = raw.saturated();
        assert_eq!(s.low, 0.5);
        assert_eq!(s.mid, 0.0);
        assert_eq!(s.high, -0.5);
    }
}
