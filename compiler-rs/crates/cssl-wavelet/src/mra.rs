//! § mra — Multi-Resolution Analysis (MRA)
//!
//! § PRIMER
//!
//! Multi-resolution analysis is the standard wavelet decomposition
//! pipeline : iterate the forward DWT on the *approximation* half at
//! each level, accumulating the *detail* half at each level into a
//! list. After L levels you have :
//!
//! ```text
//! [a_L, d_L, d_{L-1}, ..., d_1]
//! ```
//!
//! where `a_L` is the L-times-coarsened approximation (length n / 2^L)
//! and each `d_k` is the detail at level k (length n / 2^k). The total
//! coefficient count equals the original signal length — wavelet
//! transforms are exactly information-preserving.
//!
//! Reconstruction goes the other way : iteratively combine `a_k +1`
//! and `d_k+1` into `a_k` via the inverse DWT, ending at `a_0` which
//! is the recovered signal.
//!
//! § INTEGRATION
//!
//! `MraCoeffs` is the canonical container ; the `decompose` and
//! `reconstruct` free-functions on `MultiResolution` are the entry
//! points. The radiance-cascade probe pyramid (07_AESTHETIC/02 § II)
//! maps directly : level L of an MRA pyramid is a "cascade-level-L
//! probe field" with 2^L spatial coarsening. The `cascade` module's
//! `CascadeProbePyramid` consumes `MraCoeffs` directly.

use crate::boundary::BoundaryMode;
use crate::WaveletBasis;

/// § Container for the multi-resolution decomposition of a signal.
///
/// Layout : `approx` is the coarsest approximation (length `n / 2^L`) ;
/// `details[0]` is the *finest* detail (length `n / 2`), `details[1]`
/// is the next-coarser detail (length `n / 4`), and so on. `details.len()`
/// equals the number of decomposition levels `L`.
#[derive(Debug, Clone)]
pub struct MraCoeffs {
    /// Coarsest-level approximation : length `n / 2^L`.
    pub approx: Vec<f32>,
    /// Per-level detail coefficients, *finest first* : `details[0]` has
    /// length `n / 2`, `details[1]` has length `n / 4`, ..., `details[L-1]`
    /// has length `n / 2^L`.
    pub details: Vec<Vec<f32>>,
    /// Original signal length.
    pub original_length: usize,
}

impl MraCoeffs {
    /// Number of decomposition levels.
    #[must_use]
    pub fn levels(&self) -> usize {
        self.details.len()
    }

    /// Total stored coefficient count. Should equal `original_length`
    /// for an information-preserving wavelet transform.
    #[must_use]
    pub fn total_coeff_count(&self) -> usize {
        self.approx.len() + self.details.iter().map(Vec::len).sum::<usize>()
    }
}

/// § Multi-resolution analysis dispatcher. Stateless helper struct ;
/// the methods just dispatch to `decompose` / `reconstruct` against the
/// passed-in wavelet basis.
#[derive(Debug, Default, Clone, Copy)]
pub struct MultiResolution;

impl MultiResolution {
    /// § Decompose `signal` into `levels` levels of approximation +
    /// detail coefficients using the given wavelet basis.
    ///
    /// `levels` must be ≥ 1 ; the signal length must be divisible by
    /// `2^levels`. Boundary mode applies to every level's DWT.
    #[must_use]
    pub fn decompose<W: WaveletBasis>(
        signal: &[f32],
        wavelet: &W,
        levels: usize,
        boundary: BoundaryMode,
    ) -> MraCoeffs {
        assert!(levels >= 1, "MRA decompose : levels must be ≥ 1");
        let n = signal.len();
        assert!(
            n.is_power_of_two() || n % (1 << levels) == 0,
            "MRA decompose : signal length must be divisible by 2^levels"
        );

        let mut current = signal.to_vec();
        let mut details: Vec<Vec<f32>> = Vec::with_capacity(levels);
        for _ in 0..levels {
            let fwd = wavelet.forward_1d(&current, boundary);
            let half = fwd.len() / 2;
            let approx_half = fwd[..half].to_vec();
            let detail_half = fwd[half..].to_vec();
            details.push(detail_half);
            current = approx_half;
        }
        MraCoeffs {
            approx: current,
            details,
            original_length: n,
        }
    }

    /// § Reconstruct the original signal from a multi-resolution
    /// coefficient set.
    #[must_use]
    pub fn reconstruct<W: WaveletBasis>(
        coeffs: &MraCoeffs,
        wavelet: &W,
        boundary: BoundaryMode,
    ) -> Vec<f32> {
        let mut current = coeffs.approx.clone();
        // Walk details from coarsest to finest (reverse iter).
        for detail in coeffs.details.iter().rev() {
            let mut packed = Vec::with_capacity(current.len() + detail.len());
            packed.extend_from_slice(&current);
            packed.extend_from_slice(detail);
            current = wavelet.inverse_1d(&packed, boundary);
        }
        current
    }

    /// § Compute the energy at each level. The L²-energy of `signal`
    /// equals the sum of the per-level energies (Parseval's theorem
    /// for orthonormal wavelets).
    #[must_use]
    pub fn level_energies(coeffs: &MraCoeffs) -> Vec<f32> {
        let mut out = Vec::with_capacity(coeffs.levels() + 1);
        out.push(coeffs.approx.iter().map(|x| x * x).sum::<f32>());
        for d in &coeffs.details {
            out.push(d.iter().map(|x| x * x).sum::<f32>());
        }
        out
    }

    /// § Threshold-shrink detail coefficients : zero out any detail
    /// coefficient whose absolute value is below `threshold`. The
    /// classical wavelet-denoising primitive ; can be lossy or
    /// lossless depending on the threshold.
    pub fn threshold(coeffs: &mut MraCoeffs, threshold: f32) {
        for level in &mut coeffs.details {
            for x in level.iter_mut() {
                if x.abs() < threshold {
                    *x = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daubechies::Daubechies;
    use crate::haar::Haar;

    fn approx_eq_slice(a: &[f32], b: &[f32], tol: f32) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < tol)
    }

    #[test]
    fn mra_haar_one_level_roundtrip() {
        let s: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let c = MultiResolution::decompose(&s, &Haar::new(), 1, BoundaryMode::Periodic);
        assert_eq!(c.levels(), 1);
        assert_eq!(c.approx.len(), 4);
        assert_eq!(c.details[0].len(), 4);
        assert_eq!(c.total_coeff_count(), 8);
        let recon = MultiResolution::reconstruct(&c, &Haar::new(), BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-4));
    }

    #[test]
    fn mra_haar_three_level_roundtrip() {
        let s: Vec<f32> = (0..32).map(|i| (i as f32 * 0.3).sin()).collect();
        let c = MultiResolution::decompose(&s, &Haar::new(), 3, BoundaryMode::Periodic);
        assert_eq!(c.levels(), 3);
        assert_eq!(c.approx.len(), 4); // 32 / 8
        assert_eq!(c.details[0].len(), 16); // n/2
        assert_eq!(c.details[1].len(), 8); // n/4
        assert_eq!(c.details[2].len(), 4); // n/8
        assert_eq!(c.total_coeff_count(), 32);
        let recon = MultiResolution::reconstruct(&c, &Haar::new(), BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-3));
    }

    #[test]
    fn mra_daub4_three_level_roundtrip() {
        let s: Vec<f32> = (0..32)
            .map(|i| (i as f32 * 0.2).cos() + (i as f32 * 0.05).sin())
            .collect();
        let c = MultiResolution::decompose(&s, &Daubechies::<4>::new(), 3, BoundaryMode::Periodic);
        let recon =
            MultiResolution::reconstruct(&c, &Daubechies::<4>::new(), BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-3));
    }

    #[test]
    fn mra_daub6_two_level_roundtrip() {
        let s: Vec<f32> = (0..32).map(|i| 0.5 * i as f32).collect();
        let c = MultiResolution::decompose(&s, &Daubechies::<6>::new(), 2, BoundaryMode::Periodic);
        let recon =
            MultiResolution::reconstruct(&c, &Daubechies::<6>::new(), BoundaryMode::Periodic);
        assert!(approx_eq_slice(&s, &recon, 1e-3));
    }

    #[test]
    fn mra_energy_conservation() {
        // Parseval : Σ x² = Σ a² + Σ d_i²
        let s: Vec<f32> = (0..16).map(|i| (i as f32 * 0.5).cos()).collect();
        let c = MultiResolution::decompose(&s, &Haar::new(), 3, BoundaryMode::Periodic);
        let energies = MultiResolution::level_energies(&c);
        let total_decomposed: f32 = energies.iter().sum();
        let total_original: f32 = s.iter().map(|x| x * x).sum();
        assert!(
            (total_decomposed - total_original).abs() < 1e-3,
            "energy : original = {total_original}, decomposed = {total_decomposed}"
        );
    }

    #[test]
    fn mra_constant_signal_only_in_approx() {
        // A constant signal has zero detail coefficients at every level
        // (assuming periodic boundary).
        let s = [5.0_f32; 16];
        let c = MultiResolution::decompose(&s, &Haar::new(), 3, BoundaryMode::Periodic);
        for level in &c.details {
            for x in level {
                assert!(x.abs() < 1e-4, "constant signal detail nonzero : {x}");
            }
        }
    }

    #[test]
    fn mra_threshold_zeroes_small_details() {
        let s: Vec<f32> = (0..16).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut c = MultiResolution::decompose(&s, &Haar::new(), 2, BoundaryMode::Periodic);
        // Threshold to drop any small details
        let detail_count_before: usize = c
            .details
            .iter()
            .map(|v| v.iter().filter(|x| x.abs() > 0.0).count())
            .sum();
        MultiResolution::threshold(&mut c, 0.4);
        let detail_count_after: usize = c
            .details
            .iter()
            .map(|v| v.iter().filter(|x| x.abs() > 0.0).count())
            .sum();
        assert!(detail_count_after <= detail_count_before);
    }

    #[test]
    fn mra_zero_signal_is_zero_coeffs() {
        let s = vec![0.0_f32; 16];
        let c = MultiResolution::decompose(&s, &Haar::new(), 2, BoundaryMode::Periodic);
        for x in &c.approx {
            assert!(x.abs() < 1e-6);
        }
        for level in &c.details {
            for x in level {
                assert!(x.abs() < 1e-6);
            }
        }
    }

    #[test]
    fn mra_levels_count_consistent() {
        let s = vec![1.0_f32; 32];
        let c = MultiResolution::decompose(&s, &Haar::new(), 4, BoundaryMode::Periodic);
        assert_eq!(c.levels(), 4);
        assert_eq!(c.approx.len(), 2);
    }

    #[test]
    fn mra_total_count_preserves_length() {
        let s = vec![1.0_f32; 64];
        let c = MultiResolution::decompose(&s, &Daubechies::<4>::new(), 5, BoundaryMode::Periodic);
        assert_eq!(c.total_coeff_count(), 64);
    }
}
