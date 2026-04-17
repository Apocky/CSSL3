//! CSSLv3 stage0 — SPIR-V module emitter for Vulkan/Level-Zero/WebGPU consumption.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — SPIR-V path + `specs/10_HW.csl`
//!         § VULKAN 1.4 BASELINE + `specs/14_BACKEND.csl` § OWNED SPIR-V EMITTER.
//!
//! § SCOPE (T10-phase-1 / this commit)
//!   - [`SpirvCapability`]      — 32-variant catalog of the capabilities CSSLv3 uses
//!     (Shader / Kernel / `PhysicalStorageBufferAddresses` / `RuntimeDescriptorArray` /
//!     `RayTracingKHR` / `CooperativeMatrixKHR` / Int8 / Int16 / Int64 / `Float16` / `Float64` / …).
//!   - [`SpirvExtension`]       — 24-variant catalog (KHR + EXT + INTEL extensions needed
//!     for Arc A770 per `specs/10` § VK-1.4-baseline).
//!   - [`SpirvTargetEnv`]       — target-env flavour (`VulkanKhr1_4` / `UniversalSpirv1_6` /
//!     `OpenClKernel` for Level-Zero).
//!   - [`MemoryModel`] / [`AddressingModel`] / [`ExecutionModel`] — canonical SPIR-V enum mirrors.
//!   - [`SpirvModule`]          — stage-0 builder that enforces the rigid module-section-order from
//!     `specs/07` § SPIR-V EMISSION INVARIANTS.
//!   - [`emit_module`]          — textual disasm-like output of a built module (can be fed to
//!     `spirv-as` or diffed directly in tests).
//!   - [`SpirvEmitError`]       — emission error enum.
//!
//! § T10-phase-2 DEFERRED
//!   - `rspirv` FFI integration (pure-Rust but heavy-build ⇒ reviewed for size-vs-benefit).
//!   - Full MIR [`CsslOp`] → SPIR-V OpCode lowering tables (only signatures + entry points at stage-0).
//!   - `spirv-val` subprocess gate (installed per-CI per `specs/07` § VALIDATION PIPELINE).
//!   - `spirv-opt -O` / `-Os` optimizer subprocess.
//!   - `NonSemantic.Shader.DebugInfo.100` debug-info emission.
//!   - Structured-CFG emission from `scf.*` / `cssl.region.*` ops.
//!
//! [`CsslOp`]: cssl_mir::CsslOp

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod capability;
pub mod emit;
pub mod module;
pub mod target;

pub use capability::{SpirvCapability, SpirvCapabilitySet, SpirvExtension, SpirvExtensionSet};
pub use emit::{emit_module, SpirvEmitError};
pub use module::{SpirvModule, SpirvSection};
pub use target::{AddressingModel, ExecutionModel, MemoryModel, SpirvTargetEnv};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
