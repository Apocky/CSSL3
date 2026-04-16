//! CSSLv3 stage0 — Z3 + CVC5 + KLEE SMT solver drivers with proof-cache.
//!
//! Authoritative design : `specs/20_SMT.csl`.
//!
//! § STATUS : T9 scaffold — obligation-generator + solver-dispatch + proof-cert pending.
//! § SOLVERS
//!   - Z3    : primary (`z3` crate)
//!   - CVC5  : fallback (verify-registry availability at T9-start)
//!   - KLEE  : symbolic-exec fallback for coverage-guided paths
//! § CACHE   : per-obligation-hash (content-addressed) on disk, invalidated on spec-rev.
//! § CERT    : emitted certificates Ed25519-signed (dev-keys for stage0).

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
