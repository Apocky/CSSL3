//! CSSLv3 stage0 — MLIR-dialect bridge + structured MIR.
//!
//! Authoritative design : `specs/02_IR.csl` + `specs/15_MLIR.csl`.
//!
//! § STATUS : T6 scaffold — HIR→MIR lowering + cssl-dialect construction pending.
//! § DIALECT : `cssl` (per `specs/15_MLIR.csl` op catalog); reuses
//!             `arith`, `scf`, `linalg`, `affine`, `gpu`, `spirv`, `transform`.

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
