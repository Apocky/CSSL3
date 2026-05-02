//! § ring — `HeartbeatRing` in-memory producer queue (drained per-tick)
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   Producer-side ring buffer. Cell-tick / KAN-step / content-publish
//!   events push `FederationPattern` records here ; the heartbeat-service
//!   drains the ring once per tick (default 60s), assembles a
//!   `FederationBundle`, applies the Σ-mask gate, and broadcasts.
//!
//!   The ring is FIFO + bounded ; on overflow the OLDEST patterns are
//!   dropped (drop-oldest = freshness-priority for the federation).
//!
//! § DETERMINISM
//!   `drain` is order-preserving FIFO ; for a given push-sequence and a
//!   fixed capacity, the drained vector is bitwise-deterministic.
//!
//! § SOVEREIGN BOUNDARY
//!   The ring NEVER leaves the local machine. Patterns crossed no boundary
//!   while sitting here ; the Σ-mask gate fires at drain-time (in the
//!   heartbeat-service's tick), not at push-time.

use crate::pattern::FederationPattern;
use std::collections::VecDeque;
use std::sync::Mutex;

/// § `DEFAULT_RING_CAPACITY` — tuned for ~5min of buffered events at 1Hz.
pub const DEFAULT_RING_CAPACITY: usize = 256;

/// § `HeartbeatRing` — bounded FIFO of `FederationPattern` records.
///
/// `Mutex<VecDeque>` chosen over a lock-free SPSC for stage-0 simplicity ;
/// the producer count is small (one host-process) and the drain rate is
/// 1/min — contention is negligible.
pub struct HeartbeatRing {
    inner: Mutex<RingInner>,
    capacity: usize,
}

struct RingInner {
    buf: VecDeque<FederationPattern>,
    pushes_total: u64,
    drops_total: u64,
}

impl HeartbeatRing {
    /// § new — construct with `DEFAULT_RING_CAPACITY`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_RING_CAPACITY)
    }

    /// § with_capacity — explicit capacity (clamped to ≥ 1).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Mutex::new(RingInner {
                buf: VecDeque::with_capacity(capacity),
                pushes_total: 0,
                drops_total: 0,
            }),
            capacity,
        }
    }

    /// § push — enqueue one pattern. On overflow, drop the OLDEST pattern.
    /// Returns `true` if the push succeeded without dropping ; `false` if
    /// the oldest was evicted.
    pub fn push(&self, p: FederationPattern) -> bool {
        let mut inner = self.inner.lock().expect("ring lock");
        inner.pushes_total += 1;
        let evicted = if inner.buf.len() >= self.capacity {
            inner.buf.pop_front();
            inner.drops_total += 1;
            true
        } else {
            false
        };
        inner.buf.push_back(p);
        !evicted
    }

    /// § drain — remove + return ALL queued patterns. Order-preserving FIFO.
    pub fn drain(&self) -> Vec<FederationPattern> {
        let mut inner = self.inner.lock().expect("ring lock");
        inner.buf.drain(..).collect()
    }

    /// § len — current queue depth (informational).
    pub fn len(&self) -> usize {
        self.inner.lock().expect("ring lock").buf.len()
    }

    /// § is_empty — for clippy-compliance + ergonomic checks.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// § capacity — fixed buffer ceiling.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// § stats — (pushes_total, drops_total). Observability snapshot.
    pub fn stats(&self) -> (u64, u64) {
        let inner = self.inner.lock().expect("ring lock");
        (inner.pushes_total, inner.drops_total)
    }

    /// § purge_emitter — defense-in-depth at sovereign-revoke time. Removes
    /// every pattern whose `emitter_handle` matches. Returns the count of
    /// purged patterns.
    pub fn purge_emitter(&self, emitter_handle: u64) -> usize {
        let mut inner = self.inner.lock().expect("ring lock");
        let before = inner.buf.len();
        inner.buf.retain(|p| p.emitter_handle() != emitter_handle);
        before - inner.buf.len()
    }
}

impl Default for HeartbeatRing {
    fn default() -> Self {
        Self::new()
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{FederationKind, FederationPatternBuilder, CAP_FED_FLAGS_ALL};

    fn mk_pattern(seed: u8) -> FederationPattern {
        FederationPatternBuilder {
            kind: FederationKind::CellState,
            cap_flags: CAP_FED_FLAGS_ALL,
            k_anon_cohort_size: 1,
            confidence: 0.5,
            ts_unix: 60 * u64::from(seed),
            payload: vec![seed; 16],
            emitter_pubkey: [seed; 32],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn push_drain_round_trip() {
        let r = HeartbeatRing::with_capacity(8);
        for i in 0..5 {
            assert!(r.push(mk_pattern(i)));
        }
        assert_eq!(r.len(), 5);
        let drained = r.drain();
        assert_eq!(drained.len(), 5);
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn overflow_drops_oldest() {
        let r = HeartbeatRing::with_capacity(3);
        assert!(r.push(mk_pattern(1)));
        assert!(r.push(mk_pattern(2)));
        assert!(r.push(mk_pattern(3)));
        // Now full ; next push should evict the oldest.
        assert!(!r.push(mk_pattern(4)));
        let drained = r.drain();
        assert_eq!(drained.len(), 3);
        // Oldest (seed=1) was evicted ; remaining = 2, 3, 4.
        assert_eq!(drained[0].emitter_handle(), mk_pattern(2).emitter_handle());
        assert_eq!(drained[2].emitter_handle(), mk_pattern(4).emitter_handle());
    }

    #[test]
    fn drain_is_fifo_order() {
        let r = HeartbeatRing::with_capacity(8);
        for i in 1..=5 {
            r.push(mk_pattern(i));
        }
        let drained = r.drain();
        for (i, p) in drained.iter().enumerate() {
            assert_eq!(p.emitter_handle(), mk_pattern((i + 1) as u8).emitter_handle());
        }
    }

    #[test]
    fn stats_counts() {
        let r = HeartbeatRing::with_capacity(2);
        r.push(mk_pattern(1));
        r.push(mk_pattern(2));
        r.push(mk_pattern(3)); // evicts oldest
        let (pushes, drops) = r.stats();
        assert_eq!(pushes, 3);
        assert_eq!(drops, 1);
    }

    #[test]
    fn purge_emitter_drops_matching_patterns() {
        let r = HeartbeatRing::with_capacity(8);
        let p1 = mk_pattern(1);
        let p2 = mk_pattern(2);
        let handle = p1.emitter_handle();
        r.push(p1);
        r.push(p2);
        r.push(mk_pattern(1)); // duplicate emitter pubkey 1
        let purged = r.purge_emitter(handle);
        assert_eq!(purged, 2);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn default_is_empty() {
        let r = HeartbeatRing::new();
        assert!(r.is_empty());
        assert_eq!(r.capacity(), DEFAULT_RING_CAPACITY);
    }
}
