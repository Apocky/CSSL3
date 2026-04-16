//! CSSLv3 stage0 — CSLv3-native + Rust-hybrid parser dispatch.
//!
//! Authoritative design : `specs/09_SYNTAX.csl` + `specs/16_DUAL_SURFACE.csl` + `specs/02_IR.csl`.
//! Cross-crate decisions : `DECISIONS.md` (T1-D2).
//!
//! § STATUS : T3 scaffold — parsers + CST construction pending.
//! § OUTPUT : shared AST (see `cssl-ast` crate), elaborated to HIR in `cssl-hir`.

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
