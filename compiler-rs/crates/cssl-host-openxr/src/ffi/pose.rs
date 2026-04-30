//! § ffi::pose : `XrSpace` + `xrLocateSpace` + `XrPosef` decode.
//!
//! § SPEC : OpenXR 1.0 § 6.5 (Spaces). A `XrSpace` is a coordinate-frame
//!          relative to which poses are expressed. The runtime reports
//!          a pose as `XrPosef = (orientation: Quaternion, position: Vector3f)`.

use bitflags::bitflags;

use super::result::XrResult;
use super::types::{StructureType, Time};

/// FFI handle for `XrSpace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct SpaceHandle(pub u64);

impl SpaceHandle {
    pub const NULL: Self = Self(0);

    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// `XrVector3f` ; FFI struct, 3 × f32 row.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Vector3f {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// `XrQuaternionf` ; FFI struct, 4 × f32. (x, y, z, w).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Quaternionf {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quaternionf {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }
    }
}

/// `XrPosef` ; FFI struct.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct XrPosef {
    pub orientation: Quaternionf,
    pub position: Vector3f,
}

bitflags! {
    /// `XrSpaceLocationFlags`. § 6.5.3 spec.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    #[repr(transparent)]
    pub struct SpaceLocationFlags: u64 {
        const ORIENTATION_VALID    = 0x0000_0001;
        const POSITION_VALID       = 0x0000_0002;
        const ORIENTATION_TRACKED  = 0x0000_0004;
        const POSITION_TRACKED     = 0x0000_0008;
    }
}

/// `XrSpaceLocation` returned by `xrLocateSpace`.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SpaceLocation {
    pub ty: StructureType,
    pub next: *mut core::ffi::c_void,
    pub location_flags: SpaceLocationFlags,
    pub pose: XrPosef,
}

impl SpaceLocation {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            ty: StructureType::SpaceLocation,
            next: core::ptr::null_mut(),
            location_flags: SpaceLocationFlags::empty(),
            pose: XrPosef::default(),
        }
    }

    /// `true` iff orientation is fully tracked + valid.
    #[must_use]
    pub fn orientation_ok(&self) -> bool {
        self.location_flags
            .contains(SpaceLocationFlags::ORIENTATION_VALID | SpaceLocationFlags::ORIENTATION_TRACKED)
    }

    /// `true` iff position is fully tracked + valid (6DoF).
    #[must_use]
    pub fn position_ok(&self) -> bool {
        self.location_flags
            .contains(SpaceLocationFlags::POSITION_VALID | SpaceLocationFlags::POSITION_TRACKED)
    }
}

/// `XrSpaceVelocity` returned by `xrLocateSpace` when chained.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SpaceVelocity {
    pub linear: Vector3f,
    pub angular: Vector3f,
}

/// Identity `XrPosef`. Used by the STUB swap-in.
#[must_use]
pub const fn identity_quaternion() -> Quaternionf {
    Quaternionf {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    }
}

#[must_use]
pub const fn identity_vector3f() -> Vector3f {
    Vector3f {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    }
}

/// Decode a raw FFI `XrPosef` into normalized form. The runtime guarantees
/// the quaternion is unit-length, but we re-normalize defensively (matches
/// the STUB-impl in `cssl-rt::host_xr`).
#[must_use]
pub fn decode_posef(raw: XrPosef) -> XrPosef {
    let q = raw.orientation;
    let mag2 = q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
    if (mag2 - 1.0).abs() < 1e-6 {
        return raw;
    }
    let mag = mag2.sqrt();
    if mag < 1e-9 {
        return XrPosef {
            orientation: identity_quaternion(),
            position: raw.position,
        };
    }
    let inv = 1.0 / mag;
    XrPosef {
        orientation: Quaternionf {
            x: q.x * inv,
            y: q.y * inv,
            z: q.z * inv,
            w: q.w * inv,
        },
        position: raw.position,
    }
}

/// In-memory mock for `XrSpace`.
#[derive(Debug, Clone)]
pub struct MockSpace {
    pub handle: SpaceHandle,
    /// Synthetic pose returned by `mock_locate_space`.
    pub pose: XrPosef,
    pub flags: SpaceLocationFlags,
}

impl MockSpace {
    /// Quest-3s view-space at neutral standing pose : 0.0m forward,
    /// 1.6m up (canonical adult standing eye-height).
    #[must_use]
    pub fn quest_3s_view_at_neutral() -> Self {
        Self {
            handle: SpaceHandle(0xC551_5944),
            pose: XrPosef {
                orientation: identity_quaternion(),
                position: Vector3f {
                    x: 0.0,
                    y: 1.6,
                    z: 0.0,
                },
            },
            flags: SpaceLocationFlags::ORIENTATION_VALID
                | SpaceLocationFlags::ORIENTATION_TRACKED
                | SpaceLocationFlags::POSITION_VALID
                | SpaceLocationFlags::POSITION_TRACKED,
        }
    }

    /// Stage-space referenced from a corner of the play-area.
    #[must_use]
    pub fn stage_origin() -> Self {
        Self {
            handle: SpaceHandle(0xC551_5746),
            pose: XrPosef::default(),
            flags: SpaceLocationFlags::ORIENTATION_VALID
                | SpaceLocationFlags::ORIENTATION_TRACKED
                | SpaceLocationFlags::POSITION_VALID
                | SpaceLocationFlags::POSITION_TRACKED,
        }
    }
}

/// Mock `xrLocateSpace` ; copies the precomputed pose into a fresh
/// `SpaceLocation`. Matches the canonical FFI signature minus the raw-
/// pointer plumbing.
#[must_use]
pub fn mock_locate_space(
    space: &MockSpace,
    _base_space: &MockSpace,
    _time: Time,
) -> SpaceLocation {
    let mut loc = SpaceLocation::empty();
    loc.location_flags = space.flags;
    loc.pose = space.pose;
    loc
}

/// Validate that an `XrPosef` is invalid by spec-rules : NaN / non-finite
/// component or zero-magnitude quaternion.
#[must_use]
pub fn is_pose_valid(p: &XrPosef) -> bool {
    let q = p.orientation;
    let v = p.position;
    let all_finite = q.x.is_finite()
        && q.y.is_finite()
        && q.z.is_finite()
        && q.w.is_finite()
        && v.x.is_finite()
        && v.y.is_finite()
        && v.z.is_finite();
    if !all_finite {
        return false;
    }
    let m2 = q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
    m2 > 1e-12
}

/// Apply `xrLocateSpace` and return `Err(POSE_INVALID)` if the result
/// fails validity checks. Matches the runtime's contract.
pub fn locate_space_validated(
    space: &MockSpace,
    base: &MockSpace,
    time: Time,
) -> Result<SpaceLocation, XrResult> {
    let loc = mock_locate_space(space, base, time);
    if !is_pose_valid(&loc.pose) {
        return Err(XrResult::ERROR_POSE_INVALID);
    }
    Ok(loc)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_posef, identity_quaternion, identity_vector3f, is_pose_valid, locate_space_validated,
        mock_locate_space, MockSpace, Quaternionf, Vector3f, XrPosef, XrResult,
    };
    use super::Time;

    #[test]
    fn decode_posef_renormalizes_quaternion() {
        let raw = XrPosef {
            orientation: Quaternionf {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 2.0,
            },
            position: Vector3f::default(),
        };
        let out = decode_posef(raw);
        let m = out.orientation.x.powi(2)
            + out.orientation.y.powi(2)
            + out.orientation.z.powi(2)
            + out.orientation.w.powi(2);
        assert!((m - 1.0).abs() < 1e-6, "magnitude after decode = {m}");
    }

    #[test]
    fn quest_3s_view_pose_decodes_at_eye_height() {
        let view = MockSpace::quest_3s_view_at_neutral();
        let stage = MockSpace::stage_origin();
        let loc = mock_locate_space(&view, &stage, Time(0));
        assert!(loc.orientation_ok());
        assert!(loc.position_ok());
        assert!((loc.pose.position.y - 1.6).abs() < 1e-6);
    }

    #[test]
    fn nan_quaternion_is_invalid() {
        let p = XrPosef {
            orientation: Quaternionf {
                x: f32::NAN,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            position: identity_vector3f(),
        };
        assert!(!is_pose_valid(&p));
    }

    #[test]
    fn locate_validated_rejects_zero_magnitude() {
        let mut bad = MockSpace::stage_origin();
        bad.pose.orientation = Quaternionf {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        };
        let r = locate_space_validated(&bad, &MockSpace::stage_origin(), Time(0));
        assert_eq!(r.unwrap_err(), XrResult::ERROR_POSE_INVALID);
    }

    #[test]
    fn identity_helpers_are_canonical() {
        let q = identity_quaternion();
        assert_eq!(q.w, 1.0);
        assert_eq!(q.x, 0.0);
        assert_eq!(q.y, 0.0);
        assert_eq!(q.z, 0.0);
        let v = identity_vector3f();
        assert_eq!(v.x, 0.0);
        assert_eq!(v.y, 0.0);
        assert_eq!(v.z, 0.0);
    }
}
