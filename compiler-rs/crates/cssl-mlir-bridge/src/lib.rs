//! CSSLv3 stage0 — mlir-sys + melior wrapper + cssl-dialect op emission (FFI crate).
//!
//! Authoritative design : `specs/15_MLIR.csl`.
//!
//! § STATUS : T6 scaffold — dialect registration + MLIR-ABI bridge pending.
//! § POLICY : `unsafe_code` permitted at FFI boundary only.
//!   Each unsafe block MUST include a `// SAFETY:` comment justifying the invariant.
//! § FALLBACK : if `melior` Windows build-chain blocks, escape-hatch via MLIR-text CLI
//!              (T6 option-b, pre-authorized in `HANDOFF_SESSION_1.csl`).

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
