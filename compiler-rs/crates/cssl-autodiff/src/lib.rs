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
//! § T7-phase-2c DEFERRED
//!   - Tape-buffer allocation (iso-capability scoped) for control-flow.
//!   - `@checkpoint` attribute recognition.
//!   - GPU-AD tape-location resolution.
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
pub mod jet;
pub mod rules;
pub mod substitute;
pub mod transform;
pub mod walker;

pub use decl::{collect_differentiable_fns, DiffDecl};
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
