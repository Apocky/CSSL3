// § seed.rs — splitmix64 deterministic RNG (no std::rand · no external dep)
// ════════════════════════════════════════════════════════════════════
// § I> per W13-5 : fps-feel-tick = seeded-RNG · replay-bit-equal across hosts
// § I> splitmix64 chosen ∵ : tiny · stateless-after-seed · Copy · canonical
//      reference impl (Vigna 2014 ; public-domain) ; matches sibling
//      cssl-host-weapons::seed + cssl-host-combat-sim::seed semantics for
//      cross-crate determinism so a single replay-manifest header can drive
//      ALL three crates with one seed.
// § I> NaN-safe : next_f32 returns ∈ [0, 1) ; bit-equal across IEEE-754 hosts
// § I> NO unsafe ; NO panic
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Deterministic splitmix64-based RNG — replay-bit-equal across hosts.
///
/// Public surface kept Copy-friendly so callers can snapshot the RNG state
/// inside an `FpsFeelTick` checkpoint and resume later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    /// New RNG from an explicit seed.
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

    /// Centered-uniform : returns ∈ [-1.0, 1.0).
    pub fn next_f32_centered(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_equal_for_same_seed() {
        let mut a = DeterministicRng::new(0xCAFE_F00D);
        let mut b = DeterministicRng::new(0xCAFE_F00D);
        for _ in 0..256 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn next_f32_in_unit_interval() {
        let mut r = DeterministicRng::new(42);
        for _ in 0..1024 {
            let v = r.next_f32();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn centered_in_signed_unit() {
        let mut r = DeterministicRng::new(7);
        for _ in 0..1024 {
            let v = r.next_f32_centered();
            assert!((-1.0..1.0).contains(&v));
        }
    }
}
