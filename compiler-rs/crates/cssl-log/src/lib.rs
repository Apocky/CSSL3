//! CSSLv3 stage0 — L1 structured logging crate.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2 (canonical L1
//! spec) ; root-of-trust : `DIAGNOSTIC_INFRA_PLAN.md` § 2.
//!
//! § ROLE : L1 logging foundation for the diagnostic-infrastructure stack.
//! Sits between L0 [`cssl-error`] (which provides `Severity` +
//! `SourceLocation` types) and L2 telemetry/MCP exporters.
//!
//! § PUBLIC SURFACE :
//!   - 6 level-specific macros : [`trace!`], [`debug!`], [`info!`],
//!     [`warn!`], [`error!`], [`fatal!`] + the generic [`log!`].
//!   - [`enabled`] : AtomicU64 fast-path (≈2ns when disabled).
//!   - [`set_level_floor`] : runtime hot-swap of log-filtering.
//!   - [`Context`] : per-emission state (severity + subsystem + source-loc
//!     + frame-N).
//!   - [`SubsystemTag`] : 21-variant catalog matching engine subsystems.
//!   - [`LogSink`] trait + [`SinkChain`] + 5 sink implementations
//!     (`Ring/Stderr/File/Mcp/Audit`).
//!   - [`Format`] : `JsonLines` / `CslGlyph` / `Binary` wire formats.
//!   - [`emit_structured`] : canonical emission entry-point ; macros
//!     funnel here.
//!   - [`set_replay_strict`] : determinism-strict mode toggle.
//!
//! § INTEGRATION-POINTS :
//!   - [`cssl_telemetry::TelemetryRing`] : ring-buffer for Binary sinks.
//!   - [`cssl_telemetry::PathHasher`] : D130 path-hash discipline.
//!   - [`cssl_telemetry::AuditChain`] : audit-sink target for Error +
//!     Fatal + PD-tagged Warnings.
//!   - `cssl-error` (T11-D155 ; not yet merged) : provides `Severity` +
//!     `SourceLocation` types — currently MOCKED in this crate
//!     ([`severity`] module) per slice-prompt directive. When D155
//!     lands, the swap is `pub use cssl_error::{Severity, SourceLocation};`.
//!
//! § DISCIPLINES PRESERVED :
//!   1. **D130 path-hash-only** : every logged path is a [`PathHashField`]
//!      (newtype around BLAKE3-salt hash) ; no raw `&str`/`&Path`
//!      constructor exists. String-shaped fields are run through
//!      [`cssl_telemetry::audit_path_op_check_raw_path_rejected`] at sink
//!      boundary.
//!   2. **Replay-determinism** : when `replay_strict=true`, ring/file/mcp
//!      sinks are no-op (or captured into [`ReplayCaptureBuffer`]) ;
//!      audit-sink continues. Frame-N is the canonical timestamp (not
//!      wall-clock).
//!   3. **No double-log** : the macros are the canonical entry-point.
//!      App code does NOT call `ring.push` directly. The
//!      [`emit_structured`] funnels through one [`SinkChain`] which owns
//!      the `RingSink`.
//!   4. **Severity classification stable** : `Severity` enum ordering +
//!      u8 wire-encoding pinned (spec § 1.3). Variants append-only.
//!
//! § PHASE-J POSITION : Wave-Jε-2 (T11-D156) ; companion to Wave-Jε-1
//! (T11-D155 cssl-error). Downstream waves : Jε-3 (lints), Jε-4
//! (loa-game panic-boundary), Jζ (telemetry-build), Jθ (debugger-MCP).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
// § Per-crate scaffold allowances (mirrors workspace pedantic clippy).
// Tighten in a follow-up polish slice.
#![allow(clippy::redundant_closure_for_method_calls)] // mutex unwrap_or_else patterns
#![allow(clippy::collapsible_if)] // nested if-let inside outer guard
#![allow(clippy::map_unwrap_or)] // explicit Option::map+unwrap_or chains
#![allow(clippy::if_not_else)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::significant_drop_tightening)] // mutex-guard hold-time clarity
#![allow(clippy::declare_interior_mutable_const)] // const ZERO: AtomicU64 = ... pattern
#![allow(clippy::use_self)]
#![allow(dead_code)] // test-only helpers

// ───────────────────────────────────────────────────────────────────────
// § Module structure
// ───────────────────────────────────────────────────────────────────────

pub mod context;
pub mod emit;
pub mod enabled;
pub mod field;
pub mod format;
pub mod macros;
pub mod path_hash_field;
pub mod replay;
pub mod sample;
pub mod severity;
pub mod sink;
pub mod sink_audit;
pub mod sink_file;
pub mod sink_mcp;
pub mod sink_stderr;
pub mod subsystem;

// ───────────────────────────────────────────────────────────────────────
// § Re-exports (public-surface)
// ───────────────────────────────────────────────────────────────────────

pub use context::{current_frame, set_current_frame, Context};
pub use emit::{active_sink_chain, build_record, emit_structured, install_sink_chain, EmitOutcome};
pub use enabled::{
    disable, disable_severity_all, enable, enable_severity_all, enabled, force_reset_to_default,
    init_default_policy, set_level_floor,
};
pub use field::FieldValue;
pub use format::{
    decode_binary_header, encode_binary, encode_csl_glyph, encode_json_lines, Format,
};
pub use macros::{install_source_hasher, source_hash, source_location_here};
pub use path_hash_field::PathHashField;
pub use replay::{
    is_replay_strict, replay_capture_buffer, set_replay_capture_buffer, set_replay_strict,
    ReplayCaptureBuffer,
};
pub use sample::{
    compute_fingerprint, current_count, per_frame_cap, set_per_frame_cap,
    try_record_per_fingerprint, try_record_per_frame, FingerprintDecision,
};
pub use severity::{Severity, SourceLocation};
pub use sink::{LogRecord, LogSink, RingSink, SinkChain, SinkError};
pub use sink_audit::AuditSink;
pub use sink_file::{rotated_path, FileSink, TelemetryEgressCap};
pub use sink_mcp::{DebugMcpCap, McpSink};
pub use sink_stderr::StderrSink;
pub use subsystem::SubsystemTag;

// ───────────────────────────────────────────────────────────────────────
// § Crate version + canonical attestation
// ───────────────────────────────────────────────────────────────────────

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME_DIRECTIVE §11 attestation (re-exported through cssl-log so
/// downstream code does not have to depend on substrate-prime-directive
/// just to spell the constant).
///
/// § SPEC § 10 + § 13 : the canonical attestation propagates verbatim
/// through every public-fn entry-point. Drift = §7-INTEGRITY violation.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Path-hash discipline attestation (re-exported from cssl-telemetry).
/// D130 extension to PRIME_DIRECTIVE §11.
pub const PATH_HASH_DISCIPLINE_ATTESTATION: &str = cssl_telemetry::PATH_HASH_DISCIPLINE_ATTESTATION;

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, PATH_HASH_DISCIPLINE_ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_canonical_text() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
        );
    }

    #[test]
    fn attestation_matches_telemetry_reexport() {
        // We re-export the path-hash-discipline attestation verbatim ;
        // verify it is the same byte-string as the canonical source.
        assert_eq!(
            PATH_HASH_DISCIPLINE_ATTESTATION,
            cssl_telemetry::PATH_HASH_DISCIPLINE_ATTESTATION
        );
    }

    #[test]
    fn attestation_mentions_no_harm() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn path_hash_attestation_mentions_blake3_and_no_raw_paths() {
        assert!(PATH_HASH_DISCIPLINE_ATTESTATION.contains("BLAKE3"));
        assert!(PATH_HASH_DISCIPLINE_ATTESTATION.contains("no raw paths"));
    }
}
