//! § T11-WAVE3-REPLAY · `recorder.rs`
//!
//! Buffered append-only writer for replay events.  Each event is serialized as
//! a single JSON line followed by `\n` (JSONL — line-delimited JSON).
//!
//! § BUFFER POLICY
//!
//! `BufWriter` capacity is bounded to 64 KiB (`MAX_BUFFER_BYTES`) per the
//! "bounded-allocation" budget.  Larger buffers risk silent loss-on-crash
//! and exceed the spec's stdlib-heavy directive.
//!
//! § DROP-FLUSH
//!
//! `Drop` impl flushes-on-drop ; failures are silently ignored (per std::io
//! convention — `Drop` cannot return Result).  Callers wanting hard
//! durability should call `flush()` + check the return before drop.
//!
//! § STATS
//!
//! `RecorderStats` reports `count` (events appended successfully) +
//! `bytes_written` (cumulative · pre-flush) + `dropped` (events that failed
//! to serialize).  `dropped` is always 0 in current impl ; reserved for
//! future serialize-error tolerance.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::event::{ReplayEvent, ReplayEventKind};

/// Maximum BufWriter capacity in bytes.  Bounded-allocation budget.
pub const MAX_BUFFER_BYTES: usize = 64 * 1024;

/// Snapshot of recorder counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecorderStats {
    /// Number of events successfully appended (post-serialize, pre-flush-error).
    pub count: u64,
    /// Number of events dropped due to serialize failures.  Currently always 0.
    pub dropped: u64,
    /// Cumulative bytes written to the BufWriter (pre-flush).
    pub bytes_written: u64,
}

/// Append-only replay-event recorder.
///
/// Opens the target file with `append + create` (no truncate) so concurrent
/// recorders or restart-resume workflows append rather than clobber.  Each
/// `append()` call writes one JSON line and bumps `count` + `bytes_written`.
pub struct Recorder {
    path: PathBuf,
    file: BufWriter<File>,
    t0: Instant,
    count: u64,
    dropped: u64,
    bytes_written: u64,
}

impl Recorder {
    /// Open `path` for append-only replay recording.
    ///
    /// Creates the file if missing ; appends if present.  No truncation.
    /// `Instant::now()` is captured as the recorder epoch (`t0`) — every
    /// subsequent `append(kind)` derives `ts_micros` as the duration since
    /// `t0`.  Recorders started at different times therefore produce
    /// different absolute timestamps but identical *relative* timing.
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path_buf)?;
        Ok(Self {
            path: path_buf,
            file: BufWriter::with_capacity(MAX_BUFFER_BYTES, file),
            t0: Instant::now(),
            count: 0,
            dropped: 0,
            bytes_written: 0,
        })
    }

    /// Append a single event, deriving `ts_micros` from `Instant::now() - t0`.
    ///
    /// On success increments `count` + `bytes_written`.  On serialize failure
    /// returns `Err` ; the recorder remains usable (no partial-write state
    /// mutation since serde_json builds the full string before writing).
    pub fn append(&mut self, kind: ReplayEventKind) -> io::Result<()> {
        let ts_micros = u64::try_from(self.t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        let event = ReplayEvent::new(ts_micros, kind);
        let line = serde_json::to_string(&event)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // Single fused write : serialize-then-emit avoids interleaved partial lines.
        let bytes = line.as_bytes();
        self.file.write_all(bytes)?;
        self.file.write_all(b"\n")?;
        self.count += 1;
        self.bytes_written += bytes.len() as u64 + 1;
        Ok(())
    }

    /// Flush the BufWriter to the underlying file.
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }

    /// Snapshot the current counters.
    pub fn stats(&self) -> RecorderStats {
        RecorderStats {
            count: self.count,
            dropped: self.dropped,
            bytes_written: self.bytes_written,
        }
    }

    /// Path the recorder is writing to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // std::io convention : Drop cannot return ; best-effort flush.
        let _ = self.file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a unique temp-file path inside the system temp dir.
    /// Hand-rolled to avoid pulling `tempfile` as a dev-dep.
    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("cssl-host-replay-{tag}-{pid}-{nanos}.jsonl"));
        p
    }

    #[test]
    fn empty_recorder_creates_file() {
        let p = temp_path("empty");
        {
            let r = Recorder::new(&p).expect("open");
            assert_eq!(r.stats(), RecorderStats::default());
            assert_eq!(r.path(), p.as_path());
        }
        assert!(p.exists(), "recorder must create the file");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn single_event_appends_one_line() {
        let p = temp_path("single");
        {
            let mut r = Recorder::new(&p).expect("open");
            r.append(ReplayEventKind::KeyDown(65)).expect("append");
            r.flush().expect("flush");
            let s = r.stats();
            assert_eq!(s.count, 1);
            assert!(s.bytes_written > 0);
        }
        let body = std::fs::read_to_string(&p).expect("read");
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("KeyDown"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn multi_event_appends_n_lines() {
        let p = temp_path("multi");
        {
            let mut r = Recorder::new(&p).expect("open");
            r.append(ReplayEventKind::KeyDown(1)).expect("append");
            r.append(ReplayEventKind::KeyUp(1)).expect("append");
            r.append(ReplayEventKind::Tick { dt_ms: 16 })
                .expect("append");
            r.flush().expect("flush");
            assert_eq!(r.stats().count, 3);
        }
        let body = std::fs::read_to_string(&p).expect("read");
        assert_eq!(body.lines().count(), 3);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn append_survives_flush() {
        let p = temp_path("survive");
        {
            let mut r = Recorder::new(&p).expect("open");
            r.append(ReplayEventKind::KeyDown(1)).expect("append");
            r.flush().expect("flush");
            r.append(ReplayEventKind::KeyDown(2)).expect("append");
            r.flush().expect("flush");
            assert_eq!(r.stats().count, 2);
        }
        let body = std::fs::read_to_string(&p).expect("read");
        let n = body.lines().count();
        assert_eq!(n, 2, "both events must persist across flush");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn stats_track_counts() {
        let p = temp_path("stats");
        {
            let mut r = Recorder::new(&p).expect("open");
            assert_eq!(r.stats().count, 0);
            r.append(ReplayEventKind::MouseScroll(1.0)).expect("append");
            assert_eq!(r.stats().count, 1);
            r.append(ReplayEventKind::MouseClick {
                btn: 0,
                x: 10.0,
                y: 20.0,
            })
            .expect("append");
            assert_eq!(r.stats().count, 2);
            assert_eq!(r.stats().dropped, 0);
            assert!(r.stats().bytes_written > 0);
        }
        let _ = std::fs::remove_file(&p);
    }
}
