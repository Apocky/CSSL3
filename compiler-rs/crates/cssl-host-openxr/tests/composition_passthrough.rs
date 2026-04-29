//! Composition layer + passthrough integration tests.
//!
//! § SPEC § IX + § XI.E.

use cssl_host_openxr::{
    composition::{
        CompositionLayerStack, CubeLayerParams, CylinderLayerParams, EquirectLayerParams,
        QuadLayerParams, XrCompositionLayer,
    },
    passthrough::{AlphaMode, PassthroughConfig, PassthroughLayer, PassthroughProvider},
    view::{identity_mat4, ViewSet},
};

#[test]
fn quad_layer_for_ui_validates_in_stack_with_projection() {
    let vs = ViewSet::stereo_identity(64.0);
    let mut s = CompositionLayerStack::empty();
    s.push(XrCompositionLayer::projection(&vs, &[100, 200], Some(&[300, 400])).unwrap());
    let p = QuadLayerParams {
        size_m: [1.6, 0.9],
        pose: identity_mat4(),
        texture: 1234,
        width: 1920,
        height: 1080,
    };
    s.push(XrCompositionLayer::quad(p));
    assert_eq!(s.len(), 2);
    assert!(s.validate().is_ok());
}

#[test]
fn cylinder_layer_for_wraparound_ui() {
    let vs = ViewSet::stereo_identity(64.0);
    let mut s = CompositionLayerStack::empty();
    s.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
    let p = CylinderLayerParams {
        radius_m: 1.5,
        central_angle_rad: std::f32::consts::FRAC_PI_2,
        aspect_ratio: 16.0 / 9.0,
        pose: identity_mat4(),
        texture: 5678,
        width: 2048,
        height: 1152,
    };
    s.push(XrCompositionLayer::cylinder(p));
    assert!(s.validate().is_ok());
}

#[test]
fn equirect_layer_for_360_video() {
    let vs = ViewSet::stereo_identity(64.0);
    let mut s = CompositionLayerStack::empty();
    s.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
    let p = EquirectLayerParams {
        pose: identity_mat4(),
        radius_m: 50.0,
        central_horizontal_angle_rad: std::f32::consts::TAU,
        upper_vertical_angle_rad: std::f32::consts::FRAC_PI_2,
        lower_vertical_angle_rad: -std::f32::consts::FRAC_PI_2,
        texture: 0xc0debabe,
    };
    s.push(XrCompositionLayer::equirect(p));
    assert!(s.validate().is_ok());
}

#[test]
fn cube_layer_for_skybox() {
    let vs = ViewSet::stereo_identity(64.0);
    let mut s = CompositionLayerStack::empty();
    s.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
    let p = CubeLayerParams {
        orientation: [0.0, 0.0, 0.0, 1.0],
        cubemap: 0xfeed,
    };
    s.push(XrCompositionLayer::cube(p));
    assert!(s.validate().is_ok());
}

#[test]
fn quest3_passthrough_layer_full_features() {
    let cfg = PassthroughConfig::quest3_default();
    assert!(cfg.environment_depth);
    assert!(cfg.hand_cutout);
    let layer = PassthroughLayer::from_config(cfg).unwrap();
    let mut s = CompositionLayerStack::empty();
    s.push(XrCompositionLayer::Passthrough(layer));
    assert!(s.validate().is_ok());
}

#[test]
fn vision_pro_passthrough_compositor_managed() {
    let cfg = PassthroughConfig::vision_pro_default();
    assert_eq!(cfg.provider, PassthroughProvider::AppleCompositorServices);
    assert!(cfg.environment_depth); // Apple uses scene-mesh
    assert!(!cfg.hand_cutout);      // Apple handles internally
    let layer = PassthroughLayer::from_config(cfg).unwrap();
    let mut s = CompositionLayerStack::empty();
    s.push(XrCompositionLayer::Passthrough(layer));
    assert!(s.validate().is_ok());
}

#[test]
fn passthrough_alpha_modes_canonical() {
    assert_eq!(AlphaMode::Additive.as_str(), "additive");
    assert_eq!(AlphaMode::Over.as_str(), "over");
    assert_eq!(AlphaMode::Under.as_str(), "under");
}

#[test]
fn full_composition_stack_projection_plus_passthrough_plus_ui() {
    let vs = ViewSet::stereo_identity(64.0);
    let mut s = CompositionLayerStack::empty();
    let pt = PassthroughLayer::from_config(PassthroughConfig::quest3_default()).unwrap();
    s.push(XrCompositionLayer::Passthrough(pt));
    s.push(XrCompositionLayer::projection(&vs, &[1, 2], Some(&[3, 4])).unwrap());
    let p = QuadLayerParams {
        size_m: [1.0, 1.0],
        pose: identity_mat4(),
        texture: 5,
        width: 256,
        height: 256,
    };
    s.push(XrCompositionLayer::quad(p));
    assert_eq!(s.len(), 3);
    assert!(s.validate().is_ok());
}

#[test]
fn empty_stack_rejected() {
    let s = CompositionLayerStack::empty();
    assert!(s.validate().is_err());
}

#[test]
fn quad_only_stack_rejected_no_primary() {
    let mut s = CompositionLayerStack::empty();
    let p = QuadLayerParams {
        size_m: [1.0, 1.0],
        pose: identity_mat4(),
        texture: 1,
        width: 256,
        height: 256,
    };
    s.push(XrCompositionLayer::quad(p));
    assert!(s.validate().is_err());
}
