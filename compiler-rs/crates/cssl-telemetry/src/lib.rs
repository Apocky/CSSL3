//! CSSLv3 stage0 — R18 ring-buffer + Level-Zero sysman + OTLP + audit-chain.
//!
//! Authoritative design : `specs/22_TELEMETRY.csl`.
//!
//! § STATUS : T11 scaffold — ring-buffer + exporters + audit-chain pending.
//! § COMPONENTS
//!   - `TelemetryRing<N>` : lock-free ring-buffer, linear-unique capability.
//!   - sysman sampling thread via `cssl-host-level-zero`.
//!   - OTLP exporter (gRPC + HTTP).
//!   - Chrome-trace exporter.
//!   - Audit-chain : BLAKE3 content-hash + Ed25519 signed-append chain.
//! § BUDGET : ≤ 0.5% overhead at `{Telemetry<Counters>}` scope (T26, §§ 23 CI).

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
