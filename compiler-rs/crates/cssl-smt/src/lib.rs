//! CSSLv3 SMT — Z3 / CVC5 / KLEE SMT-LIB emission + solver dispatch.
//!
//! § SPEC : `specs/20_SMT.csl`.
//!
//! § SCOPE (T9-phase-1 / this commit)
//!   - [`Theory`]      : LIA / LRA / NRA / BV / UF (SMT-LIB logic names).
//!   - [`Sort`]        : Bool / Int / Real / BitVec(N) / Uninterp(name).
//!   - [`Term`]        : basic SMT-LIB term tree (Var / Lit / App / Forall / Exists / Let).
//!   - [`Query`]       : logic + sort declarations + fn declarations + assertions + check-sat.
//!   - [`emit_smtlib`] : textual emission producing valid SMT-LIB 2.6 format.
//!   - [`Solver`] trait + [`Z3CliSolver`] / [`Cvc5CliSolver`] : subprocess-based solvers
//!     that pipe SMT-LIB text through the respective CLI. No `z3-sys` / `cvc5-sys` FFI
//!     is required — matches the MLIR-text-CLI fallback pattern from T6-phase-1.
//!   - [`discharge`]   : entry point that takes a `cssl_hir::ObligationBag` + solver,
//!     produces per-obligation verdicts.
//!
//! § T9-phase-2 DEFERRED
//!   - Direct `z3-sys` / `cvc5-sys` FFI (requires MSVC toolchain per T1-D7).
//!   - KLEE symbolic-exec fallback for coverage-guided paths.
//!   - Proof-certificate emission + Ed25519-signed certs (R18 audit-chain).
//!   - Per-obligation-hash cache on-disk.
//!   - Full HIR-expression → SMT-Term translation (stage-0 uses text proxies).
//!   - Incremental solving (`push` / `pop`).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::use_self)]

pub mod emit;
pub mod query;
pub mod solver;
pub mod term;

pub use emit::emit_smtlib;
pub use query::{Assertion, FnDecl, Query, Verdict};
pub use solver::{discharge, Cvc5CliSolver, Solver, SolverError, SolverKind, Z3CliSolver};
pub use term::{Literal, Sort, Term, Theory};

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
