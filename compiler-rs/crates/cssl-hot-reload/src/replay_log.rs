//! Replay-log : append-only record of every applied swap event.
//!
//! § DESIGN
//!
//! Replay determinism is a hard contract :
//!
//! `replay(record(R)) == R   byte-equal`
//!
//! The hot-reload subsystem participates by recording every applied swap
//! into this log. Each `ReplayRecord` carries the LOGICAL frame index the
//! swap was applied on plus the `SwapKind` payload verbatim. The log is
//! keyed on the (`frame_id`, `sequence`) pair so two swaps applied in the
//! same frame are still totally ordered.
//!
//! § HARD RULES
//!
//! 1. NO wall-clock. `std::time::Instant` is forbidden in this module.
//!    Logical frames are the one-and-only ordinal.
//! 2. NO mutation. Once `record` has been called the entry is immutable
//!    (the public surface only exposes `&[ReplayRecord]` to readers).
//! 3. Insertion preserves frame-monotone order. A `record` call that
//!    attempts to insert a `frame_id` strictly less than the last
//!    recorded frame is rejected with `ReplayLogError::FrameRegression`.
//! 4. KAN no-op swaps (`is_noop()`) are NOT recorded — § 3.6 spec :
//!    "no-op hot-swap doesn't perturb the engine ; nothing changes,
//!    no replay event recorded".

use crate::event::{FrameId, SwapEvent, SwapKind};

/// One row in the replay log.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplayRecord {
    /// Logical frame the swap was applied on.
    pub frame_id: FrameId,
    /// Per-frame monotone sequence (matches the originating `SwapEvent`).
    pub sequence: u32,
    /// The swap-kind payload, preserved byte-for-byte from the originating
    /// event. Replay reconstructs the in-flight resource from
    /// `payload.fingerprint_post` (or `path_hash` for filesystem variants)
    /// against the replay-asset-store.
    pub payload: SwapKind,
}

impl ReplayRecord {
    /// Construct a record from a `SwapEvent` (used internally + by tests).
    #[must_use]
    pub fn from_event(event: &SwapEvent) -> Self {
        Self {
            frame_id: event.frame_id,
            sequence: event.sequence,
            payload: event.kind.clone(),
        }
    }

    /// Order key : (frame, sequence). Stable across multiple swaps in one frame.
    #[must_use]
    pub const fn order_key(&self) -> (FrameId, u32) {
        (self.frame_id, self.sequence)
    }
}

/// Errors the replay-log can surface.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayLogError {
    /// A `record` call attempted to insert a frame-id strictly less than the
    /// last recorded frame-id. Replay-determinism requires monotone frames.
    #[error("replay-log frame regression : last={last}, attempted={attempted}")]
    FrameRegression {
        /// Last successfully-recorded frame-id.
        last: FrameId,
        /// The (rejected) attempted frame-id.
        attempted: FrameId,
    },
    /// Sequence regression within the same frame (sequences must be monotone
    /// within a single `frame_id`).
    #[error("replay-log sequence regression in frame {frame}: last={last}, attempted={attempted}")]
    SequenceRegression {
        /// The frame the regression occurred on.
        frame: FrameId,
        /// Last successfully-recorded sequence in that frame.
        last: u32,
        /// The (rejected) attempted sequence.
        attempted: u32,
    },
}

/// Append-only replay log.
///
/// Stage-0 backs the log with a `Vec<ReplayRecord>` ; the surface returns
/// only `&[ReplayRecord]` to readers so callers can never mutate prior
/// entries. A real implementation will swap the backing store for a
/// memory-mapped append-only segment file — the surface is unchanged.
#[derive(Debug, Default, Clone)]
pub struct ReplayLog {
    records: Vec<ReplayRecord>,
}

impl ReplayLog {
    /// Construct an empty log.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Total recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Is the log empty ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Read-only view of all records (in insertion order = frame-monotone).
    #[must_use]
    pub fn records(&self) -> &[ReplayRecord] {
        &self.records
    }

    /// Last recorded frame-id (or `None` if empty).
    #[must_use]
    pub fn last_frame(&self) -> Option<FrameId> {
        self.records.last().map(|r| r.frame_id)
    }

    /// Last recorded record (or `None` if empty).
    #[must_use]
    pub fn last(&self) -> Option<&ReplayRecord> {
        self.records.last()
    }

    /// Record a swap event.
    ///
    /// Returns `Ok(false)` (NOT recorded) for KAN no-op swaps (§ 3.6 spec).
    /// Returns `Ok(true)` on successful record. Returns `Err` on frame or
    /// sequence regression.
    ///
    /// # Errors
    /// `FrameRegression` — `event.frame_id` is strictly less than the last
    /// recorded frame-id.
    /// `SequenceRegression` — `event.frame_id` matches the last recorded
    /// frame-id but `event.sequence` is not strictly greater than the last
    /// recorded sequence in that frame.
    pub fn record(&mut self, event: &SwapEvent) -> Result<bool, ReplayLogError> {
        if event.kind.is_noop() {
            return Ok(false);
        }
        if let Some(last) = self.records.last() {
            if event.frame_id < last.frame_id {
                return Err(ReplayLogError::FrameRegression {
                    last: last.frame_id,
                    attempted: event.frame_id,
                });
            }
            if event.frame_id == last.frame_id && event.sequence <= last.sequence {
                return Err(ReplayLogError::SequenceRegression {
                    frame: last.frame_id,
                    last: last.sequence,
                    attempted: event.sequence,
                });
            }
        }
        self.records.push(ReplayRecord::from_event(event));
        Ok(true)
    }

    /// Records filtered to a specific frame. Returns empty slice if no
    /// records match.
    #[must_use]
    pub fn records_in_frame(&self, frame_id: FrameId) -> Vec<&ReplayRecord> {
        self.records
            .iter()
            .filter(|r| r.frame_id == frame_id)
            .collect()
    }

    /// Records filtered to a swap-kind tag (`asset` / `shader` / `config` /
    /// `kan-weight`).
    #[must_use]
    pub fn records_by_tag(&self, tag: &str) -> Vec<&ReplayRecord> {
        self.records
            .iter()
            .filter(|r| r.payload.tag() == tag)
            .collect()
    }

    /// Drain all records. After `drain()` the log is empty + the
    /// `last_frame` watermark is reset to `None`. Use sparingly — usually
    /// the log is sealed-and-shipped.
    pub fn drain(&mut self) -> Vec<ReplayRecord> {
        std::mem::take(&mut self.records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AssetKind, ConfigKind, ShaderKind};

    fn h(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn asset_event(frame: FrameId, seq: u32, byte: u8) -> SwapEvent {
        SwapEvent::new(
            frame,
            seq,
            SwapKind::Asset {
                kind: AssetKind::Png,
                path_hash: h(byte),
                handle: u64::from(byte),
            },
        )
    }

    fn kan_noop(frame: FrameId, seq: u32, byte: u8) -> SwapEvent {
        SwapEvent::new(
            frame,
            seq,
            SwapKind::KanWeight {
                network_handle: 1,
                fingerprint_pre: h(byte),
                fingerprint_post: h(byte),
            },
        )
    }

    fn kan_real(frame: FrameId, seq: u32, pre: u8, post: u8) -> SwapEvent {
        SwapEvent::new(
            frame,
            seq,
            SwapKind::KanWeight {
                network_handle: 1,
                fingerprint_pre: h(pre),
                fingerprint_post: h(post),
            },
        )
    }

    #[test]
    fn empty_log_invariants() {
        let log = ReplayLog::new();
        assert_eq!(log.len(), 0);
        assert!(log.is_empty());
        assert!(log.records().is_empty());
        assert_eq!(log.last_frame(), None);
        assert_eq!(log.last(), None);
    }

    #[test]
    fn record_single_event() {
        let mut log = ReplayLog::new();
        let e = asset_event(1, 0, 7);
        let recorded = log.record(&e).unwrap();
        assert!(recorded);
        assert_eq!(log.len(), 1);
        assert_eq!(log.last_frame(), Some(1));
    }

    #[test]
    fn record_multiple_frames_monotone() {
        let mut log = ReplayLog::new();
        for f in 0..10 {
            assert!(log.record(&asset_event(f, 0, 0)).unwrap());
        }
        assert_eq!(log.len(), 10);
        assert_eq!(log.last_frame(), Some(9));
    }

    #[test]
    fn record_multiple_sequences_in_one_frame() {
        let mut log = ReplayLog::new();
        for s in 0..5_u32 {
            assert!(log.record(&asset_event(7, s, 0)).unwrap());
        }
        assert_eq!(log.len(), 5);
        assert_eq!(log.records_in_frame(7).len(), 5);
    }

    #[test]
    fn frame_regression_rejected() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(5, 0, 0)).unwrap();
        let err = log.record(&asset_event(3, 0, 0)).unwrap_err();
        assert!(matches!(
            err,
            ReplayLogError::FrameRegression {
                last: 5,
                attempted: 3
            }
        ));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn sequence_regression_within_same_frame_rejected() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(5, 3, 0)).unwrap();
        let err = log.record(&asset_event(5, 3, 0)).unwrap_err();
        assert!(matches!(err, ReplayLogError::SequenceRegression { .. }));
    }

    #[test]
    fn sequence_strictly_monotone_within_frame() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(5, 3, 0)).unwrap();
        let err = log.record(&asset_event(5, 2, 0)).unwrap_err();
        assert!(matches!(err, ReplayLogError::SequenceRegression { .. }));
    }

    #[test]
    fn sequence_resets_per_frame() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(1, 5, 0)).unwrap();
        // New frame — sequence resets to 0 (allowed because frame moved).
        log.record(&asset_event(2, 0, 0)).unwrap();
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn kan_noop_swap_not_recorded() {
        let mut log = ReplayLog::new();
        let recorded = log.record(&kan_noop(1, 0, 7)).unwrap();
        assert!(!recorded);
        assert!(log.is_empty());
    }

    #[test]
    fn kan_real_swap_is_recorded() {
        let mut log = ReplayLog::new();
        assert!(log.record(&kan_real(1, 0, 7, 8)).unwrap());
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn records_in_frame_filter() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(1, 0, 0)).unwrap();
        log.record(&asset_event(1, 1, 0)).unwrap();
        log.record(&asset_event(2, 0, 0)).unwrap();
        assert_eq!(log.records_in_frame(1).len(), 2);
        assert_eq!(log.records_in_frame(2).len(), 1);
        assert_eq!(log.records_in_frame(99).len(), 0);
    }

    #[test]
    fn records_by_tag_filter() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(1, 0, 0)).unwrap();
        log.record(&SwapEvent::new(
            2,
            0,
            SwapKind::Shader {
                kind: ShaderKind::SpirV,
                path_hash: h(0),
                pipeline: 0,
            },
        ))
        .unwrap();
        log.record(&SwapEvent::new(
            3,
            0,
            SwapKind::Config {
                kind: ConfigKind::Engine,
                path_hash: h(0),
                subsystem: 0,
            },
        ))
        .unwrap();
        log.record(&kan_real(4, 0, 1, 2)).unwrap();
        assert_eq!(log.records_by_tag("asset").len(), 1);
        assert_eq!(log.records_by_tag("shader").len(), 1);
        assert_eq!(log.records_by_tag("config").len(), 1);
        assert_eq!(log.records_by_tag("kan-weight").len(), 1);
    }

    #[test]
    fn drain_returns_records_and_empties_log() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(1, 0, 0)).unwrap();
        log.record(&asset_event(2, 0, 0)).unwrap();
        let drained = log.drain();
        assert_eq!(drained.len(), 2);
        assert!(log.is_empty());
        assert_eq!(log.last_frame(), None);
    }

    #[test]
    fn drain_then_record_starts_fresh() {
        let mut log = ReplayLog::new();
        log.record(&asset_event(5, 0, 0)).unwrap();
        log.drain();
        // Frame 1 is now allowed because the watermark reset to None.
        assert!(log.record(&asset_event(1, 0, 0)).is_ok());
    }

    #[test]
    fn replay_record_clone_eq() {
        let e = asset_event(1, 0, 7);
        let a = ReplayRecord::from_event(&e);
        let mut v = vec![a.clone()];
        v.push(a);
        assert_eq!(v[0], v[1]);
        assert_eq!(v[0].order_key(), (1, 0));
    }

    #[test]
    fn order_key_lex_within_records() {
        let e1 = asset_event(1, 0, 0);
        let e2 = asset_event(1, 1, 0);
        let r1 = ReplayRecord::from_event(&e1);
        let r2 = ReplayRecord::from_event(&e2);
        assert!(r1.order_key() < r2.order_key());
    }

    #[test]
    fn payload_preserved_byte_equal() {
        let path_hash = h(42);
        let event = SwapEvent::new(
            10,
            0,
            SwapKind::Asset {
                kind: AssetKind::Gltf,
                path_hash,
                handle: 1234,
            },
        );
        let mut log = ReplayLog::new();
        log.record(&event).unwrap();
        let recorded = log.records()[0].payload.clone();
        assert_eq!(recorded, event.kind);
    }
}
