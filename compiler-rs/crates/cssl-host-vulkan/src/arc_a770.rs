//! Hard-coded profile of the primary v1 target : Intel Arc A770 (Xe-HPG DG2-512).
//!
//! § SPEC : `specs/10_HW.csl` § ARC A770 DETAILED SPECS.

use crate::device::{DeviceFeatures, DeviceType, GpuVendor, VulkanDevice, VulkanVersion};
use crate::extensions::{VulkanExtension, VulkanExtensionSet};

/// Canonical properties for the Arc A770 as exposed by the Intel ISV driver
/// (verified 2026-Q2 per `specs/10_HW.csl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcA770Profile {
    /// Device-name string the driver reports.
    pub device_name: &'static str,
    /// PCI vendor ID (Intel = 0x8086).
    pub vendor_id: u32,
    /// PCI device ID (DG2-512 A770 = 0x56A0).
    pub device_id: u32,
    /// Vulkan API version exposed.
    pub api_version: VulkanVersion,
    /// Number of Xe-cores.
    pub xe_cores: u32,
    /// Total XVE (vector-engine) units.
    pub total_xve: u32,
    /// Total XMX (matrix-engine) units.
    pub total_xmx: u32,
    /// RT cores.
    pub rt_cores: u32,
    /// Clock boost (MHz).
    pub clock_boost_mhz: u32,
    /// VRAM (MB).
    pub vram_mb: u32,
    /// Memory bandwidth (GB/s).
    pub memory_bandwidth_gbps: u32,
    /// L2 cache (MB).
    pub l2_cache_mb: u32,
    /// PCIe generation (4).
    pub pcie_gen: u32,
    /// PCIe lanes (16).
    pub pcie_lanes: u32,
    /// TDP (watts).
    pub tdp_w: u32,
}

impl ArcA770Profile {
    /// Canonical Arc A770 (Alchemist DG2-512) profile.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            device_name: "Intel(R) Arc(TM) A770 Graphics",
            vendor_id: 0x8086,
            device_id: 0x56A0,
            api_version: VulkanVersion::V1_4,
            xe_cores: 32,
            total_xve: 512,
            total_xmx: 512,
            rt_cores: 32,
            clock_boost_mhz: 2100,
            vram_mb: 16 * 1024,
            memory_bandwidth_gbps: 560,
            l2_cache_mb: 16,
            pcie_gen: 4,
            pcie_lanes: 16,
            tdp_w: 225,
        }
    }

    /// Turn the hardware profile into a populated `VulkanDevice` record.
    #[must_use]
    pub fn to_vulkan_device(&self) -> VulkanDevice {
        VulkanDevice {
            name: self.device_name.to_string(),
            vendor_id: self.vendor_id,
            device_id: self.device_id,
            vendor: GpuVendor::from_pci_id(self.vendor_id),
            device_type: DeviceType::Discrete,
            api_version: self.api_version,
            driver_version: 0x2000_2165, // driver 32.0.101.8629 approximate
            features: expected_features(),
        }
    }

    /// Extensions known-enabled on this device by the ISV driver.
    #[must_use]
    pub fn expected_extensions() -> VulkanExtensionSet {
        VulkanExtensionSet::from_iter([
            VulkanExtension::KhrSwapchain,
            VulkanExtension::KhrDynamicRendering,
            VulkanExtension::KhrDynamicRenderingLocalRead,
            VulkanExtension::KhrPushDescriptor,
            VulkanExtension::KhrShaderSubgroupRotate,
            VulkanExtension::KhrShaderExpectAssume,
            VulkanExtension::KhrShaderFloatControls2,
            VulkanExtension::KhrIndexTypeUint8,
            VulkanExtension::KhrLineRasterization,
            VulkanExtension::KhrVertexAttributeDivisor,
            VulkanExtension::KhrMaintenance5,
            VulkanExtension::KhrMaintenance6,
            VulkanExtension::KhrMaintenance7,
            VulkanExtension::KhrMaintenance8,
            VulkanExtension::KhrCooperativeMatrix,
            VulkanExtension::KhrRayTracingPipeline,
            VulkanExtension::KhrAccelerationStructure,
            VulkanExtension::KhrRayQuery,
            VulkanExtension::ExtDescriptorIndexing,
            VulkanExtension::ExtMutableDescriptorType,
            VulkanExtension::KhrShaderNonSemanticInfo,
            VulkanExtension::KhrBufferDeviceAddress,
            VulkanExtension::KhrVulkanMemoryModel,
            VulkanExtension::ExtShaderAtomicFloat,
            VulkanExtension::ExtShaderAtomicFloat2,
            VulkanExtension::ExtMeshShader,
            VulkanExtension::ExtMemoryBudget,
            VulkanExtension::ExtMemoryPriority,
            VulkanExtension::ExtCalibratedTimestamps,
        ])
    }

    /// Peak FP32 TFLOPs (17.2 per `specs/10`).
    #[must_use]
    pub const fn peak_fp32_tflops_times_10(&self) -> u32 {
        // 17.2 TFLOPs stored as 172 to avoid float in const fn.
        172
    }
}

fn expected_features() -> DeviceFeatures {
    DeviceFeatures {
        storage_buffer_16bit_access: true,
        uniform_and_storage_buffer_8bit_access: true,
        shader_float16: true,
        shader_int8: true,
        shader_int16: true,
        shader_int64: true,
        buffer_device_address: true,
        runtime_descriptor_array: true,
        shader_non_uniform: true,
        vulkan_memory_model: true,
        vulkan_memory_model_device_scope: true,
        cooperative_matrix: true,
        ray_tracing_pipeline: true,
        ray_query: true,
        acceleration_structure: true,
        mesh_shader: true,
        subgroup_uniform_control_flow: true,
        shader_subgroup_rotate: true,
        shader_expect_assume: true,
        shader_float_controls2: true,
        shader_atomic_float_add: true,
        shader_atomic_float_min_max: true,
        mutable_descriptor_type: true,
        demote_to_helper_invocation: true,
        shader_non_semantic_info: true,
    }
}

#[cfg(test)]
mod tests {
    use super::ArcA770Profile;
    use crate::device::{DeviceType, GpuVendor, VulkanVersion};
    use crate::extensions::VulkanExtension;

    #[test]
    fn canonical_matches_spec() {
        let p = ArcA770Profile::canonical();
        assert_eq!(p.device_name, "Intel(R) Arc(TM) A770 Graphics");
        assert_eq!(p.vendor_id, 0x8086);
        assert_eq!(p.device_id, 0x56A0);
        assert_eq!(p.api_version, VulkanVersion::V1_4);
        assert_eq!(p.xe_cores, 32);
        assert_eq!(p.total_xve, 512);
        assert_eq!(p.total_xmx, 512);
        assert_eq!(p.rt_cores, 32);
        assert_eq!(p.vram_mb, 16 * 1024);
        assert_eq!(p.memory_bandwidth_gbps, 560);
        assert_eq!(p.tdp_w, 225);
    }

    #[test]
    fn to_vulkan_device_preserves_spec_facts() {
        let p = ArcA770Profile::canonical();
        let d = p.to_vulkan_device();
        assert_eq!(d.vendor, GpuVendor::Intel);
        assert_eq!(d.device_type, DeviceType::Discrete);
        assert_eq!(d.api_version, VulkanVersion::V1_4);
        assert!(d.features.cooperative_matrix);
        assert!(d.features.ray_tracing_pipeline);
        assert!(d.features.shader_float_controls2);
    }

    #[test]
    fn expected_extensions_includes_coop_matrix_and_rt() {
        let exts = ArcA770Profile::expected_extensions();
        assert!(exts.contains(VulkanExtension::KhrCooperativeMatrix));
        assert!(exts.contains(VulkanExtension::KhrRayTracingPipeline));
        assert!(exts.contains(VulkanExtension::KhrRayQuery));
        assert!(exts.contains(VulkanExtension::ExtMutableDescriptorType));
    }

    #[test]
    fn peak_fp32_tflops_value() {
        // 172 / 10 = 17.2 TFLOPs.
        assert_eq!(ArcA770Profile::canonical().peak_fp32_tflops_times_10(), 172);
    }

    #[test]
    fn expected_features_all_set() {
        let p = ArcA770Profile::canonical();
        let d = p.to_vulkan_device();
        assert_eq!(d.features.count_enabled(), 25);
    }
}
