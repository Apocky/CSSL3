//! § MotorJoint / MotorJointBlend — PGA-Motor-based joint kinematics.
//!
//! § THESIS
//!   A joint is a constraint on the relative motion of two adjacent bones.
//!   Conventional skeletal-animation runtimes encode joint state as
//!   Euler angles, axis-angle pairs, or quaternions. Each of these has
//!   pathologies :
//!     - **Euler** : gimbal lock at certain angle combinations.
//!     - **Axis-angle** : ill-defined at angle = 0 (axis is arbitrary).
//!     - **Quaternion** : the slerp-near-collinear instability — when
//!       two end-of-blend quaternions are nearly antiparallel, the
//!       interpolation degenerates and small float perturbations flip
//!       the rotation direction.
//!
//!   **PGA Motors** sidestep all three. A motor `M ∈ G(3,0,1)` is the
//!   algebraically-closed representation of a rigid motion. The
//!   composition `M_a ∘ M_b` is well-defined for any two motors. The
//!   blend along the geodesic of SE(3) is given by `exp(t * log(M_a^-1
//!   * M_b)) * M_a` — a single closed-form path that doesn't degenerate
//!   anywhere on SE(3).
//!
//!   This module supplies :
//!     - [`MotorJoint`] : the joint state for one bone-pair, expressed
//!       as a pair `(M_rest, M_current)` of motors plus a kind tag.
//!     - [`MotorJointBlend`] : the canonical N-way blend over a set of
//!       MotorJoint targets ; produces a single output motor on the SE(3)
//!       geodesic.
//!
//! § DETERMINISM
//!   PGA compose / sandwich / exp-log are all closed-form deterministic
//!   functions. Same inputs ⇒ same outputs across runs.

use cssl_pga::{Motor, Rotor, Translator};

/// Joint kind — informs the IK solver and behavior-priors what
/// constraints apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotorJointKind {
    /// Free joint — no constraint. Ball-and-socket equivalent.
    Free,
    /// Hinge joint — single rotational axis. Knee, elbow, fingers.
    Hinge,
    /// Universal joint — two rotational axes (no twist). Wrist.
    Universal,
    /// Twist joint — single rotational axis aligned with bone direction.
    /// Used for spine-twist and forearm-twist.
    Twist,
    /// Prismatic joint — pure translation along one axis. Used for
    /// telescoping limbs and energy-being tendril extensions.
    Prismatic,
    /// Spring-damped joint — passive joint with restoring force toward
    /// rest. Used for tail / antenna / soft-body "drift" joints.
    Spring,
}

/// Joint state for one bone in PGA-Motor representation.
#[derive(Debug, Clone, Copy)]
pub struct MotorJoint {
    /// The bone this joint controls.
    pub bone_idx: usize,
    /// Joint kind.
    pub kind: MotorJointKind,
    /// Rest motor — the joint's neutral / bind-pose configuration.
    pub rest: Motor,
    /// Current motor — the joint's instantaneous state.
    pub current: Motor,
    /// Hinge / universal axis primary direction (in bone-local space).
    /// Ignored for Free / Twist / Prismatic / Spring kinds.
    pub primary_axis: [f32; 3],
    /// Spring stiffness for Spring-kind joints. Ignored otherwise.
    pub spring_stiffness: f32,
}

impl MotorJoint {
    /// Construct a free joint at the rest pose.
    #[must_use]
    pub fn free(bone_idx: usize, rest: Motor) -> Self {
        Self {
            bone_idx,
            kind: MotorJointKind::Free,
            rest,
            current: rest,
            primary_axis: [0.0, 1.0, 0.0],
            spring_stiffness: 0.0,
        }
    }

    /// Construct a hinge joint.
    #[must_use]
    pub fn hinge(bone_idx: usize, rest: Motor, axis: [f32; 3]) -> Self {
        Self {
            bone_idx,
            kind: MotorJointKind::Hinge,
            rest,
            current: rest,
            primary_axis: axis,
            spring_stiffness: 0.0,
        }
    }

    /// Construct a spring-damped joint with a stiffness.
    #[must_use]
    pub fn spring(bone_idx: usize, rest: Motor, stiffness: f32) -> Self {
        Self {
            bone_idx,
            kind: MotorJointKind::Spring,
            rest,
            current: rest,
            primary_axis: [0.0, 1.0, 0.0],
            spring_stiffness: stiffness.max(0.0),
        }
    }

    /// Snap the joint back to its rest pose.
    pub fn reset(&mut self) {
        self.current = self.rest;
    }

    /// Apply a delta motor : `current = current ∘ delta`. Used by the
    /// physics-IK solver to push the joint toward an IK target without
    /// destroying its rest-pose reference.
    pub fn apply_delta(&mut self, delta: Motor) {
        self.current = self.current.compose(delta);
    }

    /// Spring-decay : pull `current` toward `rest` by `factor`. Used by
    /// the physics integrator each tick for Spring-kind joints. `factor`
    /// is typically `1.0 - exp(-stiffness * dt)`.
    pub fn spring_decay(&mut self, factor: f32) {
        let factor = factor.clamp(0.0, 1.0);
        self.current = motor_geodesic_blend(self.current, self.rest, factor);
    }
}

/// One target in an N-way motor blend.
#[derive(Debug, Clone, Copy)]
pub struct MotorBlendTarget {
    /// The motor target.
    pub motor: Motor,
    /// Blend weight in `[0, 1]`. Weights are normalized at evaluation
    /// time so the caller doesn't need to ensure they sum to 1.
    pub weight: f32,
}

impl MotorBlendTarget {
    /// Construct a target with explicit motor + weight.
    #[must_use]
    pub fn new(motor: Motor, weight: f32) -> Self {
        Self {
            motor,
            weight: weight.max(0.0),
        }
    }
}

/// N-way motor blend. Produces a single output motor on the SE(3)
/// geodesic that minimizes weighted-distance to the targets.
#[derive(Debug, Clone, Default)]
pub struct MotorJointBlend {
    targets: Vec<MotorBlendTarget>,
}

impl MotorJointBlend {
    /// New empty blend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a target with explicit weight.
    pub fn add_target(&mut self, motor: Motor, weight: f32) {
        self.targets.push(MotorBlendTarget::new(motor, weight));
    }

    /// Evaluate the blend. Returns `Motor::IDENTITY` if no targets are
    /// registered. Two targets blend along the SE(3) geodesic ; three+
    /// targets fall back to weighted-rotor + weighted-translator
    /// composition (a stage-0 simplification ; full N-way SE(3) Karcher
    /// mean lands when `cssl-kan` graduates).
    #[must_use]
    pub fn evaluate(&self) -> Motor {
        if self.targets.is_empty() {
            return Motor::IDENTITY;
        }
        if self.targets.len() == 1 {
            return self.targets[0].motor;
        }
        if self.targets.len() == 2 {
            let total = self.targets[0].weight + self.targets[1].weight;
            if total < f32::EPSILON {
                return self.targets[0].motor;
            }
            let t = self.targets[1].weight / total;
            return motor_geodesic_blend(self.targets[0].motor, self.targets[1].motor, t);
        }
        // N-way fallback : weighted average of rotor + translator parts.
        let total: f32 = self.targets.iter().map(|t| t.weight).sum();
        if total < f32::EPSILON {
            return Motor::IDENTITY;
        }
        let inv = total.recip();
        let (mut s, mut r1, mut r2, mut r3) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        let (mut t1, mut t2, mut t3, mut m0) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        // Reference rotor : take the first as the canonical-sign anchor.
        let ref_q = (
            self.targets[0].motor.s,
            self.targets[0].motor.r1,
            self.targets[0].motor.r2,
            self.targets[0].motor.r3,
        );
        for tg in &self.targets {
            let w = tg.weight * inv;
            // Sign-disambiguate the rotor part to the reference.
            let dot = tg.motor.s * ref_q.0
                + tg.motor.r1 * ref_q.1
                + tg.motor.r2 * ref_q.2
                + tg.motor.r3 * ref_q.3;
            let sign = if dot < 0.0 { -1.0 } else { 1.0 };
            s += w * sign * tg.motor.s;
            r1 += w * sign * tg.motor.r1;
            r2 += w * sign * tg.motor.r2;
            r3 += w * sign * tg.motor.r3;
            t1 += w * tg.motor.t1;
            t2 += w * tg.motor.t2;
            t3 += w * tg.motor.t3;
            m0 += w * tg.motor.m0;
        }
        // Renormalize the rotor part.
        let r_norm_sq = s * s + r1 * r1 + r2 * r2 + r3 * r3;
        let inv_r = if r_norm_sq > f32::EPSILON {
            r_norm_sq.sqrt().recip()
        } else {
            1.0
        };
        Motor::from_components(
            s * inv_r,
            r1 * inv_r,
            r2 * inv_r,
            r3 * inv_r,
            t1,
            t2,
            t3,
            m0,
        )
    }

    /// Number of targets registered.
    #[must_use]
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }

    /// Read-only access to the targets.
    #[must_use]
    pub fn targets(&self) -> &[MotorBlendTarget] {
        &self.targets
    }

    /// Clear all targets.
    pub fn clear(&mut self) {
        self.targets.clear();
    }
}

/// Geodesic blend between two motors on SE(3). Works by separately
/// blending the rotor part along the spherical geodesic (slerp) and the
/// translator part linearly, then composing the results. For any two
/// motors this is well-defined and continuous ; the slerp-near-collinear
/// case is handled by falling back to nlerp.
#[must_use]
pub fn motor_geodesic_blend(a: Motor, b: Motor, t: f32) -> Motor {
    let t = t.clamp(0.0, 1.0);
    // Rotor part — quaternion-style slerp on (s, r1, r2, r3).
    let mut dot = a.s * b.s + a.r1 * b.r1 + a.r2 * b.r2 + a.r3 * b.r3;
    let sign = if dot < 0.0 {
        dot = -dot;
        -1.0
    } else {
        1.0
    };
    let bs = b.s * sign;
    let br1 = b.r1 * sign;
    let br2 = b.r2 * sign;
    let br3 = b.r3 * sign;
    let (rs, rr1, rr2, rr3);
    if dot > 0.9995 {
        // Near-collinear : fall back to normalized linear interpolation
        // for numerical stability.
        let s = a.s + (bs - a.s) * t;
        let r1 = a.r1 + (br1 - a.r1) * t;
        let r2 = a.r2 + (br2 - a.r2) * t;
        let r3 = a.r3 + (br3 - a.r3) * t;
        let n_sq = s * s + r1 * r1 + r2 * r2 + r3 * r3;
        let inv = if n_sq > f32::EPSILON {
            n_sq.sqrt().recip()
        } else {
            1.0
        };
        rs = s * inv;
        rr1 = r1 * inv;
        rr2 = r2 * inv;
        rr3 = r3 * inv;
    } else {
        let theta = dot.acos();
        let sin_theta = theta.sin();
        if sin_theta.abs() < f32::EPSILON {
            rs = a.s;
            rr1 = a.r1;
            rr2 = a.r2;
            rr3 = a.r3;
        } else {
            let inv_sin = sin_theta.recip();
            let s0 = ((1.0 - t) * theta).sin() * inv_sin;
            let s1 = (t * theta).sin() * inv_sin;
            rs = a.s * s0 + bs * s1;
            rr1 = a.r1 * s0 + br1 * s1;
            rr2 = a.r2 * s0 + br2 * s1;
            rr3 = a.r3 * s0 + br3 * s1;
        }
    }
    // Translator part — linear blend on the bivector components.
    let nt1 = a.t1 + (b.t1 - a.t1) * t;
    let nt2 = a.t2 + (b.t2 - a.t2) * t;
    let nt3 = a.t3 + (b.t3 - a.t3) * t;
    let nm0 = a.m0 + (b.m0 - a.m0) * t;
    Motor::from_components(rs, rr1, rr2, rr3, nt1, nt2, nt3, nm0)
}

/// Convenience : build a Motor from a translation vector.
#[must_use]
pub fn motor_translation(tx: f32, ty: f32, tz: f32) -> Motor {
    Motor::from_translator(Translator::from_translation(tx, ty, tz))
}

/// Convenience : build a Motor from an axis-angle rotation.
#[must_use]
pub fn motor_axis_angle(ax: f32, ay: f32, az: f32, angle_rad: f32) -> Motor {
    Motor::from_rotor(Rotor::from_axis_angle(ax, ay, az, angle_rad))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn free_joint_starts_at_rest() {
        let r = motor_translation(1.0, 0.0, 0.0);
        let j = MotorJoint::free(0, r);
        assert!(approx(j.current.t1, j.rest.t1, 1e-6));
    }

    #[test]
    fn reset_returns_to_rest() {
        let r = motor_translation(1.0, 0.0, 0.0);
        let mut j = MotorJoint::free(0, r);
        j.apply_delta(motor_translation(2.0, 0.0, 0.0));
        j.reset();
        assert!(approx(j.current.t1, j.rest.t1, 1e-6));
    }

    #[test]
    fn apply_delta_changes_current() {
        let r = motor_translation(1.0, 0.0, 0.0);
        let mut j = MotorJoint::free(0, r);
        let before = j.current.t1;
        j.apply_delta(motor_translation(0.5, 0.0, 0.0));
        assert!((j.current.t1 - before).abs() > 1e-6);
    }

    #[test]
    fn empty_blend_returns_identity() {
        let b = MotorJointBlend::new();
        let r = b.evaluate();
        assert!(approx(r.s, Motor::IDENTITY.s, 1e-6));
    }

    #[test]
    fn single_target_returns_self() {
        let mut b = MotorJointBlend::new();
        let m = motor_translation(1.0, 0.0, 0.0);
        b.add_target(m, 1.0);
        let r = b.evaluate();
        assert!(approx(r.t1, m.t1, 1e-6));
    }

    #[test]
    fn two_target_blend_at_half_is_midpoint_translation() {
        let mut b = MotorJointBlend::new();
        let m1 = motor_translation(0.0, 0.0, 0.0);
        let m2 = motor_translation(2.0, 0.0, 0.0);
        b.add_target(m1, 1.0);
        b.add_target(m2, 1.0);
        let r = b.evaluate();
        // Midpoint translation vector should be 1.0.
        let (tx, _, _) = Translator {
            t01: r.t1,
            t02: r.t2,
            t03: r.t3,
        }
        .to_translation();
        assert!((tx - 1.0).abs() < 1e-4, "midpoint tx = {}", tx);
    }

    #[test]
    fn two_target_blend_at_zero_returns_first() {
        let mut b = MotorJointBlend::new();
        let m1 = motor_translation(0.0, 0.0, 0.0);
        let m2 = motor_translation(2.0, 0.0, 0.0);
        b.add_target(m1, 1.0);
        b.add_target(m2, 0.0);
        let r = b.evaluate();
        assert!(approx(r.t1, m1.t1, 1e-6));
    }

    #[test]
    fn three_target_blend_is_weighted_average() {
        let mut b = MotorJointBlend::new();
        b.add_target(motor_translation(0.0, 0.0, 0.0), 1.0);
        b.add_target(motor_translation(2.0, 0.0, 0.0), 1.0);
        b.add_target(motor_translation(4.0, 0.0, 0.0), 1.0);
        let r = b.evaluate();
        let (tx, _, _) = Translator {
            t01: r.t1,
            t02: r.t2,
            t03: r.t3,
        }
        .to_translation();
        // Mean translation should be 2.0.
        assert!((tx - 2.0).abs() < 1e-3, "mean tx = {}", tx);
    }

    #[test]
    fn near_collinear_blend_falls_back_to_nlerp() {
        let m = motor_axis_angle(0.0, 1.0, 0.0, 0.001);
        let r = motor_geodesic_blend(Motor::IDENTITY, m, 0.5);
        // Should not produce NaN.
        assert!(r.s.is_finite());
        assert!(r.r1.is_finite());
        assert!(r.r2.is_finite());
        assert!(r.r3.is_finite());
    }

    #[test]
    fn antiparallel_rotor_blend_is_safe() {
        // Two rotations of 179° about Y. Sign-disambiguation should
        // pick the shorter arc.
        let m1 = motor_axis_angle(0.0, 1.0, 0.0, 0.99 * std::f32::consts::PI);
        let m2 = motor_axis_angle(0.0, 1.0, 0.0, -0.99 * std::f32::consts::PI);
        let r = motor_geodesic_blend(m1, m2, 0.5);
        assert!(r.s.is_finite());
    }

    #[test]
    fn spring_decay_pulls_toward_rest() {
        let r = motor_axis_angle(0.0, 1.0, 0.0, 0.5);
        let mut j = MotorJoint::spring(0, Motor::IDENTITY, 1.0);
        j.current = r;
        j.spring_decay(0.5);
        // After half-decay, current should be between r and rest.
        // Test : norm of "rotated-away-from-identity" component is
        // smaller after decay than before.
        let r1_mag_after = j.current.r2.abs();
        let r1_mag_before = r.r2.abs();
        assert!(
            r1_mag_after < r1_mag_before,
            "spring should decay toward rest"
        );
    }

    #[test]
    fn hinge_constructor_carries_axis() {
        let j = MotorJoint::hinge(0, Motor::IDENTITY, [1.0, 0.0, 0.0]);
        assert_eq!(j.kind, MotorJointKind::Hinge);
        assert_eq!(j.primary_axis, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn target_count_reflects_additions() {
        let mut b = MotorJointBlend::new();
        b.add_target(Motor::IDENTITY, 1.0);
        b.add_target(Motor::IDENTITY, 1.0);
        assert_eq!(b.target_count(), 2);
    }

    #[test]
    fn blend_clear_drops_targets() {
        let mut b = MotorJointBlend::new();
        b.add_target(Motor::IDENTITY, 1.0);
        b.clear();
        assert_eq!(b.target_count(), 0);
    }

    #[test]
    fn motor_translation_helper_applies_correct_offset() {
        let m = motor_translation(3.0, 0.0, 0.0);
        let (tx, _, _) = Translator {
            t01: m.t1,
            t02: m.t2,
            t03: m.t3,
        }
        .to_translation();
        assert!((tx - 3.0).abs() < 1e-5);
    }

    #[test]
    fn motor_axis_angle_helper_produces_unit_rotor() {
        let m = motor_axis_angle(0.0, 1.0, 0.0, std::f32::consts::FRAC_PI_2);
        let n = (m.s * m.s + m.r1 * m.r1 + m.r2 * m.r2 + m.r3 * m.r3).sqrt();
        assert!((n - 1.0).abs() < 1e-4);
    }
}
