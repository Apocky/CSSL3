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
//! § T10-phase-2 DEFERRED (closed @ T11-D72 / S6-D1)
//!   - `rspirv` FFI integration ✓ (T11-D34) — `binary_emit::emit_module_binary`
//!   - Full MIR [`CsslOp`] → SPIR-V OpCode lowering tables ✓ (T11-D72) —
//!     [`emit_kernel_module`] handles arith, scf, memref, return.
//!   - Structured-CFG emission from `scf.*` ops ✓ (T11-D72) — selection-merge
//!     for `scf.if`, loop-merge for `scf.for/while/loop`.
//!
//! § DIFFERENTIABLE-SHADER EMISSION (T11-D139, this commit)
//!   - [`diff_shader::emit_forward_diff_shader`] : registers a compute
//!     entry-point + declares the GPU-AD capabilities (Float-controls v2,
//!     `AtomicFloat32AddEXT` if native-FAdd, `CooperativeMatrixKHR` if a
//!     coop-matrix path is requested).
//!   - [`diff_shader::emit_reverse_diff_shader`] : mirrors the forward
//!     emission for the reverse-pass (tape-replay walk + atomic adjoint
//!     accumulation).
//!   - [`diff_shader::DiffShaderConfig`] : tape-storage + atomic-mode +
//!     coop-matrix configuration.
//!   - [`diff_shader::reverse_partial_rule`] : per-`OpRecordKind` adjoint-
//!     partial table the reverse-pass body emits.
//!   - [`diff_shader::recognize_gpu_ad_op_name`] : recognizes the
//!     `cssl.diff.gpu_tape_*` op-names emitted via `CsslOp::Std`.
//!
//! § STILL DEFERRED
//!   - `spirv-val` subprocess gate (installed per-CI per `specs/07` § VALIDATION
//!     PIPELINE). The workspace pins `spirv-tools = "0.12"` but the crate links
//!     a heavy C++ toolchain ; T11-D72 keeps the rspirv `dr::load_words`
//!     round-trip as the structural validator and defers the native gate to a
//!     future CI-test slice.
//!   - `spirv-opt -O` / `-Os` optimizer subprocess.
//!   - `NonSemantic.Shader.DebugInfo.100` debug-info emission.
//!   - `cssl.region.*` op emission (region ops are CPU-CPS scaffolding ; not
//!     currently flowed through compute kernels).
//!   - Real fn-parameter passing through Vulkan descriptors / push-constants
//!     (Phase-E host work).
//!   - Heap (`cssl.heap.*`) lowering via USM / BDA ; currently rejected per
//!     [`BodyEmitError::HeapNotSupportedOnGpu`].
//!   - Closure (`cssl.closure*`) lowering via function pointers + indirect
//!     call ; currently rejected per
//!     [`BodyEmitError::ClosuresNotSupportedOnGpu`].
//!
//! [`CsslOp`]: cssl_mir::CsslOp

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod binary_emit;
pub mod body_emit;
pub mod capability;
pub mod diff_shader;
pub mod emit;
pub mod module;
pub mod substrate_kernel;
pub mod target;

pub use binary_emit::{emit_module_binary, BinaryEmitError};
pub use body_emit::{emit_kernel_module, BodyEmitError};
pub use capability::{SpirvCapability, SpirvCapabilitySet, SpirvExtension, SpirvExtensionSet};
pub use diff_shader::{
    declare_diff_shader_caps, emit_forward_diff_shader, emit_reverse_diff_shader,
    recognize_gpu_ad_op_name, reverse_partial_rule, supports_diff_shader, DiffShaderConfig,
    DiffShaderError, ForwardEmitReport, PartialFactor, PartialRule, ReverseEmitReport,
};
pub use emit::{emit_module, SpirvEmitError};
pub use module::{SpirvModule, SpirvSection};
pub use substrate_kernel::{
    emit_substrate_kernel_spirv, emit_substrate_kernel_spirv_bytes, SubstrateKernelEmitError,
    SubstrateKernelSpec,
};
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
