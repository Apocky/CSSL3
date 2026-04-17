//! CSSLv3 automatic differentiation — source-to-source on HIR (F1).
//!
//! § SPEC : `specs/05_AUTODIFF.csl`.
//!
//! § SCOPE (T7-phase-1 / this commit)
//!   - [`DiffMode`] : Primal / Fwd / Bwd.
//!   - [`DiffRule`] + [`DiffRuleTable`] : per-primitive rule table covering 15 primitives.
//!   - [`DiffDecl`] + [`collect_differentiable_fns`] : `@differentiable` extraction.
//!   - [`DiffTransform`] + [`DiffVariants`] : skeleton HIR → HIR-with-diff-variants.
//!
//! § T7-phase-2 DEFERRED
//!   - Full rule-application : walking `HirExpr` + applying per-primitive rules.
//!   - Tape-buffer allocation (iso-capability scoped).
//!   - `@checkpoint` attribute recognition.
//!   - GPU-AD tape-location resolution.
//!   - Killer-app gate : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::similar_names)]

pub mod decl;
pub mod rules;
pub mod transform;

pub use decl::{collect_differentiable_fns, DiffDecl};
pub use rules::{DiffMode, DiffRule, DiffRuleTable, Primitive};
pub use transform::{DiffTransform, DiffVariants};

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
