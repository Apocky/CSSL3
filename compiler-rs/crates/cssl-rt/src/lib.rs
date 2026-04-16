//! CSSLv3 stage0 — runtime library (`cssl-rt`).
//!
//! Authoritative design : `specs/01_BOOTSTRAP.csl` + `specs/18_ORTHOPERSIST.csl`.
//!
//! § STATUS : T10+ scaffold — runtime entry-points + persistence bridge pending.
//! § ROLE   : linked into every CSSLv3 artifact; provides allocator, TelemetryRing
//!            hooks, evidence-passing plumbing, and orthopersist image API.

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
