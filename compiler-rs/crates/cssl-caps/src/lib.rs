//! CSSLv3 Pony-6 capability system + Vale gen-refs + linear × handler discipline.
//!
//! § SPEC : `specs/12_CAPABILITIES.csl`.
//!
//! § SCOPE (T5 = T5 proper + T3.4-phase-2 cap-inference slice)
//!   This crate provides the capability algebra used by `cssl-hir` during elaboration :
//!     - [`CapKind`] — the 6 Pony capabilities (`iso / trn / ref / val / box / tag`).
//!     - [`AliasMatrix`] — the compile-time table of `can-alias` / `can-mutate` flags
//!       indexed by [`CapKind`] pairs ; encodes the Pony-6 invariants.
//!     - [`Subtype`] — capability subtyping relation (`iso <: trn`, `trn <: box`, …).
//!     - [`LinearTracker`] — iso-value use-count bookkeeping ; detects
//!       multi-consumption, forgotten-consumption, and escaping-through-closure
//!       violations at block-scope granularity.
//!     - [`GenRef`] — Vale-style packed u64 (u40 index + u24 generation) for
//!       `ref<T>` lowering ; `Handle<T>` ≡ `tag<T>` at source ≡ `GenRef` at MIR.
//!
//! § WHAT'S NOT HERE (T6+ dependencies)
//!   - MIR lowering of `cssl.handle.*` ops (T6 cssl-dialect).
//!   - Runtime `Pool<T>` implementation for gen-ref deref (T10 runtime).
//!   - SMT-backed proofs for cap-obligations (T9).
//!
//! § LINEAGE (per `specs/12` § LINEAGE)
//!   - Pony : Clebsch et al. 2015 "Deny Capabilities for Safe, Fast Actors".
//!   - Vale : Verdagon et al. 2020+ generational-references.
//!   - Eio  : OCaml-5 one-shot-continuations (2022) — linear-via-handler rule.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — tighten @ T5-phase-2 stabilization.
#![allow(clippy::match_same_arms)] // mirror-of-spec tables benefit from explicit arms
#![allow(clippy::struct_excessive_bools)] // AliasRights carries exactly 4 bools per §§ 12
#![allow(clippy::should_implement_trait)] // CapKind::from_str is intentionally an inherent fn

pub mod cap;
pub mod genref;
pub mod linearity;
pub mod matrix;
pub mod subtype;

pub use cap::{CapKind, CapSet};
pub use genref::{GenRef, GEN_BITS, GEN_MASK, IDX_BITS, IDX_MASK};
pub use linearity::{LinearTracker, LinearUse, LinearViolation, UseKind};
pub use matrix::{AliasMatrix, AliasRights, AliasRow};
pub use subtype::{coerce, is_subtype, Subtype, SubtypeError};

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
