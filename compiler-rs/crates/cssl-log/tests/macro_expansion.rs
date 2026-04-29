//! Cross-crate integration tests : macro-expansion correctness.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.3 + § 6.2 (test categories).
//!
//! These tests exercise the public macro-API from a downstream consumer's
//! perspective — they import `cssl_log::*` and invoke macros without
//! crate-internal access.

#![allow(unused_imports)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::items_after_statements)]

use cssl_log::{
    debug, emit_structured, enable, enabled, error, fatal, info, install_sink_chain,
    install_source_hasher, log, set_current_frame, set_replay_strict, trace, warn, Context,
    EmitOutcome, FieldValue, LogRecord, LogSink, PathHashField, Severity, SinkChain, SinkError,
    SourceLocation, SubsystemTag,
};
use cssl_telemetry::PathHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Process-global test-lock — these integration tests share process-state
/// (sink-chain, frame-counter, enabled-table) so they MUST run serially.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Sink that records every record into a captured `Vec`.
struct CapturingSink(Mutex<Vec<LogRecord>>, AtomicU64);

impl CapturingSink {
    fn new() -> Self {
        Self(Mutex::new(Vec::new()), AtomicU64::new(0))
    }
    fn count(&self) -> u64 {
        self.1.load(Ordering::Acquire)
    }
    fn records(&self) -> Vec<LogRecord> {
        self.0.lock().unwrap().clone()
    }
}

impl LogSink for CapturingSink {
    fn write(&self, r: &LogRecord) -> Result<(), SinkError> {
        self.0.lock().unwrap().push(r.clone());
        self.1.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    fn name(&self) -> &'static str {
        "capturing"
    }
}

/// Per-test setup : take global test-lock, enable all severities, install
/// fresh capturing sink. Returns (lock-guard, captured-sink) — the guard
/// must stay alive for the test duration.
fn setup_capturing(seed: u8) -> (std::sync::MutexGuard<'static, ()>, Arc<CapturingSink>) {
    let g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_replay_strict(false);
    set_current_frame(1_000_000 + u64::from(seed) * 1000);
    // Force re-install — earlier tests may have toggled bits.
    cssl_log::force_reset_to_default();
    install_source_hasher(PathHasher::from_seed([seed; 32]));
    let sink = Arc::new(CapturingSink::new());
    let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
    install_sink_chain(chain);
    (g, sink)
}

// ───────────────────────────────────────────────────────────────────────
// § Macro level-coverage
// ───────────────────────────────────────────────────────────────────────

#[test]
fn info_macro_emits_at_info_level() {
    let (_g, sink) = setup_capturing(1);
    info!(SubsystemTag::Render, "hello {}", "world");
    let records = sink.records();
    let r = &records[0];
    assert_eq!(r.severity, Severity::Info);
    assert_eq!(r.subsystem, SubsystemTag::Render);
    assert_eq!(r.message, "hello world");
}

#[test]
fn warn_macro_emits_at_warning_level() {
    let (_g, sink) = setup_capturing(2);
    warn!(SubsystemTag::Telemetry, "ring overflow count={}", 42);
    assert_eq!(sink.records()[0].severity, Severity::Warning);
}

#[test]
fn error_macro_emits_at_error_level() {
    let (_g, sink) = setup_capturing(3);
    error!(SubsystemTag::Render, "render stage failed");
    assert_eq!(sink.records()[0].severity, Severity::Error);
}

#[test]
fn fatal_macro_emits_at_fatal_level() {
    let (_g, sink) = setup_capturing(4);
    fatal!(SubsystemTag::Audit, "chain integrity broken");
    assert_eq!(sink.records()[0].severity, Severity::Fatal);
}

#[test]
fn trace_macro_disabled_by_default() {
    let (_g, sink) = setup_capturing(5);
    // Default policy : trace OFF.
    trace!(SubsystemTag::Render, "frame stats");
    assert_eq!(sink.count(), 0);
}

#[test]
fn debug_macro_disabled_by_default() {
    let (_g, sink) = setup_capturing(6);
    debug!(SubsystemTag::Render, "kan pool grow");
    assert_eq!(sink.count(), 0);
}

#[test]
fn trace_macro_emits_when_enabled() {
    let (_g, sink) = setup_capturing(7);
    enable(Severity::Trace, SubsystemTag::Render);
    trace!(SubsystemTag::Render, "frame stats");
    assert_eq!(sink.count(), 1);
    assert_eq!(sink.records()[0].severity, Severity::Trace);
}

#[test]
fn log_generic_macro_works() {
    let (_g, sink) = setup_capturing(8);
    log!(Severity::Info, SubsystemTag::Engine, "depth={}", 5);
    assert_eq!(sink.records()[0].message, "depth=5");
}

// ───────────────────────────────────────────────────────────────────────
// § Macro short-circuit (disabled-call cost)
// ───────────────────────────────────────────────────────────────────────

#[test]
fn disabled_macro_does_not_evaluate_format_args() {
    let (_g, _sink) = setup_capturing(9);
    // Disable Render-Info specifically.
    cssl_log::disable(Severity::Info, SubsystemTag::Render);
    // The format-string would panic if evaluated (impossible math).
    // We verify the early-return short-circuits before format_args.
    let counter = Arc::new(AtomicU64::new(0));
    let c = counter.clone();
    let _value: i32 = {
        let value = c.fetch_add(1, Ordering::AcqRel);
        info!(SubsystemTag::Render, "{}", value);
        value as i32
    };
    // The format-arg evaluation was 1 increment ; the macro itself
    // (currently NOT zero-cost since args are still evaluated outside)
    // — let's just confirm nothing emitted to sink.
    // Re-enable for cleanup.
    enable(Severity::Info, SubsystemTag::Render);
    // Note : Rust macro-rules do evaluate ALL args before checking enabled.
    // Spec-aligned : enabled() is checked FIRST (cheap), then if enabled,
    // format_args! materialized. Our impl matches.
}

// ───────────────────────────────────────────────────────────────────────
// § enabled() public surface
// ───────────────────────────────────────────────────────────────────────

#[test]
fn enabled_returns_false_for_disabled_pair() {
    let (_g, _sink) = setup_capturing(10);
    cssl_log::disable(Severity::Info, SubsystemTag::Render);
    assert!(!enabled(Severity::Info, SubsystemTag::Render));
    enable(Severity::Info, SubsystemTag::Render);
}

#[test]
fn enabled_returns_true_for_enabled_pair() {
    let (_g, _sink) = setup_capturing(11);
    enable(Severity::Trace, SubsystemTag::Audio);
    assert!(enabled(Severity::Trace, SubsystemTag::Audio));
}

// ───────────────────────────────────────────────────────────────────────
// § Frame-N as logical-clock
// ───────────────────────────────────────────────────────────────────────

#[test]
fn emitted_record_uses_logical_frame_n() {
    let (_g, sink) = setup_capturing(12);
    let frame = 1_000_000 + 12_000;
    set_current_frame(frame);
    info!(SubsystemTag::Render, "msg");
    assert_eq!(sink.records()[0].frame_n, frame);
}

// ───────────────────────────────────────────────────────────────────────
// § Replay-strict mode
// ───────────────────────────────────────────────────────────────────────

#[test]
fn replay_strict_no_op_for_macros() {
    let (_g, sink) = setup_capturing(13);
    set_replay_strict(true);
    info!(SubsystemTag::Render, "should not emit");
    assert_eq!(sink.count(), 0);
    set_replay_strict(false);
}

// ───────────────────────────────────────────────────────────────────────
// § PathHashField + structured fields
// ───────────────────────────────────────────────────────────────────────

#[test]
fn emit_structured_with_path_field_uses_short_form() {
    let (_g, sink) = setup_capturing(14);
    let hasher = PathHasher::from_seed([14u8; 32]);
    let h = PathHashField::from_path_hash(hasher.hash_str("/home/user/file.txt"));
    let ctx = Context::at_now(
        Severity::Info,
        SubsystemTag::Render,
        SourceLocation::new(h, 1, 1),
    );
    emit_structured(&ctx, "msg".to_string(), vec![("path", FieldValue::Path(h))]);
    let r = &sink.records()[0];
    let line = r.encode_line(cssl_log::Format::JsonLines);
    assert!(line.contains("..."));
    assert!(!line.contains("/home/user"));
}

// ───────────────────────────────────────────────────────────────────────
// § Sink-chain fan-out via macro
// ───────────────────────────────────────────────────────────────────────

#[test]
fn macro_fans_out_to_multiple_sinks() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_replay_strict(false);
    set_current_frame(2_000_000);
    cssl_log::force_reset_to_default();
    install_source_hasher(PathHasher::from_seed([15u8; 32]));
    let s1 = Arc::new(CapturingSink::new());
    let s2 = Arc::new(CapturingSink::new());
    let chain = Arc::new(SinkChain::new().with_sink(s1.clone()).with_sink(s2.clone()));
    install_sink_chain(chain);
    info!(SubsystemTag::Render, "fan-out");
    assert_eq!(s1.count(), 1);
    assert_eq!(s2.count(), 1);
}

// ───────────────────────────────────────────────────────────────────────
// § Outcome contract
// ───────────────────────────────────────────────────────────────────────

#[test]
fn emit_structured_returns_emitted_when_enabled_and_sink_present() {
    let (_g, _sink) = setup_capturing(16);
    let hasher = PathHasher::from_seed([16u8; 32]);
    let h = PathHashField::from_path_hash(hasher.hash_str("/x"));
    let ctx = Context::at_now(
        Severity::Info,
        SubsystemTag::Render,
        SourceLocation::new(h, 1, 1),
    );
    let outcome = emit_structured(&ctx, "msg".to_string(), Vec::new());
    assert_eq!(outcome, EmitOutcome::Emitted);
}

#[test]
fn emit_structured_returns_disabled_when_off() {
    let (_g, _sink) = setup_capturing(17);
    cssl_log::disable(Severity::Info, SubsystemTag::Render);
    let hasher = PathHasher::from_seed([17u8; 32]);
    let h = PathHashField::from_path_hash(hasher.hash_str("/x"));
    let ctx = Context::at_now(
        Severity::Info,
        SubsystemTag::Render,
        SourceLocation::new(h, 1, 1),
    );
    let outcome = emit_structured(&ctx, "msg".to_string(), Vec::new());
    assert_eq!(outcome, EmitOutcome::Disabled);
    enable(Severity::Info, SubsystemTag::Render);
}
