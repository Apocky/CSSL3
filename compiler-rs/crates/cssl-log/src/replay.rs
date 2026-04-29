//! Replay-strict mode flag (spec § 2.4 + § 7.2).
//!
//! § DETERMINISM CONTRACT :
//!   When `replay_strict=true` :
//!     - cssl-log macros emit-to-ring : NO-OP.
//!     - Audit-chain still receives entries (audit ¬ optional).
//!     - File-sink + MCP-sink ⟶ disabled (engine-init removes them
//!       from the chain when entering replay).
//!     - Ring is reserved for telemetry-events that are part of replay-
//!       input (frame-n, stage-id-counters, etc.).
//!
//!   When `replay_strict=false` (default) :
//!     - Full logging active ; lossy-ring acceptable ; N! determinism
//!       guarantee.
//!
//! § BIT-EQUALITY TEST : `replay_strict_log_determinism` (in
//! tests/replay_determinism.rs) validates byte-for-byte ring-state across
//! two replays.
//!
//! § OPTIONAL CAPTURE :
//!   Spec § 2.4 also permits an alternative : "captured into separate
//!   replay-log w/ N! reorder + N! drop". We implement this via
//!   [`set_replay_capture_buffer`] : when set, records are pushed into
//!   the buffer instead of the ring. Default = capture-buffer = `None` =
//!   strict-NO-OP.

use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::sink::LogRecord;

static REPLAY_STRICT: AtomicBool = AtomicBool::new(false);

/// Set the replay-strict flag. When `true`, [`crate::emit::emit_structured`]
/// becomes a no-op for ring/file/mcp sinks (audit-sink continues to work).
pub fn set_replay_strict(strict: bool) {
    REPLAY_STRICT.store(strict, Ordering::Release);
}

/// Read the replay-strict flag.
#[must_use]
pub fn is_replay_strict() -> bool {
    REPLAY_STRICT.load(Ordering::Acquire)
}

#[cfg(test)]
pub(crate) fn reset_replay_for_test() {
    REPLAY_STRICT.store(false, Ordering::Release);
    set_replay_capture_buffer(None);
}

// ───────────────────────────────────────────────────────────────────────
// § Optional replay capture-buffer
// ───────────────────────────────────────────────────────────────────────

/// Buffer that captures records during replay-strict mode (spec § 2.4
/// "captured into separate replay-log w/ N! reorder + N! drop").
pub struct ReplayCaptureBuffer {
    entries: Mutex<Vec<LogRecord>>,
}

impl ReplayCaptureBuffer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Append a record.
    pub fn append(&self, record: LogRecord) {
        let mut e = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        e.push(record);
    }

    /// Read-only snapshot of all captured records.
    pub fn snapshot(&self) -> Vec<LogRecord> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ReplayCaptureBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// Process-global pointer to the active capture-buffer (None = strict-NO-OP).
fn capture_buffer() -> &'static Mutex<Option<Arc<ReplayCaptureBuffer>>> {
    use std::sync::OnceLock;
    static SLOT: OnceLock<Mutex<Option<Arc<ReplayCaptureBuffer>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Install (or remove) the active replay capture-buffer.
pub fn set_replay_capture_buffer(buf: Option<Arc<ReplayCaptureBuffer>>) {
    let mut g = capture_buffer().lock().unwrap_or_else(|e| e.into_inner());
    *g = buf;
}

/// Retrieve the currently-installed replay capture-buffer (if any).
#[must_use]
pub fn replay_capture_buffer() -> Option<Arc<ReplayCaptureBuffer>> {
    capture_buffer()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::{
        is_replay_strict, replay_capture_buffer, reset_replay_for_test, set_replay_capture_buffer,
        set_replay_strict, ReplayCaptureBuffer,
    };
    use crate::field::FieldValue;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::sink::LogRecord;
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;
    use std::sync::{Arc, Mutex};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_replay_for_test();
        g
    }

    fn fresh_record() -> LogRecord {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        LogRecord {
            frame_n: 0,
            severity: Severity::Info,
            subsystem: SubsystemTag::Render,
            source: SourceLocation::new(PathHashField::from_path_hash(h), 1, 1),
            message: String::from("msg"),
            fields: vec![("k", FieldValue::I64(1))],
        }
    }

    #[test]
    fn replay_strict_default_false() {
        let _g = lock_and_reset();
        assert!(!is_replay_strict());
    }

    #[test]
    fn set_replay_strict_visible() {
        let _g = lock_and_reset();
        set_replay_strict(true);
        assert!(is_replay_strict());
        set_replay_strict(false);
        assert!(!is_replay_strict());
    }

    #[test]
    fn capture_buffer_default_none() {
        let _g = lock_and_reset();
        assert!(replay_capture_buffer().is_none());
    }

    #[test]
    fn capture_buffer_set_get() {
        let _g = lock_and_reset();
        let buf = Arc::new(ReplayCaptureBuffer::new());
        set_replay_capture_buffer(Some(buf.clone()));
        let got = replay_capture_buffer().unwrap();
        assert!(Arc::ptr_eq(&buf, &got));
    }

    #[test]
    fn capture_buffer_remove_via_none() {
        let _g = lock_and_reset();
        let buf = Arc::new(ReplayCaptureBuffer::new());
        set_replay_capture_buffer(Some(buf));
        set_replay_capture_buffer(None);
        assert!(replay_capture_buffer().is_none());
    }

    #[test]
    fn capture_buffer_append_visible_in_snapshot() {
        let buf = ReplayCaptureBuffer::new();
        buf.append(fresh_record());
        buf.append(fresh_record());
        assert_eq!(buf.len(), 2);
        let snap = buf.snapshot();
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn capture_buffer_default_empty() {
        let buf = ReplayCaptureBuffer::default();
        assert!(buf.is_empty());
    }

    #[test]
    fn capture_buffer_thread_safe() {
        use std::thread;
        let buf = Arc::new(ReplayCaptureBuffer::new());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let b = buf.clone();
                thread::spawn(move || {
                    for _ in 0..100 {
                        b.append(fresh_record());
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(buf.len(), 800);
    }

    #[test]
    fn capture_buffer_preserves_order_within_thread() {
        let buf = ReplayCaptureBuffer::new();
        for i in 0..10 {
            let mut r = fresh_record();
            r.frame_n = i;
            buf.append(r);
        }
        let snap = buf.snapshot();
        for (i, r) in snap.iter().enumerate() {
            assert_eq!(r.frame_n, i as u64);
        }
    }
}
