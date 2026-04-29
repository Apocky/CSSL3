//! Joints — rigid constraints between two bodies.
//!
//! § STAGE-0 COVERAGE
//!   - **HingeJoint** : 1-DOF rotational ; bodies share a hinge-axis,
//!     rotate about it freely, but their relative position is locked.
//!     Common for doors, swinging arms.
//!   - **BallSocketJoint** : 3-DOF rotational ; bodies share a pivot point,
//!     rotate freely about it, but their relative position at the pivot
//!     is locked. Common for shoulders, hips.
//!   - **DistanceJoint** : rigid stick ; bodies maintain a fixed distance
//!     between two anchor points. Common for ropes (with `min_distance`
//!     vs `max_distance` separation).
//!
//! § DEFERRED
//!   - Slider (1-DOF translation along an axis)
//!   - 6-DOF (per-axis configurable rotation + translation limits)
//!   - Motor-driven (joint-with-target-angular-velocity)
//!   These come post-stage-0 once the basic 3 are exercised in scenes.
//!
//! § COORDINATE CONVENTIONS
//!   - Anchor points are stored in body-local space.
//!   - Hinge axes are stored in body-local space ; world-space direction
//!     is derived per-step from each body's orientation.

use crate::body::BodyId;
use crate::math::Vec3;

// ────────────────────────────────────────────────────────────────────────
// § JointId
// ────────────────────────────────────────────────────────────────────────

/// Stable identifier for a joint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JointId(pub u64);

// ────────────────────────────────────────────────────────────────────────
// § HingeJoint
// ────────────────────────────────────────────────────────────────────────

/// 1-DOF rotational joint. Bodies pivot about a shared hinge-axis ; their
/// relative position at the pivot is locked, and rotation is constrained
/// to the hinge-axis direction only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingeJoint {
    /// Anchor point in body-A-local space.
    pub anchor_a: Vec3,
    /// Anchor point in body-B-local space.
    pub anchor_b: Vec3,
    /// Hinge axis in body-A-local space.
    pub axis_a: Vec3,
    /// Hinge axis in body-B-local space.
    pub axis_b: Vec3,
}

// ────────────────────────────────────────────────────────────────────────
// § BallSocketJoint
// ────────────────────────────────────────────────────────────────────────

/// 3-DOF rotational joint. Bodies share a pivot ; their pivots' world-space
/// positions are constrained equal. No rotation constraint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallSocketJoint {
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
}

// ────────────────────────────────────────────────────────────────────────
// § DistanceJoint
// ────────────────────────────────────────────────────────────────────────

/// Rigid distance-stick. Anchor on each body, rigidly held at `target_distance`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistanceJoint {
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
    pub target_distance: f64,
}

// ────────────────────────────────────────────────────────────────────────
// § JointKind enum
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JointKind {
    Hinge(HingeJoint),
    BallSocket(BallSocketJoint),
    Distance(DistanceJoint),
}

// ────────────────────────────────────────────────────────────────────────
// § Joint container
// ────────────────────────────────────────────────────────────────────────

/// A joint binds two bodies via a `JointKind` and accumulates a warm-start
/// impulse for the next solver pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Joint {
    pub body_a: BodyId,
    pub body_b: BodyId,
    pub kind: JointKind,
    /// Accumulated linear impulse from prior frame (warm-start).
    pub accumulated_impulse: Vec3,
}

impl Joint {
    #[must_use]
    pub fn new(body_a: BodyId, body_b: BodyId, kind: JointKind) -> Self {
        Self {
            body_a,
            body_b,
            kind,
            accumulated_impulse: Vec3::ZERO,
        }
    }

    #[must_use]
    pub fn hinge(
        body_a: BodyId,
        body_b: BodyId,
        anchor_a: Vec3,
        anchor_b: Vec3,
        axis_a: Vec3,
        axis_b: Vec3,
    ) -> Self {
        Self::new(
            body_a,
            body_b,
            JointKind::Hinge(HingeJoint {
                anchor_a,
                anchor_b,
                axis_a: axis_a.normalize_or_zero(),
                axis_b: axis_b.normalize_or_zero(),
            }),
        )
    }

    #[must_use]
    pub fn ball_socket(body_a: BodyId, body_b: BodyId, anchor_a: Vec3, anchor_b: Vec3) -> Self {
        Self::new(
            body_a,
            body_b,
            JointKind::BallSocket(BallSocketJoint { anchor_a, anchor_b }),
        )
    }

    #[must_use]
    pub fn distance(
        body_a: BodyId,
        body_b: BodyId,
        anchor_a: Vec3,
        anchor_b: Vec3,
        target: f64,
    ) -> Self {
        Self::new(
            body_a,
            body_b,
            JointKind::Distance(DistanceJoint {
                anchor_a,
                anchor_b,
                target_distance: target,
            }),
        )
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn hinge_joint_axis_normalized() {
        let j = Joint::hinge(
            BodyId(0),
            BodyId(1),
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        if let JointKind::Hinge(h) = j.kind {
            assert!(approx_eq(h.axis_a.length(), 1.0));
            assert!(approx_eq(h.axis_b.length(), 1.0));
        } else {
            panic!("expected hinge");
        }
    }

    #[test]
    fn ball_socket_constructed() {
        let j = Joint::ball_socket(
            BodyId(0),
            BodyId(1),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        if let JointKind::BallSocket(b) = j.kind {
            assert_eq!(b.anchor_a, Vec3::new(1.0, 0.0, 0.0));
            assert_eq!(b.anchor_b, Vec3::new(0.0, 1.0, 0.0));
        } else {
            panic!("expected ball-socket");
        }
    }

    #[test]
    fn distance_joint_target_stored() {
        let j = Joint::distance(BodyId(0), BodyId(1), Vec3::ZERO, Vec3::ZERO, 2.5);
        if let JointKind::Distance(d) = j.kind {
            assert_eq!(d.target_distance, 2.5);
        } else {
            panic!("expected distance");
        }
    }

    #[test]
    fn joint_warm_start_impulse_zero_initially() {
        let j = Joint::ball_socket(BodyId(0), BodyId(1), Vec3::ZERO, Vec3::ZERO);
        assert_eq!(j.accumulated_impulse, Vec3::ZERO);
    }

    #[test]
    fn joint_id_ord() {
        assert!(JointId(0) < JointId(1));
    }
}
