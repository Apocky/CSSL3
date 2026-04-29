//! HDR + 10-bit color depth.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § XIII.
//!
//! § DESIGN
//!   - `HdrConfig` : per-target HDR parameters.
//!   - `ColorSpace` : `XR_COLOR_SPACE_REC2020` / `XR_COLOR_SPACE_REC709` /
//!     `XR_COLOR_SPACE_DCI_P3`.
//!   - `ToneMapCurve` : ACES2 / Reinhard / Hable (matches 07_03 §VII tone-curve set).

use crate::error::XRFailure;
use crate::per_eye::ColorFormat;

/// Color-space (mirrors XR_FB_color_space enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorSpace {
    /// Rec.709 (SDR HDTV).
    Rec709,
    /// sRGB (computer monitor canonical SDR).
    Srgb,
    /// DCI-P3 (cinema digital intermediate).
    DciP3,
    /// Wide-P3 (Apple Display-P3 ; visionOS canonical).
    WideP3,
    /// Rec.2020 (UHDTV / wide-gamut HDR).
    Rec2020,
    /// Adobe-RGB (rare ; legacy).
    AdobeRgb,
}

impl ColorSpace {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rec709 => "rec-709",
            Self::Srgb => "srgb",
            Self::DciP3 => "dci-p3",
            Self::WideP3 => "wide-p3",
            Self::Rec2020 => "rec-2020",
            Self::AdobeRgb => "adobe-rgb",
        }
    }

    /// `true` iff this color-space is wide-gamut (vs. sRGB-equivalent).
    #[must_use]
    pub const fn is_wide_gamut(self) -> bool {
        matches!(
            self,
            Self::DciP3 | Self::WideP3 | Self::Rec2020 | Self::AdobeRgb
        )
    }
}

/// Tone-map curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToneMapCurve {
    /// ACES-2 (07_AESTHETIC/03 §VII canonical).
    Aces2,
    /// Reinhard (luminance-only ; classical).
    Reinhard,
    /// Hable / "filmic" (Naughty Dog / Uncharted curve).
    Hable,
    /// Linear (debug only).
    Linear,
}

impl ToneMapCurve {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aces2 => "aces-2",
            Self::Reinhard => "reinhard",
            Self::Hable => "hable",
            Self::Linear => "linear",
        }
    }

    /// `true` iff this is the canonical curve per 07_03 §VII.
    #[must_use]
    pub const fn is_canonical(self) -> bool {
        matches!(self, Self::Aces2)
    }
}

/// HDR config for a session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HdrConfig {
    /// Color-space requested.
    pub color_space: ColorSpace,
    /// Color-format requested.
    pub format: ColorFormat,
    /// Peak-brightness in nits (HDR-cap of the panel).
    pub peak_nits: f32,
    /// Tone-map curve.
    pub tone_map: ToneMapCurve,
    /// `true` iff swapchain is HDR (10-bit+ or float).
    pub hdr_enabled: bool,
}

impl HdrConfig {
    /// SDR fallback : sRGB 8-bit + Rec.709 + Reinhard.
    #[must_use]
    pub const fn sdr_fallback() -> Self {
        Self {
            color_space: ColorSpace::Srgb,
            format: ColorFormat::Srgb8,
            peak_nits: 100.0,
            tone_map: ToneMapCurve::Reinhard,
            hdr_enabled: false,
        }
    }

    /// Vision Pro default : 10-bit Wide-P3 + ACES-2 + 1B-color path.
    /// § XIII : "Vision Pro 1B-color path engaged".
    #[must_use]
    pub const fn vision_pro_default() -> Self {
        Self {
            color_space: ColorSpace::WideP3,
            format: ColorFormat::Rgb10A2WideP3,
            peak_nits: 5000.0, // Vision Pro internal-headroom
            tone_map: ToneMapCurve::Aces2,
            hdr_enabled: true,
        }
    }

    /// Quest 3 default : 10-bit swapchain optional ; ACES-2 ; Rec.2020 internal.
    #[must_use]
    pub const fn quest3_default() -> Self {
        Self {
            color_space: ColorSpace::Rec2020,
            format: ColorFormat::Rgb10A2WideP3,
            peak_nits: 1000.0,
            tone_map: ToneMapCurve::Aces2,
            hdr_enabled: true,
        }
    }

    /// Pimax Crystal Super default : RGBA16F internal + HDR-1000 nit.
    #[must_use]
    pub const fn pimax_default() -> Self {
        Self {
            color_space: ColorSpace::Rec2020,
            format: ColorFormat::Rgba16F,
            peak_nits: 1000.0,
            tone_map: ToneMapCurve::Aces2,
            hdr_enabled: true,
        }
    }

    /// 5-yr Mirror-Lake default : 12-bit per-channel + Rec.2020 + 1500-nit.
    /// § II.C.
    #[must_use]
    pub const fn future_mirror_lake_default() -> Self {
        Self {
            color_space: ColorSpace::Rec2020,
            format: ColorFormat::Rgba16F, // 12-bit when format-extension lands ; today 16F
            peak_nits: 1500.0,
            tone_map: ToneMapCurve::Aces2,
            hdr_enabled: true,
        }
    }

    /// Validate.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if self.peak_nits < 80.0 {
            return Err(XRFailure::SwapchainCreate {
                code: -110,
                format: 0,
            });
        }
        if self.hdr_enabled && !self.format.is_hdr() {
            return Err(XRFailure::SwapchainCreate {
                code: -111,
                format: 0,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorSpace, HdrConfig, ToneMapCurve};
    use crate::per_eye::ColorFormat;

    #[test]
    fn color_space_wide_gamut_classification() {
        assert!(!ColorSpace::Rec709.is_wide_gamut());
        assert!(!ColorSpace::Srgb.is_wide_gamut());
        assert!(ColorSpace::DciP3.is_wide_gamut());
        assert!(ColorSpace::WideP3.is_wide_gamut());
        assert!(ColorSpace::Rec2020.is_wide_gamut());
    }

    #[test]
    fn tone_map_canonical_is_aces2() {
        assert!(ToneMapCurve::Aces2.is_canonical());
        assert!(!ToneMapCurve::Reinhard.is_canonical());
        assert!(!ToneMapCurve::Hable.is_canonical());
        assert!(!ToneMapCurve::Linear.is_canonical());
    }

    #[test]
    fn sdr_fallback_not_hdr() {
        let c = HdrConfig::sdr_fallback();
        assert!(!c.hdr_enabled);
        assert_eq!(c.format, ColorFormat::Srgb8);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn vision_pro_hdr_engaged_widep3_aces2() {
        let c = HdrConfig::vision_pro_default();
        assert!(c.hdr_enabled);
        assert_eq!(c.color_space, ColorSpace::WideP3);
        assert_eq!(c.tone_map, ToneMapCurve::Aces2);
        assert!(c.peak_nits >= 1000.0);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn quest3_hdr_engaged() {
        let c = HdrConfig::quest3_default();
        assert!(c.hdr_enabled);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn pimax_uses_rgba16f() {
        let c = HdrConfig::pimax_default();
        assert_eq!(c.format, ColorFormat::Rgba16F);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn future_mirror_lake_peak_1500_nit() {
        let c = HdrConfig::future_mirror_lake_default();
        assert_eq!(c.peak_nits, 1500.0);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_rejects_too_low_peak_nits() {
        let mut c = HdrConfig::sdr_fallback();
        c.peak_nits = 10.0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_rejects_hdr_with_sdr_format() {
        let c = HdrConfig {
            color_space: ColorSpace::Rec2020,
            format: ColorFormat::Srgb8,
            peak_nits: 1000.0,
            tone_map: ToneMapCurve::Aces2,
            hdr_enabled: true,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn color_space_as_str_canonical() {
        assert_eq!(ColorSpace::WideP3.as_str(), "wide-p3");
        assert_eq!(ColorSpace::Rec2020.as_str(), "rec-2020");
    }
}
