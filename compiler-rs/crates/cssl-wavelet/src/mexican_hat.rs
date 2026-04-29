//! § mexican_hat — Ricker (Mexican-hat) continuous wavelet
//!
//! § PRIMER
//!
//! The Mexican-hat (Ricker) wavelet is the negative second derivative
//! of a Gaussian :
//!
//! ```text
//! ψ(t) = (1 − t²) · exp(−t² / 2)         (unnormalized)
//! ψ(t) = (2 / (√3 · π^(1/4))) · (1 − t²) · exp(−t² / 2)   (L²-normalized)
//! ```
//!
//! It is a *continuous* wavelet : there is no finite filter-tap form
//! and no quadrature-mirror-filter pair. The discrete wavelet transform
//! path therefore does not apply ; instead, the natural use case is
//! continuous-wavelet-transform-style scale-space analysis, where the
//! signal is convolved against `ψ(t / s)` at a sequence of scales `s`.
//! This is exactly the cascade-multi-band mode the radiance-cascade GI
//! subsystem (Axiom-10 § IV) uses for spatial probe pyramids.
//!
//! § INTEGRATION HOOKS
//!
//! `MexicanHatScale` carries a scale parameter and exposes
//! `evaluate(t)` for direct sampling and `convolve(signal, dt)` for
//! discretized signal analysis at that scale. The `cascade` module's
//! `CascadeProbePyramid` consumes a sequence of `MexicanHatScale` for
//! the multi-resolution probe layout.

use crate::boundary::BoundaryMode;
use crate::WaveletBasis;

/// § Continuous Mexican-hat wavelet evaluator.
///
/// The mother wavelet `ψ(t) = (1 − t²) · exp(−t² / 2)`. With L²-normalization
/// applied, the normalization constant `2 / (√3 · π^(1/4))` ≈ 0.867325 — set
/// `normalize` to `true` at construction to apply it.
#[derive(Debug, Clone, Copy)]
pub struct MexicanHat {
    /// Center scale ; the wavelet is evaluated as `ψ(t / scale)`.
    pub scale: f32,
    /// Whether to apply the L²-normalization constant.
    pub normalize: bool,
}

impl MexicanHat {
    /// Construct a Mexican-hat wavelet at the given scale (unnormalized).
    #[must_use]
    pub const fn new(scale: f32) -> Self {
        Self {
            scale,
            normalize: false,
        }
    }

    /// Construct an L²-normalized Mexican-hat wavelet at the given scale.
    #[must_use]
    pub const fn new_normalized(scale: f32) -> Self {
        Self {
            scale,
            normalize: true,
        }
    }

    /// Evaluate the wavelet at point `t`.
    #[must_use]
    pub fn evaluate(&self, t: f32) -> f32 {
        // Norm constant 2 / (√3 · π^(1/4)). Pre-computed to f32 precision.
        const NORM: f32 = 0.867_325_5;
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        let u = t / s;
        let raw = (1.0 - u * u) * (-0.5 * u * u).exp();
        if self.normalize {
            NORM * raw / s.sqrt()
        } else {
            raw
        }
    }

    /// Convolve a discretely-sampled signal against the Mexican-hat
    /// wavelet at this scale, with sample spacing `dt`. Returns a buffer
    /// of the same length as `signal`. Uses zero-boundary by default ;
    /// pass `boundary` to override.
    #[must_use]
    pub fn convolve(&self, signal: &[f32], dt: f32, boundary: BoundaryMode) -> Vec<f32> {
        let n = signal.len();
        if n == 0 {
            return Vec::new();
        }
        // Effective support of the Mexican-hat is roughly ±5σ where σ = scale.
        // Sample taps at integer multiples of dt out to that radius.
        let radius_t = 5.0_f32 * self.scale;
        let radius_n = (radius_t / dt).ceil() as isize;
        let mut out = vec![0.0_f32; n];
        for (i, out_i) in out.iter_mut().enumerate() {
            let mut acc = 0.0_f32;
            for k in (-radius_n)..=radius_n {
                let t = k as f32 * dt;
                let psi = self.evaluate(t);
                let s_idx = i as isize - k;
                let s = crate::boundary::sample_at(signal, s_idx, boundary);
                acc = psi.mul_add(s * dt, acc);
            }
            *out_i = acc;
        }
        out
    }
}

impl Default for MexicanHat {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl WaveletBasis for MexicanHat {
    fn forward_1d(&self, signal: &[f32], boundary: BoundaryMode) -> Vec<f32> {
        // For the WaveletBasis surface, treat the Mexican-hat as a
        // continuous scale-space convolution — no decimation, no QMF
        // pair, no `[approx; detail]` packing. The forward output is
        // simply the wavelet-convolved signal at the configured scale.
        self.convolve(signal, 1.0, boundary)
    }

    fn inverse_1d(&self, coeffs: &[f32], _boundary: BoundaryMode) -> Vec<f32> {
        // Mexican-hat is not invertible from a single scale ; the
        // continuous-wavelet inverse requires integration over all
        // scales. Return the input unchanged so the trait surface is
        // total ; callers that need a real inverse must use the
        // multi-scale Morlet-style synthesis path (out-of-scope here).
        coeffs.to_vec()
    }

    fn is_orthonormal(&self) -> bool {
        false
    }

    fn tap_count(&self) -> usize {
        usize::MAX
    }
}

/// § A Mexican-hat scale-space layer : a wavelet at one specific scale
/// suitable for inclusion in a multi-scale cascade.
#[derive(Debug, Clone, Copy)]
pub struct MexicanHatScale {
    pub scale: f32,
    pub normalize: bool,
}

impl MexicanHatScale {
    #[must_use]
    pub const fn new(scale: f32) -> Self {
        Self {
            scale,
            normalize: true,
        }
    }

    #[must_use]
    pub fn wavelet(&self) -> MexicanHat {
        if self.normalize {
            MexicanHat::new_normalized(self.scale)
        } else {
            MexicanHat::new(self.scale)
        }
    }
}

/// § Free-function convenience : evaluate the (normalized) Mexican-hat
/// at scale `s` and point `t`.
#[must_use]
pub fn mexican_hat(t: f32, s: f32) -> f32 {
    MexicanHat::new_normalized(s).evaluate(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mexican_hat_at_origin_is_one_unnormalized() {
        let mh = MexicanHat::new(1.0);
        let v = mh.evaluate(0.0);
        // ψ(0) = (1 - 0) · exp(0) = 1
        assert!((v - 1.0).abs() < 1e-5, "got {v}");
    }

    #[test]
    fn mexican_hat_at_unit_is_zero() {
        let mh = MexicanHat::new(1.0);
        // ψ(±1) = (1 - 1) · exp(-1/2) = 0
        assert!(mh.evaluate(1.0).abs() < 1e-5);
        assert!(mh.evaluate(-1.0).abs() < 1e-5);
    }

    #[test]
    fn mexican_hat_is_even() {
        let mh = MexicanHat::new(1.0);
        for t in [0.5_f32, 1.5, 2.0, 3.7] {
            let p = mh.evaluate(t);
            let n = mh.evaluate(-t);
            assert!((p - n).abs() < 1e-5, "evenness fails at t = {t}");
        }
    }

    #[test]
    fn mexican_hat_decays_to_zero() {
        let mh = MexicanHat::new(1.0);
        for t in [10.0_f32, 20.0, 50.0] {
            let v = mh.evaluate(t);
            assert!(v.abs() < 1e-4, "at t = {t} : {v}");
        }
    }

    #[test]
    fn mexican_hat_normalized_constant_correct() {
        let mh = MexicanHat::new_normalized(1.0);
        let v = mh.evaluate(0.0);
        // ψ(0) · norm = 1 · (2 / (√3 · π^(1/4))) ≈ 0.867325
        assert!((v - 0.867_325_5).abs() < 1e-5, "got {v}");
    }

    #[test]
    fn mexican_hat_scaled_negative_minimum_at_unit() {
        // The local minima of ψ(t) at t = ±√3 (where d²ψ/dt² = 0).
        let mh = MexicanHat::new(1.0);
        let v_min = mh.evaluate(3.0_f32.sqrt());
        // Should be negative
        assert!(v_min < 0.0, "got {v_min}");
    }

    #[test]
    fn mexican_hat_convolve_constant_signal_zero() {
        // Mexican-hat has zero DC response (negative-2nd-derivative-of-Gaussian
        // integrates to 0). A constant signal should convolve to ~0.
        let mh = MexicanHat::new(1.0);
        let s = vec![5.0_f32; 64];
        let c = mh.convolve(&s, 0.5, BoundaryMode::Periodic);
        // Interior should be ~0 ; boundary terms may not be
        let mid = c.len() / 2;
        assert!(c[mid].abs() < 0.05, "DC response non-zero : {}", c[mid]);
    }

    #[test]
    fn mexican_hat_continuous_wavelet_surface() {
        let mh = MexicanHat::new(1.0);
        assert!(!mh.is_orthonormal());
        assert_eq!(mh.tap_count(), usize::MAX);
    }

    #[test]
    fn mexican_hat_scale_changes_support() {
        let narrow = MexicanHat::new(0.5);
        let wide = MexicanHat::new(2.0);
        // At t = 1 :
        //   narrow : (1 - (1/0.5)²) · exp(-(1/0.5)² / 2) = (1 - 4) · exp(-2) = -3 · exp(-2) ≈ -0.406
        //   wide   : (1 - (1/2)²) · exp(-(1/2)² / 2) = 0.75 · exp(-0.125) ≈ 0.662
        let n_v = narrow.evaluate(1.0);
        let w_v = wide.evaluate(1.0);
        assert!(n_v < 0.0, "narrow at 1.0 should be negative (past zero)");
        assert!(
            w_v > 0.0,
            "wide at 1.0 should be positive (still in main lobe)"
        );
    }

    #[test]
    fn mexican_hat_scale_layer_constructs_wavelet() {
        let layer = MexicanHatScale::new(2.0);
        let w = layer.wavelet();
        assert!((w.scale - 2.0).abs() < 1e-6);
        assert!(w.normalize);
    }

    #[test]
    fn mexican_hat_free_fn_matches_method() {
        let v_fn = mexican_hat(0.5, 1.0);
        let v_method = MexicanHat::new_normalized(1.0).evaluate(0.5);
        assert!((v_fn - v_method).abs() < 1e-6);
    }

    #[test]
    fn mexican_hat_zero_scale_falls_back_to_one() {
        let mh = MexicanHat::new(0.0);
        // Should not divide by zero ; falls back to scale = 1.
        let v = mh.evaluate(0.0);
        assert!(v.is_finite());
    }
}
