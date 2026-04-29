//! `PerEyeOutput` contract : the per-view (color + motion-vector +
//! linear-depth + accommodation-depth) bundle every render-graph frame
//! must produce.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § III.A.
//!
//! § DESIGN
//!   - `motion_vector` + `linear_depth` are **always-produced** (not
//!     "VR-only") because AppSW + TAA + reprojection ALL need them.
//!     Spec § III.A : "EVERY per-eye color output has companion-outputs
//!     in the same render-graph-frame."
//!   - `accommodation_depth` is `Option`-typed for forward-compat to the
//!     5-yr varifocal accommodation-actuated-lens-display path (§ XIV.A).
//!     Today : `None` default ⊗ no render-cost ⊗ callers ignore at no-cost.
//!   - The texture-handles here are opaque `u64` resource-IDs ; the
//!     actual GPU-texture binding lives in `cssl-host-vulkan` /
//!     `cssl-host-d3d12` / `cssl-host-metal`. This keeps the OpenXR
//!     crate independent of any specific GPU backend.

use crate::error::XRFailure;
use crate::view::{ViewSet, MAX_VIEWS};

use smallvec::SmallVec;

/// Texture format-hint for swapchain creation. § XIII : 10-bit on
/// Vision-Pro / Quest-3 / Pimax-C-S ; SDR fallback for low-end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorFormat {
    /// 8-bit per channel sRGB (SDR fallback).
    Srgb8,
    /// 10-bit per channel + Wide-P3 (Vision-Pro 1B-color path).
    Rgb10A2WideP3,
    /// 16-bit float per channel (HDR-internal).
    Rgba16F,
    /// 32-bit float per channel (debug + spectral-staging).
    Rgba32F,
}

impl ColorFormat {
    /// `true` iff this format is HDR (10-bit+ or float).
    #[must_use]
    pub const fn is_hdr(self) -> bool {
        matches!(self, Self::Rgb10A2WideP3 | Self::Rgba16F | Self::Rgba32F)
    }

    /// Bytes-per-pixel for this format.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Srgb8 => 4,
            Self::Rgb10A2WideP3 => 4,
            Self::Rgba16F => 8,
            Self::Rgba32F => 16,
        }
    }

    /// Display-name (canonical short).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srgb8 => "srgb8",
            Self::Rgb10A2WideP3 => "rgb10a2-wide-p3",
            Self::Rgba16F => "rgba16f",
            Self::Rgba32F => "rgba32f",
        }
    }
}

/// Format-hint for depth buffers. Linear-depth (not NDC-Z) per § VI
/// AppSW correctness condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DepthFormat {
    /// 16-bit unsigned-normalized depth (low-end fallback).
    Depth16Unorm,
    /// 24-bit unsigned-normalized depth + 8-bit stencil.
    Depth24UnormStencil8,
    /// 32-bit float depth (canonical).
    Depth32F,
    /// 32-bit float depth + 8-bit stencil.
    Depth32FStencil8,
}

impl DepthFormat {
    /// Bytes-per-pixel for this format.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Depth16Unorm => 2,
            Self::Depth24UnormStencil8 => 4,
            Self::Depth32F => 4,
            Self::Depth32FStencil8 => 8,
        }
    }

    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Depth16Unorm => "depth16unorm",
            Self::Depth24UnormStencil8 => "depth24unorm-stencil8",
            Self::Depth32F => "depth32f",
            Self::Depth32FStencil8 => "depth32f-stencil8",
        }
    }
}

/// Format-hint for motion-vector buffers. § VI AppSW + TAA-history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionVectorFormat {
    /// 16-bit signed float per channel (canonical : enough range +
    /// precision for screen-space velocity).
    Rg16F,
    /// 16-bit unsigned-normalized per channel (compact ; fits 0..1
    /// post-bias).
    Rg16Unorm,
    /// 32-bit float per channel (debug + over-ranged scenes).
    Rg32F,
}

impl MotionVectorFormat {
    /// Bytes-per-pixel.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Rg16F | Self::Rg16Unorm => 4,
            Self::Rg32F => 8,
        }
    }
}

/// Per-eye output bundle for one frame. § III.A.
///
/// Texture-handles are opaque `u64` resource-IDs. The actual GPU
/// texture binding lives in the GPU host crate (`cssl-host-vulkan` /
/// `cssl-host-d3d12` / `cssl-host-metal`). This keeps `cssl-host-openxr`
/// independent of any specific GPU backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PerEyeOutput {
    /// Primary HDR radiance output (spectral-derived per `07_03 §VII`).
    pub color: u64,
    /// Color format. Negotiated at swapchain-create.
    pub color_format: ColorFormat,
    /// Screen-space motion-vector. § VI.A AppSW correctness.
    pub motion_vector: u64,
    /// Motion-vector format.
    pub motion_vector_format: MotionVectorFormat,
    /// Linear-Z depth (NOT NDC-Z). § VI.A AppSW correctness.
    pub linear_depth: u64,
    /// Linear-depth format.
    pub linear_depth_format: DepthFormat,
    /// Optional accommodation-depth for varifocal display
    /// (5-yr forward-compat ; `None` today). § XIV.A.
    pub accommodation_depth: Option<u64>,
    /// Index into the parent `ViewSet.views`.
    pub view_index: u32,
    /// Width in pixels at acquire-time.
    pub width: u32,
    /// Height in pixels at acquire-time.
    pub height: u32,
}

impl PerEyeOutput {
    /// Stage-0 placeholder constructor : zero-handle for every texture.
    /// Real handles arrive when the GPU host crate binds swapchain-images.
    #[must_use]
    pub const fn placeholder(view_index: u32, width: u32, height: u32) -> Self {
        Self {
            color: 0,
            color_format: ColorFormat::Rgba16F,
            motion_vector: 0,
            motion_vector_format: MotionVectorFormat::Rg16F,
            linear_depth: 0,
            linear_depth_format: DepthFormat::Depth32F,
            accommodation_depth: None,
            view_index,
            width,
            height,
        }
    }

    /// `true` iff this output has the AppSW-required companions :
    /// motion_vector + linear_depth both non-zero. § VI.A.
    #[must_use]
    pub fn has_appsw_companions(&self) -> bool {
        self.motion_vector != 0 && self.linear_depth != 0
    }

    /// `true` iff varifocal accommodation-depth is bound (5-yr path).
    #[must_use]
    pub const fn has_accommodation(&self) -> bool {
        self.accommodation_depth.is_some()
    }
}

/// Per-eye output array, one slot per view in the parent `ViewSet`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PerEyeOutputArray {
    /// One entry per view.
    pub outputs: SmallVec<[PerEyeOutput; 8]>,
}

impl PerEyeOutputArray {
    /// Empty array.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            outputs: SmallVec::new(),
        }
    }

    /// Construct one placeholder output per view in `view_set`.
    #[must_use]
    pub fn placeholder_for(view_set: &ViewSet, width: u32, height: u32) -> Self {
        let mut outputs = SmallVec::<[PerEyeOutput; 8]>::new();
        for v in &view_set.views {
            outputs.push(PerEyeOutput::placeholder(v.view_index, width, height));
        }
        Self { outputs }
    }

    /// Validate one entry per view + view_index matches the slot index.
    pub fn validate(&self, view_set: &ViewSet) -> Result<(), XRFailure> {
        if self.outputs.len() != view_set.view_count as usize {
            return Err(XRFailure::ViewCountOutOfRange {
                got: view_set.view_count,
            });
        }
        if self.outputs.len() > MAX_VIEWS {
            return Err(XRFailure::ViewCountOutOfRange {
                got: self.outputs.len() as u32,
            });
        }
        for (i, out) in self.outputs.iter().enumerate() {
            if out.view_index as usize != i {
                return Err(XRFailure::ViewCountOutOfRange {
                    got: out.view_index,
                });
            }
        }
        Ok(())
    }

    /// `true` iff every entry has AppSW-required companions.
    #[must_use]
    pub fn all_have_appsw_companions(&self) -> bool {
        self.outputs.iter().all(PerEyeOutput::has_appsw_companions)
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorFormat, DepthFormat, MotionVectorFormat, PerEyeOutput, PerEyeOutputArray};
    use crate::view::ViewSet;

    #[test]
    fn color_format_hdr_classification() {
        assert!(!ColorFormat::Srgb8.is_hdr());
        assert!(ColorFormat::Rgb10A2WideP3.is_hdr());
        assert!(ColorFormat::Rgba16F.is_hdr());
        assert!(ColorFormat::Rgba32F.is_hdr());
    }

    #[test]
    fn color_format_bytes_per_pixel() {
        assert_eq!(ColorFormat::Srgb8.bytes_per_pixel(), 4);
        assert_eq!(ColorFormat::Rgb10A2WideP3.bytes_per_pixel(), 4);
        assert_eq!(ColorFormat::Rgba16F.bytes_per_pixel(), 8);
        assert_eq!(ColorFormat::Rgba32F.bytes_per_pixel(), 16);
    }

    #[test]
    fn depth_format_bytes_per_pixel() {
        assert_eq!(DepthFormat::Depth16Unorm.bytes_per_pixel(), 2);
        assert_eq!(DepthFormat::Depth24UnormStencil8.bytes_per_pixel(), 4);
        assert_eq!(DepthFormat::Depth32F.bytes_per_pixel(), 4);
        assert_eq!(DepthFormat::Depth32FStencil8.bytes_per_pixel(), 8);
    }

    #[test]
    fn motion_vector_format_bytes_per_pixel() {
        assert_eq!(MotionVectorFormat::Rg16F.bytes_per_pixel(), 4);
        assert_eq!(MotionVectorFormat::Rg16Unorm.bytes_per_pixel(), 4);
        assert_eq!(MotionVectorFormat::Rg32F.bytes_per_pixel(), 8);
    }

    #[test]
    fn placeholder_output_lacks_appsw_companions() {
        let out = PerEyeOutput::placeholder(0, 1024, 1024);
        assert!(!out.has_appsw_companions());
        assert!(!out.has_accommodation());
        assert_eq!(out.width, 1024);
        assert_eq!(out.height, 1024);
    }

    #[test]
    fn placeholder_with_companion_handles_passes() {
        let mut out = PerEyeOutput::placeholder(0, 512, 512);
        out.motion_vector = 0xdead;
        out.linear_depth = 0xbeef;
        assert!(out.has_appsw_companions());
    }

    #[test]
    fn varifocal_accommodation_optional() {
        let mut out = PerEyeOutput::placeholder(0, 512, 512);
        assert!(!out.has_accommodation());
        out.accommodation_depth = Some(0xc0ffee);
        assert!(out.has_accommodation());
    }

    #[test]
    fn array_placeholder_for_stereo() {
        let vs = ViewSet::stereo_identity(64.0);
        let arr = PerEyeOutputArray::placeholder_for(&vs, 1024, 1024);
        assert_eq!(arr.outputs.len(), 2);
        assert!(arr.validate(&vs).is_ok());
    }

    #[test]
    fn array_placeholder_for_quad_view() {
        let vs = ViewSet::quad_view_foveated(64.0);
        let arr = PerEyeOutputArray::placeholder_for(&vs, 1024, 1024);
        assert_eq!(arr.outputs.len(), 4);
        assert!(arr.validate(&vs).is_ok());
    }

    #[test]
    fn array_validate_rejects_count_mismatch() {
        let vs = ViewSet::stereo_identity(64.0);
        let mut arr = PerEyeOutputArray::placeholder_for(&vs, 256, 256);
        arr.outputs.pop();
        assert!(arr.validate(&vs).is_err());
    }

    #[test]
    fn array_appsw_companions_require_all_eyes_filled() {
        let vs = ViewSet::stereo_identity(64.0);
        let mut arr = PerEyeOutputArray::placeholder_for(&vs, 256, 256);
        assert!(!arr.all_have_appsw_companions());
        arr.outputs[0].motion_vector = 1;
        arr.outputs[0].linear_depth = 1;
        assert!(
            !arr.all_have_appsw_companions(),
            "left eye filled but not right"
        );
        arr.outputs[1].motion_vector = 1;
        arr.outputs[1].linear_depth = 1;
        assert!(arr.all_have_appsw_companions(), "both eyes filled");
    }
}
