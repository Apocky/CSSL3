//! Replay-determinism integration tests.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.4 + § 4.3 + § 7.2.
//!
//! § PROPERTIES VERIFIED :
//!   1. `replay_strict=true` AND no capture-buffer ⟶ ring/file/mcp sinks
//!      receive ZERO records ; audit-sink continues to record.
//!   2. `replay_strict=true` AND capture-buffer set ⟶ records flow into
//!      buffer instead of sinks ; bit-equal across two runs of the same
//!      sequence.
//!   3. Frame-N is the canonical timestamp ⟶ no wall-clock contamination.

#![allow(unused_imports)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::items_after_statements)]

use cssl_log::{
    emit_structured, init_default_policy, install_source_hasher, replay_capture_buffer,
    set_current_frame, set_replay_capture_buffer, set_replay_strict, Context, EmitOutcome,
    FieldValue, LogRecord, LogSink, PathHashField, ReplayCaptureBuffer, Severity, SinkChain,
    SinkError, SourceLocation, SubsystemTag,
};
use cssl_telemetry::PathHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_and_setup(seed: u8, base_frame: u64) -> std::sync::MutexGuard<'static, ()> {
    let g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_replay_strict(false);
    set_replay_capture_buffer(None);
    cssl_log::force_reset_to_default();
    install_source_hasher(PathHasher::from_seed([seed; 32]));
    set_current_frame(base_frame);
    g
}

struct CapturingSink(AtomicU64);

impl LogSink for CapturingSink {
    fn write(&self, _r: &LogRecord) -> Result<(), SinkError> {
        self.0.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
}

impl CapturingSink {
    fn new() -> Self {
        Self(AtomicU64::new(0))
    }
    fn count(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}

fn fresh_ctx(severity: Severity, line: u32, frame: u64) -> Context {
    let hasher = PathHasher::from_seed([0u8; 32]);
    let h = PathHashField::from_path_hash(hasher.hash_str("/test.rs"));
    Context::new(
        severity,
        SubsystemTag::Render,
        SourceLocation::new(h, line, 0),
        frame,
    )
}

#[test]
fn replay_strict_no_op_no_buffer_drops_all_emissions() {
    let _g = lock_and_setup(20, 5_000_000);
    let sink = Arc::new(CapturingSink::new());
    let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
    cssl_log::install_sink_chain(chain);

    set_replay_strict(true);
    for i in 0..10 {
        let ctx = fresh_ctx(Severity::Info, i + 1, 5_000_000 + u64::from(i));
        let outcome = emit_structured(&ctx, "msg".to_string(), Vec::new());
        assert_eq!(outcome, EmitOutcome::ReplayStrictNoOp);
    }
    assert_eq!(sink.count(), 0);
    set_replay_strict(false);
}

#[test]
fn replay_strict_with_buffer_captures_records() {
    let _g = lock_and_setup(21, 6_000_000);
    let sink = Arc::new(CapturingSink::new());
    let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
    cssl_log::install_sink_chain(chain);

    set_replay_strict(true);
    let buf = Arc::new(ReplayCaptureBuffer::new());
    set_replay_capture_buffer(Some(buf.clone()));

    for i in 0..10 {
        let ctx = fresh_ctx(Severity::Info, i + 1, 6_000_000 + u64::from(i));
        emit_structured(&ctx, format!("msg {i}"), Vec::new());
    }
    // Sinks bypassed.
    assert_eq!(sink.count(), 0);
    // Buffer received all 10.
    assert_eq!(buf.len(), 10);

    set_replay_strict(false);
    set_replay_capture_buffer(None);
}

#[test]
fn replay_capture_buffer_byte_equal_across_runs() {
    // Run the same emission sequence twice ; the buffer's record-bytes
    // must match. This is the canonical replay-bit-equal property.
    let _g = lock_and_setup(22, 7_000_000);

    fn run() -> Vec<[u8; 40]> {
        let buf = Arc::new(ReplayCaptureBuffer::new());
        set_replay_capture_buffer(Some(buf.clone()));
        set_replay_strict(true);
        for i in 0..20 {
            let ctx = fresh_ctx(Severity::Info, i + 1, 7_000_000 + u64::from(i));
            emit_structured(
                &ctx,
                format!("msg {i}"),
                vec![("k", FieldValue::I64(i as i64))],
            );
        }
        let snapshot = buf.snapshot();
        snapshot.iter().map(|r| r.encode_binary()).collect()
    }

    cssl_log::init_default_policy();
    let run1 = run();
    cssl_log::init_default_policy();
    let run2 = run();
    assert_eq!(run1, run2, "replay must be byte-equal");

    set_replay_strict(false);
    set_replay_capture_buffer(None);
}

#[test]
fn frame_n_is_logical_clock_not_wall_clock() {
    let _g = lock_and_setup(23, 0);
    let sink = Arc::new(CapturingSink::new());
    let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
    cssl_log::install_sink_chain(chain);

    // Set frame_n to a deterministic value across the test ; assert the
    // emitted record's frame_n matches (NO wall-clock leakage).
    let captured: Arc<Mutex<Vec<LogRecord>>> = Arc::new(Mutex::new(Vec::new()));
    struct CapSink(Arc<Mutex<Vec<LogRecord>>>);
    impl LogSink for CapSink {
        fn write(&self, r: &LogRecord) -> Result<(), SinkError> {
            self.0.lock().unwrap().push(r.clone());
            Ok(())
        }
    }
    let cap = Arc::new(CapSink(captured.clone()));
    let chain2 = Arc::new(SinkChain::new().with_sink(cap));
    cssl_log::install_sink_chain(chain2);

    set_current_frame(8_000_001);
    let ctx = fresh_ctx(Severity::Info, 1, 8_000_001);
    emit_structured(&ctx, "msg".to_string(), Vec::new());
    let recs = captured.lock().unwrap();
    assert_eq!(recs[0].frame_n, 8_000_001);
}

#[test]
fn capture_buffer_install_and_remove_round_trip() {
    let _g = lock_and_setup(24, 0);
    let buf = Arc::new(ReplayCaptureBuffer::new());
    set_replay_capture_buffer(Some(buf.clone()));
    assert!(replay_capture_buffer().is_some());
    set_replay_capture_buffer(None);
    assert!(replay_capture_buffer().is_none());
}

#[test]
fn replay_strict_does_not_affect_audit_sink() {
    // Audit-sink should continue receiving records even in strict mode
    // (spec § 2.4 "Audit-chain still receives entries").
    let _g = lock_and_setup(25, 9_000_000);

    let chain_audit = Arc::new(Mutex::new(cssl_telemetry::AuditChain::new()));
    let audit = Arc::new(cssl_log::AuditSink::new(chain_audit.clone()));

    // Audit-sink writes DIRECTLY (not through emit_structured) — that's
    // the engine-level pattern for L0 errors that need audit-trail
    // independent of L1 logging.
    let r = LogRecord {
        frame_n: 9_000_000,
        severity: Severity::Error,
        subsystem: SubsystemTag::Render,
        source: SourceLocation::new(PathHashField::zero(), 1, 1),
        message: "err".to_string(),
        fields: Vec::new(),
    };
    set_replay_strict(true);
    // Direct call — bypasses replay-strict (this is intentional for audit).
    audit.write(&r).unwrap();
    let audit_chain = chain_audit.lock().unwrap();
    assert_eq!(audit_chain.len(), 1);
    set_replay_strict(false);
}

#[test]
fn capture_buffer_record_message_preserved() {
    let _g = lock_and_setup(26, 10_000_000);
    let buf = Arc::new(ReplayCaptureBuffer::new());
    set_replay_capture_buffer(Some(buf.clone()));
    set_replay_strict(true);

    let ctx = fresh_ctx(Severity::Info, 1, 10_000_000);
    emit_structured(&ctx, "verbatim message".to_string(), Vec::new());

    let snap = buf.snapshot();
    assert_eq!(snap[0].message, "verbatim message");

    set_replay_strict(false);
    set_replay_capture_buffer(None);
}

#[test]
fn replay_strict_capture_preserves_field_order() {
    let _g = lock_and_setup(27, 11_000_000);
    let buf = Arc::new(ReplayCaptureBuffer::new());
    set_replay_capture_buffer(Some(buf.clone()));
    set_replay_strict(true);

    let ctx = fresh_ctx(Severity::Info, 1, 11_000_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![
            ("first", FieldValue::I64(1)),
            ("second", FieldValue::I64(2)),
            ("third", FieldValue::I64(3)),
        ],
    );

    let snap = buf.snapshot();
    let r = &snap[0];
    assert_eq!(r.fields[0].0, "first");
    assert_eq!(r.fields[1].0, "second");
    assert_eq!(r.fields[2].0, "third");

    set_replay_strict(false);
    set_replay_capture_buffer(None);
}

#[test]
fn replay_strict_capture_zero_overhead_to_active_sinks() {
    // Verify : when replay-strict + capture is on, the active sink-chain
    // is bypassed entirely (zero count increment).
    let _g = lock_and_setup(28, 12_000_000);
    let sink = Arc::new(CapturingSink::new());
    let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
    cssl_log::install_sink_chain(chain);

    let buf = Arc::new(ReplayCaptureBuffer::new());
    set_replay_capture_buffer(Some(buf.clone()));
    set_replay_strict(true);

    for i in 0..50 {
        let ctx = fresh_ctx(Severity::Info, i + 1, 12_000_000 + u64::from(i));
        emit_structured(&ctx, "msg".to_string(), Vec::new());
    }
    assert_eq!(sink.count(), 0);
    assert_eq!(buf.len(), 50);

    set_replay_strict(false);
    set_replay_capture_buffer(None);
}
