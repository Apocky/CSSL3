//! CSSLv3 stage0 — WebGPU host submission via wgpu.
//!
//! Authoritative design : `specs/14_BACKEND.csl`.
//!
//! § STATUS : T10 scaffold — wgpu-core integration pending (stage0 stub OK).
//! § NOTE   : wgpu exposes a safe API surface; `unsafe_code` is forbidden in this crate.
//!            Backend-specific internals (raw handle access) remain within wgpu itself.

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
