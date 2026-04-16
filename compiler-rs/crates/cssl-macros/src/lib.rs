//! CSSLv3 stage0 — Racket-hygienic macros + proc-macro tier-3.
//!
//! Authoritative design : `specs/13_MACROS.csl`.
//!
//! § STATUS : T8 scaffold — tier-1/2/3 dispatch + SyntaxObject hygiene pending.
//! § TIERS
//!   - Tier-1 : `@attr`-macros (compile-time annotations)
//!   - Tier-2 : declarative macros (pattern-directed rewrite)
//!   - Tier-3 : `#run` proc-macros (sandboxed comptime code)
//! § HYGIENE : Racket SyntaxObject model — unified with staging per §§ R3 + R9.

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
