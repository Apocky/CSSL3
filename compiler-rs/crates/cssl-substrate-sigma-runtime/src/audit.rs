//! § audit.rs — bounded ring-buffer for grant / revoke / evaluate events.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § DESIGN (per memory_sawyer_pokemon_efficiency)
//!   - Pre-allocated `Vec<AuditEntry>` of fixed [`AUDIT_RING_DEFAULT_CAPACITY`]
//!     entries · NEVER reallocates after construction.
//!   - Fixed 32-byte packed [`AuditEntry`] : timestamp + actor-hash + subject-
//!     hash + decision-tag + effect-bit + k-anon-tag + audit-seq.
//!   - Each [`AuditRing::push`] is O(1) lock + index-bump + slot-write — no
//!     allocation in the hot-path.
//!   - Drain is also zero-allocation : caller provides an output slice.
//!
//! § INVARIANTS
//!   - Once written, an entry slot is OVERWRITTEN by wrap-around, but the
//!     `audit_seq` field is monotone-increasing across all entries ; sinks
//!     (e.g. cssl-host-substrate-knowledge) detect skips by `seq` gaps.
//!   - The ring drops oldest-first when capacity exceeded ; this is documented
//!     and intentional. Lossless-archival is the on-chain Σ-Chain's job.
//!
//! § PRIME_DIRECTIVE alignment
//!   - § 7 INTEGRITY : append-only semantics within a single ring-cycle ;
//!     ring overwrites are recorded in the `wrap_count` so callers can
//!     detect if telemetry-pipe missed a drain-window.

use std::sync::Mutex;

/// Default ring-buffer capacity (entries).
///
/// § DESIGN : 8192 × 32 B = 256 KiB pre-allocated. Tunable by callers
/// who run higher evaluate-rates (e.g. mycelium-chat-sync hot-loop).
pub const AUDIT_RING_DEFAULT_CAPACITY: usize = 8192;

/// Decision-tag enum encoded as a single u8 for the 32-byte ring entry.
///
/// § STABILITY : variant byte-positions are FROZEN. Reordering = ABI break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum DecisionTag {
    /// Cap-grant issued.
    GrantIssued = 0,
    /// Cap-grant revoked (mask.revoke or cap.revocation_ref set).
    Revoked = 1,
    /// Evaluate returned Allow.
    Allow = 2,
    /// Evaluate returned Deny.
    Deny = 3,
    /// Evaluate returned NeedsKAnonymity.
    NeedsKAnon = 4,
    /// Evaluate returned NeedsCap.
    NeedsCap = 5,
    /// Evaluate returned Expired.
    Expired = 6,
    /// Mask checksum failed (tamper-detect).
    Tampered = 7,
}

impl DecisionTag {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// 32-byte packed audit-entry.
///
/// § BYTE-LAYOUT
///
/// ```text
///   offset | bytes | field            | semantic
///   -------+-------+------------------+--------------------------------------
///     0    |   8   | timestamp_us     | u64 microseconds-since-epoch
///     8    |   8   | actor_hash_lo    | u64 low-half of BLAKE3(actor-id)
///    16    |   8   | subject_hash_lo  | u64 low-half of BLAKE3(subject-id)
///    24    |   1   | decision_tag     | DecisionTag-as-u8
///    25    |   3   | effect_bit_le24  | u24 effect-cap bit (low 24 of u32)
///    28    |   1   | k_anon_tag       | u8 (k-anon-floor encountered)
///    29    |   3   | audit_seq_lo24   | u24 monotone audit-seq low 24 bits
///   -------+-------+------------------+--------------------------------------
///                  32 B total
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEntry {
    pub timestamp_us: u64,
    pub actor_hash_lo: u64,
    pub subject_hash_lo: u64,
    pub decision_tag: u8,
    pub effect_bit: u32,
    pub k_anon_tag: u8,
    pub audit_seq: u32,
}

impl AuditEntry {
    /// Pack into canonical 32-byte little-endian byte-form.
    pub fn pack(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[0..8].copy_from_slice(&self.timestamp_us.to_le_bytes());
        out[8..16].copy_from_slice(&self.actor_hash_lo.to_le_bytes());
        out[16..24].copy_from_slice(&self.subject_hash_lo.to_le_bytes());
        out[24] = self.decision_tag;
        out[25] = (self.effect_bit & 0xFF) as u8;
        out[26] = ((self.effect_bit >> 8) & 0xFF) as u8;
        out[27] = ((self.effect_bit >> 16) & 0xFF) as u8;
        out[28] = self.k_anon_tag;
        out[29] = (self.audit_seq & 0xFF) as u8;
        out[30] = ((self.audit_seq >> 8) & 0xFF) as u8;
        out[31] = ((self.audit_seq >> 16) & 0xFF) as u8;
        out
    }

    /// Convenience builder for tests + audit::push call-sites.
    pub fn new(
        timestamp_us: u64,
        actor_hash_lo: u64,
        subject_hash_lo: u64,
        decision: DecisionTag,
        effect_bit: u32,
        k_anon_tag: u8,
        audit_seq: u32,
    ) -> Self {
        Self {
            timestamp_us,
            actor_hash_lo,
            subject_hash_lo,
            decision_tag: decision.as_u8(),
            effect_bit,
            k_anon_tag,
            audit_seq,
        }
    }
}

/// Bounded ring-buffer · pre-allocated · zero-allocation hot-path.
///
/// § THREAD-SAFETY : single-mutex serializes writes. For very-high-rate
/// evaluators, a future revision can shard rings per-thread + merge on
/// drain ; current sibling-crates target ≤10k evaluate/s which is well
/// within single-mutex-throughput.
#[derive(Debug)]
pub struct AuditRing {
    inner: Mutex<RingInner>,
}

#[derive(Debug)]
struct RingInner {
    /// Pre-allocated slot buffer · index-modulo `capacity`.
    slots: Vec<AuditEntry>,
    /// Next-write index modulo capacity.
    head: usize,
    /// Number of entries written (for monotone audit_seq).
    total_written: u64,
    /// Number of times the ring has wrapped (oldest-overwrite count).
    wrap_count: u64,
    /// Capacity (constant after construction).
    capacity: usize,
}

impl AuditRing {
    /// Construct a new ring with `capacity` slots. Uses
    /// [`AUDIT_RING_DEFAULT_CAPACITY`] when 0 is supplied.
    pub fn new(capacity: usize) -> Self {
        let capacity = if capacity == 0 {
            AUDIT_RING_DEFAULT_CAPACITY
        } else {
            capacity
        };
        let zero_entry = AuditEntry::new(0, 0, 0, DecisionTag::Allow, 0, 0, 0);
        let slots = vec![zero_entry; capacity];
        Self {
            inner: Mutex::new(RingInner {
                slots,
                head: 0,
                total_written: 0,
                wrap_count: 0,
                capacity,
            }),
        }
    }

    /// Push one entry. `audit_seq` is auto-assigned + returned.
    ///
    /// § HOT-PATH : O(1) lock + slot-write + counter-bump. Zero allocations.
    pub fn push(&self, mut entry: AuditEntry) -> u64 {
        let mut g = self.inner.lock().expect("audit-ring poisoned");
        let seq = g.total_written;
        // Truncate to u24 for the packed-entry field ; full u64 retained
        // in `total_written`. Sinks correlate via timestamp + lo-bits.
        entry.audit_seq = (seq & 0x00FF_FFFF) as u32;
        let cap = g.capacity;
        let head = g.head;
        g.slots[head] = entry;
        let new_head = head + 1;
        if new_head >= cap {
            g.head = 0;
            g.wrap_count += 1;
        } else {
            g.head = new_head;
        }
        g.total_written = seq + 1;
        seq
    }

    /// Drain up to `dst.len()` most-recent entries, oldest-first within
    /// the requested window. Returns the number of entries written.
    ///
    /// § HOT-PATH : zero-allocation · caller-supplied slice. Lock is held
    /// for the duration of the copy ; on contended workloads consider
    /// drain-into-thread-local-buffer + post-merge.
    pub fn drain(&self, dst: &mut [AuditEntry]) -> usize {
        let g = self.inner.lock().expect("audit-ring poisoned");
        let avail = g.total_written.min(g.capacity as u64) as usize;
        let n = dst.len().min(avail);
        if n == 0 {
            return 0;
        }
        // Reconstruct oldest-first ordering. If the ring has wrapped,
        // oldest entry is at `head` ; otherwise it's at index 0.
        let oldest_idx = if g.total_written > g.capacity as u64 {
            g.head
        } else {
            0
        };
        let cap = g.capacity;
        let start = oldest_idx + (avail - n);
        for i in 0..n {
            dst[i] = g.slots[(start + i) % cap];
        }
        n
    }

    /// Total entries written across the lifetime of the ring (incl. wraps).
    pub fn total_written(&self) -> u64 {
        self.inner.lock().expect("audit-ring poisoned").total_written
    }

    /// Number of times the ring has wrapped (oldest-overwrite count).
    pub fn wrap_count(&self) -> u64 {
        self.inner.lock().expect("audit-ring poisoned").wrap_count
    }

    /// Capacity (slot count).
    pub fn capacity(&self) -> usize {
        self.inner.lock().expect("audit-ring poisoned").capacity
    }
}

impl Default for AuditRing {
    fn default() -> Self {
        Self::new(AUDIT_RING_DEFAULT_CAPACITY)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_entry(seq_hint: u32) -> AuditEntry {
        AuditEntry::new(seq_hint as u64 * 1_000, 0xAA, 0xBB, DecisionTag::Allow, 0x01, 0, 0)
    }

    #[test]
    fn t01_push_assigns_monotone_seq() {
        let r = AuditRing::new(64);
        assert_eq!(r.push(mk_entry(0)), 0);
        assert_eq!(r.push(mk_entry(1)), 1);
        assert_eq!(r.push(mk_entry(2)), 2);
        assert_eq!(r.total_written(), 3);
    }

    #[test]
    fn t02_drain_returns_oldest_first_within_window() {
        let r = AuditRing::new(64);
        for i in 0..5 {
            r.push(mk_entry(i));
        }
        let mut buf = [AuditEntry::new(0, 0, 0, DecisionTag::Allow, 0, 0, 0); 5];
        let n = r.drain(&mut buf);
        assert_eq!(n, 5);
        // timestamp encodes seq-hint × 1000 ; assert ascending.
        for i in 1..5 {
            assert!(buf[i].timestamp_us > buf[i - 1].timestamp_us);
        }
    }

    #[test]
    fn t03_pack_size_is_32_bytes() {
        let e = mk_entry(0);
        let bytes = e.pack();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn t04_wrap_overwrites_oldest_and_bumps_wrap_count() {
        let r = AuditRing::new(4);
        for i in 0..10 {
            r.push(mk_entry(i));
        }
        assert_eq!(r.total_written(), 10);
        assert_eq!(r.wrap_count(), 2, "10 pushes / 4 capacity = 2 full wraps + 2 in current cycle");
        // Drain 4 → should be the 4 most-recent (seq 6,7,8,9) oldest-first.
        let mut buf = [AuditEntry::new(0, 0, 0, DecisionTag::Allow, 0, 0, 0); 4];
        let n = r.drain(&mut buf);
        assert_eq!(n, 4);
        // ascending timestamps confirm oldest-first
        for i in 1..4 {
            assert!(buf[i].timestamp_us > buf[i - 1].timestamp_us);
        }
    }

    #[test]
    fn t05_default_capacity_is_8192() {
        let r = AuditRing::default();
        assert_eq!(r.capacity(), AUDIT_RING_DEFAULT_CAPACITY);
    }

    #[test]
    fn t06_drain_of_empty_returns_zero() {
        let r = AuditRing::new(8);
        let mut buf = [mk_entry(0); 4];
        assert_eq!(r.drain(&mut buf), 0);
    }

    #[test]
    fn t07_audit_seq_monotone_across_wrap() {
        let r = AuditRing::new(4);
        let mut last = 0u64;
        for _ in 0..16 {
            let s = r.push(mk_entry(0));
            if last > 0 {
                assert!(s > last - 1);
            }
            last = s;
        }
        assert_eq!(r.total_written(), 16);
    }

    #[test]
    fn t08_decision_tag_byte_values_are_stable() {
        // ABI-locked variant byte-positions.
        assert_eq!(DecisionTag::GrantIssued.as_u8(), 0);
        assert_eq!(DecisionTag::Revoked.as_u8(), 1);
        assert_eq!(DecisionTag::Allow.as_u8(), 2);
        assert_eq!(DecisionTag::Deny.as_u8(), 3);
        assert_eq!(DecisionTag::NeedsKAnon.as_u8(), 4);
        assert_eq!(DecisionTag::NeedsCap.as_u8(), 5);
        assert_eq!(DecisionTag::Expired.as_u8(), 6);
        assert_eq!(DecisionTag::Tampered.as_u8(), 7);
    }
}
