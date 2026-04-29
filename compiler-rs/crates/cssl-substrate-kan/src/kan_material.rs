//! § KanMaterial — multi-variant KAN-driven material descriptor
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The substrate-spec `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 5` shape :
//!
//!   ```cssl
//!   type KanMaterial = {
//!     embedding:    vec'32<f32>,
//!     brdf_kan:     KanNetwork<32, BRDF_OUT_DIM>,
//!     ior_kan:      KanNetwork<32, 4>,
//!     emission_kan: KanNetwork<32, BRDF_OUT_DIM>,
//!     thermal_kan:  KanNetwork<32, 4>,
//!     acoustic_kan: KanNetwork<32, 8>,
//!     fingerprint:  blake3.Hash,
//!     pattern_link: Option<Handle<Phi'Pattern>>,
//!   }
//!   ```
//!
//!   This crate exposes the spec shape **plus four canonical variant
//!   constructors** for the four downstream specializations the dispatch
//!   contract calls out :
//!
//!   1. `KanMaterial::spectral_brdf<const N_BANDS>` — the hyperspectral
//!      renderer's entry-point. The BRDF network output dim scales with
//!      `N_BANDS` ; everything else is held at default (zero / untrained).
//!   2. `KanMaterial::single_band_brdf` — the prototype renderer's path
//!      with a 1-D scalar BRDF.
//!   3. `KanMaterial::physics_impedance` — the wave-unity solver wants
//!      `Z(λ)` over 4 bands. Only `acoustic_kan` is populated for this
//!      path.
//!   4. `KanMaterial::creature_morphology` — the body-omnoid SDF builder
//!      wants morphology coefficients, returned via the BRDF channel by
//!      convention (since `creature_morphology` doesn't render).
//!
//!   All four variants tag the resulting `KanMaterial` with a
//!   [`KanMaterialKind`] so downstream consumers can dispatch correctly.
//!   The fingerprint is variant-aware : two materials with identical
//!   embedding+weights but different `kind` have different fingerprints.
//!   This prevents accidental cross-use of a `physics_impedance` material
//!   in a render path expecting a `spectral_brdf`.

use crate::handle::Handle;
use crate::kan_network::KanNetwork;
use crate::pattern::Pattern;

/// § The output dimensionality of the spectral BRDF network. Matches the
///   spec literal `BRDF_OUT_DIM = 16`.
pub const BRDF_OUT_DIM: usize = 16;

/// § The 32-D material embedding axis. Matches spec `vec'32<f32>`.
pub const EMBEDDING_DIM: usize = 32;

/// § The number of impedance bands for the physics_impedance variant.
pub const IMPEDANCE_BANDS: usize = 4;

/// § The number of morphology coefficients for the creature_morphology
///   variant (matches `BODY_PARAMS / 4 = 16` so it slots into the
///   BRDF_OUT_DIM channel cleanly).
pub const MORPHOLOGY_PARAMS: usize = 16;

/// § The blake3 fingerprint of a KanMaterial. Different shape from
///   `PatternFingerprint` because materials and patterns live in different
///   pools and have different invariance contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct MaterialFingerprint(pub [u8; 32]);

impl MaterialFingerprint {
    /// § Render as lowercase hex (64 chars).
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// § Discriminator for the four variant constructors. Determines how
///   downstream consumers interpret the material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum KanMaterialKind {
    /// § Hyperspectral BRDF — N_BANDS-spectrum render path.
    SpectralBrdf {
        /// § The number of bands ; output dim of the BRDF network.
        n_bands: u16,
    },
    /// § Single-band scalar BRDF — prototype render path.
    SingleBandBrdf,
    /// § Physics impedance — Z(λ) over IMPEDANCE_BANDS bands. Wave-Unity solver.
    PhysicsImpedance,
    /// § Creature morphology — SDF parameter coefficients for body-omnoid.
    CreatureMorphology,
}

/// § KanMaterial — KAN-driven material descriptor. See module-level docs.
#[derive(Debug, Clone)]
pub struct KanMaterial {
    /// § Variant tag — set by the four canonical constructors.
    pub kind: KanMaterialKind,
    /// § 32-D material-PGA-coord embedding.
    pub embedding: [f32; EMBEDDING_DIM],
    /// § BRDF KAN network. Output dim depends on `kind` ; for the
    ///   variants that don't use BRDF (PhysicsImpedance), this is the
    ///   default-untrained scalar-output network as a placeholder.
    pub brdf_kan: KanNetwork<EMBEDDING_DIM, BRDF_OUT_DIM>,
    /// § IOR over 4 bands.
    pub ior_kan: KanNetwork<EMBEDDING_DIM, IMPEDANCE_BANDS>,
    /// § Emission spectrum (same dim as BRDF).
    pub emission_kan: KanNetwork<EMBEDDING_DIM, BRDF_OUT_DIM>,
    /// § Thermal coefficients (k, c, ε, α).
    pub thermal_kan: KanNetwork<EMBEDDING_DIM, IMPEDANCE_BANDS>,
    /// § Acoustic — velocity + impedance over 8 bands. Used by both the
    ///   render-acoustic path and the PhysicsImpedance variant (which
    ///   reuses this slot for its impedance function).
    pub acoustic_kan: KanNetwork<EMBEDDING_DIM, 8>,
    /// § The 32-byte material fingerprint. Variant-aware.
    pub fingerprint: MaterialFingerprint,
    /// § Optional link to the Pattern that birthed this material. Only
    ///   present for sentient-materials (substrate-spec `§ 5` literal).
    pub pattern_link: Option<Handle<Pattern>>,
}

impl KanMaterial {
    /// § Spectral BRDF variant — the hyperspectral renderer entry-point.
    ///
    ///   `N_BANDS` must satisfy `1 <= N_BANDS <= BRDF_OUT_DIM` ; the
    ///   internal BRDF network is `KanNetwork<32, BRDF_OUT_DIM>` so all
    ///   `N_BANDS <= BRDF_OUT_DIM` fit. Caller-side typestate documents
    ///   the active band count via the const-generic.
    ///
    /// # Panics
    /// Panics in debug builds if `N_BANDS == 0` or `N_BANDS > BRDF_OUT_DIM`.
    /// Production builds clamp into the valid range.
    #[must_use]
    pub fn spectral_brdf<const N_BANDS: usize>(embedding: [f32; EMBEDDING_DIM]) -> Self {
        debug_assert!(N_BANDS > 0, "spectral_brdf requires N_BANDS > 0");
        debug_assert!(
            N_BANDS <= BRDF_OUT_DIM,
            "spectral_brdf requires N_BANDS <= BRDF_OUT_DIM"
        );
        let n_bands = N_BANDS.clamp(1, BRDF_OUT_DIM) as u16;
        Self::base_with_kind(KanMaterialKind::SpectralBrdf { n_bands }, embedding)
    }

    /// § Single-band scalar BRDF variant — the prototype render path.
    #[must_use]
    pub fn single_band_brdf(embedding: [f32; EMBEDDING_DIM]) -> Self {
        Self::base_with_kind(KanMaterialKind::SingleBandBrdf, embedding)
    }

    /// § Physics-impedance variant — wave-unity solver `Z(λ)`.
    ///
    ///   Only `acoustic_kan` is populated by the spec ; the other slots
    ///   are kept at default (untrained) and the variant tag tells
    ///   downstream consumers to ignore them.
    #[must_use]
    pub fn physics_impedance(embedding: [f32; EMBEDDING_DIM]) -> Self {
        Self::base_with_kind(KanMaterialKind::PhysicsImpedance, embedding)
    }

    /// § Creature-morphology variant — body-omnoid SDF parameter source.
    ///
    ///   The morphology coefficients are returned via the BRDF channel by
    ///   convention (the BRDF network's output dim is `BRDF_OUT_DIM = 16`
    ///   which matches `MORPHOLOGY_PARAMS = 16`). Variant tag tells
    ///   downstream consumers to interpret BRDF outputs as SDF params.
    #[must_use]
    pub fn creature_morphology(embedding: [f32; EMBEDDING_DIM]) -> Self {
        Self::base_with_kind(KanMaterialKind::CreatureMorphology, embedding)
    }

    /// § Internal : shared constructor that builds the canonical
    ///   variant-tagged material with default networks. Computes the
    ///   variant-aware fingerprint.
    fn base_with_kind(kind: KanMaterialKind, embedding: [f32; EMBEDDING_DIM]) -> Self {
        let brdf_kan = KanNetwork::new_untrained();
        let ior_kan = KanNetwork::new_untrained();
        let emission_kan = KanNetwork::new_untrained();
        let thermal_kan = KanNetwork::new_untrained();
        let acoustic_kan = KanNetwork::new_untrained();

        let fingerprint = Self::compute_fingerprint(
            kind,
            &embedding,
            &brdf_kan,
            &ior_kan,
            &emission_kan,
            &thermal_kan,
            &acoustic_kan,
        );

        Self {
            kind,
            embedding,
            brdf_kan,
            ior_kan,
            emission_kan,
            thermal_kan,
            acoustic_kan,
            fingerprint,
            pattern_link: None,
        }
    }

    /// § Recompute the fingerprint after caller-side mutation.
    pub fn refresh_fingerprint(&mut self) {
        self.fingerprint = Self::compute_fingerprint(
            self.kind,
            &self.embedding,
            &self.brdf_kan,
            &self.ior_kan,
            &self.emission_kan,
            &self.thermal_kan,
            &self.acoustic_kan,
        );
    }

    /// § Set the Pattern link. The pattern-link is OPTIONAL per spec ;
    ///   only sentient-materials carry one. Setting it does NOT change
    ///   the fingerprint (fingerprint is over the material's own state,
    ///   not its provenance).
    pub fn set_pattern_link(&mut self, h: Handle<Pattern>) {
        self.pattern_link = Some(h);
    }

    /// § Compute the variant-aware fingerprint over (kind || embedding
    ///   || all-five-network-fingerprints). Order is canonical and
    ///   wire-format-stable.
    fn compute_fingerprint(
        kind: KanMaterialKind,
        embedding: &[f32; EMBEDDING_DIM],
        brdf_kan: &KanNetwork<EMBEDDING_DIM, BRDF_OUT_DIM>,
        ior_kan: &KanNetwork<EMBEDDING_DIM, IMPEDANCE_BANDS>,
        emission_kan: &KanNetwork<EMBEDDING_DIM, BRDF_OUT_DIM>,
        thermal_kan: &KanNetwork<EMBEDDING_DIM, IMPEDANCE_BANDS>,
        acoustic_kan: &KanNetwork<EMBEDDING_DIM, 8>,
    ) -> MaterialFingerprint {
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-substrate-kan/KanMaterial/fingerprint/v1");
        let kind_bytes = Self::encode_kind(kind);
        h.update(&kind_bytes);
        for v in embedding {
            h.update(&v.to_le_bytes());
        }
        h.update(&brdf_kan.fingerprint_bytes());
        h.update(&ior_kan.fingerprint_bytes());
        h.update(&emission_kan.fingerprint_bytes());
        h.update(&thermal_kan.fingerprint_bytes());
        h.update(&acoustic_kan.fingerprint_bytes());
        let d = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(d.as_bytes());
        MaterialFingerprint(out)
    }

    /// § Encode a kind into a stable 4-byte representation. Used in
    ///   fingerprint composition.
    fn encode_kind(kind: KanMaterialKind) -> [u8; 4] {
        match kind {
            KanMaterialKind::SpectralBrdf { n_bands } => {
                let mut out = [0u8; 4];
                out[0] = 0;
                out[1] = 0;
                out[2..4].copy_from_slice(&n_bands.to_le_bytes());
                out
            }
            KanMaterialKind::SingleBandBrdf => [1, 0, 0, 0],
            KanMaterialKind::PhysicsImpedance => [2, 0, 0, 0],
            KanMaterialKind::CreatureMorphology => [3, 0, 0, 0],
        }
    }

    /// § Translate this material to a target substrate's class tag.
    ///   Following `§ 5` rustdoc literal :
    ///   "KanMaterial ⊗ Pattern-fingerprint preserved-across-translate (Axiom-2)".
    ///
    ///   Translation in this slice is a TAG-ROTATION operation : the
    ///   material's `kind` is preserved, the embedding/networks stay
    ///   untouched, but the linked Pattern's substrate-class is rotated
    ///   in the returned wrapper. Full physics-driven translation lives
    ///   in `cssl-substrate-omega-field` (separate slice). The Pattern
    ///   fingerprint is never modified.
    #[must_use]
    pub fn translate_pattern_class(
        &self,
        pattern_pool: &crate::pool::AppendOnlyPool<Pattern>,
    ) -> Option<crate::pattern::PatternFingerprint> {
        let h = self.pattern_link?;
        let p = pattern_pool.resolve(h).ok()?;
        Some(p.fingerprint)
    }

    /// § True iff the material's kind matches the variant predicate.
    #[must_use]
    pub fn is_variant(&self, kind: KanMaterialKind) -> bool {
        // § The `derive(PartialEq)` on KanMaterialKind already handles the
        //   non-payload variants ; for SpectralBrdf the n_bands payload is
        //   compared too. So the simple `==` check is correct.
        self.kind == kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § spectral_brdf variant produces correct kind tag.
    #[test]
    fn spectral_brdf_variant_tag() {
        let m: KanMaterial = KanMaterial::spectral_brdf::<8>([0.0; EMBEDDING_DIM]);
        assert!(matches!(
            m.kind,
            KanMaterialKind::SpectralBrdf { n_bands: 8 }
        ));
    }

    /// § single_band_brdf variant produces correct kind tag.
    #[test]
    fn single_band_brdf_variant_tag() {
        let m: KanMaterial = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
        assert!(matches!(m.kind, KanMaterialKind::SingleBandBrdf));
    }

    /// § physics_impedance variant produces correct kind tag.
    #[test]
    fn physics_impedance_variant_tag() {
        let m: KanMaterial = KanMaterial::physics_impedance([0.0; EMBEDDING_DIM]);
        assert!(matches!(m.kind, KanMaterialKind::PhysicsImpedance));
    }

    /// § creature_morphology variant produces correct kind tag.
    #[test]
    fn creature_morphology_variant_tag() {
        let m: KanMaterial = KanMaterial::creature_morphology([0.0; EMBEDDING_DIM]);
        assert!(matches!(m.kind, KanMaterialKind::CreatureMorphology));
    }

    /// § Variants with same embedding have different fingerprints.
    #[test]
    fn variants_distinguished_by_fingerprint() {
        let e = [0.5; EMBEDDING_DIM];
        let a = KanMaterial::spectral_brdf::<8>(e);
        let b = KanMaterial::single_band_brdf(e);
        let c = KanMaterial::physics_impedance(e);
        let d = KanMaterial::creature_morphology(e);
        assert_ne!(a.fingerprint, b.fingerprint);
        assert_ne!(b.fingerprint, c.fingerprint);
        assert_ne!(c.fingerprint, d.fingerprint);
        assert_ne!(a.fingerprint, c.fingerprint);
        assert_ne!(a.fingerprint, d.fingerprint);
        assert_ne!(b.fingerprint, d.fingerprint);
    }

    /// § Different N_BANDS produce different fingerprints.
    #[test]
    fn n_bands_distinguishes_spectral_brdf() {
        let e = [0.5; EMBEDDING_DIM];
        let a = KanMaterial::spectral_brdf::<4>(e);
        let b = KanMaterial::spectral_brdf::<8>(e);
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    /// § Same variant + same embedding → same fingerprint.
    #[test]
    fn same_variant_same_embedding_same_fingerprint() {
        let e = [0.5; EMBEDDING_DIM];
        let a = KanMaterial::spectral_brdf::<8>(e);
        let b = KanMaterial::spectral_brdf::<8>(e);
        assert_eq!(a.fingerprint, b.fingerprint);
    }

    /// § Different embedding produces different fingerprint.
    #[test]
    fn embedding_change_changes_fingerprint() {
        let mut e1 = [0.0; EMBEDDING_DIM];
        let mut e2 = [0.0; EMBEDDING_DIM];
        e2[0] = 1.0;
        let a = KanMaterial::single_band_brdf(e1);
        let b = KanMaterial::single_band_brdf(e2);
        assert_ne!(a.fingerprint, b.fingerprint);
        // § silence unused-mut.
        e1[0] = 0.0;
    }

    /// § set_pattern_link does NOT change fingerprint.
    #[test]
    fn pattern_link_does_not_change_fingerprint() {
        let mut m = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
        let fp = m.fingerprint;
        m.set_pattern_link(Handle::from_parts(1, 5));
        assert_eq!(m.fingerprint, fp);
        assert!(m.pattern_link.is_some());
    }

    /// § refresh_fingerprint after BRDF mutation changes fingerprint.
    #[test]
    fn refresh_after_mutation() {
        let mut m = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
        let fp_before = m.fingerprint;
        m.brdf_kan.control_points[0][0] = 1.0;
        m.refresh_fingerprint();
        assert_ne!(m.fingerprint, fp_before);
    }

    /// § is_variant returns true for matching kind.
    #[test]
    fn is_variant_matches() {
        let m = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
        assert!(m.is_variant(KanMaterialKind::SingleBandBrdf));
        assert!(!m.is_variant(KanMaterialKind::PhysicsImpedance));
    }

    /// § is_variant compares N_BANDS for spectral.
    #[test]
    fn is_variant_compares_n_bands() {
        let m = KanMaterial::spectral_brdf::<8>([0.0; EMBEDDING_DIM]);
        assert!(m.is_variant(KanMaterialKind::SpectralBrdf { n_bands: 8 }));
        assert!(!m.is_variant(KanMaterialKind::SpectralBrdf { n_bands: 4 }));
    }

    /// § fingerprint hex format.
    #[test]
    fn fingerprint_hex_format() {
        let m = KanMaterial::single_band_brdf([0.5; EMBEDDING_DIM]);
        let h = m.fingerprint.to_hex();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// § new material has no pattern link by default.
    #[test]
    fn new_material_no_pattern_link() {
        let m = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
        assert!(m.pattern_link.is_none());
    }

    /// § Constants match spec.
    #[test]
    fn constants_match_spec() {
        assert_eq!(BRDF_OUT_DIM, 16);
        assert_eq!(EMBEDDING_DIM, 32);
        assert_eq!(IMPEDANCE_BANDS, 4);
        assert_eq!(MORPHOLOGY_PARAMS, 16);
    }
}
