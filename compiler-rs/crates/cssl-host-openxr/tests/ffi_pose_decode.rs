//! § T11-D260 (W-H3) — FFI integration test : pose decode + locate-space.
//!
//! Confirms `decode_posef` re-normalizes degenerate quaternions and
//! `mock_locate_space` returns valid pose-data when the source space
//! advertises tracking-valid flags.

#![allow(clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]

use cssl_host_openxr::ffi::{
    decode_posef, identity_quaternion, identity_vector3f, mock_locate_space, MockSpace,
    Quaternionf, SpaceLocationFlags, Vector3f, XrDuration, XrPosef, XrTime,
};

#[test]
fn renormalizes_arbitrary_magnitude_quaternion() {
    let raw = XrPosef {
        orientation: Quaternionf {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            w: 4.0,
        },
        position: Vector3f {
            x: 0.5,
            y: 1.6,
            z: -0.25,
        },
    };
    let out = decode_posef(raw);
    let m = out.orientation.x.powi(2)
        + out.orientation.y.powi(2)
        + out.orientation.z.powi(2)
        + out.orientation.w.powi(2);
    assert!((m - 1.0).abs() < 1e-6);
    // Position is preserved unchanged.
    assert_eq!(out.position, raw.position);
}

#[test]
fn zero_magnitude_quaternion_falls_back_to_identity() {
    let raw = XrPosef {
        orientation: Quaternionf {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
        position: Vector3f {
            x: 0.0,
            y: 1.6,
            z: 0.0,
        },
    };
    let out = decode_posef(raw);
    assert_eq!(out.orientation, identity_quaternion());
    assert_eq!(out.position.y, 1.6);
}

#[test]
fn quest_3s_view_locate_returns_eye_height_pose() {
    let view = MockSpace::quest_3s_view_at_neutral();
    let stage = MockSpace::stage_origin();
    let loc = mock_locate_space(&view, &stage, XrTime(1_000));
    assert!(loc
        .location_flags
        .contains(SpaceLocationFlags::ORIENTATION_VALID));
    assert!(loc
        .location_flags
        .contains(SpaceLocationFlags::POSITION_VALID));
    assert!(loc
        .location_flags
        .contains(SpaceLocationFlags::ORIENTATION_TRACKED));
    assert!(loc
        .location_flags
        .contains(SpaceLocationFlags::POSITION_TRACKED));
    assert!(loc.orientation_ok());
    assert!(loc.position_ok());
    assert!((loc.pose.position.y - 1.6).abs() < 1e-6);
    assert_eq!(loc.pose.position.x, 0.0);
    assert_eq!(loc.pose.position.z, 0.0);
}

#[test]
fn identity_helpers_are_const() {
    let q: Quaternionf = identity_quaternion();
    let v: Vector3f = identity_vector3f();
    assert_eq!(q.w, 1.0);
    assert_eq!(v.x + v.y + v.z, 0.0);
}

#[test]
fn duration_helpers_round_trip() {
    let ms = XrDuration::from_millis(50);
    let ns = XrDuration::from_nanos(50_000_000);
    assert_eq!(ms, ns);
    assert_eq!(XrDuration::NONE, XrDuration(0));
    assert_eq!(XrDuration::INFINITE.0, i64::MAX);
}
