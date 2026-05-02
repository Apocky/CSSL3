//! § ring — `ChatPatternRing` bounded SPSC ring-buffer
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   Per-player local-observation buffer. Bounded · pre-allocated · ¬ heap-
//!   churn-on-hot-path. Mutex<VecDeque> with overwrite-oldest discipline
//!   gives SPSC-safe semantics for our access pattern (single producer +
//!   single drain-consumer). The std-only choice avoids the parking_lot /
//!   crossbeam dlltool gate on the Windows-GNU toolchain (see crate
//!   Cargo.toml for the full diagnosis).
//!
//! § CAPACITY
//!   Default 1024 patterns ⊑ 32-bytes-each ⟶ 32 KiB per-player. At a
//!   typical 1 chat-line / 5s rate, this holds ≈ 85 minutes of observations
//!   before the producer overruns ; the digest-loop drains every 60s so
//!   overrun is rare in steady-state.
//!
//! § DROP POLICY
//!   On full-buffer push, the OLDEST pattern is dropped (overwrite-oldest
//!   ring discipline). This preserves the freshness of recent observations
//!   ; a stale 60s-old pattern is more expendable than a fresh one.
//!
//! § OBSERVABILITY
//!   - `pushed_total` — monotonic count of `push` calls (incl. dropped)
//!   - `dropped_total` — count of overwrite-oldest evictions
//!   - `drained_total` — count of `drain_all` extractions

use crate::pattern::ChatPattern;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// § ChatPatternRing — bounded ring of `ChatPattern`s.
///
/// `Arc`-shareable so producer (player-input-handler) and consumer (digest-
/// loop) can hold references concurrently. Internally a `Mutex<VecDeque>`
/// with capacity-bounded push ; the lock-scope is the unit of atomicity for
/// the overwrite-oldest discipline + counter-bumps.
pub struct ChatPatternRing {
    inner: Arc<Mutex<VecDeque<ChatPattern>>>,
    pushed_total: Arc<AtomicU64>,
    dropped_total: Arc<AtomicU64>,
    drained_total: Arc<AtomicU64>,
    capacity: usize,
}

impl Clone for ChatPatternRing {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            pushed_total: Arc::clone(&self.pushed_total),
            dropped_total: Arc::clone(&self.dropped_total),
            drained_total: Arc::clone(&self.drained_total),
            capacity: self.capacity,
        }
    }
}

impl std::fmt::Debug for ChatPatternRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatPatternRing")
            .field("capacity", &self.capacity)
            .field("len", &self.len())
            .field("pushed_total", &self.pushed_total())
            .field("dropped_total", &self.dropped_total())
            .field("drained_total", &self.drained_total())
            .finish()
    }
}

/// § DEFAULT_CAPACITY — 1024 patterns ⊑ 32 KiB.
pub const DEFAULT_CAPACITY: usize = 1024;

impl ChatPatternRing {
    /// § new — allocate a ring with `capacity` slots. Capacity must be ≥ 1.
    /// A capacity of 0 is silently bumped to 1 to keep the queue valid.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(cap))),
            pushed_total: Arc::new(AtomicU64::new(0)),
            dropped_total: Arc::new(AtomicU64::new(0)),
            drained_total: Arc::new(AtomicU64::new(0)),
            capacity: cap,
        }
    }

    /// § default-cap — 1024 slots.
    #[must_use]
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    /// § push — enqueue a pattern. On full-buffer, evicts the oldest entry
    /// (overwrite-oldest discipline). Returns `true` iff the new pattern
    /// was accepted (always true ; signature kept for forward-compat).
    pub fn push(&self, pattern: ChatPattern) -> bool {
        self.pushed_total.fetch_add(1, Ordering::Relaxed);
        let Ok(mut g) = self.inner.lock() else {
            return false;
        };
        if g.len() >= self.capacity {
            let _ = g.pop_front();
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
        }
        g.push_back(pattern);
        true
    }

    /// § drain_all — pop every pattern currently in the buffer ; returns
    /// them in FIFO-insertion-order. After this call the ring is empty.
    pub fn drain_all(&self) -> Vec<ChatPattern> {
        let Ok(mut g) = self.inner.lock() else {
            return Vec::new();
        };
        let out: Vec<ChatPattern> = g.drain(..).collect();
        drop(g);
        self.drained_total
            .fetch_add(out.len() as u64, Ordering::Relaxed);
        out
    }

    /// § len — current occupancy. Snapshot ; may be stale by the time the
    /// caller acts on it.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map_or(0, |g| g.len())
    }

    /// § is_empty — `len() == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// § capacity — fixed at-construction.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub fn pushed_total(&self) -> u64 {
        self.pushed_total.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn dropped_total(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn drained_total(&self) -> u64 {
        self.drained_total.load(Ordering::Relaxed)
    }

    /// § purge — drop EVERYTHING in the ring + return the count purged.
    /// Used by the sovereign-revoke flow : flushing local observations
    /// before requesting peer-purge is a defense-in-depth measure.
    pub fn purge(&self) -> usize {
        let Ok(mut g) = self.inner.lock() else {
            return 0;
        };
        let n = g.len();
        g.clear();
        drop(g);
        self.dropped_total.fetch_add(n as u64, Ordering::Relaxed);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{ArcPhase, ChatPatternBuilder, IntentKind, ResponseShape, CAP_FLAGS_ALL};

    fn mk_pattern(seed: u8) -> ChatPattern {
        ChatPatternBuilder {
            intent_kind: IntentKind::Question,
            response_shape: ResponseShape::ScenicNarrative,
            arc_phase: ArcPhase::Setup,
            confidence: 0.5,
            ts_unix: 60 * (u64::from(seed) + 1),
            region_tag: u16::from(seed),
            opt_in_tier: 1,
            cap_flags: CAP_FLAGS_ALL,
            emitter_pubkey: [seed; 32],
            co_signers: vec![],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn push_and_drain_basic() {
        let r = ChatPatternRing::new(4);
        assert!(r.is_empty());
        r.push(mk_pattern(1));
        r.push(mk_pattern(2));
        r.push(mk_pattern(3));
        assert_eq!(r.len(), 3);
        let drained = r.drain_all();
        assert_eq!(drained.len(), 3);
        assert!(r.is_empty());
    }

    #[test]
    fn capacity_min_one() {
        let r = ChatPatternRing::new(0);
        assert_eq!(r.capacity(), 1);
    }

    #[test]
    fn overwrite_oldest_when_full() {
        let r = ChatPatternRing::new(2);
        r.push(mk_pattern(1));
        r.push(mk_pattern(2));
        // full ; this push must evict and accept.
        r.push(mk_pattern(3));
        assert_eq!(r.len(), 2);
        assert!(r.dropped_total() >= 1);
        let drained = r.drain_all();
        // Eldest (seed=1) should be gone ; seeds 2 and 3 remain.
        let regions: Vec<u16> = drained.iter().map(super::ChatPattern::region_tag).collect();
        assert!(regions.contains(&2));
        assert!(regions.contains(&3));
        assert!(!regions.contains(&1));
    }

    #[test]
    fn counters_track_activity() {
        let r = ChatPatternRing::new(8);
        for i in 0..5 {
            r.push(mk_pattern(i));
        }
        assert_eq!(r.pushed_total(), 5);
        let _ = r.drain_all();
        assert_eq!(r.drained_total(), 5);
    }

    #[test]
    fn purge_clears_buffer() {
        let r = ChatPatternRing::new(8);
        for i in 0..5 {
            r.push(mk_pattern(i));
        }
        assert_eq!(r.purge(), 5);
        assert!(r.is_empty());
    }

    #[test]
    fn clone_shares_state() {
        let r1 = ChatPatternRing::new(8);
        let r2 = r1.clone();
        r1.push(mk_pattern(1));
        assert_eq!(r2.len(), 1);
        let drained = r2.drain_all();
        assert_eq!(drained.len(), 1);
        assert!(r1.is_empty());
    }

    #[test]
    fn drain_empty_returns_empty_vec() {
        let r = ChatPatternRing::new(4);
        let drained = r.drain_all();
        assert!(drained.is_empty());
    }
}
