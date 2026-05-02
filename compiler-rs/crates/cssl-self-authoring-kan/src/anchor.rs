//! § anchor.rs — Σ-Chain anchor cadence + checkpoint records (rollback-safe).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § AnchorRecord
//!   Every [`ANCHOR_EVERY_N_UPDATES`] (1024) successful bias-updates, the
//!   loop emits an [`AnchorRecord`] capturing the current TemplateBiasMap
//!   state-hash, the update-counter, and a sequence-number. Rollback to a
//!   prior anchor is supported by replaying signals from the reservoir
//!   forward from the anchor's update-counter.
//!
//! § AnchorRing
//!   Pre-allocated fixed-capacity ring of AnchorRecord. Older anchors evict
//!   FIFO. Default capacity = 64 (≈ 64 × 1024 = 65k updates of replay-history).
//!
//! § DESIGN (per memory_sawyer_pokemon_efficiency)
//!   - Pre-alloc : `Box<[Option<AnchorRecord>; ANCHOR_RING_CAPACITY]>`.
//!   - 16-byte hash (BLAKE3-128 truncated · sufficient for in-memory tamper).
//!   - Differential timestamps : record timestamp_offset_seconds from
//!     `AnchorRing::created_at_seconds`.

// ───────────────────────────────────────────────────────────────────────────
// § Constants
// ───────────────────────────────────────────────────────────────────────────

/// Anchor cadence : every 1024 successful bias-updates ⇒ one anchor.
pub const ANCHOR_EVERY_N_UPDATES: u64 = 1024;

/// Anchor-ring capacity (FIFO eviction beyond this).
pub const ANCHOR_RING_CAPACITY: usize = 64;

// ───────────────────────────────────────────────────────────────────────────
// § AnchorRecord — single checkpoint.
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorRecord {
    /// Monotone-increasing anchor sequence (0, 1, 2, ...).
    pub seq: u64,
    /// Update-counter snapshot AT-the-anchor-point.
    pub update_count_at_anchor: u64,
    /// BLAKE3-128 truncated hash of TemplateBiasMap state.
    pub state_hash: [u8; 16],
    /// Differential seconds-from-ring-creation when the anchor was emitted.
    pub timestamp_offset_seconds: u32,
}

impl AnchorRecord {
    pub const fn new(
        seq: u64,
        update_count_at_anchor: u64,
        state_hash: [u8; 16],
        timestamp_offset_seconds: u32,
    ) -> Self {
        Self {
            seq,
            update_count_at_anchor,
            state_hash,
            timestamp_offset_seconds,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § AnchorRing
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct AnchorRing {
    records: Box<[Option<AnchorRecord>; ANCHOR_RING_CAPACITY]>,
    head: usize,
    next_seq: u64,
    created_at_seconds: u64,
}

impl AnchorRing {
    pub fn new(created_at_seconds: u64) -> Self {
        Self {
            records: Box::new([None; ANCHOR_RING_CAPACITY]),
            head: 0,
            next_seq: 0,
            created_at_seconds,
        }
    }

    pub const fn created_at_seconds(&self) -> u64 {
        self.created_at_seconds
    }

    pub const fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Number of currently-occupied slots (max ANCHOR_RING_CAPACITY).
    pub fn occupied(&self) -> usize {
        self.records.iter().filter(|r| r.is_some()).count()
    }

    /// Determine whether a fresh anchor should be emitted given the current
    /// update-counter. Anchor at update-count == K * ANCHOR_EVERY_N_UPDATES
    /// for K > 0.
    pub const fn should_anchor(&self, update_count: u64) -> bool {
        update_count > 0 && (update_count % ANCHOR_EVERY_N_UPDATES) == 0
    }

    /// Push a new anchor. FIFO eviction at capacity.
    /// Returns the assigned sequence-number.
    pub fn push(
        &mut self,
        update_count_at_anchor: u64,
        state_hash: [u8; 16],
        now_seconds: u64,
    ) -> u64 {
        let seq = self.next_seq;
        let timestamp_offset =
            (now_seconds.saturating_sub(self.created_at_seconds)).min(u64::from(u32::MAX)) as u32;
        let rec = AnchorRecord::new(seq, update_count_at_anchor, state_hash, timestamp_offset);
        self.records[self.head] = Some(rec);
        self.head = (self.head + 1) % ANCHOR_RING_CAPACITY;
        self.next_seq = self.next_seq.saturating_add(1);
        seq
    }

    /// Look up an anchor by sequence-number. Returns None if seq has been
    /// evicted or never existed.
    pub fn get_by_seq(&self, seq: u64) -> Option<AnchorRecord> {
        for r in self.records.iter().flatten() {
            if r.seq == seq {
                return Some(*r);
            }
        }
        None
    }

    /// Most-recent anchor (highest seq currently in ring), or None if empty.
    pub fn latest(&self) -> Option<AnchorRecord> {
        self.records
            .iter()
            .filter_map(|s| s.as_ref().copied())
            .max_by_key(|r| r.seq)
    }

    /// All anchors currently held, ascending by seq.
    pub fn all_records_sorted(&self) -> Vec<AnchorRecord> {
        let mut out: Vec<AnchorRecord> = self
            .records
            .iter()
            .filter_map(|s| s.as_ref().copied())
            .collect();
        out.sort_by_key(|r| r.seq);
        out
    }

    /// Verify a stored anchor against a freshly-recomputed state-hash.
    /// True iff the stored hash matches the supplied hash.
    pub fn verify_anchor(&self, seq: u64, current_hash: &[u8; 16]) -> bool {
        match self.get_by_seq(seq) {
            Some(rec) => rec.state_hash == *current_hash,
            None => false,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_ring_basic_push_and_seq() {
        let mut ring = AnchorRing::new(1000);
        let seq0 = ring.push(1024, [0xAA; 16], 1000);
        let seq1 = ring.push(2048, [0xBB; 16], 1010);
        assert_eq!(seq0, 0);
        assert_eq!(seq1, 1);
        assert_eq!(ring.next_seq(), 2);
        assert_eq!(ring.occupied(), 2);

        let r0 = ring.get_by_seq(0).unwrap();
        assert_eq!(r0.update_count_at_anchor, 1024);
        assert_eq!(r0.state_hash, [0xAA; 16]);
        assert_eq!(r0.timestamp_offset_seconds, 0);

        let r1 = ring.get_by_seq(1).unwrap();
        assert_eq!(r1.timestamp_offset_seconds, 10);
    }

    #[test]
    fn anchor_cadence_check() {
        let ring = AnchorRing::new(0);
        assert!(!ring.should_anchor(0));
        assert!(!ring.should_anchor(512));
        assert!(ring.should_anchor(1024));
        assert!(!ring.should_anchor(1500));
        assert!(ring.should_anchor(2048));
        assert!(ring.should_anchor(1024 * 50));
        assert!(!ring.should_anchor(1024 * 50 + 1));
    }

    #[test]
    fn anchor_ring_fifo_eviction_at_capacity() {
        let mut ring = AnchorRing::new(0);
        for i in 0..(ANCHOR_RING_CAPACITY + 5) {
            let mut h = [0u8; 16];
            h[0] = (i & 0xFF) as u8;
            ring.push(i as u64 * ANCHOR_EVERY_N_UPDATES, h, i as u64);
        }
        // After CAP+5 pushes, occupied = CAP.
        assert_eq!(ring.occupied(), ANCHOR_RING_CAPACITY);
        // Earliest 5 sequences are evicted.
        assert!(ring.get_by_seq(0).is_none());
        assert!(ring.get_by_seq(4).is_none());
        // Sequence 5 should still be present.
        assert!(ring.get_by_seq(5).is_some());
        // Latest seq is CAP+4.
        let latest = ring.latest().unwrap();
        assert_eq!(latest.seq, (ANCHOR_RING_CAPACITY + 4) as u64);
    }

    #[test]
    fn verify_anchor_matches_current_hash() {
        let mut ring = AnchorRing::new(0);
        let h = [0x42; 16];
        ring.push(1024, h, 0);
        assert!(ring.verify_anchor(0, &h));
        assert!(!ring.verify_anchor(0, &[0xFF; 16]));
        assert!(!ring.verify_anchor(99, &h));
    }
}
