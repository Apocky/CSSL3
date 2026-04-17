//! SPIR-V target-environment + canonical-model enums.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § SPIR-V EMISSION INVARIANTS +
//!         `specs/10_HW.csl` § VULKAN 1.4 BASELINE + § LEVEL-ZERO BASELINE.

use core::fmt;

/// SPIR-V target-environment flavour. Dictates which capabilities + extensions
/// are legal to declare + which memory-model / addressing-model pair is canonical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpirvTargetEnv {
    /// Vulkan 1.0 profile (legacy, not a primary target but catalogued for completeness).
    VulkanKhr1_0,
    /// Vulkan 1.1 profile.
    VulkanKhr1_1,
    /// Vulkan 1.2 profile.
    VulkanKhr1_2,
    /// Vulkan 1.3 profile.
    VulkanKhr1_3,
    /// Vulkan 1.4 profile (v1 primary — matches `specs/10` VK baseline).
    VulkanKhr1_4,
    /// Universal SPIR-V 1.5 (pre-Vulkan-1.3 environment).
    UniversalSpirv1_5,
    /// Universal SPIR-V 1.6 (Vulkan-1.3+ baseline).
    UniversalSpirv1_6,
    /// OpenCL-Kernel execution model (used by Level-Zero direct-compute path).
    OpenClKernel2_2,
    /// WebGPU-compatible profile (SPIR-V → WGSL via Tint).
    WebGpu,
}

impl SpirvTargetEnv {
    /// spirv-val `--target-env` string.
    #[must_use]
    pub const fn target_env_str(self) -> &'static str {
        match self {
            Self::VulkanKhr1_0 => "vulkan1.0",
            Self::VulkanKhr1_1 => "vulkan1.1",
            Self::VulkanKhr1_2 => "vulkan1.2",
            Self::VulkanKhr1_3 => "vulkan1.3",
            Self::VulkanKhr1_4 => "vulkan1.4",
            Self::UniversalSpirv1_5 => "spv1.5",
            Self::UniversalSpirv1_6 => "spv1.6",
            Self::OpenClKernel2_2 => "opencl2.2",
            Self::WebGpu => "webgpu0",
        }
    }

    /// Canonical memory-model for this target-env.
    #[must_use]
    pub const fn default_memory_model(self) -> MemoryModel {
        match self {
            Self::VulkanKhr1_0
            | Self::VulkanKhr1_1
            | Self::VulkanKhr1_2
            | Self::VulkanKhr1_3
            | Self::VulkanKhr1_4
            | Self::UniversalSpirv1_5
            | Self::UniversalSpirv1_6 => MemoryModel::Vulkan,
            Self::OpenClKernel2_2 => MemoryModel::OpenCL,
            Self::WebGpu => MemoryModel::Vulkan,
        }
    }

    /// Canonical addressing-model for this target-env.
    #[must_use]
    pub const fn default_addressing_model(self) -> AddressingModel {
        match self {
            Self::VulkanKhr1_0
            | Self::VulkanKhr1_1
            | Self::VulkanKhr1_2
            | Self::VulkanKhr1_3
            | Self::VulkanKhr1_4
            | Self::UniversalSpirv1_5
            | Self::UniversalSpirv1_6 => AddressingModel::PhysicalStorageBuffer64,
            Self::OpenClKernel2_2 => AddressingModel::Physical64,
            Self::WebGpu => AddressingModel::Logical,
        }
    }
}

impl fmt::Display for SpirvTargetEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.target_env_str())
    }
}

/// SPIR-V `MemoryModel` enum mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryModel {
    /// Simple memory-model (rarely used for modern shaders).
    Simple,
    /// GLSL memory-model.
    Glsl450,
    /// OpenCL-Kernel memory model.
    OpenCL,
    /// Vulkan memory-model (Vulkan-1.1+ ; preferred for all v1 CSSLv3 targets except L0-kernel).
    Vulkan,
}

impl MemoryModel {
    /// Canonical SPIR-V-disasm form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Glsl450 => "GLSL450",
            Self::OpenCL => "OpenCL",
            Self::Vulkan => "Vulkan",
        }
    }
}

impl fmt::Display for MemoryModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SPIR-V `AddressingModel` enum mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressingModel {
    /// No physical pointers — only logical Storage-Class indexing.
    Logical,
    /// Physical 32-bit addressing (OpenCL on 32-bit platforms).
    Physical32,
    /// Physical 64-bit addressing (OpenCL on 64-bit platforms).
    Physical64,
    /// Physical 64-bit addressing via `SPV_KHR_physical_storage_buffer`.
    PhysicalStorageBuffer64,
}

impl AddressingModel {
    /// Canonical SPIR-V-disasm form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Logical => "Logical",
            Self::Physical32 => "Physical32",
            Self::Physical64 => "Physical64",
            Self::PhysicalStorageBuffer64 => "PhysicalStorageBuffer64",
        }
    }
}

impl fmt::Display for AddressingModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SPIR-V `ExecutionModel` enum mirror — the full catalog of shader/kernel entry-stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionModel {
    /// `Vertex` shader.
    Vertex,
    /// `TessellationControl` (hull) shader.
    TessellationControl,
    /// `TessellationEvaluation` (domain) shader.
    TessellationEvaluation,
    /// `Geometry` shader.
    Geometry,
    /// `Fragment` (pixel) shader.
    Fragment,
    /// `GLCompute` / compute shader (graphics pipeline).
    GlCompute,
    /// `Kernel` (OpenCL / Level-Zero compute).
    Kernel,
    /// `TaskEXT` (mesh-shader amplification stage).
    TaskExt,
    /// `MeshEXT` (mesh-shader primitive stage).
    MeshExt,
    /// `RayGenerationKHR`.
    RayGenerationKhr,
    /// `IntersectionKHR`.
    IntersectionKhr,
    /// `AnyHitKHR`.
    AnyHitKhr,
    /// `ClosestHitKHR`.
    ClosestHitKhr,
    /// `MissKHR`.
    MissKhr,
    /// `CallableKHR`.
    CallableKhr,
}

impl ExecutionModel {
    /// Canonical SPIR-V-disasm form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vertex => "Vertex",
            Self::TessellationControl => "TessellationControl",
            Self::TessellationEvaluation => "TessellationEvaluation",
            Self::Geometry => "Geometry",
            Self::Fragment => "Fragment",
            Self::GlCompute => "GLCompute",
            Self::Kernel => "Kernel",
            Self::TaskExt => "TaskEXT",
            Self::MeshExt => "MeshEXT",
            Self::RayGenerationKhr => "RayGenerationKHR",
            Self::IntersectionKhr => "IntersectionKHR",
            Self::AnyHitKhr => "AnyHitKHR",
            Self::ClosestHitKhr => "ClosestHitKHR",
            Self::MissKhr => "MissKHR",
            Self::CallableKhr => "CallableKHR",
        }
    }

    /// All 15 execution models.
    pub const ALL_MODELS: [Self; 15] = [
        Self::Vertex,
        Self::TessellationControl,
        Self::TessellationEvaluation,
        Self::Geometry,
        Self::Fragment,
        Self::GlCompute,
        Self::Kernel,
        Self::TaskExt,
        Self::MeshExt,
        Self::RayGenerationKhr,
        Self::IntersectionKhr,
        Self::AnyHitKhr,
        Self::ClosestHitKhr,
        Self::MissKhr,
        Self::CallableKhr,
    ];
}

impl fmt::Display for ExecutionModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{AddressingModel, ExecutionModel, MemoryModel, SpirvTargetEnv};

    #[test]
    fn target_env_strings() {
        assert_eq!(SpirvTargetEnv::VulkanKhr1_4.target_env_str(), "vulkan1.4");
        assert_eq!(
            SpirvTargetEnv::OpenClKernel2_2.target_env_str(),
            "opencl2.2"
        );
        assert_eq!(SpirvTargetEnv::WebGpu.target_env_str(), "webgpu0");
    }

    #[test]
    fn vulkan_default_models() {
        assert_eq!(
            SpirvTargetEnv::VulkanKhr1_4.default_memory_model(),
            MemoryModel::Vulkan
        );
        assert_eq!(
            SpirvTargetEnv::VulkanKhr1_4.default_addressing_model(),
            AddressingModel::PhysicalStorageBuffer64
        );
    }

    #[test]
    fn opencl_default_models() {
        assert_eq!(
            SpirvTargetEnv::OpenClKernel2_2.default_memory_model(),
            MemoryModel::OpenCL
        );
        assert_eq!(
            SpirvTargetEnv::OpenClKernel2_2.default_addressing_model(),
            AddressingModel::Physical64
        );
    }

    #[test]
    fn webgpu_default_models() {
        assert_eq!(
            SpirvTargetEnv::WebGpu.default_memory_model(),
            MemoryModel::Vulkan
        );
        assert_eq!(
            SpirvTargetEnv::WebGpu.default_addressing_model(),
            AddressingModel::Logical
        );
    }

    #[test]
    fn memory_model_names() {
        assert_eq!(MemoryModel::Vulkan.as_str(), "Vulkan");
        assert_eq!(MemoryModel::OpenCL.as_str(), "OpenCL");
    }

    #[test]
    fn addressing_model_names() {
        assert_eq!(AddressingModel::Logical.as_str(), "Logical");
        assert_eq!(
            AddressingModel::PhysicalStorageBuffer64.as_str(),
            "PhysicalStorageBuffer64"
        );
    }

    #[test]
    fn execution_model_catalog_complete() {
        assert_eq!(ExecutionModel::ALL_MODELS.len(), 15);
        let names: std::collections::HashSet<_> = ExecutionModel::ALL_MODELS
            .iter()
            .map(|m| m.as_str())
            .collect();
        assert_eq!(names.len(), 15);
    }

    #[test]
    fn rt_execution_models_have_khr_suffix() {
        assert!(ExecutionModel::RayGenerationKhr.as_str().ends_with("KHR"));
        assert!(ExecutionModel::ClosestHitKhr.as_str().ends_with("KHR"));
    }
}
