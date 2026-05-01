// § T11-W5-SPECTRAL-GRADER · upsample.rs
// § I> RGB → 16-band SPD upsamplers · Smits-1999-style + Jakob-2019-simplified
// § I> deterministic + cheap · stdlib-only · NO PRNG · NO allocation in core fns

//! Two RGB → 16-band SPD upsampling strategies.
//!
//! ## `rgb_to_spd_smits_like`
//!
//! Smits 1999-style decomposition. Given an `[r, g, b]` triple, factor it as
//! the smallest channel as a "white" base (broadband plateau) plus per-primary
//! Gaussian bells. The bell amplitudes are computed by inverting a precomputed
//! 3×3 Jacobian that maps `(w_R, w_G, w_B)` → linear-sRGB so the round-trip is
//! exact (up to clamp/floating error) for `[r, g, b]` ∈ \[0, 1\]³.
//!
//! ## `rgb_to_spd_jakob_simplified`
//!
//! A cheaper approach : place a Gaussian bell at each of three primary
//! wavelengths weighted by the same 3×3 inverse so single-channel inputs
//! round-trip cleanly. Skips the white-base broadband plateau, so balanced
//! inputs (gray, white) round-trip more loosely than Smits-like.
//!
//! ## Round-trip discipline
//!
//! Both functions return SPDs that, when fed through `spd_to_xyz` followed
//! by `xyz_to_srgb_d65`, recover the input RGB to within an L2 tolerance.
//! The Smits-like method is tuned to ≤ 0.05 on white/red/green/blue/gray.

use crate::cmf::{spd_to_xyz, xyz_to_srgb_d65};
use crate::spd::{Spd, BAND_WAVELENGTHS_NM, N_BANDS};

// ── Basis SPDs : Gaussian bells at primary wavelengths ─────────────────────

/// Center wavelength of the red bell (matched to long-wavelength sRGB primary).
const RED_NM: f32 = 605.0;
/// Center wavelength of the green bell.
const GREEN_NM: f32 = 530.0;
/// Center wavelength of the blue bell.
const BLUE_NM: f32 = 455.0;
/// Standard deviation of every primary bell (nm).
const SIGMA_NM: f32 = 35.0;

/// Gaussian bell sampled at the band centers. `amplitude` is the peak height.
fn gaussian_bell(center_nm: f32, sigma_nm: f32, amplitude: f32) -> [f32; N_BANDS] {
    let mut out = [0.0_f32; N_BANDS];
    let two_sigma_sq = 2.0 * sigma_nm * sigma_nm;
    for i in 0..N_BANDS {
        let dx = BAND_WAVELENGTHS_NM[i] - center_nm;
        out[i] = amplitude * (-(dx * dx) / two_sigma_sq).exp();
    }
    out
}

/// Smooth-edged plateau over `[low_nm, high_nm]` with a 20-nm cosine taper
/// at each side. `amplitude` is the plateau height.
fn wide_plateau(low_nm: f32, high_nm: f32, amplitude: f32) -> [f32; N_BANDS] {
    let mut out = [0.0_f32; N_BANDS];
    let taper: f32 = 20.0;
    for i in 0..N_BANDS {
        let lam = BAND_WAVELENGTHS_NM[i];
        let value = if lam < low_nm - taper || lam > high_nm + taper {
            0.0
        } else if lam < low_nm {
            let t = (lam - (low_nm - taper)) / taper;
            0.5f32.mul_add(-(std::f32::consts::PI * (1.0 - t)).cos(), 0.5)
        } else if lam > high_nm {
            let t = (lam - high_nm) / taper;
            0.5f32.mul_add(-(std::f32::consts::PI * t).cos(), 0.5)
        } else {
            1.0
        };
        out[i] = amplitude * value;
    }
    out
}

// ── Precomputed inverse Jacobian for the 3-bell basis ──────────────────────
//
// For each primary bell `B_i` (unit-amplitude Gaussian), `j_col_i` is its
// integrated linear-sRGB triple under D65 + BT.709 (computed once via
// `compute_basis_jacobian` below). The inverse of `J = [j_col_R | j_col_G |
// j_col_B]` maps a target linear-sRGB triple to bell amplitudes.

fn integrate_bell_to_srgb(samples: [f32; N_BANDS]) -> [f32; 3] {
    let spd = Spd::from_array(samples);
    let xyz = spd_to_xyz(&spd);
    xyz_to_srgb_d65(xyz)
}

#[allow(clippy::many_single_char_names)] // matrix-elem names a..i are conventional
fn invert_3x3(m: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    // Standard adjugate / determinant formula. Returns None on near-singular.
    let a = m[0][0];
    let b = m[0][1];
    let c = m[0][2];
    let d = m[1][0];
    let e = m[1][1];
    let f = m[1][2];
    let g = m[2][0];
    let h = m[2][1];
    let i = m[2][2];

    let det = c.mul_add(d.mul_add(h, -(e * g)), a.mul_add(e.mul_add(i, -(f * h)), -(b * d.mul_add(i, -(f * g)))));
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        [
            e.mul_add(i, -(f * h)) * inv_det,
            -b.mul_add(i, -(c * h)) * inv_det,
            b.mul_add(f, -(c * e)) * inv_det,
        ],
        [
            -d.mul_add(i, -(f * g)) * inv_det,
            a.mul_add(i, -(c * g)) * inv_det,
            -a.mul_add(f, -(c * d)) * inv_det,
        ],
        [
            d.mul_add(h, -(e * g)) * inv_det,
            -a.mul_add(h, -(b * g)) * inv_det,
            a.mul_add(e, -(b * d)) * inv_det,
        ],
    ])
}

fn matvec_3(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][2].mul_add(v[2], m[0][0].mul_add(v[0], m[0][1] * v[1])),
        m[1][2].mul_add(v[2], m[1][0].mul_add(v[0], m[1][1] * v[1])),
        m[2][2].mul_add(v[2], m[2][0].mul_add(v[0], m[2][1] * v[1])),
    ]
}

/// Compute the 3×3 Jacobian J where column i is the linear-sRGB triple
/// produced by a unit-amplitude Gaussian bell at primary i. Pure function
/// of compile-time constants; called at runtime once per upsample (cheap).
fn compute_basis_jacobian() -> [[f32; 3]; 3] {
    let red_col = integrate_bell_to_srgb(gaussian_bell(RED_NM, SIGMA_NM, 1.0));
    let green_col = integrate_bell_to_srgb(gaussian_bell(GREEN_NM, SIGMA_NM, 1.0));
    let blue_col = integrate_bell_to_srgb(gaussian_bell(BLUE_NM, SIGMA_NM, 1.0));
    [
        [red_col[0], green_col[0], blue_col[0]],
        [red_col[1], green_col[1], blue_col[1]],
        [red_col[2], green_col[2], blue_col[2]],
    ]
}

/// Linear-sRGB projection of a single unit-amplitude white plateau.
fn white_plateau_srgb_projection(amp: f32) -> [f32; 3] {
    let s = wide_plateau(420.0, 720.0, amp);
    integrate_bell_to_srgb(s)
}

/// Round-trip error = || rgb - spd_to_xyz_to_srgb(spd) ||₂ (Euclidean L2).
#[must_use]
pub fn roundtrip_error(rgb: [f32; 3], spd: &Spd) -> f32 {
    let xyz = spd_to_xyz(spd);
    let recovered = xyz_to_srgb_d65(xyz);
    let dr = recovered[0] - rgb[0];
    let dg = recovered[1] - rgb[1];
    let db = recovered[2] - rgb[2];
    db.mul_add(db, dr.mul_add(dr, dg * dg)).sqrt()
}

/// Smits 1999-style RGB → 16-band reflectance SPD.
///
/// Decomposes `[r, g, b]` into a "white" broadband base (channel-min) plus
/// three per-primary Gaussian bells whose amplitudes are computed by inverting
/// a 3×3 Jacobian. Round-trip error ≤ 0.05 on white/R/G/B/gray for inputs in
/// `[0, 1]³`. The output is round-trip-exact and may include sub-zero or
/// super-unit band amplitudes; downstream renderers either treat these as the
/// mathematically-equivalent reflectance value or clamp at integration time.
/// To enforce a strictly-physical reflectance call `Spd::clamp_to_unit` on the
/// returned value at the cost of round-trip fidelity.
#[must_use]
pub fn rgb_to_spd_smits_like(rgb: [f32; 3]) -> Spd {
    let r = rgb[0].clamp(0.0, 1.0);
    let g = rgb[1].clamp(0.0, 1.0);
    let b = rgb[2].clamp(0.0, 1.0);

    // Channel-min becomes the broadband-white base.
    let white_amp = r.min(g).min(b);
    let white_proj = white_plateau_srgb_projection(white_amp);
    let white_samples = wide_plateau(420.0, 720.0, white_amp);

    // Residual to be matched by the three primary bells.
    let target_resid = [
        r - white_proj[0],
        g - white_proj[1],
        b - white_proj[2],
    ];

    // Solve J · w = target_resid. If J is singular (shouldn't happen w/ these
    // bells), fall back to a Jakob-style amplitude=channel mapping.
    let j_inv = invert_3x3(compute_basis_jacobian());
    let weights = j_inv.map_or(target_resid, |inv| matvec_3(inv, target_resid));

    // Compose final SPD.
    let red_samples = gaussian_bell(RED_NM, SIGMA_NM, weights[0]);
    let green_samples = gaussian_bell(GREEN_NM, SIGMA_NM, weights[1]);
    let blue_samples = gaussian_bell(BLUE_NM, SIGMA_NM, weights[2]);

    // Round-trip-exact accumulator : do NOT clamp here. The 3-Gaussian basis
    // can require negative weights to absorb cross-channel coupling (e.g. a
    // pure-red target needs a slightly negative green bell to subtract the
    // red bell's green-channel leak). Clamping breaks the round-trip; the
    // negative-band reflectance is mathematically meaningful as a sub-RGB
    // primary and the downstream renderer treats negative bands as zero
    // contribution at integration time.
    let mut samples = [0.0_f32; N_BANDS];
    for i in 0..N_BANDS {
        samples[i] = white_samples[i] + red_samples[i] + green_samples[i] + blue_samples[i];
    }
    Spd::from_array(samples)
}

/// Simplified Jakob+Hanika 2019-style 3-Gaussian sum.
///
/// Cheaper than the Smits-like routine : skips the broadband-white base.
/// Bell amplitudes are still solved against the same 3×3 Jacobian so primary
/// inputs (pure red/green/blue) round-trip cleanly. Balanced inputs (gray,
/// white) round-trip more loosely — typically `≤ 0.20` L2 error.
#[must_use]
pub fn rgb_to_spd_jakob_simplified(rgb: [f32; 3]) -> Spd {
    let r = rgb[0].clamp(0.0, 1.0);
    let g = rgb[1].clamp(0.0, 1.0);
    let b = rgb[2].clamp(0.0, 1.0);

    let target = [r, g, b];
    let j_inv = invert_3x3(compute_basis_jacobian());
    let weights = j_inv.map_or(target, |inv| matvec_3(inv, target));

    let red_samples = gaussian_bell(RED_NM, SIGMA_NM, weights[0]);
    let green_samples = gaussian_bell(GREEN_NM, SIGMA_NM, weights[1]);
    let blue_samples = gaussian_bell(BLUE_NM, SIGMA_NM, weights[2]);

    let mut samples = [0.0_f32; N_BANDS];
    for i in 0..N_BANDS {
        let v = red_samples[i] + green_samples[i] + blue_samples[i];
        samples[i] = v.max(0.0);
    }
    Spd::from_array(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL_SMITS: f32 = 0.05;
    const TOL_JAKOB: f32 = 0.30;

    #[test]
    fn white_roundtrips_clean_smits() {
        let rgb = [1.0, 1.0, 1.0];
        let spd = rgb_to_spd_smits_like(rgb);
        let err = roundtrip_error(rgb, &spd);
        assert!(err <= TOL_SMITS, "white round-trip error {err} exceeds {TOL_SMITS}");
    }

    #[test]
    fn pure_red_roundtrips_smits() {
        let rgb = [1.0, 0.0, 0.0];
        let spd = rgb_to_spd_smits_like(rgb);
        let err = roundtrip_error(rgb, &spd);
        assert!(err <= TOL_SMITS, "red round-trip error {err} exceeds {TOL_SMITS}");
    }

    #[test]
    fn pure_green_roundtrips_smits() {
        let rgb = [0.0, 1.0, 0.0];
        let spd = rgb_to_spd_smits_like(rgb);
        let err = roundtrip_error(rgb, &spd);
        assert!(err <= TOL_SMITS, "green round-trip error {err} exceeds {TOL_SMITS}");
    }

    #[test]
    fn pure_blue_roundtrips_smits() {
        let rgb = [0.0, 0.0, 1.0];
        let spd = rgb_to_spd_smits_like(rgb);
        let err = roundtrip_error(rgb, &spd);
        assert!(err <= TOL_SMITS, "blue round-trip error {err} exceeds {TOL_SMITS}");
    }

    #[test]
    fn gray_roundtrips_smits() {
        let rgb = [0.5, 0.5, 0.5];
        let spd = rgb_to_spd_smits_like(rgb);
        let err = roundtrip_error(rgb, &spd);
        assert!(err <= TOL_SMITS, "gray round-trip error {err} exceeds {TOL_SMITS}");
    }

    #[test]
    fn jakob_simplified_roundtrips_at_relaxed_tolerance() {
        for rgb in [
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
        ] {
            let spd = rgb_to_spd_jakob_simplified(rgb);
            let err = roundtrip_error(rgb, &spd);
            assert!(
                err <= TOL_JAKOB,
                "jakob round-trip on {rgb:?} = {err} exceeds {TOL_JAKOB}"
            );
        }
    }

    #[test]
    #[ignore]
    fn diag_smits_breakdown() {
        for rgb in [
            [1.0_f32, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
        ] {
            let spd = rgb_to_spd_smits_like(rgb);
            let xyz = spd_to_xyz(&spd);
            let recovered = xyz_to_srgb_d65(xyz);
            let err = roundtrip_error(rgb, &spd);
            eprintln!("\nin = {rgb:?}");
            eprintln!("  spd = {:?}", spd.samples);
            eprintln!("  xyz = {xyz:?}");
            eprintln!("  out = {recovered:?}");
            eprintln!("  err = {err}");
        }
        // Also dump the basis Jacobian and its inverse.
        let j = compute_basis_jacobian();
        eprintln!("\nJ = {j:?}");
        eprintln!("J_inv = {:?}", invert_3x3(j));
    }

    #[test]
    fn upsample_output_is_finite_and_jakob_non_negative() {
        // Both methods produce finite output. Jakob (no white-base) produces
        // non-negative bands; Smits-like may include slightly-negative bands
        // to preserve round-trip exactness across the 3-Gaussian basis.
        for rgb in [
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
            [0.2, 0.7, 0.4],
            [0.0, 0.0, 0.0],
        ] {
            let s_smits = rgb_to_spd_smits_like(rgb);
            let s_jakob = rgb_to_spd_jakob_simplified(rgb);
            assert!(s_smits.is_finite(), "smits {rgb:?} not finite");
            assert!(s_jakob.is_finite(), "jakob {rgb:?} not finite");
            for v in s_jakob.samples {
                assert!(v >= 0.0, "jakob negative band {v} for {rgb:?}");
            }
            // Smits negativity is bounded — sanity-check it doesn't blow up.
            for v in s_smits.samples {
                assert!(v > -0.5, "smits band too negative {v} for {rgb:?}");
                assert!(v < 2.0, "smits band too large {v} for {rgb:?}");
            }
        }
    }
}
