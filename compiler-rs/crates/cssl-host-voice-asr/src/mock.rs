//! Stage-0 mock-mode phoneme-stream generator.
//!
//! § ROLE
//!   Always-works · platform-independent · deterministic-replay
//!   phoneme-stream synthesizer. NOT a real ASR · the generated stream
//!   is intentionally NOT-meaningful-as-English — it is a STABLE
//!   per-(cap_token · handle-counter) phoneme-bytes sequence that the
//!   downstream HDC-bind / KAN-classify stages can be exercised against.
//!
//! § DETERMINISM
//!   The generator is a pure-fn of (`cap_token`, `handle`). It uses an
//!   xorshift32 hash-mixer to spread input-bits across the output stream
//!   so distinct inputs yield distinguishable streams (validated by
//!   `different_input_different_stream` in lib.rs tests).
//!
//! § PROPRIETARY-SUBSTRATE alignment
//!   This generator is a STAGE-0 BOOTSTRAP-SHIM — when csslc supports
//!   the substrate-side phoneme-extraction primitives (cssl-substrate-
//!   audio + cssl-hdc), this file is replaced by a thin call into
//!   substrate primitives. Until then, mock generates a substrate-style
//!   stream-of-bytes that the rest of the pipeline can lock onto.

#![forbid(unsafe_code)]

use crate::{MAX_PHONEMES_PER_CAPTURE, PHONEME_VOCAB_SIZE};

/// Generate a deterministic phoneme-stream for the given (cap_token, handle).
/// Stream length is bounded by [`MAX_PHONEMES_PER_CAPTURE`]. Each phoneme
/// is in 0..[`PHONEME_VOCAB_SIZE`].
pub fn generate_phoneme_stream(cap_token: u32, handle: u32) -> Vec<u32> {
    let mut state = mix_seed(cap_token, handle);
    // Stream length = 8..=64 phonemes ; pseudo-random within bounds.
    // Bounds chosen to keep tests fast while exercising both small + medium.
    let len = 8 + (state.wrapping_mul(0x9E37_79B9) >> 26) % 57; // ≈ 8..=64
    let len = len.min(MAX_PHONEMES_PER_CAPTURE);

    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        state = xorshift32(state ^ rotate_left(i, i & 31));
        let p = state % PHONEME_VOCAB_SIZE;
        out.push(p);
    }
    out
}

/// Confidence-percent for the given (cap_token, handle).
/// Returns a deterministic value in 60..=95 (mock is "fairly-confident"
/// since synthesized streams have no transcription error).
/// Excludes 100 to model "even-perfect-mic-has-some-uncertainty".
pub fn confidence_for(cap_token: u32, handle: u32) -> u32 {
    let s = mix_seed(cap_token.wrapping_add(0x1234_5678), handle);
    60 + (s % 36) // 60..=95
}

// ─────────────────────────────────────────────────────────────────────
// § hash-mixer · xorshift32 + 32-bit avalanche
// ─────────────────────────────────────────────────────────────────────
//
// Why xorshift32 not BLAKE3 here :
//   - Tiny + dependency-free + deterministic + sufficient-spread for
//     stage-0 mock. Cryptographic strength is NOT a property we need ;
//     downstream HDC-bind is what gives semantic robustness.
//   - The wider proprietary-substrate uses BLAKE3 + Ed25519 for content-
//     addressing per spec § DETERMINISM AUDIT. A future slice may
//     migrate this mock to BLAKE3-truncate-u32 to share canonicalization
//     with the rest of the host ; behavior-equivalent · purely-internal.

fn mix_seed(a: u32, b: u32) -> u32 {
    // splitmix-32 style mixer ; ensures (0,0) doesn't trap at zero.
    let mut x = a.wrapping_add(0x9E37_79B9).wrapping_mul(0x85EB_CA6B);
    x ^= b.rotate_left(13);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    if x == 0 {
        0xDEAD_BEEF
    } else {
        x
    }
}

fn xorshift32(mut x: u32) -> u32 {
    if x == 0 {
        x = 0xDEAD_BEEF;
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

fn rotate_left(x: u32, k: u32) -> u32 {
    x.rotate_left(k & 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_bounded_and_in_vocab() {
        for (a, b) in [(1u32, 1u32), (42, 7), (0xFFFF_FFFF, 1), (0, 1)] {
            let s = generate_phoneme_stream(a, b);
            assert!(s.len() >= 8 && s.len() <= 64);
            assert!(s.len() as u32 <= MAX_PHONEMES_PER_CAPTURE);
            for &p in &s {
                assert!(p < PHONEME_VOCAB_SIZE);
            }
        }
    }

    #[test]
    fn confidence_in_bounds() {
        for seed in 0..50u32 {
            for handle in 1..5u32 {
                let c = confidence_for(seed, handle);
                assert!((60..=95).contains(&c));
            }
        }
    }

    #[test]
    fn pure_fn_property() {
        // Same input → same output, every call.
        let a = generate_phoneme_stream(7, 3);
        let b = generate_phoneme_stream(7, 3);
        assert_eq!(a, b);
        let c = confidence_for(7, 3);
        let d = confidence_for(7, 3);
        assert_eq!(c, d);
    }
}
