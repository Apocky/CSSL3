// § T11-WAVE3-PROCGEN-ROOMS : pure-stdlib PCG-32 RNG
// ══════════════════════════════════════════════════════════════════
//! Pure-stdlib PCG-XSH-RR (PCG-32) RNG.
//!
//! § O'Neill 2014 ; period 2^64 ; output 32 bits ; state 64 bits.
//!
//! Avoids `rand` crate dependency to keep procgen self-contained and
//! deterministic across rustc versions (rand-impl details can change ;
//! a hand-rolled PCG-32 cannot).

/// § PCG-32 (XSH-RR variant) state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pcg32 {
    state: u64,
    inc:   u64,
}

impl Pcg32 {
    /// § Default stream-selector (odd const) ; seeds with `seed` mixed in.
    const DEFAULT_INC: u64 = 0xda3e_39cb_94b9_5bdb;
    /// § PCG multiplier per O'Neill reference impl.
    const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

    /// § Construct a Pcg32 from a 64-bit seed using the canonical PCG init.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc:   (Self::DEFAULT_INC << 1) | 1,
        };
        // Standard PCG init : advance state, fold seed, advance state.
        let _ = rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        let _ = rng.next_u32();
        rng
    }

    /// § Advance state, output 32 bits via XSH-RR transform.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let oldstate = self.state;
        self.state = oldstate.wrapping_mul(Self::MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((oldstate >> 18) ^ oldstate) >> 27) as u32;
        let rot = (oldstate >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// § Output a uniform `f32` in `[0.0, 1.0)`.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        // 24-bit precision : top 24 bits / 2^24.
        let bits = self.next_u32() >> 8;
        bits as f32 / (1u32 << 24) as f32
    }

    /// § Output a uniform `u32` in `[lo, hi)`. Clamps if `hi <= lo`.
    #[inline]
    pub fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        let span = hi - lo;
        lo + (self.next_u32() % span)
    }
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    /// § Determinism : same seed → same output sequence.
    #[test]
    fn determinism_same_seed_same_output() {
        let mut a = Pcg32::new(0xdead_beef);
        let mut b = Pcg32::new(0xdead_beef);
        for _ in 0..1024 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    /// § Different seeds yield different sequences (with overwhelming probability).
    #[test]
    fn different_seeds_differ() {
        let mut a = Pcg32::new(1);
        let mut b = Pcg32::new(2);
        let mut differ = false;
        for _ in 0..32 {
            if a.next_u32() != b.next_u32() {
                differ = true;
                break;
            }
        }
        assert!(differ, "two different-seeded PCG-32 streams should diverge within 32 draws");
    }

    /// § range_u32 respects [lo, hi) bounds.
    #[test]
    fn range_respects_bounds() {
        let mut rng = Pcg32::new(42);
        for _ in 0..10_000 {
            let v = rng.range_u32(10, 20);
            assert!(v >= 10 && v < 20, "range_u32 returned {v} outside [10, 20)");
        }
        // Edge case : hi == lo → returns lo.
        assert_eq!(rng.range_u32(5, 5), 5);
        // Edge case : hi < lo → returns lo (clamp).
        assert_eq!(rng.range_u32(10, 3), 10);
    }

    /// § next_f32 outputs values in [0.0, 1.0).
    #[test]
    fn f32_in_zero_one() {
        let mut rng = Pcg32::new(7);
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "next_f32 returned {v} outside [0.0, 1.0)");
        }
    }

    /// § u32 distribution is non-degenerate (more than one distinct value).
    #[test]
    fn u32_distribution_not_degenerate() {
        let mut rng = Pcg32::new(99);
        let mut bucket_a = 0u32;
        let mut bucket_b = 0u32;
        for _ in 0..1000 {
            if rng.next_u32() & 1 == 0 {
                bucket_a += 1;
            } else {
                bucket_b += 1;
            }
        }
        // Each bucket should hold a meaningful share of the draws.
        assert!(bucket_a > 100 && bucket_b > 100, "bucket_a={bucket_a} bucket_b={bucket_b} — distribution degenerate");
    }
}
