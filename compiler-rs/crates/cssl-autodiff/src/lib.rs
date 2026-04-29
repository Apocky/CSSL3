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
//! § HIGHER-ORDER FWD-MODE AD (T11-D133, this commit)
//!   - [`Jet<T, N>`] : Taylor-truncation type with `N` stored terms (primal +
//!     `N - 1` derivatives). `Jet<T, 2>` subsumes the existing first-order
//!     [`apply_fwd`] semantics ; `Jet<T, k+1>` extends to `k`-th order.
//!   - [`JetField`] : algebraic vocabulary (f32 + f64 implementations).
//!   - Arithmetic + transcendentals + composition + Hessian-vector-product
//!     surface — see `jet.rs` module-doc for spec-mapping + per-op rationale.
//!
//! § GPU-AD TAPE + JET-ON-GPU (T11-D139, this commit)
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
//! § T7-phase-2c DEFERRED
//!   - Tape-buffer allocation (iso-capability scoped) for control-flow.
//!   - `@checkpoint` attribute recognition.
//!   - Multi-result tangent-tuple emission.
//!   - Killer-app gate : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::module_name_repetitions)]

pub mod decl;
pub mod gpu;
pub mod jet;
pub mod rules;
pub mod substitute;
pub mod transform;
pub mod walker;

pub use decl::{collect_differentiable_fns, DiffDecl};
pub use gpu::{
    select_storage_mode, AtomicAdjointAccumulator, AtomicFallback, AtomicMode, CoopMatrixPath,
    CoopMatrixVendor, GpuAdOp, GpuAdOpName, GpuJet, GpuJetError, GpuTape, GpuTapeError,
    KanGpuForward, KanLayerKind, KanShape, KanVariant, OpRecord, OpRecordKind, OperationDensity,
    RecordedOperand, TapeStats, TapeStorageMode, TileShape, GPU_JET_REGISTER_LIMIT,
};
pub use jet::{hessian_vector_product_1d, hvp_axis, Jet, JetField, MAX_JET_ORDER_PLUS_ONE};
pub use rules::{DiffMode, DiffRule, DiffRuleTable, Primitive};
pub use substitute::{apply_bwd, apply_fwd, SubstitutionReport, TangentMap};
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
