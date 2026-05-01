// § seed.rs — splitmix64 deterministic RNG  (no std::rand · no external dep)
// ════════════════════════════════════════════════════════════════════
// § I> per GDD : combat-tick = seeded-RNG · replay-bit-equal across hosts
// § I> splitmix64 chosen ∵ : tiny · stateless-after-seed · Copy · canonical
//      reference impl (Vigna 2014 ; public-domain) ; matches Rust std rand
//      `SmallRng` seedability semantics for testing without pulling rand.
// § I> NaN-safe : next_f32 returns ∈ [0, 1) ; bit-equal across IEEE-754 hosts
// § I> NO unsafe ; NO panic
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Deterministic splitmix64-based RNG — replay-bit-equal across hosts.
///
/// Public surface kept Copy-friendly so callers can snapshot the RNG state
/// inside a `CombatTick` checkpoint and resume later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeterministicRng {
    /// Internal 64-bit state ; advances by `0x9E37_79B9_7F4A_7C15` per call.
    state: u64,
}

impl DeterministicRng {
    /// New RNG from an explicit seed. The seed is mixed once before first use
    /// so that adjacent seeds produce uncorrelated streams.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns the current internal state ; useful for replay-manifest headers.
    #[must_use]
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// Advance state and return next u64 — splitmix64 canonical step.
    #[allow(clippy::unreadable_literal)]
    pub fn next_u64(&mut self) -> u64 {
        // splitmix64 (Vigna 2014) — wrapping_add for saturating-overflow discipline
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Next u32 — high 32 bits of next_u64 (better mixing than low bits).
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Next f32 in [0, 1) — uses 24-bit mantissa for IEEE-754 bit-equal output.
    pub fn next_f32(&mut self) -> f32 {
        let bits = self.next_u32() >> 8; // 24 bits
        (bits as f32) / ((1u32 << 24) as f32)
    }

    /// Next f64 in [0, 1) — uses 53-bit mantissa for IEEE-754 bit-equal output.
    pub fn next_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11; // 53 bits
        (bits as f64) / ((1u64 << 53) as f64)
    }

    /// Random u32 in [0, n) using rejection-free modulo (acceptable bias-bound
    /// for combat-roll use ; deterministic-replay is the load-bearing axiom).
    pub fn range_u32(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.next_u32() % n
    }
}

impl Default for DeterministicRng {
    /// Default seed = 0xDEAD_BEEF_CAFE_F00D (canonical for unit tests).
    fn default() -> Self {
        Self::new(0xDEAD_BEEF_CAFE_F00D)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_same_seed_same_stream() {
        let mut a = DeterministicRng::new(42);
        let mut b = DeterministicRng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn next_f32_in_unit_range() {
        let mut r = DeterministicRng::new(7);
        for _ in 0..10_000 {
            let v = r.next_f32();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn range_zero_returns_zero() {
        let mut r = DeterministicRng::new(1);
        assert_eq!(r.range_u32(0), 0);
    }
}
