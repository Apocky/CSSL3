//! § f64_path — 64-bit-precision wavelet basis surface (feature-gated)
//!
//! This module compiles only when the `f64` feature is enabled. The
//! arithmetic mirrors the f32 path exactly with `f32` → `f64` substitution.
//! Use cases : long signal lengths where roundoff accumulates, audio-band
//! analysis where the dynamic range exceeds f32's mantissa, or scientific
//! workloads that need double-precision determinism.
//!
//! § FILTER COEFFICIENTS
//!   The Daubechies coefficients have many more correct digits than f32
//!   stores ; the f64 path uses higher-precision constants taken from
//!   PyWavelets's reference tables (which themselves source from
//!   Daubechies's *Ten Lectures on Wavelets*, Table 6.1).

/// § Haar low-pass filter (f64).
pub const HAAR_LO_F64: &[f64] = &[
    std::f64::consts::FRAC_1_SQRT_2,
    std::f64::consts::FRAC_1_SQRT_2,
];

/// § Daubechies-4 low-pass filter (f64).
pub const DAUB4_LO_F64: &[f64] = &[
    0.482_962_913_144_534_2,
    0.836_516_303_737_807_9,
    0.224_143_868_042_013_4,
    -0.129_409_522_551_260_4,
];

/// § Daubechies-6 low-pass filter (f64).
pub const DAUB6_LO_F64: &[f64] = &[
    0.332_670_552_950_082_6,
    0.806_891_509_311_092_5,
    0.459_877_502_118_491_6,
    -0.135_011_020_010_254_6,
    -0.085_441_273_882_026_67,
    0.035_226_291_882_100_66,
];

/// § Daubechies-8 low-pass filter (f64).
pub const DAUB8_LO_F64: &[f64] = &[
    0.230_377_813_308_896_5,
    0.714_846_570_552_915_7,
    0.630_880_767_929_858_5,
    -0.027_983_769_416_859_85,
    -0.187_034_811_718_881_4,
    0.030_841_381_835_560_7,
    0.032_883_011_666_982_75,
    -0.010_597_401_785_069_032,
];

/// § Sample at index using the requested boundary mode (f64 path).
fn sample_at_f64(signal: &[f64], idx: isize, mode: crate::boundary::BoundaryMode) -> f64 {
    use crate::boundary::BoundaryMode;
    let n = signal.len() as isize;
    if n == 0 {
        return 0.0;
    }
    if idx >= 0 && idx < n {
        return signal[idx as usize];
    }
    match mode {
        BoundaryMode::Periodic => {
            let m = ((idx % n) + n) % n;
            signal[m as usize]
        }
        BoundaryMode::Symmetric => {
            let period = 2 * n - 2;
            if period == 0 {
                return signal[0];
            }
            let mut m = ((idx % period) + period) % period;
            if m >= n {
                m = period - m;
            }
            signal[m as usize]
        }
        BoundaryMode::Zero => 0.0,
    }
}

/// § Compute the high-pass tap `g[n] = (-1)^n · h[L - 1 - n]` on demand.
fn hi_tap_f64(lo: &[f64], n: usize) -> f64 {
    let l = lo.len();
    let h = lo[l - 1 - n];
    if n & 1 == 0 {
        h
    } else {
        -h
    }
}

/// § Forward DWT (f64). Same algorithm as `qmf::QmfPair::forward`.
#[must_use]
pub fn forward_1d_f64(
    lo: &[f64],
    signal: &[f64],
    boundary: crate::boundary::BoundaryMode,
) -> Vec<f64> {
    let n = signal.len();
    if n == 0 || n % 2 != 0 {
        return signal.to_vec();
    }
    let l = lo.len();
    let half = n / 2;
    let mut out = vec![0.0_f64; n];
    for k in 0..half {
        let mut acc_lo = 0.0_f64;
        let mut acc_hi = 0.0_f64;
        for tap in 0..l {
            let idx = 2 * k as isize + tap as isize;
            let s = sample_at_f64(signal, idx, boundary);
            let h = lo[tap];
            let g = hi_tap_f64(lo, tap);
            acc_lo += h * s;
            acc_hi += g * s;
        }
        out[k] = acc_lo;
        out[half + k] = acc_hi;
    }
    out
}

/// § Inverse DWT (f64). Same algorithm as `qmf::QmfPair::inverse`.
#[must_use]
pub fn inverse_1d_f64(
    lo: &[f64],
    coeffs: &[f64],
    boundary: crate::boundary::BoundaryMode,
) -> Vec<f64> {
    use crate::boundary::BoundaryMode;
    let n = coeffs.len();
    if n == 0 || n % 2 != 0 {
        return coeffs.to_vec();
    }
    let l = lo.len();
    let half = n / 2;
    let approx = &coeffs[..half];
    let detail = &coeffs[half..];
    let mut out = vec![0.0_f64; n];

    let sample_half = |c: &[f64], k: isize| -> f64 {
        if half == 0 {
            return 0.0;
        }
        let h = half as isize;
        if k >= 0 && k < h {
            return c[k as usize];
        }
        match boundary {
            BoundaryMode::Periodic => {
                let m = ((k % h) + h) % h;
                c[m as usize]
            }
            BoundaryMode::Symmetric => {
                let period = 2 * h - 2;
                if period == 0 {
                    return c[0];
                }
                let mut m = ((k % period) + period) % period;
                if m >= h {
                    m = period - m;
                }
                c[m as usize]
            }
            BoundaryMode::Zero => 0.0,
        }
    };

    for (i, out_i) in out.iter_mut().enumerate() {
        let mut acc = 0.0_f64;
        for tap in 0..l {
            let m = i as isize - tap as isize;
            if m.rem_euclid(2) != 0 {
                continue;
            }
            let k = m.div_euclid(2);
            let a = sample_half(approx, k);
            let d = sample_half(detail, k);
            acc += lo[tap] * a;
            acc += hi_tap_f64(lo, tap) * d;
        }
        *out_i = acc;
    }
    out
}

/// § Mexican-hat continuous wavelet (f64).
#[must_use]
pub fn mexican_hat_f64(t: f64, scale: f64) -> f64 {
    const NORM: f64 = 0.867_325_440_851_734_2; // 2 / (√3 · π^(1/4))
    let s = if scale > 0.0 { scale } else { 1.0 };
    let u = t / s;
    NORM * (1.0 - u * u) * (-0.5 * u * u).exp() / s.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary::BoundaryMode;

    fn approx_eq_slice_f64(a: &[f64], b: &[f64], tol: f64) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < tol)
    }

    #[test]
    fn f64_haar_roundtrip() {
        let s: Vec<f64> = (0..16).map(|i| i as f64 * 0.3).collect();
        let fwd = forward_1d_f64(HAAR_LO_F64, &s, BoundaryMode::Periodic);
        let recon = inverse_1d_f64(HAAR_LO_F64, &fwd, BoundaryMode::Periodic);
        assert!(approx_eq_slice_f64(&s, &recon, 1e-12));
    }

    #[test]
    fn f64_daub4_roundtrip() {
        let s: Vec<f64> = (0..32).map(|i| (i as f64 * 0.4).cos()).collect();
        let fwd = forward_1d_f64(DAUB4_LO_F64, &s, BoundaryMode::Periodic);
        let recon = inverse_1d_f64(DAUB4_LO_F64, &fwd, BoundaryMode::Periodic);
        assert!(approx_eq_slice_f64(&s, &recon, 1e-10));
    }

    #[test]
    fn f64_daub6_orthonormality() {
        let h = DAUB6_LO_F64;
        let auto: f64 = h.iter().map(|x| x * x).sum();
        assert!((auto - 1.0).abs() < 1e-10, "got {auto}");
    }

    #[test]
    fn f64_daub8_orthonormality() {
        let h = DAUB8_LO_F64;
        let auto: f64 = h.iter().map(|x| x * x).sum();
        assert!((auto - 1.0).abs() < 1e-10, "got {auto}");
    }

    #[test]
    fn f64_mexican_hat_at_origin() {
        let v = mexican_hat_f64(0.0, 1.0);
        assert!((v - 0.867_325_440_851_734_2).abs() < 1e-12);
    }

    #[test]
    fn f64_mexican_hat_at_unit() {
        let v = mexican_hat_f64(1.0, 1.0);
        assert!(v.abs() < 1e-12);
    }
}
