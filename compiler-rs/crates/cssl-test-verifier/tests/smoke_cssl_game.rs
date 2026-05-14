// § tests/smoke_cssl_game.rs : end-to-end smoke test for the verifier
// ══════════════════════════════════════════════════════════════════
// covers :
//   ✓ inline manifest parse
//   ✓ synthetic JSONL trace parse
//   ✓ verify(...) → pass on full trace
//   ✓ verify(...) → fail w/ MissingRequired when expected event removed
//   ✓ silent-fallback alarm fires for skip-event ∉ allow-skip
//   ✓ count-spec range tolerance
// ══════════════════════════════════════════════════════════════════

use cssl_test_verifier::events::parse_jsonl;
use cssl_test_verifier::manifest::parse_manifest;
use cssl_test_verifier::verify::{verify, Failure};

const MANIFEST: &str = r#"
# § cssl_game.events.csl · stage0 fixture
profile default

require window.create     entry =1
require window.create     exit  =1
require gpu.acquire_image entry [540,660]
require gpu.acquire_image exit  [540,660]

allow-skip telemetry.batch

order window.create  before gpu.acquire_image

result window.create     exit . != 0
result gpu.acquire_image exit .image_idx != 4294967295

latency-max gpu.acquire_image exit 5000000
"#;

fn synthetic_trace_full() -> String {
    let mut out = String::new();
    // window.create entry/exit
    out.push_str(r#"{"ts_ns":1,"src":"cssl-rt::host_window","op":"window.create","kind":"entry","args":{},"result":null,"latency_ns":null,"note":null}"#);
    out.push('\n');
    out.push_str(r#"{"ts_ns":2,"src":"cssl-rt::host_window","op":"window.create","kind":"exit","args":{},"result":42,"latency_ns":1000,"note":null}"#);
    out.push('\n');
    // 600 acquire_image entry+exit pairs
    for i in 0..600u64 {
        let ts_e = 100 + i * 10;
        let ts_x = 100 + i * 10 + 1;
        out.push_str(&format!(
            r#"{{"ts_ns":{},"src":"cssl-rt::host_gpu","op":"gpu.acquire_image","kind":"entry","args":{{}},"result":null,"latency_ns":null,"note":null}}"#,
            ts_e
        ));
        out.push('\n');
        out.push_str(&format!(
            r#"{{"ts_ns":{},"src":"cssl-rt::host_gpu","op":"gpu.acquire_image","kind":"exit","args":{{}},"result":{{"image_idx":{}}},"latency_ns":2000,"note":null}}"#,
            ts_x,
            i % 3
        ));
        out.push('\n');
    }
    // an allowed skip
    out.push_str(r#"{"ts_ns":99999,"src":"cssl-rt::host_telemetry","op":"telemetry.batch","kind":"skip","args":{},"result":null,"latency_ns":null,"note":"allowlisted"}"#);
    out.push('\n');
    out
}

#[test]
fn full_trace_passes() {
    let m = parse_manifest(MANIFEST, None).expect("manifest parses");
    let raw = synthetic_trace_full();
    let events = parse_jsonl(&raw, "synthetic").expect("trace parses");
    let report = verify(&m, &events);
    assert!(
        report.passed,
        "expected pass but got failures = {:#?} / silent = {:#?}",
        report.failures, report.silent_fallbacks
    );
    assert_eq!(report.failed_count, 0);
}

#[test]
fn missing_required_fails() {
    let m = parse_manifest(MANIFEST, None).expect("manifest parses");
    // start from full trace, drop the window.create entry line
    let raw = synthetic_trace_full();
    let filtered: String = raw
        .lines()
        .filter(|l| !l.contains(r#""op":"window.create""#) || !l.contains(r#""kind":"entry""#))
        .collect::<Vec<_>>()
        .join("\n");
    let events = parse_jsonl(&filtered, "synthetic-missing").expect("trace parses");
    let report = verify(&m, &events);
    assert!(!report.passed, "expected fail when window.create entry missing");
    let has_missing = report
        .failures
        .iter()
        .any(|f| matches!(f, Failure::MissingRequired { op, kind, .. } if op == "window.create" && kind == "entry"));
    assert!(has_missing, "expected MissingRequired ; got {:#?}", report.failures);
}

#[test]
fn silent_fallback_fires() {
    let m = parse_manifest(MANIFEST, None).expect("manifest parses");
    // append an unallowed skip event
    let mut raw = synthetic_trace_full();
    raw.push_str(r#"{"ts_ns":999999,"src":"cssl-rt::host_swapchain","op":"swapchain.recreate","kind":"skip","args":{},"result":null,"latency_ns":null,"note":"fallback"}"#);
    raw.push('\n');
    let events = parse_jsonl(&raw, "synthetic-skip").expect("trace parses");
    let report = verify(&m, &events);
    assert!(!report.passed, "unallowed skip must fail the run");
    assert_eq!(report.silent_fallbacks.len(), 1);
    assert_eq!(report.silent_fallbacks[0].op, "swapchain.recreate");
}

#[test]
fn count_range_tolerance() {
    // produce only 539 acquire pairs ← below [540,660]
    let m = parse_manifest(MANIFEST, None).expect("manifest parses");
    let mut out = String::new();
    out.push_str(r#"{"ts_ns":1,"src":"cssl-rt::host_window","op":"window.create","kind":"entry","args":{},"result":null,"latency_ns":null,"note":null}"#);
    out.push('\n');
    out.push_str(r#"{"ts_ns":2,"src":"cssl-rt::host_window","op":"window.create","kind":"exit","args":{},"result":42,"latency_ns":1000,"note":null}"#);
    out.push('\n');
    for i in 0..539u64 {
        let ts_e = 100 + i * 10;
        let ts_x = 100 + i * 10 + 1;
        out.push_str(&format!(
            r#"{{"ts_ns":{},"src":"cssl-rt::host_gpu","op":"gpu.acquire_image","kind":"entry","args":{{}},"result":null,"latency_ns":null,"note":null}}"#,
            ts_e
        ));
        out.push('\n');
        out.push_str(&format!(
            r#"{{"ts_ns":{},"src":"cssl-rt::host_gpu","op":"gpu.acquire_image","kind":"exit","args":{{}},"result":{{"image_idx":1}},"latency_ns":2000,"note":null}}"#,
            ts_x
        ));
        out.push('\n');
    }
    let events = parse_jsonl(&out, "synthetic-low").expect("trace parses");
    let report = verify(&m, &events);
    assert!(!report.passed, "539 < 540 must fail count-deviation");
    let has_dev = report
        .failures
        .iter()
        .any(|f| matches!(f, Failure::CountDeviation { actual: 539, .. }));
    assert!(has_dev, "expected CountDeviation ; got {:#?}", report.failures);
}
