//! CSSLv3 stage0 — typed-elaborated HIR + inference engine.
//!
//! Authoritative design : `specs/02_IR.csl` + `specs/03_TYPES.csl`.
//!
//! § STATUS : T3 scaffold — elaborator + bidirectional inference pending.
//! § SCOPE  : Hindley-Milner + effect rows + capability inference + IFC-label
//!            propagation + refinement-obligation generation (routed to `cssl-smt`).

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
