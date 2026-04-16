//! CSSLv3 stage0 — Koka row-polymorphic effects + Xie-Leijen evidence passing.
//!
//! Authoritative design : `specs/04_EFFECTS.csl`.
//!
//! § STATUS : T4 scaffold — 28 built-in effects + row-unification pending.
//! § DISCIPLINE : linear × handler one-shot (R8) — multi-shot + iso ≡ compile-error.
//! § EVIDENCE : HIR-transform synthesizes evidence records per `specs/04_EFFECTS.csl` decl.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    #[test]
    fn scaffold_version_present() {
        assert!(!super::STAGE0_SCAFFOLD.is_empty());
    }
}
