//! § cssl-hdc::bind — pointwise XOR binding for binary hypervectors
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Implements the VSA `bind` operation : pointwise XOR for the binary
//!   form, equivalent to pointwise multiplication on the bipolar form.
//!   Bind is the multiplicative-style operator in the HDC algebra :
//!   - `bind(a, b)` is **dissimilar** to both `a` and `b` — the bound
//!     vector lives in a "different region" of hyperspace.
//!   - `bind` is **invertible** : `unbind(bind(a, b), b) = a` exactly.
//!   - `bind` is **commutative** for binary XOR : `bind(a, b) = bind(b, a)`.
//!   - `bind` distributes over `bundle` (approximately) : binding a
//!     superposition is the same as superposing the per-component bindings.
//!
//! § GENOME USE-CASE
//!   `06_PROCEDURAL/02_CREATURES_FROM_GENOME.csl.md § II` declares an
//!   `ancestry_chain: SmallVec<Handle<CreatureGenome>, 8>` that is
//!   HDC-bound — the chain is a sequence of bind operations
//!   `bind(parent_n, bind(parent_{n-1}, ..., bind(parent_1, root)))` so
//!   given any single ancestor's hypervector, the rest of the chain can
//!   be peeled off via `unbind`. Chain length 8 is well within the
//!   capacity bound (≈ √D = 100 for D = 10000).
//!
//! § HOLOGRAM USE-CASE
//!   The classic Kanerva "key-value store as a hypervector" works by
//!   bundling many `bind(key_i, value_i)` pairs together. Querying with
//!   `unbind(bundle, key_j)` recovers `value_j` plus noise from the
//!   other M-1 bindings ; the noise is below the signal threshold for
//!   `M ≤ √D / log(D)` ≈ 50 entries at D = 10000.
//!
//! § PERF NOTE
//!   The hot loop is `dst[i] = a[i] ^ b[i]` over `words_for_dim(D)`
//!   `u64` words — embarrassingly vectorizable. The compiler emits
//!   `vpxor` on AVX2 builds with no further annotation needed. The
//!   `bind_in_place` variant avoids the destination allocation when
//!   the caller can mutate `a`.

use crate::hypervector::{words_for_dim, Hypervector};

/// § Bind two hypervectors : `dst[i] = a[i] XOR b[i]` for every bit `i`.
///
/// Bind is :
/// - **commutative** : `bind(a, b) = bind(b, a)` (XOR commutes).
/// - **associative** : `bind(a, bind(b, c)) = bind(bind(a, b), c)`.
/// - **self-inverse** : `bind(a, a) = 0` (the all-zeros hypervector).
/// - **identity-equivalent** : `bind(a, ZERO) = a`.
///
/// Tail-zero invariant is preserved automatically because XOR of two
/// tail-zero values is tail-zero.
#[must_use]
pub fn bind<const D: usize>(a: &Hypervector<D>, b: &Hypervector<D>) -> Hypervector<D> {
    let n = words_for_dim(D);
    let mut out_words = Vec::with_capacity(n);
    for i in 0..n {
        out_words.push(a.word(i) ^ b.word(i));
    }
    // § from_words masks the tail, but XOR-of-zero-tails is already
    //   zero so the mask is a no-op. Going through from_words anyway
    //   for the assert-on-length safety net.
    Hypervector::from_words(out_words)
}

/// § In-place bind : `a ^= b`. Avoids the allocation. Useful in
///   ancestry-chain construction where the chain is built up
///   incrementally — `current.bind_in_place(&parent)` replaces the
///   `let next = bind(&current, &parent)` allocation.
pub fn bind_in_place<const D: usize>(a: &mut Hypervector<D>, b: &Hypervector<D>) {
    let n = a.word_count();
    debug_assert_eq!(
        n,
        b.word_count(),
        "bind_in_place : word_count mismatch (this is a logic bug — \
         both hypervectors share const-generic D so word counts must match)"
    );
    for i in 0..n {
        let bw = b.word(i);
        let aw = a.word_mut(i);
        *aw ^= bw;
    }
    // § Tail-zero preserved by XOR-zero-tails ; no canonicalize call.
}

/// § Unbind : inverse of bind. For binary XOR this is identical to
///   bind itself (XOR is involutive : `(a XOR b) XOR b = a`). Exposed
///   as a separate function for caller clarity at use-site — when a
///   reader sees `unbind(bound, key)` they know the intent is "recover
///   the original from a bound pair", whereas `bind(bound, key)` reads
///   as "make a new bound vector".
///
/// Mathematically this is the same XOR loop as [`bind`].
#[must_use]
pub fn unbind<const D: usize>(bound: &Hypervector<D>, key: &Hypervector<D>) -> Hypervector<D> {
    bind(bound, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Foundational identity : `bind(a, b) ⊕ b = a`. This is the
    ///   reason the entire VSA scheme works. If this ever broke, every
    ///   downstream bind/unbind chain would corrupt.
    #[test]
    fn bind_unbind_roundtrip() {
        let a: Hypervector<10000> = Hypervector::random_from_seed(1);
        let b: Hypervector<10000> = Hypervector::random_from_seed(2);
        let bound = bind(&a, &b);
        let recovered = unbind(&bound, &b);
        assert_eq!(a, recovered);
    }

    /// § `bind(a, a) = 0`. Self-bind is the additive identity in the
    ///   binary form (because every bit XORs to zero).
    #[test]
    fn bind_self_is_zero() {
        let a: Hypervector<256> = Hypervector::random_from_seed(42);
        let z = bind(&a, &a);
        assert_eq!(z.popcount(), 0);
    }

    /// § Commutativity : `bind(a, b) = bind(b, a)`.
    #[test]
    fn bind_commutative() {
        let a: Hypervector<512> = Hypervector::random_from_seed(0xAAA);
        let b: Hypervector<512> = Hypervector::random_from_seed(0xBBB);
        let ab = bind(&a, &b);
        let ba = bind(&b, &a);
        assert_eq!(ab, ba);
    }

    /// § Associativity : `bind(bind(a, b), c) = bind(a, bind(b, c))`.
    ///   Required for ancestry-chain construction order independence.
    #[test]
    fn bind_associative() {
        let a: Hypervector<512> = Hypervector::random_from_seed(0xA);
        let b: Hypervector<512> = Hypervector::random_from_seed(0xB);
        let c: Hypervector<512> = Hypervector::random_from_seed(0xC);
        let left = bind(&bind(&a, &b), &c);
        let right = bind(&a, &bind(&b, &c));
        assert_eq!(left, right);
    }

    /// § Identity : `bind(a, ZERO) = a`.
    #[test]
    fn bind_with_zero_is_identity() {
        let a: Hypervector<256> = Hypervector::random_from_seed(7);
        let z: Hypervector<256> = Hypervector::zero();
        let result = bind(&a, &z);
        assert_eq!(result, a);
    }

    /// § In-place bind matches the allocating bind.
    #[test]
    fn bind_in_place_matches() {
        let a: Hypervector<256> = Hypervector::random_from_seed(11);
        let b: Hypervector<256> = Hypervector::random_from_seed(13);
        let allocating = bind(&a, &b);
        let mut in_place = a.clone();
        bind_in_place(&mut in_place, &b);
        assert_eq!(allocating, in_place);
    }

    /// § Three-deep ancestry chain : root, parent, child. From child
    ///   alone (= bind(bind(root, parent_key), child_key)), given the
    ///   keys we recover root.
    #[test]
    fn ancestry_chain_three_deep() {
        let root: Hypervector<10000> = Hypervector::random_from_seed(100);
        let parent_key: Hypervector<10000> = Hypervector::random_from_seed(200);
        let child_key: Hypervector<10000> = Hypervector::random_from_seed(300);

        // § Chain forward : root → bound_parent → bound_child.
        let bound_parent = bind(&root, &parent_key);
        let bound_child = bind(&bound_parent, &child_key);

        // § Unwind backward : peel child_key, then parent_key.
        let stage_1 = unbind(&bound_child, &child_key);
        let recovered_root = unbind(&stage_1, &parent_key);

        assert_eq!(recovered_root, root);
    }

    /// § Eight-deep ancestry — matches the spec's
    ///   `SmallVec<Handle<CreatureGenome>, 8>` capacity.
    #[test]
    fn ancestry_chain_eight_deep() {
        let root: Hypervector<10000> = Hypervector::random_from_seed(1);
        let keys: Vec<Hypervector<10000>> = (0..8)
            .map(|i| Hypervector::random_from_seed(1000 + i as u64))
            .collect();

        // § Forward chain.
        let mut bound = root.clone();
        for k in &keys {
            bound_in_place(&mut bound, k);
        }

        // § Backward unwind in reverse order.
        for k in keys.iter().rev() {
            bound_in_place(&mut bound, k);
        }

        assert_eq!(bound, root);
    }

    /// § Helper used in eight-deep test — local inline because the
    ///   in-place variant is a thin wrapper.
    fn bound_in_place<const D: usize>(a: &mut Hypervector<D>, b: &Hypervector<D>) {
        bind_in_place(a, b);
    }

    /// § Bind preserves tail-zero invariant. Construct a hypervector
    ///   with `D` not divisible by 64, bind it with itself's complement
    ///   (the all-ones with same D), check tail bits remain zero.
    #[test]
    fn bind_preserves_tail_invariant() {
        let a: Hypervector<100> = Hypervector::random_from_seed(7);
        let b: Hypervector<100> = Hypervector::ones();
        let result = bind(&a, &b);
        // § Word 1 has 36 valid bits ; high 28 must be zero.
        let last = result.word(1);
        assert_eq!(last & !((1u64 << 36) - 1), 0, "tail bits leaked : {last:#x}");
    }
}
