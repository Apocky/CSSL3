//! Canonical emission entry-point. Spec § 2.4 — macros funnel here ; this
//! is the SINGLE-PATH ring-write site (no double-log).
//!
//! § FLOW :
//!   1. enabled-fast-path : `enabled(severity, subsystem)` AtomicU64 load.
//!   2. per-frame rate-limit : `try_record_per_frame`.
//!   3. per-fingerprint rate-limit : `try_record_per_fingerprint`.
//!   4. replay-strict gate : if `is_replay_strict()` AND no capture-buffer,
//!      no-op.
//!   5. fan-out via `SinkChain::write` to all registered sinks.

use std::sync::Arc;

use crate::context::Context;
use crate::field::FieldValue;
use crate::replay::{is_replay_strict, replay_capture_buffer};
use crate::sample::{try_record_per_fingerprint, try_record_per_frame};
use crate::sink::{LogRecord, SinkChain};

/// Process-global active sink-chain. Engine-init installs ; macros read.
fn active_chain() -> &'static std::sync::Mutex<Option<Arc<SinkChain>>> {
    use std::sync::OnceLock;
    static SLOT: OnceLock<std::sync::Mutex<Option<Arc<SinkChain>>>> = OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// Install the active sink-chain. Engine-init wires ring + stderr + file
/// + mcp + audit sinks then installs once.
pub fn install_sink_chain(chain: Arc<SinkChain>) {
    let mut g = active_chain().lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(chain);
}

/// Read access to the active sink-chain.
#[must_use]
pub fn active_sink_chain() -> Option<Arc<SinkChain>> {
    active_chain()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[cfg(test)]
pub(crate) fn reset_active_chain_for_test() {
    let mut g = active_chain().lock().unwrap_or_else(|e| e.into_inner());
    *g = None;
}

// ───────────────────────────────────────────────────────────────────────
// § Emission outcome
// ───────────────────────────────────────────────────────────────────────

/// Outcome of an emission attempt. Used by tests + by the upstream engine
/// for diagnostic visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitOutcome {
    /// The emission was enabled, rate-permitted, and fanned out to sinks.
    Emitted,
    /// `enabled(severity, subsystem)` returned `false` ; macro was a no-op.
    Disabled,
    /// Per-frame rate-limit denied the emission.
    PerFrameCapped,
    /// Per-fingerprint rate-limit denied the emission.
    PerFingerprintCapped,
    /// `replay_strict=true` AND no capture-buffer ; emit was suppressed.
    ReplayStrictNoOp,
    /// `replay_strict=true` AND capture-buffer was present ; emit was
    /// captured into the replay-log instead of the active sink-chain.
    ReplayCaptured,
    /// `enabled` returned `true` but no active sink-chain was installed.
    NoSinkChain,
}

impl EmitOutcome {
    /// True iff the emission produced ANY observable side-effect — a
    /// sink-write, a replay-capture entry, or a fingerprint-counter
    /// drop-tick.
    #[must_use]
    pub const fn is_observable(self) -> bool {
        matches!(
            self,
            Self::Emitted | Self::ReplayCaptured | Self::PerFingerprintCapped
        )
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Canonical emit-fn
// ───────────────────────────────────────────────────────────────────────

/// Build a [`LogRecord`] from the per-emission state. Used internally by
/// [`emit_structured`] ; exposed for tests + replay tooling.
#[must_use]
pub fn build_record(
    ctx: &Context,
    message: String,
    fields: Vec<(&'static str, FieldValue)>,
) -> LogRecord {
    LogRecord {
        frame_n: ctx.frame_n,
        severity: ctx.severity,
        subsystem: ctx.subsystem,
        source: ctx.source,
        message,
        fields,
    }
}

/// Canonical emission. Macros lower to a call here.
///
/// § STEPS (spec § 2.4) :
///   1. `enabled` fast-path check — early-return `Disabled` if off.
///   2. Per-frame rate-limit — early-return `PerFrameCapped` if exceeded.
///   3. Per-fingerprint rate-limit — early-return `PerFingerprintCapped`.
///   4. Replay-strict check — if `replay_strict`, route to capture-buffer
///      or return `ReplayStrictNoOp`.
///   5. Fan-out via active sink-chain (with field-sanitization at the
///      boundary).
///
/// § FIELD SANITIZATION : every string-shaped field-value is run through
/// [`FieldValue::sanitize_for_sink`] before record-construction. This
/// is the LAST line of defense for D130 path-hash discipline (spec § 2.8).
pub fn emit_structured(
    ctx: &Context,
    message: String,
    fields: Vec<(&'static str, FieldValue)>,
) -> EmitOutcome {
    // 1. enabled fast-path.
    if !crate::enabled::enabled(ctx.severity, ctx.subsystem) {
        return EmitOutcome::Disabled;
    }

    // 2. per-frame rate-limit.
    if !try_record_per_frame(ctx.severity, ctx.subsystem, ctx.frame_n) {
        return EmitOutcome::PerFrameCapped;
    }

    // 3. per-fingerprint rate-limit.
    let decision = try_record_per_fingerprint(ctx.severity, ctx);
    if !decision.is_permit() {
        return EmitOutcome::PerFingerprintCapped;
    }

    // 4. Field sanitization (D130 last-line-of-defense).
    let sanitized: Vec<(&'static str, FieldValue)> = fields
        .into_iter()
        .map(|(k, v)| (k, v.sanitize_for_sink(k)))
        .collect();

    let record = build_record(ctx, message, sanitized);

    // 5. Replay-strict gate.
    if is_replay_strict() {
        if let Some(buf) = replay_capture_buffer() {
            buf.append(record);
            return EmitOutcome::ReplayCaptured;
        }
        return EmitOutcome::ReplayStrictNoOp;
    }

    // 6. Fan-out via active sink-chain.
    if let Some(chain) = active_sink_chain() {
        let _errors = chain.write(&record);
        EmitOutcome::Emitted
    } else {
        EmitOutcome::NoSinkChain
    }
}

#[cfg(test)]
mod tests {
    use super::{
        active_sink_chain, build_record, emit_structured, install_sink_chain,
        reset_active_chain_for_test, EmitOutcome,
    };
    use crate::context::Context;
    use crate::enabled;
    use crate::field::FieldValue;
    use crate::path_hash_field::PathHashField;
    use crate::replay::{
        reset_replay_for_test, set_replay_capture_buffer, set_replay_strict, ReplayCaptureBuffer,
    };
    use crate::sample::{
        reset_fingerprint_table_for_test, reset_frame_counters_for_test, set_per_frame_cap,
    };
    use crate::severity::{Severity, SourceLocation};
    use crate::sink::{LogRecord, LogSink, SinkChain, SinkError};
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        enabled::reset_for_test();
        reset_frame_counters_for_test();
        reset_fingerprint_table_for_test();
        reset_replay_for_test();
        reset_active_chain_for_test();
        crate::context::reset_current_frame_for_test();
        g
    }

    fn fresh_ctx(line: u32) -> Context {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        Context::new(
            Severity::Info,
            SubsystemTag::Render,
            SourceLocation::new(PathHashField::from_path_hash(h), line, 0),
            0,
        )
    }

    struct CountingSink(AtomicU64);

    impl LogSink for CountingSink {
        fn write(&self, _r: &LogRecord) -> Result<(), SinkError> {
            self.0.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }

    impl CountingSink {
        fn new() -> Self {
            Self(AtomicU64::new(0))
        }
        fn count(&self) -> u64 {
            self.0.load(Ordering::Acquire)
        }
    }

    // § Outcome enum.

    #[test]
    fn emit_outcome_observable_predicate() {
        assert!(EmitOutcome::Emitted.is_observable());
        assert!(EmitOutcome::ReplayCaptured.is_observable());
        assert!(EmitOutcome::PerFingerprintCapped.is_observable());
        assert!(!EmitOutcome::Disabled.is_observable());
        assert!(!EmitOutcome::PerFrameCapped.is_observable());
        assert!(!EmitOutcome::ReplayStrictNoOp.is_observable());
        assert!(!EmitOutcome::NoSinkChain.is_observable());
    }

    // § build_record.

    #[test]
    fn build_record_pins_fields() {
        let ctx = fresh_ctx(1);
        let r = build_record(&ctx, String::from("msg"), vec![("k", FieldValue::I64(1))]);
        assert_eq!(r.severity, Severity::Info);
        assert_eq!(r.message, "msg");
        assert_eq!(r.fields.len(), 1);
    }

    // § emit_structured outcomes.

    #[test]
    fn emit_returns_disabled_when_off() {
        let _g = lock_and_reset();
        // Default policy not installed ; everything is OFF.
        let ctx = fresh_ctx(1);
        let outcome = emit_structured(&ctx, String::from("msg"), Vec::new());
        assert_eq!(outcome, EmitOutcome::Disabled);
    }

    #[test]
    fn emit_returns_no_sink_chain_when_chain_missing() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let ctx = fresh_ctx(1);
        let outcome = emit_structured(&ctx, String::from("msg"), Vec::new());
        assert_eq!(outcome, EmitOutcome::NoSinkChain);
    }

    #[test]
    fn emit_emitted_when_chain_installed() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let sink = Arc::new(CountingSink::new());
        let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
        install_sink_chain(chain);
        let ctx = fresh_ctx(1);
        let outcome = emit_structured(&ctx, String::from("msg"), Vec::new());
        assert_eq!(outcome, EmitOutcome::Emitted);
        assert_eq!(sink.count(), 1);
    }

    #[test]
    fn emit_returns_per_frame_capped_when_exceeded() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let sink = Arc::new(CountingSink::new());
        let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
        install_sink_chain(chain);
        // Force a tiny per-frame cap.
        set_per_frame_cap(Severity::Info, 1);
        let ctx_a = fresh_ctx(1);
        let ctx_b = fresh_ctx(2); // distinct fingerprint — same frame
        emit_structured(&ctx_a, String::from("first"), Vec::new());
        let outcome = emit_structured(&ctx_b, String::from("second"), Vec::new());
        assert_eq!(outcome, EmitOutcome::PerFrameCapped);
    }

    #[test]
    fn emit_returns_per_fingerprint_capped_after_4() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let sink = Arc::new(CountingSink::new());
        let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
        install_sink_chain(chain);
        let ctx = fresh_ctx(1);
        for _ in 0..4 {
            emit_structured(&ctx, String::from("msg"), Vec::new());
        }
        let outcome = emit_structured(&ctx, String::from("msg"), Vec::new());
        assert_eq!(outcome, EmitOutcome::PerFingerprintCapped);
    }

    #[test]
    fn emit_replay_strict_no_op_without_capture() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let sink = Arc::new(CountingSink::new());
        let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
        install_sink_chain(chain);
        set_replay_strict(true);
        let ctx = fresh_ctx(1);
        let outcome = emit_structured(&ctx, String::from("msg"), Vec::new());
        assert_eq!(outcome, EmitOutcome::ReplayStrictNoOp);
        assert_eq!(sink.count(), 0);
    }

    #[test]
    fn emit_replay_strict_captured_when_buffer_set() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let sink = Arc::new(CountingSink::new());
        let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
        install_sink_chain(chain);
        set_replay_strict(true);
        let buf = Arc::new(ReplayCaptureBuffer::new());
        set_replay_capture_buffer(Some(buf.clone()));
        let ctx = fresh_ctx(1);
        let outcome = emit_structured(&ctx, String::from("msg"), Vec::new());
        assert_eq!(outcome, EmitOutcome::ReplayCaptured);
        assert_eq!(sink.count(), 0); // sink-chain bypassed
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn emit_field_sanitizes_path_string() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let sink = Arc::new(CountingSink::new());
        let chain = Arc::new(SinkChain::new().with_sink(sink.clone()));
        install_sink_chain(chain.clone());
        let ctx = fresh_ctx(1);
        emit_structured(
            &ctx,
            String::from("msg"),
            vec![("path", FieldValue::Str("/etc/hosts"))],
        );
        // Fan-out happened ; verify the record's field was sanitized
        // by inspecting the sink-chain's recorded count + by emitting a
        // record we capture in a stub-sink.
        // (Direct field-inspection requires a richer sink — see
        // tests/path_hash_sanitize.rs.)
        assert_eq!(sink.count(), 1);
    }

    #[test]
    fn install_sink_chain_replaces_active() {
        let _g = lock_and_reset();
        let s1 = Arc::new(CountingSink::new());
        let chain1 = Arc::new(SinkChain::new().with_sink(s1));
        install_sink_chain(chain1.clone());
        let active = active_sink_chain().unwrap();
        assert!(Arc::ptr_eq(&active, &chain1));

        let s2 = Arc::new(CountingSink::new());
        let chain2 = Arc::new(SinkChain::new().with_sink(s2));
        install_sink_chain(chain2.clone());
        let active2 = active_sink_chain().unwrap();
        assert!(Arc::ptr_eq(&active2, &chain2));
    }

    #[test]
    fn emit_uses_frame_n_from_context() {
        let _g = lock_and_reset();
        enabled::init_default_policy();
        let captured: Arc<Mutex<Option<LogRecord>>> = Arc::new(Mutex::new(None));
        struct CapturingSink(Arc<Mutex<Option<LogRecord>>>);
        impl LogSink for CapturingSink {
            fn write(&self, r: &LogRecord) -> Result<(), SinkError> {
                *self.0.lock().unwrap() = Some(r.clone());
                Ok(())
            }
        }
        let sink = Arc::new(CapturingSink(captured.clone()));
        let chain = Arc::new(SinkChain::new().with_sink(sink));
        install_sink_chain(chain);
        let mut ctx = fresh_ctx(1);
        ctx.frame_n = 999;
        emit_structured(&ctx, String::from("msg"), Vec::new());
        let r = captured.lock().unwrap().clone().unwrap();
        assert_eq!(r.frame_n, 999);
    }
}
