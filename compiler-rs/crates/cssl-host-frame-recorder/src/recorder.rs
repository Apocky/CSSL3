//! § recorder.rs
//! ══════════════════════════════════════════════════════════════════
//! § FrameRecorder = bounded-ring frame accumulator. Push frames in
//! capture-order ; when the ring is full the oldest frame is dropped
//! and a `dropped` counter is incremented. `total_bytes` tracks the
//! lifetime payload byte-count (NOT the live bytes — useful for
//! dashboards / hotkey-feedback).
//!
//! § design notes
//!   • capacity == 0 is permitted ; every push immediately drops.
//!   • `started_at_ts` is set on the FIRST successful push ; it is
//!     NOT reset by drain — a recorder records a continuous timeline
//!     even if drained mid-stream. drain DOES reset live `total_bytes`
//!     so the next session's dashboard starts fresh per spec.
//!   • dropped counter is monotonic ; never reset by drain.

use crate::frame::Frame;

/// § bounded-ring frame accumulator with drop accounting.
#[derive(Debug, Clone)]
pub struct FrameRecorder {
    capacity: usize,
    frames: Vec<Frame>,
    dropped: u64,
    total_bytes: u64,
    started_at_ts: Option<u64>,
}

impl FrameRecorder {
    /// § new recorder with a fixed maximum number of in-flight frames.
    ///
    /// `capacity == 0` is permitted (acts as a pure drop-counter).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            frames: Vec::with_capacity(capacity.min(1024)),
            dropped: 0,
            total_bytes: 0,
            started_at_ts: None,
        }
    }

    /// § push a frame ; if the ring is full, the oldest frame is
    /// dropped and `dropped` is incremented. `total_bytes` accumulates
    /// the payload size of the just-pushed frame regardless of whether
    /// an eviction occurred.
    pub fn push(&mut self, frame: Frame) {
        if self.capacity == 0 {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        if self.started_at_ts.is_none() {
            self.started_at_ts = Some(frame.ts_micros);
        }
        if self.frames.len() >= self.capacity {
            // ring overrun — drop oldest
            let _ = self.frames.remove(0);
            self.dropped = self.dropped.saturating_add(1);
        }
        self.total_bytes = self
            .total_bytes
            .saturating_add(frame.rgba.len() as u64);
        self.frames.push(frame);
    }

    /// § immutable view over the current ring contents in capture-order.
    #[must_use]
    pub fn snapshot(&self) -> &[Frame] {
        &self.frames
    }

    /// § take ownership of the buffered frames + reset live byte-count
    /// so the next session's dashboard starts fresh. `dropped` and
    /// `started_at_ts` are NOT reset — the recorder's continuous
    /// timeline survives drain.
    pub fn drain(&mut self) -> Vec<Frame> {
        let out = std::mem::take(&mut self.frames);
        self.total_bytes = 0;
        out
    }

    /// § live frame count in the ring.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// § monotonic count of frames evicted by ring-overrun (lifetime).
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// § total live RGBA payload bytes since last drain (or recorder-init).
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// § configured ring capacity (frames).
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// § microseconds elapsed between the first-pushed frame's ts and
    /// the last live frame's ts. Returns 0 if fewer than 2 live frames.
    #[must_use]
    pub fn duration_micros(&self) -> u64 {
        let Some(start) = self.started_at_ts else {
            return 0;
        };
        let Some(last) = self.frames.last() else {
            return 0;
        };
        last.ts_micros.saturating_sub(start)
    }

    /// § timestamp of the first ever pushed frame (monotonic across drain).
    #[must_use]
    pub fn started_at(&self) -> Option<u64> {
        self.started_at_ts
    }

    /// § construct from a pre-existing Vec — used by the LFRC decoder.
    /// Caller is responsible for validating each frame.
    #[must_use]
    pub(crate) fn from_decoded(
        capacity: usize,
        frames: Vec<Frame>,
        started_at_ts: Option<u64>,
    ) -> Self {
        let total_bytes: u64 = frames.iter().map(|f| f.rgba.len() as u64).sum();
        Self {
            capacity,
            frames,
            dropped: 0,
            total_bytes,
            started_at_ts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, FrameKind};

    fn mk(w: u32, h: u32, ts: u64) -> Frame {
        let len = (w as usize) * (h as usize) * 4;
        Frame {
            width: w,
            height: h,
            ts_micros: ts,
            kind: FrameKind::KeyFrame,
            rgba: vec![0u8; len],
        }
    }

    #[test]
    fn new_recorder_is_empty() {
        let r = FrameRecorder::new(8);
        assert_eq!(r.frame_count(), 0);
        assert_eq!(r.dropped(), 0);
        assert_eq!(r.total_bytes(), 0);
        assert_eq!(r.capacity(), 8);
        assert_eq!(r.duration_micros(), 0);
        assert!(r.snapshot().is_empty());
        assert!(r.started_at().is_none());
    }

    #[test]
    fn push_fills_until_capacity() {
        let mut r = FrameRecorder::new(4);
        for i in 0..3u64 {
            r.push(mk(2, 2, i * 100));
        }
        assert_eq!(r.frame_count(), 3);
        assert_eq!(r.dropped(), 0);
        // 3 frames × 2*2*4 = 48 bytes
        assert_eq!(r.total_bytes(), 48);
    }

    #[test]
    fn ring_overruns_drop_oldest() {
        let mut r = FrameRecorder::new(2);
        r.push(mk(1, 1, 10));
        r.push(mk(1, 1, 20));
        r.push(mk(1, 1, 30));
        r.push(mk(1, 1, 40));
        assert_eq!(r.frame_count(), 2);
        assert_eq!(r.dropped(), 2);
        // oldest (ts=10,20) evicted ; ring should hold ts=30,40
        let snap = r.snapshot();
        assert_eq!(snap[0].ts_micros, 30);
        assert_eq!(snap[1].ts_micros, 40);
    }

    #[test]
    fn drain_empties_and_resets_bytes_but_keeps_dropped() {
        let mut r = FrameRecorder::new(2);
        r.push(mk(1, 1, 0));
        r.push(mk(1, 1, 1));
        r.push(mk(1, 1, 2)); // drops one
        assert_eq!(r.dropped(), 1);
        let drained = r.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(r.frame_count(), 0);
        assert_eq!(r.total_bytes(), 0);
        assert_eq!(r.dropped(), 1, "dropped counter is monotonic across drain");
        assert!(r.started_at().is_some(), "timeline survives drain");
    }

    #[test]
    fn duration_correct_across_pushes() {
        let mut r = FrameRecorder::new(4);
        r.push(mk(1, 1, 1_000));
        assert_eq!(r.duration_micros(), 0); // single frame = 0 duration
        r.push(mk(1, 1, 2_500));
        assert_eq!(r.duration_micros(), 1_500);
        r.push(mk(1, 1, 5_000));
        assert_eq!(r.duration_micros(), 4_000);
    }

    #[test]
    fn zero_capacity_drops_everything() {
        let mut r = FrameRecorder::new(0);
        r.push(mk(1, 1, 0));
        r.push(mk(1, 1, 1));
        r.push(mk(1, 1, 2));
        assert_eq!(r.frame_count(), 0);
        assert_eq!(r.dropped(), 3);
        assert_eq!(r.total_bytes(), 0);
        assert!(r.started_at().is_none());
    }

    #[test]
    fn snapshot_returns_capture_order() {
        let mut r = FrameRecorder::new(8);
        for i in 0..5u64 {
            r.push(mk(1, 1, i * 7));
        }
        let snap = r.snapshot();
        for (i, f) in snap.iter().enumerate() {
            assert_eq!(f.ts_micros, (i as u64) * 7);
        }
    }
}
