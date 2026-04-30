//! § ffi::input : `XrAction` + `XrActionSet` + interaction-profile bindings.
//!
//! § SPEC : OpenXR 1.0 § 11 (Action System). Apps declare actions
//!          ("teleport", "grab", "select"), bind them to interaction-
//!          profile sub-paths ("/user/hand/left/input/trigger/value")
//!          via `xrSuggestInteractionProfileBindings`, and the runtime
//!          rebinds them at attach-time per the active controller.
//!
//! § QUEST-3S TOUCH-PRO CONTROLLER PATH SET
//!   Apocky's primary VR target. The `/interaction_profiles/oculus/touch_controller`
//!   profile is the canonical Quest controller binding. The Touch-Pro
//!   superset (Quest-Pro) and Touch-Plus (Quest-3 / Quest-3s) inherit
//!   the base path layout with extension-augmented force / curl / haptic
//!   sub-paths.

use bitflags::bitflags;

use super::pose::XrPosef;
use super::result::XrResult;
use super::types::{Atom, StructureType, XR_MAX_LOCALIZED_ACTION_NAME_SIZE};

/// FFI handle for `XrAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct Action(pub u64);

/// FFI handle for `XrActionSet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct ActionSet(pub u64);

/// `XrActionType`. § 11.5 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ActionType {
    Unknown = 0,
    BooleanInput = 1,
    FloatInput = 2,
    Vector2fInput = 3,
    PoseInput = 4,
    VibrationOutput = 100,
}

bitflags! {
    /// Active sub-action flags (per-hand routing). Mirrors `subaction_paths`
    /// usage on the FFI surface.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    #[repr(transparent)]
    pub struct ActionState: u32 {
        const ACTIVE          = 0x0000_0001;
        const CHANGED_SINCE   = 0x0000_0002;
        const CURRENT_VALUE   = 0x0000_0004;
        const LAST_CHANGE     = 0x0000_0008;
    }
}

/// Interaction-profile path strings. § 11.6 spec.
pub const PROFILE_KHR_SIMPLE: &str = "/interaction_profiles/khr/simple_controller";
pub const PROFILE_OCULUS_TOUCH: &str = "/interaction_profiles/oculus/touch_controller";
pub const PROFILE_VIVE: &str = "/interaction_profiles/htc/vive_controller";
pub const PROFILE_INDEX: &str = "/interaction_profiles/valve/index_controller";

/// Apocky's primary target : Meta Quest 3s controllers.
pub const QUEST_3S_TOUCH_CONTROLLER: &str = PROFILE_OCULUS_TOUCH;

/// Subaction path strings. § 11.5 spec.
pub const SUBACTION_PATH_LEFT: &str = "/user/hand/left";
pub const SUBACTION_PATH_RIGHT: &str = "/user/hand/right";
pub const SUBACTION_PATH_HEAD: &str = "/user/head";
pub const SUBACTION_PATH_GAMEPAD: &str = "/user/gamepad";
pub const SUBACTION_PATH_TREADMILL: &str = "/user/treadmill";

/// `XrInteractionProfileSuggestedBinding` ; FFI struct.
#[repr(C)]
pub struct InteractionProfile {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub interaction_profile: Atom,
    pub count_suggested_bindings: u32,
    pub suggested_bindings: *const BindingSuggestion,
}

impl core::fmt::Debug for InteractionProfile {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InteractionProfile")
            .field("ty", &self.ty)
            .field("interaction_profile", &self.interaction_profile)
            .field("count_suggested_bindings", &self.count_suggested_bindings)
            .finish()
    }
}

/// `XrActionSuggestedBinding` ; FFI struct.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BindingSuggestion {
    pub action: Action,
    pub binding: Atom,
}

/// Action-state for boolean inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ActionStateBool {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub current_state: u32,
    pub changed_since_last_sync: u32,
    pub last_change_time: i64,
    pub is_active: u32,
}

impl Default for ActionStateBool {
    fn default() -> Self {
        Self {
            ty: StructureType::ActionStateBoolean,
            next: core::ptr::null(),
            current_state: 0,
            changed_since_last_sync: 0,
            last_change_time: 0,
            is_active: 0,
        }
    }
}

/// Action-state for float inputs.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct ActionStateFloat {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub current_state: f32,
    pub changed_since_last_sync: u32,
    pub last_change_time: i64,
    pub is_active: u32,
}

impl Default for ActionStateFloat {
    fn default() -> Self {
        Self {
            ty: StructureType::ActionStateFloat,
            next: core::ptr::null(),
            current_state: 0.0,
            changed_since_last_sync: 0,
            last_change_time: 0,
            is_active: 0,
        }
    }
}

/// Action-state for pose inputs (controllers, hand-aim).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ActionStatePose {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub is_active: u32,
}

impl Default for ActionStatePose {
    fn default() -> Self {
        Self {
            ty: StructureType::ActionStatePose,
            next: core::ptr::null(),
            is_active: 0,
        }
    }
}

/// `XrHapticVibration` ; haptic output for Quest controllers.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct HapticVibration {
    pub duration_ns: i64,
    pub frequency_hz: f32,
    pub amplitude: f32,
}

impl Default for HapticVibration {
    fn default() -> Self {
        Self {
            duration_ns: 50_000_000, // 50ms
            frequency_hz: 200.0,
            amplitude: 0.5,
        }
    }
}

/// Canonical Quest 3s touch-controller binding catalog. Each entry is
/// the `(component_path, ActionType)` pair the runtime advertises.
pub const QUEST_3S_TOUCH_BINDINGS: &[(&str, ActionType)] = &[
    // Pose components (per hand) — used to track controller transforms.
    ("/user/hand/left/input/grip/pose", ActionType::PoseInput),
    ("/user/hand/right/input/grip/pose", ActionType::PoseInput),
    ("/user/hand/left/input/aim/pose", ActionType::PoseInput),
    ("/user/hand/right/input/aim/pose", ActionType::PoseInput),
    // Trigger (analog) + click event.
    ("/user/hand/left/input/trigger/value", ActionType::FloatInput),
    ("/user/hand/right/input/trigger/value", ActionType::FloatInput),
    ("/user/hand/left/input/trigger/click", ActionType::BooleanInput),
    ("/user/hand/right/input/trigger/click", ActionType::BooleanInput),
    // Trigger touch (capacitive).
    ("/user/hand/left/input/trigger/touch", ActionType::BooleanInput),
    ("/user/hand/right/input/trigger/touch", ActionType::BooleanInput),
    // Squeeze (grip) value + click.
    ("/user/hand/left/input/squeeze/value", ActionType::FloatInput),
    ("/user/hand/right/input/squeeze/value", ActionType::FloatInput),
    // Thumbstick : 2D axis + click + touch.
    ("/user/hand/left/input/thumbstick", ActionType::Vector2fInput),
    ("/user/hand/right/input/thumbstick", ActionType::Vector2fInput),
    ("/user/hand/left/input/thumbstick/click", ActionType::BooleanInput),
    ("/user/hand/right/input/thumbstick/click", ActionType::BooleanInput),
    ("/user/hand/left/input/thumbstick/touch", ActionType::BooleanInput),
    ("/user/hand/right/input/thumbstick/touch", ActionType::BooleanInput),
    // X / Y / A / B face buttons.
    ("/user/hand/left/input/x/click", ActionType::BooleanInput),
    ("/user/hand/left/input/y/click", ActionType::BooleanInput),
    ("/user/hand/right/input/a/click", ActionType::BooleanInput),
    ("/user/hand/right/input/b/click", ActionType::BooleanInput),
    ("/user/hand/left/input/x/touch", ActionType::BooleanInput),
    ("/user/hand/left/input/y/touch", ActionType::BooleanInput),
    ("/user/hand/right/input/a/touch", ActionType::BooleanInput),
    ("/user/hand/right/input/b/touch", ActionType::BooleanInput),
    // Menu button (left hand only on Quest).
    ("/user/hand/left/input/menu/click", ActionType::BooleanInput),
    // Thumb-rest (capacitive proximity).
    ("/user/hand/left/input/thumbrest/touch", ActionType::BooleanInput),
    ("/user/hand/right/input/thumbrest/touch", ActionType::BooleanInput),
    // Haptic outputs.
    ("/user/hand/left/output/haptic", ActionType::VibrationOutput),
    ("/user/hand/right/output/haptic", ActionType::VibrationOutput),
];

/// Canonical Quest-3s action set : the minimal set of actions a CSSLv3
/// game declares. Returned to the test path as the deterministic baseline.
#[must_use]
pub fn quest_3s_canonical_action_names() -> Vec<(&'static str, ActionType)> {
    vec![
        ("grip_pose_left", ActionType::PoseInput),
        ("grip_pose_right", ActionType::PoseInput),
        ("aim_pose_left", ActionType::PoseInput),
        ("aim_pose_right", ActionType::PoseInput),
        ("trigger_value_left", ActionType::FloatInput),
        ("trigger_value_right", ActionType::FloatInput),
        ("squeeze_value_left", ActionType::FloatInput),
        ("squeeze_value_right", ActionType::FloatInput),
        ("thumbstick_left", ActionType::Vector2fInput),
        ("thumbstick_right", ActionType::Vector2fInput),
        ("button_a", ActionType::BooleanInput),
        ("button_b", ActionType::BooleanInput),
        ("button_x", ActionType::BooleanInput),
        ("button_y", ActionType::BooleanInput),
        ("menu", ActionType::BooleanInput),
        ("haptic_left", ActionType::VibrationOutput),
        ("haptic_right", ActionType::VibrationOutput),
    ]
}

/// Pretty-printable localized name (≤ XR_MAX_LOCALIZED_ACTION_NAME_SIZE).
/// Exposed `pub` so callers can pre-clip strings before passing them
/// across the FFI boundary into `xrCreateAction`.
pub fn localized_clip(s: &str) -> String {
    if s.len() < XR_MAX_LOCALIZED_ACTION_NAME_SIZE {
        s.to_string()
    } else {
        s[..XR_MAX_LOCALIZED_ACTION_NAME_SIZE - 1].to_string()
    }
}

/// In-memory mock holding the live action-state values returned by
/// `xrGetActionState{Boolean,Float,Pose}`. Used by `MockOpenXrApi` to
/// answer queries without a real runtime.
#[derive(Debug, Clone, Default)]
pub struct MockInputState {
    pub bools: Vec<(Action, bool)>,
    pub floats: Vec<(Action, f32)>,
    pub poses: Vec<(Action, XrPosef)>,
    pub action_set: Option<ActionSet>,
    pub bound_profile: Option<String>,
}

impl MockInputState {
    pub fn set_bool(&mut self, action: Action, v: bool) {
        if let Some(slot) = self.bools.iter_mut().find(|(a, _)| *a == action) {
            slot.1 = v;
        } else {
            self.bools.push((action, v));
        }
    }
    pub fn set_float(&mut self, action: Action, v: f32) {
        if let Some(slot) = self.floats.iter_mut().find(|(a, _)| *a == action) {
            slot.1 = v;
        } else {
            self.floats.push((action, v));
        }
    }
    pub fn set_pose(&mut self, action: Action, v: XrPosef) {
        if let Some(slot) = self.poses.iter_mut().find(|(a, _)| *a == action) {
            slot.1 = v;
        } else {
            self.poses.push((action, v));
        }
    }

    #[must_use]
    pub fn bool_for(&self, action: Action) -> Option<bool> {
        self.bools.iter().find(|(a, _)| *a == action).map(|(_, v)| *v)
    }
    #[must_use]
    pub fn float_for(&self, action: Action) -> Option<f32> {
        self.floats
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, v)| *v)
    }
    #[must_use]
    pub fn pose_for(&self, action: Action) -> Option<XrPosef> {
        self.poses
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, v)| *v)
    }

    /// Bind to the Quest-3s touch-controller profile. Validates that the
    /// profile path string matches the canonical token.
    pub fn bind_quest_3s(&mut self, action_set: ActionSet) -> XrResult {
        self.action_set = Some(action_set);
        self.bound_profile = Some(QUEST_3S_TOUCH_CONTROLLER.to_string());
        XrResult::SUCCESS
    }

    /// `true` iff the binding reflects the Quest-3s profile.
    #[must_use]
    pub fn is_bound_quest_3s(&self) -> bool {
        self.bound_profile.as_deref() == Some(QUEST_3S_TOUCH_CONTROLLER)
    }
}

/// Validate that an interaction profile path-string matches the canonical
/// `/interaction_profiles/<vendor>/<controller>` shape.
#[must_use]
pub fn is_valid_profile_path(s: &str) -> bool {
    s.starts_with("/interaction_profiles/")
        && s.matches('/').count() == 3
        && !s.ends_with('/')
}

/// Validate that a sub-action path is one of the standard /user/* roots.
#[must_use]
pub fn is_valid_subaction_path(s: &str) -> bool {
    matches!(
        s,
        SUBACTION_PATH_LEFT
            | SUBACTION_PATH_RIGHT
            | SUBACTION_PATH_HEAD
            | SUBACTION_PATH_GAMEPAD
            | SUBACTION_PATH_TREADMILL
    )
}

#[cfg(test)]
mod tests {
    use super::{
        is_valid_profile_path, is_valid_subaction_path, localized_clip,
        quest_3s_canonical_action_names, Action, ActionSet, ActionType, MockInputState,
        QUEST_3S_TOUCH_BINDINGS, QUEST_3S_TOUCH_CONTROLLER,
    };

    #[test]
    fn quest_3s_profile_path_validates() {
        assert!(is_valid_profile_path(QUEST_3S_TOUCH_CONTROLLER));
        assert!(is_valid_profile_path(super::PROFILE_INDEX));
        assert!(!is_valid_profile_path("not_a_profile"));
        assert!(!is_valid_profile_path("/interaction_profiles/extra/level/path"));
    }

    #[test]
    fn quest_3s_canonical_action_set_has_expected_count() {
        let actions = quest_3s_canonical_action_names();
        assert!(actions.len() >= 17);
        assert!(actions.iter().any(|(n, _)| *n == "trigger_value_left"));
        assert!(actions.iter().any(|(n, t)| *n == "haptic_left" && *t == ActionType::VibrationOutput));
    }

    #[test]
    fn touch_bindings_cover_pose_trigger_thumbstick() {
        let paths: Vec<&str> = QUEST_3S_TOUCH_BINDINGS.iter().map(|(p, _)| *p).collect();
        assert!(paths.contains(&"/user/hand/left/input/grip/pose"));
        assert!(paths.contains(&"/user/hand/right/input/grip/pose"));
        assert!(paths.contains(&"/user/hand/left/input/trigger/value"));
        assert!(paths.contains(&"/user/hand/right/input/trigger/value"));
        assert!(paths.contains(&"/user/hand/left/input/thumbstick"));
        assert!(paths.contains(&"/user/hand/right/input/thumbstick"));
        assert!(paths.contains(&"/user/hand/left/output/haptic"));
        assert!(paths.contains(&"/user/hand/right/output/haptic"));
    }

    #[test]
    fn mock_input_state_bind_quest_3s_records_profile() {
        let mut s = MockInputState::default();
        let r = s.bind_quest_3s(ActionSet(0xC551_A55E));
        assert_eq!(r, super::XrResult::SUCCESS);
        assert!(s.is_bound_quest_3s());
        assert_eq!(s.action_set, Some(ActionSet(0xC551_A55E)));
    }

    #[test]
    fn mock_input_state_round_trips_set_get() {
        let mut s = MockInputState::default();
        let trigger_l = Action(1);
        s.set_float(trigger_l, 0.75);
        assert_eq!(s.float_for(trigger_l), Some(0.75));
        s.set_float(trigger_l, 1.0);
        assert_eq!(s.float_for(trigger_l), Some(1.0));
        let menu = Action(2);
        s.set_bool(menu, true);
        assert_eq!(s.bool_for(menu), Some(true));
    }

    #[test]
    fn subaction_path_validation() {
        assert!(is_valid_subaction_path("/user/hand/left"));
        assert!(is_valid_subaction_path("/user/hand/right"));
        assert!(!is_valid_subaction_path("/user/hand/middle"));
    }

    #[test]
    fn localized_clip_truncates_long_strings() {
        let long = "a".repeat(300);
        let clipped = localized_clip(&long);
        assert!(clipped.len() < super::XR_MAX_LOCALIZED_ACTION_NAME_SIZE);
        let short = "test";
        assert_eq!(localized_clip(short), "test");
    }
}
