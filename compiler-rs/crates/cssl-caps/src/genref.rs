//! Vale-style generational reference : packed `u40` index + `u24` generation into `u64`.
//!
//! § SPEC (`specs/12` § VALE GEN-REFS AS `ref<T>`) :
//!
//! ```text
//!   struct GenRef<T> : @layout(cpu, std430) {
//!     idx  : u40
//!     gen  : u24
//!   }  // packed u64
//!
//!   deref(r, pool) = if pool.generations[r.idx] == r.gen  { Ok(&pool[r.idx]) }
//!                    else { Err(StaleRef) }
//!
//!   invariant : pool.free(idx) → pool.generations[idx] += 1
//! ```
//!
//! § STAGE-0 SHAPE
//!   This crate exposes the packed layout as a Rust `u64` newtype plus
//!   pack / unpack helpers. The actual `Pool<T>` implementation + runtime
//!   deref-check synthesis is T10 work (runtime + codegen) ; stage-0 just
//!   validates the packed layout matches the spec.

/// Number of bits used for the generation counter.
pub const GEN_BITS: u32 = 24;
/// Number of bits used for the index.
pub const IDX_BITS: u32 = 40;
/// Bitmask for the generation field.
pub const GEN_MASK: u64 = (1u64 << GEN_BITS) - 1;
/// Bitmask for the index field.
pub const IDX_MASK: u64 = (1u64 << IDX_BITS) - 1;

/// Packed 64-bit generational reference. Low `IDX_BITS` hold the index ; high
/// `GEN_BITS` hold the generation counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]
pub struct GenRef(pub u64);

impl GenRef {
    /// Pack an index + generation into a `GenRef`. If either field overflows
    /// its width, the value is silently truncated — callers are expected to
    /// validate inputs before packing.
    #[must_use]
    pub const fn pack(idx: u64, gen: u64) -> Self {
        let i = idx & IDX_MASK;
        let g = gen & GEN_MASK;
        Self((g << IDX_BITS) | i)
    }

    /// Extract the index field.
    #[must_use]
    pub const fn idx(self) -> u64 {
        self.0 & IDX_MASK
    }

    /// Extract the generation field.
    #[must_use]
    pub const fn gen(self) -> u64 {
        (self.0 >> IDX_BITS) & GEN_MASK
    }

    /// Returns a new `GenRef` with the generation incremented by 1 (wrapping).
    /// Used when a pool slot is freed : the next-allocation sees an incremented
    /// generation, so any lingering ref with the old generation becomes stale.
    #[must_use]
    pub const fn bump_gen(self) -> Self {
        let next = (self.gen().wrapping_add(1)) & GEN_MASK;
        Self::pack(self.idx(), next)
    }

    /// A sentinel `GenRef` representing "null" / "unallocated".
    pub const NULL: Self = Self(0);
}

#[cfg(test)]
mod tests {
    use super::{GenRef, GEN_BITS, GEN_MASK, IDX_BITS, IDX_MASK};

    #[test]
    fn bit_widths_match_spec() {
        assert_eq!(IDX_BITS, 40);
        assert_eq!(GEN_BITS, 24);
        assert_eq!(IDX_BITS + GEN_BITS, 64);
    }

    #[test]
    fn masks_cover_correct_bits() {
        assert_eq!(IDX_MASK, (1u64 << 40) - 1);
        assert_eq!(GEN_MASK, (1u64 << 24) - 1);
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let r = GenRef::pack(42, 7);
        assert_eq!(r.idx(), 42);
        assert_eq!(r.gen(), 7);
    }

    #[test]
    fn pack_truncates_overflow_silently() {
        // gen > 24 bits
        let r = GenRef::pack(100, 1u64 << 25);
        assert_eq!(r.idx(), 100);
        assert_eq!(r.gen(), 0); // high bit of gen is masked away
    }

    #[test]
    fn bump_gen_increments_generation() {
        let r = GenRef::pack(1, 5);
        let b = r.bump_gen();
        assert_eq!(b.idx(), 1);
        assert_eq!(b.gen(), 6);
    }

    #[test]
    fn bump_gen_wraps() {
        let r = GenRef::pack(0, GEN_MASK);
        let b = r.bump_gen();
        assert_eq!(b.gen(), 0, "generation wraps at 2^24");
    }

    #[test]
    fn null_sentinel_is_zero() {
        assert_eq!(GenRef::NULL.0, 0);
        assert_eq!(GenRef::NULL.idx(), 0);
        assert_eq!(GenRef::NULL.gen(), 0);
    }

    #[test]
    fn max_idx_and_gen_packable() {
        let r = GenRef::pack(IDX_MASK, GEN_MASK);
        assert_eq!(r.idx(), IDX_MASK);
        assert_eq!(r.gen(), GEN_MASK);
    }

    #[test]
    fn pack_is_const_eval_capable() {
        // Verify `pack` can be called in a const context.
        const REF: GenRef = GenRef::pack(100, 50);
        assert_eq!(REF.idx(), 100);
        assert_eq!(REF.gen(), 50);
    }
}
