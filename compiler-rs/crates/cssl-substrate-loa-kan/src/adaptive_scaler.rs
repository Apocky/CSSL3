//! § AdaptiveContentScaler — derives KAN-edge-budget from fovea-mask.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-2 ⊗ Stage-3 glue : reads the gaze-collapse fovea-mask + KAN-
//!   detail-budget and produces a per-cell modulation scaler that drives
//!   Stage-6 (BRDF) and Stage-7 (fractal-amp) detail-level. This is how
//!   the LoA scene-author ties their KAN-extension regions into the
//!   gaze-driven adaptive-quality system without bypassing it.
//!
//! § PRIME-DIRECTIVE
//!   - Cannot override Stage-2 KanBudget — only attenuate. A peripheral-
//!     cell budget ALWAYS produces a peripheral-tier scaler ; the
//!     scene-author cannot upgrade peripheral cells to foveal-tier
//!     detail.
//!   - Σ-mask Sample-bit consulted : if the cell's consent does not
//!     permit Sample, the scaler returns identity-scale (no detail-
//!     enhancement). This is how Frozen / Halted cells stay opaque to
//!     the adaptive scaler.
//!
//! § SPEC
//!   - `specs/32_SIGNATURE_RENDERING.csl` § STAGE-2 (FoveaMask + KanBudget).
//!   - `specs/32_SIGNATURE_RENDERING.csl` § STAGE-7 (FractalAmplifier).

use crate::modulation::{LoaKanCellModulation, ModulationError, MODULATION_BOUND};
use cssl_substrate_prime_directive::sigma::SigmaMaskPacked;

/// § Detail-tier discriminator. Drives the modulation-scaler magnitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KanDetailTier {
    /// § Peripheral tier — minimal detail. Fragment-amp scaler ≤ 0.25.
    #[default]
    Peripheral = 0,
    /// § Mid tier — moderate detail. Scaler ≤ 0.5.
    Mid = 1,
    /// § Near-foveal — high detail. Scaler ≤ 0.85.
    NearFoveal = 2,
    /// § Foveal — maximum detail. Scaler ≤ 1.0.
    Foveal = 3,
}

impl KanDetailTier {
    /// § All variants in canonical-ascending-detail order.
    #[must_use]
    pub const fn all() -> [KanDetailTier; 4] {
        [Self::Peripheral, Self::Mid, Self::NearFoveal, Self::Foveal]
    }

    /// § Canonical name for telemetry.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Peripheral => "peripheral",
            Self::Mid => "mid",
            Self::NearFoveal => "near_foveal",
            Self::Foveal => "foveal",
        }
    }

    /// § The maximum modulation-scale magnitude this tier permits.
    ///   Per spec § STAGE-2 KanBudget cascade.
    #[must_use]
    pub const fn max_scale(self) -> f32 {
        match self {
            Self::Peripheral => 0.25,
            Self::Mid => 0.50,
            Self::NearFoveal => 0.85,
            Self::Foveal => 1.0,
        }
    }

    /// § Decode from u8 ; unknown values clamp to Peripheral (the safest).
    #[must_use]
    pub const fn from_u8(v: u8) -> KanDetailTier {
        match v {
            0 => Self::Peripheral,
            1 => Self::Mid,
            2 => Self::NearFoveal,
            3 => Self::Foveal,
            _ => Self::Peripheral,
        }
    }

    /// § Pack to u8.
    #[must_use]
    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// § Tier intersection : the more-restrictive tier of two. Used by
    ///   the multi-Sovereign overlap path (more-restrictive wins).
    #[must_use]
    pub fn min(self, other: KanDetailTier) -> KanDetailTier {
        if self.to_u8() < other.to_u8() {
            self
        } else {
            other
        }
    }
}

/// § Adaptive-content scaler : maps (tier, requested-scale, Σ-mask) to
///   a [`LoaKanCellModulation`] that respects the Stage-2 budget.
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveContentScaler {
    /// § The tier this scaler is configured for.
    pub tier: KanDetailTier,
    /// § The Sovereign that authored the scene-region (used as the
    ///   modulation's sovereign_handle).
    pub sovereign_handle: u16,
    /// § The author's REQUESTED scale (may be capped by tier-max).
    pub requested_scale: f32,
}

impl AdaptiveContentScaler {
    /// § Construct a scaler for a given tier + author-Sovereign +
    ///   requested scale. The actual applied scale is `min(requested,
    ///   tier.max_scale())`.
    #[must_use]
    pub fn new(
        tier: KanDetailTier,
        sovereign_handle: u16,
        requested_scale: f32,
    ) -> AdaptiveContentScaler {
        AdaptiveContentScaler {
            tier,
            sovereign_handle,
            requested_scale,
        }
    }

    /// § The effective scale after tier-clamping.
    #[must_use]
    pub fn effective_scale(&self) -> f32 {
        let bound = self.tier.max_scale();
        self.requested_scale.clamp(-bound, bound)
    }

    /// § Derive the modulation for a cell with the given Σ-mask. The
    ///   Sample-bit on the mask gates whether the modulation is active —
    ///   cells without Sample consent receive identity-modulation.
    ///
    /// # Errors
    /// Returns [`ScalerError::CoefficientExceeded`] when the requested-
    /// scale exceeds the global modulation-bound.
    pub fn derive(&self, sigma: SigmaMaskPacked) -> Result<LoaKanCellModulation, ScalerError> {
        if self.requested_scale.abs() > MODULATION_BOUND {
            return Err(ScalerError::CoefficientExceeded {
                value: self.requested_scale,
                bound: MODULATION_BOUND,
            });
        }
        // Cells without Sample consent receive identity-modulation (no
        // detail enhancement applied — preserves Σ-mask refusal at the
        // Stage-7 fractal-amp boundary per spec § STAGE-7 @sigma_aware).
        if !sigma.can_sample() {
            return Ok(LoaKanCellModulation::identity());
        }
        let s = self.effective_scale();
        match LoaKanCellModulation::uniform(s, self.sovereign_handle) {
            Ok(m) => Ok(m),
            Err(ModulationError::CoefficientOutOfBounds { .. }) => {
                Err(ScalerError::CoefficientExceeded {
                    value: s,
                    bound: MODULATION_BOUND,
                })
            }
            Err(e) => Err(ScalerError::FromModulation(e.to_string())),
        }
    }

    /// § True iff the requested-scale would be capped by the tier's max.
    #[must_use]
    pub fn is_clipped(&self) -> bool {
        self.requested_scale.abs() > self.tier.max_scale()
    }
}

/// § Failure modes for [`AdaptiveContentScaler`].
#[derive(Debug, thiserror::Error)]
pub enum ScalerError {
    /// § The author-requested scale exceeds the global modulation-bound.
    #[error("LK0030 — adaptive-scaler requested-scale exceeds modulation bound : value={value}, bound=±{bound}")]
    CoefficientExceeded { value: f32, bound: f32 },
    /// § Underlying modulation-error (rare ; usually wrapped into
    ///   CoefficientExceeded).
    #[error("LK0031 — adaptive-scaler upstream modulation error : {0}")]
    FromModulation(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaPolicy};

    // ── Tier discriminant + ordering ───────────────────────────────

    #[test]
    fn detail_tier_all_count() {
        assert_eq!(KanDetailTier::all().len(), 4);
    }

    #[test]
    fn detail_tier_max_scale_monotone_increasing() {
        let scales: Vec<f32> = KanDetailTier::all().iter().map(|t| t.max_scale()).collect();
        for w in scales.windows(2) {
            assert!(w[0] < w[1], "tier max_scale must be monotone increasing");
        }
    }

    #[test]
    fn detail_tier_canonical_names_unique() {
        let names: Vec<&'static str> = KanDetailTier::all()
            .iter()
            .map(|t| t.canonical_name())
            .collect();
        let mut s = names.clone();
        s.sort_unstable();
        let original = s.len();
        s.dedup();
        assert_eq!(s.len(), original);
    }

    #[test]
    fn detail_tier_min_is_more_restrictive() {
        let r = KanDetailTier::Peripheral.min(KanDetailTier::Foveal);
        assert_eq!(r, KanDetailTier::Peripheral);
    }

    #[test]
    fn detail_tier_roundtrip_u8() {
        for &t in &KanDetailTier::all() {
            let r = KanDetailTier::from_u8(t.to_u8());
            assert_eq!(r, t);
        }
    }

    #[test]
    fn detail_tier_unknown_clamps_to_peripheral() {
        assert_eq!(KanDetailTier::from_u8(255), KanDetailTier::Peripheral);
    }

    // ── Effective scale + clip ────────────────────────────────────

    #[test]
    fn requested_within_tier_passes() {
        let s = AdaptiveContentScaler::new(KanDetailTier::Foveal, 7, 0.5);
        assert_eq!(s.effective_scale(), 0.5);
        assert!(!s.is_clipped());
    }

    #[test]
    fn requested_above_tier_clipped() {
        let s = AdaptiveContentScaler::new(KanDetailTier::Peripheral, 7, 1.0);
        // Peripheral max_scale = 0.25 ; requested 1.0 clamped to 0.25.
        assert_eq!(s.effective_scale(), 0.25);
        assert!(s.is_clipped());
    }

    #[test]
    fn requested_negative_clipped_to_negative_bound() {
        let s = AdaptiveContentScaler::new(KanDetailTier::Mid, 7, -2.0);
        assert_eq!(s.effective_scale(), -0.5);
        assert!(s.is_clipped());
    }

    // ── Derive — Σ-mask gating ────────────────────────────────────

    #[test]
    fn derive_without_sample_consent_returns_identity() {
        let s = AdaptiveContentScaler::new(KanDetailTier::Foveal, 7, 1.0);
        // Default-Private (Observe-only) does not include Sample.
        let mask = SigmaMaskPacked::default_mask();
        let m = s.derive(mask).unwrap();
        assert!(m.is_identity());
    }

    #[test]
    fn derive_with_sample_consent_returns_modulation() {
        let s = AdaptiveContentScaler::new(KanDetailTier::Mid, 7, 0.4);
        let mask = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead);
        // PublicRead = Observe + Sample
        assert!(mask.can_sample());
        let m = s.derive(mask).unwrap();
        assert!(m.active);
        for i in 0..super::super::MODULATION_DIM {
            assert_eq!(m.coeffs[i], 0.4);
        }
    }

    #[test]
    fn derive_above_modulation_bound_refused() {
        let s = AdaptiveContentScaler::new(KanDetailTier::Foveal, 7, 100.0);
        let mask = SigmaMaskPacked::default_mask()
            .with_consent(ConsentBit::Observe.bits() | ConsentBit::Sample.bits());
        let err = s.derive(mask).unwrap_err();
        assert!(matches!(err, ScalerError::CoefficientExceeded { .. }));
    }

    #[test]
    fn derive_clipped_scale_reaches_tier_cap() {
        let s = AdaptiveContentScaler::new(KanDetailTier::Mid, 7, 5.0);
        let mask = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead);
        let m = s.derive(mask).unwrap();
        // Tier cap = 0.5 ; requested 5.0 clipped down.
        for i in 0..super::super::MODULATION_DIM {
            assert_eq!(m.coeffs[i], 0.5);
        }
    }
}
