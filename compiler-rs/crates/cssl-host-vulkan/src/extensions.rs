//! Vulkan extension + layer catalog.
//!
//! § SPEC : `specs/10_HW.csl` § VULKAN 1.4 BASELINE.

use core::fmt;
use std::collections::BTreeSet;

/// Catalog of Vulkan extensions CSSLv3 stage-0 cares about.
///
/// Extensions that are *core in VK-1.4* (like `VK_KHR_dynamic_rendering`) are still
/// catalogued so the compiler can emit correct `OpExtension` declarations for older
/// target-envs if/when multi-version support lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VulkanExtension {
    /// `VK_KHR_swapchain` (surface presentation).
    KhrSwapchain,
    /// `VK_KHR_dynamic_rendering` (core 1.3).
    KhrDynamicRendering,
    /// `VK_KHR_dynamic_rendering_local_read` (VK-1.4 core).
    KhrDynamicRenderingLocalRead,
    /// `VK_KHR_push_descriptor`.
    KhrPushDescriptor,
    /// `VK_KHR_shader_subgroup_rotate`.
    KhrShaderSubgroupRotate,
    /// `VK_KHR_shader_expect_assume`.
    KhrShaderExpectAssume,
    /// `VK_KHR_global_priority`.
    KhrGlobalPriority,
    /// `VK_KHR_shader_float_controls2`.
    KhrShaderFloatControls2,
    /// `VK_KHR_index_type_uint8`.
    KhrIndexTypeUint8,
    /// `VK_KHR_line_rasterization`.
    KhrLineRasterization,
    /// `VK_KHR_vertex_attribute_divisor`.
    KhrVertexAttributeDivisor,
    /// `VK_KHR_maintenance5` — 1.4 core.
    KhrMaintenance5,
    /// `VK_KHR_maintenance6` — 1.4 core.
    KhrMaintenance6,
    /// `VK_KHR_maintenance7` — 1.4 core.
    KhrMaintenance7,
    /// `VK_KHR_maintenance8` — 1.4 core.
    KhrMaintenance8,
    /// `VK_KHR_cooperative_matrix` (dGPU only on Arc).
    KhrCooperativeMatrix,
    /// `VK_KHR_ray_tracing_pipeline`.
    KhrRayTracingPipeline,
    /// `VK_KHR_acceleration_structure`.
    KhrAccelerationStructure,
    /// `VK_KHR_ray_query` (inline RT).
    KhrRayQuery,
    /// `VK_EXT_descriptor_indexing` (1.2 core but widely used).
    ExtDescriptorIndexing,
    /// `VK_EXT_mutable_descriptor_type` (Arc BDA-bindless workaround).
    ExtMutableDescriptorType,
    /// `VK_KHR_shader_non_semantic_info`.
    KhrShaderNonSemanticInfo,
    /// `VK_KHR_buffer_device_address` (BDA, core in 1.2).
    KhrBufferDeviceAddress,
    /// `VK_KHR_vulkan_memory_model` (core in 1.2).
    KhrVulkanMemoryModel,
    /// `VK_EXT_shader_atomic_float`.
    ExtShaderAtomicFloat,
    /// `VK_EXT_shader_atomic_float2`.
    ExtShaderAtomicFloat2,
    /// `VK_EXT_mesh_shader`.
    ExtMeshShader,
    /// `VK_EXT_conservative_rasterization`.
    ExtConservativeRasterization,
    /// `VK_EXT_memory_budget`.
    ExtMemoryBudget,
    /// `VK_EXT_memory_priority`.
    ExtMemoryPriority,
    /// `VK_EXT_calibrated_timestamps` (R18 telemetry).
    ExtCalibratedTimestamps,
}

impl VulkanExtension {
    /// Canonical extension string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KhrSwapchain => "VK_KHR_swapchain",
            Self::KhrDynamicRendering => "VK_KHR_dynamic_rendering",
            Self::KhrDynamicRenderingLocalRead => "VK_KHR_dynamic_rendering_local_read",
            Self::KhrPushDescriptor => "VK_KHR_push_descriptor",
            Self::KhrShaderSubgroupRotate => "VK_KHR_shader_subgroup_rotate",
            Self::KhrShaderExpectAssume => "VK_KHR_shader_expect_assume",
            Self::KhrGlobalPriority => "VK_KHR_global_priority",
            Self::KhrShaderFloatControls2 => "VK_KHR_shader_float_controls2",
            Self::KhrIndexTypeUint8 => "VK_KHR_index_type_uint8",
            Self::KhrLineRasterization => "VK_KHR_line_rasterization",
            Self::KhrVertexAttributeDivisor => "VK_KHR_vertex_attribute_divisor",
            Self::KhrMaintenance5 => "VK_KHR_maintenance5",
            Self::KhrMaintenance6 => "VK_KHR_maintenance6",
            Self::KhrMaintenance7 => "VK_KHR_maintenance7",
            Self::KhrMaintenance8 => "VK_KHR_maintenance8",
            Self::KhrCooperativeMatrix => "VK_KHR_cooperative_matrix",
            Self::KhrRayTracingPipeline => "VK_KHR_ray_tracing_pipeline",
            Self::KhrAccelerationStructure => "VK_KHR_acceleration_structure",
            Self::KhrRayQuery => "VK_KHR_ray_query",
            Self::ExtDescriptorIndexing => "VK_EXT_descriptor_indexing",
            Self::ExtMutableDescriptorType => "VK_EXT_mutable_descriptor_type",
            Self::KhrShaderNonSemanticInfo => "VK_KHR_shader_non_semantic_info",
            Self::KhrBufferDeviceAddress => "VK_KHR_buffer_device_address",
            Self::KhrVulkanMemoryModel => "VK_KHR_vulkan_memory_model",
            Self::ExtShaderAtomicFloat => "VK_EXT_shader_atomic_float",
            Self::ExtShaderAtomicFloat2 => "VK_EXT_shader_atomic_float2",
            Self::ExtMeshShader => "VK_EXT_mesh_shader",
            Self::ExtConservativeRasterization => "VK_EXT_conservative_rasterization",
            Self::ExtMemoryBudget => "VK_EXT_memory_budget",
            Self::ExtMemoryPriority => "VK_EXT_memory_priority",
            Self::ExtCalibratedTimestamps => "VK_EXT_calibrated_timestamps",
        }
    }

    /// True iff this extension is promoted to core in VK-1.4.
    #[must_use]
    pub const fn is_core_in_vk_1_4(self) -> bool {
        matches!(
            self,
            Self::KhrMaintenance5
                | Self::KhrMaintenance6
                | Self::KhrMaintenance7
                | Self::KhrMaintenance8
                | Self::KhrDynamicRenderingLocalRead
                | Self::KhrPushDescriptor
                | Self::KhrShaderSubgroupRotate
                | Self::KhrShaderExpectAssume
                | Self::KhrGlobalPriority
                | Self::KhrShaderFloatControls2
                | Self::KhrIndexTypeUint8
                | Self::KhrLineRasterization
                | Self::KhrVertexAttributeDivisor
        )
    }
}

impl fmt::Display for VulkanExtension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Vulkan instance / device layer catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VulkanLayer {
    /// `VK_LAYER_KHRONOS_validation` — validation layer.
    KhronosValidation,
    /// `VK_LAYER_LUNARG_api_dump` — log every API call.
    LunarGApiDump,
    /// `VK_LAYER_LUNARG_monitor` — FPS monitor overlay.
    LunarGMonitor,
    /// `VK_LAYER_KHRONOS_profiles` — profiles layer (emulate other platforms).
    KhronosProfiles,
    /// `VK_LAYER_KHRONOS_synchronization2` — synchronization validation.
    KhronosSynchronization2,
}

impl VulkanLayer {
    /// Canonical layer string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KhronosValidation => "VK_LAYER_KHRONOS_validation",
            Self::LunarGApiDump => "VK_LAYER_LUNARG_api_dump",
            Self::LunarGMonitor => "VK_LAYER_LUNARG_monitor",
            Self::KhronosProfiles => "VK_LAYER_KHRONOS_profiles",
            Self::KhronosSynchronization2 => "VK_LAYER_KHRONOS_synchronization2",
        }
    }
}

/// Set of declared extensions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VulkanExtensionSet {
    exts: BTreeSet<VulkanExtension>,
}

impl VulkanExtensionSet {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add.
    pub fn add(&mut self, e: VulkanExtension) {
        self.exts.insert(e);
    }

    /// Present.
    #[must_use]
    pub fn contains(&self, e: VulkanExtension) -> bool {
        self.exts.contains(&e)
    }

    /// Iterator (sorted).
    pub fn iter(&self) -> impl Iterator<Item = VulkanExtension> + '_ {
        self.exts.iter().copied()
    }

    /// Size.
    #[must_use]
    pub fn len(&self) -> usize {
        self.exts.len()
    }

    /// Empty check.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exts.is_empty()
    }
}

impl FromIterator<VulkanExtension> for VulkanExtensionSet {
    fn from_iter<I: IntoIterator<Item = VulkanExtension>>(iter: I) -> Self {
        let mut s = Self::new();
        for e in iter {
            s.add(e);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::{VulkanExtension, VulkanExtensionSet, VulkanLayer};

    #[test]
    fn extension_names() {
        assert_eq!(VulkanExtension::KhrSwapchain.as_str(), "VK_KHR_swapchain");
        assert_eq!(
            VulkanExtension::KhrRayTracingPipeline.as_str(),
            "VK_KHR_ray_tracing_pipeline"
        );
        assert_eq!(
            VulkanExtension::ExtMutableDescriptorType.as_str(),
            "VK_EXT_mutable_descriptor_type"
        );
    }

    #[test]
    fn core_in_vk_1_4_flag() {
        assert!(VulkanExtension::KhrMaintenance5.is_core_in_vk_1_4());
        assert!(VulkanExtension::KhrShaderFloatControls2.is_core_in_vk_1_4());
        assert!(!VulkanExtension::KhrRayQuery.is_core_in_vk_1_4());
        assert!(!VulkanExtension::KhrCooperativeMatrix.is_core_in_vk_1_4());
    }

    #[test]
    fn layer_names() {
        assert_eq!(
            VulkanLayer::KhronosValidation.as_str(),
            "VK_LAYER_KHRONOS_validation"
        );
    }

    #[test]
    fn extension_set_ops() {
        let mut s = VulkanExtensionSet::new();
        s.add(VulkanExtension::KhrSwapchain);
        s.add(VulkanExtension::KhrDynamicRendering);
        assert!(s.contains(VulkanExtension::KhrSwapchain));
        assert_eq!(s.len(), 2);
        assert!(!s.is_empty());
    }

    #[test]
    fn extension_set_from_iter_sorted() {
        let s = VulkanExtensionSet::from_iter([
            VulkanExtension::KhrRayQuery,
            VulkanExtension::KhrSwapchain,
        ]);
        let order: Vec<_> = s.iter().collect();
        // Enum-decl order : KhrSwapchain < KhrRayQuery.
        assert_eq!(
            order,
            vec![VulkanExtension::KhrSwapchain, VulkanExtension::KhrRayQuery,]
        );
    }
}
