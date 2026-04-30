//! § T11-D260 (W-H3) — FFI integration test : Quest 3s touch-controller
//! action-bindings + input-state round-trip.

use cssl_host_openxr::ffi::{
    Action, ActionSet, ActionType, HapticVibration, HostXrApi, MockInputState, MockOpenXrApi,
    QUEST_3S_TOUCH_CONTROLLER, SUBACTION_PATH_LEFT, SUBACTION_PATH_RIGHT, XrResult,
};

#[test]
fn quest_3s_profile_string_is_canonical() {
    assert_eq!(
        QUEST_3S_TOUCH_CONTROLLER,
        "/interaction_profiles/oculus/touch_controller"
    );
    assert_eq!(SUBACTION_PATH_LEFT, "/user/hand/left");
    assert_eq!(SUBACTION_PATH_RIGHT, "/user/hand/right");
}

#[test]
fn mock_input_state_carries_quest_3s_binding() {
    let mut s = MockInputState::default();
    let r = s.bind_quest_3s(ActionSet(0xC551_A55E));
    assert_eq!(r, XrResult::SUCCESS);
    assert!(s.is_bound_quest_3s());
}

#[test]
fn mock_input_state_round_trips_per_action_value() {
    let mut s = MockInputState::default();
    let trigger_l = Action(101);
    let menu = Action(202);
    s.set_float(trigger_l, 0.85);
    s.set_bool(menu, true);
    assert_eq!(s.float_for(trigger_l), Some(0.85));
    assert_eq!(s.bool_for(menu), Some(true));
    // Overwrite preserves slot identity.
    s.set_float(trigger_l, 0.99);
    assert_eq!(s.float_for(trigger_l), Some(0.99));
    // Missing action returns None.
    assert!(s.bool_for(Action(999)).is_none());
}

#[test]
fn mock_xr_api_routes_input_through_state_cache() {
    let mut api = MockOpenXrApi::new();
    let trigger_r = Action(11);
    api.input_state.set_float(trigger_r, 0.5);
    let _ = api.action_sync();
    let v = api.action_state_float(trigger_r);
    assert_eq!(v, Some(0.5));
    let undef = api.action_state_bool(Action(99));
    assert!(undef.is_none());
}

#[test]
fn mock_xr_api_haptic_records_call() {
    let mut api = MockOpenXrApi::new();
    let r = api.haptic_apply(
        Action(7),
        HapticVibration {
            duration_ns: 100_000_000,
            frequency_hz: 175.0,
            amplitude: 0.75,
        },
    );
    assert_eq!(r, XrResult::SUCCESS);
    assert_eq!(api.haptic_calls, 1);
    let _ = api.haptic_apply(Action(7), HapticVibration::default());
    assert_eq!(api.haptic_calls, 2);
}

#[test]
fn quest_3s_canonical_action_type_distribution() {
    let names = cssl_host_openxr::ffi::input::quest_3s_canonical_action_names();
    let pose_count = names.iter().filter(|(_, t)| *t == ActionType::PoseInput).count();
    let bool_count = names
        .iter()
        .filter(|(_, t)| *t == ActionType::BooleanInput)
        .count();
    let float_count = names
        .iter()
        .filter(|(_, t)| *t == ActionType::FloatInput)
        .count();
    let haptic_count = names
        .iter()
        .filter(|(_, t)| *t == ActionType::VibrationOutput)
        .count();
    // 4 poses (grip+aim per hand) ; ≥ 4 bools (face buttons) ; 4 floats
    // (trigger+squeeze per hand) ; 2 haptics.
    assert_eq!(pose_count, 4);
    assert!(bool_count >= 4);
    assert!(float_count >= 4);
    assert_eq!(haptic_count, 2);
}
