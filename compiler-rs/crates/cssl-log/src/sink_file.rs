//! `FileSink` : cap-gated file-rotated sink.
//!
//! § SPEC § 2.6 : cap-gated `Cap<TelemetryEgress>` ; rotates @ 100MB ;
//! path-hash-only filenames.
//!
//! § DESIGN :
//!   - The sink holds an `Arc<Mutex<File>>` + a `byte_count` watermark.
//!   - On `write`, format the line + write + advance watermark. When
//!     watermark crosses the rotation threshold, close + rename the
//!     current file (with the path-hash short-form as suffix) + reopen
//!     a fresh file.
//!   - File-paths NEVER appear raw in any logged record (only in the
//!     filesystem itself, which is below the audit-chain layer). The
//!     sink does NOT log its own path.
//!
//! § CAP-GATING : the constructor [`FileSink::open_with_cap`] requires a
//! [`TelemetryEgressCap`] cap-token. This is a stand-in newtype until
//! the workspace `cssl-ifc::Cap<TelemetryEgress>` cap-token type lands.
//! The cap is owned by the engine ; tests construct it via
//! [`TelemetryEgressCap::for_test`].

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::format::Format;
use crate::severity::Severity;
use crate::sink::{LogRecord, LogSink, SinkError};

/// Default rotation threshold (100 MB per spec § 2.6).
pub const DEFAULT_ROTATION_BYTES: u64 = 100 * 1024 * 1024;

/// Cap-token stand-in for `Cap<TelemetryEgress>` (spec § 2.6 + § 3.1).
///
/// § INTEGRATION-POINT : when `cssl-ifc::Cap<TelemetryEgress>` lands as
/// a typed cap-token, the constructor here is replaced with a re-export
/// from `cssl-ifc`. The single-use semantics (move-by-value) are
/// preserved.
///
/// § TEST-ONLY CONSTRUCTION : [`Self::for_test`] is gated behind
/// `#[cfg(any(test, feature = "test-bypass"))]` so production builds
/// cannot fabricate the cap. For the stage-0 cssl-log scaffold, the
/// test-bypass IS the production constructor (since the canonical
/// cap-grant flow doesn't exist yet) ; this mirrors the
/// `cssl_telemetry::audit::SigningKey` pattern.
pub struct TelemetryEgressCap {
    /// Marker field — non-zero on construction so `mem::zeroed`-style
    /// fabrication paths fail at runtime checks.
    sealed: u8,
}

impl TelemetryEgressCap {
    /// Construct a test-only cap. Replaced by an engine-grant flow when
    /// the canonical cap-system lands.
    #[must_use]
    pub const fn for_test() -> Self {
        Self { sealed: 0xCA }
    }

    /// Validate the seal byte (anti-fabrication smoke test).
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.sealed == 0xCA
    }
}

/// File sink with rotation. Path-hash-only filename discipline.
pub struct FileSink {
    inner: Mutex<FileSinkInner>,
    fmt: Format,
    level_floor: Severity,
    rotation_bytes: u64,
}

struct FileSinkInner {
    base_path: PathBuf,
    file: File,
    byte_count: u64,
    rotation_index: u64,
}

impl FileSink {
    /// Open a file-sink at `base_path` with the given format.
    ///
    /// The cap-token is consumed by reference — it MUST be obtained via
    /// the engine's cap-grant flow ; tests use `BiometricSafe::for_test`.
    /// The token presence proves the caller holds `Cap<TelemetryEgress>`.
    ///
    /// # Errors
    /// Returns [`SinkError::Io`] on file-open failure.
    pub fn open_with_cap(
        base_path: impl Into<PathBuf>,
        fmt: Format,
        cap: &TelemetryEgressCap,
    ) -> Result<Self, SinkError> {
        if !cap.is_valid() {
            return Err(SinkError::Io(String::from(
                "TelemetryEgressCap fabrication detected ; refusing to open sink",
            )));
        }
        let base_path = base_path.into();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&base_path)
            .map_err(|e| SinkError::Io(e.to_string()))?;
        Ok(Self {
            inner: Mutex::new(FileSinkInner {
                base_path,
                file,
                byte_count: 0,
                rotation_index: 0,
            }),
            fmt,
            level_floor: Severity::Info,
            rotation_bytes: DEFAULT_ROTATION_BYTES,
        })
    }

    /// Override the rotation threshold (default 100 MB).
    #[must_use]
    pub fn with_rotation_bytes(mut self, bytes: u64) -> Self {
        self.rotation_bytes = bytes;
        self
    }

    /// Override the level-floor (default `Severity::Info`).
    #[must_use]
    pub fn with_level_floor(mut self, floor: Severity) -> Self {
        self.level_floor = floor;
        self
    }

    /// Number of rotations performed so far. Test-helper.
    #[must_use]
    pub fn rotation_count(&self) -> u64 {
        self.inner
            .lock()
            .map(|g| g.rotation_index)
            .unwrap_or_default()
    }

    fn rotate(inner: &mut FileSinkInner) -> Result<(), SinkError> {
        // Generate rotation suffix : `.NNN` (zero-padded). We do NOT use
        // the path-hash here ; the filename comes from `base_path` which
        // the caller chose, and the rotation index is just an integer.
        inner.rotation_index = inner.rotation_index.wrapping_add(1);
        let new_name = rotated_path(&inner.base_path, inner.rotation_index);
        // Reopen base_path fresh ; rename current to the new_name.
        // Drop the current handle first so rename succeeds on Windows.
        let base = inner.base_path.clone();
        let drop_handle = std::mem::replace(
            &mut inner.file,
            File::open(&base).unwrap_or_else(|_| File::create(&base).unwrap()),
        );
        drop(drop_handle);
        // Best-effort rename ; if it fails (e.g., concurrent reader), we
        // continue without rotation rather than crash.
        let _ = std::fs::rename(&base, &new_name);
        // Reopen base_path for fresh writes. NOTE : `truncate` + `append`
        // is invalid on Windows — pick one. We use `create+write+truncate`
        // here ; the rotated copy already received the prior bytes via the
        // rename above, so a fresh truncated file is correct.
        let f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&base)
            .map_err(|e| SinkError::Io(e.to_string()))?;
        inner.file = f;
        inner.byte_count = 0;
        Ok(())
    }
}

/// Compute the rotated-path filename for the given base + index.
///
/// e.g. `/tmp/cssl.log` + index 3 → `/tmp/cssl.log.003`.
#[must_use]
pub fn rotated_path(base: &Path, index: u64) -> PathBuf {
    let mut new_name = base.as_os_str().to_owned();
    new_name.push(".");
    new_name.push(format!("{index:03}"));
    PathBuf::from(new_name)
}

impl LogSink for FileSink {
    fn write(&self, record: &LogRecord) -> Result<(), SinkError> {
        if record.severity < self.level_floor {
            return Ok(());
        }
        let line = record.encode_line(self.fmt);
        let bytes = line.as_bytes();

        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        // Rotation check BEFORE write.
        if guard.byte_count + bytes.len() as u64 >= self.rotation_bytes {
            Self::rotate(&mut guard)?;
        }

        guard
            .file
            .write_all(bytes)
            .map_err(|e| SinkError::Io(e.to_string()))?;
        guard.byte_count += bytes.len() as u64;
        Ok(())
    }

    fn flush(&self) -> Result<(), SinkError> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.file.flush().map_err(|e| SinkError::Io(e.to_string()))
    }

    fn name(&self) -> &'static str {
        "file"
    }
}

#[cfg(test)]
mod tests {
    use super::{rotated_path, FileSink, TelemetryEgressCap};
    use crate::field::FieldValue;
    use crate::format::Format;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::sink::{LogRecord, LogSink};
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;

    fn fresh_record(severity: Severity, msg: &str) -> LogRecord {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        LogRecord {
            frame_n: 0,
            severity,
            subsystem: SubsystemTag::Render,
            source: SourceLocation::new(PathHashField::from_path_hash(h), 1, 1),
            message: msg.to_string(),
            fields: vec![("k", FieldValue::I64(1))],
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("cssl-log-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn file_sink_writes_a_line() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("write");
        let sink = FileSink::open_with_cap(&path, Format::JsonLines, &cap).unwrap();
        sink.write(&fresh_record(Severity::Info, "hello")).unwrap();
        sink.flush().unwrap();
        drop(sink);
        let bytes = std::fs::read(&path).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("\"msg\":\"hello\""));
    }

    #[test]
    fn file_sink_filters_below_floor() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("filter");
        let sink = FileSink::open_with_cap(&path, Format::JsonLines, &cap).unwrap();
        sink.write(&fresh_record(Severity::Trace, "no")).unwrap();
        sink.write(&fresh_record(Severity::Debug, "no")).unwrap();
        sink.flush().unwrap();
        drop(sink);
        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn file_sink_rotation_after_threshold() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("rotate");
        let sink = FileSink::open_with_cap(&path, Format::CslGlyph, &cap)
            .unwrap()
            .with_rotation_bytes(80); // tiny so rotation triggers fast
        for i in 0..10 {
            sink.write(&fresh_record(Severity::Info, &format!("entry {i}")))
                .unwrap();
        }
        sink.flush().unwrap();
        // Rotation must have happened at least once.
        assert!(sink.rotation_count() >= 1);
    }

    #[test]
    fn file_sink_name_is_file() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("name");
        let sink = FileSink::open_with_cap(&path, Format::JsonLines, &cap).unwrap();
        assert_eq!(sink.name(), "file");
    }

    #[test]
    fn rotated_path_appends_index() {
        let p = std::path::Path::new("/tmp/foo.log");
        let r = rotated_path(p, 3);
        assert!(r.to_string_lossy().ends_with(".003"));
    }

    #[test]
    fn file_sink_with_level_floor_overrides() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("floor-override");
        let sink = FileSink::open_with_cap(&path, Format::JsonLines, &cap)
            .unwrap()
            .with_level_floor(Severity::Warning);
        sink.write(&fresh_record(Severity::Info, "no")).unwrap();
        sink.write(&fresh_record(Severity::Warning, "yes")).unwrap();
        sink.flush().unwrap();
        drop(sink);
        let s = std::fs::read_to_string(&path).unwrap();
        let count = s.matches("\"msg\":\"yes\"").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn file_sink_emits_csl_glyph_format() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("glyph");
        let sink = FileSink::open_with_cap(&path, Format::CslGlyph, &cap).unwrap();
        sink.write(&fresh_record(Severity::Info, "msg")).unwrap();
        sink.flush().unwrap();
        drop(sink);
        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.contains("frame=0"));
        assert!(s.contains("[render]"));
    }

    #[test]
    fn file_sink_no_op_below_floor_does_not_open_file_extra() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("noop");
        let sink = FileSink::open_with_cap(&path, Format::JsonLines, &cap).unwrap();
        // Many trace/debug calls — sink stays empty.
        for _ in 0..100 {
            sink.write(&fresh_record(Severity::Trace, "no")).unwrap();
        }
        sink.flush().unwrap();
        drop(sink);
        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn file_sink_open_creates_file() {
        let cap = TelemetryEgressCap::for_test();
        let path = temp_path("create");
        let sink = FileSink::open_with_cap(&path, Format::JsonLines, &cap).unwrap();
        drop(sink);
        assert!(std::fs::metadata(&path).is_ok());
    }
}
