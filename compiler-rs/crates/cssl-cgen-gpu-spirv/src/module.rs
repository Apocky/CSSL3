//! Stage-0 SPIR-V module builder enforcing canonical section ordering.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § SPIR-V EMISSION INVARIANTS § module-section-order.
//!
//! § SECTION ORDER (rigid) :
//!   Capabilities → Extensions → `ExtInstImports` → `MemoryModel` →
//!   `EntryPoints` → `ExecutionModes` → debug → annotations/decorations →
//!   types/constants/global-vars → fn-declarations → fn-definitions

use crate::capability::{SpirvCapability, SpirvCapabilitySet, SpirvExtension, SpirvExtensionSet};
use crate::target::{AddressingModel, ExecutionModel, MemoryModel, SpirvTargetEnv};

/// Canonical section-index (rigid order per `specs/07` § SPIR-V EMISSION INVARIANTS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpirvSection {
    /// § 1 — `OpCapability`.
    Capability,
    /// § 2 — `OpExtension`.
    Extension,
    /// § 3 — `OpExtInstImport`.
    ExtInstImport,
    /// § 4 — `OpMemoryModel`.
    MemoryModel,
    /// § 5 — `OpEntryPoint`.
    EntryPoint,
    /// § 6 — `OpExecutionMode` / `OpExecutionModeId`.
    ExecutionMode,
    /// § 7 — debug instructions (`OpString`, `OpSource`, `OpName`, `OpMemberName`).
    Debug,
    /// § 8 — annotations / decorations (`OpDecorate`, `OpMemberDecorate`).
    Annotation,
    /// § 9 — types + constants + global variables.
    TypesConstantsGlobals,
    /// § 10 — fn declarations.
    FnDecl,
    /// § 11 — fn definitions.
    FnDef,
}

impl SpirvSection {
    /// Human-readable section name for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Capability => "capabilities",
            Self::Extension => "extensions",
            Self::ExtInstImport => "ext-inst-imports",
            Self::MemoryModel => "memory-model",
            Self::EntryPoint => "entry-points",
            Self::ExecutionMode => "execution-modes",
            Self::Debug => "debug",
            Self::Annotation => "annotations",
            Self::TypesConstantsGlobals => "types-constants-globals",
            Self::FnDecl => "fn-decls",
            Self::FnDef => "fn-defs",
        }
    }

    /// All 11 sections in canonical order.
    pub const ALL_SECTIONS: [Self; 11] = [
        Self::Capability,
        Self::Extension,
        Self::ExtInstImport,
        Self::MemoryModel,
        Self::EntryPoint,
        Self::ExecutionMode,
        Self::Debug,
        Self::Annotation,
        Self::TypesConstantsGlobals,
        Self::FnDecl,
        Self::FnDef,
    ];
}

/// An entry-point registered in the SPIR-V module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpirvEntryPoint {
    /// Execution model (vertex / fragment / compute / ray-gen / …).
    pub model: ExecutionModel,
    /// Canonical entry-point name (matches the MIR fn name).
    pub name: String,
    /// Execution-mode declarations for this entry point (e.g., `LocalSize 32 1 1`).
    pub execution_modes: Vec<String>,
}

/// Stage-0 SPIR-V module builder. Stores contents in canonical sections so
/// the emitter can walk them in order without re-sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpirvModule {
    /// Target environment this module is emitted for.
    pub target_env: SpirvTargetEnv,
    /// Memory-model.
    pub memory_model: MemoryModel,
    /// Addressing-model.
    pub addressing_model: AddressingModel,
    /// Capabilities.
    pub capabilities: SpirvCapabilitySet,
    /// Extensions + ext-inst-set imports (split at emit-time).
    pub extensions: SpirvExtensionSet,
    /// Entry points.
    pub entry_points: Vec<SpirvEntryPoint>,
    /// Source-language hint for debug info (e.g., `"CSSLv3"`).
    pub source_language: Option<String>,
    /// Optional version tag recorded in `OpSource`.
    pub source_version: Option<u32>,
}

impl SpirvModule {
    /// Build a module for `target_env` with canonical memory + addressing defaults.
    #[must_use]
    pub fn new(target_env: SpirvTargetEnv) -> Self {
        Self {
            target_env,
            memory_model: target_env.default_memory_model(),
            addressing_model: target_env.default_addressing_model(),
            capabilities: SpirvCapabilitySet::new(),
            extensions: SpirvExtensionSet::new(),
            entry_points: Vec::new(),
            source_language: Some("CSSLv3".to_string()),
            source_version: None,
        }
    }

    /// Declare a capability. Shader-vs-Kernel exclusivity is not enforced at stage-0
    /// (the caller must match the target-env ; `spirv-val` catches violations).
    pub fn declare_capability(&mut self, c: SpirvCapability) {
        self.capabilities.add(c);
    }

    /// Declare an extension or ext-inst-set import.
    pub fn declare_extension(&mut self, e: SpirvExtension) {
        self.extensions.add(e);
    }

    /// Register an entry point.
    pub fn add_entry_point(&mut self, ep: SpirvEntryPoint) {
        self.entry_points.push(ep);
    }

    /// Apply sensible defaults for a Vulkan-1.4 shader module : Shader capability +
    /// `PhysicalStorageBufferAddresses` + `VulkanMemoryModelDeviceScope` + the
    /// corresponding extensions.
    pub fn seed_vulkan_1_4_defaults(&mut self) {
        self.declare_capability(SpirvCapability::Shader);
        self.declare_capability(SpirvCapability::PhysicalStorageBufferAddresses);
        self.declare_capability(SpirvCapability::VulkanMemoryModelDeviceScope);
        self.declare_extension(SpirvExtension::KhrPhysicalStorageBuffer);
        self.declare_extension(SpirvExtension::KhrVulkanMemoryModel);
        self.declare_extension(SpirvExtension::GlslStd450);
    }

    /// Apply sensible defaults for an OpenCL-Kernel (Level-Zero) module : Kernel capability
    /// + Int64 / Float64 / Addresses + common CL extensions.
    pub fn seed_opencl_kernel_defaults(&mut self) {
        self.declare_capability(SpirvCapability::Kernel);
        self.declare_capability(SpirvCapability::Int64);
        self.declare_capability(SpirvCapability::Float64);
        self.declare_extension(SpirvExtension::IntelFunctionPointers);
    }
}

#[cfg(test)]
mod tests {
    use super::{SpirvEntryPoint, SpirvModule, SpirvSection};
    use crate::capability::{SpirvCapability, SpirvExtension};
    use crate::target::{AddressingModel, ExecutionModel, MemoryModel, SpirvTargetEnv};

    #[test]
    fn all_sections_listed_in_order() {
        // Monotonic ordering on the derived PartialOrd.
        for pair in SpirvSection::ALL_SECTIONS.windows(2) {
            assert!(pair[0] < pair[1], "section ordering broken : {pair:?}");
        }
    }

    #[test]
    fn section_names_are_unique() {
        let names: std::collections::HashSet<_> = SpirvSection::ALL_SECTIONS
            .iter()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(names.len(), 11);
    }

    #[test]
    fn new_module_picks_canonical_models() {
        let m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        assert_eq!(m.target_env, SpirvTargetEnv::VulkanKhr1_4);
        assert_eq!(m.memory_model, MemoryModel::Vulkan);
        assert_eq!(m.addressing_model, AddressingModel::PhysicalStorageBuffer64);
        assert_eq!(m.source_language.as_deref(), Some("CSSLv3"));
    }

    #[test]
    fn seed_vulkan_defaults_adds_expected_caps() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        m.seed_vulkan_1_4_defaults();
        assert!(m.capabilities.contains(SpirvCapability::Shader));
        assert!(m
            .capabilities
            .contains(SpirvCapability::PhysicalStorageBufferAddresses));
        assert!(m
            .extensions
            .contains(SpirvExtension::KhrPhysicalStorageBuffer));
        assert!(m.extensions.contains(SpirvExtension::GlslStd450));
    }

    #[test]
    fn seed_opencl_defaults_adds_kernel_cap() {
        let mut m = SpirvModule::new(SpirvTargetEnv::OpenClKernel2_2);
        m.seed_opencl_kernel_defaults();
        assert!(m.capabilities.contains(SpirvCapability::Kernel));
        assert!(m.capabilities.contains(SpirvCapability::Int64));
        assert!(m.capabilities.contains(SpirvCapability::Float64));
    }

    #[test]
    fn entry_point_push_preserves_order() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        m.add_entry_point(SpirvEntryPoint {
            model: ExecutionModel::Vertex,
            name: "main_vs".into(),
            execution_modes: vec![],
        });
        m.add_entry_point(SpirvEntryPoint {
            model: ExecutionModel::Fragment,
            name: "main_fs".into(),
            execution_modes: vec!["OriginUpperLeft".into()],
        });
        assert_eq!(m.entry_points.len(), 2);
        assert_eq!(m.entry_points[0].model, ExecutionModel::Vertex);
        assert_eq!(m.entry_points[1].name, "main_fs");
    }
}
