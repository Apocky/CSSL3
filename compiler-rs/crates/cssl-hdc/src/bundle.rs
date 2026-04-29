//! § cssl-hdc::bundle — popcount-majority bundling (superposition)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Implements the VSA `bundle` operation : majority-vote across N
//!   binary hypervectors. Bundle is the additive-style operator in the
//!   HDC algebra :
//!   - `bundle(a, b, c, ...)` is **similar** to each input — the
//!     bundled vector's Hamming distance to any single input is below
//!     the random-pair baseline of D/2.
//!   - `bundle` is **commutative** : order does not matter.
//!   - `bundle` is **lossy** : you cannot exactly recover any single
//!     input from the bundle, only approximate it via similarity-search.
//!   - The capacity bound (how many vectors can be bundled before the
//!     similarity drops below a usable threshold) is roughly
//!     `M ≤ √(D / log(D))` ≈ 33 for D = 10000.
//!
//! § ALGORITHM
//!   For each bit position `i` in `0..D`, count how many of the N
//!   inputs have bit `i` set. Output bit `i` is `1` iff `count > N/2`.
//!   For even `N`, ties are broken deterministically by rounding **up**
//!   (i.e. `count > N/2`, not `≥`) so that `bundle([h])` returns `h`
//!   and `bundle([h, h])` returns `h` and the operation is idempotent
//!   on repeated inputs.
//!
//! § PERF NOTE
//!   The naive implementation does `D * N` bit reads. We accelerate by
//!   processing word-by-word : for each `u64` word position, we sum
//!   `N` bit-counts using a per-bit accumulator vector of length 64.
//!   With `D = 10000, N = 16, words = 157` this is `157 * (16 * 64) =
//!   ~160k` ops — single-digit microseconds. The `bundle_with_threshold`
//!   variant lets the caller specify a custom threshold (useful for
//!   "weighted" bundles where some inputs count double).

use crate::hypervector::{words_for_dim, Hypervector};

/// § Bundle a slice of hypervectors via per-bit majority vote.
///
/// Returns the all-zero hypervector when given an empty slice ; this is
/// the "empty bundle" identity element. Single-input bundle returns the
/// input verbatim.
#[must_use]
pub fn bundle<const D: usize>(inputs: &[&Hypervector<D>]) -> Hypervector<D> {
    if inputs.is_empty() {
        return Hypervector::zero();
    }
    if inputs.len() == 1 {
        return inputs[0].clone();
    }
    // § Threshold for "majority" : `count > n / 2` ⇒ ceiling for odd N,
    //   strict-greater for even N. We use `count * 2 > n` to avoid the
    //   integer-division rounding question entirely.
    let n = inputs.len();
    bundle_with_threshold(inputs, |count| count * 2 > n)
}

/// § Bundle with a custom threshold predicate. The closure receives the
///   per-bit set-count and returns whether the output bit should be set.
///
///   Default majority is `|count| count * 2 > n` ; a "set if any" union
///   is `|count| count > 0` ; a "set if all" intersection is
///   `|count| count == n`. The threshold runs `D` times per bundle so
///   it should be cheap — prefer a closure capturing simple integers.
#[must_use]
pub fn bundle_with_threshold<const D: usize>(
    inputs: &[&Hypervector<D>],
    mut threshold: impl FnMut(usize) -> bool,
) -> Hypervector<D> {
    let n_words = words_for_dim(D);
    let mut out = vec![0u64; n_words];

    if inputs.is_empty() {
        return Hypervector::from_words(out);
    }

    // § Per-word, per-bit accumulator. We allocate a single 64-element
    //   buffer reused for every word — `D = 10000 ⇒ 157 * 64 = 10048`
    //   accumulator updates total, well under microsecond budget.
    let mut bit_counts = [0u32; 64];

    for (w, out_word) in out.iter_mut().enumerate().take(n_words) {
        bit_counts.fill(0);
        // § Sum each input's contribution to this word.
        for hv in inputs {
            let word = hv.word(w);
            for (b, count) in bit_counts.iter_mut().enumerate() {
                if (word >> b) & 1 == 1 {
                    *count += 1;
                }
            }
        }
        // § Apply the threshold per bit.
        let mut packed = 0u64;
        for (b, &count) in bit_counts.iter().enumerate() {
            if threshold(count as usize) {
                packed |= 1u64 << b;
            }
        }
        *out_word = packed;
    }

    Hypervector::from_words(out)
}

/// § Bundle from any iterator over `&Hypervector`. Allocates a `Vec<&_>`
///   internally to apply the count-then-threshold pass — exposed because
///   callers often have a `Vec<Hypervector>` and want to avoid double-
///   collecting into `Vec<&Hypervector>`.
#[must_use]
pub fn bundle_iter<'a, const D: usize, I>(iter: I) -> Hypervector<D>
where
    I: IntoIterator<Item = &'a Hypervector<D>>,
{
    let collected: Vec<&Hypervector<D>> = iter.into_iter().collect();
    bundle(&collected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::similarity::hamming_distance_normalized;

    /// § Empty-bundle is the all-zero hypervector (additive identity).
    #[test]
    fn bundle_empty_is_zero() {
        let result: Hypervector<256> = bundle(&[]);
        assert_eq!(result, Hypervector::zero());
    }

    /// § Single-input bundle is the input itself (identity).
    #[test]
    fn bundle_single_is_identity() {
        let a: Hypervector<256> = Hypervector::random_from_seed(1);
        let result = bundle(&[&a]);
        assert_eq!(result, a);
    }

    /// § Two identical inputs : majority threshold `count * 2 > 2`
    ///   means count must be > 1, i.e. count = 2 ⇒ bit set ;
    ///   count = 0 ⇒ bit clear. Result is the input verbatim.
    #[test]
    fn bundle_two_identical_is_identity() {
        let a: Hypervector<256> = Hypervector::random_from_seed(7);
        let result = bundle(&[&a, &a]);
        assert_eq!(result, a);
    }

    /// § Three inputs : true majority. Test where two of three have a
    ///   bit set ⇒ output set. One of three has bit set ⇒ output clear.
    #[test]
    fn bundle_three_majority() {
        let mut a: Hypervector<128> = Hypervector::zero();
        let mut b: Hypervector<128> = Hypervector::zero();
        let mut c: Hypervector<128> = Hypervector::zero();
        // § Bit 0 : 2 of 3 set ⇒ output set.
        a.set_bit(0, true);
        b.set_bit(0, true);
        // § Bit 1 : 1 of 3 set ⇒ output clear.
        a.set_bit(1, true);
        // § Bit 2 : 3 of 3 set ⇒ output set.
        a.set_bit(2, true);
        b.set_bit(2, true);
        c.set_bit(2, true);
        let result = bundle(&[&a, &b, &c]);
        assert!(result.bit(0));
        assert!(!result.bit(1));
        assert!(result.bit(2));
    }

    /// § Two distinct inputs (even N) : tie-break — `count * 2 > 2`
    ///   means count must be > 1. Bit set in one of two ⇒ count = 1 ⇒
    ///   output clear (so a bit set in only one of two inputs is lost).
    #[test]
    fn bundle_even_n_tie_break() {
        let mut a: Hypervector<128> = Hypervector::zero();
        let b: Hypervector<128> = Hypervector::zero();
        a.set_bit(5, true);
        let result = bundle(&[&a, &b]);
        // § count = 1, n = 2, 1*2 > 2 is false ⇒ output bit clear.
        assert!(!result.bit(5));
    }

    /// § Bundled output is **closer** to each input than two random
    ///   hypervectors are to each other. Random distance ≈ 0.5 ;
    ///   bundle-to-input distance should be < 0.45 for N ≤ √D / log(D).
    #[test]
    fn bundle_preserves_similarity() {
        let inputs: Vec<Hypervector<10000>> = (0..7)
            .map(|i| Hypervector::random_from_seed(1000 + i as u64))
            .collect();
        let refs: Vec<&Hypervector<10000>> = inputs.iter().collect();
        let bundled = bundle(&refs);

        // § Each input should be similar to the bundle.
        for (i, hv) in inputs.iter().enumerate() {
            let d = hamming_distance_normalized(&bundled, hv);
            assert!(
                d < 0.45,
                "input {i} too far from bundle : d = {d} (expected < 0.45)"
            );
        }

        // § Random baseline check : two random hypervectors are at
        //   distance ≈ 0.5.
        let r1: Hypervector<10000> = Hypervector::random_from_seed(99001);
        let r2: Hypervector<10000> = Hypervector::random_from_seed(99002);
        let baseline = hamming_distance_normalized(&r1, &r2);
        assert!(
            (0.45..=0.55).contains(&baseline),
            "random baseline drift : {baseline}"
        );
    }

    /// § Bundle-iter equivalent to bundle(&refs).
    #[test]
    fn bundle_iter_matches() {
        let inputs: Vec<Hypervector<256>> = (0..5)
            .map(|i| Hypervector::random_from_seed(i as u64))
            .collect();
        let refs: Vec<&Hypervector<256>> = inputs.iter().collect();
        let direct = bundle(&refs);
        let via_iter: Hypervector<256> = bundle_iter(inputs.iter());
        assert_eq!(direct, via_iter);
    }

    /// § Custom threshold "set if any" produces an OR of all inputs.
    #[test]
    fn bundle_threshold_or() {
        let mut a: Hypervector<128> = Hypervector::zero();
        let mut b: Hypervector<128> = Hypervector::zero();
        a.set_bit(3, true);
        b.set_bit(7, true);
        let or_result = bundle_with_threshold(&[&a, &b], |c| c > 0);
        assert!(or_result.bit(3));
        assert!(or_result.bit(7));
    }

    /// § Custom threshold "set if all" produces an AND.
    #[test]
    fn bundle_threshold_and() {
        let mut a: Hypervector<128> = Hypervector::zero();
        let mut b: Hypervector<128> = Hypervector::zero();
        a.set_bit(3, true);
        a.set_bit(7, true);
        b.set_bit(7, true);
        let and_result = bundle_with_threshold(&[&a, &b], |c| c == 2);
        assert!(!and_result.bit(3)); // only in a
        assert!(and_result.bit(7)); // in both
    }

    /// § Bundle preserves tail-zero invariant.
    #[test]
    fn bundle_preserves_tail() {
        let a: Hypervector<100> = Hypervector::random_from_seed(1);
        let b: Hypervector<100> = Hypervector::random_from_seed(2);
        let c: Hypervector<100> = Hypervector::random_from_seed(3);
        let result = bundle(&[&a, &b, &c]);
        assert_eq!(
            result.word(1) & !((1u64 << 36) - 1),
            0,
            "tail bits leaked"
        );
    }

    /// § Capacity test : bundle 33 random hypervectors at D = 10000.
    ///   Each individual input should still be retrievable above noise
    ///   (similarity > 0.5 in the cosine-bipolar form).
    #[test]
    fn bundle_capacity_sqrt_d() {
        use crate::similarity::similarity_bipolar;
        let inputs: Vec<Hypervector<10000>> = (0..33)
            .map(|i| Hypervector::random_from_seed(2000 + i as u64))
            .collect();
        let refs: Vec<&Hypervector<10000>> = inputs.iter().collect();
        let bundled = bundle(&refs);
        // § At capacity, similarity to each input is roughly
        //   1/√M ≈ 0.17 — above the random-noise floor of 0 ± few-σ.
        for hv in &inputs {
            let s = similarity_bipolar(&bundled, hv);
            assert!(
                s > 0.05,
                "capacity exceeded : input similarity {s} below threshold"
            );
        }
    }
}
