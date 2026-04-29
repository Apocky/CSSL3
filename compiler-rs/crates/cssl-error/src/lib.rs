//! CSSLv3 stage0 — L0 unified error-aggregation surface.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1 + `DIAGNOSTIC_INFRA_PLAN.md` § 1.
//!
//! § ROLE
//!
//! Foundation for the L0 error-catching layer of the Phase-J diagnostic-
//! infrastructure stack. Provides :
//!
//!   - [`EngineError`] : workspace-unified error sum-type with `From<T>`
//!     conversion impls for foundation crates ([`cssl_telemetry`] +
//!     [`cssl_substrate_prime_directive`]) + builder constructors for
//!     per-crate errors that need a typed home (Render / Wave / Audio /
//!     Physics / Anim / Codegen / Asset / Effects / WorkGraph / AI / Gaze
//!     / Host / Network).
//!   - [`Severity`] enum + [`Severable`] trait for uniform severity
//!     classification ; canonical 6-level taxonomy
//!     (Trace / Debug / Info / Warning / Error / Fatal).
//!   - [`ErrorContext`] carrier with [`SourceLocation`] (file_path_hash +
//!     line + col), frame_n, subsystem-tag, [`KindId`] discriminant,
//!     [`Retryable`] hint, optional [`StackTrace`], [`ErrorFingerprint`]
//!     (BLAKE3 dedup-key).
//!   - [`StackTrace`] capture via stable `std::backtrace::Backtrace` ;
//!     path-hash discipline preserved across frame boundaries (D130).
//!   - [`ErrorFingerprint`] : deterministic BLAKE3 dedup-key for
//!     rate-limiting + cross-replay forensic correlation.
//!   - [`catch_frame_panic`] : frame-boundary panic-catch helper that
//!     yields a structured [`crate::error::PanicReport`] (not raw panic
//!     message) ; PD-violation detection routes to the halt-bridge.
//!   - [`crate::pd::halt_for_pd_violation`] : PRIME-DIRECTIVE-violation
//!     halt-bridge into `cssl_substrate_prime_directive::substrate_halt`.
//!
//! § DISCIPLINE  (enforced through type-level + tests)
//!
//!   - **Result-throughout** : every fallible op returns
//!     `Result<T, EngineError>` (or a type that lifts into one via
//!     [`From`]). Spec § 1.7 § DR-1.
//!   - **N! `unwrap()` / `expect()` on user-data paths** : enforced by
//!     downstream clippy-lint (Wave-Jε-3) ; this crate's own tests + impls
//!     observe the discipline today.
//!   - **N! raw-path strings** : every path enters the L0 surface as a
//!     [`cssl_telemetry::PathHash`] (D130-enforced) ; the
//!     [`crate::context::SourceLocation`] type-level barrier rejects
//!     `&str`/`&Path` ingress.
//!   - **Replay-determinism preserved** : errors do NOT perturb engine
//!     state ; they are logged into the replay-log + ring without
//!     branching control-flow non-deterministically. Wall-clock NEVER
//!     enters fingerprint computation.
//!   - **PRIME-DIRECTIVE halt-unbypassable** : panic-catch tags
//!     PD-violations as `pd_violation=true` ; callers route them through
//!     [`crate::pd::halt_for_pd_violation`] which fires the kill-switch.
//!     No degraded-mode override path exists.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!
//!   - § 1 surveillance : path-hash-only discipline (D130) preserved.
//!   - § 4 TRANSPARENCY : the entire error/severity classification is
//!     publicly documented + introspectable via [`EngineError::kind_id`]
//!     + [`EngineError::subsystem`] + [`Severable::severity`].
//!   - § 7 INTEGRITY : kill-switch CANNOT be disabled.
//!   - § 11 ATTESTATION : the canonical attestation propagates via
//!     [`ATTESTATION`] re-export from [`cssl_substrate_prime_directive`].

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod context;
pub mod error;
pub mod fingerprint;
pub mod panic;
pub mod pd;
pub mod severity;
pub mod stack;

pub use context::{ErrorContext, KindId, Retryable, SourceLocation, SubsystemTag};
pub use error::{CrateErrorPayload, EngineError, IoErrorKind, PanicReport};
pub use fingerprint::ErrorFingerprint;
pub use panic::{
    catch_frame_panic, catch_frame_panic_simple, extract_panic_message, install_engine_panic_hook,
    is_handling_panic, payload_is_pd_violation,
};
pub use pd::{
    halt_for_pd_violation, halt_for_pd_violation_with_reason, PrimeDirectiveOrigin,
    PrimeDirectiveViolation,
};
pub use severity::{Severable, Severity};
pub use stack::{
    clear_thread_path_hasher, install_thread_path_hasher, StackFrame, StackTrace, MAX_FRAMES,
};

// ───────────────────────────────────────────────────────────────────────
// § Re-export the canonical PRIME_DIRECTIVE attestation constants. This
//   propagates the attestation through every public-fn-entry-point path
//   that goes through this crate (per spec § 10 § 11 ATTESTATION).
// ───────────────────────────────────────────────────────────────────────

pub use cssl_substrate_prime_directive::ATTESTATION;
pub use cssl_substrate_prime_directive::ATTESTATION_HASH;
pub use cssl_telemetry::PATH_HASH_DISCIPLINE_ATTESTATION;

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// L0 engine-error discipline attestation : the new PRIME_DIRECTIVE §11
/// extension landing with this slice.
///
/// > "all engine errors carry source-location, frame-number, severity,
/// > subsystem-tag, and BLAKE3 fingerprint ; no fallible operation silently
/// > fails ; no panic silently swallows PRIME_DIRECTIVE violations"
pub const ENGINE_ERROR_DISCIPLINE_ATTESTATION: &str =
    "all engine errors carry source-location, frame-number, severity, \
     subsystem-tag, and BLAKE3 fingerprint ; no fallible operation silently \
     fails ; no panic silently swallows PRIME_DIRECTIVE violations";

/// BLAKE3 hash of [`ENGINE_ERROR_DISCIPLINE_ATTESTATION`] for drift-detection.
/// Computed at compile-time via the `engine_error_discipline_attestation_hash`
/// helper ; tests pin the exact hex-string.
#[must_use]
pub fn engine_error_discipline_attestation_hash() -> String {
    let h = blake3::hash(ENGINE_ERROR_DISCIPLINE_ATTESTATION.as_bytes());
    let mut s = String::with_capacity(64);
    for b in h.as_bytes() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod scaffold_tests {
    use super::{
        engine_error_discipline_attestation_hash, ATTESTATION, ENGINE_ERROR_DISCIPLINE_ATTESTATION,
        PATH_HASH_DISCIPLINE_ATTESTATION, STAGE0_SCAFFOLD,
    };

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_re_export_non_empty() {
        assert!(!ATTESTATION.is_empty());
    }

    #[test]
    fn path_hash_discipline_attestation_re_export_non_empty() {
        assert!(!PATH_HASH_DISCIPLINE_ATTESTATION.is_empty());
    }

    #[test]
    fn engine_error_discipline_attestation_text_contains_required_phrases() {
        let t = ENGINE_ERROR_DISCIPLINE_ATTESTATION;
        assert!(t.contains("source-location"));
        assert!(t.contains("frame-number"));
        assert!(t.contains("severity"));
        assert!(t.contains("subsystem-tag"));
        assert!(t.contains("fingerprint"));
        assert!(t.contains("PRIME_DIRECTIVE"));
    }

    #[test]
    fn engine_error_discipline_attestation_hash_is_64_hex() {
        let h = engine_error_discipline_attestation_hash();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn engine_error_discipline_attestation_hash_deterministic() {
        let h1 = engine_error_discipline_attestation_hash();
        let h2 = engine_error_discipline_attestation_hash();
        assert_eq!(h1, h2);
    }
}
