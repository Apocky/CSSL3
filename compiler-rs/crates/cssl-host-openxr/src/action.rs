//! Action-set + action-binding primitives.
//!
//! § SPEC : OpenXR Input System (action-set + action + interaction-profile +
//!         binding) per `07_AESTHETIC/05_VR_RENDERING.csl` § XII.B (comfort
//!         suite : snap-turn / smooth-turn / teleport).
//!
//! § DESIGN
//!   - `ActionType` mirrors `XrActionType` enum.
//!   - `Action` carries name + type + sub-action-paths.
//!   - `ActionSet` is the canonical bundle of related actions.
//!   - `InteractionProfile` enumerates the controller-profile bindings
//!     OpenXR knows (Meta Touch, HTC Vive, Valve Index, etc.).

use crate::error::XRFailure;

/// `XrActionType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionType {
    /// `XR_ACTION_TYPE_BOOLEAN_INPUT`.
    BooleanInput,
    /// `XR_ACTION_TYPE_FLOAT_INPUT`.
    FloatInput,
    /// `XR_ACTION_TYPE_VECTOR2F_INPUT` (e.g. thumbstick).
    Vector2fInput,
    /// `XR_ACTION_TYPE_POSE_INPUT`.
    PoseInput,
    /// `XR_ACTION_TYPE_VIBRATION_OUTPUT`.
    VibrationOutput,
}

impl ActionType {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BooleanInput => "boolean",
            Self::FloatInput => "float",
            Self::Vector2fInput => "vector2f",
            Self::PoseInput => "pose",
            Self::VibrationOutput => "vibration",
        }
    }

    /// `true` iff this is an input-action (vs. output).
    #[must_use]
    pub const fn is_input(self) -> bool {
        !matches!(self, Self::VibrationOutput)
    }
}

/// Interaction-profile : the controller / hand / eye binding spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionProfile {
    /// Khronos simple-controller profile (cross-vendor minimal).
    KhrSimple,
    /// `/interaction_profiles/oculus/touch_controller`.
    OculusTouch,
    /// `/interaction_profiles/oculus/touch_controller_pro` (Quest Pro).
    OculusTouchPro,
    /// `/interaction_profiles/meta/touch_pro_controller`.
    MetaTouchPro,
    /// `/interaction_profiles/meta/touch_plus_controller` (Quest 3).
    MetaTouchPlus,
    /// `/interaction_profiles/htc/vive_controller`.
    HtcVive,
    /// `/interaction_profiles/htc/vive_focus3_controller`.
    HtcViveFocus3,
    /// `/interaction_profiles/valve/index_controller`.
    ValveIndex,
    /// `/interaction_profiles/microsoft/motion_controller`.
    MicrosoftMixedReality,
    /// `/interaction_profiles/bytedance/pico_neo3_controller`.
    PicoNeo3,
    /// `/interaction_profiles/bytedance/pico4_controller`.
    Pico4,
    /// `/interaction_profiles/ext/eye_gaze_interaction`.
    ExtEyeGaze,
    /// `/interaction_profiles/ext/hand_interaction_ext`.
    ExtHandInteraction,
    /// `/interaction_profiles/visionos/hand_pinch` (visionOS canonical).
    VisionOsPinch,
}

impl InteractionProfile {
    /// Canonical OpenXR path.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::KhrSimple => "/interaction_profiles/khr/simple_controller",
            Self::OculusTouch => "/interaction_profiles/oculus/touch_controller",
            Self::OculusTouchPro => "/interaction_profiles/oculus/touch_controller_pro",
            Self::MetaTouchPro => "/interaction_profiles/meta/touch_pro_controller",
            Self::MetaTouchPlus => "/interaction_profiles/meta/touch_plus_controller",
            Self::HtcVive => "/interaction_profiles/htc/vive_controller",
            Self::HtcViveFocus3 => "/interaction_profiles/htc/vive_focus3_controller",
            Self::ValveIndex => "/interaction_profiles/valve/index_controller",
            Self::MicrosoftMixedReality => "/interaction_profiles/microsoft/motion_controller",
            Self::PicoNeo3 => "/interaction_profiles/bytedance/pico_neo3_controller",
            Self::Pico4 => "/interaction_profiles/bytedance/pico4_controller",
            Self::ExtEyeGaze => "/interaction_profiles/ext/eye_gaze_interaction",
            Self::ExtHandInteraction => "/interaction_profiles/ext/hand_interaction_ext",
            Self::VisionOsPinch => "/interaction_profiles/visionos/hand_pinch",
        }
    }

    /// All profiles.
    pub const ALL: [Self; 14] = [
        Self::KhrSimple,
        Self::OculusTouch,
        Self::OculusTouchPro,
        Self::MetaTouchPro,
        Self::MetaTouchPlus,
        Self::HtcVive,
        Self::HtcViveFocus3,
        Self::ValveIndex,
        Self::MicrosoftMixedReality,
        Self::PicoNeo3,
        Self::Pico4,
        Self::ExtEyeGaze,
        Self::ExtHandInteraction,
        Self::VisionOsPinch,
    ];
}

/// Single action.
#[derive(Debug, Clone)]
pub struct Action {
    /// Action-name (lowercase, dashes, OpenXR-validated).
    pub name: String,
    /// Localized name shown to user in OpenXR-runtime UI.
    pub localized_name: String,
    /// Action-type.
    pub kind: ActionType,
    /// Sub-action-paths (e.g. `/user/hand/left`, `/user/hand/right`).
    pub sub_action_paths: Vec<String>,
}

impl Action {
    /// New action.
    #[must_use]
    pub fn new(name: &str, localized_name: &str, kind: ActionType) -> Self {
        Self {
            name: name.to_string(),
            localized_name: localized_name.to_string(),
            kind,
            sub_action_paths: Vec::new(),
        }
    }

    /// Add a sub-action-path (`/user/hand/left` etc.).
    pub fn with_sub_action_path(mut self, path: &str) -> Self {
        self.sub_action_paths.push(path.to_string());
        self
    }

    /// Validate : name lowercase + dashes only + non-empty.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if self.name.is_empty() {
            return Err(XRFailure::ActionSetInstall { code: -70 });
        }
        for c in self.name.chars() {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
                return Err(XRFailure::ActionSetInstall { code: -71 });
            }
        }
        Ok(())
    }
}

/// Action-set : a bundle of related actions, attached to a session.
#[derive(Debug, Clone)]
pub struct ActionSet {
    /// Canonical name.
    pub name: String,
    /// Localized name (UI).
    pub localized_name: String,
    /// Priority (higher = wins binding-conflict).
    pub priority: u32,
    /// Actions in this set.
    pub actions: Vec<Action>,
}

impl ActionSet {
    /// New empty action-set.
    #[must_use]
    pub fn new(name: &str, localized_name: &str, priority: u32) -> Self {
        Self {
            name: name.to_string(),
            localized_name: localized_name.to_string(),
            priority,
            actions: Vec::new(),
        }
    }

    /// Add an action.
    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// Validate the set.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if self.name.is_empty() {
            return Err(XRFailure::ActionSetInstall { code: -72 });
        }
        for a in &self.actions {
            a.validate()?;
        }
        Ok(())
    }

    /// Build a default Omniverse action-set : navigation + UI + comfort.
    /// § XII.B comfort-suite (snap-turn / smooth-turn / teleport).
    pub fn omniverse_default() -> Self {
        let mut s = Self::new("omniverse", "Omniverse", 1);
        // Locomotion
        s.push(
            Action::new("locomotion-stick", "Locomotion (thumbstick)", ActionType::Vector2fInput)
                .with_sub_action_path("/user/hand/left")
                .with_sub_action_path("/user/hand/right"),
        );
        s.push(
            Action::new("snap-turn", "Snap turn", ActionType::BooleanInput)
                .with_sub_action_path("/user/hand/right"),
        );
        s.push(
            Action::new("smooth-turn", "Smooth turn", ActionType::Vector2fInput)
                .with_sub_action_path("/user/hand/right"),
        );
        s.push(
            Action::new("teleport", "Teleport", ActionType::BooleanInput)
                .with_sub_action_path("/user/hand/left"),
        );
        // UI
        s.push(
            Action::new("ui-select", "UI select", ActionType::BooleanInput)
                .with_sub_action_path("/user/hand/left")
                .with_sub_action_path("/user/hand/right"),
        );
        s.push(
            Action::new("ui-menu", "UI menu", ActionType::BooleanInput)
                .with_sub_action_path("/user/hand/left"),
        );
        // Pose for grip/aim
        s.push(
            Action::new("grip-pose", "Grip pose", ActionType::PoseInput)
                .with_sub_action_path("/user/hand/left")
                .with_sub_action_path("/user/hand/right"),
        );
        s.push(
            Action::new("aim-pose", "Aim pose", ActionType::PoseInput)
                .with_sub_action_path("/user/hand/left")
                .with_sub_action_path("/user/hand/right"),
        );
        // Haptics
        s.push(
            Action::new("haptic", "Haptic", ActionType::VibrationOutput)
                .with_sub_action_path("/user/hand/left")
                .with_sub_action_path("/user/hand/right"),
        );
        s
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, ActionSet, ActionType, InteractionProfile};

    #[test]
    fn action_type_input_classification() {
        assert!(ActionType::BooleanInput.is_input());
        assert!(ActionType::FloatInput.is_input());
        assert!(ActionType::Vector2fInput.is_input());
        assert!(ActionType::PoseInput.is_input());
        assert!(!ActionType::VibrationOutput.is_input());
    }

    #[test]
    fn interaction_profile_paths_canonical() {
        assert_eq!(
            InteractionProfile::OculusTouch.path(),
            "/interaction_profiles/oculus/touch_controller"
        );
        assert_eq!(
            InteractionProfile::ValveIndex.path(),
            "/interaction_profiles/valve/index_controller"
        );
        assert_eq!(
            InteractionProfile::ExtEyeGaze.path(),
            "/interaction_profiles/ext/eye_gaze_interaction"
        );
    }

    #[test]
    fn interaction_profile_all_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in InteractionProfile::ALL {
            assert!(seen.insert(p.path()), "dup : {}", p.path());
        }
        assert_eq!(seen.len(), 14);
    }

    #[test]
    fn action_validates_lowercase_name() {
        let a = Action::new("snap-turn", "Snap Turn", ActionType::BooleanInput);
        assert!(a.validate().is_ok());
    }

    #[test]
    fn action_rejects_uppercase_name() {
        let a = Action::new("SnapTurn", "Snap Turn", ActionType::BooleanInput);
        assert!(a.validate().is_err());
    }

    #[test]
    fn action_rejects_empty_name() {
        let a = Action::new("", "Empty", ActionType::BooleanInput);
        assert!(a.validate().is_err());
    }

    #[test]
    fn action_rejects_special_chars() {
        let a = Action::new("snap turn", "Snap Turn", ActionType::BooleanInput);
        assert!(a.validate().is_err());
    }

    #[test]
    fn action_set_omniverse_default_validates() {
        let s = ActionSet::omniverse_default();
        assert!(s.validate().is_ok());
        // Sanity : we expect ≥ 8 actions in the default set.
        assert!(s.actions.len() >= 8);
    }

    #[test]
    fn action_set_omniverse_has_locomotion_and_haptic() {
        let s = ActionSet::omniverse_default();
        let names: Vec<_> = s.actions.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"locomotion-stick"));
        assert!(names.contains(&"snap-turn"));
        assert!(names.contains(&"teleport"));
        assert!(names.contains(&"haptic"));
        assert!(names.contains(&"grip-pose"));
        assert!(names.contains(&"aim-pose"));
    }

    #[test]
    fn action_with_sub_paths() {
        let a = Action::new("grip", "Grip", ActionType::PoseInput)
            .with_sub_action_path("/user/hand/left")
            .with_sub_action_path("/user/hand/right");
        assert_eq!(a.sub_action_paths.len(), 2);
    }
}
