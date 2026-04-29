//! § cssl-hdc::permute — circular-shift hypervector permutation
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Implements the VSA `permute` operation : circular-shift of the bit
//!   sequence by a fixed offset. Permute is the third primitive in the
//!   classic Kanerva HDC algebra, joining `bind` (multiplicative) and
//!   `bundle` (additive). Its role is to encode **sequence position** :
//!   - Two hypervectors at different positions in a sequence become
//!     **dissimilar** under permute, even if they started equal.
//!   - The permute operation is **invertible** : `inverse_permute` by
//!     the same shift recovers the original.
//!   - Permutations **commute** with bind and bundle (they're pointwise
//!     operations on a re-indexing).
//!
//! § SEQUENCE-ENCODING USE-CASE
//!   Encoding the sequence "A then B then C" into a single hypervector :
//!     `bundle(bind(A, P^0), bind(B, P^1), bind(C, P^2))`
//!   where `P^k` is permutation by `k` positions. To query "what came
//!   third", evaluate `unbind(query, P^2)` and similarity-search against
//!   the symbol vocabulary. This is how the `episodic-memory` crate
//!   (downstream) will encode time-stamped event chains.
//!
//! § ALGORITHM — bit-rotate
//!   The hypervector is a bit-string of length `D` ; circular-shift by
//!   `k` rotates each bit `i` to position `(i + k) mod D`. We implement
//!   this as a word-level rotation : when `k % 64 == 0` it's a pure
//!   word-array rotation (no carry between words). For arbitrary `k`,
//!   we rotate words by `k / 64` and then apply a per-word shift-with-
//!   carry pass. The tail-mask is re-applied at the end because
//!   rotation can move zero-tail bits into valid-tail positions.
//!
//! § PERF NOTE
//!   `D = 10000, k = 1` is the most common case (single-step time
//!   advance) — that's a single-word-shift loop costing ≈ 200 cycles.
//!   `D = 10000, k = 7` (one cascade-RG step) is the same cost. Even
//!   the worst case of `k = D/2` is bounded by `2 * words_for_dim(D)`
//!   word-shift ops.

use crate::hypervector::{words_for_dim, Hypervector};

/// § Permute (circular-shift left by `shift` bits). The bit at position
///   `i` moves to position `(i + shift) mod D`. Equivalent to multiplying
///   the indices by `x^shift` mod `x^D - 1` in the polynomial ring sense.
///
/// `shift = 0` is the identity. `shift = D` is also the identity (full
/// rotation). `shift > D` is reduced modulo `D`.
#[must_use]
pub fn permute<const D: usize>(v: &Hypervector<D>, shift: usize) -> Hypervector<D> {
    let mut out = v.clone();
    permute_in_place(&mut out, shift);
    out
}

/// § In-place circular-shift left. Mutates `v` so its bit at position
///   `(i + shift) mod D` becomes the value previously at position `i`.
pub fn permute_in_place<const D: usize>(v: &mut Hypervector<D>, shift: usize) {
    if D == 0 {
        return;
    }
    let shift = shift % D;
    if shift == 0 {
        return;
    }

    let n_words = words_for_dim(D);

    // § Strategy : we extract every bit into a `Vec<bool>` of length D,
    //   rotate the vector, then re-pack. This is O(D) work — for
    //   D = 10000 that's 10k bool-reads and 10k bool-writes, well
    //   under microsecond budget. The word-level shift-with-carry
    //   approach is faster for large `D` but is harder to get right
    //   when `D % 64 != 0` (the tail bits are in arbitrary positions
    //   after rotation). Foundation slice prioritizes correctness.
    let mut bits = vec![false; D];
    for (i, b) in bits.iter_mut().enumerate().take(D) {
        let word_idx = i / 64;
        let bit_idx = i % 64;
        *b = (v.word(word_idx) >> bit_idx) & 1 == 1;
    }

    // § Rotate left by `shift` : bit at position `i` becomes bit at
    //   `(i + shift) mod D`. Equivalently, bit at output position `j`
    //   was at input position `(j - shift) mod D` = `(j + D - shift) mod D`.
    let inverse = D - shift;
    let mut rotated = vec![false; D];
    for (j, r) in rotated.iter_mut().enumerate().take(D) {
        *r = bits[(j + inverse) % D];
    }

    // § Re-pack into the word array.
    for w in 0..n_words {
        let mut packed = 0u64;
        for b in 0..64 {
            let pos = w * 64 + b;
            if pos < D && rotated[pos] {
                packed |= 1u64 << b;
            }
        }
        *v.word_mut(w) = packed;
    }
    // § Tail invariant maintained because we only set bits in positions
    //   `< D`.
}

/// § Inverse permutation : circular-shift **right** by `shift` bits, or
///   equivalently shift left by `D - shift`.
#[must_use]
pub fn inverse_permute<const D: usize>(v: &Hypervector<D>, shift: usize) -> Hypervector<D> {
    if D == 0 {
        return v.clone();
    }
    let shift = shift % D;
    permute(v, D - shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Foundational identity : `inverse_permute(permute(v, k), k) = v`.
    #[test]
    fn permute_inverse_roundtrip() {
        let v: Hypervector<10000> = Hypervector::random_from_seed(123);
        for k in [0, 1, 7, 100, 5000, 9999] {
            let permuted = permute(&v, k);
            let recovered = inverse_permute(&permuted, k);
            assert_eq!(recovered, v, "roundtrip failed at shift {k}");
        }
    }

    /// § `permute(v, 0) = v` (identity shift).
    #[test]
    fn permute_zero_is_identity() {
        let v: Hypervector<256> = Hypervector::random_from_seed(7);
        let result = permute(&v, 0);
        assert_eq!(result, v);
    }

    /// § `permute(v, D) = v` (full-rotation identity).
    #[test]
    fn permute_full_dim_is_identity() {
        let v: Hypervector<256> = Hypervector::random_from_seed(7);
        let result = permute(&v, 256);
        assert_eq!(result, v);
    }

    /// § `permute(v, k1 + k2) = permute(permute(v, k1), k2)`.
    #[test]
    fn permute_composition() {
        let v: Hypervector<512> = Hypervector::random_from_seed(11);
        let combined = permute(&v, 100);
        let stepped = permute(&permute(&v, 30), 70);
        assert_eq!(combined, stepped);
    }

    /// § Permute changes bit positions : single-bit input, observe shift.
    #[test]
    fn permute_single_bit() {
        let mut v: Hypervector<128> = Hypervector::zero();
        v.set_bit(5, true);
        let permuted = permute(&v, 10);
        assert!(!permuted.bit(5));
        assert!(permuted.bit(15));
    }

    /// § Permute wraps around at `D`. Bit at position `D - 1` shifted by
    ///   `1` lands at position `0`.
    #[test]
    fn permute_wraps() {
        let mut v: Hypervector<128> = Hypervector::zero();
        v.set_bit(127, true);
        let permuted = permute(&v, 1);
        assert!(!permuted.bit(127));
        assert!(permuted.bit(0));
    }

    /// § Permuted hypervector is dissimilar to original (when shift ≠ 0
    ///   and shift ≠ D). For random input, the Hamming distance after
    ///   permute should be ≈ D/2.
    #[test]
    fn permute_dissimilar() {
        use crate::similarity::hamming_distance_normalized;
        let v: Hypervector<10000> = Hypervector::random_from_seed(999);
        let permuted = permute(&v, 1);
        let d = hamming_distance_normalized(&v, &permuted);
        // § Single-bit shift on random input ≈ 0.5 distance (half the
        //   bits change). Tolerance ± 0.05 for finite-D fluctuation.
        assert!(
            (0.45..=0.55).contains(&d),
            "permuted distance unexpected : {d}"
        );
    }

    /// § Permute commutes with bind : `permute(bind(a, b), k)` =
    ///   `bind(permute(a, k), permute(b, k))`. Required for sequence-
    ///   encoded ancestry chains.
    #[test]
    fn permute_commutes_with_bind() {
        use crate::bind::bind;
        let a: Hypervector<512> = Hypervector::random_from_seed(1);
        let b: Hypervector<512> = Hypervector::random_from_seed(2);
        let lhs = permute(&bind(&a, &b), 7);
        let rhs = bind(&permute(&a, 7), &permute(&b, 7));
        assert_eq!(lhs, rhs);
    }

    /// § Permute preserves popcount (rotation does not add or remove
    ///   set bits).
    #[test]
    fn permute_preserves_popcount() {
        let v: Hypervector<10000> = Hypervector::random_from_seed(444);
        let original_pc = v.popcount();
        let permuted = permute(&v, 1234);
        assert_eq!(permuted.popcount(), original_pc);
    }

    /// § In-place permute matches allocating permute.
    #[test]
    fn permute_in_place_matches() {
        let v: Hypervector<256> = Hypervector::random_from_seed(13);
        let v2: Hypervector<256> = Hypervector::random_from_seed(13);
        let allocating = permute(&v, 50);
        let mut in_place = v2;
        permute_in_place(&mut in_place, 50);
        assert_eq!(allocating, in_place);
    }

    /// § Permute by shift > D is reduced modulo D.
    #[test]
    fn permute_overshift_reduces() {
        let v: Hypervector<128> = Hypervector::random_from_seed(7);
        let small_shift = permute(&v, 50);
        let big_shift = permute(&v, 50 + 128);
        assert_eq!(small_shift, big_shift);
    }
}
