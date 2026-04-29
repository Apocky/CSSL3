//! Face-tracking integration. § X.C.
//!
//! § PRIME-DIRECTIVE §1 hard-gate on face-tracking : every `FaceWeights`
//! wrapped in `LabeledValue<FaceWeights>` with `SensitiveDomain::Face`.
//! Egress non-overridable-refused.
//!
//! § DESIGN
//!   - 60+ blendshape-weights per-frame (FACS-aligned typical).
//!   - Providers : Meta XR_FB_face_tracking2, HTC XR_HTC_facial_tracking,
//!     Apple ARKit (Persona-system blendshapes).
//!   - Engine-mapping (08_BODY MACHINE-layer face-blendshape-KAN) lives
//!     in `cssl-substrate-kan` ; this crate ships only host-side capture.

use crate::error::XRFailure;
use crate::ifc_shim::{Label, LabeledValue, SensitiveDomain};

/// Maximum blendshape-count tracked. § X.C : 60+ FACS-aligned typical ;
/// XR_FB_face_tracking2 reports up to 70.
pub const MAX_BLENDSHAPES: usize = 70;

/// Face-tracking provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceTrackingProvider {
    /// Meta XR_FB_face_tracking2 (Quest 3 / Quest Pro).
    /// 70 FACS-aligned blendshapes (eyelid + brow + cheek + jaw + lip).
    MetaFb2,
    /// HTC XR_HTC_facial_tracking (Vive XR Elite, Vive Focus Vision).
    /// 36 ARKit-equivalent blendshapes.
    Htc,
    /// Apple ARKit Persona (visionOS).
    /// 52 ARKit-canonical blendshapes.
    AppleArkit,
    /// No face-tracking (Index, Bigscreen Beyond stock, Quest 2/3S).
    None,
}

impl FaceTrackingProvider {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MetaFb2 => "meta-fb2",
            Self::Htc => "htc",
            Self::AppleArkit => "apple-arkit",
            Self::None => "none",
        }
    }

    /// Blendshape-count this provider returns.
    #[must_use]
    pub const fn blendshape_count(self) -> usize {
        match self {
            Self::MetaFb2 => 70,
            Self::Htc => 36,
            Self::AppleArkit => 52,
            Self::None => 0,
        }
    }
}

/// Face blendshape-weight bundle.
///
/// **‼ ALWAYS wrap in `LabeledValue<FaceWeights>` with
/// `SensitiveDomain::Face` before passing across host boundary.**
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceWeights {
    /// Blendshape weights ∈ [0, 1] each. Valid range : `[..weight_count]`.
    pub weights: [f32; MAX_BLENDSHAPES],
    /// Number of populated weights (provider-dependent).
    pub weight_count: u32,
    /// Provider.
    pub provider: FaceTrackingProvider,
    /// Acquisition-timestamp-ns.
    pub timestamp_ns: u64,
}

impl FaceWeights {
    /// Identity (all-zero, neutral expression).
    #[must_use]
    pub fn identity(provider: FaceTrackingProvider) -> Self {
        Self {
            weights: [0.0; MAX_BLENDSHAPES],
            weight_count: provider.blendshape_count() as u32,
            provider,
            timestamp_ns: 0,
        }
    }

    /// Wrap in `LabeledValue` with `SensitiveDomain::Face`.
    #[must_use]
    pub fn into_labeled(self) -> LabeledValue<Self> {
        LabeledValue::with_domain(self, Label::bottom(), SensitiveDomain::Face)
    }

    /// Maximum weight-value across all populated blendshapes.
    #[must_use]
    pub fn max_weight(&self) -> f32 {
        let n = self.weight_count as usize;
        self.weights[..n]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
    }

    /// `true` iff any blendshape weight > `threshold`.
    /// Used for "expression-changed" debounce in 08_BODY/02 Coherence-Engine.
    #[must_use]
    pub fn any_above(&self, threshold: f32) -> bool {
        self.max_weight() > threshold
    }
}

/// Face-tracker capabilities. Negotiated at create-time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceTrackerCaps {
    /// Provider available.
    pub provider: FaceTrackingProvider,
    /// Eyelid + brow tracked separately (FB2 ✓ ; HTC ✓).
    pub eyelid_brow: bool,
    /// Lip + jaw tracked (FB2 ✓ ; ARKit ✓).
    pub lip_jaw: bool,
    /// Cheek tracked (FB2 ✓).
    pub cheek: bool,
}

impl FaceTrackerCaps {
    /// No face-tracker.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            provider: FaceTrackingProvider::None,
            eyelid_brow: false,
            lip_jaw: false,
            cheek: false,
        }
    }

    /// Quest 3 / Quest Pro default (FB2 full).
    #[must_use]
    pub const fn quest3_default() -> Self {
        Self {
            provider: FaceTrackingProvider::MetaFb2,
            eyelid_brow: true,
            lip_jaw: true,
            cheek: true,
        }
    }

    /// Vision Pro default.
    #[must_use]
    pub const fn vision_pro_default() -> Self {
        Self {
            provider: FaceTrackingProvider::AppleArkit,
            eyelid_brow: true,
            lip_jaw: true,
            cheek: false,
        }
    }

    /// Validate.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if matches!(self.provider, FaceTrackingProvider::None)
            && (self.eyelid_brow || self.lip_jaw || self.cheek)
        {
            return Err(XRFailure::ActionSetInstall { code: -50 });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{FaceTrackerCaps, FaceTrackingProvider, FaceWeights, MAX_BLENDSHAPES};
    use crate::eye_gaze::try_egress;
    use crate::ifc_shim::SensitiveDomain;

    #[test]
    fn max_blendshapes_is_70() {
        assert_eq!(MAX_BLENDSHAPES, 70);
    }

    #[test]
    fn provider_blendshape_counts() {
        assert_eq!(FaceTrackingProvider::MetaFb2.blendshape_count(), 70);
        assert_eq!(FaceTrackingProvider::Htc.blendshape_count(), 36);
        assert_eq!(FaceTrackingProvider::AppleArkit.blendshape_count(), 52);
        assert_eq!(FaceTrackingProvider::None.blendshape_count(), 0);
    }

    #[test]
    fn weights_identity_neutral() {
        let w = FaceWeights::identity(FaceTrackingProvider::MetaFb2);
        assert_eq!(w.weight_count, 70);
        assert_eq!(w.max_weight(), 0.0);
    }

    #[test]
    fn weights_into_labeled_carries_face_domain() {
        let lv = FaceWeights::identity(FaceTrackingProvider::MetaFb2).into_labeled();
        assert!(lv.is_biometric());
        assert!(lv.sensitive_domains.contains(&SensitiveDomain::Face));
    }

    #[test]
    fn weights_egress_refused() {
        let lv = FaceWeights::identity(FaceTrackingProvider::MetaFb2).into_labeled();
        assert!(try_egress(&lv).unwrap_err().is_biometric_refusal());
    }

    #[test]
    fn quest3_caps_full() {
        let c = FaceTrackerCaps::quest3_default();
        assert_eq!(c.provider, FaceTrackingProvider::MetaFb2);
        assert!(c.eyelid_brow);
        assert!(c.lip_jaw);
        assert!(c.cheek);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn vision_pro_caps_no_cheek() {
        let c = FaceTrackerCaps::vision_pro_default();
        assert_eq!(c.provider, FaceTrackingProvider::AppleArkit);
        assert!(c.eyelid_brow);
        assert!(c.lip_jaw);
        assert!(!c.cheek);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn caps_none_with_features_invalid() {
        let c = FaceTrackerCaps {
            provider: FaceTrackingProvider::None,
            eyelid_brow: true,
            lip_jaw: false,
            cheek: false,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn max_weight_finds_maximum() {
        let mut w = FaceWeights::identity(FaceTrackingProvider::MetaFb2);
        w.weights[0] = 0.3;
        w.weights[5] = 0.8;
        w.weights[10] = 0.5;
        assert!((w.max_weight() - 0.8).abs() < 1e-6);
    }

    #[test]
    fn any_above_threshold() {
        let mut w = FaceWeights::identity(FaceTrackingProvider::MetaFb2);
        w.weights[0] = 0.4;
        assert!(w.any_above(0.3));
        assert!(!w.any_above(0.5));
    }
}
