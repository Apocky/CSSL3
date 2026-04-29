//! § cssl-hdc::genome — Genome 10000-D specialization
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The 10000-D specialization that the CSSLv3 substrate spec
//!   `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 7 Genome` requires :
//!
//!   ```cssl
//!   type Genome = {
//!     hdc:          HDC.Hypervector<u64, HDC_DIM>,    // HDC_DIM = 10000
//!     kan_weights:  KanGenomeWeights,
//!     signature:    blake3.Hash,
//!     generation:   u64,
//!     parent_hashes: SmallVec<blake3.Hash, 4>,
//!   }
//!   ```
//!
//!   This module supplies the **HDC component** (`hdc:` field) and the
//!   genome-level operations the spec declares :
//!   - `Genome::new_from_seed(seed: u64) -> Genome` — deterministic
//!     construction from a single seed.
//!   - `Genome::cross(a, b, rng) -> Genome` — bipolar bit-mix between
//!     two parent genomes (sexual reproduction analog).
//!   - `Genome::mutate(self, rng, rate) -> Genome` — per-bit
//!     probabilistic flip at the given rate.
//!
//!   The KAN-weights field is owned by the `cssl-creature-genome` crate
//!   (T11-D138 follow-up dispatch) ; this crate provides the HDC
//!   foundation and a `Genome::distance` helper for ancestry queries.
//!
//! § DIMENSION = 10000 — why this specific value
//!   Kanerva's empirical guidance : `D ≥ 1000` for any meaningful HDC,
//!   `D ≈ 10000` for "robust under realistic noise". The 10000 number
//!   appears throughout the literature (Kanerva 2009, Plate 1995,
//!   Schlegel 2022) and the substrate spec adopts it verbatim. Fixing
//!   `HDC_DIM = 10000` here means :
//!   - 1 hypervector = 1.25 KiB (157 × u64).
//!   - bind / bundle ≈ 1 µs on commodity CPU.
//!   - capacity for ≈ 50 superposed pairs in a hologram before noise
//!     dominates.
//!   - genome-distance (Hamming) costs ≈ 200 ns.
//!
//! § DETERMINISM
//!   Every operation in this module is `{DetRNG}`-compliant : same
//!   seed ⇒ same Genome bit-for-bit, on every host, every toolchain.
//!   `Genome::cross` and `Genome::mutate` take an explicit `&mut rng`
//!   (as `SplitMix64`) rather than a thread-local or time-based source.
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!   The Genome type is the spec's "individual identity" carrier — it
//!   IS the thing whose Sovereignty (at tier L4+) is consent-protected.
//!   This crate's surface MUST :
//!   - **Be deterministic** — replay-equivalence is what makes the
//!     `Audit<'genome_*>` row meaningful.
//!   - **Preserve information** — `cross` mixes inputs without
//!     destroying ; `mutate` is a controlled-variance operation.
//!   - **Not leak entropy** — no `thread_rng`, no `SystemTime::now()`,
//!     no per-host fingerprinting.

use crate::bind::bind;
use crate::hypervector::Hypervector;
use crate::prng::{splitmix64_next, SplitMix64};
use crate::similarity::{hamming_distance, similarity_bipolar};

/// § The canonical genome dimension. Matches
///   `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 7` declaration
///   `const HDC_DIM : usize = 10000`.
pub const HDC_DIM: usize = 10000;

/// § Genome — the HDC component of the substrate spec's Genome record.
///
/// Wraps a [`Hypervector<HDC_DIM>`] with genome-specific operations :
/// distance to another genome (Hamming / bipolar similarity), crossover
/// (mix two parents), mutate (per-bit flip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genome {
    /// § The hyperdimensional encoding.
    hdc: Hypervector<HDC_DIM>,
}

impl Genome {
    /// § Construct a genome from a single seed. The hypervector is
    ///   `Hypervector::random_from_seed(seed)`. Same seed ⇒ same
    ///   genome on every host and every run.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        Self {
            hdc: Hypervector::random_from_seed(seed),
        }
    }

    /// § Construct from an existing hypervector. Caller-owned construction
    ///   path — useful when the hypervector was already computed (e.g.
    ///   via cross / mutate / unbind).
    #[must_use]
    pub fn from_hdc(hdc: Hypervector<HDC_DIM>) -> Self {
        Self { hdc }
    }

    /// § The all-zero genome. Useful as a sentinel / additive-identity
    ///   element for bundle-style genome-pool aggregation.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            hdc: Hypervector::zero(),
        }
    }

    /// § Borrow the underlying hypervector.
    #[must_use]
    pub fn hdc(&self) -> &Hypervector<HDC_DIM> {
        &self.hdc
    }

    /// § Consume into the underlying hypervector. Useful when the
    ///   downstream consumer wants the raw hypervector for HDC
    ///   primitives that don't need the Genome wrapper.
    #[must_use]
    pub fn into_hdc(self) -> Hypervector<HDC_DIM> {
        self.hdc
    }

    /// § Compute the Hamming distance to another genome. Range [0, HDC_DIM].
    ///   Random pair → ≈ 5000. Identical → 0. Useful for ancestry
    ///   queries : "are these two creatures siblings ?" if distance < 100.
    #[must_use]
    pub fn distance(a: &Genome, b: &Genome) -> GenomeDistance {
        let h = hamming_distance(&a.hdc, &b.hdc);
        let s = similarity_bipolar(&a.hdc, &b.hdc);
        GenomeDistance {
            hamming: h,
            normalized: (h as f32) / (HDC_DIM as f32),
            bipolar_similarity: s,
        }
    }

    /// § Cross two parent genomes : produce a child whose hypervector
    ///   mixes `a` and `b` bit-by-bit. The mixing is a per-bit random
    ///   choice : with probability 0.5, the child's bit is from `a` ;
    ///   otherwise from `b`. This matches the standard genetic-algorithm
    ///   uniform-crossover scheme and preserves the Hamming-similarity
    ///   structure : `distance(child, a) + distance(child, b)
    ///   ≈ distance(a, b)`.
    pub fn cross(a: &Genome, b: &Genome, rng: &mut SplitMix64) -> Self {
        let n_words = a.hdc.word_count();
        let mut child_words = Vec::with_capacity(n_words);
        for w in 0..n_words {
            // § For each word : draw a 64-bit random mask. Bits set in
            //   the mask take from `a`, bits clear take from `b`.
            //   This is the standard MUX-by-mask trick :
            //     out = (a & mask) | (b & !mask)
            let mask = rng.next();
            let aw = a.hdc.word(w);
            let bw = b.hdc.word(w);
            let mixed = (aw & mask) | (bw & !mask);
            child_words.push(mixed);
        }
        Self {
            hdc: Hypervector::from_words(child_words),
        }
    }

    /// § Mutate a genome by per-bit probabilistic flip. `rate` is the
    ///   probability that any given bit is flipped, in `[0.0, 1.0]`.
    ///   Returns a new Genome ; the original is unchanged.
    ///
    ///   At rate 0.0 the output equals the input. At rate 1.0 every
    ///   bit is flipped (output = bit-complement). At rate 0.5 the
    ///   output is uncorrelated noise. Practical mutation rates are
    ///   `0.001..0.05` matching biological mutation rates per generation.
    ///
    ///   The implementation draws `D` bool-decisions and flips
    ///   accordingly — `O(D)` work, ≈ 1 µs at D = 10000.
    #[must_use]
    pub fn mutate(self, rng: &mut SplitMix64, rate: f32) -> Self {
        let rate = rate.clamp(0.0, 1.0);
        if rate == 0.0 {
            return self;
        }
        if rate == 1.0 {
            // § Full flip : bind with all-ones is the bit-complement.
            return Self {
                hdc: bind(&self.hdc, &Hypervector::ones()),
            };
        }
        // § Quantize rate to a u32 threshold : `threshold = rate * 2^32`.
        //   For each bit, draw a u32 and compare. This is faster than
        //   per-bit `next_f32()` and gives identical statistics.
        let threshold = (rate * (u32::MAX as f32)) as u32;
        let n_words = self.hdc.word_count();
        let mut new_words = Vec::with_capacity(n_words);
        for w in 0..n_words {
            let original = self.hdc.word(w);
            let mut flip_mask = 0u64;
            // § Two u32 draws per word — 64 bit-decisions per word.
            for half in 0..2 {
                let mut chunk = 0u64;
                for b in 0..32 {
                    let r = rng.next_u32();
                    if r < threshold {
                        chunk |= 1u64 << b;
                    }
                }
                flip_mask |= chunk << (half * 32);
            }
            new_words.push(original ^ flip_mask);
        }
        Self {
            hdc: Hypervector::from_words(new_words),
        }
    }

    /// § Compute a stable u64 hash of the genome. Useful as a
    ///   non-cryptographic fingerprint for ancestry-set membership
    ///   testing. NOT a cryptographic hash — for that, downstream
    ///   `cssl-creature-genome` will use blake3 over the hdc bytes plus
    ///   the kan_weights bytes, matching the spec's `signature` field.
    #[must_use]
    pub fn fingerprint_u64(&self) -> u64 {
        let mut acc = 0u64;
        for w in self.hdc.words() {
            // § Mix each word into the accumulator via splitmix64.
            //   Order matters : changing one bit in any word produces
            //   a different fingerprint with high probability.
            acc = splitmix64_next(acc ^ *w);
        }
        acc
    }
}

/// § Triple-form distance result for genome-similarity queries.
///   Different downstream uses prefer different metrics :
///   - `hamming` for integer ancestry-tree distance.
///   - `normalized` for fraction-of-bits-different threshold checks.
///   - `bipolar_similarity` for [-1, +1] correlation in active-inference
///     loops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenomeDistance {
    /// § Integer Hamming distance ∈ [0, HDC_DIM = 10000].
    pub hamming: u32,
    /// § Hamming / HDC_DIM ∈ [0.0, 1.0].
    pub normalized: f32,
    /// § Bipolar similarity ∈ [-1.0, +1.0].
    pub bipolar_similarity: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § HDC_DIM is exactly 10000 (locked by spec).
    #[test]
    fn hdc_dim_is_10000() {
        assert_eq!(HDC_DIM, 10000);
    }

    /// § Genome::from_seed is deterministic : same seed ⇒ same genome.
    #[test]
    fn from_seed_deterministic() {
        let a = Genome::from_seed(0xDEAD_BEEF);
        let b = Genome::from_seed(0xDEAD_BEEF);
        assert_eq!(a, b);
    }

    /// § Different seeds produce different genomes.
    #[test]
    fn different_seeds_different_genomes() {
        let a = Genome::from_seed(1);
        let b = Genome::from_seed(2);
        assert_ne!(a, b);
    }

    /// § Genome::distance to self is zero.
    #[test]
    fn distance_self_zero() {
        let g = Genome::from_seed(42);
        let d = Genome::distance(&g, &g);
        assert_eq!(d.hamming, 0);
        assert_eq!(d.normalized, 0.0);
        assert_eq!(d.bipolar_similarity, 1.0);
    }

    /// § Random pair Hamming ≈ 5000 ; normalized ≈ 0.5.
    #[test]
    fn distance_random_pair_baseline() {
        let a = Genome::from_seed(1);
        let b = Genome::from_seed(2);
        let d = Genome::distance(&a, &b);
        assert!((4500..=5500).contains(&d.hamming), "h = {}", d.hamming);
        assert!((0.45..=0.55).contains(&d.normalized));
        assert!((-0.1..=0.1).contains(&d.bipolar_similarity));
    }

    /// § Cross of identical parents produces the same genome.
    #[test]
    fn cross_identical_parents() {
        let a = Genome::from_seed(7);
        let mut rng = SplitMix64::new(42);
        let child = Genome::cross(&a, &a, &mut rng);
        assert_eq!(child, a);
    }

    /// § Cross is deterministic given the same RNG seed.
    #[test]
    fn cross_deterministic() {
        let a = Genome::from_seed(1);
        let b = Genome::from_seed(2);
        let mut rng_1 = SplitMix64::new(99);
        let mut rng_2 = SplitMix64::new(99);
        let child_1 = Genome::cross(&a, &b, &mut rng_1);
        let child_2 = Genome::cross(&a, &b, &mut rng_2);
        assert_eq!(child_1, child_2);
    }

    /// § Cross produces a child within the parent-distance band.
    ///   `distance(child, a) + distance(child, b) ≈ distance(a, b)`.
    #[test]
    fn cross_preserves_parent_distance() {
        let a = Genome::from_seed(11);
        let b = Genome::from_seed(13);
        let mut rng = SplitMix64::new(0);
        let child = Genome::cross(&a, &b, &mut rng);

        let d_ab = Genome::distance(&a, &b).hamming;
        let d_ca = Genome::distance(&child, &a).hamming;
        let d_cb = Genome::distance(&child, &b).hamming;

        // § Child should be in the convex-Hamming-ball between parents.
        //   With uniform crossover at 50/50, child takes ≈ half from
        //   each parent ⇒ d(c, a) ≈ d(c, b) ≈ d(a, b) / 2.
        assert!(d_ca + d_cb >= d_ab.saturating_sub(50));
        assert!((d_ca as i32 - d_cb as i32).unsigned_abs() < 500);
    }

    /// § Mutate rate 0.0 returns identity.
    #[test]
    fn mutate_rate_zero_identity() {
        let g = Genome::from_seed(7);
        let mut rng = SplitMix64::new(0);
        let mutated = g.clone().mutate(&mut rng, 0.0);
        assert_eq!(mutated, g);
    }

    /// § Mutate rate 1.0 returns bit-complement.
    #[test]
    fn mutate_rate_one_complement() {
        let g = Genome::from_seed(7);
        let mut rng = SplitMix64::new(0);
        let mutated = g.clone().mutate(&mut rng, 1.0);
        let d = Genome::distance(&g, &mutated);
        assert_eq!(d.hamming, HDC_DIM as u32);
    }

    /// § Mutate rate 0.5 produces ≈ 5000 bits flipped.
    #[test]
    fn mutate_rate_half() {
        let g = Genome::from_seed(7);
        let mut rng = SplitMix64::new(0);
        let mutated = g.mutate(&mut rng, 0.5);
        // § g is gone here, since mutate consumes self ; test just by
        //   constructing distance from a fresh from_seed.
        let g2 = Genome::from_seed(7);
        let d = Genome::distance(&g2, &mutated);
        assert!(
            (4500..=5500).contains(&d.hamming),
            "rate-0.5 flips : {}",
            d.hamming
        );
    }

    /// § Mutate small rate produces few flips.
    #[test]
    fn mutate_low_rate_few_flips() {
        let g = Genome::from_seed(7);
        let g2 = Genome::from_seed(7);
        let mut rng = SplitMix64::new(0);
        let mutated = g.mutate(&mut rng, 0.01);
        let d = Genome::distance(&g2, &mutated);
        // § Rate 0.01 ⇒ ≈ 100 flips ± 30.
        assert!(
            (50..=200).contains(&d.hamming),
            "low-rate flips : {}",
            d.hamming
        );
    }

    /// § Mutate clamps out-of-range rates.
    #[test]
    fn mutate_clamps_rate() {
        let g = Genome::from_seed(7);
        let mut rng = SplitMix64::new(0);
        // § Out-of-range rate -1.0 should clamp to 0.0 ⇒ identity.
        let m_neg = g.clone().mutate(&mut rng, -1.0);
        assert_eq!(m_neg, g);
        // § Out-of-range rate 2.0 should clamp to 1.0 ⇒ complement.
        let m_high = g.mutate(&mut rng, 2.0);
        let g2 = Genome::from_seed(7);
        let d = Genome::distance(&g2, &m_high);
        assert_eq!(d.hamming, HDC_DIM as u32);
    }

    /// § Fingerprint is deterministic.
    #[test]
    fn fingerprint_deterministic() {
        let a = Genome::from_seed(0xCAFE);
        let b = Genome::from_seed(0xCAFE);
        assert_eq!(a.fingerprint_u64(), b.fingerprint_u64());
    }

    /// § Different genomes have different fingerprints (high probability).
    #[test]
    fn fingerprint_different_genomes_differ() {
        let a = Genome::from_seed(1);
        let b = Genome::from_seed(2);
        assert_ne!(a.fingerprint_u64(), b.fingerprint_u64());
    }

    /// § Genome storage size : 157 u64 = 1256 bytes for the hdc field.
    #[test]
    fn genome_storage_size() {
        let g = Genome::from_seed(0);
        assert_eq!(g.hdc().word_count(), 157);
    }

    /// § Round-trip from_hdc / into_hdc.
    #[test]
    fn from_hdc_into_hdc_roundtrip() {
        let hv: Hypervector<HDC_DIM> = Hypervector::random_from_seed(99);
        let g = Genome::from_hdc(hv.clone());
        let recovered = g.into_hdc();
        assert_eq!(recovered, hv);
    }

    /// § Zero genome.
    #[test]
    fn zero_genome() {
        let g = Genome::zero();
        assert_eq!(g.hdc().popcount(), 0);
    }

    /// § Cross does NOT modify either parent.
    #[test]
    fn cross_does_not_modify_parents() {
        let a = Genome::from_seed(1);
        let b = Genome::from_seed(2);
        let a_copy = a.clone();
        let b_again = Genome::from_seed(2);
        let mut rng = SplitMix64::new(0);
        let _child = Genome::cross(&a, &b, &mut rng);
        assert_eq!(a, a_copy);
        assert_eq!(b, b_again);
    }
}
