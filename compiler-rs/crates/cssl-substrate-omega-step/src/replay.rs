//! `ReplayLog` — append-only record of `(frame, input_event, rng_seed)` tuples
//! that lets `OmegaScheduler::replay_from(log)` reconstruct a bit-equal run.
//!
//! § THESIS
//!   Per `specs/30_SUBSTRATE.csl § DETERMINISTIC-REPLAY-INVARIANTS`, the
//!   scheduler's replay-mode reconstructs an Ω-tensor history by feeding
//!   the same input stream + the same RNG seed back to a fresh scheduler
//!   instance. This module supplies the log format.
//!
//! § FORMAT (stage-0)
//!   The log is a contiguous `Vec<ReplayEntry>` in frame-monotonic order.
//!   Each entry is tagged with its frame number + carries either an input
//!   event (struct mirror of `ctx::InputEvent`) or an RNG-stream-state
//!   checkpoint (state, inc, stream-id).
//!
//!   This is intentionally a stage-0 in-memory format. The S8-H5 slice
//!   integrates this with `cssl-persist` for save/load on disk.
//!
//! § BIT-EQUALITY CONTRACT
//!   For two scheduler runs to produce bit-identical Ω-tensor states :
//!     1. Master seed equal.
//!     2. The ordered sequence of `ReplayEntry::Input(_)` events is
//!        replayed in the same frame.
//!     3. RNG-stream checkpoints are NOT load-bearing for replay (they
//!        are derived state) but are stored for fast-forward recovery
//!        when the user wants to "rewind to frame N" without replaying
//!        from frame 0.
//!
//! § ABI
//!   Discriminants STABLE from S8-H2. New variants append-only. Renaming
//!   = major-version-bump per the T11-D76 ABI-stability invariant.

use crate::ctx::InputEvent;
use crate::rng::RngStreamId;

/// A single entry in the replay log.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplayEntry {
    /// An input event delivered at the given frame number. The scheduler
    /// re-injects this event during `replay_from(log)`.
    Input { frame: u64, event: InputEvent },
    /// An RNG-stream checkpoint. The scheduler can fast-forward by restoring
    /// the (state, inc) for `stream` at `frame` without rolling forward
    /// from frame 0. Stage-0 always records a checkpoint at frame 0 only.
    RngCheckpoint {
        frame: u64,
        stream: RngStreamId,
        state: u64,
        inc: u64,
    },
    /// A human-readable marker (e.g., "section: chapter-1"). Useful for
    /// editing replays. Not load-bearing for bit-equality.
    Marker { frame: u64, label: String },
}

impl ReplayEntry {
    /// The frame number at which this entry applies.
    #[must_use]
    pub fn frame(&self) -> u64 {
        match self {
            Self::Input { frame, .. }
            | Self::RngCheckpoint { frame, .. }
            | Self::Marker { frame, .. } => *frame,
        }
    }
}

/// Append-only ordered log of replay entries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplayLog {
    /// Stage-0 form : a flat Vec. Frames are monotone non-decreasing. Multiple
    /// entries per frame are permitted (e.g., multiple input events on the
    /// same tick).
    entries: Vec<ReplayEntry>,
    /// Master seed used by the scheduler whose run produced this log. The
    /// replayer MUST seed identically to reproduce bit-equal output.
    master_seed: u64,
}

impl ReplayLog {
    /// Construct a fresh log with the given master seed.
    #[must_use]
    pub fn new(master_seed: u64) -> Self {
        Self {
            entries: Vec::new(),
            master_seed,
        }
    }

    /// Master seed read.
    #[must_use]
    pub fn master_seed(&self) -> u64 {
        self.master_seed
    }

    /// Append an entry. Stage-0 form does not enforce monotone frame
    /// numbers ; the scheduler always appends in-order so the invariant
    /// holds by construction. A debug-mode check could add it.
    pub fn append(&mut self, entry: ReplayEntry) {
        self.entries.push(entry);
    }

    /// Read-only view of the entries.
    #[must_use]
    pub fn entries(&self) -> &[ReplayEntry] {
        &self.entries
    }

    /// Number of entries logged.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty (no entries — but `master_seed` may still
    /// be set).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All input events at a particular frame, in insertion order.
    #[must_use]
    pub fn inputs_at_frame(&self, frame: u64) -> Vec<&InputEvent> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                ReplayEntry::Input { frame: f, event } if *f == frame => Some(event),
                _ => None,
            })
            .collect()
    }

    /// Highest frame number recorded, or 0 for an empty log.
    #[must_use]
    pub fn max_frame(&self) -> u64 {
        self.entries
            .iter()
            .map(ReplayEntry::frame)
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_log_empty() {
        let log = ReplayLog::new(0);
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        assert_eq!(log.max_frame(), 0);
    }

    #[test]
    fn append_input_event() {
        let mut log = ReplayLog::new(42);
        log.append(ReplayEntry::Input {
            frame: 7,
            event: InputEvent::KeyPress { keycode: 65 },
        });
        assert_eq!(log.len(), 1);
        assert_eq!(log.max_frame(), 7);
        let inputs = log.inputs_at_frame(7);
        assert_eq!(inputs.len(), 1);
    }

    #[test]
    fn frame_extraction_works_for_all_variants() {
        let a = ReplayEntry::Input {
            frame: 3,
            event: InputEvent::Tick,
        };
        let b = ReplayEntry::RngCheckpoint {
            frame: 5,
            stream: RngStreamId(0),
            state: 0,
            inc: 1,
        };
        let c = ReplayEntry::Marker {
            frame: 11,
            label: "chapter-1".into(),
        };
        assert_eq!(a.frame(), 3);
        assert_eq!(b.frame(), 5);
        assert_eq!(c.frame(), 11);
    }

    #[test]
    fn inputs_at_frame_filters() {
        let mut log = ReplayLog::new(0);
        log.append(ReplayEntry::Input {
            frame: 0,
            event: InputEvent::KeyPress { keycode: 1 },
        });
        log.append(ReplayEntry::Marker {
            frame: 0,
            label: "begin".into(),
        });
        log.append(ReplayEntry::Input {
            frame: 0,
            event: InputEvent::KeyPress { keycode: 2 },
        });
        log.append(ReplayEntry::Input {
            frame: 1,
            event: InputEvent::KeyPress { keycode: 3 },
        });
        assert_eq!(log.inputs_at_frame(0).len(), 2);
        assert_eq!(log.inputs_at_frame(1).len(), 1);
        assert_eq!(log.inputs_at_frame(99).len(), 0);
    }

    #[test]
    fn max_frame_tracks_highest() {
        let mut log = ReplayLog::new(0);
        log.append(ReplayEntry::Marker {
            frame: 5,
            label: "a".into(),
        });
        log.append(ReplayEntry::Marker {
            frame: 12,
            label: "b".into(),
        });
        log.append(ReplayEntry::Marker {
            frame: 9,
            label: "c".into(),
        });
        assert_eq!(log.max_frame(), 12);
    }

    #[test]
    fn master_seed_round_trips() {
        let log = ReplayLog::new(0xCAFE_BABE);
        assert_eq!(log.master_seed(), 0xCAFE_BABE);
    }

    #[test]
    fn equality_compares_full_log() {
        let mut a = ReplayLog::new(1);
        let mut b = ReplayLog::new(1);
        a.append(ReplayEntry::Input {
            frame: 0,
            event: InputEvent::Tick,
        });
        b.append(ReplayEntry::Input {
            frame: 0,
            event: InputEvent::Tick,
        });
        assert_eq!(a, b);
    }
}
