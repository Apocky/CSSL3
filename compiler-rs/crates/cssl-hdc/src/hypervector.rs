//! § cssl-hdc::hypervector — the core `Hypervector<D>` type
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Defines [`Hypervector<D>`] — the binary u64-packed hyperdimensional
//!   vector that is the workhorse of every other module in this crate. The
//!   const-generic `D` is the dimension count (e.g. 10000 for genomes) ;
//!   the storage is `[u64 ; ceil(D / 64)]` with the high-bits of the last
//!   word zeroed when `D % 64 != 0`. The bipolar i8-form [`HypervectorI8`]
//!   is the unpacked alternative.
//!
//! § INVARIANT — tail-bits are zero
//!   When `D` does not divide 64, the last `u64` word contains exactly
//!   `D % 64` valid bits in its low end ; the upper `64 - D % 64` bits
//!   MUST be zero. This is enforced by every constructor and every
//!   in-place op via [`Hypervector::canonicalize_tail`]. Why : Hamming
//!   distance is computed as `popcount(a XOR b)` summed over all words,
//!   and a stray tail bit would inflate the distance by 1 unit per
//!   spurious set bit — silently breaking similarity comparisons. We
//!   protect this invariant by construction rather than at use-site so
//!   the popcount path stays branch-free.
//!
//! § STORAGE NOTES
//!   - `repr(C)` so `&[Hypervector<D>]` slices can be cast to
//!     `&[u64]` for SIMD or GPU upload paths (matches `cssl-math`'s
//!     vector layout discipline).
//!   - The number of `u64` words is `(D + 63) / 64`, computed via the
//!     [`words_for_dim`] helper at compile time. We use a `[u64 ; N]`
//!     array (not a `Vec<u64>`) so the type is `Copy` for small `D`s
//!     and lives entirely on the stack. For `D = 10000` this is a
//!     1.25 KiB stack-resident value — fine for hot paths and well
//!     under any reasonable stack-frame budget.
//!   - We could not use a generic-const-expression to express
//!     `[u64 ; (D + 63) / 64]` directly under MSRV 1.75 (that requires
//!     `feature(generic_const_exprs)`). Instead we expose a parallel
//!     `Hypervector<const D: usize, const W: usize>` shape only via the
//!     [`hypervector!`] macro and the [`Genome`] specialization ; the
//!     general user-facing surface uses runtime-allocated `Vec<u64>` via
//!     [`HypervectorDyn`] for general `D`. The two share their algebra
//!     through the [`HamFn`] trait.
//!
//! § BIPOLAR ↔ BINARY MAPPING
//!
//!   ```text
//!   bipolar    binary    rationale
//!     -1         0       bipolar pointwise multiply (a · b) maps to
//!                        binary XOR :   (2a-1)·(2b-1) = 1 - 2·(a XOR b)
//!     +1         1       so XOR-then-popcount-then-affine recovers the
//!                        bipolar dot-product exactly.
//!   ```
//!
//!   A consequence : `bind` is its own inverse on the binary form
//!   (because `XOR` is involutive), and the bipolar form's `bind` is
//!   pointwise multiplication which is also its own inverse on
//!   `{-1, +1}`. So `unbind = bind` regardless of representation.

use crate::prng::splitmix64_next;

/// § Compute the number of `u64` words required to store `D` bits.
///
/// Used at every `Hypervector<D>` site to determine the inner array
/// length. `const fn` so it can drive compile-time array sizing where
/// possible, and so the `D = 10000 ⇒ W = 157` pairing is verifiable in
/// a `const _ : ()` static assertion.
#[inline]
#[must_use]
pub const fn words_for_dim(d: usize) -> usize {
    (d + 63) / 64
}

/// § Read-only trait abstracting raw `u64`-word access for hypervectors
///   of any flavor (compile-time `D`, runtime-length, bipolar-i8). The
///   bind / bundle / similarity surface is generic over this trait so
///   future slices can add new hypervector shapes without rewriting the
///   algebra.
///
/// The trait method names are prefixed `ham_*` to avoid collision with
/// the inherent methods on [`Hypervector<D>`] — callers that want trait-
/// polymorphic access import the trait into scope and call e.g.
/// `hv.ham_word(i)`. Callers operating on a concrete type continue to
/// use the inherent `hv.word(i)` directly.
pub trait HamFn {
    /// § Dimension count.
    fn ham_dim(&self) -> usize;

    /// § Number of `u64` words.
    fn ham_word_count(&self) -> usize;

    /// § Read word `i`.
    fn ham_word(&self, i: usize) -> u64;

    /// § Slice view of the words array.
    fn ham_words(&self) -> &[u64];
}

/// § Const-dimension hypervector. Storage is a heap-allocated `Box<[u64]>`
///   of length `words_for_dim(D)`. The const-generic `D` is the dimension
///   count ; we deliberately do NOT use `feature(generic_const_exprs)` to
///   express `[u64; (D + 63) / 64]` because that gates MSRV-1.75
///   compatibility. The Box-backed form is sufficient for the foundation
///   slice — every construction site goes through the constructors which
///   know `words_for_dim(D)` at runtime.
///
/// For the genome-specific D = 10000 case the [`Genome`] type provides
/// a fixed-array specialization that is `Copy` and stack-resident.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hypervector<const D: usize> {
    /// § Word-packed bit-storage. Length is `words_for_dim(D)`. The
    ///   tail bits in the last word are zero (invariant).
    words: Box<[u64]>,
}

impl<const D: usize> Hypervector<D> {
    /// § Construct an all-zero hypervector. Bit-wise this is the bipolar
    ///   `[-1; D]` vector — useful as the additive identity for bundling.
    #[must_use]
    pub fn zero() -> Self {
        let n = words_for_dim(D);
        Self {
            words: vec![0u64; n].into_boxed_slice(),
        }
    }

    /// § Construct an all-ones hypervector. Bit-wise this is the bipolar
    ///   `[+1; D]` vector. Tail bits are zeroed to maintain invariant.
    #[must_use]
    pub fn ones() -> Self {
        let n = words_for_dim(D);
        let mut words = vec![u64::MAX; n].into_boxed_slice();
        // § Mask the tail bits in the last word. If `D % 64 == 0` the
        //   shift-by-64 would be UB on `u64::shr` ; we guard with the
        //   `if` branch so the shift is always in `0..64`.
        let tail_bits = D % 64;
        if tail_bits != 0 && n > 0 {
            let mask = (1u64 << tail_bits) - 1;
            words[n - 1] &= mask;
        }
        Self { words }
    }

    /// § Construct a hypervector with bits chosen pseudo-randomly from
    ///   the given seed. The PRNG is splitmix64 ; same seed ⇒ same
    ///   hypervector byte-for-byte across runs / hosts / toolchains.
    ///
    /// Each `u64` word is a single splitmix64 draw, so the entropy is
    /// 64 bits per word. With `D = 10000` we draw 157 words ≈ 10000
    /// independent bits of randomness — exactly matching the dimension.
    #[must_use]
    pub fn random_from_seed(seed: u64) -> Self {
        let n = words_for_dim(D);
        let mut state = seed;
        let mut words = vec![0u64; n].into_boxed_slice();
        for w in words.iter_mut() {
            // § Match SplitMix64 stream order : advance state by golden-
            //   ratio constant before mixing. This keeps the
            //   `random_from_seed` output identical to constructing a
            //   `SplitMix64` and calling `next()` repeatedly.
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            *w = splitmix64_next(state);
        }
        // § Canonicalize tail.
        let tail_bits = D % 64;
        if tail_bits != 0 && n > 0 {
            let mask = (1u64 << tail_bits) - 1;
            words[n - 1] &= mask;
        }
        Self { words }
    }

    /// § Construct from a raw `Vec<u64>`. Caller must supply exactly
    ///   `words_for_dim(D)` words ; this asserts. Tail-bits are masked
    ///   to maintain the invariant — callers do not need to pre-mask.
    pub fn from_words(mut words: Vec<u64>) -> Self {
        let n = words_for_dim(D);
        assert_eq!(
            words.len(),
            n,
            "Hypervector<{D}>::from_words : expected {n} words, got {}",
            words.len()
        );
        let tail_bits = D % 64;
        if tail_bits != 0 && n > 0 {
            let mask = (1u64 << tail_bits) - 1;
            words[n - 1] &= mask;
        }
        Self {
            words: words.into_boxed_slice(),
        }
    }

    /// § Read bit `i` as a `bool`. `i ∈ 0..D` ; out-of-range returns
    ///   `false` rather than panicking — match the binary form's
    ///   "tail bits are zero" semantics extended to the conceptual tail.
    #[must_use]
    pub fn bit(&self, i: usize) -> bool {
        if i >= D {
            return false;
        }
        let word_idx = i / 64;
        let bit_idx = i % 64;
        (self.words[word_idx] >> bit_idx) & 1 == 1
    }

    /// § Write bit `i`. `i ∈ 0..D` ; out-of-range silently ignored to
    ///   match the read-side semantics. Single-bit writes do not break
    ///   the tail-zero invariant because `D % 64` bounds the writable
    ///   range below the masked tail.
    pub fn set_bit(&mut self, i: usize, value: bool) {
        if i >= D {
            return;
        }
        let word_idx = i / 64;
        let bit_idx = i % 64;
        let mask = 1u64 << bit_idx;
        if value {
            self.words[word_idx] |= mask;
        } else {
            self.words[word_idx] &= !mask;
        }
    }

    /// § Re-canonicalize the tail bits. Callers performing bulk word
    ///   rewrites (e.g. SIMD-XOR with another hypervector that has the
    ///   same `D`) automatically preserve the invariant — XOR of two
    ///   tail-zero words is still tail-zero. But explicit-write paths
    ///   like [`Self::from_words`] use this helper to avoid replication.
    pub fn canonicalize_tail(&mut self) {
        let n = self.words.len();
        let tail_bits = D % 64;
        if tail_bits != 0 && n > 0 {
            let mask = (1u64 << tail_bits) - 1;
            self.words[n - 1] &= mask;
        }
    }

    /// § Population count — number of `1` bits in the binary form, i.e.
    ///   number of `+1` entries in the bipolar form. Used as a
    ///   denominator for some similarity metrics.
    #[must_use]
    pub fn popcount(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    // § Inherent forwards — these mirror the [`HamFn`] trait methods so
    //   in-crate callers don't need to `use HamFn` for raw word access.
    //   The trait is retained for generic-over-`HamFn` callers (the
    //   bind/bundle/similarity surface that wants to be polymorphic
    //   over const-D + dyn-D in a future slice).

    /// § Inherent : number of u64 words.
    #[inline]
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    /// § Inherent : read u64 word at index `i`.
    #[inline]
    #[must_use]
    pub fn word(&self, i: usize) -> u64 {
        self.words[i]
    }

    /// § Inherent : mutable u64 word at index `i`.
    #[inline]
    pub fn word_mut(&mut self, i: usize) -> &mut u64 {
        &mut self.words[i]
    }

    /// § Inherent : slice view of all u64 words.
    #[inline]
    #[must_use]
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// § Inherent : mutable slice view.
    #[inline]
    pub fn words_mut(&mut self) -> &mut [u64] {
        &mut self.words
    }

    /// § Inherent : dimension count.
    #[inline]
    #[must_use]
    pub fn dim_count(&self) -> usize {
        D
    }
}

impl<const D: usize> HamFn for Hypervector<D> {
    #[inline]
    fn ham_dim(&self) -> usize {
        D
    }

    #[inline]
    fn ham_word_count(&self) -> usize {
        self.words.len()
    }

    #[inline]
    fn ham_word(&self, i: usize) -> u64 {
        self.words[i]
    }

    #[inline]
    fn ham_words(&self) -> &[u64] {
        &self.words
    }
}

/// § Bipolar `i8` hypervector — one byte per dimension, value in
///   `{-1, +1}`. Used by KAN-network input encoders that operate on
///   continuous-valued bipolar samples and need direct addressability
///   without bit-unpacking. Conversion to / from binary [`Hypervector<D>`]
///   is exposed via [`Self::to_binary`] and [`Self::from_binary`].
///
/// Storage is a `Vec<i8>` of length `D` (no const-generic array because
/// even `D = 10000` is well under any heap-fragmentation concern, and a
/// 10000-byte stack value would push past typical stack budgets).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HypervectorI8 {
    values: Vec<i8>,
}

impl HypervectorI8 {
    /// § Construct an all-`-1` bipolar hypervector of dimension `d`.
    #[must_use]
    pub fn neg_ones(d: usize) -> Self {
        Self {
            values: vec![-1i8; d],
        }
    }

    /// § Construct an all-`+1` bipolar hypervector.
    #[must_use]
    pub fn pos_ones(d: usize) -> Self {
        Self {
            values: vec![1i8; d],
        }
    }

    /// § Construct random bipolar hypervector at `d`-dim from a seed.
    ///   Each entry is a single-bit draw projected to `{-1, +1}` so the
    ///   distribution is uniform.
    #[must_use]
    pub fn random_from_seed(d: usize, seed: u64) -> Self {
        let mut values = Vec::with_capacity(d);
        let mut state = seed;
        let mut buffer = 0u64;
        let mut buffer_bits = 0u32;
        for _ in 0..d {
            if buffer_bits == 0 {
                state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
                buffer = splitmix64_next(state);
                buffer_bits = 64;
            }
            let bit = (buffer & 1) as i8;
            buffer >>= 1;
            buffer_bits -= 1;
            values.push(if bit == 0 { -1 } else { 1 });
        }
        Self { values }
    }

    /// § Convert to packed-binary [`Hypervector`] of compatible `D`.
    ///   Asserts `D == self.values.len()`.
    pub fn to_binary<const D: usize>(&self) -> Hypervector<D> {
        assert_eq!(
            self.values.len(),
            D,
            "HypervectorI8::to_binary : dim mismatch (i8 dim {} ; binary dim {D})",
            self.values.len()
        );
        let n = words_for_dim(D);
        let mut words = vec![0u64; n];
        for (i, &v) in self.values.iter().enumerate() {
            // § Bipolar +1 ⇒ binary 1 ; bipolar -1 ⇒ binary 0.
            //   Other values would be a logic-bug — we assert them out
            //   in debug to catch construction errors early.
            debug_assert!(v == 1 || v == -1, "non-bipolar entry {v} at index {i}");
            if v == 1 {
                let word_idx = i / 64;
                let bit_idx = i % 64;
                words[word_idx] |= 1u64 << bit_idx;
            }
        }
        Hypervector::from_words(words)
    }

    /// § Construct from a packed-binary [`Hypervector`].
    pub fn from_binary<const D: usize>(hv: &Hypervector<D>) -> Self {
        let mut values = Vec::with_capacity(D);
        for i in 0..D {
            values.push(if hv.bit(i) { 1 } else { -1 });
        }
        Self { values }
    }

    /// § Read entry `i` ; returns `0` for out-of-range to match the
    ///   binary `bit()` semantics (out-of-range reads as bipolar-zero
    ///   in the implicit-tail).
    #[must_use]
    pub fn entry(&self, i: usize) -> i8 {
        self.values.get(i).copied().unwrap_or(0)
    }

    /// § Slice view.
    #[must_use]
    pub fn as_slice(&self) -> &[i8] {
        &self.values
    }

    /// § Mutable slice view.
    pub fn as_mut_slice(&mut self) -> &mut [i8] {
        &mut self.values
    }

    /// § Dimension count.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.values.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_for_dim_known() {
        assert_eq!(words_for_dim(64), 1);
        assert_eq!(words_for_dim(65), 2);
        assert_eq!(words_for_dim(128), 2);
        assert_eq!(words_for_dim(10000), 157);
        assert_eq!(words_for_dim(0), 0);
        assert_eq!(words_for_dim(1), 1);
    }

    #[test]
    fn zero_constructor_invariant() {
        let z: Hypervector<10000> = Hypervector::zero();
        assert_eq!(z.popcount(), 0);
        assert_eq!(z.words().len(), 157);
    }

    #[test]
    fn ones_constructor_invariant() {
        let o: Hypervector<10000> = Hypervector::ones();
        assert_eq!(o.popcount(), 10000);
        // § Tail-zero invariant : 157 * 64 = 10048, so 48 high bits of
        //   word 156 must be zero. Word 156 should have exactly the
        //   low-16 bits set.
        let last = o.word(156);
        assert_eq!(last, (1u64 << 16) - 1);
    }

    #[test]
    fn random_from_seed_is_deterministic() {
        let a: Hypervector<1024> = Hypervector::random_from_seed(0xCAFE_F00D);
        let b: Hypervector<1024> = Hypervector::random_from_seed(0xCAFE_F00D);
        assert_eq!(a, b);
    }

    #[test]
    fn random_from_seed_different_seeds_differ() {
        let a: Hypervector<1024> = Hypervector::random_from_seed(0);
        let b: Hypervector<1024> = Hypervector::random_from_seed(1);
        assert_ne!(a, b);
    }

    #[test]
    fn random_popcount_balanced() {
        // § For D = 10000 random bits, popcount ≈ 5000 ± 50 (3σ ≈ 150).
        let h: Hypervector<10000> = Hypervector::random_from_seed(0xFEED);
        let pc = h.popcount();
        assert!(
            (4500..=5500).contains(&pc),
            "popcount unbalanced : {pc} / 10000"
        );
    }

    #[test]
    fn bit_get_set_roundtrip() {
        let mut h: Hypervector<128> = Hypervector::zero();
        h.set_bit(0, true);
        h.set_bit(63, true);
        h.set_bit(64, true);
        h.set_bit(127, true);
        assert!(h.bit(0));
        assert!(!h.bit(1));
        assert!(h.bit(63));
        assert!(h.bit(64));
        assert!(h.bit(127));
        // § Out-of-range is `false`.
        assert!(!h.bit(128));
        assert!(!h.bit(usize::MAX));
    }

    #[test]
    fn from_words_canonicalizes_tail() {
        // § D = 100, so 1 valid bit + 28 invalid (in word 1's high
        //   bits). Caller passes garbage tail bits ; constructor must
        //   mask them away.
        let mut words = vec![0u64; 2];
        words[0] = u64::MAX; // 64 bits set
        words[1] = u64::MAX; // 64 bits — but only low 36 are valid
        let h: Hypervector<100> = Hypervector::from_words(words);
        // § popcount = 64 (word 0) + 36 (word 1 masked) = 100.
        assert_eq!(h.popcount(), 100);
        // § Tail bits zero.
        assert_eq!(h.word(1) & !((1u64 << 36) - 1), 0);
    }

    #[test]
    fn i8_random_is_bipolar() {
        let h = HypervectorI8::random_from_seed(1024, 0xABCD);
        for &v in h.as_slice() {
            assert!(v == 1 || v == -1);
        }
        assert_eq!(h.dim(), 1024);
    }

    #[test]
    fn i8_to_binary_roundtrip() {
        let original = HypervectorI8::random_from_seed(256, 0xBEEF);
        let binary: Hypervector<256> = original.to_binary();
        let recovered = HypervectorI8::from_binary(&binary);
        assert_eq!(original, recovered);
    }

    #[test]
    fn i8_neg_pos_constructors() {
        let neg = HypervectorI8::neg_ones(100);
        let pos = HypervectorI8::pos_ones(100);
        assert!(neg.as_slice().iter().all(|&v| v == -1));
        assert!(pos.as_slice().iter().all(|&v| v == 1));
    }

    #[test]
    fn i8_entry_out_of_range() {
        let h = HypervectorI8::pos_ones(100);
        assert_eq!(h.entry(99), 1);
        assert_eq!(h.entry(100), 0);
        assert_eq!(h.entry(usize::MAX), 0);
    }

    #[test]
    fn ham_fn_word_access() {
        let h: Hypervector<128> = Hypervector::ones();
        assert_eq!(h.dim_count(), 128);
        assert_eq!(h.word_count(), 2);
        assert_eq!(h.word(0), u64::MAX);
        assert_eq!(h.word(1), u64::MAX);
    }

    #[test]
    fn ham_fn_trait_access() {
        let h: Hypervector<128> = Hypervector::ones();
        assert_eq!(h.ham_dim(), 128);
        assert_eq!(h.ham_word_count(), 2);
        assert_eq!(h.ham_word(0), u64::MAX);
        assert_eq!(h.ham_words().len(), 2);
    }
}
