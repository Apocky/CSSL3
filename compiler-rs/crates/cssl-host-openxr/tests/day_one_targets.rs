//! Day-one tier-1 + secondary-day-one + 5-yr-future target verification.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § II.A + § II.B + § II.C +
//!         § XVI ACCEPTANCE.

use cssl_host_openxr::{
    extensions::XrExtension,
    instance::MockInstance,
    runtime_select::{XrRuntime, XrTarget},
    XrInstanceBuilder, DAY_ONE_TIER_1_SHIP_LIST, FUTURE_5_YEAR_LIST, SECONDARY_DAY_ONE_SHIP_LIST,
};

#[test]
fn day_one_tier_1_is_three_targets_quest3_visionpro_pimax() {
    assert_eq!(DAY_ONE_TIER_1_SHIP_LIST.len(), 3);
    assert!(DAY_ONE_TIER_1_SHIP_LIST.contains(&"Meta Quest 3"));
    assert!(DAY_ONE_TIER_1_SHIP_LIST.contains(&"Apple Vision Pro"));
    assert!(DAY_ONE_TIER_1_SHIP_LIST.contains(&"Pimax Crystal Super"));
}

#[test]
fn day_one_tier_1_targets_each_build_a_session() {
    for t in [
        XrTarget::Quest3,
        XrTarget::VisionPro,
        XrTarget::PimaxCrystalSuper,
    ] {
        let inst = XrInstanceBuilder::new(t).build_mock();
        assert!(inst.is_ok(), "{:?} instance build failed", t);
    }
}

#[test]
fn quest3_target_uses_meta_quest_runtime_and_required_extensions() {
    let target = XrTarget::Quest3;
    assert_eq!(target.runtime(), XrRuntime::MetaQuest);
    assert!(target.is_day_one_tier_1());
    let req = target.default_required_extensions();
    assert!(req.contains(XrExtension::KhrVulkanEnable2));
    assert!(req.contains(XrExtension::FbSpaceWarp));
    assert!(req.contains(XrExtension::FbFoveation));
    assert!(req.contains(XrExtension::FbPassthrough));
    assert!(req.contains(XrExtension::ExtHandTracking));
}

#[test]
fn vision_pro_target_uses_compositor_services_bridge() {
    let target = XrTarget::VisionPro;
    assert_eq!(target.runtime(), XrRuntime::AppleVisionPro);
    assert!(target.runtime().is_compositor_services_bridge());
    assert!(target.is_day_one_tier_1());
}

#[test]
fn pimax_target_uses_pimax_xr_runtime_and_quad_views() {
    let target = XrTarget::PimaxCrystalSuper;
    assert_eq!(target.runtime(), XrRuntime::PimaxXR);
    assert!(target.is_day_one_tier_1());
    let req = target.default_required_extensions();
    assert!(req.contains(XrExtension::VarjoQuadViews));
    assert!(req.contains(XrExtension::VarjoFoveatedRendering));
    assert!(req.contains(XrExtension::ExtEyeGazeInteraction));
    assert!(req.contains(XrExtension::KhrD3D12Enable));
}

#[test]
fn quest3_app_sw_required_per_spec_ii_a_quirk() {
    assert!(XrTarget::Quest3.requires_app_sw());
    assert!(XrTarget::Quest3S.requires_app_sw());
}

#[test]
fn pimax_app_sw_not_required() {
    // PCVR class : engine-side scheduler may engage AppSW under
    // pressure but it is not REQUIRED-shipped.
    assert!(!XrTarget::PimaxCrystalSuper.requires_app_sw());
}

#[test]
fn dfr_default_engaged_on_eye_tracked_targets() {
    assert!(XrTarget::Quest3.dfr_default_engaged());
    assert!(XrTarget::QuestPro.dfr_default_engaged());
    assert!(XrTarget::VisionPro.dfr_default_engaged());
    assert!(XrTarget::PimaxCrystalSuper.dfr_default_engaged());
    assert!(XrTarget::VarjoXr3.dfr_default_engaged());
    assert!(!XrTarget::ValveIndex.dfr_default_engaged());
    assert!(!XrTarget::FlatMonitor.dfr_default_engaged());
}

#[test]
fn flat_monitor_degenerate_runs_same_render_graph() {
    let target = XrTarget::FlatMonitor;
    assert!(!target.is_day_one_tier_1()); // it's degenerate, not tier-1
    assert!(target.is_secondary_day_one()); // listed under § II.B
    let inst = MockInstance::flat_monitor_default().unwrap();
    // No required XR extensions ; only debug-utils may slip in via the
    // cfg(debug_assertions) path (off in release-builds).
    if cfg!(debug_assertions) {
        assert!(inst.enabled_extensions.contains(XrExtension::ExtDebugUtils));
        // No real XR-runtime extensions in the enabled set.
        let has_xr_runtime_ext = inst
            .enabled_extensions
            .iter()
            .any(|e| e != XrExtension::ExtDebugUtils);
        assert!(
            !has_xr_runtime_ext,
            "flat-monitor must not pull XR-runtime extensions"
        );
    } else {
        assert!(inst.enabled_extensions.is_empty());
    }
}

#[test]
fn future_mirror_lake_is_5yr_target() {
    let target = XrTarget::FutureMirrorLake;
    assert!(!target.is_day_one_tier_1());
    assert!(target.is_5yr_future());
    assert!((target.refresh_rate_floor_hz() - 144.0).abs() < f32::EPSILON);
    assert!((target.native_refresh_rate_hz() - 240.0).abs() < f32::EPSILON);
}

#[test]
fn secondary_day_one_includes_thirteen_targets() {
    assert!(SECONDARY_DAY_ONE_SHIP_LIST.len() >= 13);
    assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Pico 4 Ultra"));
    assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"HTC Vive XR Elite"));
    assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Valve Index"));
    assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Bigscreen Beyond"));
    assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Varjo XR-3"));
}

#[test]
fn future_5yr_list_includes_canonical_extensions() {
    let blob = FUTURE_5_YEAR_LIST.join(" | ");
    assert!(blob.to_lowercase().contains("varifocal"));
    assert!(blob.to_lowercase().contains("light-field"));
    assert!(
        blob.to_lowercase().contains("ml-foveated") || blob.to_lowercase().contains("mirror-lake")
    );
    assert!(blob.to_lowercase().contains("rec.2020") || blob.to_lowercase().contains("12-bit"));
    assert!(blob.to_lowercase().contains("kan") || blob.to_lowercase().contains("haptic"));
}

#[test]
fn refresh_rate_floors_at_least_90_for_tier_1() {
    for t in [
        XrTarget::Quest3,
        XrTarget::VisionPro,
        XrTarget::PimaxCrystalSuper,
    ] {
        assert!(t.refresh_rate_floor_hz() >= 90.0, "{:?}", t);
    }
}

#[test]
fn future_mirror_lake_240hz_native() {
    assert!((XrTarget::FutureMirrorLake.native_refresh_rate_hz() - 240.0).abs() < f32::EPSILON);
}

#[test]
fn xr_extension_all_50_unique_canonical_names() {
    let mut seen = std::collections::HashSet::new();
    for ext in XrExtension::ALL {
        assert!(seen.insert(ext.name()));
    }
    assert_eq!(seen.len(), 50);
}
