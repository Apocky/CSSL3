//! Stack-frame spill-slot management.
//!
//! § ALIGNMENT INVARIANT
//!   Every spill slot is **16-byte aligned**. SSE / AVX loads + stores require
//!   16-byte alignment for `movaps` / `movdqa` paths ; rather than tracking
//!   per-vreg alignment requirements, we over-allocate every slot to 16 bytes.
//!   This wastes ~12 bytes per i32 spill — at S7-G2 the simplicity is worth it.
//!   The tighter packing (4-byte slots for i32 / 8-byte for i64 / 16 for SIMD)
//!   is a future-slice optimization once profiling surfaces frame-size as a
//!   bottleneck.
//!
//! § FRAME LAYOUT (low → high addresses)
//!   ┌──────────────────────────┐ rsp post-prologue
//!   │ slot 0 (16 bytes)        │
//!   │ slot 1 (16 bytes)        │
//!   │ ...                      │
//!   │ slot N-1 (16 bytes)      │
//!   ├──────────────────────────┤ rsp + total_frame_size
//!   │ saved callee-saved regs  │ (push'd by prologue)
//!   ├──────────────────────────┤
//!   │ return addr              │ (push'd by call instruction)
//!   ├──────────────────────────┤
//!   │ ... caller stack ...     │
//!
//!   Slot-i is at `[rsp + i * 16]` post-`sub rsp, total_frame_size`.

use crate::reg::RegBank;
use core::fmt;

/// One spill slot — 16-byte-aligned ; bank tags whether it's GP or XMM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpillSlot {
    /// Slot index (0-based). Final byte offset = `index * 16`.
    pub index: u32,
    /// Bank — used for emission-time `mov` vs `movsd` selection.
    pub bank: RegBank,
}

impl SpillSlot {
    /// Byte offset from rsp post-prologue (always a multiple of 16).
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.index * 16
    }
}

impl fmt::Display for SpillSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[rsp + {}]", self.offset())
    }
}

/// Allocator for spill slots within a function. Bumps a counter per allocation ;
/// the resulting `total_frame_size` is the bytes the prologue must subtract from
/// rsp.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpillSlots {
    next_index: u32,
    slots: Vec<SpillSlot>,
}

impl SpillSlots {
    /// Construct an empty slot allocator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_index: 0,
            slots: Vec::new(),
        }
    }

    /// Allocate a fresh 16-byte slot for the given bank.
    pub fn alloc(&mut self, bank: RegBank) -> SpillSlot {
        let s = SpillSlot {
            index: self.next_index,
            bank,
        };
        self.next_index += 1;
        self.slots.push(s);
        s
    }

    /// Number of slots allocated so far.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.next_index
    }

    /// Total frame size in bytes (16-byte aligned by construction).
    #[must_use]
    pub fn total_frame_size(&self) -> u32 {
        self.next_index * 16
    }

    /// All slots in allocation order.
    #[must_use]
    pub fn all(&self) -> &[SpillSlot] {
        &self.slots
    }
}

#[cfg(test)]
mod spill_tests {
    use super::*;
    use crate::reg::RegBank;

    #[test]
    fn empty_slots_have_zero_frame_size() {
        let s = SpillSlots::new();
        assert_eq!(s.count(), 0);
        assert_eq!(s.total_frame_size(), 0);
    }

    #[test]
    fn each_slot_is_sixteen_bytes() {
        let mut s = SpillSlots::new();
        let s0 = s.alloc(RegBank::Gp);
        let s1 = s.alloc(RegBank::Xmm);
        assert_eq!(s0.offset(), 0);
        assert_eq!(s1.offset(), 16);
        assert_eq!(s.total_frame_size(), 32);
    }

    #[test]
    fn slot_offset_always_multiple_of_sixteen() {
        let mut s = SpillSlots::new();
        for _ in 0..32 {
            let slot = s.alloc(RegBank::Gp);
            assert_eq!(slot.offset() % 16, 0, "slot {} not 16-aligned", slot.index);
        }
    }

    #[test]
    fn bank_preserved_through_alloc() {
        let mut s = SpillSlots::new();
        let gp = s.alloc(RegBank::Gp);
        let xmm = s.alloc(RegBank::Xmm);
        assert_eq!(gp.bank, RegBank::Gp);
        assert_eq!(xmm.bank, RegBank::Xmm);
    }
}
