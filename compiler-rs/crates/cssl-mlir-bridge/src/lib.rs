//! CSSLv3 ↔ MLIR bridge — textual emission + melior/mlir-sys FFI stubs.
//!
//! § SPEC : `specs/15_MLIR.csl`.
//!
//! § STATUS (T6-phase-1 / this commit)
//!   Pure-Rust textual emission path — wraps `cssl_mir::print_module` and exposes
//!   `emit_module_to_string` + `emit_module_to_writer` so the compiler driver can
//!   dump MLIR textual format via `--emit-mlir` regardless of whether the melior
//!   FFI is available.
//!
//! § T6-phase-2 (deferred — requires MSVC toolchain per T1-D7)
//!   - melior context / module construction.
//!   - mlir-sys dialect registration (`cssl::registerDialect`).
//!   - `mlir::Operation` construction from `MirOp` via Rust ↔ C++ FFI.
//!   - Pass-pipeline invocation (`mlir-opt`-equivalent via melior).
//!
//! § FALLBACK (pre-authorized at T1, HANDOFF § SUCCESS-GATES + § WHEN-STUCK)
//!   If melior Windows build-chain blocks at T6-entry, this crate's textual-emission
//!   path continues working and the compiler driver pipes output through an external
//!   `mlir-opt` CLI (option-b). cssl-mlir-bridge stays the canonical integration
//!   point regardless.
//!
//! § FFI SAFETY
//!   `unsafe_code` is permitted here *only* at the melior FFI boundary. Until T6-phase-2
//!   brings in actual FFI, this crate contains zero unsafe blocks.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod emit;

pub use emit::{emit_module_to_string, emit_module_to_writer};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
