//! CSSLv3 automatic differentiation — source-to-source on HIR + MIR (F1).
//!
//! § SPEC : `specs/05_AUTODIFF.csl`.
//!
//! § SCOPE (T7-phase-2b / this commit)
//!   - [`DiffMode`] : Primal / Fwd / Bwd.
//!   - [`DiffRule`] + [`DiffRuleTable`] : per-primitive rule table covering 15 primitives.
//!   - [`DiffDecl`] + [`collect_differentiable_fns`] : `@differentiable` extraction.
//!   - [`DiffTransform`] + [`DiffVariants`] : HIR-level name-table.
//!   - [`AdWalker`] : MIR-module driver that emits fwd + bwd variants per
//!     `@differentiable` primal.
//!   - [`apply_fwd`] / [`apply_bwd`] : real dual-substitution emitting tangent-
//!     carrying and adjoint-accumulation MIR ops for 10 differentiable primitives
//!     (FAdd / FSub / FMul / FDiv / FNeg + Sqrt / Sin / Cos / Exp / Log).
//!   - [`TangentMap`] + [`SubstitutionReport`] : per-variant diagnostic surface.
//!
//! § HIGHER-ORDER FWD-MODE AD (T11-D133)
//!   - [`Jet<T, N>`] : Taylor-truncation type with `N` stored terms (primal +
//!     `N - 1` derivatives). `Jet<T, 2>` subsumes the existing first-order
//!     [`apply_fwd`] semantics ; `Jet<T, k+1>` extends to `k`-th order.
//!   - [`JetField`] : algebraic vocabulary (f32 + f64 implementations).
//!   - Arithmetic + transcendentals + composition + Hessian-vector-product
//!     surface — see `jet.rs` module-doc for spec-mapping + per-op rationale.
//!
//! § GPU-AD TAPE + JET-ON-GPU (T11-D139)
//!   - [`gpu::GpuTape`] : three-mode tape simulator (thread-local LDS,
//!     workgroup-shared, global-SSBO) with bit-exact record-replay semantics
//!     to the SPIR-V emission produced by the
//!     `cssl-cgen-gpu-spirv::diff_shader` module.
//!   - [`gpu::select_storage_mode`] : op-density-aware storage selector.
//!   - [`gpu::GpuAdOp`] : symbolic MIR-op vocabulary
//!     (`cssl.diff.gpu_tape_alloc` / `record` / `replay`) — emitted via
//!     `CsslOp::Std` at the autodiff-walker.
//!   - [`gpu::GpuJet`] : register-packed Jet for `N ≤ 4`, with shared-memory
//!     spill / reload for larger `N`.
//!   - [`gpu::AtomicAdjointAccumulator`] : `OpAtomicFAdd` with CAS-loop
//!     fallback for shared-adjoint reduction.
//!   - [`gpu::CoopMatrixPath`] : per-vendor cooperative-matrix tile-shape
//!     (Vulkan KHR / D3D12 SM6.9 / Metal-3 simdgroup-matrix) for
//!     batched-Jacobian.
//!   - [`gpu::KanGpuForward`] : KAN-runtime forward-pass adapter — the
//!     integration point T11-D115 hooks for end-to-end GPU training.
//!
//! § CALL-OP AUTO-DISPATCH + CONTROL-FLOW TAPE-RECORD (T11-D140)
//!   - [`CalleeVariantTable`] : map `<primal>` → (`<primal>_fwd`,
//!     `<primal>_bwd`) so fwd/bwd substitution can route `func.call` to the
//!     right callee variant ; auto-built from `@differentiable` fn-list.
//!   - [`marshal_fwd_call_operands`] / [`marshal_bwd_call_operands`] : produce
//!     the dual-arg operand list per spec ("Call(g, args)  ⇒  call g_fwd
//!     (or g_bwd) with dual-args") ; interleaved `[a, d_a, b, d_b, ...]` for
//!     fwd, `[a, b, ..., d_y]` for bwd.
//!   - [`BranchTape`] : per-fn ring-buffer of [`BranchEvent`] cells recording
//!     scf.if arm-index / scf.for/while/loop iter-counts on the fwd pass for
//!     bwd-replay (per spec "If / Loop ⇒ record branch-taken / iter-count on
//!     tape for bwd-replay").
//!   - [`TapeReplay`] : reverse-iteration cursor over a [`BranchTape`].
//!   - `apply_fwd` / `apply_bwd` now :
//!     * recurse into nested scf.if / scf.for / scf.while / scf.loop regions
//!       and emit real tangent / adjoint ops (not just placeholders).
//!     * auto-dispatch func.call to the registered fwd/bwd variant when the
//!       callee is in [`AdWalker::callee_table`].
//!     * stamp record / replay attributes on the structured-CFG ops so the
//!       bwd-pass can pop the matching event from the per-fn tape.
//!
//! § STILL-DEFERRED
//!   - On-device tape-buffer allocation (iso-capability scoped, GPU memory).
//!   - `@checkpoint` attribute recognition (selective recomputation).
//!   - Multi-result tangent-tuple emission.
//!   - Killer-app gate : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic
//!     (will be the next slice that runs the CPU-JIT / cranelift execution
//!     of the bwd-variant end-to-end).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::module_name_repetitions)]
// T11-D140 toolchain-bump compat : pre-existing jet.rs (D133) docstrings use
// over-indented list-continuation bullets that the newer clippy flags. The
// docstrings render correctly as Markdown — fixing them would touch unrelated
// content. Allow at crate-level until a separate docstring-polish slice lands.
// `unknown_lints` guards against pre-1.86 toolchains that don't know the new
// lint name yet.
#![allow(unknown_lints)]
#![allow(clippy::doc_overindented_list_items)]

pub mod call_dispatch;
pub mod decl;
pub mod gpu;
pub mod jet;
pub mod rules;
pub mod substitute;
pub mod tape;
pub mod transform;
pub mod walker;

pub use call_dispatch::{
    marshal_bwd_call_operands, marshal_fwd_call_operands, BwdCallMarshal, CalleeVariantTable,
    CalleeVariants, FwdCallMarshal,
};
pub use decl::{collect_differentiable_fns, DiffDecl};
pub use gpu::{
    select_storage_mode, AtomicAdjointAccumulator, AtomicFallback, AtomicMode, CoopMatrixPath,
    CoopMatrixVendor, GpuAdOp, GpuAdOpName, GpuJet, GpuJetError, GpuTape, GpuTapeError,
    KanGpuForward, KanLayerKind, KanShape, KanVariant, OpRecord, OpRecordKind, OperationDensity,
    RecordedOperand, TapeStats, TapeStorageMode, TileShape, GPU_JET_REGISTER_LIMIT,
};
pub use jet::{hessian_vector_product_1d, hvp_axis, Jet, JetField, MAX_JET_ORDER_PLUS_ONE};
pub use rules::{DiffMode, DiffRule, DiffRuleTable, Primitive};
pub use substitute::{
    apply_bwd, apply_bwd_with_callees, apply_fwd, apply_fwd_with_callees, SubstitutionReport,
    TangentMap,
};
pub use tape::{BranchEvent, BranchTape, TapeError, TapeReplay, DEFAULT_TAPE_CAP};
pub use transform::{DiffTransform, DiffVariants};
pub use walker::{
    op_to_primitive, specialize_transcendental, AdWalker, AdWalkerPass, AdWalkerReport,
};

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
