//! Body-tracking integration. § X.B.
//!
//! § PRIME-DIRECTIVE §1 hard-gate on body-tracking : every `BodySkeleton`
//! wrapped in `LabeledValue<BodySkeleton>` with `SensitiveDomain::Body`.
//! Egress non-overridable-refused.

use crate::error::XRFailure;
use crate::ifc_shim::{Label, LabeledValue, SensitiveDomain};

/// Joint count in canonical XR_FB_body_tracking full-body skeleton.
/// Spec § X.B : 70+ joints (upper-body + lower-IK on Quest 3).
pub const FULL_BODY_JOINT_COUNT: usize = 70;

/// Joint-pose for one body joint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointPose {
    /// Position (m, head-relative).
    pub position: [f32; 3],
    /// Orientation (quaternion `[x, y, z, w]`).
    pub orientation: [f32; 4],
    /// Confidence ∈ [0, 1].
    pub confidence: f32,
}

impl JointPose {
    /// Identity-pose.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0, 1.0],
            confidence: 0.0,
        }
    }
}

/// Body-tracking provider. § X.B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyTrackingProvider {
    /// Meta XR_FB_body_tracking (Quest 3 / Quest Pro).
    MetaFb,
    /// HTC XR_HTC_body_tracking.
    Htc,
    /// ByteDance/Pico XR_BD_body_tracking.
    PicoBd,
    /// Apple Vision Pro ARKit body-tracking.
    AppleArkit,
    /// No body-tracking.
    None,
}

impl BodyTrackingProvider {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MetaFb => "meta-fb",
            Self::Htc => "htc",
            Self::PicoBd => "pico-bd",
            Self::AppleArkit => "apple-arkit",
            Self::None => "none",
        }
    }

    /// Joint-count this provider returns.
    #[must_use]
    pub const fn joint_count(self) -> usize {
        match self {
            Self::MetaFb => FULL_BODY_JOINT_COUNT,
            Self::Htc => 32,
            Self::PicoBd => 24,
            Self::AppleArkit => 27, // ARKit canonical skeleton
            Self::None => 0,
        }
    }
}

/// Full-body skeleton (canonical 70-joint per § X.B).
///
/// **‼ ALWAYS wrap in `LabeledValue<BodySkeleton>` with
/// `SensitiveDomain::Body` before passing across host boundary.**
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodySkeleton {
    /// 70-joint array.
    pub joints: [JointPose; FULL_BODY_JOINT_COUNT],
    /// Number of populated joints (provider-dependent).
    pub joint_count: u32,
    /// Provider that produced this skeleton.
    pub provider: BodyTrackingProvider,
    /// Acquisition-timestamp-ns.
    pub timestamp_ns: u64,
}

impl BodySkeleton {
    /// Identity-skeleton.
    #[must_use]
    pub fn identity(provider: BodyTrackingProvider) -> Self {
        Self {
            joints: [JointPose::identity(); FULL_BODY_JOINT_COUNT],
            joint_count: provider.joint_count() as u32,
            provider,
            timestamp_ns: 0,
        }
    }

    /// Wrap in `LabeledValue` with `SensitiveDomain::Body`.
    #[must_use]
    pub fn into_labeled(self) -> LabeledValue<Self> {
        LabeledValue::with_domain(self, Label::bottom(), SensitiveDomain::Body)
    }

    /// Aggregate-confidence over populated joints.
    #[must_use]
    pub fn aggregate_confidence(&self) -> f32 {
        if self.joint_count == 0 {
            return 0.0;
        }
        let n = self.joint_count as usize;
        let sum: f32 = self.joints[..n].iter().map(|j| j.confidence).sum();
        sum / n as f32
    }
}

/// Body-tracker capabilities. Negotiated at create-time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyTrackerCaps {
    /// Provider available.
    pub provider: BodyTrackingProvider,
    /// Lower-body IK supported (Quest 3 ✓ ; Quest Pro ✗).
    pub lower_body_ik: bool,
    /// Confidence-per-joint reported.
    pub confidence_per_joint: bool,
}

impl BodyTrackerCaps {
    /// No body-tracker.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            provider: BodyTrackingProvider::None,
            lower_body_ik: false,
            confidence_per_joint: false,
        }
    }

    /// Quest 3 default.
    #[must_use]
    pub const fn quest3_default() -> Self {
        Self {
            provider: BodyTrackingProvider::MetaFb,
            lower_body_ik: true,
            confidence_per_joint: true,
        }
    }

    /// Validate.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if matches!(self.provider, BodyTrackingProvider::None)
            && (self.lower_body_ik || self.confidence_per_joint)
        {
            return Err(XRFailure::ActionSetInstall { code: -40 });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BodySkeleton, BodyTrackerCaps, BodyTrackingProvider, JointPose, FULL_BODY_JOINT_COUNT,
    };
    use crate::eye_gaze::try_egress;
    use crate::ifc_shim::SensitiveDomain;

    #[test]
    fn full_body_joint_count_70() {
        assert_eq!(FULL_BODY_JOINT_COUNT, 70);
    }

    #[test]
    fn identity_joint_at_origin() {
        let j = JointPose::identity();
        assert_eq!(j.position, [0.0, 0.0, 0.0]);
        assert_eq!(j.orientation, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn provider_joint_counts() {
        assert_eq!(BodyTrackingProvider::MetaFb.joint_count(), 70);
        assert_eq!(BodyTrackingProvider::Htc.joint_count(), 32);
        assert_eq!(BodyTrackingProvider::PicoBd.joint_count(), 24);
        assert_eq!(BodyTrackingProvider::AppleArkit.joint_count(), 27);
        assert_eq!(BodyTrackingProvider::None.joint_count(), 0);
    }

    #[test]
    fn skeleton_identity_records_provider_joint_count() {
        let s = BodySkeleton::identity(BodyTrackingProvider::MetaFb);
        assert_eq!(s.joint_count, 70);
        assert_eq!(s.aggregate_confidence(), 0.0);
    }

    #[test]
    fn skeleton_into_labeled_carries_body_domain() {
        let lv = BodySkeleton::identity(BodyTrackingProvider::MetaFb).into_labeled();
        assert!(lv.is_biometric());
        assert!(lv.sensitive_domains.contains(&SensitiveDomain::Body));
    }

    #[test]
    fn skeleton_egress_refused() {
        let lv = BodySkeleton::identity(BodyTrackingProvider::MetaFb).into_labeled();
        assert!(try_egress(&lv).unwrap_err().is_biometric_refusal());
    }

    #[test]
    fn quest3_caps_full() {
        let c = BodyTrackerCaps::quest3_default();
        assert_eq!(c.provider, BodyTrackingProvider::MetaFb);
        assert!(c.lower_body_ik);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn caps_none_with_features_invalid() {
        let c = BodyTrackerCaps {
            provider: BodyTrackingProvider::None,
            lower_body_ik: true,
            confidence_per_joint: false,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn caps_none_without_features_valid() {
        assert!(BodyTrackerCaps::none().validate().is_ok());
    }

    #[test]
    fn aggregate_confidence_averages_populated_joints() {
        let mut s = BodySkeleton::identity(BodyTrackingProvider::Htc);
        // Provider says 32 joints ; populate the first 32.
        for i in 0..32 {
            s.joints[i].confidence = 0.5;
        }
        assert!((s.aggregate_confidence() - 0.5).abs() < 1e-6);
    }
}
