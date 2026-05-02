//! § backpressure — bounded queue absorbing bundles when cloud is down
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   The mycelial network is best-effort. When the cloud endpoint is
//!   unreachable (network partition · 503 · scheduled-maintenance), the
//!   local heartbeat-service should not lose data. The `BackpressureQueue`
//!   is a bounded FIFO that absorbs bundles during outages ; on reconnect,
//!   the service drains FIFO until empty.
//!
//!   On overflow (the queue itself fills up), we drop the OLDEST bundle.
//!   This trades replay-completeness for liveness — fresh patterns are
//!   prioritized over stale ones during long outages. The dropped count
//!   is observable so operators can tune the queue capacity.

use crate::bundle::FederationBundle;
use std::collections::VecDeque;
use std::sync::Mutex;

/// § `DEFAULT_QUEUE_CAPACITY` — ~1 hour of buffered bundles at 1/min cadence.
pub const DEFAULT_QUEUE_CAPACITY: usize = 60;

/// § `BackpressureQueue` — bounded FIFO of compressed bundle blobs.
///
/// We store the COMPRESSED wire-blob (Vec<u8>) rather than the
/// `FederationBundle` struct so re-emission is a straight HTTP body-copy
/// without re-serializing.
pub struct BackpressureQueue {
    inner: Mutex<QueueInner>,
    capacity: usize,
}

struct QueueInner {
    buf: VecDeque<Vec<u8>>,
    enqueued_total: u64,
    drained_total: u64,
    drops_total: u64,
}

impl BackpressureQueue {
    /// § new — construct with `DEFAULT_QUEUE_CAPACITY`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY)
    }

    /// § with_capacity — explicit capacity (clamped to ≥ 1).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Mutex::new(QueueInner {
                buf: VecDeque::with_capacity(capacity),
                enqueued_total: 0,
                drained_total: 0,
                drops_total: 0,
            }),
            capacity,
        }
    }

    /// § enqueue — push a compressed bundle blob. On overflow, drop the
    /// OLDEST blob and increment the drops counter. Returns `true` if no
    /// drop occurred.
    pub fn enqueue(&self, blob: Vec<u8>) -> bool {
        let mut inner = self.inner.lock().expect("queue lock");
        inner.enqueued_total += 1;
        let dropped = if inner.buf.len() >= self.capacity {
            inner.buf.pop_front();
            inner.drops_total += 1;
            true
        } else {
            false
        };
        inner.buf.push_back(blob);
        !dropped
    }

    /// § drain_one — pop the OLDEST blob (FIFO) for re-emission. Returns
    /// `None` if the queue is empty.
    pub fn drain_one(&self) -> Option<Vec<u8>> {
        let mut inner = self.inner.lock().expect("queue lock");
        let blob = inner.buf.pop_front();
        if blob.is_some() {
            inner.drained_total += 1;
        }
        blob
    }

    /// § drain_all — drop everything ; used on sovereign-revoke or reset.
    pub fn drain_all(&self) -> usize {
        let mut inner = self.inner.lock().expect("queue lock");
        let n = inner.buf.len();
        inner.buf.clear();
        inner.drained_total += n as u64;
        n
    }

    /// § len — current depth.
    pub fn len(&self) -> usize {
        self.inner.lock().expect("queue lock").buf.len()
    }

    /// § is_empty — for clippy + ergonomic checks.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// § capacity — fixed buffer ceiling.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// § stats — (enqueued, drained, drops). Observability snapshot.
    pub fn stats(&self) -> (u64, u64, u64) {
        let inner = self.inner.lock().expect("queue lock");
        (inner.enqueued_total, inner.drained_total, inner.drops_total)
    }
}

impl Default for BackpressureQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// § `enqueue_bundle` — convenience helper : compress + enqueue in one call.
/// Returns the compressed blob size (for bandwidth observability).
pub fn enqueue_bundle(q: &BackpressureQueue, b: &FederationBundle) -> Result<usize, EnqueueError> {
    let json = serde_json::to_vec(b).map_err(|e| EnqueueError::Encode(e.to_string()))?;
    let compressed = crate::compress::compress_bundle(&json);
    let n = compressed.len();
    q.enqueue(compressed);
    Ok(n)
}

#[derive(Debug, thiserror::Error)]
pub enum EnqueueError {
    #[error("encode failed : {0}")]
    Encode(String),
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{FederationKind, FederationPatternBuilder, CAP_FED_FLAGS_ALL};

    fn mk_bundle(seed: u8) -> FederationBundle {
        let p = FederationPatternBuilder {
            kind: FederationKind::CellState,
            cap_flags: CAP_FED_FLAGS_ALL,
            k_anon_cohort_size: 12,
            confidence: 0.5,
            ts_unix: 60 * u64::from(seed),
            payload: vec![seed; 16],
            emitter_pubkey: [seed; 32],
        }
        .build()
        .unwrap();
        FederationBundle::build(seed as u64, 0, seed as u32, vec![p]).unwrap()
    }

    #[test]
    fn enqueue_drain_round_trip() {
        let q = BackpressureQueue::with_capacity(8);
        for i in 1..=4 {
            let b = mk_bundle(i);
            enqueue_bundle(&q, &b).unwrap();
        }
        assert_eq!(q.len(), 4);
        let mut drained = 0;
        while q.drain_one().is_some() {
            drained += 1;
        }
        assert_eq!(drained, 4);
        assert!(q.is_empty());
    }

    #[test]
    fn overflow_drops_oldest() {
        let q = BackpressureQueue::with_capacity(2);
        enqueue_bundle(&q, &mk_bundle(1)).unwrap();
        enqueue_bundle(&q, &mk_bundle(2)).unwrap();
        // Now full ; this should drop bundle 1.
        enqueue_bundle(&q, &mk_bundle(3)).unwrap();
        let (_, _, drops) = q.stats();
        assert_eq!(drops, 1);
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn drain_all_resets_queue() {
        let q = BackpressureQueue::with_capacity(8);
        for i in 1..=3 {
            enqueue_bundle(&q, &mk_bundle(i)).unwrap();
        }
        assert_eq!(q.drain_all(), 3);
        assert!(q.is_empty());
    }

    #[test]
    fn drain_on_reconnect_preserves_order() {
        let q = BackpressureQueue::with_capacity(8);
        for i in 1..=5 {
            enqueue_bundle(&q, &mk_bundle(i)).unwrap();
        }
        // "Reconnect" : drain everything FIFO.
        let mut drained_order = Vec::new();
        while let Some(blob) = q.drain_one() {
            // Decompress + decode to assert order.
            let json = crate::compress::decompress_bundle(&blob).unwrap();
            let b: FederationBundle = serde_json::from_slice(&json).unwrap();
            drained_order.push(b.tick_id);
        }
        assert_eq!(drained_order, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn stats_track_enqueue_drain_drops() {
        let q = BackpressureQueue::with_capacity(2);
        enqueue_bundle(&q, &mk_bundle(1)).unwrap();
        enqueue_bundle(&q, &mk_bundle(2)).unwrap();
        enqueue_bundle(&q, &mk_bundle(3)).unwrap(); // drops oldest
        q.drain_one();
        let (eq, dr, drp) = q.stats();
        assert_eq!(eq, 3);
        assert_eq!(dr, 1);
        assert_eq!(drp, 1);
    }

    #[test]
    fn default_starts_empty() {
        let q = BackpressureQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.capacity(), DEFAULT_QUEUE_CAPACITY);
    }
}
