//! § scoped-mark guard · RAII begin/end pair
//!
//! ```ignore
//! let mut interner = LabelInterner::default();
//! let frame = interner.intern("frame");
//! let ring = RtRing::new(1024);
//!
//! {
//!     let _scope = scoped_mark(&ring, frame);
//!     // … work …
//! } // ← Drop pushes MarkEnd with elapsed-micros in value_a
//! ```

use crate::event::{RtEvent, RtEventKind};
use crate::ring::RtRing;
use std::time::{SystemTime, UNIX_EPOCH};

/// § monotonic-ish micros since Unix epoch. Stage-0 uses `SystemTime` ;
/// stage-1 may swap for `Instant::now().duration_since(EPOCH_INSTANT)`
/// when we wire a process-start anchor.
pub fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

/// § RAII guard : pushes `MarkBegin` on construction, `MarkEnd` (with
/// elapsed-micros in `value_a`) on `Drop`.
pub struct ScopedMark<'a> {
    ring: &'a RtRing,
    label_idx: u16,
    start_ts: u64,
    /// § set to true by [`ScopedMark::end_explicit`] to suppress the
    /// drop-time push. Currently unused publicly ; reserved for future
    /// "manual end" + double-drop guard.
    ended: bool,
}

impl<'a> ScopedMark<'a> {
    /// § access the start-timestamp (micros) ← used by macros + drain analyses.
    #[must_use]
    pub fn start_ts(&self) -> u64 {
        self.start_ts
    }

    /// § interned label index for this scope.
    #[must_use]
    pub fn label_idx(&self) -> u16 {
        self.label_idx
    }
}

impl Drop for ScopedMark<'_> {
    fn drop(&mut self) {
        if self.ended {
            return;
        }
        let now = now_micros();
        let elapsed = now.saturating_sub(self.start_ts);
        self.ring.push(
            RtEvent::new(now, RtEventKind::MarkEnd, self.label_idx).with_a(elapsed),
        );
        self.ended = true;
    }
}

/// § create a scoped-mark : pushes `MarkBegin` immediately, returns a
/// guard that pushes `MarkEnd` with elapsed-micros on `Drop`.
#[must_use = "ScopedMark must be bound to a name so Drop fires at end-of-scope"]
pub fn scoped_mark(ring: &RtRing, label_idx: u16) -> ScopedMark<'_> {
    let start_ts = now_micros();
    ring.push(RtEvent::new(start_ts, RtEventKind::MarkBegin, label_idx));
    ScopedMark {
        ring,
        label_idx,
        start_ts,
        ended: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::RtEventKind;

    #[test]
    fn scope_pushes_begin_on_create() {
        let ring = RtRing::new(16);
        let scope = scoped_mark(&ring, 7);
        // After construction, exactly one event (MarkBegin) is in the ring.
        let snap = ring.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].kind, RtEventKind::MarkBegin);
        assert_eq!(snap[0].label_idx, 7);
        drop(scope);
    }

    #[test]
    fn scope_pushes_end_on_drop() {
        let ring = RtRing::new(16);
        {
            let _s = scoped_mark(&ring, 3);
            // One event so far — MarkBegin.
            assert_eq!(ring.snapshot().len(), 1);
        } // Drop fires here.
        let snap = ring.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].kind, RtEventKind::MarkBegin);
        assert_eq!(snap[1].kind, RtEventKind::MarkEnd);
        assert_eq!(snap[1].label_idx, 3);
    }

    #[test]
    fn scope_elapsed_stored_in_value_a() {
        let ring = RtRing::new(16);
        {
            let _s = scoped_mark(&ring, 1);
            // Sleep ~2ms so elapsed is observable but bounded.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let snap = ring.snapshot();
        assert_eq!(snap.len(), 2);
        let mark_end = snap[1];
        assert_eq!(mark_end.kind, RtEventKind::MarkEnd);
        // value_a holds elapsed-micros — should be ≥ 1500 (1.5ms) and < 1s.
        assert!(
            mark_end.value_a >= 1_500,
            "elapsed too small : {}",
            mark_end.value_a
        );
        assert!(
            mark_end.value_a < 1_000_000,
            "elapsed too large : {}",
            mark_end.value_a
        );
    }

    #[test]
    fn explicit_end_via_drop_not_double() {
        // § ScopedMark must push exactly ONE MarkEnd, even if Drop is called
        // explicitly via `drop(scope)` followed by going-out-of-scope (Rust
        // forbids the latter — the explicit drop moves it). Verify single-end
        // semantics via observing snapshot.
        let ring = RtRing::new(16);
        let scope = scoped_mark(&ring, 9);
        drop(scope);
        let snap = ring.snapshot();
        // Exactly 2 events : 1 begin, 1 end.
        assert_eq!(snap.len(), 2);
        let begins = snap.iter().filter(|e| e.kind == RtEventKind::MarkBegin).count();
        let ends = snap.iter().filter(|e| e.kind == RtEventKind::MarkEnd).count();
        assert_eq!(begins, 1);
        assert_eq!(ends, 1);
    }
}
