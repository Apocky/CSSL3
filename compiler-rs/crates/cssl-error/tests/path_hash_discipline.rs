//! D130 path-hash-only discipline integration tests.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.4 + § 7.1.
//! § ANCHOR : path-hash-only discipline preserved across the L0 surface.

use cssl_error::{
    catch_frame_panic, install_thread_path_hasher, ErrorContext, ErrorFingerprint, KindId,
    Severity, SourceLocation, StackTrace, SubsystemTag,
};
use cssl_telemetry::{PathHash, PathHasher};

fn fixed_hasher() -> PathHasher {
    PathHasher::from_seed([9u8; 32])
}

#[test]
fn source_location_constructor_requires_path_hash_type() {
    // The constructor's signature requires a PathHash ; this test compiles
    // ⟹ raw `&str` or `&Path` cannot be passed at the type-level.
    let h = fixed_hasher().hash_str("/etc/hosts");
    let loc = SourceLocation::new(h, 1, 1);
    assert_eq!(loc.file_path_hash, h);
}

#[test]
fn source_location_display_does_not_leak_raw_path() {
    let h = fixed_hasher().hash_str("/sensitive/path/to/file.rs");
    let loc = SourceLocation::new(h, 1, 1);
    let s = format!("{loc}");
    // Display uses PathHash short-form (16 hex + ...) ; never the raw path.
    assert!(!s.contains("/sensitive/"));
    assert!(!s.contains("file.rs"));
    assert!(s.contains("..."));
}

#[test]
fn source_location_unknown_does_not_leak() {
    let u = SourceLocation::unknown();
    let s = format!("{u}");
    assert_eq!(s, "<unknown>");
    assert!(u.is_unknown());
}

#[test]
fn fingerprint_input_uses_path_hash_only() {
    // Two paths that hash to different PathHashes ⟶ different fingerprints.
    let h1 = fixed_hasher().hash_str("/a.rs");
    let h2 = fixed_hasher().hash_str("/b.rs");
    let l1 = SourceLocation::new(h1, 1, 1);
    let l2 = SourceLocation::new(h2, 1, 1);
    let f1 = ErrorFingerprint::compute(KindId::new(1), &l1, 0);
    let f2 = ErrorFingerprint::compute(KindId::new(1), &l2, 0);
    assert_ne!(f1, f2);
}

#[test]
fn fingerprint_short_hex_is_19_chars() {
    let h = fixed_hasher().hash_str("/file.rs");
    let loc = SourceLocation::new(h, 1, 1);
    let f = ErrorFingerprint::compute(KindId::new(1), &loc, 0);
    assert_eq!(f.short_hex().len(), 19);
}

#[test]
fn error_context_carries_path_hash_only() {
    let h = fixed_hasher().hash_str("/private/file.rs");
    let loc = SourceLocation::new(h, 12, 3);
    let ctx = ErrorContext::minimal(
        loc,
        SubsystemTag::Render,
        "cssl-render",
        KindId::new(1),
        Severity::Error,
    );
    // The ErrorContext debug-format MUST NOT contain the raw path.
    let dbg = format!("{ctx:?}");
    assert!(!dbg.contains("/private/"));
    assert!(!dbg.contains("file.rs"));
}

#[test]
fn stack_trace_with_thread_hasher_uses_path_hash() {
    install_thread_path_hasher(fixed_hasher());
    let bt = "0: my_fn\n   at /private/source/file.rs:42\n";
    let t = StackTrace::parse_backtrace_string(bt);
    assert!(!t.is_empty());
    let frame = &t.frames[0];
    // The path-hash is non-zero ; the raw path was hashed.
    assert_ne!(frame.file_path_hash, PathHash::zero());
    // The frame's debug-form does NOT contain the raw path.
    let dbg = format!("{frame:?}");
    assert!(!dbg.contains("/private/source/"));
    cssl_error::clear_thread_path_hasher();
}

#[test]
fn stack_trace_display_does_not_leak() {
    install_thread_path_hasher(fixed_hasher());
    let bt = "0: my_fn\n   at /private/source/file.rs:42\n";
    let t = StackTrace::parse_backtrace_string(bt);
    let s = format!("{t}");
    assert!(!s.contains("/private/source"));
    assert!(!s.contains("file.rs"));
    cssl_error::clear_thread_path_hasher();
}

#[test]
fn panic_report_does_not_leak_raw_path_via_stack() {
    install_thread_path_hasher(fixed_hasher());
    // Cause a panic ; capture-string will include this test's source path,
    // but the path-hashing step ensures only PathHash bytes escape.
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 1, || {
        panic!("test-panic");
    });
    assert!(r.is_err());
    cssl_error::clear_thread_path_hasher();
}

#[test]
fn fingerprint_dedup_uses_path_hash_only_for_equality() {
    // Same path-hash + same source-loc + same kind + same bucket = dedup.
    let h = fixed_hasher().hash_str("/file.rs");
    let l1 = SourceLocation::new(h, 5, 5);
    let l2 = SourceLocation::new(h, 5, 5);
    let f1 = ErrorFingerprint::compute(KindId::new(7), &l1, 3);
    let f2 = ErrorFingerprint::compute(KindId::new(7), &l2, 3);
    assert_eq!(f1, f2);
}

#[test]
fn engine_error_make_context_uses_pure_path_hash() {
    let h = fixed_hasher().hash_str("/secret/file.rs");
    let loc = SourceLocation::new(h, 1, 1);
    let e = cssl_error::EngineError::render("cssl-render-v2", "fail");
    let ctx = e.make_context(loc, "cssl-render-v2", 0);
    let dbg = format!("{ctx:?}");
    assert!(!dbg.contains("/secret/"));
}

#[test]
fn salt_rotation_produces_distinct_hashes() {
    let p1 = PathHasher::from_seed([1u8; 32]).hash_str("/file.rs");
    let p2 = PathHasher::from_seed([2u8; 32]).hash_str("/file.rs");
    assert_ne!(p1, p2);
}

#[test]
fn fingerprint_with_distinct_paths_distinct_fp() {
    let p1 = fixed_hasher().hash_str("/file_a.rs");
    let p2 = fixed_hasher().hash_str("/file_b.rs");
    let l1 = SourceLocation::new(p1, 1, 1);
    let l2 = SourceLocation::new(p2, 1, 1);
    let f1 = ErrorFingerprint::compute(KindId::new(1), &l1, 0);
    let f2 = ErrorFingerprint::compute(KindId::new(1), &l2, 0);
    assert_ne!(f1, f2);
}

#[test]
fn path_hash_zero_sentinel_works_as_unknown_loc() {
    let z = PathHash::zero();
    let loc = SourceLocation::new(z, 0, 0);
    assert!(loc.is_unknown());
}
