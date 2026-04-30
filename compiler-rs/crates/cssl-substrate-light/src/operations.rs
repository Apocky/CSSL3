//! § operations — ApockyLight construction + composition + querying operators.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The canonical operator-suite for [`crate::light::ApockyLight`].
//!   Construction helpers produce well-formed quanta from physical
//!   primitives (Planck blackbody, monochromatic delta, D65 illuminant).
//!   Composition operators implement the linear-radiance algebra : add /
//!   scale / attenuate / Mueller-matrix application ; all produce fresh
//!   quanta and never aliase the inputs.
//!
//! § SPEC
//!   - `specs/34_APOCKY_LIGHT.csl` § OPERATIONS — full operator surface.
//!   - `specs/30_SUBSTRATE_v3.csl` § APOCKY-LIGHT § Composition.
//!   - `specs/36_CFER_RENDERER.csl` § Field-evolution PDE — the linear-
//!     radiance-transport algebra these operators implement.
//!
//! § DESIGN-DISCIPLINE
//!   - All operators are PURE : zero side-effects + zero hidden state.
//!   - Composition preserves IFC-flow invariants via [`crate::ifc_flow`].
//!   - Construction validators return typed errors per [`LightConstructionError`].
//!   - Composition validators return typed errors per [`LightCompositionError`].

use thiserror::Error;

use crate::ifc_flow::{combine_caps_raw, IfcFlowError};
use crate::light::{
    ApockyLight, EvidenceGlyph, ACCOMPANIMENT_COUNT, LAMBDA_MAX_NM, LAMBDA_MIN_NM,
};

// ───────────────────────────────────────────────────────────────────────────
// § Error types
// ───────────────────────────────────────────────────────────────────────────

/// § Errors raised by light-composition operators.
///
/// Composition can fail when the two quanta have incompatible capability
/// handles (different Pony-cap subsets that don't combine cleanly), or
/// when a Mueller-matrix application produces a non-physical Stokes vector
/// (reconstructed s3² < 0).
#[derive(Debug, Error, PartialEq)]
pub enum LightCompositionError {
    /// § Two quanta carry incompatible capability handles.
    #[error("incompatible capability handles : a={a:#x} b={b:#x}")]
    IncompatibleCaps {
        /// First handle.
        a: u32,
        /// Second handle.
        b: u32,
    },

    /// § IFC-flow check refused the composition.
    #[error("IFC-flow check refused : {0}")]
    IfcFlow(#[from] IfcFlowError),

    /// § Mueller-matrix application produced non-physical state
    ///   (DoP > 1.0 or s3² < 0 after composition).
    #[error("Mueller-matrix composition produced non-physical Stokes state")]
    NonPhysicalStokes,

    /// § Scale factor was negative or NaN.
    #[error("scale factor must be non-negative + finite, got {0}")]
    InvalidScale(f32),

    /// § Attenuation factor was outside [0.0, 1.0].
    #[error("attenuation factor must be in [0.0, 1.0], got {0}")]
    InvalidAttenuation(f32),
}

/// § Errors raised by light-construction validators.
#[derive(Debug, Error, PartialEq)]
pub enum LightConstructionError {
    /// § Hero wavelength is out of physical range [LAMBDA_MIN_NM, LAMBDA_MAX_NM].
    #[error("hero wavelength {wavelength} nm out of range [{LAMBDA_MIN_NM}, {LAMBDA_MAX_NM}]")]
    WavelengthOutOfRange {
        /// Offending wavelength.
        wavelength: f32,
    },

    /// § DoP magnitude > 1.0 (non-physical polarization).
    #[error("DoP {0} out of range [0.0, 1.0]")]
    DopOutOfRange(f32),

    /// § Hero radiance is NaN or negative.
    #[error("hero radiance {0} is NaN or negative")]
    NegativeRadiance(f32),

    /// § Blackbody temperature is non-positive.
    #[error("blackbody temperature {0} K must be positive")]
    InvalidTemperature(f32),
}

// ───────────────────────────────────────────────────────────────────────────
// § Construction helpers
// ───────────────────────────────────────────────────────────────────────────

/// § Construct a zero / null-light quantum. Equivalent to
///   [`ApockyLight::zero`] but exposed in operations for symmetry.
#[must_use]
pub fn zero() -> ApockyLight {
    ApockyLight::zero()
}

/// § Construct a monochromatic single-wavelength delta-quantum.
///
/// `radiance` is the hero-band irradiance (W·sr⁻¹·m⁻²·nm⁻¹). All
/// accompaniment bands are zero (delta-function). Direction defaults to
/// `+z` ; caller may rotate post-construction.
pub fn monochromatic(
    radiance: f32,
    lambda_nm: f32,
    direction: [f32; 3],
    cap_handle: u32,
) -> Result<ApockyLight, LightConstructionError> {
    if radiance.is_nan() || radiance < 0.0 {
        return Err(LightConstructionError::NegativeRadiance(radiance));
    }
    if lambda_nm < LAMBDA_MIN_NM || lambda_nm > LAMBDA_MAX_NM {
        return Err(LightConstructionError::WavelengthOutOfRange {
            wavelength: lambda_nm,
        });
    }
    Ok(ApockyLight::new(
        radiance,
        lambda_nm,
        [0.0; ACCOMPANIMENT_COUNT],
        0.0,
        0.0,
        0.0,
        direction,
        0,
        EvidenceGlyph::Default,
        cap_handle,
    ))
}

/// § Construct a Planck-blackbody radiator quantum at temperature `T_kelvin`.
///
/// The hero-band wavelength is set to Wien's-displacement-law peak λ_peak ≈
/// 2_897_768.5 / T (μK·m → nm conversion built in). Accompaniment bands
/// are evaluated at λ_peak ± k·25 nm using the Planck spectral-radiance
/// formula B(λ, T).
pub fn blackbody(t_kelvin: f32, cap_handle: u32) -> Result<ApockyLight, LightConstructionError> {
    if t_kelvin <= 0.0 || t_kelvin.is_nan() {
        return Err(LightConstructionError::InvalidTemperature(t_kelvin));
    }
    // Wien-displacement λ_peak in nm = 2_897_768.5 / T(K).
    let lambda_peak_nm = (2_897_768.5_f32 / t_kelvin)
        .max(LAMBDA_MIN_NM)
        .min(LAMBDA_MAX_NM);

    // Hero radiance = Planck spectral radiance at λ_peak.
    let hero_radiance = planck_spectral_radiance_nm(lambda_peak_nm, t_kelvin);

    // Accompaniment bands at λ_peak ± k·25 nm.
    let mut accomp = [0.0_f32; ACCOMPANIMENT_COUNT];
    for i in 0..ACCOMPANIMENT_COUNT {
        let dl = ((i as f32) + 1.0) * 25.0;
        let sign = if i & 1 == 0 { -1.0 } else { 1.0 };
        let l = (lambda_peak_nm + sign * dl).max(LAMBDA_MIN_NM).min(LAMBDA_MAX_NM);
        accomp[i] = planck_spectral_radiance_nm(l, t_kelvin);
    }

    Ok(ApockyLight::new(
        hero_radiance,
        lambda_peak_nm,
        accomp,
        0.0, // unpolarized blackbody
        0.0,
        0.0,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        cap_handle,
    ))
}

/// § Construct a D65 illuminant reference quantum (~6500 K daylight).
///
/// Hero λ = 550 nm (CIE-D65 reference white-point). Accompaniment bands
/// at 525, 575, 500, 600, 475, 625, 450, 650 nm with normalized D65 weights.
pub fn d65(intensity: f32, cap_handle: u32) -> Result<ApockyLight, LightConstructionError> {
    if intensity.is_nan() || intensity < 0.0 {
        return Err(LightConstructionError::NegativeRadiance(intensity));
    }
    // D65 illuminant relative power-distribution (normalized at 555 nm = 1.0).
    // Tabulated values at the 8 accompaniment-band centers.
    // Order (matches accompaniment[i] = hero ± (i+1)*25 nm with alternating sign) :
    // i=0: 525 nm  (D65 ≈ 105.0)
    // i=1: 575 nm  (D65 ≈ 100.0)
    // i=2: 500 nm  (D65 ≈ 110.0)
    // i=3: 600 nm  (D65 ≈ 96.0)
    // i=4: 475 nm  (D65 ≈ 116.0)
    // i=5: 625 nm  (D65 ≈ 87.0)
    // i=6: 450 nm  (D65 ≈ 117.0)
    // i=7: 650 nm  (D65 ≈ 80.0)
    // Normalized so hero(550) = 1.04 ≈ 1.0 ; we scale by `intensity`.
    const HERO_D65: f32 = 1.04;
    let accomp_d65 = [1.05, 1.00, 1.10, 0.96, 1.16, 0.87, 1.17, 0.80];
    let mut accomp = [0.0_f32; ACCOMPANIMENT_COUNT];
    for i in 0..ACCOMPANIMENT_COUNT {
        accomp[i] = accomp_d65[i] * intensity;
    }
    Ok(ApockyLight::new(
        HERO_D65 * intensity,
        550.0,
        accomp,
        0.0,
        0.0,
        0.0,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        cap_handle,
    ))
}

// ───────────────────────────────────────────────────────────────────────────
// § Composition operators
// ───────────────────────────────────────────────────────────────────────────

/// § Linear addition of two ApockyLight quanta.
///
/// Sums hero-radiance + accompaniment-bands per-band. The output adopts the
/// hero-wavelength of the brighter input (max-radiance arg). Polarization
/// is averaged-by-radiance-weight. Direction is taken from the brighter
/// input (the dominant carrier wins). Cap-handles are combined per
/// [`combine_caps_raw`] ; if the combination is invalid, returns
/// `IncompatibleCaps`.
///
/// The output's evidence-glyph is the higher-priority of the two inputs'
/// glyphs (Forbidden > Alert > Uncertain > Increasing > Decreasing >
/// Rejected > Trusted > Default).
pub fn add(a: &ApockyLight, b: &ApockyLight) -> Result<ApockyLight, LightCompositionError> {
    let cap_handle = combine_caps_raw(a.cap_handle(), b.cap_handle())?;
    let total = a.intensity() + b.intensity();
    let (hero_lambda, direction) = if a.intensity() >= b.intensity() {
        (a.lambda_nm(), a.direction())
    } else {
        (b.lambda_nm(), b.direction())
    };

    // Per-band accompaniment sum.
    let aa = a.accompaniments();
    let bb = b.accompaniments();
    let mut accomp = [0.0_f32; ACCOMPANIMENT_COUNT];
    for i in 0..ACCOMPANIMENT_COUNT {
        accomp[i] = aa[i] + bb[i];
    }

    // Radiance-weighted Stokes mixing.
    let stokes_a = a.stokes();
    let stokes_b = b.stokes();
    let (s1, s2, dop) = if total > 0.0 {
        let s1_total = stokes_a[1] + stokes_b[1];
        let s2_total = stokes_a[2] + stokes_b[2];
        let s3_total = stokes_a[3] + stokes_b[3];
        let dop_combined =
            ((s1_total * s1_total + s2_total * s2_total + s3_total * s3_total).sqrt() / total)
                .min(1.0);
        // Normalize linear-Stokes by total intensity to recover relative q1.7 vector.
        (s1_total / total, s2_total / total, dop_combined)
    } else {
        (0.0, 0.0, 0.0)
    };

    // Evidence-glyph priority : Forbidden > Alert > Uncertain > Increasing >
    // Decreasing > Rejected > Trusted > Default.
    let glyph = max_priority_glyph(a.evidence(), b.evidence());

    Ok(ApockyLight::new(
        total,
        hero_lambda,
        accomp,
        dop,
        s1,
        s2,
        direction,
        a.kan_band_handle().max(b.kan_band_handle()),
        glyph,
        cap_handle,
    ))
}

/// § Scale a quantum by a non-negative scalar factor.
///
/// All radiance values (hero + accompaniments) are multiplied by `factor`.
/// Polarization is preserved (DoP is intensity-relative). Direction +
/// cap_handle + evidence are preserved. `factor == 0.0` produces a zero
/// quantum with the original direction + cap.
pub fn scale(a: &ApockyLight, factor: f32) -> Result<ApockyLight, LightCompositionError> {
    if factor.is_nan() || factor < 0.0 {
        return Err(LightCompositionError::InvalidScale(factor));
    }

    let mut accomp = a.accompaniments();
    for v in accomp.iter_mut() {
        *v *= factor;
    }

    Ok(ApockyLight::new(
        a.intensity() * factor,
        a.lambda_nm(),
        accomp,
        a.dop(),
        ((a.dop_packed >> 16) & 0xFF) as i8 as f32 / 127.0,
        ((a.dop_packed >> 24) & 0xFF) as i8 as f32 / 127.0,
        a.direction(),
        a.kan_band_handle(),
        a.evidence(),
        a.cap_handle(),
    ))
}

/// § Attenuate a quantum by a transmittance factor t ∈ [0.0, 1.0].
///
/// Equivalent to `scale(a, transmittance)` but with a tighter input
/// validator + canonical evidence-glyph update : if the transmittance
/// is < 0.01, the output evidence becomes [`EvidenceGlyph::Decreasing`]
/// to drive the CFER iteration to re-evaluate the now-near-dark region.
pub fn attenuate(
    a: &ApockyLight,
    transmittance: f32,
) -> Result<ApockyLight, LightCompositionError> {
    if transmittance.is_nan() || !(0.0..=1.0).contains(&transmittance) {
        return Err(LightCompositionError::InvalidAttenuation(transmittance));
    }
    let mut out = scale(a, transmittance)?;
    if transmittance < 0.01 {
        out.set_evidence(EvidenceGlyph::Decreasing);
    }
    Ok(out)
}

/// § Apply a 4×4 Mueller matrix to the quantum's Stokes vector.
///
/// Mueller matrices model polarization-dependent transmission / reflection
/// (e.g. dielectric-Fresnel + birefringent-stack). The matrix is in
/// row-major order : `M[row][col]`.
///
/// The output preserves the hero-wavelength + direction + accompaniment
/// bands ; only the Stokes vector is transformed. If the result is non-
/// physical (DoP > 1 or s3² < 0), returns [`LightCompositionError::NonPhysicalStokes`].
pub fn mueller_apply(
    a: &ApockyLight,
    m: [[f32; 4]; 4],
) -> Result<ApockyLight, LightCompositionError> {
    let s_in = a.stokes();
    let mut s_out = [0.0_f32; 4];
    for i in 0..4 {
        s_out[i] = m[i][0] * s_in[0] + m[i][1] * s_in[1] + m[i][2] * s_in[2] + m[i][3] * s_in[3];
    }
    let s0 = s_out[0].max(0.0);
    if s0 < 1e-12 {
        // Output is fully extinguished ; return a dark quantum that
        // preserves direction + cap + lambda.
        return scale(a, 0.0);
    }
    let s1 = s_out[1] / s0;
    let s2 = s_out[2] / s0;
    let s3 = s_out[3] / s0;
    let dop_sq = s1 * s1 + s2 * s2 + s3 * s3;
    if dop_sq > 1.0 + 1e-3 {
        return Err(LightCompositionError::NonPhysicalStokes);
    }
    let dop = dop_sq.sqrt().min(1.0);

    // Construct output preserving lambda + direction + accompaniments + cap.
    let accomp = a.accompaniments();
    Ok(ApockyLight::new(
        s0,
        a.lambda_nm(),
        accomp,
        dop,
        s1,
        s2,
        a.direction(),
        a.kan_band_handle(),
        a.evidence(),
        a.cap_handle(),
    ))
}

// ───────────────────────────────────────────────────────────────────────────
// § Internal helpers
// ───────────────────────────────────────────────────────────────────────────

/// § Planck spectral radiance per nm at wavelength λ (nm) and temperature
///   T (K), returned as W·sr⁻¹·m⁻²·nm⁻¹.
///
/// B(λ, T) = (2 h c²) / (λ⁵ × (exp(hc/λkT) - 1))
///
/// We use the cgs-friendly form with λ in meters internally then scale to nm.
fn planck_spectral_radiance_nm(lambda_nm: f32, t_kelvin: f32) -> f32 {
    let h = 6.626_07_e-34_f32; // Planck constant
    let c = 2.997_924_58_e8_f32; // speed of light
    let k = 1.380_649_e-23_f32; // Boltzmann
    let lambda_m = lambda_nm * 1.0e-9;
    let exponent = (h * c) / (lambda_m * k * t_kelvin);
    let denom = (exponent.exp() - 1.0).max(1e-30);
    let b_per_m = (2.0 * h * c * c) / (lambda_m.powi(5) * denom);
    // Convert per-m → per-nm by multiplying by 1e-9.
    b_per_m * 1.0e-9
}

/// § Glyph-priority ordering for evidence-glyph composition. Returns the
///   higher-priority of two glyphs.
///
/// Priority : Forbidden > Alert > Uncertain > Increasing > Decreasing >
///            Rejected > Trusted > Default.
fn max_priority_glyph(a: EvidenceGlyph, b: EvidenceGlyph) -> EvidenceGlyph {
    let pa = glyph_priority(a);
    let pb = glyph_priority(b);
    if pa >= pb {
        a
    } else {
        b
    }
}

const fn glyph_priority(g: EvidenceGlyph) -> u8 {
    match g {
        EvidenceGlyph::Forbidden => 7,
        EvidenceGlyph::Alert => 6,
        EvidenceGlyph::Uncertain => 5,
        EvidenceGlyph::Increasing => 4,
        EvidenceGlyph::Decreasing => 3,
        EvidenceGlyph::Rejected => 2,
        EvidenceGlyph::Trusted => 1,
        EvidenceGlyph::Default => 0,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monochromatic_quantum_basic() {
        let l = monochromatic(2.5, 600.0, [0.0, 0.0, 1.0], 0).unwrap();
        assert!((l.intensity() - 2.5).abs() < 1e-3);
        assert!((l.lambda_nm() - 600.0).abs() < 1e-3);
        // All accompaniment bands should be zero (delta).
        for v in l.accompaniments() {
            assert!(v.abs() < 1e-3);
        }
    }

    #[test]
    fn monochromatic_rejects_bad_wavelength() {
        assert!(matches!(
            monochromatic(1.0, 100.0, [0.0, 0.0, 1.0], 0),
            Err(LightConstructionError::WavelengthOutOfRange { .. })
        ));
        assert!(matches!(
            monochromatic(1.0, 5000.0, [0.0, 0.0, 1.0], 0),
            Err(LightConstructionError::WavelengthOutOfRange { .. })
        ));
    }

    #[test]
    fn monochromatic_rejects_negative_radiance() {
        assert!(matches!(
            monochromatic(-1.0, 550.0, [0.0, 0.0, 1.0], 0),
            Err(LightConstructionError::NegativeRadiance(_))
        ));
    }

    #[test]
    fn blackbody_solar_temperature() {
        // Sun ≈ 5778 K ; Wien-peak ≈ 501 nm (peak in visible-blue-green).
        let l = blackbody(5778.0, 0).unwrap();
        let peak_nm = 2_897_768.5_f32 / 5778.0;
        assert!((l.lambda_nm() - peak_nm).abs() < 1.0);
        assert!(l.intensity() > 0.0);
    }

    #[test]
    fn blackbody_rejects_zero_temp() {
        assert!(matches!(
            blackbody(0.0, 0),
            Err(LightConstructionError::InvalidTemperature(_))
        ));
        assert!(matches!(
            blackbody(-100.0, 0),
            Err(LightConstructionError::InvalidTemperature(_))
        ));
    }

    #[test]
    fn d65_illuminant_basic() {
        let l = d65(1.0, 0).unwrap();
        assert!((l.lambda_nm() - 550.0).abs() < 1e-3);
        assert!(l.intensity() > 0.0);
        // All 8 accompaniment bands should be positive.
        for v in l.accompaniments() {
            assert!(v > 0.0);
        }
    }

    #[test]
    fn add_two_monochromatic_lights() {
        let a = monochromatic(2.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
        let b = monochromatic(3.0, 600.0, [0.0, 0.0, 1.0], 0).unwrap();
        let sum = add(&a, &b).unwrap();
        assert!((sum.intensity() - 5.0).abs() < 1e-3);
        // Brighter input is `b` (3.0 > 2.0), so hero-lambda = 600.
        assert!((sum.lambda_nm() - 600.0).abs() < 1e-3);
    }

    #[test]
    fn scale_preserves_direction_and_cap() {
        let a = monochromatic(2.0, 550.0, [1.0, 0.0, 0.0], 0xCAFE_BABE).unwrap();
        let s = scale(&a, 0.5).unwrap();
        assert!((s.intensity() - 1.0).abs() < 1e-3);
        // Direction preserved (octahedral round-trip ~1°).
        let dir = s.direction();
        assert!(dir[0] > 0.95);
        assert_eq!(s.cap_handle(), 0xCAFE_BABE);
    }

    #[test]
    fn scale_rejects_negative_factor() {
        let a = monochromatic(1.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
        assert!(matches!(
            scale(&a, -0.5),
            Err(LightCompositionError::InvalidScale(_))
        ));
    }

    #[test]
    fn attenuate_basic() {
        let a = monochromatic(1.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
        let t = attenuate(&a, 0.5).unwrap();
        assert!((t.intensity() - 0.5).abs() < 1e-3);
    }

    #[test]
    fn attenuate_extreme_sets_decreasing_glyph() {
        let a = monochromatic(1.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
        let t = attenuate(&a, 0.001).unwrap();
        assert_eq!(t.evidence(), EvidenceGlyph::Decreasing);
    }

    #[test]
    fn attenuate_rejects_out_of_range() {
        let a = monochromatic(1.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
        assert!(matches!(
            attenuate(&a, 1.5),
            Err(LightCompositionError::InvalidAttenuation(_))
        ));
        assert!(matches!(
            attenuate(&a, -0.1),
            Err(LightCompositionError::InvalidAttenuation(_))
        ));
    }

    #[test]
    fn mueller_identity_preserves_quantum() {
        let a = monochromatic(1.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
        let identity = [
            [1.0_f32, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let out = mueller_apply(&a, identity).unwrap();
        assert!((out.intensity() - a.intensity()).abs() < 1e-3);
    }

    #[test]
    fn mueller_extinction_dark_quantum() {
        let a = monochromatic(1.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
        let zero_m = [[0.0_f32; 4]; 4];
        let out = mueller_apply(&a, zero_m).unwrap();
        assert!(out.intensity() < 1e-3);
    }

    #[test]
    fn glyph_priority_orders_correctly() {
        // Forbidden > Alert > Uncertain > everything-else.
        let f = max_priority_glyph(EvidenceGlyph::Forbidden, EvidenceGlyph::Trusted);
        assert_eq!(f, EvidenceGlyph::Forbidden);
        let a = max_priority_glyph(EvidenceGlyph::Default, EvidenceGlyph::Alert);
        assert_eq!(a, EvidenceGlyph::Alert);
        let u = max_priority_glyph(EvidenceGlyph::Trusted, EvidenceGlyph::Uncertain);
        assert_eq!(u, EvidenceGlyph::Uncertain);
    }
}
