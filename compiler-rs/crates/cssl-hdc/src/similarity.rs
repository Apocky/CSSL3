//! § cssl-hdc::similarity — Hamming / cosine / dot-product metrics
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The metrics surface for hypervector comparison. The principal
//!   metric in HDC is **Hamming distance** — number of bit positions
//!   where two hypervectors differ. From Hamming we derive the bipolar
//!   **dot-product** (`D - 2·hamming`) and the **cosine similarity**
//!   (dot / D for unit-bipolar vectors). These three are equivalent up
//!   to affine transform but each has its place :
//!   - **Hamming** : raw count, integer-valued, fastest to compute
//!     (one popcount per word).
//!   - **Normalized Hamming** : `hamming / D ∈ [0, 1]`. Random pair
//!     converges to `0.5` ; identical pair gives `0.0`.
//!   - **Bipolar similarity** : `1 - 2·hamming/D ∈ [-1, +1]`. Random
//!     pair gives `0`. Most natural for "is this similar" predicates
//!     because the threshold is intuitive.
//!   - **Cosine** (binary) : `(2·popcount(a AND b) - popcount(a) -
//!     popcount(b)) / sqrt(popcount(a) · popcount(b))` for the binary
//!     `{0, 1}` representation. Less commonly used but provided for
//!     callers that want "geometric" similarity rather than bipolar.
//!   - **Dot-product** : raw `popcount(a) - 2·hamming` ∈ `[-D, +D]`.
//!     Useful as a denominator-free signal when the consumer is going
//!     to apply its own normalization downstream.
//!
//! § PERF NOTE
//!   The hot path is `popcount(a XOR b)` summed over words. The SIMD
//!   module dispatches to AVX2's `vpopcntq` when available ; the
//!   fallback uses Rust's `u64::count_ones` which most compilers
//!   lower to `popcnt` on SSE4.2 builds. The fallback is what
//!   `hamming_distance` uses ; the SIMD path lives in `simd.rs` and
//!   is opt-in via [`crate::simd::popcount_xor_slice`].

use crate::hypervector::Hypervector;

/// § Compute the integer Hamming distance between two hypervectors :
///   the number of bit positions where they differ.
///
/// `a, b` must be the same `D` (enforced at the type level). The
/// computation is `sum(popcount(a[i] XOR b[i])) for i in 0..n_words`.
/// The tail-zero invariant guarantees the result is exactly the count
/// of differing valid bits — no spurious tail contribution.
#[must_use]
pub fn hamming_distance<const D: usize>(a: &Hypervector<D>, b: &Hypervector<D>) -> u32 {
    let n = a.word_count();
    let mut sum = 0u32;
    for i in 0..n {
        sum += (a.word(i) ^ b.word(i)).count_ones();
    }
    sum
}

/// § Hamming distance normalized to `[0, 1]` : `hamming / D`. Useful
///   for similarity-threshold checks where the meaningful threshold is
///   a fraction (e.g. "below 0.4 is similar enough"). Returns `0.0`
///   when `D = 0`.
#[must_use]
pub fn hamming_distance_normalized<const D: usize>(
    a: &Hypervector<D>,
    b: &Hypervector<D>,
) -> f32 {
    if D == 0 {
        return 0.0;
    }
    let h = hamming_distance(a, b) as f32;
    h / (D as f32)
}

/// § Bipolar dot-product : the inner product of `a, b` viewed as
///   `{-1, +1}` vectors. Returns `D - 2·hamming` as `i32`. Range is
///   `[-D, +D]`. `+D` for identical vectors ; `-D` for opposite ;
///   `0` for random pair.
#[must_use]
pub fn dot_product<const D: usize>(a: &Hypervector<D>, b: &Hypervector<D>) -> i32 {
    let h = hamming_distance(a, b) as i32;
    (D as i32) - 2 * h
}

/// § Bipolar similarity normalized to `[-1, +1]` : `(D - 2·hamming) / D`.
///   This is the canonical "are they similar" metric. Threshold of
///   `0.0` is the random-baseline floor ; values approaching `+1.0`
///   indicate near-identity and values approaching `-1.0` indicate
///   anti-correlation.
#[must_use]
pub fn similarity_bipolar<const D: usize>(a: &Hypervector<D>, b: &Hypervector<D>) -> f32 {
    if D == 0 {
        return 0.0;
    }
    let dot = dot_product(a, b) as f32;
    dot / (D as f32)
}

/// § Cosine similarity in the binary `{0, 1}` representation :
///   `<a, b> / (||a|| · ||b||)` where `<a, b> = popcount(a AND b)` and
///   `||a|| = sqrt(popcount(a))`.
///
/// Returns `0.0` if either input has zero popcount (the all-zero
/// hypervector has no defined direction). Range is `[0, 1]` for
/// non-empty inputs.
///
/// Note : binary cosine is **different** from bipolar similarity. Use
/// [`similarity_bipolar`] for the bipolar interpretation. Cosine on
/// binary is appropriate when the hypervectors represent **sets** (a
/// bit is "present in the set" iff `1`) rather than bipolar signals.
#[must_use]
pub fn cosine_similarity<const D: usize>(a: &Hypervector<D>, b: &Hypervector<D>) -> f32 {
    let pc_a = a.popcount() as f32;
    let pc_b = b.popcount() as f32;
    if pc_a == 0.0 || pc_b == 0.0 {
        return 0.0;
    }
    let n = a.word_count();
    let mut intersect = 0u32;
    for i in 0..n {
        intersect += (a.word(i) & b.word(i)).count_ones();
    }
    (intersect as f32) / (pc_a.sqrt() * pc_b.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Identical hypervectors have Hamming distance 0.
    #[test]
    fn hamming_self_zero() {
        let a: Hypervector<10000> = Hypervector::random_from_seed(1);
        assert_eq!(hamming_distance(&a, &a), 0);
        assert_eq!(hamming_distance_normalized(&a, &a), 0.0);
        assert_eq!(similarity_bipolar(&a, &a), 1.0);
    }

    /// § Random pair Hamming distance ≈ D/2 ; normalized ≈ 0.5 ;
    ///   bipolar similarity ≈ 0.0.
    #[test]
    fn hamming_random_pair_baseline() {
        let a: Hypervector<10000> = Hypervector::random_from_seed(1);
        let b: Hypervector<10000> = Hypervector::random_from_seed(2);
        let h = hamming_distance(&a, &b);
        let n = hamming_distance_normalized(&a, &b);
        let s = similarity_bipolar(&a, &b);
        assert!((4500..=5500).contains(&h), "h = {h}");
        assert!((0.45..=0.55).contains(&n), "n = {n}");
        assert!((-0.1..=0.1).contains(&s), "s = {s}");
    }

    /// § Anti-correlated pair : `bind(a, ones)` flips every bit ⇒
    ///   Hamming = D, similarity = -1.
    #[test]
    fn hamming_anti_correlated() {
        use crate::bind::bind;
        let a: Hypervector<10000> = Hypervector::random_from_seed(7);
        let ones: Hypervector<10000> = Hypervector::ones();
        let neg_a = bind(&a, &ones);
        assert_eq!(hamming_distance(&a, &neg_a), 10000);
        assert_eq!(similarity_bipolar(&a, &neg_a), -1.0);
    }

    /// § Dot-product range : identical → +D ; anti → -D ; random → 0.
    #[test]
    fn dot_product_range() {
        let a: Hypervector<10000> = Hypervector::random_from_seed(11);
        let b: Hypervector<10000> = Hypervector::random_from_seed(13);
        assert_eq!(dot_product(&a, &a), 10000);
        let dp = dot_product(&a, &b);
        assert!((-1000..=1000).contains(&dp), "dot = {dp}");
    }

    /// § Hamming triangle inequality : `h(a, c) ≤ h(a, b) + h(b, c)`.
    #[test]
    fn hamming_triangle_inequality() {
        let a: Hypervector<256> = Hypervector::random_from_seed(1);
        let b: Hypervector<256> = Hypervector::random_from_seed(2);
        let c: Hypervector<256> = Hypervector::random_from_seed(3);
        let h_ab = hamming_distance(&a, &b);
        let h_bc = hamming_distance(&b, &c);
        let h_ac = hamming_distance(&a, &c);
        assert!(
            h_ac <= h_ab + h_bc,
            "triangle violated : {h_ac} > {h_ab} + {h_bc}"
        );
    }

    /// § Hamming symmetry : `h(a, b) = h(b, a)`.
    #[test]
    fn hamming_symmetric() {
        let a: Hypervector<512> = Hypervector::random_from_seed(101);
        let b: Hypervector<512> = Hypervector::random_from_seed(202);
        assert_eq!(hamming_distance(&a, &b), hamming_distance(&b, &a));
    }

    /// § Cosine of identical hypervectors is 1 (assuming non-zero).
    #[test]
    fn cosine_self_is_one() {
        let a: Hypervector<512> = Hypervector::random_from_seed(0xCC);
        let s = cosine_similarity(&a, &a);
        assert!((s - 1.0).abs() < 1e-6, "cosine self = {s}");
    }

    /// § Cosine with all-zero is 0.
    #[test]
    fn cosine_with_zero_is_zero() {
        let a: Hypervector<512> = Hypervector::random_from_seed(7);
        let z: Hypervector<512> = Hypervector::zero();
        assert_eq!(cosine_similarity(&a, &z), 0.0);
        assert_eq!(cosine_similarity(&z, &a), 0.0);
    }

    /// § Cosine range bounded `[0, 1]` for binary inputs.
    #[test]
    fn cosine_range() {
        let a: Hypervector<512> = Hypervector::random_from_seed(1);
        let b: Hypervector<512> = Hypervector::random_from_seed(2);
        let s = cosine_similarity(&a, &b);
        assert!((0.0..=1.0).contains(&s));
    }

    /// § Hamming + bipolar similarity : `h = (1 - s) * D / 2`.
    #[test]
    fn hamming_similarity_relation() {
        let a: Hypervector<10000> = Hypervector::random_from_seed(101);
        let b: Hypervector<10000> = Hypervector::random_from_seed(202);
        let h = hamming_distance(&a, &b) as f32;
        let s = similarity_bipolar(&a, &b);
        let expected_h = (1.0 - s) * 10000.0 / 2.0;
        assert!((h - expected_h).abs() < 1.0, "h = {h} ; expected ≈ {expected_h}");
    }

    /// § Zero-dim degenerate cases.
    #[test]
    fn zero_dim_metrics() {
        let a: Hypervector<0> = Hypervector::zero();
        let b: Hypervector<0> = Hypervector::zero();
        assert_eq!(hamming_distance(&a, &b), 0);
        assert_eq!(hamming_distance_normalized(&a, &b), 0.0);
        assert_eq!(similarity_bipolar(&a, &b), 0.0);
    }
}
