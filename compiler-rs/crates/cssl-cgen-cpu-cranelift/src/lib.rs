//! CSSLv3 stage0 — Cranelift-based CPU codegen (stage0 throwaway).
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND + `specs/14_BACKEND.csl`.
//!
//! § SCOPE (T10-phase-1 / this commit)
//!   - [`CpuTarget`]         — µarch enum (Alder/Raptor/Meteor/Arrow Lake + Zen4/5 + generic-v3).
//!   - [`SimdTier`]           — SIMD ISA tier (ScalarOnly / Sse2 / Avx2 / Avx512).
//!   - [`CpuFeature`]         — individual feature flags (FMA / BMI1/2 / POPCNT / LZCNT / MOVBE / AVX512F / AVX512DQ / …).
//!   - [`Abi`]                — SysV-AMD64 / Windows-x64 / Darwin-AMD64.
//!   - [`ObjectFormat`]       — ELF / COFF / `MachO`.
//!   - [`CpuTargetProfile`]   — bundle of { target, simd-tier, feature-set, abi, object-format, debug-format }.
//!   - [`emit_module`]        — MIR [`MirModule`] → stage-0 text-CLIF artifact with per-fn skeleton.
//!   - [`CpuCodegenError`]    — emission error enum.
//!
//! § T10-phase-2 DEFERRED
//!   - `cranelift-codegen` + `cranelift-frontend` + `cranelift-module` + `cranelift-object` FFI
//!     integration (pure-Rust so no MSVC block, but heavy build-time ⇒ deferred until validated-
//!     reproducible on Apocky's machine).
//!   - Per-op lowering tables (MIR [`CsslOp`] → CLIF opcodes).
//!   - regalloc2 dispatch + linear-scan fallback.
//!   - Machine-code emission + `cranelift-object` object-file writing (ELF / COFF / `MachO`).
//!   - DWARF-5 + CodeView debug-info emission.
//!   - Runtime CPU-dispatch (AVX2 + AVX-512 multi-variant fat-kernels).
//!
//! [`MirModule`]: cssl_mir::MirModule
//! [`CsslOp`]: cssl_mir::CsslOp

// T11-D20 : `unsafe_code` downgraded from `forbid` to `deny` — JIT execution
// requires casting machine-code addresses to fn-pointers (see `jit.rs`).
// The unsafe use is scoped narrowly + documented with SAFETY comments.
#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod abi;
pub mod build_manifest;
pub mod effect_runtime;
pub mod emit;
pub mod feature;
pub mod jit;
pub mod lower;
pub mod target;
pub mod types;

pub use abi::{Abi, ObjectFormat};
pub use build_manifest::{
    emit_build_manifest_json, read_build_manifest, verify_manifest, verify_manifest_file,
    verify_manifest_json, write_build_manifest, BuildManifest, BuildManifestError, SymbolHash,
};
pub use effect_runtime::{
    effect_witnesses_from_module, is_io_ffi_symbol, verify_effect_row_witnesses,
    EffectRuntimeError, EffectRuntimeReport, EffectRuntimeWitness,
};
pub use emit::{emit_module, CpuCodegenError, EmittedArtifact};
pub use feature::{CpuFeature, CpuFeatureSet, SimdTier};
pub use jit::{JitError, JitFn, JitModule};
pub use lower::{format_operands, format_value, lower_op, ClifInsn};
pub use target::{CpuTarget, CpuTargetProfile, DebugFormat};
pub use types::{clif_type_for, ClifType};

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
