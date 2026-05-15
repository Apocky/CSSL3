// § run_observe_gate.rs · spec-70 § item-05 (A05.1 · A05.2 · A05.3) e2e
// ══════════════════════════════════════════════════════════════════════
// Verifies the run-and-observe gate end-to-end at the Rust layer:
//
//   A05.1 · `require-observable` manifest directive + a trace containing
//           ONLY rt-internal events (process.start / process.exit /
//           loa_startup.ctor / sentinel.*) → verifier FAILS with a
//           NoObservableEffect failure.
//   A05.2 · a single `test.observe` event in the trace satisfies the gate.
//   A05.3 · the empty `fn user_main() {}` golden case (simulated as a
//           trace with no user-emitted events) FAILS A05.1.
//
// The full csslc → .exe → run → JSONL → verify pipeline is gated on
// item-04 (build-manifest); this Rust-layer test exercises the same
// verifier code-path that will sit at the end of that pipeline.
// ══════════════════════════════════════════════════════════════════════

use cssl_test_verifier::events::{Event, EventKind};
use cssl_test_verifier::manifest::parse_manifest;
use cssl_test_verifier::verify::{verify, Failure};

/// Build a synthetic trace that contains ONLY rt-internal events — what an
/// empty `fn user_main() {}` program produces (the runtime ctor + atexit fire
/// regardless of user code).
fn rt_internal_only_trace() -> Vec<Event> {
    vec![
        Event {
            ts_ns: 1,
            src: "cssl-rt::loa_startup".into(),
            op: "process.start".into(),
            kind: EventKind::Exit,
            args: serde_json::json!({"pid": 12345}),
            result: serde_json::json!({"ok": true}),
            latency_ns: Some(0),
            note: None,
        },
        Event {
            ts_ns: 2,
            src: "cssl-rt::loa_startup".into(),
            op: "loa_startup.ctor".into(),
            kind: EventKind::Entry,
            args: serde_json::json!({"pid": 12345}),
            result: serde_json::Value::Null,
            latency_ns: None,
            note: None,
        },
        Event {
            ts_ns: 3,
            src: "cssl-rt::loa_startup".into(),
            op: "loa_startup.ctor".into(),
            kind: EventKind::Exit,
            args: serde_json::json!({"pid": 12345}),
            result: serde_json::json!({"banner_emitted": true}),
            latency_ns: Some(100),
            note: None,
        },
        Event {
            ts_ns: 4,
            src: "cssl-rt::loa_startup".into(),
            op: "process.exit".into(),
            kind: EventKind::Exit,
            args: serde_json::json!({}),
            result: serde_json::json!({"ok": true}),
            latency_ns: None,
            note: None,
        },
    ]
}

/// One explicit `cssl_test_observe("hello")` JSONL entry.
fn observe_event(name: &str, ts: u64) -> Event {
    Event {
        ts_ns: ts,
        src: "cssl-rt::test_harness".into(),
        op: "test.observe".into(),
        kind: EventKind::Branch,
        args: serde_json::json!({"name": name}),
        result: serde_json::Value::Null,
        latency_ns: None,
        note: Some("cssl_test_observe".into()),
    }
}

fn manifest_with_observable() -> &'static str {
    "# § spec-70 § item-05 minimal manifest\nrequire-observable\n"
}

#[test]
fn a05_1_empty_user_main_fails_run_observe_gate() {
    // A05.3 : the empty `fn user_main() {}` golden test must fail A05.1.
    let m = parse_manifest(manifest_with_observable(), None).expect("manifest parses");
    assert!(m.requires_observable, "directive should flip the flag");

    let trace = rt_internal_only_trace();
    let report = verify(&m, &trace);

    assert!(
        !report.passed,
        "verifier should reject rt-internal-only trace ; report = {:?}",
        report
    );
    let no_obs = report
        .failures
        .iter()
        .find_map(|f| match f {
            Failure::NoObservableEffect { rt_internal_event_count } => {
                Some(*rt_internal_event_count)
            }
            _ => None,
        })
        .expect("expected NoObservableEffect failure");
    assert_eq!(no_obs, 4, "all 4 rt-internal events should be counted");
}

#[test]
fn a05_2_explicit_observe_event_satisfies_gate() {
    // A test that explicitly registers an observable side-effect via
    // cssl_test_observe(...) passes the gate.
    let m = parse_manifest(manifest_with_observable(), None).expect("manifest parses");
    let mut trace = rt_internal_only_trace();
    trace.push(observe_event("phase-a-item-05", 5));

    let report = verify(&m, &trace);
    assert!(
        report.passed,
        "trace with one test.observe should pass ; failures = {:?}",
        report.failures
    );
}

#[test]
fn gate_off_by_default_when_directive_absent() {
    // The bare manifest (no `require-observable`) does NOT trigger the gate ;
    // a zero-event trace passes vacuously. This preserves the behavior of
    // OQ.03 idle-cycle benchmark tests and existing manifests.
    let bare = "# nothing here\n";
    let m = parse_manifest(bare, None).expect("manifest parses");
    assert!(!m.requires_observable);

    let report = verify(&m, &[]);
    assert!(report.passed, "default behavior should pass empty traces");
}

#[test]
fn user_ffi_event_satisfies_gate_without_explicit_observe() {
    // FM.1 (b)+(c) : any non-rt-internal event counts as observable, not just
    // explicit test.observe. A real csslc-built program that touches the GPU
    // FFI surface (e.g. window.spawn) passes the gate without needing to
    // sprinkle cssl_test_observe calls.
    let m = parse_manifest(manifest_with_observable(), None).expect("manifest parses");
    let mut trace = rt_internal_only_trace();
    trace.push(Event {
        ts_ns: 5,
        src: "cssl-rt::host_window".into(),
        op: "window.spawn".into(),
        kind: EventKind::Entry,
        args: serde_json::json!({"w": 1920, "h": 1080}),
        result: serde_json::Value::Null,
        latency_ns: None,
        note: None,
    });
    let report = verify(&m, &trace);
    assert!(
        report.passed,
        "user FFI event should satisfy the gate ; failures = {:?}",
        report.failures
    );
}

#[test]
fn manifest_directive_is_idempotent() {
    let raw = "require-observable\nrequire-observable\nrequire-observable\n";
    let m = parse_manifest(raw, None).expect("parses");
    assert!(m.requires_observable);
}

#[test]
fn no_observable_effect_failure_is_serializable() {
    // The failure variant must round-trip through serde so the verifier's
    // JSON report (output.rs path) can name-and-shame it in CI logs.
    let f = Failure::NoObservableEffect { rt_internal_event_count: 7 };
    let s = serde_json::to_string(&f).expect("serializable");
    assert!(s.contains("NoObservableEffect"), "got: {}", s);
    assert!(s.contains("7"), "got: {}", s);
}
