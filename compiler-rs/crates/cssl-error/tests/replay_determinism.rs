//! Replay-determinism preservation tests.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 4.3 + § 7.2.
//!
//! § INVARIANT
//!   - errors do NOT perturb engine state ;
//!   - same input ⟶ same fingerprint, same severity, same kind-id ;
//!   - no wall-clock leak through error context.

use cssl_error::{
    EngineError, ErrorContext, ErrorFingerprint, IoErrorKind, KindId, PanicReport,
    PrimeDirectiveViolation, Retryable, Severable, Severity, SourceLocation, SubsystemTag,
};
use cssl_telemetry::PathHasher;

fn h() -> PathHasher {
    PathHasher::from_seed([0xAB; 32])
}

#[test]
fn fingerprint_replay_idempotent() {
    let p = h().hash_str("/file.rs");
    let loc = SourceLocation::new(p, 42, 13);
    let f1 = ErrorFingerprint::compute(KindId::new(7), &loc, 100);
    let f2 = ErrorFingerprint::compute(KindId::new(7), &loc, 100);
    let f3 = ErrorFingerprint::compute(KindId::new(7), &loc, 100);
    assert_eq!(f1, f2);
    assert_eq!(f2, f3);
}

#[test]
fn fingerprint_no_wall_clock_drift() {
    // Real-time delays between fingerprint computations don't change the hash.
    let p = h().hash_str("/file.rs");
    let loc = SourceLocation::new(p, 1, 1);
    let f_before = ErrorFingerprint::compute(KindId::new(1), &loc, 0);
    std::thread::sleep(std::time::Duration::from_millis(5));
    let f_after = ErrorFingerprint::compute(KindId::new(1), &loc, 0);
    assert_eq!(f_before, f_after);
}

#[test]
fn error_context_minimal_replay_safe() {
    let p = h().hash_str("/x.rs");
    let loc = SourceLocation::new(p, 5, 5);
    let c1 = ErrorContext::minimal(
        loc,
        SubsystemTag::Render,
        "cssl-render",
        KindId::new(1),
        Severity::Error,
    );
    let c2 = ErrorContext::minimal(
        loc,
        SubsystemTag::Render,
        "cssl-render",
        KindId::new(1),
        Severity::Error,
    );
    assert_eq!(c1.fingerprint, c2.fingerprint);
    assert_eq!(c1.subsystem, c2.subsystem);
    assert_eq!(c1.kind, c2.kind);
    assert_eq!(c1.severity, c2.severity);
    assert_eq!(c1.frame_n, c2.frame_n);
}

#[test]
fn error_context_with_frame_n_changes_fingerprint_predictably() {
    let p = h().hash_str("/x.rs");
    let loc = SourceLocation::new(p, 1, 1);
    let c0 = ErrorContext::minimal(
        loc,
        SubsystemTag::Render,
        "cssl-render",
        KindId::new(1),
        Severity::Error,
    );
    // Frames 0..59 = bucket 0 ⟶ same fingerprint.
    let c30 = c0.clone().with_frame_n(30);
    let c59 = c0.clone().with_frame_n(59);
    assert_eq!(c0.fingerprint, c30.fingerprint);
    assert_eq!(c30.fingerprint, c59.fingerprint);
    // Frame 60 = bucket 1 ⟶ different fingerprint.
    let c60 = c0.clone().with_frame_n(60);
    assert_ne!(c0.fingerprint, c60.fingerprint);
}

#[test]
fn engine_error_severity_classification_deterministic() {
    let make = || -> Vec<(EngineError, Severity)> {
        vec![
            (
                EngineError::PrimeDirective(PrimeDirectiveViolation::new("PD0001", "x")),
                Severity::Fatal,
            ),
            (
                EngineError::Audit(cssl_telemetry::AuditError::SignatureInvalid),
                Severity::Fatal,
            ),
            (
                EngineError::Telemetry(cssl_telemetry::RingError::Overflow),
                Severity::Warning,
            ),
            (
                EngineError::PathLog(cssl_telemetry::PathLogError::RawPathInField {
                    field: "x".into(),
                }),
                Severity::Warning,
            ),
            (EngineError::io(IoErrorKind::TimedOut), Severity::Warning),
            (EngineError::io(IoErrorKind::NotFound), Severity::Error),
            (EngineError::other("misc"), Severity::Error),
            (
                EngineError::Panic(PanicReport::new("x", SubsystemTag::Render)),
                Severity::Error,
            ),
            (
                EngineError::Panic(
                    PanicReport::new("x", SubsystemTag::Render).with_pd_violation(true),
                ),
                Severity::Fatal,
            ),
        ]
    };
    for (err, expected) in make() {
        for _ in 0..10 {
            assert_eq!(err.severity(), expected);
        }
    }
}

#[test]
fn engine_error_kind_id_classification_deterministic() {
    let r1 = EngineError::render("c", "x");
    let r2 = EngineError::render("c", "x");
    assert_eq!(r1.kind_id(), r2.kind_id());
    let a1 = EngineError::anim("c", "x");
    let a2 = EngineError::anim("c", "x");
    assert_eq!(a1.kind_id(), a2.kind_id());
    // Different variants ⟶ different kind-ids.
    assert_ne!(r1.kind_id(), a1.kind_id());
}

#[test]
fn engine_error_subsystem_classification_deterministic() {
    let r1 = EngineError::render("c", "x");
    let r2 = EngineError::render("c", "x");
    assert_eq!(r1.subsystem(), r2.subsystem());
}

#[test]
fn retryable_classification_idempotent() {
    let kinds = IoErrorKind::all();
    for k in kinds {
        for _ in 0..5 {
            let e = EngineError::io(*k);
            if let EngineError::Io { retryable, .. } = e {
                assert_eq!(retryable, k.retryable());
            }
        }
    }
}

#[test]
fn fingerprint_dedup_window_of_60_frames() {
    let p = h().hash_str("/x.rs");
    let loc = SourceLocation::new(p, 1, 1);
    let mut seen = std::collections::HashSet::new();
    for f in 0..60 {
        let bucket = ErrorFingerprint::frame_bucket_for(f);
        seen.insert(ErrorFingerprint::compute(KindId::new(1), &loc, bucket));
    }
    assert_eq!(seen.len(), 1, "frames 0..59 must share one fingerprint");
}

#[test]
fn fingerprint_separates_60_frame_windows() {
    let p = h().hash_str("/x.rs");
    let loc = SourceLocation::new(p, 1, 1);
    let mut seen = std::collections::HashSet::new();
    for f in 0..(60 * 5) {
        let bucket = ErrorFingerprint::frame_bucket_for(f);
        seen.insert(ErrorFingerprint::compute(KindId::new(1), &loc, bucket));
    }
    assert_eq!(seen.len(), 5);
}

#[test]
fn replay_two_identical_runs_produce_identical_error_state() {
    fn run() -> Vec<u32> {
        let mut kinds = Vec::new();
        let r1 = EngineError::render("c", "x");
        kinds.push(r1.kind_id().as_u32());
        let r2 = EngineError::wave("c", "x");
        kinds.push(r2.kind_id().as_u32());
        let r3 = EngineError::audio("c", "x");
        kinds.push(r3.kind_id().as_u32());
        kinds
    }
    let a = run();
    let b = run();
    assert_eq!(a, b);
}

#[test]
fn retryable_default_is_no() {
    assert_eq!(Retryable::default(), Retryable::No);
}

#[test]
fn error_context_default_is_unknown_loc() {
    let c = ErrorContext::default();
    assert!(c.source.is_unknown());
    assert_eq!(c.frame_n, 0);
}

#[test]
fn fingerprint_zero_sentinel_distinct_from_computed() {
    let p = h().hash_str("/x.rs");
    let loc = SourceLocation::new(p, 1, 1);
    let computed = ErrorFingerprint::compute(KindId::new(1), &loc, 0);
    let zero = ErrorFingerprint::zero();
    assert_ne!(computed, zero);
    assert!(zero.is_zero());
    assert!(!computed.is_zero());
}
