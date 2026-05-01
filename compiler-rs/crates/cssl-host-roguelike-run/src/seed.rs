// § seed-pinning ← GDDs/ROGUELIKE_LOOP.csl §SEED-PINNING
// ════════════════════════════════════════════════════════════════════
// § I> seed = (player-id-hash ⊕ run-id-counter) : u128
// § I> every procgen-call uses seed-derived-RNG → deterministic + replay-bit-equal
// § I> seed-mutation forbidden mid-run ; re-seed only at new-run-genesis
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// § Pin a run-seed from player-id-hash ⊕ run-counter.
///
/// Layout : high-64-bits = player-id-hash · low-64-bits = run-id-counter.
/// XOR-fold then domain-mix via SplitMix64 step to disperse bits — guards
/// against linearly-correlated seeds when both inputs change in lockstep.
pub fn pin_seed(player_id_hash: u64, run_counter: u64) -> u128 {
    // Mix each half independently with SplitMix64 finalizer, then concat.
    let hi = splitmix64(player_id_hash ^ ROGUELIKE_DOMAIN_TAG_HI);
    let lo = splitmix64(run_counter ^ ROGUELIKE_DOMAIN_TAG_LO);
    ((hi as u128) << 64) | (lo as u128)
}

/// Domain-tag constants for seed derivation. Distinct random-looking
/// nonces ensure player-id-hash=0 ∧ run-counter=0 still produces a
/// non-zero seed (avoids degenerate all-zero RNG state).
const ROGUELIKE_DOMAIN_TAG_HI: u64 = 0x9E37_79B9_7F4A_7C15; // golden-ratio-frac
const ROGUELIKE_DOMAIN_TAG_LO: u64 = 0xBF58_476D_1CE4_E5B9; // SplitMix mix-1

/// SplitMix64 finalizer — well-known bit-mixer · deterministic · branch-free.
const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// § Deterministic RNG (xorshift128+ derived from a u128 run-seed).
///
/// Streamable : `next_u64` advances state by one mix-step. Bit-equal
/// across CPU/platform/build because we use only `wrapping_*` u64 ops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetRng {
    /// State-high (mutable) — survives serialization for replay.
    pub state_hi: u64,
    /// State-low (mutable).
    pub state_lo: u64,
    /// Original seed pinned at genesis (immutable post-construction).
    pub seed: u128,
    /// Step-counter — number of `next_u64` advances ; for audit/attestation.
    pub steps: u64,
}

impl DetRng {
    /// Construct from a pinned u128 seed.
    pub fn from_seed(seed: u128) -> Self {
        let hi = (seed >> 64) as u64;
        let lo = seed as u64;
        // Avoid all-zero state (degenerate xorshift).
        let safe_hi = if hi == 0 { 0xDEAD_BEEF_CAFE_F00D } else { hi };
        let safe_lo = if lo == 0 { 0x1234_5678_9ABC_DEF0 } else { lo };
        Self {
            state_hi: safe_hi,
            state_lo: safe_lo,
            seed,
            steps: 0,
        }
    }

    /// Advance and return the next u64.
    ///
    /// xorshift128+ step (Vigna 2014) — passes BigCrush statistical battery,
    /// suitable for procgen (¬ cryptographic — caller-attested non-secret).
    pub fn next_u64(&mut self) -> u64 {
        let mut s1 = self.state_hi;
        let s0 = self.state_lo;
        let result = s0.wrapping_add(s1);
        s1 ^= s1 << 23;
        self.state_hi = s1 ^ s0 ^ (s1 >> 18) ^ (s0 >> 5);
        self.state_lo = s0;
        // Swap so next call reads in the conventional xorshift128+ order.
        std::mem::swap(&mut self.state_hi, &mut self.state_lo);
        self.steps = self.steps.saturating_add(1);
        result
    }
}

/// Derive a u32 from the pinned seed at the given call-site index.
///
/// `call_site` should be a stable enum-discriminant or compile-time hash
/// representing the procgen-callsite (room-layout · loot-roll · NPC-spawn).
/// Two distinct call-sites with the same seed produce uncorrelated u32s.
pub fn derive_rng_u32(seed: u128, call_site: u64) -> u32 {
    let mixed = splitmix64((seed as u64) ^ call_site)
        ^ splitmix64(((seed >> 64) as u64).wrapping_add(call_site));
    (mixed >> 32) as u32
}

/// Derive a u64 from the pinned seed at the given call-site index.
pub fn derive_rng_u64(seed: u128, call_site: u64) -> u64 {
    let lo = splitmix64((seed as u64) ^ call_site);
    let hi = splitmix64(((seed >> 64) as u64) ^ call_site.wrapping_add(1));
    lo ^ hi.rotate_left(17)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_seed_zero_inputs_nonzero_output() {
        let s = pin_seed(0, 0);
        assert_ne!(s, 0);
    }

    #[test]
    fn det_rng_replay_bit_equal() {
        let seed = pin_seed(0xCAFE, 7);
        let mut a = DetRng::from_seed(seed);
        let mut b = DetRng::from_seed(seed);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn derive_callsite_uncorrelated() {
        let seed = pin_seed(0xDEADBEEF, 42);
        let a = derive_rng_u64(seed, 1);
        let b = derive_rng_u64(seed, 2);
        assert_ne!(a, b);
    }
}
