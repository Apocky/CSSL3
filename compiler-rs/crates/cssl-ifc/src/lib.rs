//! CSSLv3 stage0 — Jif-DLM label lattice + declassification + PRIME-DIRECTIVE encoding.
//!
//! Authoritative design : `specs/11_IFC.csl`.
//!
//! § STATUS : T3+ scaffold — label propagation + declass validator pending.
//! § PRIME-DIRECTIVE (immutable) :
//!     `consent=OS • violation=bug • no-override-exists`
//!   encoded structurally via IFC labels + `{Sensitive<dom>}` + `{Audit<dom>}` +
//!   `{Privilege<level>}` effects — NOT as policy attached at runtime.

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
