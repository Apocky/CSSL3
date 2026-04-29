//! § spectral_hook — D118 spectral-KAN-BRDF integration trait.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   T11-D118 lands the SpectralKanBrdfPass (Stage-6). Stage-5 emits the
//!   per-pixel surface-context that Stage-6 needs to compute the 16-band
//!   spectral radiance. This module defines the trait Stage-6 implements +
//!   a [`SpectralRadianceTransport`] data-class that carries the band-output.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III Stage-6` —
//!     KAN-BRDF eval per-fragment ; 16-band hyperspectral.
//!   - `Omniverse/07_AESTHETIC/03_SPECTRAL_PATH_TRACING § IV` — material-
//!     spectral-response.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH § KAN-spline` — KAN forward-pass.

/// 16-band spectral radiance carrier. Foundation slice : a `[f32; 16]` array.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, align(16))]
pub struct SpectralRadianceTransport {
    /// 16-band radiance values (visible-spectrum + a few IR / UV bands).
    pub bands: [f32; 16],
    /// View-index this radiance is for (0=left eye, 1=right eye for stereo).
    pub view_index: u32,
    /// Pixel coordinate (px, py).
    pub pixel: [u32; 2],
    /// Padding to keep alignment.
    pub _pad0: u32,
}

impl Default for SpectralRadianceTransport {
    fn default() -> Self {
        SpectralRadianceTransport {
            bands: [0.0; 16],
            view_index: 0,
            pixel: [0, 0],
            _pad0: 0,
        }
    }
}

impl SpectralRadianceTransport {
    /// New instance.
    #[must_use]
    pub fn new(bands: [f32; 16], view_index: u32, pixel: [u32; 2]) -> Self {
        SpectralRadianceTransport {
            bands,
            view_index,
            pixel,
            _pad0: 0,
        }
    }

    /// Total integrated radiance (sum across bands).
    #[must_use]
    pub fn total(&self) -> f32 {
        self.bands.iter().sum()
    }

    /// Mean radiance across bands.
    #[must_use]
    pub fn mean(&self) -> f32 {
        self.total() / self.bands.len() as f32
    }

    /// Whether any band carries non-zero radiance (the surface "sees" some
    /// light).
    #[must_use]
    pub fn has_radiance(&self) -> bool {
        self.bands.iter().any(|&b| b > 1e-6)
    }
}

/// Trait the Stage-6 spectral-KAN-BRDF pass implements. Stage-5 has emitted
/// a GBuffer + VolumetricAccum ; the spectral-hook combines those with the
/// PsiField (Stage-4 output) + M-facet (OmegaField) to produce the per-pixel
/// 16-band spectral radiance.
pub trait SpectralRadianceHook {
    /// Compute the spectral radiance for one pixel in one view.
    fn evaluate(
        &self,
        gbuffer_row: &crate::gbuffer::GBufferRow,
    ) -> SpectralRadianceTransport;

    /// Number of bands (16 for the canonical hyperspectral path ; some
    /// fallback paths drop to 8 or 4).
    fn band_count(&self) -> u8 {
        16
    }
}

/// Mock spectral hook : returns a flat-spectrum default radiance based on the
/// SDF distance (closer = brighter). Used pre-D118.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockSpectralHook {
    /// Brightness scale.
    pub scale: f32,
}

impl SpectralRadianceHook for MockSpectralHook {
    fn evaluate(
        &self,
        gbuffer_row: &crate::gbuffer::GBufferRow,
    ) -> SpectralRadianceTransport {
        let depth = gbuffer_row.depth_meters;
        let intensity = if depth.is_finite() {
            self.scale * (1.0 / (1.0 + depth))
        } else {
            0.0
        };
        let bands = [intensity; 16];
        SpectralRadianceTransport::new(bands, gbuffer_row.view_index, [0, 0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gbuffer::GBufferRow;
    use crate::normals::SurfaceNormal;

    #[test]
    fn transport_default_zero() {
        let t = SpectralRadianceTransport::default();
        assert!((t.total() - 0.0).abs() < 1e-6);
        assert!(!t.has_radiance());
    }

    #[test]
    fn transport_total_sums_bands() {
        let bands = [0.1; 16];
        let t = SpectralRadianceTransport::new(bands, 0, [0, 0]);
        assert!((t.total() - 1.6).abs() < 1e-5);
    }

    #[test]
    fn transport_mean_is_average() {
        let bands = [0.5; 16];
        let t = SpectralRadianceTransport::new(bands, 0, [0, 0]);
        assert!((t.mean() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn transport_has_radiance_when_nonzero() {
        let mut bands = [0.0; 16];
        bands[5] = 0.1;
        let t = SpectralRadianceTransport::new(bands, 0, [0, 0]);
        assert!(t.has_radiance());
    }

    #[test]
    fn transport_alignment_16_bytes() {
        assert_eq!(core::mem::align_of::<SpectralRadianceTransport>(), 16);
    }

    #[test]
    fn mock_hook_returns_intensity_falloff_with_depth() {
        let h = MockSpectralHook { scale: 1.0 };
        let row = GBufferRow::hit(
            1.0,
            [0.0, 0.0, 1.0],
            SurfaceNormal::from_grad([0.0, 1.0, 0.0]),
            0.0,
            0,
            0,
        );
        let t = h.evaluate(&row);
        assert!(t.has_radiance());
        // Intensity at depth=1 is 1/(1+1) = 0.5.
        assert!((t.bands[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn mock_hook_returns_zero_for_miss() {
        let h = MockSpectralHook { scale: 1.0 };
        let row = GBufferRow::miss(0);
        let t = h.evaluate(&row);
        assert!(!t.has_radiance());
    }

    #[test]
    fn band_count_default_is_16() {
        let h = MockSpectralHook::default();
        assert_eq!(h.band_count(), 16);
    }
}
