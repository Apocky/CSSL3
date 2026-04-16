//! CSSLv3 stage0 — Metal host submission (cfg-gated macOS / iOS) (FFI crate).
//!
//! Authoritative design : `specs/14_BACKEND.csl`.
//!
//! § STATUS : T10 scaffold — cfg-gated; empty on non-Apple targets.
//! § POLICY : `unsafe_code` permitted at FFI boundary only (`metal` crate bindings).
//!   Each unsafe block MUST include a `// SAFETY:` comment.

#![allow(unsafe_code)]
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
