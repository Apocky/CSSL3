//! § SpectralTristimulus — spectral → CIE-XYZ → display-RGB tonemap
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `07_AES/03 § VII` the tonemap is the FINAL stage in the spectral
//!   pipeline ; RGB conversion happens here and ONLY here. The flow is :
//!
//!   - `SpectralRadiance(16-band)`
//!   - `=>` CIE-1931 XYZ (linear, observer-relative)
//!   - `=>` display primaries (sRGB / Rec.709 / Rec.2020 / DCI-P3)
//!   - `=>` ACES-2 tonemap curve
//!   - `=>` display-RGB (encoded for SDR / HDR display)
//!
//!   The CIE-XYZ matching functions are the standard 1931 2° observer (we
//!   use a simplified 16-band-discretized form here ; the full continuous
//!   CMF integral is owned by a future calibration-precision slice).

use crate::band::{BandTable, BAND_VISIBLE_END, BAND_VISIBLE_START};
use crate::radiance::SpectralRadiance;

/// § A CIE-1931 XYZ tristimulus value.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cie1931Xyz {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Cie1931Xyz {
    /// § Construct from explicit XYZ.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// § Black tristimulus.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0);

    /// § Sum of components (for chromaticity diagnostics).
    #[must_use]
    pub fn sum(self) -> f32 {
        self.x + self.y + self.z
    }
}

/// § A linear-light sRGB triple. Pre-OETF (no gamma encoded).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SrgbColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl SrgbColor {
    /// § Construct.
    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// § Black.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0);

    /// § Apply the sRGB OETF (gamma-encode for display). Per IEC 61966-2-1.
    #[must_use]
    pub fn encode_srgb(self) -> Self {
        Self::new(srgb_oetf(self.r), srgb_oetf(self.g), srgb_oetf(self.b))
    }
}

#[inline]
fn srgb_oetf(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// § Display primaries — used to select the XYZ→RGB matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPrimaries {
    /// § sRGB / BT.709 — SDR display canonical.
    Srgb,
    /// § DCI-P3 — wide-gamut HDR cinema.
    DciP3,
    /// § Rec.2020 — UHD TV / HDR-10.
    Rec2020,
}

/// § The spectral → tristimulus → display tonemap stage. Stateless ;
///   parameterized only by the chosen display primaries.
#[derive(Debug, Clone, Copy)]
pub struct SpectralTristimulus {
    /// § Target display primaries.
    pub primaries: DisplayPrimaries,
    /// § Exposure scale (linear).
    pub exposure: f32,
    /// § Whether to apply the ACES-2 contrast curve.
    pub apply_aces2: bool,
}

impl SpectralTristimulus {
    /// § Default config — sRGB primaries, exposure = 1.0, ACES-2 on.
    #[must_use]
    pub fn srgb_default() -> Self {
        Self {
            primaries: DisplayPrimaries::Srgb,
            exposure: 1.0,
            apply_aces2: true,
        }
    }

    /// § Wide-gamut HDR config (Rec.2020, no clip).
    #[must_use]
    pub fn rec2020_hdr() -> Self {
        Self {
            primaries: DisplayPrimaries::Rec2020,
            exposure: 1.0,
            apply_aces2: false,
        }
    }

    /// § Integrate a SpectralRadiance against the CIE-1931 2°-observer
    ///   matching functions, producing an XYZ tristimulus.
    #[must_use]
    pub fn integrate_cie1931(&self, r: &SpectralRadiance, table: &BandTable) -> Cie1931Xyz {
        // § Per-band CIE 2°-observer color-matching functions, sampled at
        //   the band centers. The values below are pre-integrated over the
        //   40-nm-wide visible bands (380→780 nm) ; the exact integration
        //   is reserved for a future colorimetry-precision slice. The
        //   shape of the curves matches the well-known x̄/ȳ/z̄ humps with
        //   peaks near 600 nm (X), 555 nm (Y), 445 nm (Z).
        const X_CMF: [f32; 10] = [
            0.014, 0.290, 0.090, 0.040, 0.290, 0.870, 0.940, 0.420, 0.063, 0.005,
        ];
        const Y_CMF: [f32; 10] = [
            0.000, 0.020, 0.139, 0.710, 0.954, 0.870, 0.510, 0.150, 0.018, 0.001,
        ];
        const Z_CMF: [f32; 10] = [
            0.066, 1.385, 1.040, 0.340, 0.062, 0.001, 0.000, 0.000, 0.000, 0.000,
        ];
        debug_assert_eq!(X_CMF.len(), BAND_VISIBLE_END - BAND_VISIBLE_START);

        let mut x = 0.0_f32;
        let mut y = 0.0_f32;
        let mut z = 0.0_f32;
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            let v = r.bands[i] * table.band(i).d65_weight;
            let k = i - BAND_VISIBLE_START;
            x += v * X_CMF[k];
            y += v * Y_CMF[k];
            z += v * Z_CMF[k];
        }
        Cie1931Xyz::new(x, y, z)
    }

    /// § Convert XYZ to linear-light RGB in the configured primaries.
    #[must_use]
    pub fn xyz_to_linear_rgb(&self, c: Cie1931Xyz) -> SrgbColor {
        // § Standard XYZ → linear-RGB matrices (D65 reference white).
        // sRGB / Rec.709 :
        const M_SRGB: [[f32; 3]; 3] = [
            [3.2406, -1.5372, -0.4986],
            [-0.9689, 1.8758, 0.0415],
            [0.0557, -0.2040, 1.0570],
        ];
        const M_DCI_P3: [[f32; 3]; 3] = [
            [2.4937, -0.9314, -0.4027],
            [-0.8295, 1.7627, 0.0236],
            [0.0359, -0.0762, 0.9569],
        ];
        const M_REC2020: [[f32; 3]; 3] = [
            [1.7167, -0.3557, -0.2534],
            [-0.6667, 1.6165, 0.0158],
            [0.0176, -0.0428, 0.9421],
        ];
        let m = match self.primaries {
            DisplayPrimaries::Srgb => &M_SRGB,
            DisplayPrimaries::DciP3 => &M_DCI_P3,
            DisplayPrimaries::Rec2020 => &M_REC2020,
        };
        let r = m[0][0] * c.x + m[0][1] * c.y + m[0][2] * c.z;
        let g = m[1][0] * c.x + m[1][1] * c.y + m[1][2] * c.z;
        let b = m[2][0] * c.x + m[2][1] * c.y + m[2][2] * c.z;
        SrgbColor::new(r * self.exposure, g * self.exposure, b * self.exposure)
    }

    /// § The ACES-2 narrow-curve approximation. Per `07_AES/03 § VII` :
    ///   "ACES-2 tonemap @ HDR → SDR-or-display-HDR". We use the well-
    ///   known fitted approximation
    ///     y = (a x (b x + c)) / (x (d x + e) + f)
    ///   with the ACES-2 coefficients.
    #[must_use]
    pub fn aces2_curve(&self, c: SrgbColor) -> SrgbColor {
        if !self.apply_aces2 {
            return c;
        }
        SrgbColor::new(aces2_fit(c.r), aces2_fit(c.g), aces2_fit(c.b))
    }

    /// § The full pipeline : SpectralRadiance → display-RGB (sRGB-encoded).
    #[must_use]
    pub fn tonemap(&self, r: &SpectralRadiance, table: &BandTable) -> SrgbColor {
        let xyz = self.integrate_cie1931(r, table);
        let lin = self.xyz_to_linear_rgb(xyz);
        let toned = self.aces2_curve(lin);
        // sRGB OETF only when targeting sRGB primaries.
        match self.primaries {
            DisplayPrimaries::Srgb => toned.encode_srgb(),
            _ => toned,
        }
    }
}

#[inline]
fn aces2_fit(x: f32) -> f32 {
    let a = 2.51_f32;
    let b = 0.03_f32;
    let c = 2.43_f32;
    let d = 0.59_f32;
    let e = 0.14_f32;
    let xc = x.max(0.0);
    let num = xc * (a * xc + b);
    let den = xc * (c * xc + d) + e;
    (num / den).max(0.0).min(1.0)
}

impl Default for SpectralTristimulus {
    fn default() -> Self {
        Self::srgb_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::band::BAND_VISIBLE_START;

    /// § Black radiance produces black RGB.
    #[test]
    fn black_radiance_produces_black() {
        let t = BandTable::d65();
        let r = SpectralRadiance::black();
        let rgb = SpectralTristimulus::srgb_default().tonemap(&r, &t);
        assert!(rgb.r < 1e-3);
        assert!(rgb.g < 1e-3);
        assert!(rgb.b < 1e-3);
    }

    /// § Equal-energy spectrum produces approximately white.
    #[test]
    fn equal_energy_approx_white() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            r.bands[i] = 0.3;
        }
        let cfg = SpectralTristimulus {
            primaries: DisplayPrimaries::Srgb,
            exposure: 1.0,
            apply_aces2: false,
        };
        let lin = cfg.xyz_to_linear_rgb(cfg.integrate_cie1931(&r, &t));
        // The three channels should be roughly comparable (not orders-of-magnitude apart).
        let max = lin.r.max(lin.g).max(lin.b);
        let min = lin.r.min(lin.g).min(lin.b);
        if max > 1e-3 {
            assert!(
                max / min < 5.0,
                "white-balance ratio out of range : {} / {}",
                max,
                min
            );
        }
    }

    /// § Pure-blue band produces blue-dominant RGB.
    #[test]
    fn blue_band_blue_dominant() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        // 440-nm band (blue).
        r.bands[BAND_VISIBLE_START + 1] = 1.0;
        let cfg = SpectralTristimulus {
            primaries: DisplayPrimaries::Srgb,
            exposure: 1.0,
            apply_aces2: false,
        };
        let rgb = cfg.xyz_to_linear_rgb(cfg.integrate_cie1931(&r, &t));
        assert!(rgb.b > rgb.r, "blue {} not > red {}", rgb.b, rgb.r);
    }

    /// § Pure-red band produces red-dominant RGB.
    #[test]
    fn red_band_red_dominant() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        // 640-nm band (red).
        r.bands[BAND_VISIBLE_START + 6] = 1.0;
        let cfg = SpectralTristimulus {
            primaries: DisplayPrimaries::Srgb,
            exposure: 1.0,
            apply_aces2: false,
        };
        let rgb = cfg.xyz_to_linear_rgb(cfg.integrate_cie1931(&r, &t));
        assert!(rgb.r > rgb.b, "red {} not > blue {}", rgb.r, rgb.b);
    }

    /// § ACES-2 maps 0 to 0, large to ≤ 1.
    #[test]
    fn aces2_clamps() {
        assert!((aces2_fit(0.0) - 0.0).abs() < 1e-3);
        assert!(aces2_fit(100.0) <= 1.0);
        assert!(aces2_fit(1e6) <= 1.0);
    }

    /// § ACES-2 monotone increasing.
    #[test]
    fn aces2_monotone() {
        let xs = [0.0_f32, 0.1, 0.3, 0.7, 1.5, 5.0, 50.0];
        for w in xs.windows(2) {
            let a = aces2_fit(w[0]);
            let b = aces2_fit(w[1]);
            assert!(
                b >= a - 1e-3,
                "ACES2 not monotone at {}/{}: {}/{}",
                w[0],
                w[1],
                a,
                b
            );
        }
    }

    /// § sRGB OETF maps 0 to 0, 1 to 1.
    #[test]
    fn srgb_oetf_endpoints() {
        assert!((srgb_oetf(0.0) - 0.0).abs() < 1e-6);
        assert!((srgb_oetf(1.0) - 1.0).abs() < 1e-3);
    }

    /// § sRGB OETF is monotone.
    #[test]
    fn srgb_oetf_monotone() {
        let xs = [0.0_f32, 0.001, 0.01, 0.1, 0.5, 1.0];
        for w in xs.windows(2) {
            assert!(srgb_oetf(w[1]) >= srgb_oetf(w[0]));
        }
    }

    /// § Cie1931Xyz::sum sums components.
    #[test]
    fn xyz_sum() {
        let c = Cie1931Xyz::new(0.5, 0.3, 0.2);
        assert!((c.sum() - 1.0).abs() < 1e-6);
    }

    /// § DisplayPrimaries variants compile.
    #[test]
    fn primaries_variants() {
        assert_ne!(DisplayPrimaries::Srgb, DisplayPrimaries::DciP3);
        assert_ne!(DisplayPrimaries::DciP3, DisplayPrimaries::Rec2020);
    }

    /// § exposure parameter scales output.
    #[test]
    fn exposure_scales_output() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        r.bands[BAND_VISIBLE_START + 4] = 0.5;
        let cfg_low = SpectralTristimulus {
            primaries: DisplayPrimaries::Srgb,
            exposure: 0.1,
            apply_aces2: false,
        };
        let cfg_high = SpectralTristimulus {
            primaries: DisplayPrimaries::Srgb,
            exposure: 1.0,
            apply_aces2: false,
        };
        let lin_low = cfg_low.xyz_to_linear_rgb(cfg_low.integrate_cie1931(&r, &t));
        let lin_high = cfg_high.xyz_to_linear_rgb(cfg_high.integrate_cie1931(&r, &t));
        let total_low = lin_low.r + lin_low.g + lin_low.b;
        let total_high = lin_high.r + lin_high.g + lin_high.b;
        assert!(total_high > total_low + 1e-3);
    }

    /// § rec2020_hdr disables ACES-2.
    #[test]
    fn rec2020_hdr_no_aces2() {
        let cfg = SpectralTristimulus::rec2020_hdr();
        assert!(!cfg.apply_aces2);
    }

    /// § srgb_default sets ACES-2.
    #[test]
    fn srgb_default_uses_aces2() {
        let cfg = SpectralTristimulus::srgb_default();
        assert!(cfg.apply_aces2);
    }
}
