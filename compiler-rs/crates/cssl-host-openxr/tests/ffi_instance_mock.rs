//! § T11-D260 (W-H3) — FFI integration test : mock instance creation +
//! extension enumeration.
//!
//! Confirms `MockInstanceConfig::quest_3s_default` produces an instance
//! that satisfies the minimum-required-extension predicate, and that the
//! Quest-3s runtime advertised-extension catalog includes the canonical
//! Meta extensions (passthrough / body-tracking / face-tracking).

use cssl_host_openxr::ffi::{
    quest_3s_runtime_advertised_extensions, ApiVersion, ApplicationInfo, DispatchTable,
    MockInstance, MockInstanceConfig, XrResult,
};

#[test]
fn quest_3s_instance_create_round_trips_through_destroy() {
    let dt = DispatchTable::unloaded();
    let cfg = MockInstanceConfig::quest_3s_default("CSSL-LoA-Acceptance");
    let mut inst = MockInstance::create(&cfg, &dt).expect("instance");
    assert!(inst.created);
    assert!(!inst.destroyed);
    assert!(inst.quest_3s_minimum_extensions_present());
    assert_eq!(inst.api_version, cssl_host_openxr::ffi::XR_CURRENT_API_VERSION);
    let r = inst.destroy();
    assert_eq!(r, XrResult::SUCCESS);
    assert!(inst.destroyed);
    // Double-destroy is HANDLE_INVALID per spec.
    let r = inst.destroy();
    assert_eq!(r, XrResult::ERROR_HANDLE_INVALID);
}

#[test]
fn instance_create_with_zero_api_version_rejects() {
    let dt = DispatchTable::unloaded();
    let cfg = MockInstanceConfig {
        application_name: "Bad".to_string(),
        enabled_extensions: vec![],
        api_version: ApiVersion(0),
    };
    let r = MockInstance::create(&cfg, &dt);
    assert_eq!(r.unwrap_err(), XrResult::ERROR_API_VERSION_UNSUPPORTED);
}

#[test]
fn quest_3s_runtime_advertises_required_extension_set() {
    let exts = quest_3s_runtime_advertised_extensions();
    let names: Vec<&str> = exts.iter().map(cssl_host_openxr::ffi::ExtensionProperties::name).collect();
    assert!(names.contains(&"XR_KHR_vulkan_enable2"));
    assert!(names.contains(&"XR_FB_passthrough"));
    assert!(names.contains(&"XR_FB_body_tracking"));
    assert!(names.contains(&"XR_FB_face_tracking2"));
    assert!(names.contains(&"XR_FB_display_refresh_rate"));
    assert!(names.contains(&"XR_EXT_eye_gaze_interaction"));
    assert!(names.contains(&"XR_EXT_hand_tracking"));
    assert!(names.contains(&"XR_META_environment_depth"));
}

#[test]
fn cssl_engine_app_info_carries_engine_identifier() {
    let info = ApplicationInfo::cssl_engine("Labyrinth-of-Apockalypse");
    let n = info
        .application_name
        .iter()
        .position(|b| *b == 0)
        .expect("zero terminator");
    assert_eq!(&info.application_name[..n], b"Labyrinth-of-Apockalypse");
    let en = info
        .engine_name
        .iter()
        .position(|b| *b == 0)
        .expect("zero terminator");
    assert_eq!(&info.engine_name[..en], b"CSSLv3");
}
