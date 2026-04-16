//! CSSLv3 stage0 — orthogonal-persistence image + schema-migration + hot-reload.
//!
//! Authoritative design : `specs/18_ORTHOPERSIST.csl`.
//!
//! § STATUS : T11 scaffold — Image model + schema-derivation + migration chain pending.
//! § LINEAGE : Pharo-class image-based hot-reload, R13 in SYNTHESIS_V2.
//! § HOOK    : `@hot_reload_preserve` attribute marks persistence-root structures.

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
