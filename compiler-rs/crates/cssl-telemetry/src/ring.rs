//! Single-producer single-consumer telemetry ring-buffer.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § RING-BUFFER IMPLEMENTATION.
//!
//! § DESIGN
//!   Stage-0 uses `std::cell::RefCell<VecDeque<TelemetrySlot>>` as a single-thread
//!   SPSC stand-in with overflow-counting semantics that match the final atomic
//!   ring. Phase-2 swaps this for a real lock-free SPSC via `AtomicU64` head/tail.
//!
//!   The public API is stable across the swap : `push` / `drain_all` / `len` /
//!   `overflow_count` / `capacity` — tests pin the SPSC-invariants (producer-
//!   never-blocks, consumer-drains-in-order, overflow-counts-not-drops-silently).

use core::cell::RefCell;
use std::collections::VecDeque;

use thiserror::Error;

use crate::scope::{TelemetryKind, TelemetryScope};

/// Fixed 64-byte ring-slot record mirroring `specs/22` § TelemetrySlot layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TelemetrySlot {
    /// Timestamp in nanoseconds (monotonic).
    pub timestamp_ns: u64,
    /// Scope encoded as `TelemetryScope::as_u16`.
    pub scope: u16,
    /// Kind encoded as `TelemetryKind::as_u16`.
    pub kind: u16,
    /// Thread-id (platform-specific ; see `spec/22` § producer).
    pub thread_id: u32,
    /// CPU-core / GPU-device id.
    pub cpu_or_gpu_id: u32,
    /// Inline small-payload (40 bytes fixed ; larger payloads spill to `payload_extern_ptr`).
    pub payload: [u8; 40],
    /// Nullable external-payload pointer (0 = inline-only).
    pub payload_extern_ptr: u64,
}

impl TelemetrySlot {
    /// Build a scope+kind+timestamp slot with empty payload.
    #[must_use]
    pub const fn new(timestamp_ns: u64, scope: TelemetryScope, kind: TelemetryKind) -> Self {
        Self {
            timestamp_ns,
            scope: scope.as_u16(),
            kind: kind.as_u16(),
            thread_id: 0,
            cpu_or_gpu_id: 0,
            payload: [0u8; 40],
            payload_extern_ptr: 0,
        }
    }

    /// Write the first `len` bytes of `bytes` into `payload` (truncating if longer).
    #[must_use]
    pub fn with_inline_payload(mut self, bytes: &[u8]) -> Self {
        let len = core::cmp::min(bytes.len(), self.payload.len());
        self.payload[..len].copy_from_slice(&bytes[..len]);
        self
    }
}

/// Stage-0 single-thread SPSC ring with overflow-counting.
#[derive(Debug)]
pub struct TelemetryRing {
    slots: RefCell<VecDeque<TelemetrySlot>>,
    capacity: usize,
    overflow: core::cell::Cell<u64>,
    total_pushed: core::cell::Cell<u64>,
}

impl TelemetryRing {
    /// Create a ring with the given slot-capacity. Panics if capacity = 0.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "TelemetryRing capacity must be > 0");
        Self {
            slots: RefCell::new(VecDeque::with_capacity(capacity)),
            capacity,
            overflow: core::cell::Cell::new(0),
            total_pushed: core::cell::Cell::new(0),
        }
    }

    /// Capacity (max slots in flight).
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Current pending slots (producer-visible, consumer-unobserved).
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.borrow().len()
    }

    /// True iff no pending slots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total overflow events (producer-writes discarded because ring was full).
    #[must_use]
    pub fn overflow_count(&self) -> u64 {
        self.overflow.get()
    }

    /// Total pushes attempted (successful + overflow).
    #[must_use]
    pub fn total_pushed(&self) -> u64 {
        self.total_pushed.get()
    }

    /// Non-blocking producer : write a slot. If the ring is full, increments
    /// `overflow_count` and returns `RingError::Overflow`. The producer never-blocks
    /// per `specs/22` § "producer-never-blocks, telemetry prefers lossy-non-blocking".
    ///
    /// # Errors
    /// Returns [`RingError::Overflow`] when the ring is at capacity. Callers should
    /// treat this as a soft drop (monotonic counter ticks).
    pub fn push(&self, slot: TelemetrySlot) -> Result<(), RingError> {
        self.total_pushed
            .set(self.total_pushed.get().saturating_add(1));
        let mut slots = self.slots.borrow_mut();
        if slots.len() >= self.capacity {
            self.overflow.set(self.overflow.get().saturating_add(1));
            return Err(RingError::Overflow);
        }
        slots.push_back(slot);
        Ok(())
    }

    /// Consumer : drain every pending slot in FIFO order.
    #[must_use]
    pub fn drain_all(&self) -> Vec<TelemetrySlot> {
        self.slots.borrow_mut().drain(..).collect()
    }

    /// Consumer : peek at the next slot without removing it.
    #[must_use]
    pub fn peek(&self) -> Option<TelemetrySlot> {
        self.slots.borrow().front().copied()
    }
}

/// Ring-buffer failure modes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RingError {
    /// Producer write was discarded because the ring was at capacity.
    #[error("ring-buffer full — producer slot dropped (overflow-counter incremented)")]
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::{RingError, TelemetryRing, TelemetrySlot};
    use crate::scope::{TelemetryKind, TelemetryScope};

    #[test]
    fn new_ring_has_capacity() {
        let r = TelemetryRing::new(16);
        assert_eq!(r.capacity(), 16);
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
        assert_eq!(r.overflow_count(), 0);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn zero_capacity_panics() {
        let _ = TelemetryRing::new(0);
    }

    #[test]
    fn slot_new_zeroes_payload() {
        let s = TelemetrySlot::new(1000, TelemetryScope::Power, TelemetryKind::Sample);
        assert_eq!(s.timestamp_ns, 1000);
        assert_eq!(s.scope, TelemetryScope::Power.as_u16());
        assert_eq!(s.kind, TelemetryKind::Sample.as_u16());
        assert_eq!(s.payload, [0u8; 40]);
    }

    #[test]
    fn slot_with_inline_payload_writes_bytes() {
        let s = TelemetrySlot::new(0, TelemetryScope::Counters, TelemetryKind::Counter)
            .with_inline_payload(b"hello");
        assert_eq!(&s.payload[0..5], b"hello");
        assert_eq!(s.payload[5], 0);
    }

    #[test]
    fn slot_with_inline_payload_truncates_long() {
        let big = [0xABu8; 64];
        let s = TelemetrySlot::new(0, TelemetryScope::Counters, TelemetryKind::Counter)
            .with_inline_payload(&big);
        assert_eq!(s.payload, [0xABu8; 40]);
    }

    #[test]
    fn push_and_drain_preserves_fifo_order() {
        let r = TelemetryRing::new(4);
        for t in 0u64..4 {
            r.push(TelemetrySlot::new(
                t,
                TelemetryScope::Counters,
                TelemetryKind::Counter,
            ))
            .unwrap();
        }
        assert_eq!(r.len(), 4);
        let drained = r.drain_all();
        assert_eq!(drained.len(), 4);
        for (i, s) in drained.iter().enumerate() {
            assert_eq!(s.timestamp_ns, i as u64);
        }
        assert!(r.is_empty());
    }

    #[test]
    fn overflow_increments_counter_not_blocks() {
        let r = TelemetryRing::new(2);
        r.push(TelemetrySlot::new(
            1,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        ))
        .unwrap();
        r.push(TelemetrySlot::new(
            2,
            TelemetryScope::Counters,
            TelemetryKind::Counter,
        ))
        .unwrap();
        // Third push : overflow.
        let err = r
            .push(TelemetrySlot::new(
                3,
                TelemetryScope::Counters,
                TelemetryKind::Counter,
            ))
            .unwrap_err();
        assert_eq!(err, RingError::Overflow);
        assert_eq!(r.overflow_count(), 1);
        assert_eq!(r.total_pushed(), 3);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn peek_does_not_remove() {
        let r = TelemetryRing::new(2);
        r.push(TelemetrySlot::new(
            42,
            TelemetryScope::Events,
            TelemetryKind::Sample,
        ))
        .unwrap();
        let peeked = r.peek().unwrap();
        assert_eq!(peeked.timestamp_ns, 42);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn total_pushed_counts_all_attempts() {
        let r = TelemetryRing::new(1);
        for t in 0..5u64 {
            let _ = r.push(TelemetrySlot::new(
                t,
                TelemetryScope::Counters,
                TelemetryKind::Counter,
            ));
        }
        assert_eq!(r.total_pushed(), 5);
        // Only 1 made it in ; 4 overflowed.
        assert_eq!(r.overflow_count(), 4);
        assert_eq!(r.len(), 1);
    }
}
