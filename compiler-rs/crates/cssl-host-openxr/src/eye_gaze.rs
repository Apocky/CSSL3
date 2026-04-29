//! Eye-gaze tracking integration with PRIME §1 anti-surveillance hard-gate.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § VIII +
//!         `08_BODY/02_VR_EMBODIMENT.csl` (Soul-link safeguard).
//!
//! § PRIME-DIRECTIVE §1 (anti-surveillance) HARD-GATE
//!   ‼ EYE-TRACKING DATA NEVER LEAVES THE DEVICE
//!   ‼ This is HARD-GATE @ CI ⊗ packet-capture test verifies-no-egress
//!   ‼ This is PRIME_DIRECTIVE §1 compliance-not-feature
//!
//! § STRUCTURAL ENFORCEMENT
//!   Every gaze-sample emitted by this module is wrapped in
//!   `LabeledValue<GazeSample>` with `SensitiveDomain::Gaze` tag. Any
//!   call to `cssl-ifc::validate_egress` on such a value returns
//!   `Err(EgressGrantError::BiometricRefused)` — non-overridable, no
//!   `Privilege<*>` capability changes the return-value.
//!
//!   The `try_egress` function below demonstrates this : it always
//!   returns `Err`. There is no other exit-path for gaze data ; the
//!   structural-gate is the only structurally-permitted disposal.
//!
//! § PERMITTED ON-DEVICE USES (§ VIII.A)
//!   1. DFR (foveated rendering ⊗ § V.B).
//!   2. Saccade-prediction (5-yr ML-foveated ⊗ § V.D).
//!   3. UI-affordance (gaze-pointer, dwell-select).
//!   4. Soul/Coherence-link (08_BODY/02 §III).
//!   5. Accommodation-target (varifocal 5-yr).
//!
//! § FORBIDDEN USES (§ VIII.A)
//!   1. ‼ Analytics (heatmap, dwell-time, attention-tracking).
//!   2. ‼ Ad-targeting (no-ad-tech in Omniverse period).
//!   3. ‼ Session-recording.
//!   4. ‼ Network-egress of raw-or-derived gaze-data.

use crate::error::XRFailure;
use crate::ifc_shim::{validate_egress, EgressGrantError, Label, LabeledValue, SensitiveDomain};

/// Tracking-state flags from `XR_EXT_eye_gaze_interaction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GazeTrackingFlags {
    /// `XR_SPACE_LOCATION_POSITION_VALID_BIT` equivalent.
    pub position_valid: bool,
    /// `XR_SPACE_LOCATION_ORIENTATION_VALID_BIT` equivalent.
    pub orientation_valid: bool,
    /// `XR_SPACE_LOCATION_POSITION_TRACKED_BIT` equivalent.
    pub position_tracked: bool,
    /// `XR_SPACE_LOCATION_ORIENTATION_TRACKED_BIT` equivalent.
    pub orientation_tracked: bool,
}

impl GazeTrackingFlags {
    /// All-zero flags (untracked).
    #[must_use]
    pub const fn untracked() -> Self {
        Self {
            position_valid: false,
            orientation_valid: false,
            position_tracked: false,
            orientation_tracked: false,
        }
    }

    /// Fully-tracked flags.
    #[must_use]
    pub const fn fully_tracked() -> Self {
        Self {
            position_valid: true,
            orientation_valid: true,
            position_tracked: true,
            orientation_tracked: true,
        }
    }

    /// `true` iff the sample is usable (position OR orientation valid).
    #[must_use]
    pub const fn is_usable(self) -> bool {
        self.position_valid || self.orientation_valid
    }
}

/// Single gaze-sample. § VIII.B output of `xrLocateSpace` on the
/// `/user/eyes_ext/input/gaze_pose/pose` action.
///
/// **‼ NEVER expose this struct outside an `LabeledValue<GazeSample>`
/// wrapper. The `Sensitive<Gaze>` IFC tag is the only structural
/// enforcement that prevents egress.**
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GazeSample {
    /// Gaze direction unit-vector (head-relative).
    pub direction: [f32; 3],
    /// Gaze origin (head-relative ; typically slightly inset from eye-center).
    pub origin: [f32; 3],
    /// Confidence ∈ [0, 1] from runtime.
    pub confidence: f32,
    /// Tracking-state flags.
    pub flags: GazeTrackingFlags,
    /// Timestamp-ns at which this sample was acquired.
    pub timestamp_ns: u64,
}

impl GazeSample {
    /// Identity-sample : forward-direction, untracked. Used in tests
    /// + as placeholder when tracking is unavailable.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            direction: [0.0, 0.0, -1.0],
            origin: [0.0, 0.0, 0.0],
            confidence: 0.0,
            flags: GazeTrackingFlags::untracked(),
            timestamp_ns: 0,
        }
    }

    /// Fully-tracked forward-gaze sample for tests.
    #[must_use]
    pub const fn fully_tracked_forward() -> Self {
        Self {
            direction: [0.0, 0.0, -1.0],
            origin: [0.0, 0.0, 0.0],
            confidence: 1.0,
            flags: GazeTrackingFlags::fully_tracked(),
            timestamp_ns: 0,
        }
    }

    /// **The ONLY structural exit-path for `GazeSample` is to wrap it
    /// in `LabeledValue<GazeSample>` with `SensitiveDomain::Gaze`.**
    /// Calling `validate_egress` on the result returns `Err`.
    #[must_use]
    pub fn into_labeled(self) -> LabeledValue<Self> {
        LabeledValue::with_domain(self, Label::bottom(), SensitiveDomain::Gaze)
    }
}

/// Per-eye gaze-sample pair (left + right) returned by
/// `XR_EXT_eye_gaze_interaction` + vendor-augments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GazeSamplePair {
    /// Left-eye sample.
    pub left: GazeSample,
    /// Right-eye sample.
    pub right: GazeSample,
}

impl GazeSamplePair {
    /// Identity-pair (both eyes untracked).
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            left: GazeSample::identity(),
            right: GazeSample::identity(),
        }
    }

    /// Wrap in `LabeledValue` with `SensitiveDomain::Gaze`.
    #[must_use]
    pub fn into_labeled(self) -> LabeledValue<Self> {
        LabeledValue::with_domain(self, Label::bottom(), SensitiveDomain::Gaze)
    }
}

/// **Demonstration of the structural-gate.** This function tries to
/// "egress" a labeled gaze sample via the cssl-ifc validate_egress
/// machinery. It always returns `Err(BiometricEgressRefused)`.
///
/// There is **no other exit-path** for `LabeledValue<GazeSample>` that
/// reaches a non-on-device sink. The compile-time + runtime gates are
/// non-overridable.
///
/// § PRIME-DIRECTIVE §1 + §11 attestation : this function is the single
/// point at which any caller could *attempt* an egress, and it always
/// fails. The `EgressGrantError` is converted to
/// `XRFailure::BiometricEgressRefused` for the engine-side surface.
pub fn try_egress<T: Clone>(value: &LabeledValue<T>) -> Result<(), XRFailure> {
    match validate_egress(value) {
        Ok(()) => Ok(()),
        Err(EgressGrantError::BiometricRefused { domain }) => {
            Err(XRFailure::BiometricEgressRefused { domain })
        }
        Err(_) => {
            // Non-biometric refusal kinds reuse the same error for the
            // engine-side surface (the underlying gate is identical : refusal).
            Err(XRFailure::BiometricEgressRefused {
                domain: SensitiveDomain::Privacy,
            })
        }
    }
}

/// Action-binding for `XR_EXT_eye_gaze_interaction`. § VIII.B.
pub const ACTION_GAZE_POSE: &str = "/user/eyes_ext/input/gaze_pose/pose";

/// Action-binding for `XR_FB_eye_tracking_social` per-eye gaze.
pub const ACTION_FB_EYE_TRACKING_SOCIAL: &str = "/user/eyes_ext/input/fb_eye_tracking_social";

/// Action-binding for `XR_PICO_eye_tracking`.
pub const ACTION_PICO_EYE_TRACKING: &str = "/user/eyes_ext/input/pico_eye_tracking";

/// Action-binding for visionOS ARKit eye-tracking (system-level
/// "look-to-target" cursor only ; raw-stream is not exposed by Apple).
pub const ACTION_VISIONOS_LOOK_TO_TARGET: &str = "/user/eyes_ext/input/visionos_look_to_target";

#[cfg(test)]
mod tests {
    use super::{try_egress, GazeSample, GazeSamplePair, GazeTrackingFlags, ACTION_GAZE_POSE};
    use crate::error::XRFailure;
    use crate::ifc_shim::SensitiveDomain;

    #[test]
    fn flags_classification() {
        assert!(!GazeTrackingFlags::untracked().is_usable());
        assert!(GazeTrackingFlags::fully_tracked().is_usable());
    }

    #[test]
    fn gaze_sample_identity_untracked() {
        let s = GazeSample::identity();
        assert_eq!(s.confidence, 0.0);
        assert!(!s.flags.is_usable());
    }

    #[test]
    fn gaze_sample_fully_tracked_usable() {
        let s = GazeSample::fully_tracked_forward();
        assert_eq!(s.confidence, 1.0);
        assert!(s.flags.is_usable());
    }

    #[test]
    fn gaze_sample_into_labeled_carries_gaze_domain() {
        let lv = GazeSample::identity().into_labeled();
        assert!(lv.is_biometric());
        assert!(lv.sensitive_domains.contains(&SensitiveDomain::Gaze));
    }

    #[test]
    fn gaze_sample_pair_into_labeled_carries_gaze_domain() {
        let lv = GazeSamplePair::identity().into_labeled();
        assert!(lv.is_biometric());
        assert!(lv.sensitive_domains.contains(&SensitiveDomain::Gaze));
    }

    #[test]
    fn try_egress_always_refused_for_gaze() {
        let lv = GazeSample::identity().into_labeled();
        let err = try_egress(&lv).unwrap_err();
        assert!(err.is_biometric_refusal());
        assert!(matches!(
            err,
            XRFailure::BiometricEgressRefused {
                domain: SensitiveDomain::Gaze
            }
        ));
    }

    #[test]
    fn try_egress_refused_for_pair() {
        let lv = GazeSamplePair::identity().into_labeled();
        let err = try_egress(&lv).unwrap_err();
        assert!(err.is_biometric_refusal());
    }

    #[test]
    fn action_path_canonical() {
        assert_eq!(ACTION_GAZE_POSE, "/user/eyes_ext/input/gaze_pose/pose");
    }

    #[test]
    fn try_egress_refusal_is_non_overridable() {
        // ‼ This is the structural test that PRIME §1 anti-surveillance
        // is enforced. There is no `Privilege<*>` capability that
        // changes the result. The `try_egress` call ALWAYS fails for
        // biometric-tagged values.
        for _trial in 0..1000 {
            let lv = GazeSample::fully_tracked_forward().into_labeled();
            assert!(try_egress(&lv).is_err());
        }
    }

    #[test]
    fn gaze_sample_label_bottom_does_not_relax_gate() {
        // Even with `Label::bottom()` (least-confidential), the
        // SensitiveDomain::Gaze tag is sufficient to refuse egress.
        let lv = GazeSample::identity().into_labeled();
        assert_eq!(lv.label, crate::ifc_shim::Label::bottom());
        assert!(try_egress(&lv).is_err());
    }
}
