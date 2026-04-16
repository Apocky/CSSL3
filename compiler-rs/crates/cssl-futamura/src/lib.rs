//! CSSLv3 stage0 — P1 / P2 / P3 partial-evaluation infrastructure.
//!
//! Authoritative design : `specs/19_FUTAMURA3.csl` + `specs/06_STAGING.csl`.
//!
//! § STATUS : T8 scaffold — projection infrastructure pending.
//! § PROJECTIONS
//!   - P1 : `spec(int, src) → compiled-prog`        (baseline via `@staged`)
//!   - P2 : `spec(spec, int) → compiler`            (stdlib builtin-specializer)
//!   - P3 : `spec(spec, spec) → compiler-generator` (enables self-bootstrap, stage1+)
//! § FIXED-POINT : T16 (OG5) CI-gate : generation-N ≡ generation-N+1 bit-exact.

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
