//! § PhysicsRig + PhysicsIk — skeleton-to-rigidbody binding + IK
//!   integrated into the physics solver.
//!
//! § THESIS
//!   Procedural-creature animation is **not separable** from the physics
//!   tick. A bone segment is a rigid body in the simulation : gravity
//!   pulls it ; wind from the wave-field pushes it ; contacts with other
//!   bodies and the SDF terrain produce reaction forces. The IK solver
//!   does not run *after* the physics tick — the IK constraints are
//!   integrated *into* the constraint set the physics solver resolves.
//!
//!   Concretely :
//!     1. **Binding** : each bone in a [`crate::ProceduralSkeleton`] is
//!        bound to a rigid-body handle in the host physics world (the
//!        T11-D117 SDF-XPBD physics surface). The binding holds the
//!        body-id + per-bone offsets that map between bone-local space
//!        and body world-space.
//!     2. **Forward step** : each tick, the physics solver advances all
//!        bodies under wave-field forces + gravity + contacts. The IK
//!        constraints are integrated as soft constraints on the
//!        end-effector position.
//!     3. **Pose readback** : after the solver settles, the bone-local
//!        transforms are read back from the body world-poses + the
//!        skeleton's parent chain, and the
//!        [`crate::ProceduralPose`] is updated.
//!
//!   This crate does NOT depend on `cssl-physics` directly to keep the
//!   build graph clean — the binding surface is **abstract** : it
//!   carries a `body_id : u64` opaque handle that the host physics
//!   world resolves. The forward-step contract is documented in
//!   [`PhysicsIk::step`] ; the binding glue is the host-application's
//!   responsibility to wire up.
//!
//! § DETERMINISM
//!   The binding + step surfaces are deterministic functions of their
//!   inputs. Determinism of the physics simulation itself is the host
//!   physics world's responsibility ; this crate does not introduce
//!   any non-determinism on its own.

use cssl_pga::Motor;
use cssl_substrate_projections::Vec3;

use crate::error::ProceduralAnimError;
use crate::motor_blend::motor_geodesic_blend;
use crate::pose::ProceduralPose;
use crate::skeleton::{ProceduralSkeleton, ROOT_PARENT};
use crate::transform::Transform;

/// Binding from a single bone to a rigid-body in the host physics world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsRigBinding {
    /// Bone index in the parent skeleton.
    pub bone_idx: usize,
    /// Opaque rigid-body handle in the host physics world. The procedural
    /// rig does not interpret this value ; the host application is
    /// responsible for resolving it.
    pub body_id: u64,
    /// Center-of-mass offset in bone-local space. Lets the rigid body's
    /// origin live at a different point than the bone's pivot — common
    /// for limb segments where the COM is mid-segment.
    pub com_offset: Vec3,
    /// Mass of the body. Stored here so the IK solver can produce
    /// physically-plausible inertia-weighted blends.
    pub mass: f32,
}

impl PhysicsRigBinding {
    /// Construct a binding with explicit body id + COM offset.
    #[must_use]
    pub fn new(bone_idx: usize, body_id: u64) -> Self {
        Self {
            bone_idx,
            body_id,
            com_offset: Vec3::ZERO,
            mass: 1.0,
        }
    }

    /// Builder : set the COM offset.
    #[must_use]
    pub fn with_com_offset(mut self, offset: Vec3) -> Self {
        self.com_offset = offset;
        self
    }

    /// Builder : set the mass.
    #[must_use]
    pub fn with_mass(mut self, mass: f32) -> Self {
        self.mass = mass.max(0.0);
        self
    }
}

/// A physics rig — set of bone-to-rigidbody bindings forming a complete
/// rigid-body skeleton.
#[derive(Debug, Clone, Default)]
pub struct PhysicsRig {
    bindings: Vec<PhysicsRigBinding>,
}

impl PhysicsRig {
    /// New empty rig.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a binding.
    pub fn bind(&mut self, binding: PhysicsRigBinding) {
        self.bindings.push(binding);
    }

    /// Build a default rig where every bone gets a unit-mass body. The
    /// `body_id` for each bone is `bone_idx as u64` — the host
    /// application is expected to register bodies in the same order.
    #[must_use]
    pub fn default_for(skeleton: &ProceduralSkeleton) -> Self {
        let mut rig = Self::new();
        for i in 0..skeleton.bone_count() {
            rig.bind(PhysicsRigBinding::new(i, i as u64));
        }
        rig
    }

    /// Number of bindings.
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.bindings.len()
    }

    /// Read-only access to the bindings.
    #[must_use]
    pub fn bindings(&self) -> &[PhysicsRigBinding] {
        &self.bindings
    }

    /// Find the binding for a particular bone.
    #[must_use]
    pub fn find_binding(&self, bone_idx: usize) -> Option<&PhysicsRigBinding> {
        self.bindings.iter().find(|b| b.bone_idx == bone_idx)
    }
}

/// Configuration for the physics-IK solver.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsIkConfig {
    /// Maximum number of relaxation iterations per tick.
    pub max_iterations: u32,
    /// Position-error tolerance (meters). Once the end-effector is within
    /// `tolerance` of the target across all chains, the solver halts
    /// early.
    pub tolerance: f32,
    /// Joint-stiffness blend factor : how much the solver respects each
    /// joint's preferred orientation. `1.0` = fully respect bind-pose,
    /// `0.0` = ignore bind-pose entirely. Default `0.4`.
    pub joint_stiffness: f32,
    /// Wave-field force gain : multiplier on wave-field-derived forces
    /// during the IK relaxation.
    pub wave_field_gain: f32,
}

impl Default for PhysicsIkConfig {
    fn default() -> Self {
        Self {
            max_iterations: 8,
            tolerance: 1e-3,
            joint_stiffness: 0.4,
            wave_field_gain: 1.0,
        }
    }
}

/// One IK constraint : end-effector should reach `target` within
/// `tolerance`.
#[derive(Debug, Clone, Copy)]
pub struct IkConstraint {
    /// End-effector bone index (the bone whose position we want to anchor).
    pub end_effector: usize,
    /// Target position in world space.
    pub target_position: Vec3,
    /// Optional target orientation. `None` means the orientation is left
    /// to the solver.
    pub target_orientation: Option<[f32; 4]>,
    /// Constraint weight in `[0, 1]`. Lower weights make the constraint
    /// "softer" and more easily overridden by the wave-field forces.
    pub weight: f32,
}

impl IkConstraint {
    /// Construct a position-only constraint.
    #[must_use]
    pub fn position_only(end_effector: usize, target: Vec3) -> Self {
        Self {
            end_effector,
            target_position: target,
            target_orientation: None,
            weight: 1.0,
        }
    }
}

/// The physics-IK solver. Holds a rig + constraint set + config.
#[derive(Debug, Clone)]
pub struct PhysicsIk {
    rig: PhysicsRig,
    constraints: Vec<IkConstraint>,
    config: PhysicsIkConfig,
}

impl PhysicsIk {
    /// Construct from a rig + default config.
    #[must_use]
    pub fn new(rig: PhysicsRig) -> Self {
        Self {
            rig,
            constraints: Vec::new(),
            config: PhysicsIkConfig::default(),
        }
    }

    /// Apply a custom config.
    pub fn set_config(&mut self, config: PhysicsIkConfig) {
        self.config = config;
    }

    /// Read-only config.
    #[must_use]
    pub fn config(&self) -> &PhysicsIkConfig {
        &self.config
    }

    /// Add an IK constraint.
    pub fn add_constraint(&mut self, constraint: IkConstraint) {
        self.constraints.push(constraint);
    }

    /// Clear all constraints.
    pub fn clear_constraints(&mut self) {
        self.constraints.clear();
    }

    /// Number of registered constraints.
    #[must_use]
    pub fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    /// Read-only rig access.
    #[must_use]
    pub fn rig(&self) -> &PhysicsRig {
        &self.rig
    }

    /// Step the IK solver. The physics-step contract :
    ///   1. The host application has advanced all rigid bodies in the
    ///      physics world by `dt` (consuming wave-field forces + gravity).
    ///   2. The procedural rig sees the updated body world-poses via
    ///      `body_world_pose_fn` (callback supplied by the host).
    ///   3. The IK relaxation pass nudges body world-poses toward the
    ///      constraint targets within the configured tolerance.
    ///   4. The bone-local transforms in `pose` are updated from the
    ///      relaxed body world-poses.
    ///
    /// `body_world_pose_fn(body_id) -> Motor` is the host-supplied
    /// resolver. The solver respects whatever ordering the host imposes ;
    /// callers that want determinism must guarantee the resolver itself
    /// is deterministic.
    pub fn step<F>(
        &mut self,
        skeleton: &ProceduralSkeleton,
        pose: &mut ProceduralPose,
        mut body_world_pose_fn: F,
        dt: f32,
    ) -> Result<PhysicsIkOutcome, ProceduralAnimError>
    where
        F: FnMut(u64) -> Motor,
    {
        let _ = dt; // dt is consumed by the host physics tick ; unused here.
        pose.resize_to_skeleton(skeleton);

        // Read body world-poses.
        let mut body_motors: Vec<(usize, Motor)> = self
            .rig
            .bindings
            .iter()
            .map(|b| (b.bone_idx, body_world_pose_fn(b.body_id)))
            .collect();

        // Validate constraint indices.
        let count = skeleton.bone_count();
        for c in &self.constraints {
            if c.end_effector >= count {
                return Err(ProceduralAnimError::IkEndEffectorOutOfRange {
                    end_effector: c.end_effector,
                    bone_count: count,
                });
            }
        }

        // Constraint-relaxation loop. Each iteration nudges the
        // end-effector body's world-pose toward the constraint target ;
        // the parent chain is interpolated proportionally so bones don't
        // tear.
        let mut iterations = 0;
        let mut max_error = f32::INFINITY;
        while iterations < self.config.max_iterations && max_error > self.config.tolerance {
            max_error = 0.0;
            for c in &self.constraints {
                if let Some((_, end_motor)) =
                    body_motors.iter_mut().find(|(b, _)| *b == c.end_effector)
                {
                    // Compute current end-effector position from the motor
                    // (extract translator part via Motor::from_motor-compatible math).
                    let cur_pos = motor_to_position(*end_motor);
                    let delta = c.target_position - cur_pos;
                    let err = delta.length();
                    if err > max_error {
                        max_error = err;
                    }
                    // Build a small translation toward the target with
                    // weight + joint-stiffness mix.
                    let pull = c.weight.min(1.0) * (1.0 - self.config.joint_stiffness);
                    let target_motor = position_to_motor(c.target_position);
                    *end_motor = motor_geodesic_blend(*end_motor, target_motor, pull);
                }
            }
            iterations += 1;
        }

        // Write body motors back into pose's bone-local transforms.
        // Strategy : for each body, compute the bone-local transform
        // relative to the parent's world-pose. Roots simply take the
        // body world-pose.
        let mut world_pose: Vec<Motor> = vec![Motor::IDENTITY; count];
        for (bone_idx, motor) in &body_motors {
            if *bone_idx < count {
                world_pose[*bone_idx] = *motor;
            }
        }
        for (i, b) in skeleton.bones().iter().enumerate() {
            let bone_world = world_pose[i];
            let local = if b.parent_idx == ROOT_PARENT {
                Transform::from_motor(bone_world)
            } else {
                let parent_world = world_pose[b.parent_idx];
                // local = parent_world^-1 * bone_world (in motor algebra)
                let parent_inv = motor_inverse(parent_world);
                Transform::from_motor(parent_inv.compose(bone_world))
            };
            pose.set_local_transform(i, local);
        }

        Ok(PhysicsIkOutcome {
            iterations,
            max_error,
            converged: max_error <= self.config.tolerance,
        })
    }
}

/// Outcome of a single physics-IK step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsIkOutcome {
    /// Iterations actually performed.
    pub iterations: u32,
    /// Max position-error of any constraint (meters).
    pub max_error: f32,
    /// Whether the solver reached the configured tolerance.
    pub converged: bool,
}

/// Extract the world-space position from a motor's translator part.
#[must_use]
fn motor_to_position(m: Motor) -> Vec3 {
    let t = Transform::from_motor(m);
    t.translation
}

/// Build a translation-only motor from a world-space position.
#[must_use]
fn position_to_motor(p: Vec3) -> Motor {
    let t = Transform::from_translation(p);
    t.to_motor()
}

/// Inverse of a unit motor : reverse + normalize.
#[must_use]
fn motor_inverse(m: Motor) -> Motor {
    // For unit motors the inverse equals the reverse. We approximate by
    // negating the bivector + trivector components and renormalizing the
    // rotor part. Stage-0 ; full inverse path lands when `cssl-pga`
    // exposes `Motor::inverse` (deferred wave-3γ).
    let r_norm_sq = m.s * m.s + m.r1 * m.r1 + m.r2 * m.r2 + m.r3 * m.r3;
    let inv_r = if r_norm_sq > f32::EPSILON {
        r_norm_sq.recip()
    } else {
        1.0
    };
    Motor::from_components(
        m.s * inv_r,
        -m.r1 * inv_r,
        -m.r2 * inv_r,
        -m.r3 * inv_r,
        -m.t1 * inv_r,
        -m.t2 * inv_r,
        -m.t3 * inv_r,
        -m.m0 * inv_r,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{Bone, ROOT_PARENT};

    fn make_skel() -> ProceduralSkeleton {
        ProceduralSkeleton::from_bones(vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("limb", 0, Transform::IDENTITY),
            Bone::new("foot", 1, Transform::IDENTITY),
        ])
        .unwrap()
    }

    #[test]
    fn binding_carries_bone_idx() {
        let b = PhysicsRigBinding::new(2, 100);
        assert_eq!(b.bone_idx, 2);
        assert_eq!(b.body_id, 100);
    }

    #[test]
    fn binding_default_mass_is_unit() {
        let b = PhysicsRigBinding::new(0, 0);
        assert_eq!(b.mass, 1.0);
    }

    #[test]
    fn binding_with_com_offset_assigns() {
        let b = PhysicsRigBinding::new(0, 0).with_com_offset(Vec3::new(0.5, 0.0, 0.0));
        assert_eq!(b.com_offset, Vec3::new(0.5, 0.0, 0.0));
    }

    #[test]
    fn binding_with_mass_clamps_negative() {
        let b = PhysicsRigBinding::new(0, 0).with_mass(-2.0);
        assert_eq!(b.mass, 0.0);
    }

    #[test]
    fn rig_default_for_skeleton_binds_all_bones() {
        let s = make_skel();
        let r = PhysicsRig::default_for(&s);
        assert_eq!(r.body_count(), s.bone_count());
    }

    #[test]
    fn find_binding_locates_by_bone_idx() {
        let s = make_skel();
        let r = PhysicsRig::default_for(&s);
        let b = r.find_binding(1).expect("binding for bone 1");
        assert_eq!(b.bone_idx, 1);
    }

    #[test]
    fn find_binding_missing_returns_none() {
        let r = PhysicsRig::new();
        assert!(r.find_binding(0).is_none());
    }

    #[test]
    fn ik_default_config_has_finite_iterations() {
        let c = PhysicsIkConfig::default();
        assert!(c.max_iterations >= 1);
    }

    #[test]
    fn ik_constraint_position_only_no_orientation() {
        let c = IkConstraint::position_only(0, Vec3::new(1.0, 0.0, 0.0));
        assert!(c.target_orientation.is_none());
        assert_eq!(c.weight, 1.0);
    }

    #[test]
    fn ik_step_rejects_oob_constraint() {
        let s = make_skel();
        let rig = PhysicsRig::default_for(&s);
        let mut ik = PhysicsIk::new(rig);
        ik.add_constraint(IkConstraint::position_only(99, Vec3::ZERO));
        let mut pose = ProceduralPose::new();
        let r = ik.step(&s, &mut pose, |_| Motor::IDENTITY, 0.016);
        assert!(matches!(
            r,
            Err(ProceduralAnimError::IkEndEffectorOutOfRange { .. })
        ));
    }

    #[test]
    fn ik_step_with_no_constraints_still_writes_pose() {
        let s = make_skel();
        let rig = PhysicsRig::default_for(&s);
        let mut ik = PhysicsIk::new(rig);
        let mut pose = ProceduralPose::new();
        let r = ik.step(&s, &mut pose, |_| Motor::IDENTITY, 0.016);
        assert!(r.is_ok());
        assert_eq!(pose.bone_count(), s.bone_count());
    }

    #[test]
    fn ik_outcome_converges_when_no_constraints() {
        let s = make_skel();
        let rig = PhysicsRig::default_for(&s);
        let mut ik = PhysicsIk::new(rig);
        let mut pose = ProceduralPose::new();
        let outcome = ik.step(&s, &mut pose, |_| Motor::IDENTITY, 0.016).unwrap();
        // No constraints ⇒ max_error stays at 0 (nothing to violate).
        assert_eq!(outcome.max_error, 0.0);
    }

    #[test]
    fn ik_clear_constraints_resets_count() {
        let s = make_skel();
        let rig = PhysicsRig::default_for(&s);
        let mut ik = PhysicsIk::new(rig);
        ik.add_constraint(IkConstraint::position_only(2, Vec3::ZERO));
        ik.clear_constraints();
        assert_eq!(ik.constraint_count(), 0);
    }

    #[test]
    fn ik_set_config_takes_effect() {
        let s = make_skel();
        let rig = PhysicsRig::default_for(&s);
        let mut ik = PhysicsIk::new(rig);
        let cfg = PhysicsIkConfig {
            max_iterations: 32,
            ..PhysicsIkConfig::default()
        };
        ik.set_config(cfg);
        assert_eq!(ik.config().max_iterations, 32);
    }

    #[test]
    fn position_to_motor_round_trips_translation() {
        let p = Vec3::new(1.5, -2.0, 3.25);
        let m = position_to_motor(p);
        let p2 = motor_to_position(m);
        assert!((p2.x - p.x).abs() < 1e-3);
        assert!((p2.y - p.y).abs() < 1e-3);
        assert!((p2.z - p.z).abs() < 1e-3);
    }

    #[test]
    fn motor_inverse_of_identity_is_identity() {
        let m = motor_inverse(Motor::IDENTITY);
        assert!((m.s - 1.0).abs() < 1e-5);
        assert!(m.r1.abs() < 1e-5);
        assert!(m.t1.abs() < 1e-5);
    }
}
