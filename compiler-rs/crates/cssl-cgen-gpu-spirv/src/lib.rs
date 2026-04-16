//! CSSLv3 stage0 — rspirv SPIR-V module emitter + spirv-val gate.
//!
//! Authoritative design : `specs/07_CODEGEN.csl` + `specs/14_BACKEND.csl` + `specs/10_HW.csl`.
//!
//! § STATUS : T10 scaffold — MIR cssl+spirv-dialect → rspirv builder pending.
//! § STRUCTURED-CFG : `scf` dialect preserved via `OpSelectionMerge` / `OpLoopMerge` (CC1).
//! § GATE    : `spirv-val` mandatory before emission accepted; `spirv-opt -O` applied post-val.
//! § DEBUG   : `NonSemantic.Shader.DebugInfo.100` source correlation.

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
