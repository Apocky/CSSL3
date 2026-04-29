//! § foveation — per-eye foveated rendering with VRS-Tier-2 / FDM / Metal-DynRQ.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Foveated rendering reduces fragment-cost in peripheral pixels by lowering
//!   shading-rate. Stage-5 reads a per-eye [`FoveaMask`] from Stage-2 and
//!   selects a per-pixel [`ShadingRate`] : 1×1 / 2×2 / 4×4. The cost saving is
//!   typically 2.5× over uniform 1×1 shading at the same perceived quality.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VII` — foveation
//!     discipline. fovea = 1×1 ; mid = 2×2 ; peripheral = 4×4.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VI` — VK
//!     `VK_KHR_fragment_shading_rate` (Tier-2) / D3D12 VRS Tier-2 / Metal
//!     dynamic-render-quality.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VII` — opt-out
//!     fallback (center-bias-foveation) when consent revoked or eye-track
//!     quality drops below threshold.

use thiserror::Error;

use crate::multiview::MultiViewConfig;

/// Per-region shading-rate. Values are powers-of-two so the fragment-cost
/// scaling is `1 / (rate.x * rate.y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingRate {
    /// Full-rate : 1 fragment per pixel.
    OneByOne,
    /// 2× downsample : 1 fragment per 2×2 quad.
    TwoByTwo,
    /// 4× downsample : 1 fragment per 4×4 quad.
    FourByFour,
}

impl ShadingRate {
    /// Return the (x, y) downsample factor.
    #[must_use]
    pub fn factor(self) -> (u32, u32) {
        match self {
            ShadingRate::OneByOne => (1, 1),
            ShadingRate::TwoByTwo => (2, 2),
            ShadingRate::FourByFour => (4, 4),
        }
    }

    /// Fragment cost as a fraction of full-rate. 1×1 = 1.0, 2×2 = 0.25, 4×4 = 0.0625.
    #[must_use]
    pub fn cost_fraction(self) -> f32 {
        let (fx, fy) = self.factor();
        1.0 / ((fx * fy) as f32)
    }
}

/// One foveation zone : center, half-cone-tangent, shading-rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoveationZones {
    /// Center of the fovea ((u, v) ∈ \[0,1\]²) — gaze-location in the
    /// per-eye render-target. Defaults to (0.5, 0.5) when gaze unavailable.
    pub center: [f32; 2],
    /// Tangent of the half-cone for the full-rate region (5° default).
    pub fovea_tan: f32,
    /// Tangent of the half-cone for the mid-rate (2×2) region (15° default).
    pub mid_tan: f32,
}

impl FoveationZones {
    /// Default zones : 5° fovea / 15° mid / >15° peripheral.
    #[must_use]
    pub fn default_5_15() -> Self {
        FoveationZones {
            center: [0.5, 0.5],
            fovea_tan: (5.0_f32).to_radians().tan(),
            mid_tan: (15.0_f32).to_radians().tan(),
        }
    }

    /// Center-bias fallback (gaze unavailable / consent revoked).
    #[must_use]
    pub fn center_bias() -> Self {
        Self::default_5_15()
    }

    /// Return the shading-rate at uv-position `(u, v)`. The "distance from
    /// center" is computed in tan-units so the cone-angles compare directly.
    #[must_use]
    pub fn shading_rate_at(&self, u: f32, v: f32) -> ShadingRate {
        let du = u - self.center[0];
        let dv = v - self.center[1];
        let r = (du * du + dv * dv).sqrt();
        if r <= self.fovea_tan {
            ShadingRate::OneByOne
        } else if r <= self.mid_tan {
            ShadingRate::TwoByTwo
        } else {
            ShadingRate::FourByFour
        }
    }
}

/// Foveation method (which GPU-stack feature drives the shading-rate).
///
/// Drives backend selection in [`crate::pipeline::Stage5Node::wire`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoveationMethod {
    /// Vulkan VK_KHR_fragment_shading_rate Tier-2.
    VulkanVrsTier2,
    /// Quest-3 fixed-density-map (FDM) extension.
    QuestFdm,
    /// Apple Metal dynamic-render-quality.
    MetalDynamicRq,
    /// CPU-side mock (no real foveation, but per-pixel rate selection).
    CpuMock,
}

/// Per-eye foveation mask. Each pixel resolves to one [`ShadingRate`]. The
/// mask is "Σ-protected by construction" : the gaze-data that produced it
/// in Stage-2 is the elevated-consent-required data ; the FoveaMask itself
/// is the de-classified post-gate output (just a 2D shading-rate map).
#[derive(Debug, Clone)]
pub struct FoveaMask {
    /// Per-eye render-target dimensions.
    pub width: u32,
    pub height: u32,
    /// Zones used to compute this mask.
    pub zones: FoveationZones,
    /// Whether this mask was produced from consented gaze-data (true) or
    /// the center-bias fallback (false).
    consented_source: bool,
}

impl FoveaMask {
    /// Construct a mask from consented gaze-zones.
    #[must_use]
    pub fn from_consented_zones(width: u32, height: u32, zones: FoveationZones) -> Self {
        FoveaMask {
            width,
            height,
            zones,
            consented_source: true,
        }
    }

    /// Construct a center-bias fallback mask (consent revoked / gaze quality low).
    #[must_use]
    pub fn center_bias_fallback(width: u32, height: u32) -> Self {
        FoveaMask {
            width,
            height,
            zones: FoveationZones::center_bias(),
            consented_source: false,
        }
    }

    /// Whether the source gaze-data was consented. Stage-5 reads this for
    /// auditing only ; the mask itself is post-gate and always-allowed.
    #[must_use]
    pub fn source_consented(&self) -> bool {
        self.consented_source
    }

    /// Resolve the shading-rate at pixel `(px, py)`.
    #[must_use]
    pub fn shading_rate_at(&self, px: u32, py: u32) -> ShadingRate {
        let u = (px as f32 + 0.5) / (self.width.max(1) as f32);
        let v = (py as f32 + 0.5) / (self.height.max(1) as f32);
        self.zones.shading_rate_at(u, v)
    }

    /// Fraction of pixels in each zone (1×1 / 2×2 / 4×4). Used for cost
    /// projection in the budget-validator.
    #[must_use]
    pub fn pixel_distribution(&self) -> [f32; 3] {
        let total = (self.width * self.height).max(1) as f32;
        let mut buckets = [0u32; 3];
        for py in 0..self.height {
            for px in 0..self.width {
                let r = self.shading_rate_at(px, py);
                let idx = match r {
                    ShadingRate::OneByOne => 0,
                    ShadingRate::TwoByTwo => 1,
                    ShadingRate::FourByFour => 2,
                };
                buckets[idx] += 1;
            }
        }
        [
            buckets[0] as f32 / total,
            buckets[1] as f32 / total,
            buckets[2] as f32 / total,
        ]
    }

    /// Weighted cost-fraction over the entire mask :
    /// `Σ_zone (frac_zone × cost_fraction(rate_zone))`.
    /// This is the multiplier the raymarcher applies to its baseline cost.
    #[must_use]
    pub fn weighted_cost_fraction(&self) -> f32 {
        let dist = self.pixel_distribution();
        dist[0] * ShadingRate::OneByOne.cost_fraction()
            + dist[1] * ShadingRate::TwoByTwo.cost_fraction()
            + dist[2] * ShadingRate::FourByFour.cost_fraction()
    }
}

/// Errors from the foveation subsystem.
#[derive(Debug, Error)]
pub enum FoveationError {
    /// Method not supported on this backend.
    #[error("foveation method {method:?} not supported by current backend")]
    MethodUnsupported { method: FoveationMethod },
    /// Mask dimensions mismatch view dimensions.
    #[error("mask {mw}x{mh} mismatches view {vw}x{vh}")]
    DimensionMismatch { mw: u32, mh: u32, vw: u32, vh: u32 },
}

/// Foveated multi-view render driver. Holds the per-view masks + the
/// foveation method selector.
#[derive(Debug, Clone)]
pub struct FoveatedMultiViewRender {
    /// Per-view foveation masks. Length matches `MultiViewConfig::view_count`.
    pub masks: Vec<FoveaMask>,
    /// Backend method.
    pub method: FoveationMethod,
}

impl FoveatedMultiViewRender {
    /// Construct from per-view masks.
    #[must_use]
    pub fn from_masks(masks: Vec<FoveaMask>, method: FoveationMethod) -> Self {
        FoveatedMultiViewRender { masks, method }
    }

    /// Construct stereo-default (two center-bias masks) ; useful for tests.
    #[must_use]
    pub fn stereo_center_bias(width: u32, height: u32, method: FoveationMethod) -> Self {
        let m1 = FoveaMask::center_bias_fallback(width, height);
        let m2 = FoveaMask::center_bias_fallback(width, height);
        FoveatedMultiViewRender {
            masks: vec![m1, m2],
            method,
        }
    }

    /// Validate the masks against a multi-view config — sizes must match per-view.
    pub fn validate(&self, cfg: &MultiViewConfig) -> Result<(), FoveationError> {
        if self.masks.len() != cfg.view_count.count() as usize {
            return Err(FoveationError::DimensionMismatch {
                mw: self.masks.len() as u32,
                mh: 0,
                vw: cfg.view_count.count(),
                vh: 0,
            });
        }
        for (i, m) in self.masks.iter().enumerate() {
            if m.width != cfg.width || m.height != cfg.height {
                return Err(FoveationError::DimensionMismatch {
                    mw: m.width,
                    mh: m.height,
                    vw: cfg.width,
                    vh: cfg.height,
                });
            }
            let _ = i;
        }
        Ok(())
    }

    /// Total cost-fraction averaged over views. Used by [`crate::budget`].
    #[must_use]
    pub fn aggregate_cost_fraction(&self) -> f32 {
        if self.masks.is_empty() {
            return 1.0;
        }
        let n = self.masks.len() as f32;
        self.masks
            .iter()
            .map(FoveaMask::weighted_cost_fraction)
            .sum::<f32>()
            / n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shading_rate_factors() {
        assert_eq!(ShadingRate::OneByOne.factor(), (1, 1));
        assert_eq!(ShadingRate::TwoByTwo.factor(), (2, 2));
        assert_eq!(ShadingRate::FourByFour.factor(), (4, 4));
    }

    #[test]
    fn shading_rate_cost_fractions_correct() {
        assert!((ShadingRate::OneByOne.cost_fraction() - 1.0).abs() < 1e-6);
        assert!((ShadingRate::TwoByTwo.cost_fraction() - 0.25).abs() < 1e-6);
        assert!((ShadingRate::FourByFour.cost_fraction() - 0.0625).abs() < 1e-6);
    }

    #[test]
    fn zones_default_fovea_at_center() {
        let z = FoveationZones::default_5_15();
        let r = z.shading_rate_at(0.5, 0.5);
        assert_eq!(r, ShadingRate::OneByOne);
    }

    #[test]
    fn zones_default_periphery_at_corner() {
        let z = FoveationZones::default_5_15();
        let r = z.shading_rate_at(0.0, 0.0);
        assert_eq!(r, ShadingRate::FourByFour);
    }

    #[test]
    fn zones_default_mid_at_intermediate() {
        let z = FoveationZones::default_5_15();
        // 5° tan ≈ 0.087 ; place at 0.5 + 0.10 = within mid (15° tan ≈ 0.27).
        let r = z.shading_rate_at(0.6, 0.5);
        assert_eq!(r, ShadingRate::TwoByTwo);
    }

    #[test]
    fn fovea_mask_consent_flag_round_trip() {
        let m1 = FoveaMask::from_consented_zones(64, 64, FoveationZones::default_5_15());
        assert!(m1.source_consented());
        let m2 = FoveaMask::center_bias_fallback(64, 64);
        assert!(!m2.source_consented());
    }

    #[test]
    fn pixel_distribution_sums_to_one() {
        let m = FoveaMask::center_bias_fallback(32, 32);
        let d = m.pixel_distribution();
        let s = d[0] + d[1] + d[2];
        assert!((s - 1.0).abs() < 1e-3, "distribution sum = {s}");
    }

    #[test]
    fn weighted_cost_fraction_lower_than_one() {
        let m = FoveaMask::center_bias_fallback(64, 64);
        let f = m.weighted_cost_fraction();
        assert!(f < 1.0);
        assert!(f > 0.0);
    }

    #[test]
    fn foveated_render_validate_size_match() {
        use crate::camera::EyeCamera;
        let cam = EyeCamera::at_origin_quest3(64, 64);
        let cfg = MultiViewConfig::stereo(cam, cam);
        let fr = FoveatedMultiViewRender::stereo_center_bias(64, 64, FoveationMethod::CpuMock);
        assert!(fr.validate(&cfg).is_ok());
    }

    #[test]
    fn foveated_render_validate_mismatch_errors() {
        use crate::camera::EyeCamera;
        let cam = EyeCamera::at_origin_quest3(64, 64);
        let cfg = MultiViewConfig::stereo(cam, cam);
        let fr = FoveatedMultiViewRender::stereo_center_bias(32, 64, FoveationMethod::CpuMock);
        assert!(fr.validate(&cfg).is_err());
    }

    #[test]
    fn aggregate_cost_fraction_averages() {
        let fr = FoveatedMultiViewRender::stereo_center_bias(32, 32, FoveationMethod::CpuMock);
        let acf = fr.aggregate_cost_fraction();
        assert!(acf > 0.0 && acf < 1.0);
    }
}
