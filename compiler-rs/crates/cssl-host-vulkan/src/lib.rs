//! CSSLv3 stage0 — Vulkan 1.4.333 host submission via ash (FFI crate).
//!
//! Authoritative design : `specs/10_HW.csl` + `specs/14_BACKEND.csl`.
//!
//! § STATUS : T10 scaffold — extension-set + device-creation + submission pending.
//! § POLICY : `unsafe_code` permitted at FFI boundary only (ash = raw Vulkan FFI).
//!   Each unsafe block MUST include a `// SAFETY:` comment.
//! § EXTENSIONS : per `specs/10_HW.csl` Arc A770 extension-set manifest.

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
