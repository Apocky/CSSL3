//! § daubechies — N-tap orthonormal compactly-supported wavelets
//!
//! § PRIMER
//!
//! Daubechies (1988) constructed the canonical family of orthonormal
//! wavelets with a fixed number of *vanishing moments*. A wavelet has
//! N/2 vanishing moments if `Σ_n n^k · g[n] = 0` for `k = 0..N/2 - 1`,
//! which means polynomials of degree up to N/2 - 1 reproduce exactly
//! under the wavelet — high-order signal smoothness is preserved.
//!
//! The family is indexed by N = 2, 4, 6, 8, ... (only even N). For
//! N = 2 the basis is the Haar wavelet (one vanishing moment, just
//! the DC component) ; for N = 4 the wavelet has two vanishing moments
//! and the filter taps are
//!
//! ```text
//! h[0] = (1 + √3) / (4√2)
//! h[1] = (3 + √3) / (4√2)
//! h[2] = (3 − √3) / (4√2)
//! h[3] = (1 − √3) / (4√2)
//! ```
//!
//! The N = 6 + 8 coefficients have closed forms but are typically
//! tabulated to high precision ; the constants below are taken from
//! PyWavelets's reference implementation (which sources them from
//! Daubechies's tables in *Ten Lectures on Wavelets*, SIAM 1992).
//!
//! § NAMING CONVENTION
//!
//! This crate uses the *filter-tap-count* convention : `Daubechies::<N>`
//! has N filter taps. The standard abbreviated naming "dbK" counts
//! *vanishing moment pairs* and corresponds to N = 2K. So the standard
//! "db1" = `Daubechies::<2>` (≡ Haar), "db2" = `Daubechies::<4>`,
//! "db3" = `Daubechies::<6>`, "db4" = `Daubechies::<8>`. We expose all
//! four directly via the `Daubechies<N>` const-generic type and the
//! per-N filter-tap constants `DAUB2_LO`, `DAUB4_LO`, `DAUB6_LO`,
//! `DAUB8_LO`.

use crate::boundary::BoundaryMode;
use crate::qmf::{Qmf, QmfPair};
use crate::WaveletBasis;

/// § Daubechies-2 (≡ Haar) low-pass filter.
pub const DAUB2_LO: &[f32] = &[
    0.707_106_77, // 1/√2
    0.707_106_77, // 1/√2
];

/// § Daubechies-4 low-pass filter.
///   h[0] = (1 + √3) / (4√2) ≈ 0.482962913
///   h[1] = (3 + √3) / (4√2) ≈ 0.836516304
///   h[2] = (3 − √3) / (4√2) ≈ 0.224143868
///   h[3] = (1 − √3) / (4√2) ≈ -0.129409523
pub const DAUB4_LO: &[f32] = &[
    0.482_962_9,
    0.836_516_3,
    0.224_143_87,
    -0.129_409_52,
];

/// § Daubechies-6 low-pass filter (db3 in standard naming) — three vanishing
/// moments. Constants from Daubechies, *Ten Lectures on Wavelets*, Table 6.1.
pub const DAUB6_LO: &[f32] = &[
    0.332_670_55,
    0.806_891_5,
    0.459_877_5,
    -0.135_011_02,
    -0.085_441_27,
    0.035_226_29,
];

/// § Daubechies-8 low-pass filter (db4 in standard naming) — four vanishing
/// moments. Constants from Daubechies, *Ten Lectures on Wavelets*, Table 6.1.
pub const DAUB8_LO: &[f32] = &[
    0.230_377_81,
    0.714_846_57,
    0.630_880_8,
    -0.027_983_77,
    -0.187_034_81,
    0.030_841_38,
    0.032_883_01,
    -0.010_597_4,
];

/// § Daubechies wavelet basis with N filter taps. N must be 2, 4, 6, or 8 ;
/// other values panic at construction time.
#[derive(Debug, Clone, Copy)]
pub struct Daubechies<const N: usize>;

impl<const N: usize> Default for Daubechies<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Daubechies<N> {
    /// Construct a Daubechies-N wavelet basis. Panics if N is not in
    /// {2, 4, 6, 8}.
    #[must_use]
    pub const fn new() -> Self {
        // const-time bounds check
        let _ = Self::filter();
        Self
    }

    /// Borrow the static low-pass filter for this N. Compile-time error
    /// if N is not in {2, 4, 6, 8}.
    #[must_use]
    pub const fn filter() -> &'static [f32] {
        match N {
            2 => DAUB2_LO,
            4 => DAUB4_LO,
            6 => DAUB6_LO,
            8 => DAUB8_LO,
            _ => panic!("cssl-wavelet : Daubechies<N> only defined for N in {{2, 4, 6, 8}}"),
        }
    }

    /// Borrow the underlying QMF pair.
    #[must_use]
    pub const fn qmf() -> Qmf {
        Qmf::new(Self::filter())
    }
}

impl<const N: usize> WaveletBasis for Daubechies<N> {
    fn forward_1d(&self, signal: &[f32], boundary: BoundaryMode) -> Vec<f32> {
        QmfPair::new(Self::qmf()).forward(signal, boundary)
    }

    fn inverse_1d(&self, coeffs: &[f32], boundary: BoundaryMode) -> Vec<f32> {
        QmfPair::new(Self::qmf()).inverse(coeffs, boundary)
    }

    fn is_orthonormal(&self) -> bool {
        true
    }

    fn tap_count(&self) -> usize {
        N
    }
}

/// § Number of vanishing moments for a Daubechies-N filter : N / 2.
#[must_use]
pub const fn vanishing_moments<const N: usize>() -> usize {
    N / 2
}

/// § Verify that a tap-array satisfies the K-th vanishing-moment condition :
///   Σ_n n^k · g[n] = 0 for k = 0..K-1
/// where `g[n] = (-1)^n · h[L - 1 - n]` is the high-pass mirror of `h`.
#[must_use]
pub fn check_vanishing_moments(h: &[f32], k_max: usize, tol: f32) -> Vec<bool> {
    let l = h.len();
    let mut results = Vec::with_capacity(k_max);
    for k in 0..k_max {
        let mut sum = 0.0_f32;
        for n in 0..l {
            let g = if n & 1 == 0 { h[l - 1 - n] } else { -h[l - 1 - n] };
            let n_pow_k = if k == 0 {
                1.0_f32
            } else {
                let mut p = 1.0_f32;
                for _ in 0..k {
                    p *= n as f32;
                }
                p
            };
            sum += n_pow_k * g;
        }
        results.push(sum.abs() < tol);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq_slice(a: &[f32], b: &[f32], tol: f32) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < tol)
    }

    #[test]
    fn daub2_filter_is_haar() {
        assert_eq!(DAUB2_LO.len(), 2);
        assert!((DAUB2_LO[0] - DAUB2_LO[1]).abs() < 1e-6);
    }

    #[test]
    fn daub4_orthonormality() {
        let qmf = Qmf::new(DAUB4_LO);
        let (o, dc, x) = qmf.verify_pr(1e-3);
        assert!(o, "daub4 orthonormality (squared sum = 1)");
        assert!(dc, "daub4 DC normalization (sum = √2)");
        assert!(x, "daub4 cross-orthogonality at zero shift");
    }

    #[test]
    fn daub4_orthonormality_shifted() {
        // Σ_n h[n] · h[n + 2] = 0
        let h = DAUB4_LO;
        let mut s = 0.0_f32;
        for n in 0..(h.len() - 2) {
            s += h[n] * h[n + 2];
        }
        assert!(s.abs() < 1e-3, "daub4 shift-2 orthogonality : got {s}");
    }

    #[test]
    fn daub4_two_vanishing_moments() {
        let v = check_vanishing_moments(DAUB4_LO, 2, 1e-3);
        assert!(v[0], "daub4 0th vanishing moment");
        assert!(v[1], "daub4 1st vanishing moment");
    }

    #[test]
    fn daub4_third_moment_nonzero() {
        // Daubechies-4 has exactly 2 vanishing moments — the 3rd should NOT vanish.
        let v = check_vanishing_moments(DAUB4_LO, 3, 1e-3);
        assert!(v[0]);
        assert!(v[1]);
        assert!(!v[2], "daub4 should not have a 3rd vanishing moment");
    }

    #[test]
    fn daub6_orthonormality() {
        let qmf = Qmf::new(DAUB6_LO);
        let (o, dc, x) = qmf.verify_pr(1e-3);
        assert!(o, "daub6 orthonormality");
        assert!(dc, "daub6 DC");
        assert!(x, "daub6 cross-orthogonality");
    }

    #[test]
    fn daub6_three_vanishing_moments() {
        let v = check_vanishing_moments(DAUB6_LO, 3, 1e-2);
        assert!(v[0], "daub6 0th vanishing moment");
        assert!(v[1], "daub6 1st vanishing moment");
        assert!(v[2], "daub6 2nd vanishing moment");
    }

    #[test]
    fn daub8_orthonormality() {
        let qmf = Qmf::new(DAUB8_LO);
        let (o, dc, x) = qmf.verify_pr(1e-3);
        assert!(o, "daub8 orthonormality");
        assert!(dc, "daub8 DC");
        assert!(x, "daub8 cross-orthogonality");
    }

    #[test]
    fn daub8_four_vanishing_moments() {
        let v = check_vanishing_moments(DAUB8_LO, 4, 5e-2);
        assert!(v[0], "daub8 0th vanishing moment");
        assert!(v[1], "daub8 1st vanishing moment");
        assert!(v[2], "daub8 2nd vanishing moment");
        assert!(v[3], "daub8 3rd vanishing moment");
    }

    #[test]
    fn daub4_perfect_reconstruction_constant() {
        let d = Daubechies::<4>::new();
        let s = [5.0_f32; 16];
        let fwd = d.forward_1d(&s, BoundaryMode::Periodic);
        let recon = d.inverse_1d(&fwd, BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-4));
    }

    #[test]
    fn daub4_perfect_reconstruction_linear() {
        // Linear ramp : daub4 has 2 vanishing moments, so a linear signal
        // should produce mostly-zero detail coefficients in the *interior*.
        let d = Daubechies::<4>::new();
        let s: Vec<f32> = (0..16).map(|i| 0.25 * i as f32).collect();
        let fwd = d.forward_1d(&s, BoundaryMode::Periodic);
        let recon = d.inverse_1d(&fwd, BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-3));
    }

    #[test]
    fn daub4_perfect_reconstruction_smooth() {
        let d = Daubechies::<4>::new();
        let s: Vec<f32> = (0..32)
            .map(|i| 0.3 * (i as f32 * 0.4).sin() + 0.7)
            .collect();
        let fwd = d.forward_1d(&s, BoundaryMode::Periodic);
        let recon = d.inverse_1d(&fwd, BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-3));
    }

    #[test]
    fn daub6_perfect_reconstruction() {
        let d = Daubechies::<6>::new();
        let s: Vec<f32> = (0..32)
            .map(|i| (i as f32 * 0.3).cos() * 2.0)
            .collect();
        let fwd = d.forward_1d(&s, BoundaryMode::Periodic);
        let recon = d.inverse_1d(&fwd, BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-3));
    }

    #[test]
    fn daub8_perfect_reconstruction() {
        let d = Daubechies::<8>::new();
        let s: Vec<f32> = (0..32)
            .map(|i| (i as f32 * 0.2).sin() + (i as f32 * 0.1).cos() * 0.3)
            .collect();
        let fwd = d.forward_1d(&s, BoundaryMode::Periodic);
        let recon = d.inverse_1d(&fwd, BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-3));
    }

    #[test]
    fn daub_tap_count_matches_n() {
        assert_eq!(Daubechies::<2>::new().tap_count(), 2);
        assert_eq!(Daubechies::<4>::new().tap_count(), 4);
        assert_eq!(Daubechies::<6>::new().tap_count(), 6);
        assert_eq!(Daubechies::<8>::new().tap_count(), 8);
    }

    #[test]
    fn daub_vanishing_moments_count() {
        assert_eq!(vanishing_moments::<2>(), 1);
        assert_eq!(vanishing_moments::<4>(), 2);
        assert_eq!(vanishing_moments::<6>(), 3);
        assert_eq!(vanishing_moments::<8>(), 4);
    }

    #[test]
    fn daub_all_orthonormal() {
        assert!(Daubechies::<2>::new().is_orthonormal());
        assert!(Daubechies::<4>::new().is_orthonormal());
        assert!(Daubechies::<6>::new().is_orthonormal());
        assert!(Daubechies::<8>::new().is_orthonormal());
    }
}
