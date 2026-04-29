//! § Pattern — substrate-invariant identity carrier (Phi'Pattern)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The substrate-invariant identity carrier that
//!   `02_CSSL/06_SUBSTRATE_EVOLUTION.csl` references throughout :
//!
//!   - `§ 1`  — `pattern_handle: Handle<Phi'Pattern>` field of `FieldCell`.
//!   - `§ 3`  — `phi_table: AppendOnlyPool<Phi'Pattern>` field of `OmegaField`.
//!   - `§ 5`  — `pattern_link: Option<Handle<Phi'Pattern>>` of `KanMaterial`.
//!   - `§ 6`  — `pattern_link: Handle<Phi'Pattern>` of `BodyOmnoid` and
//!             `MachineLayer`.
//!   - `§ 9`  — `pattern: Handle<Phi'Pattern>` of `ThroatRecord`.
//!
//!   Together with `Handle<Phi'Pattern>` and `AppendOnlyPool<Pattern>`,
//!   this type defines the Φ-FACET layer of the substrate runtime — the
//!   layer that lets identity survive substrate-translation
//!   (`08_BODY/03_DIMENSIONAL_TRAVEL.csl`).
//!
//! § PATTERN SHAPE
//!   ```text
//!   pub struct Pattern {
//!     fingerprint:        PatternFingerprint,    // 32-byte blake3
//!     genome_signature:   [u8; 32],              // ref to Genome.signature
//!     genome_generation:  u64,                   // Genome.generation
//!     genome_distance_marker: u32,               // Hamming-distance-from-zero (HDC)
//!     kan_weights_digest: [u8; 32],              // KanGenomeWeights.fingerprint_bytes
//!     substrate_class_tag: SubstrateClassTag,    // origin substrate-class
//!     stamp_epoch:         u64,                  // monotonic stamp counter
//!   }
//!   ```
//!
//!   The Pattern record carries enough to :
//!   - **Fingerprint** : 256-bit blake3 over (genome-signature || kan-digest ||
//!     substrate-tag || stamp-epoch). This is THE substrate-invariant —
//!     `Pattern.fingerprint` does NOT change across `translate_to_substrate`.
//!   - **Resolve genome** : the `genome_signature` is the byte-stable hash of
//!     the source `Genome` ; downstream code can look it up in a genome-pool.
//!     The marker fields make round-trip tests practical without requiring
//!     the full 1.25 KiB hypervector to be embedded.
//!   - **Verify substrate-translation** : the round-trip test (`§ V`) compares
//!     `before.fingerprint == after.fingerprint` ; this passes by construction
//!     because translation does NOT touch the fingerprint.
//!
//! § STAMP DISCIPLINE
//!   `Pattern::stamp(genome, weights, tag) -> Pattern` is the canonical
//!   constructor. It :
//!   1. Computes the 32-byte fingerprint over the four canonical sources
//!      (genome-signature, kan-digest, substrate-tag, stamp-epoch).
//!   2. Returns a fully-populated Pattern record.
//!   The stamp_epoch is sourced from a `&mut` epoch counter the caller
//!   threads (so the test paths can pin it deterministically) ; production
//!   call-sites use `OmegaField`'s monotonic epoch counter.
//!
//! § IMMUTABILITY POST-STAMP
//!   The fields of `Pattern` are public for read access (matching the
//!   substrate spec's `@layout(soa)` declaration which exposes facet-fields
//!   directly). But the type as a whole is treated as immutable post-stamp
//!   by the runtime : every consumer takes `&Pattern` and never mutates.
//!   The `AppendOnlyPool` discipline mechanically enforces this — the pool
//!   has no `&mut Pattern` accessor.

use crate::handle::Handle;
use crate::kan_genome_weights::KanGenomeWeights;
use cssl_hdc::genome::Genome;

/// § The 256-bit blake3 fingerprint that uniquely identifies a Pattern.
///
/// This is the load-bearing substrate-invariant : `translate_to_substrate`
/// guarantees `before.fingerprint == after.fingerprint` (Axiom-2 substrate-
/// relativity invariant + `08_BODY/03_DIMENSIONAL_TRAVEL.csl § V` round-trip
/// test).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PatternFingerprint(pub [u8; 32]);

impl PatternFingerprint {
    /// § The all-zero fingerprint. Reserved as the NULL fingerprint ; a
    ///   stamped Pattern's fingerprint MUST NOT equal this (the stamp
    ///   epoch counter is XOR-mixed in to ensure non-zero output even
    ///   for the zero-genome / zero-weights edge case).
    pub const NULL: Self = Self([0u8; 32]);

    /// § True if this fingerprint is the NULL sentinel.
    #[must_use]
    pub fn is_null(self) -> bool {
        self.0 == [0u8; 32]
    }

    /// § Render as lowercase hex (64 chars). Used by audit-log paths.
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// § Substrate-class tag — coarse-grained substrate-family discriminator.
///
/// The full substrate-class taxonomy is owned by the upcoming
/// `cssl-substrate-omega-field` crate (`§ 9` `enum ManifoldClass`). This
/// crate supplies a coarse-grained tag suitable for Pattern provenance :
/// it identifies which substrate-family the Pattern was originally stamped
/// in, NOT which specific manifold. That coarseness is intentional :
/// `translate_to_substrate` must not invalidate the Pattern fingerprint
/// when crossing manifold boundaries within the same family, but a
/// material that's only meaningful in one substrate-family (e.g. a
/// plasma-specific Pattern) can carry that constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum SubstrateClassTag {
    /// § Universal — admissible in every substrate. Default.
    #[default]
    Universal = 0,
    /// § Euclidean / Riemannian / Lorentzian — the "classical" family.
    Classical = 1,
    /// § Spinor / Twistor — fermionic / topological-charge-bearing.
    Spinor = 2,
    /// § Toroidal / Klein / Mobius / Quotient — periodic / non-orientable.
    Periodic = 3,
    /// § Plasma / Energy — non-discretized excitation field.
    Plasma = 4,
    /// § Custom — caller supplies a tag in the high range. Tags ≥ 256 are
    ///   reserved for downstream substrate-extensions ; values 5..256 are
    ///   reserved for future spec growth.
    Custom256 = 256,
}

impl SubstrateClassTag {
    /// § Encode as u32 — used in Pattern fingerprint computation and the
    ///   wire format.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// § Errors returned by [`Pattern::stamp`] paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternStampError {
    /// § The provided inputs would produce a Pattern whose fingerprint
    ///   collides with the NULL sentinel ([`PatternFingerprint::NULL`]).
    ///
    ///   This can only happen in synthetic test-only code paths because
    ///   the real-flow inputs (`KanGenomeWeights::new_untrained` has a
    ///   non-zero domain-separation prefix in its digest, even when its
    ///   networks are zero-valued) never produce a NULL output.
    ///
    ///   Caller must perturb at least one input (genome / weights /
    ///   substrate-tag / epoch).
    DegenerateInputs,
}

impl core::fmt::Display for PatternStampError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DegenerateInputs => write!(
                f,
                "Pattern::stamp refused : all-zero genome + all-zero kan-weights + Universal-tag would collide with NULL fingerprint"
            ),
        }
    }
}

/// § Pattern — the substrate-invariant identity carrier (Phi'Pattern).
///
/// See module-level docs for shape + invariants. Public fields are
/// read-mostly ; the runtime stores Pattern records in
/// [`AppendOnlyPool<Pattern>`](crate::pool::AppendOnlyPool) and never
/// hands out `&mut Pattern`.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// § The 256-bit blake3 fingerprint. Substrate-invariant.
    pub fingerprint: PatternFingerprint,
    /// § Byte-stable hash of the source Genome's signature field.
    pub genome_signature: [u8; 32],
    /// § The Genome's generation counter at stamp-time.
    pub genome_generation: u64,
    /// § Hamming distance from the all-zero hypervector (an HDC marker).
    ///   Useful for round-trip verification when the full hypervector is
    ///   not available.
    pub genome_distance_marker: u32,
    /// § Byte-stable digest of the Genome's KanGenomeWeights bundle.
    pub kan_weights_digest: [u8; 32],
    /// § Substrate-class tag at stamp-time. Origin substrate ; the Pattern
    ///   may translate into other substrates that admit the tag.
    pub substrate_class_tag: SubstrateClassTag,
    /// § Stamp epoch — monotonic counter at stamp-time. Used to break ties
    ///   when the same genome+weights+tag stamps twice in the same field.
    pub stamp_epoch: u64,
}

impl Pattern {
    /// § Stamp a new Pattern from the canonical inputs. Returns the fully-
    ///   populated record ; the caller passes it into the pool via
    ///   [`crate::PhiTable::stamp`] (which calls this internally).
    ///
    ///   The `stamp_epoch` is monotonic ; production call-sites supply the
    ///   `OmegaField` epoch counter, test-only call-sites can pass any
    ///   distinct value.
    pub fn stamp(
        genome: &Genome,
        weights: &KanGenomeWeights,
        substrate_class_tag: SubstrateClassTag,
        stamp_epoch: u64,
    ) -> Result<Pattern, PatternStampError> {
        // § Source the genome's signature ; for this slice we re-derive
        //   it from the genome's HDC content rather than depending on the
        //   creature-genome crate's `signature` field (which lives in a
        //   separate dispatch). The signature here is blake3 over the
        //   hypervector words, matching what `cssl-creature-genome` will
        //   compute when it lands.
        let mut sig_h = blake3::Hasher::new();
        sig_h.update(b"cssl-substrate-kan/Pattern/genome-signature/v1");
        for w in genome.hdc().words() {
            sig_h.update(&w.to_le_bytes());
        }
        let genome_signature = {
            let d = sig_h.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(d.as_bytes());
            out
        };

        let genome_distance_marker = genome.hdc().popcount();

        let kan_weights_digest = weights.fingerprint_bytes();

        // § Compose the fingerprint. The composition order is canonical
        //   and load-bearing : signature → kan-digest → tag → epoch.
        //   Changing the order would invalidate every existing Pattern
        //   fingerprint downstream — wire-format-stable.
        let mut fp_h = blake3::Hasher::new();
        fp_h.update(b"cssl-substrate-kan/Pattern/fingerprint/v1");
        fp_h.update(&genome_signature);
        fp_h.update(&kan_weights_digest);
        fp_h.update(&substrate_class_tag.as_u32().to_le_bytes());
        fp_h.update(&stamp_epoch.to_le_bytes());
        let fp = {
            let d = fp_h.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(d.as_bytes());
            PatternFingerprint(out)
        };

        // § Refuse iff the resulting fingerprint coincides with the NULL
        //   sentinel — astronomically unlikely (probability 2^-256) but
        //   the post-hash check is cheap and the alternative would be a
        //   silent collision with `Handle::NULL` semantics.
        if fp.is_null() {
            return Err(PatternStampError::DegenerateInputs);
        }

        Ok(Pattern {
            fingerprint: fp,
            genome_signature,
            genome_generation: 0, // § filled by caller from Genome.generation at integration time
            genome_distance_marker,
            kan_weights_digest,
            substrate_class_tag,
            stamp_epoch,
        })
    }

    /// § Override the genome-generation field. The base [`stamp`] entry-
    ///   point defaults this to `0` because cssl-hdc's `Genome` does not
    ///   yet expose a `.generation()` accessor (it lives in the upcoming
    ///   `cssl-creature-genome` crate). Call-sites that have the
    ///   generation in hand attach it via this builder method.
    #[must_use]
    pub fn with_generation(mut self, gen: u64) -> Self {
        self.genome_generation = gen;
        self
    }

    /// § Re-emit a Pattern under a different substrate-class tag.
    ///   Used by `KanMaterial::translate_to_substrate` : the runtime
    ///   may need to register the same Pattern in a target substrate's
    ///   Φ-table under a different tag, but the fingerprint must NOT
    ///   change. This method preserves the fingerprint exactly — it
    ///   is the canonical "preserve fingerprint, change tag" path.
    ///
    ///   NOTE : the resulting Pattern's `substrate_class_tag` field is
    ///   updated, but the `fingerprint` is not recomputed. This is the
    ///   substrate-invariance contract.
    #[must_use]
    pub fn re_tag(mut self, new_tag: SubstrateClassTag) -> Self {
        self.substrate_class_tag = new_tag;
        self
    }

    /// § Convenience : the fingerprint as a 32-byte array.
    #[must_use]
    pub fn fingerprint_bytes(&self) -> [u8; 32] {
        self.fingerprint.0
    }
}

/// § Convenience handle alias — `Handle<Pattern>` named to match the spec's
///   `Handle<Phi'Pattern>` literal.
pub type PatternHandle = Handle<Pattern>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kan_genome_weights::KanGenomeWeights;
    use cssl_hdc::genome::Genome;

    /// § stamp produces a Pattern with non-NULL fingerprint.
    #[test]
    fn stamp_non_null_fingerprint() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
        assert!(!p.fingerprint.is_null());
    }

    /// § stamp is deterministic.
    #[test]
    fn stamp_deterministic() {
        let g = Genome::from_seed(42);
        let w = KanGenomeWeights::new_untrained();
        let a = Pattern::stamp(&g, &w, SubstrateClassTag::Classical, 7).unwrap();
        let b = Pattern::stamp(&g, &w, SubstrateClassTag::Classical, 7).unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
    }

    /// § stamp sensitive to genome.
    #[test]
    fn stamp_sensitive_to_genome() {
        let g1 = Genome::from_seed(1);
        let g2 = Genome::from_seed(2);
        let w = KanGenomeWeights::new_untrained();
        let p1 = Pattern::stamp(&g1, &w, SubstrateClassTag::Universal, 1).unwrap();
        let p2 = Pattern::stamp(&g2, &w, SubstrateClassTag::Universal, 1).unwrap();
        assert_ne!(p1.fingerprint, p2.fingerprint);
    }

    /// § stamp sensitive to weights.
    #[test]
    fn stamp_sensitive_to_weights() {
        let g = Genome::from_seed(1);
        let w1 = KanGenomeWeights::new_untrained();
        let mut w2 = KanGenomeWeights::new_untrained();
        w2.body_kan.control_points[0][0] = 1.0;
        let p1 = Pattern::stamp(&g, &w1, SubstrateClassTag::Universal, 1).unwrap();
        let p2 = Pattern::stamp(&g, &w2, SubstrateClassTag::Universal, 1).unwrap();
        assert_ne!(p1.fingerprint, p2.fingerprint);
    }

    /// § stamp sensitive to substrate-class tag.
    #[test]
    fn stamp_sensitive_to_tag() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let p_uni = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
        let p_pla = Pattern::stamp(&g, &w, SubstrateClassTag::Plasma, 1).unwrap();
        assert_ne!(p_uni.fingerprint, p_pla.fingerprint);
    }

    /// § stamp sensitive to epoch.
    #[test]
    fn stamp_sensitive_to_epoch() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let a = Pattern::stamp(&g, &w, SubstrateClassTag::Classical, 1).unwrap();
        let b = Pattern::stamp(&g, &w, SubstrateClassTag::Classical, 2).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    /// § stamp captures genome distance marker.
    #[test]
    fn stamp_captures_distance_marker() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
        // § random-seed genome popcount ≈ HDC_DIM / 2 = 5000 ± few hundred.
        assert!(
            (3000..=7000).contains(&p.genome_distance_marker),
            "popcount = {}",
            p.genome_distance_marker
        );
    }

    /// § Stamping with a zero-genome succeeds (no degeneracy under realistic
    ///   inputs because the KanGenomeWeights digest has its own domain-
    ///   separation prefix — see Pattern::stamp rustdoc).
    #[test]
    fn stamp_accepts_zero_genome() {
        let g = Genome::zero();
        let w = KanGenomeWeights::new_untrained();
        let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 0).unwrap();
        assert!(!p.fingerprint.is_null());
    }

    /// § stamp accepts zero-genome + non-zero-epoch.
    #[test]
    fn stamp_accepts_zero_genome_with_nonzero_epoch() {
        let g = Genome::zero();
        let w = KanGenomeWeights::new_untrained();
        let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
        assert!(!p.fingerprint.is_null());
    }

    /// § with_generation builder works.
    #[test]
    fn with_generation_builder() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1)
            .unwrap()
            .with_generation(42);
        assert_eq!(p.genome_generation, 42);
    }

    /// § re_tag preserves fingerprint.
    #[test]
    fn re_tag_preserves_fingerprint() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let p = Pattern::stamp(&g, &w, SubstrateClassTag::Classical, 1).unwrap();
        let fp_before = p.fingerprint;
        let q = p.re_tag(SubstrateClassTag::Plasma);
        assert_eq!(q.fingerprint, fp_before);
        assert_eq!(q.substrate_class_tag, SubstrateClassTag::Plasma);
    }

    /// § PatternFingerprint hex round-trip.
    #[test]
    fn fingerprint_hex_format() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
        let h = p.fingerprint.to_hex();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// § SubstrateClassTag default is Universal.
    #[test]
    fn substrate_class_tag_default() {
        assert_eq!(SubstrateClassTag::default(), SubstrateClassTag::Universal);
    }

    /// § SubstrateClassTag as_u32 stable.
    #[test]
    fn substrate_class_tag_u32() {
        assert_eq!(SubstrateClassTag::Universal.as_u32(), 0);
        assert_eq!(SubstrateClassTag::Classical.as_u32(), 1);
        assert_eq!(SubstrateClassTag::Spinor.as_u32(), 2);
        assert_eq!(SubstrateClassTag::Periodic.as_u32(), 3);
        assert_eq!(SubstrateClassTag::Plasma.as_u32(), 4);
        assert_eq!(SubstrateClassTag::Custom256.as_u32(), 256);
    }

    /// § PatternFingerprint::NULL is null.
    #[test]
    fn null_fingerprint_is_null() {
        let n = PatternFingerprint::NULL;
        assert!(n.is_null());
    }

    /// § Two stamps with different epochs of the same genome have
    ///   identical genome_signature but different fingerprints.
    #[test]
    fn epoch_differentiates_same_genome() {
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let p1 = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
        let p2 = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 2).unwrap();
        assert_eq!(p1.genome_signature, p2.genome_signature);
        assert_ne!(p1.fingerprint, p2.fingerprint);
    }
}
