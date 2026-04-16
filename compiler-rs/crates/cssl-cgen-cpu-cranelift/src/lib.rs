//! CSSLv3 stage0 — Cranelift-based CPU codegen (stage0 throwaway).
//!
//! Authoritative design : `specs/07_CODEGEN.csl` + `specs/14_BACKEND.csl`.
//!
//! § STATUS : T10 scaffold — `llvm` subset → CLIF lowering pending.
//! § STAGE  : stage0-only — stage1+ replaces with `cssl-cgen-cpu-owned` per §§ 14_BACKEND.
//! § DEBUG  : DWARF-5 (ELF) + CodeView (COFF) emission per §§ 07_CODEGEN.

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
