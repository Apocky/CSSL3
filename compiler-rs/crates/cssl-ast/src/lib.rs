//! CSSLv3 stage0 — concrete syntax tree + source-preserving forms.
//!
//! Authoritative design : `specs/02_IR.csl`.
//!
//! § STATUS : T3 scaffold — CST node definitions pending.
//! § ROLE   : intermediate between parser output and HIR elaboration;
//!            preserves source-form for round-trip formatter.

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
