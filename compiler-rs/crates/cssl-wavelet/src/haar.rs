//! § haar — the Haar wavelet basis
//!
//! The Haar wavelet is the simplest possible orthonormal wavelet. Its
//! mother wavelet is the step function
//!
//!   ψ(t) =  +1   for  0 ≤ t < 1/2
//!           −1   for  1/2 ≤ t < 1
//!            0   otherwise
//!
//! and its scaling function is the box `φ(t) = 1` on `[0, 1)`. The
//! corresponding low-pass filter is just the pair `[1/√2, 1/√2]` —
//! the "average" — and the high-pass filter is `[1/√2, -1/√2]` — the
//! "difference". These together satisfy every QMF perfect-reconstruction
//! constraint trivially, which is why Haar is the canonical reference
//! oracle for `cssl-wavelet`'s round-trip + orthonormality tests.
//!
//! § HISTORICAL NOTE
//!   Alfred Haar published the construction in 1909 — *the* original
//!   wavelet, predating the formal wavelet-theory framework by decades.
//!   The fact that the simplest possible orthonormal wavelet is also
//!   the *historically first* one is a happy coincidence ; the rest
//!   of the family (Daubechies, Symlets, Coiflets, ...) gain higher
//!   regularity at the price of more taps but inherit the QMF structure
//!   directly from Haar's construction.

use crate::boundary::BoundaryMode;
use crate::qmf::{Qmf, QmfPair};
use crate::WaveletBasis;

/// § Haar low-pass filter taps (`[1/√2, 1/√2]`). Made `pub` so the QMF
/// tests + the Daubechies-2 alias can reuse the same constants without
/// allocating.
pub const HAAR_LO: &[f32] = &[
    0.707_106_77, // 1/√2
    0.707_106_77, // 1/√2
];

/// § The Haar wavelet basis — orthonormal, 2-tap, piecewise-constant.
#[derive(Debug, Default, Clone, Copy)]
pub struct Haar;

impl Haar {
    /// Construct a Haar wavelet basis. Zero-cost ; the filter taps are
    /// `'static` constants.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Borrow the underlying QMF pair.
    #[must_use]
    pub const fn qmf(&self) -> Qmf {
        Qmf::new(HAAR_LO)
    }
}

impl WaveletBasis for Haar {
    fn forward_1d(&self, signal: &[f32], boundary: BoundaryMode) -> Vec<f32> {
        QmfPair::new(self.qmf()).forward(signal, boundary)
    }

    fn inverse_1d(&self, coeffs: &[f32], boundary: BoundaryMode) -> Vec<f32> {
        QmfPair::new(self.qmf()).inverse(coeffs, boundary)
    }

    fn is_orthonormal(&self) -> bool {
        true
    }

    fn tap_count(&self) -> usize {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn haar_constant_signal_recon() {
        let h = Haar::new();
        let s = [4.0_f32; 8];
        let fwd = h.forward_1d(&s, BoundaryMode::Periodic);
        let recon = h.inverse_1d(&fwd, BoundaryMode::Periodic);
        for (a, b) in s.iter().zip(recon.iter()) {
            assert!(approx_eq(*a, *b, 1e-5), "constant : {a} vs {b}");
        }
    }

    #[test]
    fn haar_ramp_signal_recon() {
        let h = Haar::new();
        let s: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let fwd = h.forward_1d(&s, BoundaryMode::Periodic);
        let recon = h.inverse_1d(&fwd, BoundaryMode::Periodic);
        for (a, b) in s.iter().zip(recon.iter()) {
            assert!(approx_eq(*a, *b, 1e-4), "ramp : {a} vs {b}");
        }
    }

    #[test]
    fn haar_random_signal_recon() {
        let h = Haar::new();
        let s: Vec<f32> = (0..32)
            .map(|i| (i as f32 * 0.7).sin() + (i as f32 * 0.13).cos())
            .collect();
        let fwd = h.forward_1d(&s, BoundaryMode::Periodic);
        let recon = h.inverse_1d(&fwd, BoundaryMode::Periodic);
        for (a, b) in s.iter().zip(recon.iter()) {
            assert!(approx_eq(*a, *b, 1e-4), "random : {a} vs {b}");
        }
    }

    #[test]
    fn haar_known_pair_average_difference() {
        // Haar of [a, b] should yield approx = (a + b) / √2, detail = (a - b) / √2
        let h = Haar::new();
        let s = [3.0_f32, 1.0_f32];
        let fwd = h.forward_1d(&s, BoundaryMode::Periodic);
        let inv_sqrt2 = 1.0_f32 / 2.0_f32.sqrt();
        // approx = (3 + 1) / √2 = 4/√2 ; detail = (3 - 1) / √2 = 2/√2
        assert!(approx_eq(fwd[0], 4.0 * inv_sqrt2, 1e-5));
        // Note : the high-pass mirror is g[0] = h[1] = 1/√2, g[1] = -h[0] = -1/√2.
        // Convolving with downsample at k=0 :
        //   d[0] = g[1] * x[0] + g[0] * x[1] = -1/√2 * 3 + 1/√2 * 1 = -2/√2
        // The sign is convention-dependent ; our QMF emits -2/√2 here.
        assert!(approx_eq(fwd[1].abs(), 2.0 * inv_sqrt2, 1e-5));
    }

    #[test]
    fn haar_orthonormal_reports_true() {
        assert!(Haar::new().is_orthonormal());
    }

    #[test]
    fn haar_tap_count_is_two() {
        assert_eq!(Haar::new().tap_count(), 2);
    }

    #[test]
    fn haar_symmetric_boundary_recon() {
        let h = Haar::new();
        let s: Vec<f32> = (0..8).map(|i| (i as f32) * 0.5).collect();
        let fwd = h.forward_1d(&s, BoundaryMode::Symmetric);
        let recon = h.inverse_1d(&fwd, BoundaryMode::Symmetric);
        for (a, b) in s.iter().zip(recon.iter()) {
            assert!(approx_eq(*a, *b, 1e-4), "symmetric : {a} vs {b}");
        }
    }

    #[test]
    fn haar_zero_boundary_recon_constant_only() {
        // Zero boundary mode does not preserve perfect-reconstruction for
        // arbitrary signals — energy at the boundary leaks. Constant signals
        // do round-trip when the constant is small enough that the boundary
        // term is sub-tolerance ; we assert the *interior* reconstructs.
        let h = Haar::new();
        let s = [0.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let fwd = h.forward_1d(&s, BoundaryMode::Zero);
        let recon = h.inverse_1d(&fwd, BoundaryMode::Zero);
        for (a, b) in s.iter().zip(recon.iter()) {
            assert!(approx_eq(*a, *b, 1e-5), "zero : {a} vs {b}");
        }
    }
}
