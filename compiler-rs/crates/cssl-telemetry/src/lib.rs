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
//!   - [`PathHasher`] (T11-D130) — installation-salted BLAKE3 path hasher ;
//!     the canonical surface for path-hash-only logging discipline (F6
//!     observability + PRIME_DIRECTIVE §1 surveillance prohibition).
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
pub mod path_hash;
pub mod ring;
pub mod schema;
pub mod scope;

pub use audit::{
    verify_detached, AuditChain, AuditEntry, AuditError, ContentHash, Signature, SigningKey,
};
pub use exporter::{ChromeTraceExporter, ExportError, Exporter, JsonExporter, OtlpExporter};
pub use path_hash::{
    path_hash_discipline_attestation_hash, PathHash, PathHasher, PathSalt,
    PATH_HASH_DISCIPLINE_ATTESTATION,
};
pub use ring::{RingError, TelemetryRing, TelemetrySlot};
pub use schema::{TelemetrySchema, TelemetryScopeSet};
pub use scope::{TelemetryKind, TelemetryScope};

use thiserror::Error;

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

// ───────────────────────────────────────────────────────────────────────
// § Audit-path-op convenience surface — the canonical entry-point for
//   any code that wants to record a path-touching op into an audit-chain.
//
//   The discipline this enforces : the path is ALREADY a [`PathHash`]
//   (so the type system caught raw-path leaks at the caller), and the
//   `extra` free-form field is scanned for raw-path patterns (`/`, `\`)
//   that would re-introduce a leak through a side-channel. Failed
//   raw-path checks return [`PathLogError`] — they do NOT panic, so the
//   caller can recover (e.g., by stripping the offending field).
// ───────────────────────────────────────────────────────────────────────

/// Failure modes for the audit-path-op surface.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PathLogError {
    /// A free-form field contained what appeared to be a raw filesystem
    /// path (`/` separator or Windows `\` drive prefix). Per the
    /// path-hash-only discipline (T11-D130) this is rejected.
    #[error(
        "PD0018 — raw filesystem path detected in audit-entry field : `{field}` ; \
         only path-hashes are permitted (specs/22 § FS-OPS + PRIME_DIRECTIVE §1)"
    )]
    RawPathInField { field: String },
}

/// Scan a candidate audit-entry field-text for raw-path patterns.
///
/// § HEURISTIC
///   We reject any field that contains :
///     - `/` outside of an obvious enumeration delimiter or hash short-form
///     - `\` outside of an obvious escape sequence in human-readable text
///     - a Windows drive-letter prefix `[A-Za-z]:\\`
///
///   Importantly, the SHORT-form of [`PathHash`] (16 hex + `...`) does NOT
///   trigger a rejection because hex characters never include `/` or `\`.
///
/// Returns `Ok(())` if the field is path-hash-discipline clean,
/// [`PathLogError::RawPathInField`] otherwise. The error carries the
/// offending field-text VERBATIM so debug-output is informative ;
/// production code should NOT log the error's text into the audit-chain
/// (the entire point is that path-strings don't enter the chain).
///
/// # Errors
/// Returns [`PathLogError::RawPathInField`] on any detected raw-path
/// signature.
pub fn audit_path_op_check_raw_path_rejected(field: &str) -> Result<(), PathLogError> {
    // Quick-reject : forward slash. Note : the SHORT-form short_hex() of
    // PathHash uses only hex + "..." so it has no `/`. Any `/` in a free-
    // form audit field is ipso-facto a raw-path leak.
    if field.contains('/') {
        return Err(PathLogError::RawPathInField {
            field: field.to_string(),
        });
    }
    // Backslash : same reasoning — hex+dots has no `\`, and it's the
    // Windows path separator.
    if field.contains('\\') {
        return Err(PathLogError::RawPathInField {
            field: field.to_string(),
        });
    }
    // Windows drive-letter prefix at field-start (e.g., "C:" without a
    // following slash also reads as a drive-letter leak).
    let bytes = field.as_bytes();
    if bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || bytes[2] == b'/' || bytes[2] == b'\\')
    {
        return Err(PathLogError::RawPathInField {
            field: field.to_string(),
        });
    }
    Ok(())
}

/// Record a path-touching op into an [`AuditChain`].
///
/// § ARGUMENTS
///   - `chain`  : the audit-chain to append to.
///   - `tag`    : event tag (e.g., `"fs-open"`, `"fs-write"`, `"save-write"`).
///   - `path`   : 32-byte [`PathHash`] (the only permissible path-form).
///   - `extra`  : free-form additional context (e.g., `"bytes=42 kind=read"`).
///                Validated against raw-path patterns.
///   - `ts`     : timestamp seconds.
///
/// § DISCIPLINE
///   This function is the canonical fs-op audit-entry constructor.
///   Callers cannot fabricate a [`PathHash`] from a `&str` without going
///   through [`PathHasher::hash_str`] (no public ctor accepts strings),
///   which is itself the structural barrier. The `extra` field is
///   scanned for accidentally-passed raw-path strings.
///
/// § ENTRY-FORMAT
///   `tag = <tag>`, `message = "path_hash=<short_hex> <extra>"`.
///
/// # Errors
/// Returns [`PathLogError::RawPathInField`] if `extra` contains raw-path
/// signatures. The audit-entry is NOT appended on error.
pub fn audit_path_op(
    chain: &mut AuditChain,
    tag: impl Into<String>,
    path: PathHash,
    extra: &str,
    ts: u64,
) -> Result<(), PathLogError> {
    audit_path_op_check_raw_path_rejected(extra)?;
    let message = if extra.is_empty() {
        format!("path_hash={path}")
    } else {
        format!("path_hash={path} {extra}")
    };
    chain.append(tag.into(), message, ts);
    Ok(())
}

#[cfg(test)]
mod scaffold_tests {
    use super::{audit_path_op, audit_path_op_check_raw_path_rejected, AuditChain, PathHasher};
    use super::{PathLogError, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn check_helper_accepts_hash_short_form() {
        assert!(audit_path_op_check_raw_path_rejected("deadbeefcafebabe...").is_ok());
        assert!(audit_path_op_check_raw_path_rejected("path_hash=abc123...").is_ok());
        assert!(audit_path_op_check_raw_path_rejected("bytes=42 kind=read").is_ok());
        assert!(audit_path_op_check_raw_path_rejected("").is_ok());
    }

    #[test]
    fn check_helper_rejects_unix_path() {
        let r = audit_path_op_check_raw_path_rejected("path=/etc/hosts");
        assert!(matches!(r, Err(PathLogError::RawPathInField { .. })));
    }

    #[test]
    fn check_helper_rejects_windows_path() {
        let r = audit_path_op_check_raw_path_rejected("path=C:\\users");
        assert!(matches!(r, Err(PathLogError::RawPathInField { .. })));
    }

    #[test]
    fn check_helper_rejects_drive_letter_alone() {
        let r = audit_path_op_check_raw_path_rejected("D:");
        assert!(matches!(r, Err(PathLogError::RawPathInField { .. })));
    }

    #[test]
    fn audit_path_op_appends_hash_only_message() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let h = hasher.hash_str("/etc/hosts");
        let mut chain = AuditChain::new();
        audit_path_op(&mut chain, "fs-open", h, "bytes=0", 100).unwrap();
        assert_eq!(chain.len(), 1);
        let entry = chain.iter().next().unwrap();
        assert_eq!(entry.tag, "fs-open");
        assert!(entry.message.starts_with("path_hash="));
        assert!(entry.message.contains("bytes=0"));
    }

    #[test]
    fn audit_path_op_rejects_raw_path_in_extra() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let h = hasher.hash_str("/etc/hosts");
        let mut chain = AuditChain::new();
        let r = audit_path_op(&mut chain, "fs-open", h, "leaked=/etc/passwd", 100);
        assert!(r.is_err());
        // Chain UNCHANGED on error.
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn audit_path_op_empty_extra_omits_field() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let h = hasher.hash_str("/etc/hosts");
        let mut chain = AuditChain::new();
        audit_path_op(&mut chain, "fs-close", h, "", 100).unwrap();
        let entry = chain.iter().next().unwrap();
        // Message is JUST the path_hash field.
        assert_eq!(entry.message, format!("path_hash={h}"));
    }

    #[test]
    fn path_log_error_message_cites_pd_code() {
        let err = audit_path_op_check_raw_path_rejected("/etc/hosts").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("PD0018"));
        assert!(msg.contains("path-hashes"));
    }
}
