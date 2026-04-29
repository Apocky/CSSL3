//! § MiseEnAbymeRadiance — per-eye 16-band spectral output buffer
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The Stage-9 output type, per spec § Stage-9.outputs :
//!
//!   ```text
//!   MiseEnAbymeRadiance<2, 16>  (per-eye spectral with-mirror/portal-recursion)
//!   ```
//!
//!   Two eyes (`EYES_PER_FRAME = 2`), 16 hyperspectral bands per eye
//!   (`BANDS_PER_EYE = 16` matching `00_EXOTICISM_PRINCIPLES.csl § V.2`
//!   wavelength schedule {380, 405, 430, ..., 750}nm). The buffer is a
//!   plain stack-allocated `[[f32; 16]; 2]` so it composes by simple
//!   accumulation — no allocation on the hot path.
//!
//! § INVARIANT (energy-conservation)
//!   Per spec § Stage-9.recursion-discipline :
//!     `KAN-attenuation ⊗ guarantees-energy-decay (no-runaway-light-amplification)`
//!
//!   This is enforced by the `WitnessCompositor::accumulate` interaction with
//!   `KanConfidence::evaluate` — every recursion-step's `attenuation_factor`
//!   is bounded `[0.0, 1.0]` so the sum-of-products is bounded by the input
//!   envelope. The `MiseEnAbymeRadiance::peak_band` helper exists so the
//!   compositor's energy-conservation test can panic-or-clamp if a numerical
//!   bug ever produces values above the input envelope.

/// § Number of eyes rendered per frame. Per spec § Stage-9.outputs and
///   § X (effect-row : `MultiView<2>`).
pub const EYES_PER_FRAME: usize = 2;

/// § Number of hyperspectral bands per eye. Matches
///   `00_EXOTICISM_PRINCIPLES.csl § V.2.a` wavelength schedule.
pub const BANDS_PER_EYE: usize = 16;

/// § Per-eye 16-band spectral radiance buffer. Stack-allocated (256B total)
///   so the recursion stack does not allocate on the hot path.
///
///   The shape `[[f32; 16]; 2]` is the canonical render-target layout :
///   the outer dim is the eye index (0 = left, 1 = right), the inner dim
///   is the wavelength band ordered low → high
///   ({380nm, 405nm, ..., 750nm}).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, align(8))]
pub struct MiseEnAbymeRadiance {
    /// § The 2 × 16 spectral matrix.
    pub bands: [[f32; BANDS_PER_EYE]; EYES_PER_FRAME],
}

impl Default for MiseEnAbymeRadiance {
    fn default() -> Self {
        Self::ZERO
    }
}

impl MiseEnAbymeRadiance {
    /// § Zero-radiance constant. Used as the additive identity for the
    ///   compositor's accumulation loop.
    pub const ZERO: Self = Self {
        bands: [[0.0; BANDS_PER_EYE]; EYES_PER_FRAME],
    };

    /// § Construct from explicit per-eye band slices. Useful in tests.
    #[must_use]
    pub const fn new(bands: [[f32; BANDS_PER_EYE]; EYES_PER_FRAME]) -> Self {
        Self { bands }
    }

    /// § Splat a single scalar across all eye/band entries. Useful for
    ///   building the "atmospheric sky" fallback radiance.
    #[must_use]
    pub fn splat(value: f32) -> Self {
        Self {
            bands: [[value; BANDS_PER_EYE]; EYES_PER_FRAME],
        }
    }

    /// § In-place accumulation with attenuation. The contract :
    ///
    ///   ```text
    ///     self += attenuation * other
    ///   ```
    ///
    ///   `attenuation` is the per-bounce KAN-confidence-derived scalar
    ///   produced by [`crate::mise_en_abyme::confidence::KanConfidence`]
    ///   ; it is BOUNDED `[0.0, 1.0]` by the KAN's output activation
    ///   (sigmoid). The combination preserves the energy-conservation
    ///   invariant.
    pub fn accumulate(&mut self, attenuation: f32, other: &Self) {
        // § Defensive clamp on attenuation. The KAN output is sigmoid-
        //   clamped but a numerical-fp drift could push it slightly outside
        //   [0,1] ; we clamp here to make the conservation invariant
        //   bullet-proof.
        let a = attenuation.clamp(0.0, 1.0);
        for eye in 0..EYES_PER_FRAME {
            for band in 0..BANDS_PER_EYE {
                self.bands[eye][band] = a.mul_add(other.bands[eye][band], self.bands[eye][band]);
            }
        }
    }

    /// § Per-eye peak-band scalar — the maximum value across all bands of
    ///   the given eye. Used by the energy-conservation test.
    #[must_use]
    pub fn peak_band(&self, eye: usize) -> f32 {
        debug_assert!(eye < EYES_PER_FRAME);
        let row = &self.bands[eye.min(EYES_PER_FRAME - 1)];
        let mut peak = f32::NEG_INFINITY;
        for &v in row {
            if v > peak {
                peak = v;
            }
        }
        peak
    }

    /// § Per-eye total-luminance integral — sum of all bands. Used as the
    ///   coarse total-energy figure for the WitnessCompositor's frame-stats.
    #[must_use]
    pub fn total_per_eye(&self, eye: usize) -> f32 {
        debug_assert!(eye < EYES_PER_FRAME);
        let row = &self.bands[eye.min(EYES_PER_FRAME - 1)];
        let mut sum = 0.0_f32;
        for &v in row {
            sum += v;
        }
        sum
    }

    /// § Total energy across both eyes — single scalar summary used in the
    ///   energy-conservation invariant + the frame-stats rollup.
    #[must_use]
    pub fn total_energy(&self) -> f32 {
        self.total_per_eye(0) + self.total_per_eye(1)
    }

    /// § Multiply the entire buffer by a scalar in-place. Used for testing
    ///   and for the cost-model's "skip remaining bounces" early-exit path.
    pub fn scale(&mut self, factor: f32) {
        for eye in 0..EYES_PER_FRAME {
            for band in 0..BANDS_PER_EYE {
                self.bands[eye][band] *= factor;
            }
        }
    }

    /// § Approximate-equality with explicit epsilon. Used in tests.
    #[must_use]
    pub fn approx_eq(&self, other: &Self, eps: f32) -> bool {
        for eye in 0..EYES_PER_FRAME {
            for band in 0..BANDS_PER_EYE {
                if (self.bands[eye][band] - other.bands[eye][band]).abs() > eps {
                    return false;
                }
            }
        }
        true
    }

    /// § Predicate : every band is finite (no NaN, no inf). Used in the
    ///   compositor's per-frame post-condition.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        for eye in 0..EYES_PER_FRAME {
            for band in 0..BANDS_PER_EYE {
                if !self.bands[eye][band].is_finite() {
                    return false;
                }
            }
        }
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — energy-conservation + accumulation correctness.
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// § Zero is the additive identity.
    #[test]
    fn zero_is_additive_identity() {
        let mut acc = MiseEnAbymeRadiance::ZERO;
        let other = MiseEnAbymeRadiance::splat(0.5);
        acc.accumulate(1.0, &other);
        assert!(acc.approx_eq(&other, 1e-6));
    }

    /// § Accumulating with attenuation = 0 leaves self unchanged.
    #[test]
    fn zero_attenuation_no_op() {
        let mut acc = MiseEnAbymeRadiance::splat(0.3);
        let other = MiseEnAbymeRadiance::splat(0.7);
        let before = acc;
        acc.accumulate(0.0, &other);
        assert!(acc.approx_eq(&before, 1e-6));
    }

    /// § Energy-conservation : accumulating with attenuation < 1 keeps the
    ///   total energy bounded by the linear envelope.
    #[test]
    fn energy_conservation_under_attenuation() {
        let acc_init = MiseEnAbymeRadiance::splat(0.1);
        let other = MiseEnAbymeRadiance::splat(1.0);
        let mut acc = acc_init;
        let attenuation = 0.5;
        acc.accumulate(attenuation, &other);
        let envelope = acc_init.total_energy() + attenuation * other.total_energy();
        assert!((acc.total_energy() - envelope).abs() < 1e-4);
    }

    /// § Defensive clamp : attenuation > 1.0 is clamped to 1.0 (cannot
    ///   amplify radiance beyond the linear envelope).
    #[test]
    fn attenuation_clamp_above_unity() {
        let acc_init = MiseEnAbymeRadiance::ZERO;
        let other = MiseEnAbymeRadiance::splat(1.0);
        let mut acc = acc_init;
        // § Pass attenuation = 2.0 — should be clamped to 1.0.
        acc.accumulate(2.0, &other);
        let clamped_envelope = 1.0 * other.total_energy();
        assert!((acc.total_energy() - clamped_envelope).abs() < 1e-4);
    }

    /// § Peak-band returns the maximum across the eye's spectral row.
    #[test]
    fn peak_band_returns_max() {
        let mut r = MiseEnAbymeRadiance::ZERO;
        r.bands[0][7] = 0.9;
        r.bands[0][3] = 0.4;
        r.bands[1][12] = 0.6;
        assert!((r.peak_band(0) - 0.9).abs() < 1e-6);
        assert!((r.peak_band(1) - 0.6).abs() < 1e-6);
    }

    /// § total_per_eye sums all bands of the given eye.
    #[test]
    fn total_per_eye_sums_bands() {
        let r = MiseEnAbymeRadiance::splat(0.25);
        let expected_eye_total = 0.25 * BANDS_PER_EYE as f32;
        assert!((r.total_per_eye(0) - expected_eye_total).abs() < 1e-4);
        assert!((r.total_per_eye(1) - expected_eye_total).abs() < 1e-4);
    }

    /// § total_energy spans both eyes.
    #[test]
    fn total_energy_spans_both_eyes() {
        let r = MiseEnAbymeRadiance::splat(0.1);
        let expected = 2.0 * 0.1 * BANDS_PER_EYE as f32;
        assert!((r.total_energy() - expected).abs() < 1e-4);
    }

    /// § scale multiplies all bands.
    #[test]
    fn scale_multiplies_all_bands() {
        let mut r = MiseEnAbymeRadiance::splat(1.0);
        r.scale(0.5);
        assert!(r.approx_eq(&MiseEnAbymeRadiance::splat(0.5), 1e-6));
    }

    /// § is_finite catches NaN/inf.
    #[test]
    fn is_finite_catches_nan() {
        let mut r = MiseEnAbymeRadiance::ZERO;
        assert!(r.is_finite());
        r.bands[0][0] = f32::NAN;
        assert!(!r.is_finite());
    }

    /// § Buffer-shape invariants : EYES_PER_FRAME = 2, BANDS_PER_EYE = 16.
    #[test]
    fn shape_invariants() {
        assert_eq!(EYES_PER_FRAME, 2);
        assert_eq!(BANDS_PER_EYE, 16);
        assert_eq!(
            core::mem::size_of::<MiseEnAbymeRadiance>(),
            EYES_PER_FRAME * BANDS_PER_EYE * 4
        );
    }
}
