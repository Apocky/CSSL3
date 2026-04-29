//! § qmf — Quadrature Mirror Filter (QMF) bank for fast wavelet transforms
//!
//! § PRIMER
//!
//! The fast discrete wavelet transform is, at its core, a pair of
//! filter-bank convolutions :
//!
//! ```text
//! a[k] = Σ_n h[n] · x[2k - n]   (low-pass / approximation)
//! d[k] = Σ_n g[n] · x[2k - n]   (high-pass / detail)
//! ```
//!
//! where `h` is the wavelet's low-pass filter and `g[n] = (-1)^n h[L - 1 - n]`
//! is the QMF "mirror" of `h`. A wavelet basis is orthonormal iff its
//! QMF pair satisfies the perfect-reconstruction constraints :
//!
//! ```text
//! Σ_n h[n] · h[n + 2k] = δ[k]    (orthonormality)
//! Σ_n h[n] = √2                   (zeroth-moment / DC = 1)
//! Σ_n h[n] · g[n] = 0             (cross-orthogonality)
//! ```
//!
//! `Qmf` owns a low-pass filter and derives its high-pass mirror at
//! construction time. `QmfPair::forward` / `QmfPair::inverse` then run
//! the convolution-and-downsample / upsample-and-convolve pair that
//! every fast-DWT implementation in this crate uses under the hood.

use crate::boundary::{sample_at, BoundaryMode};

/// § A quadrature-mirror-filter pair : a low-pass filter `h` plus its
/// derived high-pass mirror `g`.
#[derive(Debug, Clone, Copy)]
pub struct Qmf {
    /// Low-pass / approximation filter taps, ordered `h[0]..h[L - 1]`.
    pub lo: &'static [f32],
}

impl Qmf {
    #[must_use]
    pub const fn new(lo: &'static [f32]) -> Self {
        Self { lo }
    }

    #[must_use]
    pub const fn tap_count(&self) -> usize {
        self.lo.len()
    }

    /// Compute the high-pass tap `g[n] = (-1)^n · h[L - 1 - n]` on demand.
    #[must_use]
    pub fn hi_tap(&self, n: usize) -> f32 {
        let l = self.lo.len();
        debug_assert!(n < l, "qmf hi_tap : n out of range");
        let h = self.lo[l - 1 - n];
        if n & 1 == 0 {
            h
        } else {
            -h
        }
    }

    /// Verify the perfect-reconstruction constraints to within `tol`.
    /// Returns `(orthonormality, dc_normalization, cross_orthogonality)`.
    #[must_use]
    pub fn verify_pr(&self, tol: f32) -> (bool, bool, bool) {
        let l = self.lo.len();
        let mut auto0 = 0.0_f32;
        for n in 0..l {
            auto0 += self.lo[n] * self.lo[n];
        }
        let ortho_self = (auto0 - 1.0).abs() < tol;
        let dc: f32 = self.lo.iter().sum();
        let dc_pass = (dc - 2.0_f32.sqrt()).abs() < tol;
        let mut cross0 = 0.0_f32;
        for n in 0..l {
            cross0 += self.lo[n] * self.hi_tap(n);
        }
        let cross_pass = cross0.abs() < tol;
        (ortho_self, dc_pass, cross_pass)
    }
}

/// § A QMF pair plus the analysis / synthesis convolution kernels.
#[derive(Debug, Clone, Copy)]
pub struct QmfPair {
    pub qmf: Qmf,
}

impl QmfPair {
    #[must_use]
    pub const fn new(qmf: Qmf) -> Self {
        Self { qmf }
    }

    /// § Forward analysis : convolve `signal` against `h` (low) and `g` (high)
    /// and downsample by 2. Output is `[approx; detail]` of total length =
    /// `signal.len()`. The signal length must be a positive even number.
    ///
    /// Convention (Mallat / Daubechies "Ten Lectures") :
    ///   a[k] = Σ_n h̃[n - 2k] · x[n]   where h̃[n] = h[-n] (time-reversed)
    ///   d[k] = Σ_n g̃[n - 2k] · x[n]   where g̃[n] = g[-n]
    /// Equivalently, with h indexed from 0 to L-1 :
    ///   a[k] = Σ_{n=0}^{L-1} h[n] · x[(2k + n) mod N]
    ///   d[k] = Σ_{n=0}^{L-1} g[n] · x[(2k + n) mod N]
    /// where g[n] = (-1)^n h[L-1-n].
    /// The synthesis exactly undoes analysis under periodic boundary +
    /// orthonormal QMF.
    #[must_use]
    pub fn forward(&self, signal: &[f32], boundary: BoundaryMode) -> Vec<f32> {
        let n = signal.len();
        if n == 0 || n % 2 != 0 {
            return signal.to_vec();
        }
        let l = self.qmf.tap_count();
        let half = n / 2;
        let mut out = vec![0.0_f32; n];
        for k in 0..half {
            let mut acc_lo = 0.0_f32;
            let mut acc_hi = 0.0_f32;
            for tap in 0..l {
                let idx = 2 * k as isize + tap as isize;
                let s = sample_at(signal, idx, boundary);
                let h = self.qmf.lo[tap];
                let g = self.qmf.hi_tap(tap);
                acc_lo = h.mul_add(s, acc_lo);
                acc_hi = g.mul_add(s, acc_hi);
            }
            out[k] = acc_lo;
            out[half + k] = acc_hi;
        }
        out
    }

    /// § Inverse synthesis : upsample-by-2 each half then convolve with
    /// the analysis filters, summing the two contributions.
    ///
    /// Convention :
    ///   x[n] = Σ_k h[n - 2k] · a[k] + Σ_k g[n - 2k] · d[k]
    /// where the sum runs over all k such that `0 ≤ n - 2k ≤ L - 1`. With
    /// periodic boundary on the coefficient side, this exactly inverts the
    /// forward pass when h, g form an orthonormal QMF pair.
    #[must_use]
    pub fn inverse(&self, coeffs: &[f32], boundary: BoundaryMode) -> Vec<f32> {
        let n = coeffs.len();
        if n == 0 || n % 2 != 0 {
            return coeffs.to_vec();
        }
        let l = self.qmf.tap_count();
        let half = n / 2;
        let approx = &coeffs[..half];
        let detail = &coeffs[half..];
        let mut out = vec![0.0_f32; n];
        for (i, out_i) in out.iter_mut().enumerate() {
            let mut acc = 0.0_f32;
            for tap in 0..l {
                // We need (i - 2k) = tap → k = (i - tap) / 2, only if (i - tap) is even and ≥ 0
                let m = i as isize - tap as isize;
                if m.rem_euclid(2) != 0 {
                    continue;
                }
                let k = m.div_euclid(2);
                let a = sample_at_half(approx, k, boundary, half as isize);
                let d = sample_at_half(detail, k, boundary, half as isize);
                acc = self.qmf.lo[tap].mul_add(a, acc);
                acc = self.qmf.hi_tap(tap).mul_add(d, acc);
            }
            *out_i = acc;
        }
        out
    }
}

#[inline]
fn sample_at_half(coeffs: &[f32], k: isize, mode: BoundaryMode, half: isize) -> f32 {
    if half == 0 {
        return 0.0;
    }
    if k >= 0 && k < half {
        return coeffs[k as usize];
    }
    match mode {
        BoundaryMode::Periodic => {
            let m = ((k % half) + half) % half;
            coeffs[m as usize]
        }
        BoundaryMode::Symmetric => {
            let period = 2 * half - 2;
            if period == 0 {
                return coeffs[0];
            }
            let mut m = ((k % period) + period) % period;
            if m >= half {
                m = period - m;
            }
            coeffs[m as usize]
        }
        BoundaryMode::Zero => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::haar::HAAR_LO;

    #[test]
    fn haar_qmf_pr_constraints() {
        let qmf = Qmf::new(HAAR_LO);
        let (o, dc, x) = qmf.verify_pr(1e-6);
        assert!(o, "haar : orthonormality");
        assert!(dc, "haar : DC normalization");
        assert!(x, "haar : cross-orthogonality");
    }

    #[test]
    fn qmf_hi_tap_alternation() {
        let qmf = Qmf::new(HAAR_LO);
        assert!((qmf.hi_tap(0) - 1.0_f32 / 2.0_f32.sqrt()).abs() < 1e-6);
        assert!((qmf.hi_tap(1) + 1.0_f32 / 2.0_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn qmf_pair_haar_roundtrip_constant() {
        let pair = QmfPair::new(Qmf::new(HAAR_LO));
        let s = [1.0_f32, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let fwd = pair.forward(&s, BoundaryMode::Periodic);
        let recon = pair.inverse(&fwd, BoundaryMode::Periodic);
        for (a, b) in s.iter().zip(recon.iter()) {
            assert!((a - b).abs() < 1e-5, "constant signal recon : {a} vs {b}");
        }
    }

    #[test]
    fn qmf_pair_haar_roundtrip_ramp() {
        let pair = QmfPair::new(Qmf::new(HAAR_LO));
        let s: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let fwd = pair.forward(&s, BoundaryMode::Periodic);
        let recon = pair.inverse(&fwd, BoundaryMode::Periodic);
        for (a, b) in s.iter().zip(recon.iter()) {
            assert!((a - b).abs() < 1e-4, "ramp recon : {a} vs {b}");
        }
    }

    #[test]
    fn qmf_pair_haar_detail_zero_for_constant() {
        let pair = QmfPair::new(Qmf::new(HAAR_LO));
        let s = [3.5_f32; 8];
        let fwd = pair.forward(&s, BoundaryMode::Periodic);
        for d in &fwd[4..] {
            assert!(d.abs() < 1e-5, "constant signal detail must be zero : {d}");
        }
    }

    #[test]
    fn qmf_pair_empty_signal_passthrough() {
        let pair = QmfPair::new(Qmf::new(HAAR_LO));
        let fwd = pair.forward(&[], BoundaryMode::Periodic);
        assert_eq!(fwd.len(), 0);
    }

    #[test]
    fn qmf_pair_odd_signal_passthrough() {
        let pair = QmfPair::new(Qmf::new(HAAR_LO));
        let s = [1.0, 2.0, 3.0];
        let fwd = pair.forward(&s, BoundaryMode::Periodic);
        assert_eq!(fwd, s.to_vec());
    }
}
