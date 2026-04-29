//! Hand-tracking integration. § X.A.
//!
//! § PRIME-DIRECTIVE §1 (anti-surveillance) HARD-GATE on body-tracking
//! data : every `HandSkeleton` is wrapped in `LabeledValue<HandSkeleton>`
//! with `SensitiveDomain::Body` tag. Egress is non-overridable-refused.
//!
//! § DESIGN
//!   - 26-bone canonical skeleton per-hand (XR_EXT_hand_tracking core).
//!   - Vendor-augments : aim-confidence (FB), capsule-collision (FB),
//!     mesh (FB), motion-range (EXT).
//!   - Output @ 60-90 Hz typical ⊗ confidence per-bone.
//!   - Engine-mapping (08_BODY MACHINE-layer hand-effector @ inverse-
//!     dynamics) is in `cssl-anim` ; this crate only ships the host-
//!     side capture surface.

use crate::error::XRFailure;
use crate::ifc_shim::{Label, LabeledValue, SensitiveDomain};

/// Bone count in the canonical XR_EXT_hand_tracking skeleton.
pub const HAND_BONE_COUNT: usize = 26;

/// Which hand a skeleton describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandSide {
    /// Left hand.
    Left,
    /// Right hand.
    Right,
}

impl HandSide {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    /// Action-binding root path.
    #[must_use]
    pub const fn action_root(self) -> &'static str {
        match self {
            Self::Left => "/user/hand/left",
            Self::Right => "/user/hand/right",
        }
    }
}

/// Bone-pose : position + orientation (quat) + radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BonePose {
    /// Bone position (m, head-relative).
    pub position: [f32; 3],
    /// Bone orientation (quaternion `[x, y, z, w]`).
    pub orientation: [f32; 4],
    /// Bone radius for capsule-collision (m). Zero if unsupported.
    pub radius: f32,
    /// Per-bone confidence ∈ [0, 1].
    pub confidence: f32,
}

impl BonePose {
    /// Identity bone : at origin, identity-orientation, zero-radius.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0, 1.0],
            radius: 0.0,
            confidence: 0.0,
        }
    }
}

/// Pinch-gesture confidence + aim-pose. § X.A vendor-augment :
/// `XR_FB_hand_tracking_aim`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinchAim {
    /// Pinch-strength ∈ [0, 1]. > 0.7 typical "pinch detected".
    pub strength: u8,
    /// Aim-pose available.
    pub aim_pose_valid: bool,
}

impl PinchAim {
    /// All-zero pinch (open hand).
    #[must_use]
    pub const fn open_hand() -> Self {
        Self {
            strength: 0,
            aim_pose_valid: false,
        }
    }

    /// Strength normalized to [0.0, 1.0].
    #[must_use]
    pub fn strength_f32(self) -> f32 {
        self.strength as f32 / 255.0
    }
}

/// Full per-hand skeleton.
///
/// **‼ ALWAYS wrap in `LabeledValue<HandSkeleton>` with
/// `SensitiveDomain::Body` before passing across the host boundary.**
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandSkeleton {
    /// Side this skeleton describes.
    pub side: HandSide,
    /// 26-bone array (XR_EXT_hand_tracking canonical).
    pub bones: [BonePose; HAND_BONE_COUNT],
    /// Pinch + aim (FB augment ; populated only when XR_FB_hand_tracking_aim is enabled).
    pub pinch: PinchAim,
    /// Acquisition-timestamp-ns.
    pub timestamp_ns: u64,
}

impl HandSkeleton {
    /// Identity skeleton (untracked).
    #[must_use]
    pub fn identity(side: HandSide) -> Self {
        Self {
            side,
            bones: [BonePose::identity(); HAND_BONE_COUNT],
            pinch: PinchAim::open_hand(),
            timestamp_ns: 0,
        }
    }

    /// Wrap in `LabeledValue` with `SensitiveDomain::Body`.
    #[must_use]
    pub fn into_labeled(self) -> LabeledValue<Self> {
        LabeledValue::with_domain(self, Label::bottom(), SensitiveDomain::Body)
    }

    /// Aggregate-confidence : average of per-bone confidence.
    #[must_use]
    pub fn aggregate_confidence(&self) -> f32 {
        let sum: f32 = self.bones.iter().map(|b| b.confidence).sum();
        sum / HAND_BONE_COUNT as f32
    }
}

/// Per-frame both-hands snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BothHands {
    /// Left-hand skeleton.
    pub left: HandSkeleton,
    /// Right-hand skeleton.
    pub right: HandSkeleton,
}

impl BothHands {
    /// Identity-snapshot.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            left: HandSkeleton::identity(HandSide::Left),
            right: HandSkeleton::identity(HandSide::Right),
        }
    }

    /// Wrap in `LabeledValue<BothHands>` with `SensitiveDomain::Body`.
    #[must_use]
    pub fn into_labeled(self) -> LabeledValue<Self> {
        LabeledValue::with_domain(self, Label::bottom(), SensitiveDomain::Body)
    }
}

/// Hand-tracker capabilities. Negotiated at xrCreateHandTrackerEXT time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandTrackerCaps {
    /// Core 26-bone tracking via XR_EXT_hand_tracking.
    pub core: bool,
    /// Aim-confidence + aim-pose via XR_FB_hand_tracking_aim.
    pub aim: bool,
    /// Capsule-collision via XR_FB_hand_tracking_capsules.
    pub capsules: bool,
    /// Mesh via XR_FB_hand_tracking_mesh.
    pub mesh: bool,
    /// Joint-motion-range via XR_EXT_hand_joints_motion_range.
    pub motion_range: bool,
}

impl HandTrackerCaps {
    /// All-disabled.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            core: false,
            aim: false,
            capsules: false,
            mesh: false,
            motion_range: false,
        }
    }

    /// Quest 3 default capabilities.
    #[must_use]
    pub const fn quest3_default() -> Self {
        Self {
            core: true,
            aim: true,
            capsules: true,
            mesh: false, // mesh is opt-in (memory cost)
            motion_range: true,
        }
    }

    /// Vision Pro default (ARKit hand-tracking via Compositor-Services).
    #[must_use]
    pub const fn vision_pro_default() -> Self {
        Self {
            core: true,
            aim: false,
            capsules: false,
            mesh: false,
            motion_range: false,
        }
    }

    /// Validate the request : core must be true if any augment is true.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if (self.aim || self.capsules || self.mesh || self.motion_range) && !self.core {
            return Err(XRFailure::ActionSetInstall { code: -30 });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BonePose, BothHands, HandSide, HandSkeleton, HandTrackerCaps, PinchAim, HAND_BONE_COUNT,
    };
    use crate::eye_gaze::try_egress;
    use crate::ifc_shim::SensitiveDomain;

    #[test]
    fn hand_side_action_root() {
        assert_eq!(HandSide::Left.action_root(), "/user/hand/left");
        assert_eq!(HandSide::Right.action_root(), "/user/hand/right");
    }

    #[test]
    fn hand_bone_count_is_26() {
        assert_eq!(HAND_BONE_COUNT, 26);
    }

    #[test]
    fn identity_bone_at_origin() {
        let b = BonePose::identity();
        assert_eq!(b.position, [0.0, 0.0, 0.0]);
        assert_eq!(b.confidence, 0.0);
    }

    #[test]
    fn pinch_open_hand_zero_strength() {
        let p = PinchAim::open_hand();
        assert_eq!(p.strength, 0);
        assert_eq!(p.strength_f32(), 0.0);
    }

    #[test]
    fn pinch_strength_normalized() {
        let p = PinchAim {
            strength: 255,
            aim_pose_valid: true,
        };
        assert!((p.strength_f32() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn skeleton_identity_has_26_bones() {
        let s = HandSkeleton::identity(HandSide::Left);
        assert_eq!(s.bones.len(), HAND_BONE_COUNT);
        assert_eq!(s.aggregate_confidence(), 0.0);
    }

    #[test]
    fn skeleton_into_labeled_carries_body_domain() {
        let lv = HandSkeleton::identity(HandSide::Right).into_labeled();
        assert!(lv.is_biometric());
        assert!(lv.sensitive_domains.contains(&SensitiveDomain::Body));
    }

    #[test]
    fn skeleton_egress_refused() {
        let lv = HandSkeleton::identity(HandSide::Left).into_labeled();
        let err = try_egress(&lv).unwrap_err();
        assert!(err.is_biometric_refusal());
    }

    #[test]
    fn both_hands_egress_refused() {
        let lv = BothHands::identity().into_labeled();
        let err = try_egress(&lv).unwrap_err();
        assert!(err.is_biometric_refusal());
    }

    #[test]
    fn quest3_caps_full() {
        let c = HandTrackerCaps::quest3_default();
        assert!(c.core);
        assert!(c.aim);
        assert!(c.capsules);
        assert!(c.motion_range);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn vision_pro_caps_minimal() {
        let c = HandTrackerCaps::vision_pro_default();
        assert!(c.core);
        assert!(!c.aim); // ARKit doesn't expose aim
        assert!(c.validate().is_ok());
    }

    #[test]
    fn caps_none_validates() {
        assert!(HandTrackerCaps::none().validate().is_ok());
    }

    #[test]
    fn caps_augment_without_core_fails() {
        let c = HandTrackerCaps {
            core: false,
            aim: true,
            capsules: false,
            mesh: false,
            motion_range: false,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn aggregate_confidence_averages() {
        let mut s = HandSkeleton::identity(HandSide::Left);
        for b in &mut s.bones {
            b.confidence = 0.5;
        }
        assert!((s.aggregate_confidence() - 0.5).abs() < 1e-6);
    }
}
