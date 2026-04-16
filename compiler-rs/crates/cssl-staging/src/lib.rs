//! CSSLv3 stage0 — `@staged` specializer + `#run` comptime evaluation.
//!
//! Authoritative design : `specs/06_STAGING.csl` + `specs/19_FUTAMURA3.csl`.
//!
//! § STATUS : T8 scaffold — MIR-pass specializer + comptime-native evaluator pending.
//! § NON-GOAL : avoid Zig-style 20× comptime-interpreter slowdown
//!             (R14 — compile-native, not interpret).

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
