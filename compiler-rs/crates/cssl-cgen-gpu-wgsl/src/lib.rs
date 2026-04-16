//! CSSLv3 stage0 — WebGPU Shading Language emitter via Tint shim.
//!
//! Authoritative design : `specs/07_CODEGEN.csl` + `specs/14_BACKEND.csl`.
//!
//! § STATUS : T10 scaffold — Tint bridge pending (stage0 stub OK).
//! § TARGET : WebGPU (browser) — per `specs/14_BACKEND.csl`.

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
