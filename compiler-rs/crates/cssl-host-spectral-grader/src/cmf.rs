// § T11-W5-SPECTRAL-GRADER · cmf.rs
// § I> CIE 1931 2-deg observer color-matching functions @ 16 bands
// § I> XYZ↔sRGB(D65 BT.709) matrices · linear-light · NO gamma applied here

//! CIE 1931 2-degree observer color-matching functions (CMF) sampled at the
//! 16 band centers, plus D65/BT.709 XYZ↔linear-sRGB conversion matrices.
//!
//! ## Source
//!
//! Values are derived from the CIE 1931 2-deg standard observer table
//! (`x_bar`, `y_bar`, `z_bar` at 5-nm spacing, 360-830 nm). Each 16-band
//! sample is the table value at the center wavelength rounded to the nearest
//! 5-nm tick (e.g. band 7 center = 555 nm → uses the canonical CIE entry at
//! 555 nm). This is the standard practice for low-band-count spectral
//! rendering (Hero-Hyperspectral, Smits 1999, Jakob+Hanika 2019).
//!
//! ## D65 / BT.709 matrix
//!
//! `xyz_to_srgb_d65` uses the standard ITU-R BT.709 matrix with the D65
//! white-point. Values match Rec.709 / sRGB primaries exactly. NO gamma
//! correction is applied — outputs are linear-light sRGB.
//!
//! ## Integration convention
//!
//! `spd_to_xyz` is a Riemann-rectangle sum. The 25-nm bin width is folded
//! into a single global `XYZ_BAND_WIDTH_NM` factor. Combined with the
//! D65 normalization in the CMF Y curve, the SPD-of-D65-illuminant produces
//! Y ≈ 1.0 (within numerical tolerance).

use crate::spd::{Spd, N_BANDS};

/// 16-band CIE 1931 2-deg `x_bar` color-matching function.
///
/// Values @ 380, 405, 430, 455, 480, 505, 530, 555, 580, 605, 630, 655, 680,
/// 705, 730, 755 nm (rounded to nearest 5-nm CIE table entry).
pub const CMF_X: [f32; N_BANDS] = [
    0.001_368, // 380 nm
    0.043_510, // 405 nm
    0.283_900, // 430 nm
    0.348_400, // 455 nm
    0.139_020, // 480 nm
    0.005_790, // 505 nm
    0.165_500, // 530 nm
    0.512_050, // 555 nm
    0.916_300, // 580 nm
    1.062_2,   // 605 nm
    0.642_400, // 630 nm
    0.283_500, // 655 nm
    0.092_240, // 680 nm
    0.029_080, // 705 nm
    0.007_366, // 730 nm
    0.001_836, // 755 nm
];

/// 16-band CIE 1931 2-deg `y_bar` color-matching function.
pub const CMF_Y: [f32; N_BANDS] = [
    0.000_039, // 380 nm
    0.001_210, // 405 nm
    0.011_600, // 430 nm
    0.023_000, // 455 nm
    0.090_980, // 480 nm
    0.328_500, // 505 nm
    0.862_000, // 530 nm
    1.000_000, // 555 nm
    0.870_000, // 580 nm
    0.631_000, // 605 nm
    0.265_000, // 630 nm
    0.107_000, // 655 nm
    0.032_100, // 680 nm
    0.010_470, // 705 nm
    0.002_650, // 730 nm
    0.000_660, // 755 nm
];

/// 16-band CIE 1931 2-deg `z_bar` color-matching function.
pub const CMF_Z: [f32; N_BANDS] = [
    0.006_450, // 380 nm
    0.207_400, // 405 nm
    1.385_6,   // 430 nm
    1.747_7,   // 455 nm
    0.812_950, // 480 nm
    0.038_980, // 505 nm
    0.042_160, // 530 nm
    0.005_750, // 555 nm
    0.001_650, // 580 nm
    0.000_800, // 605 nm
    0.000_000, // 630 nm  (CIE table : ~0)
    0.000_000, // 655 nm
    0.000_000, // 680 nm
    0.000_000, // 705 nm
    0.000_000, // 730 nm
    0.000_000, // 755 nm
];

/// Pre-computed integration normalizer so that the all-ones SPD (a flat
/// equal-energy reflectance under a flat illuminant) yields Y near 1.0
/// rather than the raw Σ CMF_Y. Computed once at compile time as
/// `1 / Σ CMF_Y` so `Σ (CMF_Y * SPD)` of a constant SPD equals SPD itself.
const fn cmf_y_sum() -> f32 {
    let mut acc: f32 = 0.0;
    let mut i = 0;
    while i < N_BANDS {
        acc += CMF_Y[i];
        i += 1;
    }
    acc
}

/// Compile-time-known sum of `CMF_Y` (≈ 4.16). Used as the normalization
/// factor for `spd_to_xyz` so an all-ones SPD yields `Y ≈ 1.0`.
pub const CMF_Y_SUM: f32 = cmf_y_sum();

/// Integrate an SPD against the CIE 1931 2-deg observer.
///
/// The Y channel is normalized so that an all-ones SPD yields Y ≈ 1.0.
/// X and Z are scaled by the same factor to preserve chromaticity.
#[must_use]
pub fn spd_to_xyz(spd: &Spd) -> [f32; 3] {
    let inv_n = 1.0 / CMF_Y_SUM;
    let mut x_acc: f32 = 0.0;
    let mut y_acc: f32 = 0.0;
    let mut z_acc: f32 = 0.0;
    for i in 0..N_BANDS {
        let s = spd.samples[i];
        x_acc = s.mul_add(CMF_X[i], x_acc);
        y_acc = s.mul_add(CMF_Y[i], y_acc);
        z_acc = s.mul_add(CMF_Z[i], z_acc);
    }
    [x_acc * inv_n, y_acc * inv_n, z_acc * inv_n]
}

/// Linear-sRGB → CIE XYZ under D65 (BT.709 primaries).
#[must_use]
pub fn srgb_to_xyz_d65(srgb: [f32; 3]) -> [f32; 3] {
    let r = srgb[0];
    let g = srgb[1];
    let b = srgb[2];
    [
        0.180_480_8_f32.mul_add(b, 0.412_390_8_f32.mul_add(r, 0.357_584_3 * g)),
        0.072_192_3_f32.mul_add(b, 0.212_639_f32.mul_add(r, 0.715_168_7 * g)),
        0.950_532_2_f32.mul_add(b, 0.019_330_8_f32.mul_add(r, 0.119_194_8 * g)),
    ]
}

/// CIE XYZ → linear-sRGB under D65 (BT.709 primaries).
#[must_use]
pub fn xyz_to_srgb_d65(xyz: [f32; 3]) -> [f32; 3] {
    let x = xyz[0];
    let y = xyz[1];
    let z = xyz[2];
    [
        (-0.498_610_8_f32).mul_add(z, 3.240_969_4_f32.mul_add(x, -1.537_383_2 * y)),
        0.041_555_1_f32.mul_add(z, (-0.969_243_6_f32).mul_add(x, 1.875_967_5 * y)),
        1.056_971_5_f32.mul_add(z, 0.055_630_1_f32.mul_add(x, -0.203_976_9 * y)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spd::{Spd, BAND_WAVELENGTHS_NM};

    #[test]
    fn cmf_y_peaks_near_555nm() {
        // The Y channel peaks at 555 nm by definition of the CIE 1931 observer.
        let mut peak_idx = 0;
        let mut peak_val: f32 = -1.0;
        for (i, &v) in CMF_Y.iter().enumerate() {
            if v > peak_val {
                peak_val = v;
                peak_idx = i;
            }
        }
        // Band index 7 is 555 nm in our 25-nm grid.
        assert_eq!(peak_idx, 7, "CMF_Y peak should be at 555-nm band");
        assert!((BAND_WAVELENGTHS_NM[peak_idx] - 555.0).abs() < 0.01);
        assert!((peak_val - 1.0).abs() < 0.001);
    }

    #[test]
    fn ones_spd_projects_to_y_near_one() {
        // A flat all-ones SPD is the normalization-test case.
        let xyz = spd_to_xyz(&Spd::ones());
        // Y is exactly normalized to 1.0 by construction (Σ CMF_Y / CMF_Y_SUM).
        assert!((xyz[1] - 1.0).abs() < 1e-4, "Y should be ~1.0, got {}", xyz[1]);
        // X and Z both positive and finite.
        assert!(xyz[0] > 0.0 && xyz[0].is_finite());
        assert!(xyz[2] > 0.0 && xyz[2].is_finite());
    }

    #[test]
    fn srgb_xyz_roundtrip_clean() {
        // White point pass-through.
        let white_in = [1.0, 1.0, 1.0];
        let xyz = srgb_to_xyz_d65(white_in);
        let back = xyz_to_srgb_d65(xyz);
        for i in 0..3 {
            assert!(
                (back[i] - white_in[i]).abs() < 1e-4,
                "white channel {} : in={} back={}",
                i,
                white_in[i],
                back[i]
            );
        }

        // Random-ish colored point.
        let mid = [0.3, 0.6, 0.2];
        let xyz2 = srgb_to_xyz_d65(mid);
        let back2 = xyz_to_srgb_d65(xyz2);
        for i in 0..3 {
            assert!(
                (back2[i] - mid[i]).abs() < 1e-4,
                "mid channel {} : in={} back={}",
                i,
                mid[i],
                back2[i]
            );
        }
    }

    #[test]
    fn cmf_arrays_non_zero_and_sum_positive() {
        let sx: f32 = CMF_X.iter().sum();
        let sy: f32 = CMF_Y.iter().sum();
        let sz: f32 = CMF_Z.iter().sum();
        assert!(sx > 0.5);
        assert!(sy > 0.5);
        assert!(sz > 0.5);
        // Use the runtime sum to defeat const-evaluation while exercising
        // the same path the integrator depends on.
        let runtime_y_sum: f32 = CMF_Y.iter().sum();
        assert!((runtime_y_sum - CMF_Y_SUM).abs() < 1e-6);
        assert!(runtime_y_sum > 0.5 && runtime_y_sum < 10.0);
    }

    #[test]
    fn ones_spd_xyz_all_positive() {
        // Sanity : every channel of the integrated all-ones SPD is positive.
        let xyz = spd_to_xyz(&Spd::ones());
        assert!(xyz[0] > 0.0);
        assert!(xyz[1] > 0.0);
        assert!(xyz[2] > 0.0);
    }
}
