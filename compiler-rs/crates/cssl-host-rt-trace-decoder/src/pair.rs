//! § pair-marks · match `MarkBegin`/`MarkEnd` events into structured pairs
//!
//! ## Algorithm
//!
//! Walk events in arrival order. Maintain a stack of open `MarkBegin` records
//! keyed by `label_idx`. On encountering a `MarkEnd` :
//!
//! 1. Search the stack from top down for a matching `label_idx`.
//! 2. If found, pop it (and any frames above it that are unmatched ; those
//!    become entries in [`unmatched_begins`]). Emit a [`MarkPair`] with depth
//!    set to the stack-size *before* the pop.
//! 3. If no match, the `MarkEnd` is recorded by [`unmatched_ends`].
//!
//! ## Why stack-discipline ?
//!
//! Trace-events from a single thread of execution form properly nested
//! intervals (the `scoped_mark!` macro ties begin/end to RAII drop-order).
//! When the host emits cross-thread events into a shared ring, ordering
//! may be partially scrambled — the stack-based matcher gracefully degrades
//! to "best-effort innermost match" rather than failing hard. Outputs are
//! deterministic for a given input slice.

use cssl_host_rt_trace::{LabelInterner, RtEvent, RtEventKind};
use serde::{Deserialize, Serialize};

/// § a paired begin/end interval with computed duration + depth.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkPair {
    /// The interned label string (resolved via [`LabelInterner::get`]).
    pub label: String,
    /// Microsecond timestamp of the matched `MarkBegin`.
    pub start_ts: u64,
    /// Microsecond timestamp of the matched `MarkEnd`.
    pub end_ts: u64,
    /// Computed `end_ts − start_ts`. Saturates to 0 on out-of-order pairs.
    pub duration_us: u64,
    /// Stack-depth at the time the begin was pushed (root = 0).
    pub depth: u8,
}

/// § resolve a label-index against the interner, falling back to `<missing>`
/// for indices that do not appear in the snapshot.
fn resolve_label(idx: u16, interner: &LabelInterner) -> String {
    interner.get(idx).unwrap_or("<missing>").to_owned()
}

/// § pair `MarkBegin` / `MarkEnd` events using stack-discipline.
///
/// Returns one [`MarkPair`] per matched begin/end. Unmatched events are
/// available via [`unmatched_begins`] and [`unmatched_ends`].
#[must_use]
pub fn pair_marks(events: &[RtEvent], interner: &LabelInterner) -> Vec<MarkPair> {
    let mut stack: Vec<(u16, u64)> = Vec::new();
    let mut out: Vec<MarkPair> = Vec::new();

    for ev in events {
        match ev.kind {
            RtEventKind::MarkBegin => {
                stack.push((ev.label_idx, ev.ts_micros));
            }
            RtEventKind::MarkEnd => {
                // Search top-down for the matching label.
                let mut matched: Option<usize> = None;
                for (i, (lid, _)) in stack.iter().enumerate().rev() {
                    if *lid == ev.label_idx {
                        matched = Some(i);
                        break;
                    }
                }
                if let Some(i) = matched {
                    // Pop the matched frame ; record depth as the stack-size
                    // before the pop minus 1 (so a single root pair has depth 0).
                    let depth_before = stack.len();
                    let (lid, start_ts) = stack.remove(i);
                    // Anything above i was orphaned ; truncate it. (Their
                    // frames will be reported via unmatched_begins on a
                    // dedicated pass.) For now keep them on the stack so a
                    // later legitimate end can still match.
                    let _ = lid;
                    let depth_u8 = u8::try_from(depth_before.saturating_sub(1)).unwrap_or(u8::MAX);
                    let duration = ev.ts_micros.saturating_sub(start_ts);
                    out.push(MarkPair {
                        label: resolve_label(ev.label_idx, interner),
                        start_ts,
                        end_ts: ev.ts_micros,
                        duration_us: duration,
                        depth: depth_u8,
                    });
                }
                // If no match, drop silently — caller can use unmatched_ends.
            }
            // Counter / Histogram / Custom : not relevant to mark-pairing.
            _ => {}
        }
    }
    out
}

/// § return labels + timestamps of `MarkBegin` events that never paired.
///
/// Re-runs the same stack-discipline pass and reports leftover frames after
/// all events are consumed — these represent regions still "open" at drain
/// time (e.g. the render-loop is mid-frame when the drain happens) or
/// emitted across-thread without a matching end yet.
#[must_use]
pub fn unmatched_begins(events: &[RtEvent], interner: &LabelInterner) -> Vec<(String, u64)> {
    let mut stack: Vec<(u16, u64)> = Vec::new();
    for ev in events {
        match ev.kind {
            RtEventKind::MarkBegin => stack.push((ev.label_idx, ev.ts_micros)),
            RtEventKind::MarkEnd => {
                if let Some(i) = stack.iter().rposition(|(l, _)| *l == ev.label_idx) {
                    stack.remove(i);
                }
            }
            _ => {}
        }
    }
    stack
        .into_iter()
        .map(|(lid, ts)| (resolve_label(lid, interner), ts))
        .collect()
}

/// § return labels + timestamps of `MarkEnd` events that never paired.
#[must_use]
pub fn unmatched_ends(events: &[RtEvent], interner: &LabelInterner) -> Vec<(String, u64)> {
    let mut stack: Vec<(u16, u64)> = Vec::new();
    let mut orphans: Vec<(String, u64)> = Vec::new();
    for ev in events {
        match ev.kind {
            RtEventKind::MarkBegin => stack.push((ev.label_idx, ev.ts_micros)),
            RtEventKind::MarkEnd => {
                if let Some(i) = stack.iter().rposition(|(l, _)| *l == ev.label_idx) {
                    stack.remove(i);
                } else {
                    orphans.push((resolve_label(ev.label_idx, interner), ev.ts_micros));
                }
            }
            _ => {}
        }
    }
    orphans
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_rt_trace::RtEvent;

    fn begin(ts: u64, lid: u16) -> RtEvent {
        RtEvent::new(ts, RtEventKind::MarkBegin, lid)
    }
    fn end(ts: u64, lid: u16) -> RtEvent {
        RtEvent::new(ts, RtEventKind::MarkEnd, lid)
    }

    #[test]
    fn empty_input_yields_empty_pairs() {
        let interner = LabelInterner::default();
        let events: Vec<RtEvent> = Vec::new();
        assert!(pair_marks(&events, &interner).is_empty());
        assert!(unmatched_begins(&events, &interner).is_empty());
        assert!(unmatched_ends(&events, &interner).is_empty());
    }

    #[test]
    fn single_pair_matches_with_depth_zero() {
        let mut interner = LabelInterner::default();
        let frame = interner.intern("frame");
        let events = vec![begin(100, frame), end(250, frame)];
        let pairs = pair_marks(&events, &interner);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].label, "frame");
        assert_eq!(pairs[0].start_ts, 100);
        assert_eq!(pairs[0].end_ts, 250);
        assert_eq!(pairs[0].duration_us, 150);
        assert_eq!(pairs[0].depth, 0);
    }

    #[test]
    fn nested_pairs_have_increasing_depth() {
        let mut interner = LabelInterner::default();
        let outer = interner.intern("frame");
        let inner = interner.intern("draw");
        let events = vec![
            begin(0, outer),
            begin(10, inner),
            end(40, inner),
            end(100, outer),
        ];
        let pairs = pair_marks(&events, &interner);
        // Inner pair pops first → emitted first.
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].label, "draw");
        assert_eq!(pairs[0].depth, 1, "inner depth must be 1");
        assert_eq!(pairs[0].duration_us, 30);
        assert_eq!(pairs[1].label, "frame");
        assert_eq!(pairs[1].depth, 0, "outer depth must be 0");
        assert_eq!(pairs[1].duration_us, 100);
    }

    #[test]
    fn interleaved_pairs_match_innermost() {
        // a-begin, b-begin, a-end-skip-because-not-top, b-end, a-end.
        // Stack-discipline matches innermost-first : we have a, b on stack ;
        // an `a-end` sees stack=[a,b], rposition matches a (idx 0), pops a,
        // leaves [b] on stack. Then b-end matches b. Final stack = [].
        let mut interner = LabelInterner::default();
        let a = interner.intern("a");
        let b = interner.intern("b");
        let events = vec![begin(0, a), begin(10, b), end(20, a), end(30, b)];
        let pairs = pair_marks(&events, &interner);
        assert_eq!(pairs.len(), 2);
        // The first end matched 'a' (start=0,end=20).
        assert_eq!(pairs[0].label, "a");
        assert_eq!(pairs[0].duration_us, 20);
        assert_eq!(pairs[1].label, "b");
        assert_eq!(pairs[1].duration_us, 20);
    }

    #[test]
    fn unmatched_begin_reported() {
        let mut interner = LabelInterner::default();
        let frame = interner.intern("frame");
        // Begin without an end — region still open at drain-time.
        let events = vec![begin(500, frame)];
        let pairs = pair_marks(&events, &interner);
        assert!(pairs.is_empty(), "no end ⇒ no pair");
        let unmatched = unmatched_begins(&events, &interner);
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].0, "frame");
        assert_eq!(unmatched[0].1, 500);
    }

    #[test]
    fn unmatched_end_reported() {
        let mut interner = LabelInterner::default();
        let stray = interner.intern("stray-end");
        // End without a matching begin — cross-thread or pre-drain truncation.
        let events = vec![end(999, stray)];
        let pairs = pair_marks(&events, &interner);
        assert!(pairs.is_empty(), "no begin ⇒ no pair");
        let orphans = unmatched_ends(&events, &interner);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].0, "stray-end");
        assert_eq!(orphans[0].1, 999);
    }
}
