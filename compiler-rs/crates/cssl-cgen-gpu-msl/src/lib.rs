//! CSSLv3 stage0 — Metal Shading Language emitter via spirv-cross shim.
//!
//! Authoritative design : `specs/07_CODEGEN.csl` + `specs/14_BACKEND.csl`.
//!
//! § STATUS : T10 scaffold — spirv-cross bridge pending (stage0 stub OK).
//! § TARGET : Metal (macOS, iOS, iPadOS).

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
