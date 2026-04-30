//! § cssl-substrate-adjoint — adjoint-method kernel for ADCS full-differentiability.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   ADCS Wave-S core (T11-D303 / W-S-CORE-4) : the backward-pass infrastructure
//!   that lets ANY loss-fn over the rendered Ω-field be differentiated with
//!   respect to ANY learnable parameter (KAN-cell weights, material-coefs,
//!   vertex-positions, NPC-trait-vectors, audio-coefs). The forward pass is
//!   the canonical CFER iteration ; the backward pass runs the same iteration
//!   in reverse with adjoint-state tensors, recovering ∂L/∂θ in O(MAX_ITER)
//!   work — same complexity as forward, modulo constant factors.
//!
//!   Memory budget is bounded by checkpointing : the trajectory is recorded
//!   only every N=16 steps (sweet-spot ≈ √MAX_ITER), and intermediate states
//!   are recomputed on demand from the nearest checkpoint. This brings the
//!   memory footprint from O(MAX_ITER · cells · coefs) down to O(√MAX_ITER).
//!
//!   Use-cases (per specs/36_CFER § DIFFERENTIABILITY § Use-cases) :
//!     - train-from-photo   : scene + photo-target → adjoint → fit material-coefs
//!     - train-NPC-behavior : record good-play → adjoint → fit utility-fn-weights
//!     - train-procedural-rules : example-content → adjoint → fit KAN-cell-update-rule
//!     - train-substrate-itself : long-game · KAN-cell weights learned from corpus
//!
//! § SPEC
//!   - `specs/30_SUBSTRATE_v3.csl` § FULL-DIFFERENTIABILITY
//!   - `specs/36_CFER_RENDERER.csl` § DIFFERENTIABILITY (adjoint method ·
//!     checkpointing) — § Forward pass / § Backward pass (adjoint) /
//!     § Checkpointing / § Use-cases.
//!   - `specs/30_SUBSTRATE_v2.csl` § DEFERRED D-1 — KAN-cell update-rule
//!     foundations (lifted by W-S-CORE-3).
//!
//! § PRIME-DIRECTIVE
//!   - Adjoint state is BOUND TO the Σ-mask of its source-cell : if a cell
//!     refuses Sovereign mutation, its parameter cannot accept gradient
//!     updates. The optimizer surfaces such cells as "frozen" rather than
//!     silently overriding consent.
//!   - All gradients are deterministic given (initial-state · params · loss-fn) ;
//!     no hidden non-determinism in checkpoint-recompute.
//!   - Finite-difference validation is built into the test-suite (gradient
//!     check ≤ 1e-3 relative-error against analytic adjoint).
//!
//! § INTEGRATION (Wave-S Core)
//!   - W-S-CORE-3 (cssl-substrate-loa-kan) supplies the KAN update-rule
//!     whose Jacobian we backprop through.
//!   - W-S-CORE-1/2 (omega-field + KAN substrate) supply the FieldCell +
//!     KanNetwork primitives we read forward and write-adjoint.
//!   - W-S-CORE-5 (downstream : full-differentiable renderer) consumes the
//!     `AdjointState` + `Optimizer` to drive scene-fitting jobs.
//!
//! § DESIGN-NOTE
//!   We intentionally implement the adjoint as an INDEPENDENT KERNEL rather
//!   than a Rust-tape (cf. cssl-autodiff which targets compiler-IR-level AD).
//!   The reason : CFER iteration is structurally a fixed-point solver, and
//!   the adjoint of a fixed-point is itself a fixed-point — running the
//!   reverse iteration to convergence is more numerically robust than naive
//!   tape-replay, and avoids the O(MAX_ITER · cells · coefs) tape-memory
//!   blow-up that motivates the checkpointing trick.
//!
//! § ATTESTATION
//!   See [`attestation::ATTESTATION`] — recorded verbatim per
//!   `PRIME_DIRECTIVE §11`.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::float_cmp)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::if_not_else)]
#![allow(clippy::option_if_let_else)]

pub mod adjoint;
pub mod attestation;
pub mod checkpoint;
pub mod loss;
pub mod optimizer;
pub mod parameter;

pub use adjoint::{
    AdjointConfig, AdjointError, AdjointState, BackwardReport, ForwardReport, ForwardTrajectory,
};
pub use checkpoint::{Checkpoint, CheckpointError, CheckpointPolicy, CheckpointStore};
pub use loss::{LossFn, LossKind, LossReport, PerceptualWeights};
pub use optimizer::{
    AdamConfig, AdamOptimizer, LrSchedule, OptimizerError, SgdConfig, SgdOptimizer, StepReport,
};
pub use parameter::{
    Parameter, ParameterError, ParameterId, ParameterKind, ParameterSet, ParameterShape,
};

/// Crate-version stamp.
pub const CSSL_ADJOINT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Crate-name stamp.
pub const CSSL_ADJOINT_CRATE: &str = "cssl-substrate-adjoint";
/// Adjoint-kernel public ABI version. Bumped when the public surface changes.
pub const ADJOINT_SURFACE_VERSION: u32 = 1;
/// Default checkpoint stride (every N=16 forward steps store a checkpoint).
/// Per specs/36 § Checkpointing : N=16 is the documented sweet-spot for
/// MAX_ITER ≈ 64 (memory drops from O(64·cells) to O(√64 · cells) ≈ O(8·cells)).
pub const DEFAULT_CHECKPOINT_STRIDE: u32 = 16;
/// Default maximum CFER iterations per forward pass. Per specs/36 :
/// "Typical : 16-64 iterations per frame for fresh scene · 4-16 for warm-cache".
pub const DEFAULT_MAX_ITER: u32 = 64;
/// Default convergence tolerance for forward iteration (‖L^{(k+1)} - L^{(k)}‖).
/// Per specs/36 : "Iterate until ‖L^{(k+1)} - L^{(k)}‖ < ε   (typically ε = 1e-3)".
pub const DEFAULT_FORWARD_TOL: f32 = 1e-3;

#[cfg(test)]
mod crate_invariants {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CSSL_ADJOINT_CRATE, "cssl-substrate-adjoint");
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!CSSL_ADJOINT_VERSION.is_empty());
    }

    #[test]
    fn surface_version_at_least_one() {
        const _GUARD: () = assert!(ADJOINT_SURFACE_VERSION >= 1);
    }

    #[test]
    fn default_checkpoint_stride_is_documented() {
        assert_eq!(DEFAULT_CHECKPOINT_STRIDE, 16);
    }

    #[test]
    fn default_max_iter_is_documented() {
        assert_eq!(DEFAULT_MAX_ITER, 64);
    }

    #[test]
    fn default_forward_tol_is_documented() {
        assert_eq!(DEFAULT_FORWARD_TOL, 1e-3);
    }
}
