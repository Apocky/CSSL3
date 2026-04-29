//! [`LogSink`] trait + [`LogRecord`] type + canonical [`RingSink`].
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.6.
//!
//! § SPEC § 2.4 N! double-log : `cssl-log::emit_structured` writes ONE
//! record into ONE [`crate::sink::SinkChain`] which fans out to multiple
//! sinks. The macros are the canonical entry-point ; callers do NOT
//! invoke `ring.push` directly. This module is the seam.

use std::sync::Arc;

use thiserror::Error;

use crate::field::FieldValue;
use crate::format::{encode_binary, encode_csl_glyph, encode_json_lines, Format};
use crate::severity::{Severity, SourceLocation};
use crate::subsystem::SubsystemTag;

/// Canonical log-record passed to every sink in the chain.
///
/// § SHAPE :
///   - `frame_n` : logical-frame-N for replay-determinism (NOT wall-clock).
///   - `severity`, `subsystem` : routing keys.
///   - `source` : (file_path_hash, line, col) — D130 path-hash discipline.
///   - `message` : pre-formatted format-args result.
///   - `fields` : structured key-value pairs ; values pre-sanitized for
///     path-hash discipline at the macro-lowering boundary.
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// Logical frame-N. NEVER wall-clock (replay-determinism).
    pub frame_n: u64,
    /// Severity level.
    pub severity: Severity,
    /// Subsystem tag.
    pub subsystem: SubsystemTag,
    /// Source-location.
    pub source: SourceLocation,
    /// Pre-formatted message (format-args result).
    pub message: String,
    /// Structured key-value fields (path-hash sanitized).
    pub fields: Vec<(&'static str, FieldValue)>,
}

impl LogRecord {
    /// Encode as a single line in the requested format. Used by sinks
    /// that need a string representation (Stderr/File/MCP).
    #[must_use]
    pub fn encode_line(&self, fmt: Format) -> String {
        match fmt {
            Format::JsonLines => encode_json_lines(self),
            Format::CslGlyph => encode_csl_glyph(self),
            // Binary returns 40 bytes ; for text-rendering callers we
            // emit a hex-blob fallback so it shows up correctly.
            Format::Binary => {
                let buf = encode_binary(self);
                let mut s = String::with_capacity(80);
                for b in buf {
                    let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
                }
                s.push('\n');
                s
            }
        }
    }

    /// Encode as the 40-byte binary ring-slot payload.
    #[must_use]
    pub fn encode_binary(&self) -> [u8; 40] {
        encode_binary(self)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Sink trait
// ───────────────────────────────────────────────────────────────────────

/// Sink failure modes (sink-specific implementations may extend via
/// `error_kind` field). Sinks SHOULD NEVER panic — they return
/// [`SinkError`] for the chain to log via a fallback path.
#[derive(Debug, Error)]
pub enum SinkError {
    /// Ring-buffer full ; emission dropped.
    #[error("[ring] sink full ; emission dropped (overflow-counter incremented)")]
    RingFull,
    /// File I/O error.
    #[error("[file] sink i/o : {0}")]
    Io(String),
    /// MCP IPC error.
    #[error("[mcp] sink ipc : {0}")]
    Mcp(String),
    /// Audit-chain append failure (Fatal — caller decides if abort).
    #[error("[audit] sink append : {0}")]
    Audit(String),
    /// Catch-all for sink-specific issues.
    #[error("[sink] {0}")]
    Other(String),
}

/// Log-sink trait. Implementors write a [`LogRecord`] into their backing
/// medium. Sinks are wrapped in [`Arc`]+[`SinkChain`] so multiple sinks
/// can process the same record.
///
/// § THREAD-SAFETY : sinks must be `Send + Sync`. Multi-thread callers
/// share a single sink-chain.
pub trait LogSink: Send + Sync {
    /// Write a record. Returns `Ok` on success ; non-fatal failures
    /// (e.g., ring-overflow) return `Err` — the chain decides whether to
    /// propagate or recover.
    fn write(&self, record: &LogRecord) -> Result<(), SinkError>;

    /// Optional flush hook. Default no-op. File-sinks override to ensure
    /// data hits disk at frame-end checkpoints (spec § 7.2).
    fn flush(&self) -> Result<(), SinkError> {
        Ok(())
    }

    /// Stable name for the sink ; used in test-assertions + diagnostics.
    fn name(&self) -> &'static str {
        "anonymous"
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Sink chain (multi-target fan-out, single-call from macro)
// ───────────────────────────────────────────────────────────────────────

/// A sink-chain : one canonical entry-point for `cssl-log::emit_structured`
/// that fans out to N registered sinks. Per spec § 2.4 "macros are the
/// canonical entry-point ; ring is single-target" — the chain owns ALL
/// sink-targets including the ring, so callers never directly push to
/// the ring (no double-log).
pub struct SinkChain {
    sinks: Vec<Arc<dyn LogSink>>,
}

impl SinkChain {
    /// Build an empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self { sinks: Vec::new() }
    }

    /// Add a sink to the chain. Order matters only for failure propagation
    /// (sinks earlier in the list see records earlier).
    #[must_use]
    pub fn with_sink(mut self, sink: Arc<dyn LogSink>) -> Self {
        self.sinks.push(sink);
        self
    }

    /// Add a sink by `Arc::clone`-ing into the chain.
    pub fn add_sink(&mut self, sink: Arc<dyn LogSink>) {
        self.sinks.push(sink);
    }

    /// Sink count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sinks.len()
    }

    /// True iff zero sinks registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sinks.is_empty()
    }

    /// Fan-out a record to every sink. Errors from individual sinks are
    /// collected ; the chain does NOT abort on first error (every sink
    /// gets a chance to record).
    pub fn write(&self, record: &LogRecord) -> Vec<SinkError> {
        let mut errors = Vec::new();
        for sink in &self.sinks {
            if let Err(e) = sink.write(record) {
                errors.push(e);
            }
        }
        errors
    }

    /// Flush every sink.
    pub fn flush(&self) -> Vec<SinkError> {
        let mut errors = Vec::new();
        for sink in &self.sinks {
            if let Err(e) = sink.flush() {
                errors.push(e);
            }
        }
        errors
    }

    /// Read access to the registered sinks (for introspection / tests).
    #[must_use]
    pub fn sinks(&self) -> &[Arc<dyn LogSink>] {
        &self.sinks
    }
}

impl Default for SinkChain {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § RingSink — always-on ; lossy ; binary-encoded
// ───────────────────────────────────────────────────────────────────────

/// Ring-sink : writes the binary payload into a [`cssl_telemetry::TelemetryRing`].
/// Lossy on overflow (overflow-counter ticks ; emission dropped).
///
/// § SPEC § 2.4 : this is the SINGLE-PATH entry to the ring. Macros call
/// `emit_structured` which fans out through [`SinkChain`] which owns this
/// `RingSink`. Direct `TelemetryRing::push` calls from app code = bug.
///
/// § THREAD-SAFETY : [`cssl_telemetry::TelemetryRing`] is single-thread
/// (uses `RefCell` + `Cell` per stage-0 spec — neither `Send` nor `Sync`).
/// We OWN the ring directly inside a `Mutex` here so multi-threaded engines
/// can safely share a `RingSink` across threads via `Arc<RingSink>`. When
/// the upstream ring lands its atomic SPSC swap (phase-2), the `Mutex`
/// becomes a no-op refactor (replace with direct field).
pub struct RingSink {
    ring: std::sync::Mutex<cssl_telemetry::TelemetryRing>,
}

impl RingSink {
    /// Build a ring-sink with the given capacity (slots).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            ring: std::sync::Mutex::new(cssl_telemetry::TelemetryRing::new(capacity)),
        }
    }

    /// Drain all pending slots (consumer-side).
    pub fn drain_all(&self) -> Vec<cssl_telemetry::TelemetrySlot> {
        let guard = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        guard.drain_all()
    }

    /// Pending-slot count.
    pub fn pending_len(&self) -> usize {
        self.ring
            .lock()
            .map(|g| g.len())
            .unwrap_or_default()
    }

    /// Overflow counter (slots discarded due to capacity).
    pub fn overflow_count(&self) -> u64 {
        self.ring
            .lock()
            .map(|g| g.overflow_count())
            .unwrap_or_default()
    }
}

impl LogSink for RingSink {
    fn write(&self, record: &LogRecord) -> Result<(), SinkError> {
        let payload = record.encode_binary();
        // Use TelemetryScope::Events + TelemetryKind::Sample for log entries.
        // The frame_n IS the timestamp (logical-clock per spec § 7.2).
        let slot = cssl_telemetry::TelemetrySlot::new(
            record.frame_n,
            cssl_telemetry::TelemetryScope::Events,
            cssl_telemetry::TelemetryKind::Sample,
        )
        .with_inline_payload(&payload);
        let guard = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        match guard.push(slot) {
            Ok(()) => Ok(()),
            Err(_) => Err(SinkError::RingFull),
        }
    }

    fn name(&self) -> &'static str {
        "ring"
    }
}

#[cfg(test)]
mod tests {
    use super::{LogRecord, LogSink, RingSink, SinkChain, SinkError};
    use crate::field::FieldValue;
    use crate::format::Format;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn fresh_record() -> LogRecord {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        LogRecord {
            frame_n: 5,
            severity: Severity::Info,
            subsystem: SubsystemTag::Render,
            source: SourceLocation::new(PathHashField::from_path_hash(h), 1, 1),
            message: String::from("msg"),
            fields: vec![("k", FieldValue::I64(7))],
        }
    }

    // § LogRecord encoders.

    #[test]
    fn record_encode_line_json_round_trips() {
        let r = fresh_record();
        let line = r.encode_line(Format::JsonLines);
        assert!(line.starts_with('{'));
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn record_encode_line_csl_glyph_human_readable() {
        let r = fresh_record();
        let line = r.encode_line(Format::CslGlyph);
        assert!(line.contains("frame=5"));
        assert!(line.contains("[render]"));
    }

    #[test]
    fn record_encode_line_binary_hex_form() {
        let r = fresh_record();
        let line = r.encode_line(Format::Binary);
        // 40 bytes × 2 hex chars + "\n" = 81.
        assert_eq!(line.len(), 81);
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn record_encode_binary_40_bytes() {
        let r = fresh_record();
        let buf = r.encode_binary();
        assert_eq!(buf.len(), 40);
    }

    // § SinkChain default + builders.

    #[test]
    fn sink_chain_new_is_empty() {
        let chain = SinkChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn sink_chain_default_is_empty() {
        let chain = SinkChain::default();
        assert!(chain.is_empty());
    }

    // § Mock-sink for chain tests.

    struct CountingSink {
        count: AtomicU64,
        fail: bool,
    }

    impl CountingSink {
        fn new() -> Self {
            Self {
                count: AtomicU64::new(0),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                count: AtomicU64::new(0),
                fail: true,
            }
        }

        fn count(&self) -> u64 {
            self.count.load(Ordering::Acquire)
        }
    }

    impl LogSink for CountingSink {
        fn write(&self, _r: &LogRecord) -> Result<(), SinkError> {
            self.count.fetch_add(1, Ordering::AcqRel);
            if self.fail {
                Err(SinkError::Other(String::from("test-fail")))
            } else {
                Ok(())
            }
        }
        fn name(&self) -> &'static str {
            "counting"
        }
    }

    #[test]
    fn sink_chain_with_sink_increments_len() {
        let s = Arc::new(CountingSink::new());
        let chain = SinkChain::new().with_sink(s);
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn sink_chain_add_sink_works() {
        let mut chain = SinkChain::new();
        chain.add_sink(Arc::new(CountingSink::new()));
        chain.add_sink(Arc::new(CountingSink::new()));
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn sink_chain_write_fans_out_to_all() {
        let s1 = Arc::new(CountingSink::new());
        let s2 = Arc::new(CountingSink::new());
        let chain = SinkChain::new()
            .with_sink(s1.clone())
            .with_sink(s2.clone());
        let r = fresh_record();
        let errs = chain.write(&r);
        assert!(errs.is_empty());
        assert_eq!(s1.count(), 1);
        assert_eq!(s2.count(), 1);
    }

    #[test]
    fn sink_chain_collects_errors_does_not_abort() {
        let s1 = Arc::new(CountingSink::failing());
        let s2 = Arc::new(CountingSink::new());
        let chain = SinkChain::new()
            .with_sink(s1.clone())
            .with_sink(s2.clone());
        let r = fresh_record();
        let errs = chain.write(&r);
        assert_eq!(errs.len(), 1);
        // Both sinks invoked despite first failing.
        assert_eq!(s1.count(), 1);
        assert_eq!(s2.count(), 1);
    }

    #[test]
    fn sink_chain_flush_invokes_each() {
        struct FlushCounter(AtomicU64);
        impl LogSink for FlushCounter {
            fn write(&self, _r: &LogRecord) -> Result<(), SinkError> {
                Ok(())
            }
            fn flush(&self) -> Result<(), SinkError> {
                self.0.fetch_add(1, Ordering::AcqRel);
                Ok(())
            }
        }
        let f = Arc::new(FlushCounter(AtomicU64::new(0)));
        let chain = SinkChain::new().with_sink(f.clone());
        chain.flush();
        assert_eq!(f.0.load(Ordering::Acquire), 1);
    }

    #[test]
    fn sink_chain_sinks_accessor() {
        let s = Arc::new(CountingSink::new());
        let chain = SinkChain::new().with_sink(s);
        assert_eq!(chain.sinks().len(), 1);
        assert_eq!(chain.sinks()[0].name(), "counting");
    }

    // § RingSink integration.

    #[test]
    fn ring_sink_writes_into_ring() {
        let sink = RingSink::with_capacity(16);
        let r = fresh_record();
        sink.write(&r).expect("ring not full");
        assert_eq!(sink.pending_len(), 1);
    }

    #[test]
    fn ring_sink_lossy_on_overflow() {
        let sink = RingSink::with_capacity(2);
        let r = fresh_record();
        sink.write(&r).unwrap();
        sink.write(&r).unwrap();
        // Third → overflow.
        let err = sink.write(&r).unwrap_err();
        assert!(matches!(err, SinkError::RingFull));
        assert_eq!(sink.overflow_count(), 1);
    }

    #[test]
    fn ring_sink_payload_decodes_binary_header() {
        let sink = RingSink::with_capacity(4);
        let r = fresh_record();
        sink.write(&r).unwrap();
        let slots = sink.drain_all();
        let slot = slots.first().unwrap();
        let (sev, sub, _fc, frame, _ln, _c) = crate::format::decode_binary_header(&slot.payload);
        assert_eq!(sev, Severity::Info.as_u8());
        assert_eq!(sub, SubsystemTag::Render.as_u8());
        assert_eq!(frame, 5);
    }

    #[test]
    fn ring_sink_uses_frame_n_as_timestamp() {
        let sink = RingSink::with_capacity(4);
        let mut r = fresh_record();
        r.frame_n = 12345;
        sink.write(&r).unwrap();
        let slots = sink.drain_all();
        let slot = slots.first().unwrap();
        // Spec § 7.2 : frame_n IS the timestamp (no wall-clock).
        assert_eq!(slot.timestamp_ns, 12345);
    }

    #[test]
    fn ring_sink_name_is_ring() {
        let sink = RingSink::with_capacity(4);
        assert_eq!(sink.name(), "ring");
    }

    #[test]
    fn ring_sink_drain_empties_ring() {
        let sink = RingSink::with_capacity(8);
        sink.write(&fresh_record()).unwrap();
        sink.write(&fresh_record()).unwrap();
        let drained = sink.drain_all();
        assert_eq!(drained.len(), 2);
        assert_eq!(sink.pending_len(), 0);
    }

    #[test]
    fn ring_sink_thread_safe_arc() {
        // Smoke-test : verify RingSink can be wrapped in Arc + shared.
        let sink = Arc::new(RingSink::with_capacity(16));
        let s2 = sink.clone();
        let h = std::thread::spawn(move || {
            for _ in 0..5 {
                s2.write(&fresh_record()).unwrap();
            }
        });
        h.join().unwrap();
        assert_eq!(sink.pending_len(), 5);
    }
}
