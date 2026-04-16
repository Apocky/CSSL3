//! CSSLv3 stage0 — low-level IR target-emission orchestration.
//!
//! Authoritative design : `specs/07_CODEGEN.csl` + `specs/14_BACKEND.csl`.
//!
//! § STATUS : T10 scaffold — target-emit orchestration pending.
//! § ROLE   : dispatches MIR → `cssl-cgen-cpu-cranelift` / `cssl-cgen-gpu-*` emitters;
//!            assembles fat-binary per `specs/07_CODEGEN.csl`.

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
