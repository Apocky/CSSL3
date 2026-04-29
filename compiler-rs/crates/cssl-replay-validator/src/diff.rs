//! Diff — bit-equal comparator for two [`ReplayLogSnapshot`]s.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.3 + AC-9 / AC-12.
//!
//! § DISCIPLINE
//!
//!   The diff result is **structured** — it tells you not just "they
//!   differ" but where exactly the divergence occurred. This makes the
//!   validator usable as a regression-test : when a change to
//!   cssl-metrics breaks the H5 contract, the diff pinpoints the first
//!   divergent byte (and the metric event around it).
//!
//! [`ReplayLogSnapshot`]: crate::ReplayLogSnapshot

use crate::metric_event::MetricEvent;
use crate::replay_log::ReplayLogSnapshot;
use crate::REPLAY_LOG_MAGIC;
use thiserror::Error;

/// Result of comparing two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryDiff {
    /// Snapshots are bit-equal — the H5 contract is preserved.
    BitEqual {
        event_count: usize,
        content_hash: [u8; 32],
    },
    /// Snapshots differ. The kind explains what kind of divergence.
    Diverged(HistoryDiffKind),
}

impl HistoryDiff {
    /// Whether this diff is bit-equal.
    #[must_use]
    pub const fn is_bit_equal(&self) -> bool {
        matches!(self, Self::BitEqual { .. })
    }

    /// Whether this diff diverged.
    #[must_use]
    pub const fn is_diverged(&self) -> bool {
        matches!(self, Self::Diverged(_))
    }

    /// Extract the `Diverged` kind, if any.
    #[must_use]
    pub fn divergence_kind(&self) -> Option<&HistoryDiffKind> {
        match self {
            Self::Diverged(k) => Some(k),
            Self::BitEqual { .. } => None,
        }
    }
}

/// Specific kind of divergence between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryDiffKind {
    /// Different number of events recorded.
    EventCountDiffers { left: usize, right: usize },
    /// Same event count, but byte-stream differs at offset.
    ByteStreamDiverged {
        offset: usize,
        left_byte: u8,
        right_byte: u8,
    },
    /// One snapshot has a malformed magic header.
    BadMagic { which: DiffSide },
    /// The encoded event-count in the byte-stream doesn't match the
    /// snapshot's reported event-count.
    EventCountFieldMismatch {
        which: DiffSide,
        encoded: u64,
        reported: usize,
    },
    /// Specific event diverged ; surfaces the first-divergent event-index
    /// + the canonical bytes of both events.
    EventDiverged {
        event_index: usize,
        left_event: Box<MetricEvent>,
        right_event: Box<MetricEvent>,
    },
}

/// Which side of a diff a finding refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffSide {
    Left,
    Right,
}

/// Errors from [`diff_snapshots`] processing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HistoryDiffError {
    /// Both snapshots have malformed magic headers.
    #[error("PD0164 — both snapshots have malformed magic headers")]
    BothMalformed,
    /// Snapshot byte-buffer is too short to be valid.
    #[error("PD0165 — snapshot byte-buffer too short ; len={len} required>={required}")]
    BufferTooShort { len: usize, required: usize },
}

/// Diff two replay-log snapshots. Returns a structured [`HistoryDiff`].
///
/// § SPEC : § VI.3 + AC-12 ("metric-history bit-equal across replay runs").
pub fn diff_snapshots(
    left: &ReplayLogSnapshot,
    right: &ReplayLogSnapshot,
) -> Result<HistoryDiff, HistoryDiffError> {
    // Validate buffer-lengths : magic(8) + count(8) + 0 events + hash(32) = 48
    let min_len = REPLAY_LOG_MAGIC.len() + 8 + 32;
    let l_bytes = left.as_bytes();
    let r_bytes = right.as_bytes();
    if l_bytes.len() < min_len {
        return Err(HistoryDiffError::BufferTooShort {
            len: l_bytes.len(),
            required: min_len,
        });
    }
    if r_bytes.len() < min_len {
        return Err(HistoryDiffError::BufferTooShort {
            len: r_bytes.len(),
            required: min_len,
        });
    }
    // Validate magic.
    let l_magic_ok = &l_bytes[0..8] == REPLAY_LOG_MAGIC;
    let r_magic_ok = &r_bytes[0..8] == REPLAY_LOG_MAGIC;
    match (l_magic_ok, r_magic_ok) {
        (true, true) => {}
        (false, true) => {
            return Ok(HistoryDiff::Diverged(HistoryDiffKind::BadMagic {
                which: DiffSide::Left,
            }))
        }
        (true, false) => {
            return Ok(HistoryDiff::Diverged(HistoryDiffKind::BadMagic {
                which: DiffSide::Right,
            }))
        }
        (false, false) => return Err(HistoryDiffError::BothMalformed),
    }
    // Validate event-count fields.
    let l_count = u64::from_le_bytes(l_bytes[8..16].try_into().unwrap_or([0u8; 8]));
    let r_count = u64::from_le_bytes(r_bytes[8..16].try_into().unwrap_or([0u8; 8]));
    if l_count as usize != left.event_count() {
        return Ok(HistoryDiff::Diverged(
            HistoryDiffKind::EventCountFieldMismatch {
                which: DiffSide::Left,
                encoded: l_count,
                reported: left.event_count(),
            },
        ));
    }
    if r_count as usize != right.event_count() {
        return Ok(HistoryDiff::Diverged(
            HistoryDiffKind::EventCountFieldMismatch {
                which: DiffSide::Right,
                encoded: r_count,
                reported: right.event_count(),
            },
        ));
    }
    if left.event_count() != right.event_count() {
        return Ok(HistoryDiff::Diverged(HistoryDiffKind::EventCountDiffers {
            left: left.event_count(),
            right: right.event_count(),
        }));
    }
    // Now byte-equal compare. If they match exactly, declare BitEqual.
    if l_bytes == r_bytes {
        return Ok(HistoryDiff::BitEqual {
            event_count: left.event_count(),
            content_hash: left.content_hash(),
        });
    }
    // Find first divergent byte.
    let common_len = l_bytes.len().min(r_bytes.len());
    let mut diverge_offset = common_len;
    for i in 0..common_len {
        if l_bytes[i] != r_bytes[i] {
            diverge_offset = i;
            break;
        }
    }
    // Determine which event-slot the divergence falls into.
    let header_len = REPLAY_LOG_MAGIC.len() + 8;
    if diverge_offset >= header_len {
        let body_offset = diverge_offset - header_len;
        // Trailer hash starts at header_len + count*32 ; if past that, the
        // divergence is in the trailer (i.e. content-hash differs because
        // body differs upstream — but this branch handles same-len mismatches
        // that didn't trigger byte-stream-equality above).
        let body_len = MetricEvent::BYTE_LEN * left.event_count();
        if body_offset < body_len {
            let event_index = body_offset / MetricEvent::BYTE_LEN;
            // Decode the events at this index from both buffers.
            let ev_offset = header_len + event_index * MetricEvent::BYTE_LEN;
            let l_ev_bytes: [u8; MetricEvent::BYTE_LEN] = l_bytes
                [ev_offset..ev_offset + MetricEvent::BYTE_LEN]
                .try_into()
                .unwrap_or([0u8; MetricEvent::BYTE_LEN]);
            let r_ev_bytes: [u8; MetricEvent::BYTE_LEN] = r_bytes
                [ev_offset..ev_offset + MetricEvent::BYTE_LEN]
                .try_into()
                .unwrap_or([0u8; MetricEvent::BYTE_LEN]);
            // If decode fails (invalid kind), fall back to byte-stream divergence.
            if let (Some(le), Some(re)) = (
                MetricEvent::from_canonical_bytes(&l_ev_bytes),
                MetricEvent::from_canonical_bytes(&r_ev_bytes),
            ) {
                return Ok(HistoryDiff::Diverged(HistoryDiffKind::EventDiverged {
                    event_index,
                    left_event: Box::new(le),
                    right_event: Box::new(re),
                }));
            }
        }
    }
    // Fall back to ByteStreamDiverged.
    Ok(HistoryDiff::Diverged(HistoryDiffKind::ByteStreamDiverged {
        offset: diverge_offset,
        left_byte: l_bytes[diverge_offset],
        right_byte: r_bytes[diverge_offset],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric_event::{MetricEvent, MetricEventKind, MetricValue};
    use crate::replay_log::ReplayLog;

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
    fn t_diff_empty_logs_bit_equal() {
        let a = ReplayLog::new().snapshot();
        let b = ReplayLog::new().snapshot();
        let d = diff_snapshots(&a, &b).unwrap();
        assert!(d.is_bit_equal());
    }

    #[test]
    fn t_diff_same_log_bit_equal() {
        let mut log_a = ReplayLog::new();
        let mut log_b = ReplayLog::new();
        for i in 0..5 {
            log_a
                .append(ev(i, MetricEventKind::CounterIncBy, i, 0))
                .unwrap();
            log_b
                .append(ev(i, MetricEventKind::CounterIncBy, i, 0))
                .unwrap();
        }
        let a = log_a.snapshot();
        let b = log_b.snapshot();
        assert!(diff_snapshots(&a, &b).unwrap().is_bit_equal());
    }

    #[test]
    fn t_diff_different_count_diverged() {
        let mut log_a = ReplayLog::new();
        let mut log_b = ReplayLog::new();
        log_a
            .append(ev(0, MetricEventKind::CounterIncBy, 1, 0))
            .unwrap();
        log_a
            .append(ev(1, MetricEventKind::CounterIncBy, 2, 0))
            .unwrap();
        log_b
            .append(ev(0, MetricEventKind::CounterIncBy, 1, 0))
            .unwrap();
        let a = log_a.snapshot();
        let b = log_b.snapshot();
        let d = diff_snapshots(&a, &b).unwrap();
        match d {
            HistoryDiff::Diverged(HistoryDiffKind::EventCountDiffers { left, right }) => {
                assert_eq!(left, 2);
                assert_eq!(right, 1);
            }
            _ => panic!("expected EventCountDiffers, got {d:?}"),
        }
    }

    #[test]
    fn t_diff_value_change_event_diverged() {
        let mut log_a = ReplayLog::new();
        let mut log_b = ReplayLog::new();
        log_a
            .append(ev(0, MetricEventKind::CounterIncBy, 5, 0))
            .unwrap();
        log_b
            .append(ev(0, MetricEventKind::CounterIncBy, 6, 0))
            .unwrap();
        let a = log_a.snapshot();
        let b = log_b.snapshot();
        let d = diff_snapshots(&a, &b).unwrap();
        match d {
            HistoryDiff::Diverged(HistoryDiffKind::EventDiverged {
                event_index,
                left_event,
                right_event,
            }) => {
                assert_eq!(event_index, 0);
                assert_eq!(left_event.value.as_u64(), 5);
                assert_eq!(right_event.value.as_u64(), 6);
            }
            _ => panic!("expected EventDiverged, got {d:?}"),
        }
    }

    #[test]
    fn t_diff_kind_change_event_diverged() {
        let mut log_a = ReplayLog::new();
        let mut log_b = ReplayLog::new();
        log_a
            .append(ev(0, MetricEventKind::CounterIncBy, 5, 0))
            .unwrap();
        log_b
            .append(ev(0, MetricEventKind::GaugeSet, 5, 0))
            .unwrap();
        let a = log_a.snapshot();
        let b = log_b.snapshot();
        let d = diff_snapshots(&a, &b).unwrap();
        assert!(matches!(
            d,
            HistoryDiff::Diverged(HistoryDiffKind::EventDiverged { .. })
        ));
    }

    #[test]
    fn t_diff_diverged_at_second_event() {
        let mut log_a = ReplayLog::new();
        let mut log_b = ReplayLog::new();
        log_a
            .append(ev(0, MetricEventKind::CounterIncBy, 1, 0))
            .unwrap();
        log_a
            .append(ev(1, MetricEventKind::CounterIncBy, 100, 0))
            .unwrap();
        log_b
            .append(ev(0, MetricEventKind::CounterIncBy, 1, 0))
            .unwrap();
        log_b
            .append(ev(1, MetricEventKind::CounterIncBy, 200, 0))
            .unwrap();
        let a = log_a.snapshot();
        let b = log_b.snapshot();
        match diff_snapshots(&a, &b).unwrap() {
            HistoryDiff::Diverged(HistoryDiffKind::EventDiverged { event_index, .. }) => {
                assert_eq!(event_index, 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn t_diff_extract_kind_helper() {
        let a = ReplayLog::new().snapshot();
        let b = ReplayLog::new().snapshot();
        let d = diff_snapshots(&a, &b).unwrap();
        assert!(d.divergence_kind().is_none());
    }

    #[test]
    fn t_diff_is_diverged_true_for_count_mismatch() {
        let mut log_a = ReplayLog::new();
        let log_b = ReplayLog::new();
        log_a
            .append(ev(0, MetricEventKind::CounterIncBy, 0, 0))
            .unwrap();
        let d = diff_snapshots(&log_a.snapshot(), &log_b.snapshot()).unwrap();
        assert!(d.is_diverged());
    }
}
