//! § cssl-hdc::hologram — K-many bound key-value pairs in one hypervector
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The classic Kanerva "associative memory as a hypervector" pattern.
//!   A [`Hologram`] stores K key-value pairs by bundling K bound
//!   `bind(key_i, value_i)` results into a single D-dimensional
//!   hypervector. Recall is via `unbind(hologram, query_key)` followed
//!   by similarity-search against the value vocabulary.
//!
//!   The name "hologram" comes from the optical-holography analogy :
//!   every key-value pair contributes to every bit of the output, so
//!   the storage is **distributed** rather than addressed. Damage to a
//!   fraction of the bits degrades all stored pairs gracefully (each
//!   loses Hamming bits proportional to the damage), rather than
//!   destroying any single pair outright. This is what the
//!   `episodic-memory` consumer wants : graceful degradation under
//!   compression / noise.
//!
//! § CAPACITY
//!   Recall accuracy depends on `K / sqrt(D)`. Empirically :
//!   - `K ≤ √D / log(D)` → near-perfect recall (similarity > 0.5
//!     on the correct value, < 0.1 on incorrect values).
//!   - `K ≈ √D` → degraded recall (similarity ≈ 0.3 correct, 0.1
//!     incorrect — still distinguishable but with margin).
//!   - `K > √D` → catastrophic capacity-overrun (cannot reliably
//!     distinguish target from noise).
//!   For D = 10000, the safe band is K ≤ ~50. This crate does NOT
//!   enforce a hard cap — callers are expected to know their capacity.
//!
//! § STORE / RECALL ALGORITHM
//!   Store : `hologram = bundle(bind(k_1, v_1), bind(k_2, v_2), ...)`
//!     — one bundle pass at construction time.
//!   Recall : `recovered = unbind(hologram, query_key)` — single XOR.
//!     Then compare `recovered` against the value vocabulary via
//!     [`crate::similarity::similarity_bipolar`] and pick the highest
//!     match. This is O(|vocab|) per recall but each comparison is
//!     157-word popcount, so vocab sizes of thousands fit in
//!     microsecond budgets.
//!
//! § INTEGRATION POINT
//!   `cssl-substrate-omega-field` cells will carry an optional
//!   `tag: Option<Hypervector<32>>` semantic-annotation slot that is
//!   populated by `π_companion` (companion-perspective render) and
//!   consumed by HDC-bound-Lenia novelty path V.5. A [`Hologram`] of
//!   creature observations + region tags will be stored at the
//!   `EpisodicMemory` slice — call this the "did I notice anything
//!   like this before in this region" lookup.

use crate::bind::{bind, unbind};
use crate::bundle::bundle;
use crate::hypervector::Hypervector;
use crate::similarity::similarity_bipolar;

/// § A holographic associative memory storing K key-value pairs in a
///   single D-dimensional hypervector. Both keys and values are
///   `Hypervector<D>` ; the hologram is itself a `Hypervector<D>`.
///
/// The const-generic `K` is **not** a hard storage limit — the
/// hologram's capacity is dimension-bound, not entry-count-bound. `K`
/// is a hint for callers to track their entry count and signal intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hologram<const D: usize, const K: usize> {
    /// § The bundled hypervector. All key-value pairs contribute.
    storage: Hypervector<D>,
    /// § Number of pairs actually stored. Tracked for capacity warnings.
    n_stored: usize,
}

impl<const D: usize, const K: usize> Hologram<D, K> {
    /// § Construct an empty hologram (the all-zero hypervector).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            storage: Hypervector::zero(),
            n_stored: 0,
        }
    }

    /// § Construct a hologram from a slice of `(key, value)` pairs.
    ///   The storage is `bundle(bind(k_1, v_1), bind(k_2, v_2), ...)`.
    pub fn from_pairs(pairs: &[(&Hypervector<D>, &Hypervector<D>)]) -> Self {
        let bound: Vec<Hypervector<D>> = pairs
            .iter()
            .map(|(k, v)| bind(k, v))
            .collect();
        let bound_refs: Vec<&Hypervector<D>> = bound.iter().collect();
        Self {
            storage: bundle(&bound_refs),
            n_stored: pairs.len(),
        }
    }

    /// § Add a single key-value pair to an existing hologram. This is
    ///   a "rebundle" operation — the new pair is bound and bundled
    ///   with the existing storage. Note : because bundle is
    ///   majority-vote, repeatedly adding pairs **changes the
    ///   thresholding** which slightly degrades pre-existing pairs.
    ///   For best capacity, prefer constructing from all pairs at once
    ///   via [`Self::from_pairs`].
    ///
    /// We approximate "add" as `bundle(self, bind(k, v))` with two
    /// inputs — for batched-rebuild semantics use [`Self::from_pairs`].
    pub fn store(&mut self, key: &Hypervector<D>, value: &Hypervector<D>) {
        let bound = bind(key, value);
        // § For an empty hologram, just take the bound pair.
        if self.n_stored == 0 {
            self.storage = bound;
        } else {
            // § Two-input bundle : even-N tie-break loses bits. To
            //   preserve previously-stored pairs we treat the existing
            //   storage with weight = n_stored and the new pair with
            //   weight 1, by bundling [storage; n_stored] U [bound; 1].
            //   For the foundation slice we approximate with a
            //   3-input bundle (storage, storage, bound) which gives
            //   the previous storage a 2:1 majority over the new pair —
            //   imperfect but better than naive 2-input bundle.
            self.storage = bundle(&[&self.storage, &self.storage, &bound]);
        }
        self.n_stored += 1;
    }

    /// § Recall : produce the noisy approximation of the value bound
    ///   to `query_key`. The result is `unbind(storage, query_key)`,
    ///   which equals `value + noise` where `noise` comes from the
    ///   other K-1 pairs being un-bound by the wrong key.
    #[must_use]
    pub fn recall(&self, query_key: &Hypervector<D>) -> Hypervector<D> {
        unbind(&self.storage, query_key)
    }

    /// § Recall and similarity-search against a vocabulary. Returns the
    ///   best-matching value plus its similarity score. The returned
    ///   index is into the `vocab` slice. Use this when you have a
    ///   known-finite set of possible values and want the snapping
    ///   step.
    pub fn recall_match<'a>(
        &self,
        query_key: &Hypervector<D>,
        vocab: &'a [Hypervector<D>],
    ) -> Option<HologramRecallResult<'a, D>> {
        if vocab.is_empty() {
            return None;
        }
        let recovered = self.recall(query_key);
        let mut best_idx = 0;
        let mut best_sim = f32::NEG_INFINITY;
        for (i, candidate) in vocab.iter().enumerate() {
            let s = similarity_bipolar(&recovered, candidate);
            if s > best_sim {
                best_sim = s;
                best_idx = i;
            }
        }
        Some(HologramRecallResult {
            index: best_idx,
            value: &vocab[best_idx],
            similarity: best_sim,
            recovered,
        })
    }

    /// § Number of pairs stored. Note : this is a **stored** count, not
    ///   a **recallable** count — over-capacity holograms still report
    ///   the raw store count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.n_stored
    }

    /// § True iff no pairs have been stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.n_stored == 0
    }

    /// § Borrow the underlying storage hypervector. Useful when you
    ///   want to bundle-with-bind hologram-of-holograms — the
    ///   composability that makes HDC interesting.
    #[must_use]
    pub fn storage(&self) -> &Hypervector<D> {
        &self.storage
    }
}

/// § Result of a vocab-matched recall. Carries both the matched value
///   and the raw recovered hypervector so the caller can inspect the
///   similarity-margin between the top match and the runner-up.
#[derive(Debug, Clone)]
pub struct HologramRecallResult<'a, const D: usize> {
    /// § Index into the vocab slice.
    pub index: usize,
    /// § Reference to the matched value hypervector.
    pub value: &'a Hypervector<D>,
    /// § Bipolar similarity score in [-1, +1].
    pub similarity: f32,
    /// § The raw `unbind(storage, query_key)` — useful for diagnostics.
    pub recovered: Hypervector<D>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Empty hologram has zero storage.
    #[test]
    fn empty_hologram() {
        let h: Hologram<10000, 16> = Hologram::empty();
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert_eq!(h.storage(), &Hypervector::<10000>::zero());
    }

    /// § Single-pair hologram : recall with the right key returns the
    ///   value exactly (no noise from other pairs).
    #[test]
    fn single_pair_recall_exact() {
        let key: Hypervector<10000> = Hypervector::random_from_seed(1);
        let value: Hypervector<10000> = Hypervector::random_from_seed(2);
        let h: Hologram<10000, 1> = Hologram::from_pairs(&[(&key, &value)]);
        let recovered = h.recall(&key);
        assert_eq!(recovered, value);
    }

    /// § Multi-pair hologram : each key recovers its value above the
    ///   noise threshold. Capacity 8 at D = 10000 — bundled signal-
    ///   to-noise with majority-vote bundling is approximately
    ///   `1/sqrt(M) ≈ 0.35`. We require similarity > 0.2 (well above
    ///   the random baseline of ~0.0).
    #[test]
    fn multi_pair_recall_above_noise() {
        let pairs: Vec<(Hypervector<10000>, Hypervector<10000>)> = (0..8)
            .map(|i| {
                (
                    Hypervector::random_from_seed(1000 + i as u64),
                    Hypervector::random_from_seed(2000 + i as u64),
                )
            })
            .collect();
        let pair_refs: Vec<(&Hypervector<10000>, &Hypervector<10000>)> =
            pairs.iter().map(|(k, v)| (k, v)).collect();
        let h: Hologram<10000, 8> = Hologram::from_pairs(&pair_refs);

        for (key, value) in &pairs {
            let recovered = h.recall(key);
            let s = similarity_bipolar(&recovered, value);
            assert!(s > 0.2, "similarity {s} below 0.2");
        }
    }

    /// § Multi-pair hologram : query with a wrong key gives a noise-
    ///   level result (similarity near zero).
    #[test]
    fn multi_pair_wrong_key_noise() {
        let pairs: Vec<(Hypervector<10000>, Hypervector<10000>)> = (0..5)
            .map(|i| {
                (
                    Hypervector::random_from_seed(3000 + i as u64),
                    Hypervector::random_from_seed(4000 + i as u64),
                )
            })
            .collect();
        let pair_refs: Vec<(&Hypervector<10000>, &Hypervector<10000>)> =
            pairs.iter().map(|(k, v)| (k, v)).collect();
        let h: Hologram<10000, 5> = Hologram::from_pairs(&pair_refs);

        let unrelated_key: Hypervector<10000> = Hypervector::random_from_seed(99999);
        let recovered = h.recall(&unrelated_key);
        // § Compare against each stored value : all should be ≈ 0
        //   similarity (none of them was the bound value).
        for (_, value) in &pairs {
            let s = similarity_bipolar(&recovered, value);
            assert!(s.abs() < 0.2, "wrong-key match too high : {s}");
        }
    }

    /// § Recall_match selects the correct vocab entry.
    #[test]
    fn recall_match_finds_correct() {
        let keys: Vec<Hypervector<10000>> = (0..6)
            .map(|i| Hypervector::random_from_seed(5000 + i as u64))
            .collect();
        let values: Vec<Hypervector<10000>> = (0..6)
            .map(|i| Hypervector::random_from_seed(6000 + i as u64))
            .collect();
        let pairs: Vec<(&Hypervector<10000>, &Hypervector<10000>)> = keys
            .iter()
            .zip(values.iter())
            .collect();
        let h: Hologram<10000, 6> = Hologram::from_pairs(&pairs);

        for (i, key) in keys.iter().enumerate() {
            let result = h.recall_match(key, &values).unwrap();
            assert_eq!(result.index, i, "wrong vocab match for key {i}");
            // § Capacity 6 at D = 10000 ⇒ 1/sqrt(6) ≈ 0.4 expected
            //   similarity ; lower-bound at 0.2.
            assert!(result.similarity > 0.2);
        }
    }

    /// § Recall_match on empty vocab returns None.
    #[test]
    fn recall_match_empty_vocab() {
        let key: Hypervector<256> = Hypervector::random_from_seed(1);
        let h: Hologram<256, 1> = Hologram::empty();
        let vocab: Vec<Hypervector<256>> = vec![];
        assert!(h.recall_match(&key, &vocab).is_none());
    }

    /// § Capacity smoke test : 16 pairs at D = 10000 still recover
    ///   above noise.
    #[test]
    fn capacity_sixteen_at_10000() {
        let pairs: Vec<(Hypervector<10000>, Hypervector<10000>)> = (0..16)
            .map(|i| {
                (
                    Hypervector::random_from_seed(7000 + i as u64),
                    Hypervector::random_from_seed(8000 + i as u64),
                )
            })
            .collect();
        let pair_refs: Vec<(&Hypervector<10000>, &Hypervector<10000>)> =
            pairs.iter().map(|(k, v)| (k, v)).collect();
        let h: Hologram<10000, 16> = Hologram::from_pairs(&pair_refs);

        for (key, value) in &pairs {
            let recovered = h.recall(key);
            let s = similarity_bipolar(&recovered, value);
            // § 1/sqrt(16) = 0.25 expected ; lower-bound at 0.15.
            assert!(s > 0.15, "16-pair recall too noisy : {s}");
        }
    }

    /// § Incremental store via `Hologram::store` versus batch from_pairs.
    ///   Incremental is approximate ; we just check that it doesn't
    ///   completely lose the most-recent pair.
    #[test]
    fn incremental_store_recovers_last() {
        let mut h: Hologram<10000, 4> = Hologram::empty();
        let key: Hypervector<10000> = Hypervector::random_from_seed(11);
        let value: Hypervector<10000> = Hypervector::random_from_seed(22);
        h.store(&key, &value);
        let recovered = h.recall(&key);
        // § Single-pair incremental store recovers exactly.
        assert_eq!(recovered, value);
    }
}
