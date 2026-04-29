//! Replay-log — append-only canonical metric-event sequence.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.2 + § VI.3.
//!
//! § DISCIPLINE
//!
//!   The replay-log is the **substitute for direct telemetry-ring-write**
//!   under `DeterminismMode::Strict`. Every metric event flows through
//!   the log instead of perturbing the real ring (which would be sensitive
//!   to wallclock / scheduler / other non-deterministic factors).
//!
//!   The log can be sealed into a [`ReplayLogSnapshot`] which is a
//!   canonical byte-sequence + a BLAKE3 content-hash. Two replay-runs of
//!   the same seed and same metric-op-stream produce **bit-equal**
//!   snapshots.
//!
//! § H5 INTEGRATION
//!
//!   The H5 contract states `omega_step` is bit-deterministic given
//!   `(seed, inputs)`. The replay-log extends this to metric-recording :
//!   given `(seed, inputs, metric-ops)`, the snapshot bytes are
//!   deterministic.

use crate::metric_event::{MetricEvent, MetricEventKind};
use crate::REPLAY_LOG_MAGIC;
use thiserror::Error;

/// Append-only replay-log. Held by the engine in `Strict` mode ; consulted
/// by the validator for bit-equal verification.
#[derive(Debug, Clone, Default)]
pub struct ReplayLog {
    events: Vec<MetricEvent>,
    /// Maximum number of events allowed before refusing further appends.
    /// Defaults to `usize::MAX` ; tests can lower this to verify the
    /// overflow-refusal path.
    capacity: Option<usize>,
}

impl ReplayLog {
    /// Construct an empty replay-log with unbounded capacity.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            events: Vec::new(),
            capacity: None,
        }
    }

    /// Construct an empty replay-log with the given hard cap.
    #[must_use]
    pub const fn with_capacity(cap: usize) -> Self {
        Self {
            events: Vec::new(),
            capacity: Some(cap),
        }
    }

    /// Number of events currently in the log.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Append a single event. `Err` if the log is at-capacity.
    pub fn append(&mut self, ev: MetricEvent) -> Result<(), ReplayLogError> {
        if let Some(cap) = self.capacity {
            if self.events.len() >= cap {
                return Err(ReplayLogError::CapacityExceeded { cap });
            }
        }
        self.events.push(ev);
        Ok(())
    }

    /// Append a batch of events. Stops at the first cap-violation and
    /// returns an `Err` ; events appended before the violation remain in
    /// the log.
    pub fn append_batch(&mut self, evs: &[MetricEvent]) -> Result<(), ReplayLogError> {
        for ev in evs {
            self.append(*ev)?;
        }
        Ok(())
    }

    /// Read a slice of events. Stable ordering — first-appended is first.
    #[must_use]
    pub fn events(&self) -> &[MetricEvent] {
        &self.events
    }

    /// Filter events by kind ; returns owned Vec (caller-side allocation).
    #[must_use]
    pub fn events_of_kind(&self, kind: MetricEventKind) -> Vec<MetricEvent> {
        self.events
            .iter()
            .copied()
            .filter(|e| e.kind == kind)
            .collect()
    }

    /// Seal the log into a canonical [`ReplayLogSnapshot`]. The snapshot
    /// contains : magic-bytes header + LE u64 event-count + (event-count ×
    /// 32-byte-canonical-event) + 32-byte BLAKE3 content-hash trailer.
    #[must_use]
    pub fn snapshot(&self) -> ReplayLogSnapshot {
        // Build canonical-bytes : magic (8) + count (8) + events*32 + hash (32)
        let event_count = self.events.len();
        let body_len = MetricEvent::BYTE_LEN * event_count;
        let total_len = REPLAY_LOG_MAGIC.len() + 8 + body_len + 32;
        let mut buf = Vec::with_capacity(total_len);
        buf.extend_from_slice(REPLAY_LOG_MAGIC);
        buf.extend_from_slice(&(event_count as u64).to_le_bytes());
        for ev in &self.events {
            buf.extend_from_slice(&ev.to_canonical_bytes());
        }
        // Compute BLAKE3 over (magic + count + events) and append.
        let hash = blake3::hash(&buf);
        buf.extend_from_slice(hash.as_bytes());
        ReplayLogSnapshot {
            bytes: buf,
            event_count,
        }
    }

    /// Clear all events. Used when restarting a replay-run.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

/// Sealed canonical byte-form of a replay-log.
///
/// § BYTE-LAYOUT
///
///   - 8 bytes  : `REPLAY_LOG_MAGIC` (`"CSSLZRL\x05"`)
///   - 8 bytes  : LE u64 event-count
///   - N × 32   : canonical metric-events
///   - 32 bytes : BLAKE3 content-hash of the preceding bytes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplayLogSnapshot {
    bytes: Vec<u8>,
    event_count: usize,
}

impl ReplayLogSnapshot {
    /// Number of events captured in this snapshot.
    #[must_use]
    pub const fn event_count(&self) -> usize {
        self.event_count
    }

    /// Read the full canonical byte-form.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Read just the BLAKE3 content-hash (32 bytes at the trailer).
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        let n = self.bytes.len();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&self.bytes[n - 32..]);
        hash
    }

    /// Bit-equal comparison with another snapshot.
    #[must_use]
    pub fn is_bit_equal_to(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }

    /// Compute a hex-string of the content-hash (canonical : 64 chars).
    #[must_use]
    pub fn content_hash_hex(&self) -> String {
        let h = self.content_hash();
        let mut s = String::with_capacity(64);
        for b in h {
            s.push(hex_nibble((b >> 4) & 0xF));
            s.push(hex_nibble(b & 0xF));
        }
        s
    }
}

const fn hex_nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => '?',
    }
}

/// Errors from replay-log operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayLogError {
    /// The replay-log is at-capacity ; further appends refused.
    #[error("PD0163 — replay-log at capacity ; cap={cap}")]
    CapacityExceeded { cap: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric_event::MetricValue;

    fn ev(frame: u64, kind: MetricEventKind, value: u64, tag: u64) -> MetricEvent {
        MetricEvent {
            frame_n: frame,
            sub_phase_index: 0,
            kind,
            metric_id: 1,
            value: MetricValue::from_u64(value),
            tag_hash: tag,
        }
    }

    #[test]
    fn t_empty_log_zero_events() {
        let log = ReplayLog::new();
        assert_eq!(log.len(), 0);
        assert!(log.is_empty());
    }

    #[test]
    fn t_append_then_len() {
        let mut log = ReplayLog::new();
        log.append(ev(0, MetricEventKind::CounterIncBy, 1, 0)).unwrap();
        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());
    }

    #[test]
    fn t_append_batch() {
        let mut log = ReplayLog::new();
        let batch = (0..5)
            .map(|f| ev(f, MetricEventKind::CounterIncBy, 1, 0))
            .collect::<Vec<_>>();
        log.append_batch(&batch).unwrap();
        assert_eq!(log.len(), 5);
    }

    #[test]
    fn t_capacity_refusal() {
        let mut log = ReplayLog::with_capacity(2);
        log.append(ev(0, MetricEventKind::CounterIncBy, 1, 0)).unwrap();
        log.append(ev(1, MetricEventKind::CounterIncBy, 1, 0)).unwrap();
        let r = log.append(ev(2, MetricEventKind::CounterIncBy, 1, 0));
        assert_eq!(r, Err(ReplayLogError::CapacityExceeded { cap: 2 }));
    }

    #[test]
    fn t_clear_zeroes_len() {
        let mut log = ReplayLog::new();
        log.append(ev(0, MetricEventKind::CounterSet, 7, 0)).unwrap();
        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn t_events_of_kind_filter() {
        let mut log = ReplayLog::new();
        log.append(ev(0, MetricEventKind::CounterIncBy, 1, 0)).unwrap();
        log.append(ev(1, MetricEventKind::GaugeSet, 2, 0)).unwrap();
        log.append(ev(2, MetricEventKind::CounterIncBy, 3, 0)).unwrap();
        let counters = log.events_of_kind(MetricEventKind::CounterIncBy);
        assert_eq!(counters.len(), 2);
    }

    #[test]
    fn t_snapshot_byte_len() {
        let mut log = ReplayLog::new();
        log.append(ev(0, MetricEventKind::CounterIncBy, 1, 0)).unwrap();
        let snap = log.snapshot();
        // 8 magic + 8 count + 32 event + 32 hash = 80 bytes
        assert_eq!(snap.as_bytes().len(), 80);
        assert_eq!(snap.event_count(), 1);
    }

    #[test]
    fn t_snapshot_magic_bytes_present() {
        let log = ReplayLog::new();
        let snap = log.snapshot();
        assert_eq!(&snap.as_bytes()[0..8], REPLAY_LOG_MAGIC);
    }

    #[test]
    fn t_snapshot_two_runs_same_input_bit_equal() {
        let mut log_a = ReplayLog::new();
        let mut log_b = ReplayLog::new();
        for i in 0..10 {
            log_a.append(ev(i, MetricEventKind::CounterIncBy, i, 0xAA)).unwrap();
            log_b.append(ev(i, MetricEventKind::CounterIncBy, i, 0xAA)).unwrap();
        }
        let a = log_a.snapshot();
        let b = log_b.snapshot();
        assert!(a.is_bit_equal_to(&b));
        assert_eq!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn t_snapshot_different_input_not_bit_equal() {
        let mut log_a = ReplayLog::new();
        let mut log_b = ReplayLog::new();
        log_a.append(ev(0, MetricEventKind::CounterIncBy, 1, 0)).unwrap();
        log_b.append(ev(0, MetricEventKind::CounterIncBy, 2, 0)).unwrap();
        let a = log_a.snapshot();
        let b = log_b.snapshot();
        assert!(!a.is_bit_equal_to(&b));
    }

    #[test]
    fn t_content_hash_hex_64_chars() {
        let log = ReplayLog::new();
        let snap = log.snapshot();
        let hex = snap.content_hash_hex();
        assert_eq!(hex.len(), 64);
        // Only lowercase hex chars.
        for c in hex.chars() {
            assert!(c.is_ascii_hexdigit() && (!c.is_ascii_uppercase()));
        }
    }

    #[test]
    fn t_empty_log_snapshot_stable() {
        let a = ReplayLog::new().snapshot();
        let b = ReplayLog::new().snapshot();
        assert!(a.is_bit_equal_to(&b));
    }

    #[test]
    fn t_snapshot_count_le_encoding() {
        let mut log = ReplayLog::new();
        for i in 0..3 {
            log.append(ev(i, MetricEventKind::CounterIncBy, 1, 0)).unwrap();
        }
        let snap = log.snapshot();
        let bytes = snap.as_bytes();
        // Count is at offset [8..16] in LE.
        let count = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        assert_eq!(count, 3);
    }

    #[test]
    fn t_event_order_preserved_in_snapshot() {
        let mut log = ReplayLog::new();
        log.append(ev(0, MetricEventKind::CounterIncBy, 0xA1, 0)).unwrap();
        log.append(ev(1, MetricEventKind::CounterIncBy, 0xB2, 0)).unwrap();
        let snap = log.snapshot();
        let bytes = snap.as_bytes();
        // First event-value at offset 16 (8 magic + 8 count + 16 inside-event).
        // Event byte 16..24 = value bits ; for event 0 that's at offset 8+8+16=32.
        let v0 = u64::from_le_bytes(bytes[32..40].try_into().unwrap());
        // Event 1 starts at offset 8+8+32=48 ; value bits at 48+16=64.
        let v1 = u64::from_le_bytes(bytes[64..72].try_into().unwrap());
        assert_eq!(v0, 0xA1);
        assert_eq!(v1, 0xB2);
    }
}
