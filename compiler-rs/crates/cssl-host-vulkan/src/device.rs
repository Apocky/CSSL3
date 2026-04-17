//! Vulkan device + vendor enumeration.

use core::fmt;

/// Vulkan API version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VulkanVersion {
    /// Vulkan 1.0.
    V1_0,
    /// Vulkan 1.1.
    V1_1,
    /// Vulkan 1.2.
    V1_2,
    /// Vulkan 1.3.
    V1_3,
    /// Vulkan 1.4 (v1 primary baseline per `specs/10`).
    V1_4,
}

impl VulkanVersion {
    /// Dotted form.
    #[must_use]
    pub const fn dotted(self) -> &'static str {
        match self {
            Self::V1_0 => "1.0",
            Self::V1_1 => "1.1",
            Self::V1_2 => "1.2",
            Self::V1_3 => "1.3",
            Self::V1_4 => "1.4",
        }
    }

    /// Packed Vulkan API version integer (ash-compatible : major << 22 | minor << 12 | patch).
    #[must_use]
    pub const fn packed(self) -> u32 {
        let (major, minor) = match self {
            Self::V1_0 => (1, 0),
            Self::V1_1 => (1, 1),
            Self::V1_2 => (1, 2),
            Self::V1_3 => (1, 3),
            Self::V1_4 => (1, 4),
        };
        (major << 22) | (minor << 12)
    }
}

impl fmt::Display for VulkanVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dotted())
    }
}

/// Canonical GPU-vendor enumeration (PCI-vendor-ID mapped to a symbolic name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuVendor {
    /// Intel (PCI VID 0x8086).
    Intel,
    /// NVIDIA (PCI VID 0x10DE).
    Nvidia,
    /// AMD (PCI VID 0x1002).
    Amd,
    /// Apple (PCI VID 0x106B ; Apple-Silicon GPUs).
    Apple,
    /// Qualcomm (PCI VID 0x5143).
    Qualcomm,
    /// ARM / Mali (PCI VID 0x13B5).
    Arm,
    /// Mesa software renderer (VID 0x10005).
    Mesa,
    /// Other / unknown vendor.
    Other,
}

impl GpuVendor {
    /// Resolve from a PCI vendor-ID.
    #[must_use]
    pub const fn from_pci_id(id: u32) -> Self {
        match id {
            0x8086 => Self::Intel,
            0x10DE => Self::Nvidia,
            0x1002 => Self::Amd,
            0x106B => Self::Apple,
            0x5143 => Self::Qualcomm,
            0x13B5 => Self::Arm,
            0x10005 => Self::Mesa,
            _ => Self::Other,
        }
    }

    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intel => "intel",
            Self::Nvidia => "nvidia",
            Self::Amd => "amd",
            Self::Apple => "apple",
            Self::Qualcomm => "qualcomm",
            Self::Arm => "arm",
            Self::Mesa => "mesa",
            Self::Other => "other",
        }
    }
}

/// Physical-device type (mirrors `VkPhysicalDeviceType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    /// Integrated GPU (iGPU).
    Integrated,
    /// Discrete GPU (dGPU).
    Discrete,
    /// Virtual GPU (GPU-passthrough).
    Virtual,
    /// CPU-side Vulkan implementation (SwiftShader / Lavapipe).
    Cpu,
    /// Other / unknown.
    Other,
}

impl DeviceType {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Integrated => "integrated",
            Self::Discrete => "discrete",
            Self::Virtual => "virtual",
            Self::Cpu => "cpu",
            Self::Other => "other",
        }
    }
}

/// Flags representing the Vulkan `VkPhysicalDeviceFeatures` fields CSSLv3 exercises.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeviceFeatures {
    pub storage_buffer_16bit_access: bool,
    pub uniform_and_storage_buffer_8bit_access: bool,
    pub shader_float16: bool,
    pub shader_int8: bool,
    pub shader_int16: bool,
    pub shader_int64: bool,
    pub buffer_device_address: bool,
    pub runtime_descriptor_array: bool,
    pub shader_non_uniform: bool,
    pub vulkan_memory_model: bool,
    pub vulkan_memory_model_device_scope: bool,
    pub cooperative_matrix: bool,
    pub ray_tracing_pipeline: bool,
    pub ray_query: bool,
    pub acceleration_structure: bool,
    pub mesh_shader: bool,
    pub subgroup_uniform_control_flow: bool,
    pub shader_subgroup_rotate: bool,
    pub shader_expect_assume: bool,
    pub shader_float_controls2: bool,
    pub shader_atomic_float_add: bool,
    pub shader_atomic_float_min_max: bool,
    pub mutable_descriptor_type: bool,
    pub demote_to_helper_invocation: bool,
    pub shader_non_semantic_info: bool,
}

impl DeviceFeatures {
    /// All features disabled.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            storage_buffer_16bit_access: false,
            uniform_and_storage_buffer_8bit_access: false,
            shader_float16: false,
            shader_int8: false,
            shader_int16: false,
            shader_int64: false,
            buffer_device_address: false,
            runtime_descriptor_array: false,
            shader_non_uniform: false,
            vulkan_memory_model: false,
            vulkan_memory_model_device_scope: false,
            cooperative_matrix: false,
            ray_tracing_pipeline: false,
            ray_query: false,
            acceleration_structure: false,
            mesh_shader: false,
            subgroup_uniform_control_flow: false,
            shader_subgroup_rotate: false,
            shader_expect_assume: false,
            shader_float_controls2: false,
            shader_atomic_float_add: false,
            shader_atomic_float_min_max: false,
            mutable_descriptor_type: false,
            demote_to_helper_invocation: false,
            shader_non_semantic_info: false,
        }
    }

    /// Count of enabled features.
    #[must_use]
    pub fn count_enabled(&self) -> u32 {
        let flags = [
            self.storage_buffer_16bit_access,
            self.uniform_and_storage_buffer_8bit_access,
            self.shader_float16,
            self.shader_int8,
            self.shader_int16,
            self.shader_int64,
            self.buffer_device_address,
            self.runtime_descriptor_array,
            self.shader_non_uniform,
            self.vulkan_memory_model,
            self.vulkan_memory_model_device_scope,
            self.cooperative_matrix,
            self.ray_tracing_pipeline,
            self.ray_query,
            self.acceleration_structure,
            self.mesh_shader,
            self.subgroup_uniform_control_flow,
            self.shader_subgroup_rotate,
            self.shader_expect_assume,
            self.shader_float_controls2,
            self.shader_atomic_float_add,
            self.shader_atomic_float_min_max,
            self.mutable_descriptor_type,
            self.demote_to_helper_invocation,
            self.shader_non_semantic_info,
        ];
        u32::try_from(flags.iter().filter(|b| **b).count()).unwrap_or(u32::MAX)
    }
}

/// Representation of an enumerable Vulkan physical device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulkanDevice {
    /// Device name (as reported by `VkPhysicalDeviceProperties::deviceName`).
    pub name: String,
    /// PCI vendor ID.
    pub vendor_id: u32,
    /// PCI device ID.
    pub device_id: u32,
    /// Resolved vendor name.
    pub vendor: GpuVendor,
    /// Device type (integrated / discrete / virtual / cpu).
    pub device_type: DeviceType,
    /// API version supported.
    pub api_version: VulkanVersion,
    /// Driver-version (raw, vendor-encoded).
    pub driver_version: u32,
    /// Feature-set exposed.
    pub features: DeviceFeatures,
}

impl VulkanDevice {
    /// Build a minimal `VulkanDevice` for testing / stub-probing.
    #[must_use]
    pub fn stub(name: impl Into<String>, vendor_id: u32, device_id: u32) -> Self {
        Self {
            name: name.into(),
            vendor_id,
            device_id,
            vendor: GpuVendor::from_pci_id(vendor_id),
            device_type: DeviceType::Other,
            api_version: VulkanVersion::V1_4,
            driver_version: 0,
            features: DeviceFeatures::none(),
        }
    }

    /// Short diagnostic-form.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} / {} / VK {} / {} / {} features",
            self.name,
            self.vendor.as_str(),
            self.api_version.dotted(),
            self.device_type.as_str(),
            self.features.count_enabled(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceFeatures, DeviceType, GpuVendor, VulkanDevice, VulkanVersion};

    #[test]
    fn vulkan_version_dotted() {
        assert_eq!(VulkanVersion::V1_0.dotted(), "1.0");
        assert_eq!(VulkanVersion::V1_4.dotted(), "1.4");
    }

    #[test]
    fn vulkan_version_packed_is_monotonic() {
        assert!(VulkanVersion::V1_4.packed() > VulkanVersion::V1_3.packed());
        assert!(VulkanVersion::V1_3.packed() > VulkanVersion::V1_2.packed());
    }

    #[test]
    fn vendor_from_pci_id() {
        assert_eq!(GpuVendor::from_pci_id(0x8086), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_pci_id(0x10DE), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_pci_id(0x1002), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_pci_id(0xDEAD), GpuVendor::Other);
    }

    #[test]
    fn device_type_names() {
        assert_eq!(DeviceType::Discrete.as_str(), "discrete");
        assert_eq!(DeviceType::Integrated.as_str(), "integrated");
    }

    #[test]
    fn device_features_none_count_is_zero() {
        let f = DeviceFeatures::none();
        assert_eq!(f.count_enabled(), 0);
    }

    #[test]
    fn device_features_count_correct() {
        let mut f = DeviceFeatures::none();
        f.buffer_device_address = true;
        f.runtime_descriptor_array = true;
        f.ray_tracing_pipeline = true;
        assert_eq!(f.count_enabled(), 3);
    }

    #[test]
    fn stub_device_defaults_to_vk_1_4() {
        let d = VulkanDevice::stub("Arc A770", 0x8086, 0x56A0);
        assert_eq!(d.api_version, VulkanVersion::V1_4);
        assert_eq!(d.vendor, GpuVendor::Intel);
        assert_eq!(d.device_id, 0x56A0);
    }

    #[test]
    fn stub_summary_contains_vendor_and_version() {
        let d = VulkanDevice::stub("Arc A770", 0x8086, 0x56A0);
        let s = d.summary();
        assert!(s.contains("intel"));
        assert!(s.contains("VK 1.4"));
    }
}
