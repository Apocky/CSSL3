// § T11-W5-SPECTRAL-GRADER · spd.rs
// § I> 16-band Spectral Power Distribution type + arithmetic
// § I> bands @ 380..760 nm @ 25-nm-spacing covers visible (CIE 360-830 truncated)

//! 16-band SPD type and bandwise arithmetic.
//!
//! Bands are sampled at 25-nm intervals starting at 380 nm, ending at 755 nm
//! (16 bands · the 16th sample is at 380 + 15*25 = 755 nm). The visible-range
//! coverage is sufficient for sRGB/BT.709 round-trip work; the CIE CMF tables
//! have effectively zero response outside 380-780 nm.
//!
//! Bandwidth-energy convention : each `samples[i]` represents the average
//! spectral power in the bin centered on `BAND_WAVELENGTHS_NM[i]`. The
//! `spd_to_xyz` integration in [`crate::cmf`] uses Riemann-rectangle summation
//! consistent with this convention.

use serde::{Deserialize, Serialize};

/// Number of spectral bands.
pub const N_BANDS: usize = 16;

/// Center wavelengths (nanometers) of each band. 25 nm spacing, 380..755 nm.
pub const BAND_WAVELENGTHS_NM: [f32; N_BANDS] = [
    380.0, 405.0, 430.0, 455.0, 480.0, 505.0, 530.0, 555.0, 580.0, 605.0, 630.0, 655.0, 680.0,
    705.0, 730.0, 755.0,
];

/// 16-band Spectral Power Distribution.
///
/// Conceptually a reflectance (range \[0, 1\]) when used for material albedo,
/// or a radiance (range \[0, ∞)) when used for emission/illumination. The
/// `is_physical` predicate enforces the reflectance interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spd {
    /// Per-band amplitude.
    pub samples: [f32; N_BANDS],
}

impl Spd {
    /// All-zero SPD.
    #[must_use]
    pub const fn zeros() -> Self {
        Self {
            samples: [0.0; N_BANDS],
        }
    }

    /// Constant unit SPD (every band = 1.0).
    #[must_use]
    pub const fn ones() -> Self {
        Self {
            samples: [1.0; N_BANDS],
        }
    }

    /// Construct from a raw band-array.
    #[must_use]
    pub const fn from_array(samples: [f32; N_BANDS]) -> Self {
        Self { samples }
    }

    /// Clamp every band into the physically-meaningful reflectance range \[0, 1\].
    pub fn clamp_to_unit(&mut self) {
        for s in &mut self.samples {
            *s = s.clamp(0.0, 1.0);
        }
    }

    /// True iff every band is in the closed range \[0, 1\] AND finite.
    #[must_use]
    pub fn is_physical(&self) -> bool {
        self.samples
            .iter()
            .all(|&s| s.is_finite() && (0.0..=1.0).contains(&s))
    }

    /// True iff every band is finite (no NaN/inf).
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.samples.iter().all(|s| s.is_finite())
    }

    /// In-place band-wise add.
    pub fn add_assign(&mut self, other: &Self) {
        for (a, &b) in self.samples.iter_mut().zip(other.samples.iter()) {
            *a += b;
        }
    }

    /// In-place uniform scale.
    pub fn scale(&mut self, factor: f32) {
        for s in &mut self.samples {
            *s *= factor;
        }
    }

    /// Band-wise dot product : Σ a[i] * b[i].
    #[must_use]
    pub fn dot(&self, other: &Self) -> f32 {
        let mut acc: f32 = 0.0;
        for i in 0..N_BANDS {
            acc += self.samples[i] * other.samples[i];
        }
        acc
    }
}

impl Default for Spd {
    fn default() -> Self {
        Self::zeros()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_is_physical() {
        let s = Spd::zeros();
        assert!(s.is_physical());
        assert!(s.is_finite());
        assert!(s.samples.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn ones_is_physical_and_clamps_oob() {
        // ones() itself is at the boundary and considered physical.
        let s = Spd::ones();
        assert!(s.is_physical());
        // Now spike a band beyond 1.0 — that violates `is_physical` until clamped.
        let mut spiked = s;
        spiked.samples[3] = 2.5;
        assert!(!spiked.is_physical());
        spiked.clamp_to_unit();
        assert!(spiked.is_physical());
        assert!((spiked.samples[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clamp_negatives_to_zero() {
        let mut s = Spd::zeros();
        s.samples[0] = -0.4;
        s.samples[5] = -1.2;
        s.samples[10] = 0.7;
        s.clamp_to_unit();
        assert!((s.samples[0] - 0.0).abs() < 1e-6);
        assert!((s.samples[5] - 0.0).abs() < 1e-6);
        assert!((s.samples[10] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn arithmetic_add_scale_dot() {
        let mut a = Spd::ones();
        let b = Spd::ones();
        a.add_assign(&b);
        assert!((a.samples[7] - 2.0).abs() < 1e-6);
        a.scale(0.5);
        assert!((a.samples[7] - 1.0).abs() < 1e-6);
        // ones · ones = N_BANDS
        let d = Spd::ones().dot(&Spd::ones());
        // N_BANDS = 16, well within f32 mantissa precision.
        let n_f = {
            #[allow(clippy::cast_precision_loss)]
            let v = N_BANDS as f32;
            v
        };
        assert!((d - n_f).abs() < 1e-4);
    }

    #[test]
    fn serde_roundtrip() {
        let mut s = Spd::zeros();
        for (i, v) in s.samples.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let f = i as f32;
            *v = f * 0.05;
        }
        let json = serde_json::to_string(&s).expect("serialize");
        let back: Spd = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }

    #[test]
    fn finite_detects_nan_and_inf() {
        let mut s = Spd::zeros();
        s.samples[2] = f32::NAN;
        assert!(!s.is_finite());
        assert!(!s.is_physical());

        let mut s2 = Spd::zeros();
        s2.samples[8] = f32::INFINITY;
        assert!(!s2.is_finite());
        assert!(!s2.is_physical());
    }
}
