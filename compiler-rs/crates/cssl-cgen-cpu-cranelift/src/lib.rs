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
// § Wave-A integration (T11-D239 follow-up) — Cranelift cgen for the new MIR ops.
// Wave-A1 (f3c2643) tagged-union cgen · Wave-A2 (96b8f65) memref cgen ·
// Wave-A5 (f5ec1c6) heap-dealloc cgen.
pub mod cgen_heap_dealloc;
pub mod cgen_memref;
// § Wave-C4 (S7-F4 / T11-D82) — net-effect cgen helpers parallel to Wave-C3
// `cgen_fs`. Maps `cssl.net.*` MIR ops onto `__cssl_net_*` FFI symbols +
// declares per-fn cranelift `Signature` + provides per-block import-need
// pre-scan via the `NetImportSet` bitfield.
pub mod cgen_net;
pub mod cgen_tagged_union;
// § Wave-A3 cgen integration (b761263) — `cssl.try` lowering helpers reuse
// the Wave-A1 cgen_tagged_union emit-helpers (emit_tag_load /
// emit_tag_eq_compare / emit_payload_load) ; pass-pipeline registration
// runs AFTER tagged_union_abi::expand_module + BEFORE Cranelift cgen drive.
pub mod cgen_try;
pub mod emit;
pub mod feature;
pub mod jit;
pub mod lower;
pub mod object;
pub mod scf;
pub mod target;
pub mod types;

pub use abi::{Abi, ObjectFormat};
pub use emit::{emit_module, CpuCodegenError, EmittedArtifact};
pub use feature::{CpuFeature, CpuFeatureSet, SimdTier};
pub use jit::{JitError, JitFn, JitModule};
pub use lower::{format_operands, format_value, lower_op, ClifInsn};
pub use object::{
    emit_object_module, emit_object_module_with_format, host_default_format, magic_prefix,
    ObjectError,
};
pub use scf::{lower_scf_for, lower_scf_if, lower_scf_loop, lower_scf_while, ScfError};
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
