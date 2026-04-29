//! `StderrSink` : opt-in via `Cap<DevMode>` ; line-format CSL-glyph or JSON-line.
//!
//! § SPEC § 2.6 : ON for Info+Warn+Error+Fatal ; OFF for Trace+Debug by
//! default. Emits to `stderr` via [`std::io::Write`].
//!
//! § DETERMINISM : when `replay_strict=true`, this sink is DISABLED at
//! the engine-init layer (spec § 2.4). The sink itself is determinism-
//! agnostic ; replay-determinism is enforced by NOT registering the sink
//! in strict mode.
//!
//! § PRINCIPLE-NOT-HARDCODE : we accept a `dyn io::Write` so tests can
//! capture stderr-bytes deterministically. Production code passes
//! `std::io::stderr()`.

use std::io::Write;
use std::sync::Mutex;

use crate::format::Format;
use crate::severity::Severity;
use crate::sink::{LogRecord, LogSink, SinkError};

/// Stderr sink. Uses an internal mutex to serialize multi-thread writes
/// so log lines do not interleave.
pub struct StderrSink<W: Write + Send + Sync = std::io::Stderr> {
    out: Mutex<W>,
    fmt: Format,
    level_floor: Severity,
}

impl StderrSink<std::io::Stderr> {
    /// Build a stderr sink with the given format. Default level-floor is
    /// `Severity::Info` (matches spec § 2.6 routing matrix).
    #[must_use]
    pub fn new(fmt: Format) -> Self {
        Self {
            out: Mutex::new(std::io::stderr()),
            fmt,
            level_floor: Severity::Info,
        }
    }
}

impl<W: Write + Send + Sync> StderrSink<W> {
    /// Build a sink writing to a custom writer (for tests). Default
    /// level-floor is `Severity::Info`.
    #[must_use]
    pub fn with_writer(out: W, fmt: Format) -> Self {
        Self {
            out: Mutex::new(out),
            fmt,
            level_floor: Severity::Info,
        }
    }

    /// Override the level-floor : records below this severity are
    /// silently dropped. Default is `Severity::Info` per spec § 2.6.
    #[must_use]
    pub fn with_level_floor(mut self, floor: Severity) -> Self {
        self.level_floor = floor;
        self
    }
}

impl<W: Write + Send + Sync> LogSink for StderrSink<W> {
    fn write(&self, record: &LogRecord) -> Result<(), SinkError> {
        if record.severity < self.level_floor {
            return Ok(());
        }
        let line = record.encode_line(self.fmt);
        let mut guard = self.out.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .write_all(line.as_bytes())
            .map_err(|e| SinkError::Io(e.to_string()))
    }

    fn flush(&self) -> Result<(), SinkError> {
        let mut guard = self.out.lock().unwrap_or_else(|e| e.into_inner());
        guard.flush().map_err(|e| SinkError::Io(e.to_string()))
    }

    fn name(&self) -> &'static str {
        "stderr"
    }
}

#[cfg(test)]
mod tests {
    use super::StderrSink;
    use crate::field::FieldValue;
    use crate::format::Format;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::sink::{LogRecord, LogSink};
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;
    use std::sync::{Arc, Mutex};

    /// Test-writer that captures every byte written for assertion.
    #[derive(Default, Clone)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl CaptureWriter {
        fn captured(&self) -> Vec<u8> {
            self.0.lock().unwrap().clone()
        }
    }

    fn fresh_record(severity: Severity) -> LogRecord {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        LogRecord {
            frame_n: 7,
            severity,
            subsystem: SubsystemTag::Render,
            source: SourceLocation::new(PathHashField::from_path_hash(h), 1, 1),
            message: String::from("hello"),
            fields: vec![("k", FieldValue::I64(1))],
        }
    }

    #[test]
    fn stderr_sink_name_is_stderr() {
        let sink = StderrSink::new(Format::JsonLines);
        assert_eq!(sink.name(), "stderr");
    }

    #[test]
    fn stderr_sink_writes_json_line_to_writer() {
        let cap = CaptureWriter::default();
        let sink = StderrSink::with_writer(cap.clone(), Format::JsonLines);
        sink.write(&fresh_record(Severity::Info)).unwrap();
        let bytes = cap.captured();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.starts_with('{'));
        assert!(s.contains("\"frame\":7"));
    }

    #[test]
    fn stderr_sink_writes_csl_glyph_to_writer() {
        let cap = CaptureWriter::default();
        let sink = StderrSink::with_writer(cap.clone(), Format::CslGlyph);
        sink.write(&fresh_record(Severity::Info)).unwrap();
        let s = String::from_utf8(cap.captured()).unwrap();
        assert!(s.starts_with("frame=7"));
    }

    #[test]
    fn stderr_sink_default_floor_drops_trace_and_debug() {
        let cap = CaptureWriter::default();
        let sink = StderrSink::with_writer(cap.clone(), Format::JsonLines);
        sink.write(&fresh_record(Severity::Trace)).unwrap();
        sink.write(&fresh_record(Severity::Debug)).unwrap();
        // Nothing emitted.
        assert!(cap.captured().is_empty());
    }

    #[test]
    fn stderr_sink_default_floor_emits_info_through_fatal() {
        for s in [
            Severity::Info,
            Severity::Warning,
            Severity::Error,
            Severity::Fatal,
        ] {
            let cap = CaptureWriter::default();
            let sink = StderrSink::with_writer(cap.clone(), Format::JsonLines);
            sink.write(&fresh_record(s)).unwrap();
            assert!(!cap.captured().is_empty(), "expected emit for {s:?}");
        }
    }

    #[test]
    fn stderr_sink_with_level_floor_overrides() {
        let cap = CaptureWriter::default();
        let sink = StderrSink::with_writer(cap.clone(), Format::JsonLines)
            .with_level_floor(Severity::Warning);
        sink.write(&fresh_record(Severity::Info)).unwrap(); // dropped
        sink.write(&fresh_record(Severity::Warning)).unwrap(); // kept
        let s = String::from_utf8(cap.captured()).unwrap();
        let count = s.matches("\"frame\":7").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn stderr_sink_flush_works() {
        let cap = CaptureWriter::default();
        let sink = StderrSink::with_writer(cap.clone(), Format::JsonLines);
        sink.flush().unwrap();
    }

    #[test]
    fn stderr_sink_serializes_concurrent_writes() {
        use std::thread;
        let cap = CaptureWriter::default();
        let sink = Arc::new(StderrSink::with_writer(cap.clone(), Format::JsonLines));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let s = sink.clone();
                thread::spawn(move || {
                    let mut r = fresh_record(Severity::Info);
                    r.frame_n = i;
                    for _ in 0..100 {
                        s.write(&r).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let s = String::from_utf8(cap.captured()).unwrap();
        // 8 threads × 100 emits = 800 lines.
        assert_eq!(s.lines().count(), 800);
    }

    #[test]
    fn stderr_sink_emits_no_raw_path_chars_in_path_hash_form() {
        let cap = CaptureWriter::default();
        let sink = StderrSink::with_writer(cap.clone(), Format::JsonLines);
        sink.write(&fresh_record(Severity::Info)).unwrap();
        let s = String::from_utf8(cap.captured()).unwrap();
        // Source-loc file_hash uses 16hex+"...". No raw `/` from path.
        assert!(!s.contains("/test.rs"));
    }

    #[test]
    fn stderr_sink_constructor_accepts_format_variants() {
        let _a = StderrSink::new(Format::JsonLines);
        let _b = StderrSink::new(Format::CslGlyph);
        let _c = StderrSink::new(Format::Binary);
    }
}
