//! CSSLv3 stage0 — R18 telemetry ring + audit-chain + exporter surface.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` (R18 observability-first-class).
//!
//! § SCOPE (T11-phase-1 / this commit)
//!   - [`TelemetryScope`]       — 26-variant scope taxonomy per `specs/22` §
//!     TELEMETRY-SCOPE TAXONOMY.
//!   - [`TelemetryKind`]        — event-kind (`Sample` / `SpanBegin` / `SpanEnd` /
//!     `Counter` / `Audit`).
//!   - [`TelemetrySlot`]        — 64-byte ring-slot record.
//!   - [`TelemetryRing`]        — SPSC lock-free ring-buffer with atomic head/tail.
//!   - [`AuditEntry`]           — BLAKE3 content-hash + Ed25519-signature record
//!     (cryptographic primitives stubbed ; full impl at T22).
//!   - [`AuditChain`]           — append-only signed chain w/ genesis-hash anchor.
//!   - [`Exporter`] trait       — OTLP / ChromeTrace / JSON exporter surface.
//!   - [`ChromeTraceExporter`]  — stage-0 JSON-object-per-span writer.
//!   - [`TelemetrySchema`]      — schema metadata for embedded fat-binary section.
//!
//! § T11-phase-2 DEFERRED
//!   - `blake3` / `ed25519-dalek` integration (currently stubbed hashes).
//!   - Real OTLP gRPC + HTTP exporter (needs `prost` / `reqwest`).
//!   - Cross-thread ring-producer (stage-0 is single-thread SPSC only).
//!   - Level-Zero sampling-thread integration (wires via `cssl-host-level-zero`
//!     `TelemetryProbe` when phase-2 adds actual FFI).
//!   - Chrome-trace file-format round-trip + DevTools compatibility check.
//!   - `{Telemetry<S>}` effect-row lowering pass (HIR-level instrumentation).
//!   - Overhead-budget enforcement (0.5% for Counters scope per `specs/22`).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod audit;
pub mod exporter;
pub mod ring;
pub mod schema;
pub mod scope;

pub use audit::{AuditChain, AuditEntry, AuditError};
pub use exporter::{ChromeTraceExporter, ExportError, Exporter, JsonExporter, OtlpExporter};
pub use ring::{RingError, TelemetryRing, TelemetrySlot};
pub use schema::{TelemetrySchema, TelemetryScopeSet};
pub use scope::{TelemetryKind, TelemetryScope};

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
