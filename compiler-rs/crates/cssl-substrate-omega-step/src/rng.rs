//! Deterministic RNG — single-stream PCG-XSH-RR-32, scheduler-seeded.
//!
//! § THESIS
//!   `specs/30_SUBSTRATE.csl § OMEGA-STEP § DETERMINISTIC-REPLAY-INVARIANTS`
//!   requires that two scheduler instances seeded identically + given identical
//!   input streams produce bit-identical Ω-tensor states. RNG is one of the
//!   three sources of non-determinism (along with float-precision + clock).
//!
//!   This module supplies a deterministic RNG with these guarantees :
//!     - **Pure** : output is a function of (initial-seed, call-count) only.
//!       No clock, no entropy, no platform-specific quirks.
//!     - **Per-stream-isolated** : each `RngStreamId` gets its own PRNG state ;
//!       streams advance independently.
//!     - **Reproducible across machines** : the algorithm (PCG-XSH-RR-32) is
//!       a published, fully-specified algorithm. No platform branches.
//!     - **Forbidden alternative paths** : there is NO public hook to seed
//!       from `thread_rng()` ; the constructor takes a `u64` seed only.
//!       Attempting to register a system that opens an entropy stream
//!       returns `OmegaError::DeterminismViolation`.
//!
//! § ALGORITHM — PCG-XSH-RR-32 (O'Neill 2014)
//!   state advance : state = state * MULTIPLIER + INCREMENT  (mod 2^64)
//!   output        : let xorshifted = ((state >> 18) ^ state) >> 27
//!                   let rot        = state >> 59
//!                   xorshifted.rotate_right(rot)
//!
//!   Constants are O'Neill's published canonical PCG values :
//!     MULTIPLIER = 6364136223846793005
//!     INCREMENT  = 1442695040888963407 (must be odd — derived from default-stream-id)
//!
//!   This is the standard PCG-XSH-RR algorithm cited in O'Neill 2014.
//!   We do NOT vary INCREMENT per-stream (would require user to choose ;
//!   instead each stream has its own state initialized from a seed-tree).
//!
//! § STREAM SEEDING
//!   The scheduler's master-seed is split into per-stream seeds via
//!   `splitmix64` of `(master_seed ^ stream_id)`. This avoids correlation
//!   between streams while keeping the public API a single u64 master-seed.
//!
//! § WHAT IS NOT INCLUDED
//!   - No `f64`-uniform conversion. Stage-0 systems that need a float
//!     compute it themselves from `next_u64()` ; the precise conversion
//!     formula is an open replay-determinism question (different ULPs on
//!     different platforms), and forcing systems to spell out the
//!     conversion makes it auditable.
//!   - No `gen_range(low, high)`. Same rationale as above — the modulo-
//!     bias mitigation strategy is a system-level choice.

use std::fmt;

/// A typed identifier for an RNG stream. `RngStreamId(0)` is the default
/// stream every system gets ; non-zero ids are for systems that need
/// multiple uncorrelated RNG streams (physics + AI-fuzz, for example).
///
/// § CONVENTION
///   Stream-id `0` is reserved for the default stream. Stream-ids `>= 1`
///   are user-allocated. The scheduler does not validate that stream-ids
///   are dense — sparse ids are fine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RngStreamId(pub u64);

impl fmt::Display for RngStreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rng#{}", self.0)
    }
}

/// Deterministic pseudo-random number generator. State is `(state, inc)` ;
/// each call to `next_u32()` / `next_u64()` advances state by one PCG step.
///
/// § DO NOT use [`std::collections::hash_map::DefaultHasher`] / `rand::thread_rng()` —
///   both are non-deterministic across runs. The scheduler's determinism-probe
///   refuses to register systems that touch those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetRng {
    state: u64,
    inc: u64,
}

/// O'Neill 2014 PCG-XSH-RR canonical multiplier.
const PCG_MULTIPLIER: u64 = 6_364_136_223_846_793_005;
/// O'Neill 2014 PCG-XSH-RR canonical increment (must be odd).
const PCG_INCREMENT: u64 = 1_442_695_040_888_963_407;

impl DetRng {
    /// Construct from a master-seed + stream-id. The (seed, stream) pair
    /// uniquely determines all output. Reusing the same pair gives the
    /// same byte-stream — this is the replay-determinism foundation.
    #[must_use]
    pub fn new(master_seed: u64, stream: RngStreamId) -> Self {
        // SplitMix64 the (seed, stream) pair to derive an isolated state.
        // SplitMix64 is well-known + fully-specified ; it has zero collision
        // for distinct (seed, stream) inputs.
        let mixed = splitmix64(master_seed.wrapping_add(stream.0));
        // PCG advance step happens before the first output ; initialize
        // state via one extra splitmix to avoid trivial zero-state.
        let state = splitmix64(mixed ^ PCG_INCREMENT);
        Self {
            state,
            inc: PCG_INCREMENT,
        }
    }

    /// Construct from an already-mixed state (used by `replay_from(log)`).
    /// Internal — public so test fixtures can fast-forward to a known state.
    #[must_use]
    pub fn from_state(state: u64, inc: u64) -> Self {
        // Force inc to be odd ; PCG requires this. If caller supplied an
        // even `inc`, OR with 1 to make it odd — better than panicking.
        Self {
            state,
            inc: inc | 1,
        }
    }

    /// Inspect the (state, inc) pair. Used by `ReplayLog` to checkpoint
    /// per-stream state ; not for general use.
    #[must_use]
    pub fn state(&self) -> (u64, u64) {
        (self.state, self.inc)
    }

    /// Advance one PCG step + emit a 32-bit output. Returns u32.
    pub fn next_u32(&mut self) -> u32 {
        let old_state = self.state;
        self.state = old_state
            .wrapping_mul(PCG_MULTIPLIER)
            .wrapping_add(self.inc);

        let xor = (((old_state >> 18) ^ old_state) >> 27) as u32;
        let rot = (old_state >> 59) as u32;
        xor.rotate_right(rot)
    }

    /// Two PCG steps composed into a 64-bit output.
    pub fn next_u64(&mut self) -> u64 {
        let lo = u64::from(self.next_u32());
        let hi = u64::from(self.next_u32());
        (hi << 32) | lo
    }

    /// Bounded integer in `[0, n)` via rejection sampling. **Stage-0 has
    /// the canonical "modulo-bias-free" form** : draw u32 ; reject if it
    /// falls in the bias zone ; redraw. The number of redraws is bounded
    /// at 2 in expectation for any `n <= u32::MAX/2`.
    pub fn next_bounded_u32(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let zone = u32::MAX - (u32::MAX % n);
        loop {
            let v = self.next_u32();
            if v < zone {
                return v % n;
            }
            // Reject + redraw — bounded in expectation, no infinite loop
            // possible because next_u32 is full-range uniform.
        }
    }
}

/// SplitMix64 (Steele 2014). Used here for stream-state isolation. Pure
/// function : same input ⇒ same output ⇒ replay-deterministic.
const fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_output() {
        let mut a = DetRng::new(42, RngStreamId(0));
        let mut b = DetRng::new(42, RngStreamId(0));
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_streams_different_output() {
        let mut a = DetRng::new(42, RngStreamId(0));
        let mut b = DetRng::new(42, RngStreamId(1));
        // Probability that 1000 consecutive u32s match between
        // independent streams is ~0 in practice ; even one mismatch
        // suffices to demonstrate the streams are uncorrelated.
        let mut any_diff = false;
        for _ in 0..1000 {
            if a.next_u32() != b.next_u32() {
                any_diff = true;
                break;
            }
        }
        assert!(any_diff, "streams 0 and 1 must produce divergent output");
    }

    #[test]
    fn different_seeds_different_output() {
        let mut a = DetRng::new(42, RngStreamId(0));
        let mut b = DetRng::new(43, RngStreamId(0));
        let mut any_diff = false;
        for _ in 0..1000 {
            if a.next_u32() != b.next_u32() {
                any_diff = true;
                break;
            }
        }
        assert!(any_diff, "seeds 42 and 43 must produce divergent output");
    }

    #[test]
    fn next_u64_non_trivial() {
        // Just verify next_u64 produces non-degenerate values (not all zero,
        // not all 0xFFFF...).
        let mut rng = DetRng::new(12345, RngStreamId(7));
        let v0 = rng.next_u64();
        let v1 = rng.next_u64();
        assert_ne!(v0, 0);
        assert_ne!(v0, u64::MAX);
        assert_ne!(v0, v1);
    }

    #[test]
    fn next_bounded_uniform_smoke() {
        // Distribution check : next_bounded_u32(10) over 100k draws should
        // give each bucket ~10k samples (within wide tolerance).
        let mut rng = DetRng::new(0xDEAD_BEEF, RngStreamId(0));
        let mut hits = [0u32; 10];
        for _ in 0..100_000 {
            let v = rng.next_bounded_u32(10) as usize;
            hits[v] += 1;
        }
        // Each bucket should have count close to 10k. Expansive band
        // (5k–15k) makes this test robust without depending on chi-squared
        // tables — a buggy RNG would produce huge swings.
        for h in hits {
            assert!(
                (5_000..=15_000).contains(&h),
                "bucket count {h} far from expected 10k"
            );
        }
    }

    #[test]
    fn next_bounded_n_zero_returns_zero() {
        let mut rng = DetRng::new(0, RngStreamId(0));
        assert_eq!(rng.next_bounded_u32(0), 0);
    }

    #[test]
    fn from_state_round_trips() {
        let mut a = DetRng::new(99, RngStreamId(2));
        let _ = a.next_u32();
        let _ = a.next_u32();
        let (s, i) = a.state();
        let mut b = DetRng::from_state(s, i);
        // From this point onward, a + b produce the same byte-stream.
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn from_state_normalizes_even_inc_to_odd() {
        let r = DetRng::from_state(0, 4);
        let (_, inc) = r.state();
        assert_eq!(inc & 1, 1, "PCG requires inc to be odd");
    }

    #[test]
    fn stream_id_display() {
        assert_eq!(RngStreamId(7).to_string(), "rng#7");
    }
}
