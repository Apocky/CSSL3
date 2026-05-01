//! § fixed-size ring of [`RtEvent`] · O(1) push · drop-oldest on overrun
//!
//! ## Concurrency model
//!
//! - **Writer** : single-producer logically (typically the render-thread).
//!   `AtomicU64::fetch_add(1, Relaxed)` on `write_idx` claims a slot.
//!   The cell is then written under a `Mutex<Vec<RtEvent>>` — stage-0
//!   correctness over stage-1 lock-free perf. The mutex is uncontended
//!   in the SPMC steady-state.
//! - **Readers** : multi-consumer. `snapshot` copies out the entire
//!   `[read_idx, write_idx)` range under the same mutex. `drain` advances
//!   `read_idx` to `write_idx` after the copy.
//! - **Overrun** : when `write_idx − read_idx > capacity`, the oldest
//!   `(write_idx − read_idx − capacity)` cells are unrecoverable
//!   (reader's window has moved past them). [`RtRing::dropped_count`]
//!   tracks the cumulative count.
//!
//! ## Why mutex + atomics, not pure atomics ?
//!
//! Cell-writes need exclusive access to a `Vec` slot. Pure-atomic ring
//! requires `UnsafeCell<MaybeUninit<RtEvent>>` + careful publication
//! ordering — that's stage-1 SHTRD lock-free territory + needs `unsafe`.
//! Stage-0 takes `parking_lot`-free `std::sync::Mutex` (uncontended) ;
//! `cargo bench` in W5b will validate the trade.

use crate::event::RtEvent;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// § fixed-size ring of trace-events.
///
/// Capacity must be a power-of-2 so index → slot is a single bitmask.
/// `new` panics on non-power-of-2 capacity (caller error · early).
pub struct RtRing {
    buf: Mutex<Vec<RtEvent>>,
    capacity: usize,
    mask: usize,
    write_idx: AtomicU64,
    read_idx: AtomicU64,
    dropped: AtomicU64,
}

impl RtRing {
    /// § create new ring with `capacity` cells. Must be power-of-2.
    ///
    /// Panics if `capacity` is 0 or not a power-of-2.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        assert!(
            capacity.is_power_of_two(),
            "capacity must be a power-of-2 for bitmask indexing (got {capacity})"
        );
        let buf = vec![RtEvent::default(); capacity];
        Self {
            buf: Mutex::new(buf),
            capacity,
            mask: capacity - 1,
            write_idx: AtomicU64::new(0),
            read_idx: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// § ring capacity (slot count).
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// § total number of events ever pushed (monotonic).
    pub fn write_count(&self) -> u64 {
        self.write_idx.load(Ordering::Acquire)
    }

    /// § events dropped due to ring-overrun (writer lapped reader).
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Acquire)
    }

    /// § O(1) atomic push of `ev`. If the writer has lapped the reader
    /// by more than `capacity`, the oldest cells are silently lost ; the
    /// drop-count is bumped accordingly.
    pub fn push(&self, ev: RtEvent) {
        // Atomically claim a slot.
        let widx = self.write_idx.fetch_add(1, Ordering::AcqRel);
        let slot = (widx as usize) & self.mask;

        // Write the cell. Mutex is uncontended in steady-state SPMC.
        // (`expect` here would only fire on a poisoned mutex, which we
        // don't enter from poisonable code paths in stage-0.)
        if let Ok(mut guard) = self.buf.lock() {
            guard[slot] = ev;
        }

        // Track overrun. If the writer is now > capacity ahead of
        // the reader, the oldest unread cells are gone forever.
        let ridx = self.read_idx.load(Ordering::Acquire);
        let in_flight = widx.saturating_sub(ridx) + 1;
        if in_flight > self.capacity as u64 {
            // We just lapped — at least one new entry was overwritten.
            self.dropped.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// § O(N) snapshot · returns a copy of the unread range
    /// `[read_idx, write_idx)`. Read-index is **not** advanced ;
    /// use [`RtRing::drain`] to also consume.
    pub fn snapshot(&self) -> Vec<RtEvent> {
        let widx = self.write_idx.load(Ordering::Acquire);
        let ridx = self.read_idx.load(Ordering::Acquire);
        if widx <= ridx {
            return Vec::new();
        }

        let in_flight = (widx - ridx).min(self.capacity as u64);
        // Advance r-pointer for our window if we're behind by > capacity.
        let effective_ridx = widx.saturating_sub(in_flight);

        let Ok(guard) = self.buf.lock() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(in_flight as usize);
        for k in 0..in_flight {
            let slot = ((effective_ridx + k) as usize) & self.mask;
            out.push(guard[slot]);
        }
        out
    }

    /// § snapshot + advance `read_idx` to `write_idx`. After drain, a
    /// subsequent snapshot returns the empty Vec until new pushes land.
    pub fn drain(&self) -> Vec<RtEvent> {
        let snap = self.snapshot();
        let widx = self.write_idx.load(Ordering::Acquire);
        self.read_idx.store(widx, Ordering::Release);
        snap
    }
}

// § Send + Sync : Mutex<Vec<RtEvent>> is Send+Sync ; AtomicU64 is Send+Sync.
// Compiler auto-derives — no explicit unsafe impl needed.

impl std::fmt::Debug for RtRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtRing")
            .field("capacity", &self.capacity)
            .field("write_idx", &self.write_count())
            .field("read_idx", &self.read_idx.load(Ordering::Acquire))
            .field("dropped", &self.dropped_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::RtEventKind;
    use std::sync::Arc;
    use std::thread;

    fn ev(ts: u64, label: u16) -> RtEvent {
        RtEvent::new(ts, RtEventKind::Counter, label).with_a(ts * 2)
    }

    #[test]
    fn new_empty() {
        let r = RtRing::new(16);
        assert_eq!(r.capacity(), 16);
        assert_eq!(r.write_count(), 0);
        assert_eq!(r.dropped_count(), 0);
        assert!(r.snapshot().is_empty());
    }

    #[test]
    fn push_snapshot() {
        let r = RtRing::new(16);
        r.push(ev(1, 0));
        r.push(ev(2, 1));
        r.push(ev(3, 2));
        let snap = r.snapshot();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].ts_micros, 1);
        assert_eq!(snap[2].ts_micros, 3);
        // Snapshot does NOT advance read-idx — calling again yields same.
        let snap2 = r.snapshot();
        assert_eq!(snap2.len(), 3);
    }

    #[test]
    fn drain_advances_read() {
        let r = RtRing::new(16);
        r.push(ev(10, 0));
        r.push(ev(20, 1));
        let d1 = r.drain();
        assert_eq!(d1.len(), 2);
        // After drain, snapshot is empty until new pushes.
        assert!(r.snapshot().is_empty());
        assert!(r.drain().is_empty());
        // New push reappears.
        r.push(ev(30, 2));
        let d2 = r.drain();
        assert_eq!(d2.len(), 1);
        assert_eq!(d2[0].ts_micros, 30);
    }

    #[test]
    fn ring_overrun_counts() {
        let r = RtRing::new(4);
        for k in 0..10u64 {
            r.push(ev(k, k as u16));
        }
        // 10 pushed into capacity-4 ring with no drains ⇒ ≥ 6 dropped.
        assert!(r.dropped_count() >= 6, "dropped={}", r.dropped_count());
        let snap = r.snapshot();
        assert_eq!(snap.len(), 4, "snapshot caps at capacity");
        // Tail wins : the last 4 events should be the most recent ones (6..10).
        assert_eq!(snap.last().unwrap().ts_micros, 9);
    }

    #[test]
    #[should_panic(expected = "power-of-2")]
    fn power_of_2_required() {
        let _ = RtRing::new(7);
    }

    #[test]
    fn capacity_respected() {
        let r = RtRing::new(8);
        for k in 0..100u64 {
            r.push(ev(k, 0));
        }
        let snap = r.snapshot();
        assert!(snap.len() <= 8, "snapshot must never exceed capacity");
    }

    #[test]
    fn multi_push_thread_safety() {
        // § basic sanity : 4 threads push 100 events each ; no panics + total
        // observed (snapshot + dropped) ≤ 400. (Concurrency-stress is wave-5b.)
        let r = Arc::new(RtRing::new(64));
        let mut handles = Vec::new();
        for t in 0..4 {
            let r = Arc::clone(&r);
            handles.push(thread::spawn(move || {
                for k in 0..100u64 {
                    r.push(ev(k, t as u16));
                }
            }));
        }
        for h in handles {
            h.join().expect("thread panic");
        }
        // 4 * 100 = 400 pushes total observed via write_idx.
        assert_eq!(r.write_count(), 400);
        // snapshot may show ≤ 64 ; dropped tracks the rest.
        let snap_len = r.snapshot().len() as u64;
        assert!(snap_len <= 64);
    }

    #[test]
    fn serde_roundtrip_of_snapshot() {
        let r = RtRing::new(16);
        for k in 0..5u64 {
            r.push(ev(k * 100, k as u16));
        }
        let snap = r.snapshot();
        let json = serde_json::to_string(&snap).expect("serialize");
        let back: Vec<RtEvent> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(snap, back);
    }
}
