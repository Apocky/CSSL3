//! § PRIME-DIRECTIVE §1 anti-surveillance ATTESTATION TESTS
//!
//! These tests are the **structural-gate verification** that the
//! `cssl-host-openxr` crate **cannot** egress biometric tracking-data
//! to a non-on-device sink. Every test here SHOULD demonstrate
//! `Err(BiometricEgressRefused { ... })` ; if any test passes by some
//! other path (Ok), the crate is non-compliant + must be refused at CI.

#![allow(clippy::uninlined_format_args)]

use cssl_host_openxr::SensitiveDomain;
use cssl_host_openxr::{
    body::{BodySkeleton, BodyTrackingProvider},
    eye_gaze::{try_egress, GazeSample, GazeSamplePair},
    face::{FaceTrackingProvider, FaceWeights},
    foveation::{DFRFoveator, Foveator, GazePrediction, MLFoveator},
    hand::{HandSide, HandSkeleton},
    XRFailure,
};

#[test]
fn gaze_sample_egress_refused() {
    let lv = GazeSample::fully_tracked_forward().into_labeled();
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
fn gaze_sample_pair_egress_refused() {
    let lv = GazeSamplePair::identity().into_labeled();
    let err = try_egress(&lv).unwrap_err();
    assert!(err.is_biometric_refusal());
}

#[test]
fn hand_skeleton_egress_refused_left_and_right() {
    for side in [HandSide::Left, HandSide::Right] {
        let lv = HandSkeleton::identity(side).into_labeled();
        let err = try_egress(&lv).unwrap_err();
        assert!(err.is_biometric_refusal());
        assert!(matches!(
            err,
            XRFailure::BiometricEgressRefused {
                domain: SensitiveDomain::Body
            }
        ));
    }
}

#[test]
fn body_skeleton_egress_refused_all_providers() {
    for prov in [
        BodyTrackingProvider::MetaFb,
        BodyTrackingProvider::Htc,
        BodyTrackingProvider::PicoBd,
        BodyTrackingProvider::AppleArkit,
    ] {
        let lv = BodySkeleton::identity(prov).into_labeled();
        let err = try_egress(&lv).unwrap_err();
        assert!(err.is_biometric_refusal(), "{:?}", prov);
    }
}

#[test]
fn face_weights_egress_refused_all_providers() {
    for prov in [
        FaceTrackingProvider::MetaFb2,
        FaceTrackingProvider::Htc,
        FaceTrackingProvider::AppleArkit,
    ] {
        let lv = FaceWeights::identity(prov).into_labeled();
        let err = try_egress(&lv).unwrap_err();
        assert!(err.is_biometric_refusal(), "{:?}", prov);
        assert!(matches!(
            err,
            XRFailure::BiometricEgressRefused {
                domain: SensitiveDomain::Face
            }
        ));
    }
}

#[test]
fn dfr_foveator_consumes_gaze_on_device_no_egress_path() {
    // The DFR foveator reads the on-device gaze prediction and produces
    // a foveation-config. There is no path through this call that
    // egresses the gaze value — it stays inside the LabeledValue<>
    // wrapper for the lifetime of `config_for_frame`.
    let mut f = DFRFoveator::aggressive();
    let vs = cssl_host_openxr::view::ViewSet::stereo_identity(64.0);
    let gaze = GazePrediction::identity().into_labeled();
    let cfg = f.config_for_frame(&vs, Some(&gaze));
    // The config carries no biometric payload itself ; it's a render-
    // side rate-map only.
    let _ = cfg.profile;
    let _ = cfg.dfr_engaged;
    // Re-attempt egress on the original gaze ; still refused.
    assert!(try_egress(&gaze).is_err());
}

#[test]
fn ml_foveator_consumes_gaze_on_device_no_egress_path() {
    let mut f = MLFoveator::stub();
    let vs = cssl_host_openxr::view::ViewSet::stereo_identity(64.0);
    let gaze = GazePrediction::identity().into_labeled();
    let _ = f.config_for_frame(&vs, Some(&gaze));
    assert!(try_egress(&gaze).is_err());
}

#[test]
fn many_samples_each_individually_refused() {
    // Bulk : ensure the gate is non-flaky / non-stateful.
    for _ in 0..1000 {
        let g = GazeSample::fully_tracked_forward().into_labeled();
        let h = HandSkeleton::identity(HandSide::Left).into_labeled();
        let b = BodySkeleton::identity(BodyTrackingProvider::MetaFb).into_labeled();
        let f = FaceWeights::identity(FaceTrackingProvider::MetaFb2).into_labeled();
        assert!(try_egress(&g).is_err());
        assert!(try_egress(&h).is_err());
        assert!(try_egress(&b).is_err());
        assert!(try_egress(&f).is_err());
    }
}

#[test]
fn cssl_ifc_validate_egress_returns_biometric_refused() {
    use cssl_host_openxr::{validate_egress, EgressGrantError};
    // Direct against cssl-ifc validate_egress ⇒ must return BiometricRefused.
    let g = GazeSample::fully_tracked_forward().into_labeled();
    let res = validate_egress(&g);
    assert!(matches!(
        res,
        Err(EgressGrantError::BiometricRefused {
            domain: SensitiveDomain::Gaze
        })
    ));
}

#[test]
fn frame_loop_locate_gaze_returns_labeled_with_gaze_domain() {
    use cssl_host_openxr::{
        comfort::JudderDetector,
        foveation::FFRFoveator,
        instance::MockInstance,
        session::{GraphicsBinding, MockSession},
        space_warp::AppSwScheduler,
        FrameLoop,
    };
    let inst = MockInstance::quest3_default().unwrap();
    let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
    s.run_to_focused();
    let mut appsw = AppSwScheduler::quest3_default();
    let mut judder = JudderDetector::quest3_default();
    let mut fov = FFRFoveator::default_high();
    let fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
    let gaze = fl.locate_gaze().unwrap();
    assert!(gaze.is_biometric());
    assert!(gaze.is_egress_banned());
    // Re-confirm via try_egress :
    let err = try_egress(&gaze).unwrap_err();
    assert!(err.is_biometric_refusal());
}
