//! Multiview shader-emit support : VK_KHR_multiview / D3D12 view-instancing /
//! Metal vertex-amplification.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § VII.
//!
//! § DESIGN
//!   - `MultiviewMode` enum : Vulkan / D3D12 / Metal / WebGPU-emulation.
//!   - `MultiviewConfig` carries the per-pipeline metadata (viewMask /
//!     view-instance-count / amplification-count).
//!   - This crate ships the **dispatch-mode descriptors** ; the actual
//!     pipeline-state objects live in `cssl-host-vulkan` /
//!     `cssl-host-d3d12` / `cssl-host-metal`.

use crate::error::XRFailure;
use crate::view::{ViewSet, MAX_VIEWS};

/// Multiview rendering-mode. § VII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MultiviewMode {
    /// `VK_KHR_multiview` — Vulkan-1.1+ core. § VII.A.
    /// Shader uses `gl_ViewIndex` builtin.
    VulkanMultiview,
    /// `view-instancing` — D3D12. § VII.B.
    /// Shader uses `SV_ViewID` semantic.
    D3D12ViewInstancing,
    /// `vertex-amplification` — Metal (visionOS canonical). § VII.C.
    /// Shader uses `[[amplification_id]]` per-vertex-attribute.
    MetalVertexAmplification,
    /// WebGPU multiview-emulation — texture-array + 2× draw-calls.
    /// § VII.D. Engine-fallback until WebGPU multiview lands native.
    WebGpuEmulation,
    /// No multiview ; render once per-eye in a draw-loop. Used for
    /// single-view (`viewCount = 1`) or very-old hardware fallback.
    SerialPerEye,
}

impl MultiviewMode {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VulkanMultiview => "vk-multiview",
            Self::D3D12ViewInstancing => "d3d12-view-instancing",
            Self::MetalVertexAmplification => "metal-vertex-amplification",
            Self::WebGpuEmulation => "webgpu-emulation",
            Self::SerialPerEye => "serial-per-eye",
        }
    }

    /// `true` iff this mode is single-draw-call multiview-amplified
    /// (vs. serial per-eye draws).
    #[must_use]
    pub const fn is_single_draw(self) -> bool {
        matches!(
            self,
            Self::VulkanMultiview | Self::D3D12ViewInstancing | Self::MetalVertexAmplification
        )
    }

    /// `true` iff this mode is the engine-default for the OpenXR-Vulkan path.
    #[must_use]
    pub const fn is_default_for_vulkan(self) -> bool {
        matches!(self, Self::VulkanMultiview)
    }

    /// `true` iff this mode is the engine-default for the OpenXR-D3D12 path.
    #[must_use]
    pub const fn is_default_for_d3d12(self) -> bool {
        matches!(self, Self::D3D12ViewInstancing)
    }

    /// `true` iff this mode is the engine-default for the visionOS path.
    #[must_use]
    pub const fn is_default_for_visionos(self) -> bool {
        matches!(self, Self::MetalVertexAmplification)
    }
}

/// Per-pipeline multiview config. § VII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MultiviewConfig {
    /// Mode in use.
    pub mode: MultiviewMode,
    /// Number of views amplified by a single draw-call.
    pub view_count: u32,
    /// Vulkan-only : 32-bit `viewMask` bitmask. § VII.A.
    /// (Bit i set ⇒ view i included in this render-pass.)
    pub vulkan_view_mask: u32,
    /// D3D12-only : `ViewInstancingDesc.ViewInstanceCount`. § VII.B.
    pub d3d12_view_instance_count: u32,
    /// Metal-only : `[[amplification_count]]`. § VII.C.
    pub metal_amplification_count: u32,
}

impl MultiviewConfig {
    /// Build a Vulkan multiview config that includes views `0..view_count`.
    pub fn vulkan(view_count: u32) -> Result<Self, XRFailure> {
        if view_count == 0 || view_count as usize > MAX_VIEWS || view_count > 32 {
            return Err(XRFailure::ViewCountOutOfRange { got: view_count });
        }
        let mask = if view_count >= 32 {
            0xFFFF_FFFF
        } else {
            (1u32 << view_count) - 1
        };
        Ok(Self {
            mode: MultiviewMode::VulkanMultiview,
            view_count,
            vulkan_view_mask: mask,
            d3d12_view_instance_count: 0,
            metal_amplification_count: 0,
        })
    }

    /// Build a D3D12 view-instancing config.
    pub fn d3d12(view_count: u32) -> Result<Self, XRFailure> {
        if view_count == 0 || view_count as usize > MAX_VIEWS {
            return Err(XRFailure::ViewCountOutOfRange { got: view_count });
        }
        Ok(Self {
            mode: MultiviewMode::D3D12ViewInstancing,
            view_count,
            vulkan_view_mask: 0,
            d3d12_view_instance_count: view_count,
            metal_amplification_count: 0,
        })
    }

    /// Build a Metal vertex-amplification config.
    pub fn metal(view_count: u32) -> Result<Self, XRFailure> {
        if view_count == 0 || view_count as usize > MAX_VIEWS {
            return Err(XRFailure::ViewCountOutOfRange { got: view_count });
        }
        Ok(Self {
            mode: MultiviewMode::MetalVertexAmplification,
            view_count,
            vulkan_view_mask: 0,
            d3d12_view_instance_count: 0,
            metal_amplification_count: view_count,
        })
    }

    /// Build a WebGPU emulation config.
    pub fn webgpu_emulation(view_count: u32) -> Result<Self, XRFailure> {
        if view_count == 0 || view_count as usize > MAX_VIEWS {
            return Err(XRFailure::ViewCountOutOfRange { got: view_count });
        }
        Ok(Self {
            mode: MultiviewMode::WebGpuEmulation,
            view_count,
            vulkan_view_mask: 0,
            d3d12_view_instance_count: 0,
            metal_amplification_count: 0,
        })
    }

    /// Build a serial-per-eye config (fallback ; no amplification).
    #[must_use]
    pub const fn serial_per_eye(view_count: u32) -> Self {
        Self {
            mode: MultiviewMode::SerialPerEye,
            view_count,
            vulkan_view_mask: 0,
            d3d12_view_instance_count: 0,
            metal_amplification_count: 0,
        }
    }

    /// Build the recommended config for a given mode + view_set.
    pub fn recommended(mode: MultiviewMode, view_set: &ViewSet) -> Result<Self, XRFailure> {
        match mode {
            MultiviewMode::VulkanMultiview => Self::vulkan(view_set.view_count),
            MultiviewMode::D3D12ViewInstancing => Self::d3d12(view_set.view_count),
            MultiviewMode::MetalVertexAmplification => Self::metal(view_set.view_count),
            MultiviewMode::WebGpuEmulation => Self::webgpu_emulation(view_set.view_count),
            MultiviewMode::SerialPerEye => Ok(Self::serial_per_eye(view_set.view_count)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MultiviewConfig, MultiviewMode};
    use crate::view::ViewSet;

    #[test]
    fn mode_classifications() {
        assert!(MultiviewMode::VulkanMultiview.is_single_draw());
        assert!(MultiviewMode::D3D12ViewInstancing.is_single_draw());
        assert!(MultiviewMode::MetalVertexAmplification.is_single_draw());
        assert!(!MultiviewMode::WebGpuEmulation.is_single_draw());
        assert!(!MultiviewMode::SerialPerEye.is_single_draw());

        assert!(MultiviewMode::VulkanMultiview.is_default_for_vulkan());
        assert!(MultiviewMode::D3D12ViewInstancing.is_default_for_d3d12());
        assert!(MultiviewMode::MetalVertexAmplification.is_default_for_visionos());
    }

    #[test]
    fn vulkan_view_mask_correct_for_2_views() {
        let cfg = MultiviewConfig::vulkan(2).unwrap();
        assert_eq!(cfg.vulkan_view_mask, 0b11);
        assert_eq!(cfg.view_count, 2);
    }

    #[test]
    fn vulkan_view_mask_correct_for_4_views() {
        let cfg = MultiviewConfig::vulkan(4).unwrap();
        assert_eq!(cfg.vulkan_view_mask, 0b1111);
    }

    #[test]
    fn vulkan_view_mask_correct_for_16_views() {
        let cfg = MultiviewConfig::vulkan(16).unwrap();
        assert_eq!(cfg.vulkan_view_mask, 0xFFFF);
    }

    #[test]
    fn vulkan_rejects_zero_views() {
        assert!(MultiviewConfig::vulkan(0).is_err());
    }

    #[test]
    fn vulkan_rejects_too_many_views() {
        assert!(MultiviewConfig::vulkan(17).is_err());
    }

    #[test]
    fn d3d12_view_instance_count_set() {
        let cfg = MultiviewConfig::d3d12(2).unwrap();
        assert_eq!(cfg.d3d12_view_instance_count, 2);
        assert_eq!(cfg.mode, MultiviewMode::D3D12ViewInstancing);
    }

    #[test]
    fn metal_amplification_count_set() {
        let cfg = MultiviewConfig::metal(2).unwrap();
        assert_eq!(cfg.metal_amplification_count, 2);
        assert_eq!(cfg.mode, MultiviewMode::MetalVertexAmplification);
    }

    #[test]
    fn webgpu_emulation_builds() {
        let cfg = MultiviewConfig::webgpu_emulation(2).unwrap();
        assert_eq!(cfg.mode, MultiviewMode::WebGpuEmulation);
        assert!(!cfg.mode.is_single_draw());
    }

    #[test]
    fn serial_per_eye_builds() {
        let cfg = MultiviewConfig::serial_per_eye(1);
        assert_eq!(cfg.mode, MultiviewMode::SerialPerEye);
        assert_eq!(cfg.view_count, 1);
    }

    #[test]
    fn recommended_for_vulkan_stereo() {
        let vs = ViewSet::stereo_identity(64.0);
        let cfg = MultiviewConfig::recommended(MultiviewMode::VulkanMultiview, &vs).unwrap();
        assert_eq!(cfg.mode, MultiviewMode::VulkanMultiview);
        assert_eq!(cfg.view_count, 2);
        assert_eq!(cfg.vulkan_view_mask, 0b11);
    }

    #[test]
    fn recommended_for_metal_visionos_stereo() {
        let vs = ViewSet::stereo_identity(64.0);
        let cfg =
            MultiviewConfig::recommended(MultiviewMode::MetalVertexAmplification, &vs).unwrap();
        assert_eq!(cfg.metal_amplification_count, 2);
    }

    #[test]
    fn recommended_for_quad_view_d3d12() {
        let vs = ViewSet::quad_view_foveated(64.0);
        let cfg = MultiviewConfig::recommended(MultiviewMode::D3D12ViewInstancing, &vs).unwrap();
        assert_eq!(cfg.d3d12_view_instance_count, 4);
    }

    #[test]
    fn recommended_for_flat_serial() {
        let vs = ViewSet::flat_monitor();
        let cfg = MultiviewConfig::recommended(MultiviewMode::SerialPerEye, &vs).unwrap();
        assert_eq!(cfg.view_count, 1);
    }
}
