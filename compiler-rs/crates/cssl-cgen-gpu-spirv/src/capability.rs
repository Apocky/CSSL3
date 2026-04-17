//! SPIR-V capability + extension catalog.
//!
//! § SPEC : `specs/10_HW.csl` § VULKAN 1.4 BASELINE + § ARC A770 DETAILED SPECS.

use core::fmt;
use std::collections::BTreeSet;

/// SPIR-V capability declarations (subset needed by CSSLv3 stage-0 code-gen).
///
/// Each variant maps 1:1 to a SPIR-V `Capability` enum value ; textual form matches the
/// canonical SPIR-V disassembler output so tests can compare directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpirvCapability {
    /// Graphics shader execution models (vertex / fragment / tess / geom / compute-in-graphics).
    Shader,
    /// OpenCL kernel execution model (Level-Zero + OpenCL C++ sources).
    Kernel,
    /// 8-bit integer ops.
    Int8,
    /// 16-bit integer ops.
    Int16,
    /// 64-bit integer ops.
    Int64,
    /// 16-bit float ops.
    Float16,
    /// 64-bit float ops.
    Float64,
    /// Atomic float add (required for R18 telemetry + histogram kernels).
    AtomicFloat32AddEXT,
    /// Atomic float min/max.
    AtomicFloat32MinMaxEXT,
    /// BDA : `VK_KHR_buffer_device_address` backing.
    PhysicalStorageBufferAddresses,
    /// Vulkan memory model with device-scope.
    VulkanMemoryModelDeviceScope,
    /// Bindless resources via `DescriptorIndexing`.
    RuntimeDescriptorArray,
    /// Non-uniform shader-indexed descriptor access.
    ShaderNonUniform,
    /// Storage-buffer 16-bit access.
    StorageBuffer16BitAccess,
    /// Storage-buffer 8-bit access.
    StorageBuffer8BitAccess,
    /// Group-non-uniform arithmetic (reduction / scan).
    GroupNonUniformArithmetic,
    /// Group-non-uniform ballot.
    GroupNonUniformBallot,
    /// Group-non-uniform shuffle.
    GroupNonUniformShuffle,
    /// Group-non-uniform quad (derivatives-like quad-subgroup).
    GroupNonUniformQuad,
    /// Demote-to-helper-invocation (1.6 core).
    DemoteToHelperInvocation,
    /// Cooperative-matrix (KHR portable extension).
    CooperativeMatrixKHR,
    /// Cooperative-matrix (NV NVIDIA variant — pre-KHR).
    CooperativeMatrixNV,
    /// Ray-tracing pipeline (shader-groups).
    RayTracingKHR,
    /// Ray-query (inline-RT).
    RayQueryKHR,
    /// Acceleration-structure.
    RayTracingProvisional,
    /// Subgroup-rotate (KHR).
    GroupNonUniformRotateKHR,
    /// Expect/assume optimizer hints.
    ExpectAssumeKHR,
    /// Float-controls v2 (NaN/Inf preservation ; F1-AD numerical stability).
    FloatControls2,
    /// Physical storage-buffer 64-bit pointer conversion.
    StoragePushConstant16,
    /// Int64 atomics.
    Int64Atomics,
    /// Non-semantic shader debug-info (RenderDoc correlation).
    ShaderNonSemanticInfo,
    /// Mesh-shader pipeline stage (NV/EXT).
    MeshShadingEXT,
}

impl SpirvCapability {
    /// Canonical SPIR-V disassembler-form name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shader => "Shader",
            Self::Kernel => "Kernel",
            Self::Int8 => "Int8",
            Self::Int16 => "Int16",
            Self::Int64 => "Int64",
            Self::Float16 => "Float16",
            Self::Float64 => "Float64",
            Self::AtomicFloat32AddEXT => "AtomicFloat32AddEXT",
            Self::AtomicFloat32MinMaxEXT => "AtomicFloat32MinMaxEXT",
            Self::PhysicalStorageBufferAddresses => "PhysicalStorageBufferAddresses",
            Self::VulkanMemoryModelDeviceScope => "VulkanMemoryModelDeviceScope",
            Self::RuntimeDescriptorArray => "RuntimeDescriptorArray",
            Self::ShaderNonUniform => "ShaderNonUniform",
            Self::StorageBuffer16BitAccess => "StorageBuffer16BitAccess",
            Self::StorageBuffer8BitAccess => "StorageBuffer8BitAccess",
            Self::GroupNonUniformArithmetic => "GroupNonUniformArithmetic",
            Self::GroupNonUniformBallot => "GroupNonUniformBallot",
            Self::GroupNonUniformShuffle => "GroupNonUniformShuffle",
            Self::GroupNonUniformQuad => "GroupNonUniformQuad",
            Self::DemoteToHelperInvocation => "DemoteToHelperInvocation",
            Self::CooperativeMatrixKHR => "CooperativeMatrixKHR",
            Self::CooperativeMatrixNV => "CooperativeMatrixNV",
            Self::RayTracingKHR => "RayTracingKHR",
            Self::RayQueryKHR => "RayQueryKHR",
            Self::RayTracingProvisional => "RayTracingProvisional",
            Self::GroupNonUniformRotateKHR => "GroupNonUniformRotateKHR",
            Self::ExpectAssumeKHR => "ExpectAssumeKHR",
            Self::FloatControls2 => "FloatControls2",
            Self::StoragePushConstant16 => "StoragePushConstant16",
            Self::Int64Atomics => "Int64Atomics",
            Self::ShaderNonSemanticInfo => "ShaderNonSemanticInfo",
            Self::MeshShadingEXT => "MeshShadingEXT",
        }
    }

    /// True iff this capability requires a corresponding SPIR-V extension declaration.
    #[must_use]
    pub const fn requires_extension(self) -> bool {
        matches!(
            self,
            Self::AtomicFloat32AddEXT
                | Self::AtomicFloat32MinMaxEXT
                | Self::PhysicalStorageBufferAddresses
                | Self::CooperativeMatrixKHR
                | Self::CooperativeMatrixNV
                | Self::RayTracingKHR
                | Self::RayQueryKHR
                | Self::GroupNonUniformRotateKHR
                | Self::ExpectAssumeKHR
                | Self::FloatControls2
                | Self::ShaderNonSemanticInfo
                | Self::MeshShadingEXT
        )
    }
}

impl fmt::Display for SpirvCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SPIR-V extension catalog (the KHR / EXT / INTEL / NV extensions used by CSSLv3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpirvExtension {
    /// `SPV_KHR_physical_storage_buffer` (BDA).
    KhrPhysicalStorageBuffer,
    /// `SPV_KHR_vulkan_memory_model`.
    KhrVulkanMemoryModel,
    /// `SPV_KHR_storage_buffer_storage_class`.
    KhrStorageBufferStorageClass,
    /// `SPV_KHR_shader_draw_parameters`.
    KhrShaderDrawParameters,
    /// `SPV_KHR_non_semantic_info` (debug-printf + debug-info).
    KhrNonSemanticInfo,
    /// `SPV_KHR_subgroup_uniform_control_flow`.
    KhrSubgroupUniformControlFlow,
    /// `SPV_KHR_shader_subgroup_rotate`.
    KhrShaderSubgroupRotate,
    /// `SPV_KHR_expect_assume`.
    KhrExpectAssume,
    /// `SPV_KHR_float_controls2`.
    KhrFloatControls2,
    /// `SPV_KHR_ray_tracing`.
    KhrRayTracing,
    /// `SPV_KHR_ray_query`.
    KhrRayQuery,
    /// `SPV_KHR_cooperative_matrix`.
    KhrCooperativeMatrix,
    /// `SPV_EXT_descriptor_indexing`.
    ExtDescriptorIndexing,
    /// `SPV_EXT_demote_to_helper_invocation`.
    ExtDemoteToHelperInvocation,
    /// `SPV_EXT_shader_atomic_float_add`.
    ExtShaderAtomicFloatAdd,
    /// `SPV_EXT_shader_atomic_float_min_max`.
    ExtShaderAtomicFloatMinMax,
    /// `SPV_EXT_mesh_shader`.
    ExtMeshShader,
    /// `SPV_EXT_mutable_descriptor_type` (Arc BDA-bindless workaround).
    ExtMutableDescriptorType,
    /// `SPV_NV_cooperative_matrix` (legacy NV variant).
    NvCooperativeMatrix,
    /// `SPV_INTEL_subgroup_matrix_multiply_accumulate` (XMX direct-access).
    IntelSubgroupMatrixMultiplyAccumulate,
    /// `SPV_INTEL_function_pointers`.
    IntelFunctionPointers,
    /// `NonSemantic.Shader.DebugInfo.100` ext-inst-set import (RenderDoc source-line).
    NonSemanticShaderDebugInfo100,
    /// `NonSemantic.DebugPrintf` ext-inst-set import.
    NonSemanticDebugPrintf,
    /// `GLSL.std.450` ext-inst-set import.
    GlslStd450,
}

impl SpirvExtension {
    /// Canonical SPIR-V extension string (as embedded in `OpExtension` / `OpExtInstImport`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KhrPhysicalStorageBuffer => "SPV_KHR_physical_storage_buffer",
            Self::KhrVulkanMemoryModel => "SPV_KHR_vulkan_memory_model",
            Self::KhrStorageBufferStorageClass => "SPV_KHR_storage_buffer_storage_class",
            Self::KhrShaderDrawParameters => "SPV_KHR_shader_draw_parameters",
            Self::KhrNonSemanticInfo => "SPV_KHR_non_semantic_info",
            Self::KhrSubgroupUniformControlFlow => "SPV_KHR_subgroup_uniform_control_flow",
            Self::KhrShaderSubgroupRotate => "SPV_KHR_shader_subgroup_rotate",
            Self::KhrExpectAssume => "SPV_KHR_expect_assume",
            Self::KhrFloatControls2 => "SPV_KHR_float_controls2",
            Self::KhrRayTracing => "SPV_KHR_ray_tracing",
            Self::KhrRayQuery => "SPV_KHR_ray_query",
            Self::KhrCooperativeMatrix => "SPV_KHR_cooperative_matrix",
            Self::ExtDescriptorIndexing => "SPV_EXT_descriptor_indexing",
            Self::ExtDemoteToHelperInvocation => "SPV_EXT_demote_to_helper_invocation",
            Self::ExtShaderAtomicFloatAdd => "SPV_EXT_shader_atomic_float_add",
            Self::ExtShaderAtomicFloatMinMax => "SPV_EXT_shader_atomic_float_min_max",
            Self::ExtMeshShader => "SPV_EXT_mesh_shader",
            Self::ExtMutableDescriptorType => "SPV_EXT_mutable_descriptor_type",
            Self::NvCooperativeMatrix => "SPV_NV_cooperative_matrix",
            Self::IntelSubgroupMatrixMultiplyAccumulate => {
                "SPV_INTEL_subgroup_matrix_multiply_accumulate"
            }
            Self::IntelFunctionPointers => "SPV_INTEL_function_pointers",
            Self::NonSemanticShaderDebugInfo100 => "NonSemantic.Shader.DebugInfo.100",
            Self::NonSemanticDebugPrintf => "NonSemantic.DebugPrintf",
            Self::GlslStd450 => "GLSL.std.450",
        }
    }

    /// True iff this is an ext-inst-set import (i.e., goes into `OpExtInstImport`)
    /// rather than a plain extension declared via `OpExtension`.
    #[must_use]
    pub const fn is_ext_inst_set(self) -> bool {
        matches!(
            self,
            Self::NonSemanticShaderDebugInfo100 | Self::NonSemanticDebugPrintf | Self::GlslStd450
        )
    }
}

impl fmt::Display for SpirvExtension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Monotonic set of capabilities — ordered iteration for deterministic emission.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpirvCapabilitySet {
    caps: BTreeSet<SpirvCapability>,
}

impl SpirvCapabilitySet {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a capability.
    pub fn add(&mut self, c: SpirvCapability) {
        self.caps.insert(c);
    }

    /// True iff present.
    #[must_use]
    pub fn contains(&self, c: SpirvCapability) -> bool {
        self.caps.contains(&c)
    }

    /// Iterate in stable (enum-variant) order.
    pub fn iter(&self) -> impl Iterator<Item = SpirvCapability> + '_ {
        self.caps.iter().copied()
    }

    /// Number of capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.caps.len()
    }

    /// True iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }
}

impl FromIterator<SpirvCapability> for SpirvCapabilitySet {
    fn from_iter<I: IntoIterator<Item = SpirvCapability>>(iter: I) -> Self {
        let mut s = Self::new();
        for c in iter {
            s.add(c);
        }
        s
    }
}

/// Monotonic set of extensions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpirvExtensionSet {
    exts: BTreeSet<SpirvExtension>,
}

impl SpirvExtensionSet {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an extension.
    pub fn add(&mut self, e: SpirvExtension) {
        self.exts.insert(e);
    }

    /// True iff present.
    #[must_use]
    pub fn contains(&self, e: SpirvExtension) -> bool {
        self.exts.contains(&e)
    }

    /// Iterate plain extensions (non-ext-inst-set).
    pub fn iter_plain(&self) -> impl Iterator<Item = SpirvExtension> + '_ {
        self.exts.iter().copied().filter(|e| !e.is_ext_inst_set())
    }

    /// Iterate ext-inst-set imports (these go in a distinct SPIR-V section).
    pub fn iter_ext_inst_sets(&self) -> impl Iterator<Item = SpirvExtension> + '_ {
        self.exts.iter().copied().filter(|e| e.is_ext_inst_set())
    }

    /// Iterate all (plain + ext-inst-set) in stable order.
    pub fn iter_all(&self) -> impl Iterator<Item = SpirvExtension> + '_ {
        self.exts.iter().copied()
    }

    /// Number of extensions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.exts.len()
    }

    /// True iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exts.is_empty()
    }
}

impl FromIterator<SpirvExtension> for SpirvExtensionSet {
    fn from_iter<I: IntoIterator<Item = SpirvExtension>>(iter: I) -> Self {
        let mut s = Self::new();
        for e in iter {
            s.add(e);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::{SpirvCapability, SpirvCapabilitySet, SpirvExtension, SpirvExtensionSet};

    #[test]
    fn capability_names() {
        assert_eq!(SpirvCapability::Shader.as_str(), "Shader");
        assert_eq!(
            SpirvCapability::PhysicalStorageBufferAddresses.as_str(),
            "PhysicalStorageBufferAddresses"
        );
        assert_eq!(SpirvCapability::RayTracingKHR.as_str(), "RayTracingKHR");
    }

    #[test]
    fn capability_requires_extension_shape() {
        assert!(!SpirvCapability::Shader.requires_extension());
        assert!(SpirvCapability::RayTracingKHR.requires_extension());
        assert!(SpirvCapability::CooperativeMatrixKHR.requires_extension());
        assert!(SpirvCapability::FloatControls2.requires_extension());
    }

    #[test]
    fn extension_names() {
        assert_eq!(
            SpirvExtension::KhrRayTracing.as_str(),
            "SPV_KHR_ray_tracing"
        );
        assert_eq!(SpirvExtension::GlslStd450.as_str(), "GLSL.std.450");
        assert_eq!(
            SpirvExtension::IntelSubgroupMatrixMultiplyAccumulate.as_str(),
            "SPV_INTEL_subgroup_matrix_multiply_accumulate"
        );
    }

    #[test]
    fn ext_inst_set_flag() {
        assert!(SpirvExtension::GlslStd450.is_ext_inst_set());
        assert!(SpirvExtension::NonSemanticDebugPrintf.is_ext_inst_set());
        assert!(!SpirvExtension::KhrRayTracing.is_ext_inst_set());
    }

    #[test]
    fn cap_set_ops() {
        let mut s = SpirvCapabilitySet::new();
        s.add(SpirvCapability::Shader);
        s.add(SpirvCapability::Int64);
        assert!(s.contains(SpirvCapability::Shader));
        assert_eq!(s.len(), 2);
        assert!(!s.is_empty());
    }

    #[test]
    fn ext_set_splits_plain_and_ext_inst() {
        let s = SpirvExtensionSet::from_iter([
            SpirvExtension::KhrRayTracing,
            SpirvExtension::GlslStd450,
            SpirvExtension::NonSemanticDebugPrintf,
        ]);
        assert_eq!(s.len(), 3);
        assert_eq!(s.iter_plain().count(), 1);
        assert_eq!(s.iter_ext_inst_sets().count(), 2);
    }

    #[test]
    fn cap_set_from_iter_is_sorted() {
        let s = SpirvCapabilitySet::from_iter([
            SpirvCapability::Int64,
            SpirvCapability::Shader,
            SpirvCapability::Float64,
        ]);
        let order: Vec<_> = s.iter().collect();
        // Enum-declaration order : Shader < Int64 < Float64.
        assert_eq!(
            order,
            vec![
                SpirvCapability::Shader,
                SpirvCapability::Int64,
                SpirvCapability::Float64,
            ]
        );
    }
}
