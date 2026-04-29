//! Reference-space management.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § XII.B :
//!   "seated / standing / room-scale all-supported via OpenXR
//!    reference-spaces (LOCAL, STAGE, LOCAL_FLOOR)".
//!
//! § DESIGN
//!   `XrReferenceSpaceType` mirrors the canonical OpenXR enum.
//!   `ReferenceSpaceConfig` carries the per-application boundary +
//!   floor-offset.

use crate::error::XRFailure;

/// OpenXR reference-space-type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum XrReferenceSpaceType {
    /// `XR_REFERENCE_SPACE_TYPE_VIEW` — head-locked.
    View,
    /// `XR_REFERENCE_SPACE_TYPE_LOCAL` — yaw-locked seated origin.
    Local,
    /// `XR_REFERENCE_SPACE_TYPE_STAGE` — room-scale boundary.
    Stage,
    /// `XR_EXT_local_floor` — Local + floor-offset = head-height.
    /// § XII.B preferred for standing experiences without explicit
    /// guardian setup.
    LocalFloor,
    /// `XR_REFERENCE_SPACE_TYPE_UNBOUNDED` (XR_MSFT_unbounded_reference_space)
    /// — large-scale unbounded for AR/MR. § IX-related.
    Unbounded,
}

impl XrReferenceSpaceType {
    /// Display-name (canonical short).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Local => "local",
            Self::Stage => "stage",
            Self::LocalFloor => "local-floor",
            Self::Unbounded => "unbounded",
        }
    }

    /// `true` iff this space is "world-locked" (vs. head-locked).
    #[must_use]
    pub const fn is_world_locked(self) -> bool {
        matches!(self, Self::Local | Self::Stage | Self::LocalFloor | Self::Unbounded)
    }

    /// `true` iff this space requires a guardian / boundary setup.
    #[must_use]
    pub const fn requires_boundary(self) -> bool {
        matches!(self, Self::Stage)
    }
}

/// Reference-space config. Per-session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferenceSpaceConfig {
    /// Type.
    pub space_type: XrReferenceSpaceType,
    /// Pose-offset from runtime-supplied space (column-major).
    pub pose_offset: [f32; 16],
    /// Optional guardian-boundary radius (meters). `None` ⇒ runtime default.
    pub boundary_radius_m: Option<f32>,
}

impl ReferenceSpaceConfig {
    /// Default for seated experiences (Local at origin).
    #[must_use]
    pub fn seated() -> Self {
        Self {
            space_type: XrReferenceSpaceType::Local,
            pose_offset: crate::view::identity_mat4(),
            boundary_radius_m: None,
        }
    }

    /// Default for standing experiences without guardian setup
    /// (`XR_EXT_local_floor`).
    #[must_use]
    pub fn standing() -> Self {
        Self {
            space_type: XrReferenceSpaceType::LocalFloor,
            pose_offset: crate::view::identity_mat4(),
            boundary_radius_m: None,
        }
    }

    /// Default for room-scale (Stage with boundary).
    #[must_use]
    pub fn room_scale(boundary_radius_m: f32) -> Self {
        Self {
            space_type: XrReferenceSpaceType::Stage,
            pose_offset: crate::view::identity_mat4(),
            boundary_radius_m: Some(boundary_radius_m),
        }
    }

    /// Default for AR/MR unbounded (large-area).
    #[must_use]
    pub fn unbounded() -> Self {
        Self {
            space_type: XrReferenceSpaceType::Unbounded,
            pose_offset: crate::view::identity_mat4(),
            boundary_radius_m: None,
        }
    }

    /// Validate.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if self.space_type == XrReferenceSpaceType::Stage
            && self.boundary_radius_m.is_none()
        {
            return Err(XRFailure::SessionCreate { code: -60 });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ReferenceSpaceConfig, XrReferenceSpaceType};

    #[test]
    fn space_type_classifications() {
        assert!(!XrReferenceSpaceType::View.is_world_locked());
        assert!(XrReferenceSpaceType::Local.is_world_locked());
        assert!(XrReferenceSpaceType::Stage.is_world_locked());
        assert!(XrReferenceSpaceType::LocalFloor.is_world_locked());
        assert!(XrReferenceSpaceType::Unbounded.is_world_locked());
    }

    #[test]
    fn stage_requires_boundary() {
        assert!(XrReferenceSpaceType::Stage.requires_boundary());
        assert!(!XrReferenceSpaceType::Local.requires_boundary());
        assert!(!XrReferenceSpaceType::LocalFloor.requires_boundary());
    }

    #[test]
    fn seated_default_is_local() {
        let s = ReferenceSpaceConfig::seated();
        assert_eq!(s.space_type, XrReferenceSpaceType::Local);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn standing_default_is_local_floor() {
        let s = ReferenceSpaceConfig::standing();
        assert_eq!(s.space_type, XrReferenceSpaceType::LocalFloor);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn room_scale_demands_boundary() {
        let s = ReferenceSpaceConfig::room_scale(2.5);
        assert_eq!(s.space_type, XrReferenceSpaceType::Stage);
        assert!(s.boundary_radius_m.is_some());
        assert!(s.validate().is_ok());
    }

    #[test]
    fn stage_without_boundary_fails() {
        let mut s = ReferenceSpaceConfig::room_scale(1.0);
        s.boundary_radius_m = None;
        assert!(s.validate().is_err());
    }

    #[test]
    fn unbounded_default() {
        let s = ReferenceSpaceConfig::unbounded();
        assert_eq!(s.space_type, XrReferenceSpaceType::Unbounded);
    }

    #[test]
    fn space_type_as_str_canonical() {
        assert_eq!(XrReferenceSpaceType::View.as_str(), "view");
        assert_eq!(XrReferenceSpaceType::Local.as_str(), "local");
        assert_eq!(XrReferenceSpaceType::Stage.as_str(), "stage");
        assert_eq!(XrReferenceSpaceType::LocalFloor.as_str(), "local-floor");
        assert_eq!(XrReferenceSpaceType::Unbounded.as_str(), "unbounded");
    }
}
