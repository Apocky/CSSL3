//! GPU-side autodiff tape + Jet-evaluation surface (T11-D139, F1-GPU).
//!
//! § SPEC : `specs/05_AUTODIFF.csl § GPU AUTODIFF` + `specs/17_JETS.csl § GPU
//!         JET (Arc A770)` + `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING §
//!         III GPU EVALUATION PATHS`.
//!
//! § PURPOSE
//!   Wave-3β D133 landed `Jet<T, N>` higher-order forward-mode AD on the CPU
//!   side. This slice (T11-D139) extends the same algebraic surface to the GPU.
//!
//!   1. [`tape::GpuTape`] — three-mode tape-storage (thread-local LDS,
//!   workgroup-shared, global-SSBO) with a density-aware selector. The
//!   CPU-side type acts as a *runtime simulator* of the GPU-side memory
//!   model so the autodiff transforms can be record/replay-tested
//!   without firing up Vulkan ; the same op-codes flow through the
//!   SPIR-V emission helpers in `cssl-cgen-gpu-spirv::diff_shader`.
//!
//!   2. [`storage::TapeStorageMode`] + [`storage::OperationDensity`] —
//!   the heuristic that picks the storage mode. Spec says : sparse /
//!   register-only chains stay in LDS, medium-density chains go to
//!   workgroup-shared, global writes-per-op > register-budget go to
//!   SSBO. `select_storage_mode` is the canonical decision-point.
//!
//!   3. [`mir_ops::GpuAdOp`] — symbolic MIR-op vocabulary
//!   (`cssl.diff.gpu_tape_alloc` / `record` / `replay`). These are
//!   emitted by the autodiff-walker via `CsslOp::Std` with the canonical
//!   name as the std-op marker (avoids inflating the cssl-mir
//!   `ALL_CSSL` enum + cascading test counters at this stage). The
//!   body-emitter in `cssl-cgen-gpu-spirv::diff_shader` recognizes the
//!   names and lowers them to SPIR-V tape-record / atomic-FAdd / OpLoad
//!   sequences.
//!
//!   4. [`jet_gpu::GpuJet`] — register-packed `Jet<T, N>` for `N ≤ 4`.
//!   Spec § 17 GPU-JET says small-N Jets fit the register-file ; this
//!   type makes the packing explicit + offers a `spill_to_shared` /
//!   `reload_from_shared` round-trip for `N > 4` cases.
//!
//!   5. [`atomic::AtomicAdjointAccumulator`] — the shared-adjoint
//!   accumulator. Forward-mode adjoints flow through `OpAtomicFAdd`
//!   (capability `AtomicFloat32AddEXT`) ; the fallback path uses
//!   `OpAtomicCompareExchange` + a CAS loop. Both are CPU-simulated here
//!   for correctness-testing.
//!
//!   6. [`coop_matrix::CoopMatrixPath`] — cooperative-matrix preferred-
//!   path detection (Vulkan KHR_cooperative_matrix / D3D12 SM6.9 /
//!   Metal-3 simdgroup-matrix) so batched-Jacobians can use the matrix
//!   engines on RTX-50 / Quest-4 / RDNA-4 / Apple-M3 / Arc A770 / Quest-
//!   4-class hardware.
//!
//!   7. [`kan::KanGpuForward`] — KAN-runtime forward-pass adapter that
//!   threads `GpuJet` through the canonical KAN-shape (matches the
//!   spectral-BRDF / BRDF-params variants in
//!   `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING § II`). This is the
//!   integration-point the KAN-runtime crate (T11-D115) hooks into
//!   when training a network end-to-end on the GPU.
//!
//! § INTEGRATION-WITH `Jet<T, N>`
//!   Each thread evaluates its own `Jet<T, N>` (CPU-side `crate::Jet` type
//!   reused verbatim — `JetField` blanket-impl works for `f32` + `f64`). The
//!   forward-pass writes the per-thread Jet's primal + tangent slices to
//!   the tape ; the reverse-pass walks the tape backward, accumulating
//!   adjoints atomically into shared parameters.
//!
//! § INTEGRATION-POINTS
//!   - `cssl-render-v2` : differentiable shaders for inverse-rendering
//!     (T11-D116 + T11-D117). The renderer obtains a `GpuTape` from
//!     `tape::alloc_global_ssbo` then dispatches the `*_fwd` shader with the
//!     tape as a SSBO binding. Reverse-pass dispatch uses `*_bwd` + the same
//!     tape buffer (read-back for the user-visible gradient).
//!   - KAN-runtime (T11-D115) : `kan::KanGpuForward::eval` returns a `GpuJet`
//!     so end-to-end-training can chain the KAN-output gradient back through
//!     the spectral-BRDF input.
//!   - cssl-spectral-render (T11-D118) : the spectral-AD path uses
//!     `coop_matrix::CoopMatrixPath::tile_shape_for(vendor)` to pick a
//!     batched-Jacobian tile that fits the vendor's matrix-engine.
//!
//! § REJECTED-DESIGNS (recorded for traceability)
//!   - Adding `GpuTapeAlloc` / `GpuTapeRecord` / `GpuTapeReplay` to
//!     `cssl_mir::CsslOp` was considered + rejected. Reason : it cascades
//!     into the `ALL_CSSL.len() == 48` invariant + the per-op signature
//!     tables + the `OpCategory` exhaustive match in 5+ downstream crates.
//!     The spec is silent on whether GPU-AD ops are first-class IR-level or
//!     pass-internal ; pass-internal-via-`Std` is the lower-friction landing
//!     for v1 and the canonical names (`cssl.diff.gpu_tape_*`) match the
//!     spec text verbatim, so promotion to first-class can land in a
//!     follow-up slice without renaming.
//!   - A separate `cssl-autodiff-gpu` crate was considered but rejected
//!     because Jet types are CPU-side already (so no FFI boundary saved) and
//!     the tape simulator + CPU-side Jet need to share the same `JetField`
//!     trait, which lives in this crate.

#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::float_cmp)] // tape stores exact-record values ; tests compare them.
#![allow(clippy::suboptimal_flops)] // textbook expressions used in tests + analytics.
#![allow(clippy::redundant_closure_for_method_calls)] // method-references kept explicit.
#![allow(clippy::match_same_arms)] // doc-divergent branches share bodies intentionally.
#![allow(clippy::cast_precision_loss)] // factorial arg < 12 ; representable in f64.
#![allow(clippy::match_wildcard_for_single_variants)] // catch-all stays for forward-compat.
#![allow(clippy::option_if_let_else)] // explicit match reads clearer in the recorded form.
#![allow(clippy::many_single_char_names)] // a, b, s, m, y are textbook diff variable names.

pub mod atomic;
pub mod coop_matrix;
pub mod jet_gpu;
pub mod kan;
pub mod mir_ops;
pub mod storage;
pub mod tape;

pub use atomic::{AtomicAdjointAccumulator, AtomicFallback, AtomicMode};
pub use coop_matrix::{CoopMatrixPath, CoopMatrixVendor, TileShape};
pub use jet_gpu::{GpuJet, GpuJetError, GPU_JET_REGISTER_LIMIT};
pub use kan::{KanGpuForward, KanLayerKind, KanShape, KanVariant};
pub use mir_ops::{GpuAdOp, GpuAdOpName};
pub use storage::{select_storage_mode, OperationDensity, TapeStorageMode};
pub use tape::{
    record_op, replay_op, GpuTape, GpuTapeError, OpRecord, OpRecordKind, RecordedOperand, TapeStats,
};
