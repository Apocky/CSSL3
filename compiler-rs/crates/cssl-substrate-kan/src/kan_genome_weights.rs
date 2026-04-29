//! § KanGenomeWeights — packaged tri-net KAN weights for genome encoding
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The substrate-spec `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 7` shape :
//!
//!   ```cssl
//!   type KanGenomeWeights = {
//!     body_kan:        KanNetwork<HDC_DIM, BODY_PARAMS>,
//!     cognitive_kan:   KanNetwork<HDC_DIM, COG_PARAMS>,
//!     capability_kan:  KanNetwork<HDC_DIM, CAP_PARAMS>,
//!   }
//!   const BODY_PARAMS : usize = 64 ;
//!   const COG_PARAMS  : usize = 128 ;
//!   const CAP_PARAMS  : usize = 32 ;
//!   ```
//!
//!   This is the per-genome KAN-weight bundle that `Genome.kan_weights`
//!   stores. The `crystallize::<S>(&phi, target)` path in
//!   `08_BODY/03_DIMENSIONAL_TRAVEL.csl § II` consumes these to derive
//!   morphology / cognition / capability in the target substrate ; the
//!   Pattern fingerprint must agree pre- and post-translation, which means
//!   the `KanGenomeWeights` byte representation must be stable.
//!
//! § FINGERPRINT — composes the three networks
//!   `KanGenomeWeights::fingerprint_bytes()` returns a 32-byte blake3 digest
//!   that combines all three internal networks' fingerprints in canonical
//!   order : body → cognitive → capability. This is what `Pattern::stamp`
//!   absorbs into the Pattern fingerprint.
//!
//! § HDC_DIM REFERENCE
//!   The constant `HDC_DIM = 10000` is owned by `cssl-hdc::genome`. This
//!   crate re-imports it via `cssl_hdc::genome::HDC_DIM` to avoid a
//!   duplicate-declaration footgun (spec invariant : `HDC_DIM` declared once,
//!   referenced everywhere).

use crate::kan_network::KanNetwork;
use cssl_hdc::genome::HDC_DIM;

/// § The output dimensionality of the body-morphology KAN. Matches spec.
pub const BODY_PARAMS: usize = 64;
/// § The output dimensionality of the cognitive-traits KAN. Matches spec.
pub const COG_PARAMS: usize = 128;
/// § The output dimensionality of the capability-vector KAN. Matches spec.
pub const CAP_PARAMS: usize = 32;

/// § The packaged tri-net weight bundle that a Genome carries.
///
/// Each genome maps its 10000-D HDC hypervector through three independent
/// KAN networks to produce the substrate-translation-stable parameter
/// vectors. Same hypervector + same weights ⇒ same parameter vectors
/// across every host and every substrate.
#[derive(Debug, Clone)]
pub struct KanGenomeWeights {
    /// § HDC → 64-D body-morphology coefficients.
    pub body_kan: KanNetwork<HDC_DIM, BODY_PARAMS>,
    /// § HDC → 128-D cognitive-traits.
    pub cognitive_kan: KanNetwork<HDC_DIM, COG_PARAMS>,
    /// § HDC → 32-D capability-scoring.
    pub capability_kan: KanNetwork<HDC_DIM, CAP_PARAMS>,
}

impl KanGenomeWeights {
    /// § Construct the all-zero (untrained) weight bundle. Useful as the
    ///   default for newly-spawned creature-genomes that haven't gone
    ///   through training yet.
    #[must_use]
    pub fn new_untrained() -> Self {
        Self {
            body_kan: KanNetwork::new_untrained(),
            cognitive_kan: KanNetwork::new_untrained(),
            capability_kan: KanNetwork::new_untrained(),
        }
    }

    /// § Produce a 32-byte blake3 fingerprint over all three networks in
    ///   canonical order. Same weights ⇒ same digest, on every host.
    #[must_use]
    pub fn fingerprint_bytes(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-substrate-kan/KanGenomeWeights/v1");
        h.update(&self.body_kan.fingerprint_bytes());
        h.update(&self.cognitive_kan.fingerprint_bytes());
        h.update(&self.capability_kan.fingerprint_bytes());
        let digest = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_bytes());
        out
    }
}

impl Default for KanGenomeWeights {
    fn default() -> Self {
        Self::new_untrained()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § new_untrained produces all-zero networks.
    #[test]
    fn new_untrained_zero_weights() {
        let w = KanGenomeWeights::new_untrained();
        assert!(!w.body_kan.trained);
        assert!(!w.cognitive_kan.trained);
        assert!(!w.capability_kan.trained);
    }

    /// § Fingerprint deterministic.
    #[test]
    fn fingerprint_deterministic() {
        let a = KanGenomeWeights::new_untrained();
        let b = KanGenomeWeights::new_untrained();
        assert_eq!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § Fingerprint sensitive to body-kan change.
    #[test]
    fn fingerprint_sensitive_to_body_change() {
        let a = KanGenomeWeights::new_untrained();
        let mut b = KanGenomeWeights::new_untrained();
        b.body_kan.control_points[0][0] = 1.0;
        assert_ne!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § Fingerprint sensitive to cognitive-kan change.
    #[test]
    fn fingerprint_sensitive_to_cognitive_change() {
        let a = KanGenomeWeights::new_untrained();
        let mut b = KanGenomeWeights::new_untrained();
        b.cognitive_kan.trained = true;
        assert_ne!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § Fingerprint sensitive to capability-kan change.
    #[test]
    fn fingerprint_sensitive_to_capability_change() {
        let a = KanGenomeWeights::new_untrained();
        let mut b = KanGenomeWeights::new_untrained();
        b.capability_kan.spline_basis = crate::kan_network::SplineBasis::CatmullRom;
        assert_ne!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § Default == new_untrained for fingerprint purposes.
    #[test]
    fn default_equals_new_untrained_fingerprint() {
        let a = KanGenomeWeights::default();
        let b = KanGenomeWeights::new_untrained();
        assert_eq!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § Constants are spec-locked.
    #[test]
    fn constants_match_spec() {
        assert_eq!(BODY_PARAMS, 64);
        assert_eq!(COG_PARAMS, 128);
        assert_eq!(CAP_PARAMS, 32);
    }
}
