//! Swapchain management.
//!
//! § SPEC § IV.A : `xrCreateSwapchain` for color + (depth via XR_KHR_composition_layer_depth) +
//! (motion-vector via XR_FB_space_warp).
//!
//! § DESIGN
//!   `Swapchain` is the engine-side handle ; each carries an
//!   array-of-image-handles. The runtime owns the textures ; the engine
//!   acquires/releases via the cycle `xrAcquireSwapchainImage →
//!   xrWaitSwapchainImage → render → xrReleaseSwapchainImage`.

use crate::error::XRFailure;
use crate::per_eye::{ColorFormat, DepthFormat, MotionVectorFormat};

/// Swapchain-purpose : what this swapchain feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwapchainPurpose {
    /// Primary color (XR_TYPE_SWAPCHAIN_USAGE_COLOR_ATTACHMENT_BIT).
    Color,
    /// Depth (XR_KHR_composition_layer_depth + AppSW + reprojection).
    Depth,
    /// Motion-vector (XR_FB_space_warp).
    MotionVector,
}

impl SwapchainPurpose {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Depth => "depth",
            Self::MotionVector => "motion-vector",
        }
    }
}

/// Swapchain-format hint : the GPU-API-specific format value the runtime
/// negotiates against. The actual mapping lives in the GPU host crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwapchainFormat {
    /// Color : sRGB 8-bit.
    ColorSrgb8,
    /// Color : 10-bit + Wide-P3 (Vision Pro).
    ColorRgb10A2WideP3,
    /// Color : RGBA16F.
    ColorRgba16F,
    /// Depth : 32F.
    DepthFloat32,
    /// Depth : 16-bit unorm.
    DepthUnorm16,
    /// Motion-vector : RG16F.
    MotionVectorRg16F,
    /// Motion-vector : RG32F.
    MotionVectorRg32F,
}

impl SwapchainFormat {
    /// Bytes per pixel.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::ColorSrgb8 | Self::ColorRgb10A2WideP3 => 4,
            Self::ColorRgba16F => 8,
            Self::DepthFloat32 => 4,
            Self::DepthUnorm16 | Self::MotionVectorRg16F => 4,
            Self::MotionVectorRg32F => 8,
        }
    }

    /// Map a `ColorFormat` to its swapchain hint.
    #[must_use]
    pub const fn from_color(c: ColorFormat) -> Self {
        match c {
            ColorFormat::Srgb8 => Self::ColorSrgb8,
            ColorFormat::Rgb10A2WideP3 => Self::ColorRgb10A2WideP3,
            ColorFormat::Rgba16F | ColorFormat::Rgba32F => Self::ColorRgba16F,
        }
    }

    /// Map a `DepthFormat` to its swapchain hint.
    #[must_use]
    pub const fn from_depth(d: DepthFormat) -> Self {
        match d {
            DepthFormat::Depth16Unorm => Self::DepthUnorm16,
            DepthFormat::Depth24UnormStencil8
            | DepthFormat::Depth32F
            | DepthFormat::Depth32FStencil8 => Self::DepthFloat32,
        }
    }

    /// Map a `MotionVectorFormat` to its swapchain hint.
    #[must_use]
    pub const fn from_motion_vector(m: MotionVectorFormat) -> Self {
        match m {
            MotionVectorFormat::Rg16F | MotionVectorFormat::Rg16Unorm => Self::MotionVectorRg16F,
            MotionVectorFormat::Rg32F => Self::MotionVectorRg32F,
        }
    }
}

/// Swapchain-create-info.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapchainCreateInfo {
    /// Purpose this swapchain feeds.
    pub purpose: SwapchainPurpose,
    /// Format hint.
    pub format: SwapchainFormat,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Sample-count (MSAA). Typically 1 for VR.
    pub sample_count: u32,
    /// Array-size (e.g. 2 for stereo multiview). Equal to view_count for
    /// multiview swapchains.
    pub array_size: u32,
    /// Mip-level-count. Typically 1.
    pub mip_count: u32,
}

impl SwapchainCreateInfo {
    /// Quest 3 default per-eye color swapchain (no multiview ; one
    /// swapchain per eye).
    #[must_use]
    pub const fn quest3_color(width: u32, height: u32) -> Self {
        Self {
            purpose: SwapchainPurpose::Color,
            format: SwapchainFormat::ColorSrgb8,
            width,
            height,
            sample_count: 1,
            array_size: 1,
            mip_count: 1,
        }
    }

    /// Quest 3 default multiview-array color swapchain (single
    /// 2-array-layer swapchain for both eyes).
    #[must_use]
    pub const fn quest3_color_multiview(width: u32, height: u32) -> Self {
        Self {
            purpose: SwapchainPurpose::Color,
            format: SwapchainFormat::ColorSrgb8,
            width,
            height,
            sample_count: 1,
            array_size: 2,
            mip_count: 1,
        }
    }

    /// Vision Pro default : 10-bit Wide-P3 multiview swapchain.
    #[must_use]
    pub const fn vision_pro_color_multiview(width: u32, height: u32) -> Self {
        Self {
            purpose: SwapchainPurpose::Color,
            format: SwapchainFormat::ColorRgb10A2WideP3,
            width,
            height,
            sample_count: 1,
            array_size: 2,
            mip_count: 1,
        }
    }

    /// Pimax Crystal Super default : RGBA16F (HDR) multiview.
    #[must_use]
    pub const fn pimax_color_multiview(width: u32, height: u32) -> Self {
        Self {
            purpose: SwapchainPurpose::Color,
            format: SwapchainFormat::ColorRgba16F,
            width,
            height,
            sample_count: 1,
            array_size: 2,
            mip_count: 1,
        }
    }

    /// Validate.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if self.width == 0 || self.height == 0 {
            return Err(XRFailure::SwapchainCreate {
                code: -80,
                format: 0,
            });
        }
        if self.sample_count == 0 || self.mip_count == 0 || self.array_size == 0 {
            return Err(XRFailure::SwapchainCreate {
                code: -81,
                format: 0,
            });
        }
        if self.array_size as usize > crate::view::MAX_VIEWS {
            return Err(XRFailure::SwapchainCreate {
                code: -82,
                format: 0,
            });
        }
        Ok(())
    }
}

/// Stage-0 mock-swapchain. The FFI follow-up slice supersedes with a
/// real `xrCreateSwapchain` round-trip.
#[derive(Debug, Clone)]
pub struct MockSwapchain {
    /// Create-info that produced this.
    pub info: SwapchainCreateInfo,
    /// Image-handles in the swapchain (typically 3-4 for double/triple-buffer).
    pub image_handles: Vec<u64>,
    /// Index of the next image to acquire.
    next_acquire: usize,
}

impl MockSwapchain {
    /// Create with `image_count` image-handles.
    pub fn create(info: SwapchainCreateInfo, image_count: usize) -> Result<Self, XRFailure> {
        info.validate()?;
        let image_handles: Vec<u64> = (1..=(image_count as u64)).collect();
        Ok(Self {
            info,
            image_handles,
            next_acquire: 0,
        })
    }

    /// Acquire the next image. Increments the acquire-cursor.
    pub fn acquire(&mut self) -> Result<u64, XRFailure> {
        if self.image_handles.is_empty() {
            return Err(XRFailure::SwapchainImageCycle { code: -90 });
        }
        let h = self.image_handles[self.next_acquire];
        self.next_acquire = (self.next_acquire + 1) % self.image_handles.len();
        Ok(h)
    }

    /// Release (no-op in mock).
    pub fn release(&mut self) -> Result<(), XRFailure> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MockSwapchain, SwapchainCreateInfo, SwapchainFormat, SwapchainPurpose};
    use crate::per_eye::{ColorFormat, DepthFormat, MotionVectorFormat};

    #[test]
    fn purpose_as_str() {
        assert_eq!(SwapchainPurpose::Color.as_str(), "color");
        assert_eq!(SwapchainPurpose::Depth.as_str(), "depth");
        assert_eq!(SwapchainPurpose::MotionVector.as_str(), "motion-vector");
    }

    #[test]
    fn format_bytes_per_pixel() {
        assert_eq!(SwapchainFormat::ColorSrgb8.bytes_per_pixel(), 4);
        assert_eq!(SwapchainFormat::ColorRgb10A2WideP3.bytes_per_pixel(), 4);
        assert_eq!(SwapchainFormat::ColorRgba16F.bytes_per_pixel(), 8);
        assert_eq!(SwapchainFormat::DepthFloat32.bytes_per_pixel(), 4);
    }

    #[test]
    fn from_color_mapping() {
        assert_eq!(
            SwapchainFormat::from_color(ColorFormat::Srgb8),
            SwapchainFormat::ColorSrgb8
        );
        assert_eq!(
            SwapchainFormat::from_color(ColorFormat::Rgb10A2WideP3),
            SwapchainFormat::ColorRgb10A2WideP3
        );
        assert_eq!(
            SwapchainFormat::from_color(ColorFormat::Rgba16F),
            SwapchainFormat::ColorRgba16F
        );
    }

    #[test]
    fn from_depth_mapping() {
        assert_eq!(
            SwapchainFormat::from_depth(DepthFormat::Depth16Unorm),
            SwapchainFormat::DepthUnorm16
        );
        assert_eq!(
            SwapchainFormat::from_depth(DepthFormat::Depth32F),
            SwapchainFormat::DepthFloat32
        );
    }

    #[test]
    fn from_motion_vector_mapping() {
        assert_eq!(
            SwapchainFormat::from_motion_vector(MotionVectorFormat::Rg16F),
            SwapchainFormat::MotionVectorRg16F
        );
        assert_eq!(
            SwapchainFormat::from_motion_vector(MotionVectorFormat::Rg32F),
            SwapchainFormat::MotionVectorRg32F
        );
    }

    #[test]
    fn quest3_color_default_validates() {
        let info = SwapchainCreateInfo::quest3_color(2064, 2208);
        assert!(info.validate().is_ok());
        assert_eq!(info.array_size, 1);
    }

    #[test]
    fn quest3_color_multiview_array_2() {
        let info = SwapchainCreateInfo::quest3_color_multiview(2064, 2208);
        assert_eq!(info.array_size, 2);
    }

    #[test]
    fn vision_pro_color_multiview_10bit_widep3() {
        let info = SwapchainCreateInfo::vision_pro_color_multiview(3660, 3200);
        assert_eq!(info.format, SwapchainFormat::ColorRgb10A2WideP3);
    }

    #[test]
    fn pimax_color_multiview_rgba16f() {
        let info = SwapchainCreateInfo::pimax_color_multiview(3840, 3552);
        assert_eq!(info.format, SwapchainFormat::ColorRgba16F);
    }

    #[test]
    fn validate_rejects_zero_dimensions() {
        let info = SwapchainCreateInfo::quest3_color(0, 100);
        assert!(info.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_array_size() {
        let mut info = SwapchainCreateInfo::quest3_color(100, 100);
        info.array_size = 0;
        assert!(info.validate().is_err());
    }

    #[test]
    fn validate_rejects_array_size_over_max_views() {
        let mut info = SwapchainCreateInfo::quest3_color(100, 100);
        info.array_size = 17;
        assert!(info.validate().is_err());
    }

    #[test]
    fn mock_swapchain_create_validates() {
        let info = SwapchainCreateInfo::quest3_color_multiview(1024, 1024);
        let sc = MockSwapchain::create(info, 3).unwrap();
        assert_eq!(sc.image_handles.len(), 3);
    }

    #[test]
    fn mock_swapchain_acquire_round_robin() {
        let info = SwapchainCreateInfo::quest3_color_multiview(1024, 1024);
        let mut sc = MockSwapchain::create(info, 3).unwrap();
        let a = sc.acquire().unwrap();
        let b = sc.acquire().unwrap();
        let c = sc.acquire().unwrap();
        let d = sc.acquire().unwrap();
        // Round-robin : 4th acquire returns same as 1st.
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_eq!(d, a);
    }

    #[test]
    fn mock_swapchain_release_noop() {
        let info = SwapchainCreateInfo::quest3_color_multiview(1024, 1024);
        let mut sc = MockSwapchain::create(info, 3).unwrap();
        assert!(sc.release().is_ok());
    }
}
