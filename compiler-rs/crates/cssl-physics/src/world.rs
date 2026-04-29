//! `PhysicsWorld` — the top-level physics simulation container.
//!
//! § THESIS
//!   Aggregates bodies + broadphase + narrowphase + solver + integrator into
//!   a single `step(dt)` driver. Implements `OmegaSystem` so it slots into
//!   the H2 scheduler ; the `Substrate.physics` snapshot key holds reference
//!   counts but the actual simulation state lives in this struct.
//!
//! § STEP ORDER (per Catto-Bender consensus + § specs/30 § PHASES sim-substep)
//!   1. integrate_velocities (gravity, applied forces, damping)
//!   2. update AABBs from current positions
//!   3. broadphase rebuild + query candidate-pairs
//!   4. narrowphase generate contacts from candidate-pairs
//!   5. solver runs : velocity-iterations + position-iterations
//!   6. integrate_positions using post-solver velocities
//!   7. clear force/torque accumulators
//!   8. update sleeping
//!   9. preserve contact-warm-start by reusing previous contacts where pairs match
//!
//! § DETERMINISM
//!   - Body iteration in BodyId-sorted order.
//!   - Contact list canonicalized (body_a < body_b ; sorted).
//!   - Solver iterates fixed N times ; no residual-tolerance loop.
//!   - No clock reads ; dt is caller-supplied.

use std::collections::BTreeMap;

use crate::body::{BodyId, BodyKind, RigidBody};
use crate::broadphase::{BroadPhase, BvhBroadPhase};
use crate::contact::Contact;
use crate::integrator::{
    clear_force_accumulators, integrate_positions, integrate_velocities, update_sleeping,
    IntegratorConfig,
};
use crate::joint::Joint;
use crate::math::Vec3;
use crate::narrowphase::shape_pair_contact;
use crate::shape::Aabb;
use crate::solver::{ConstraintSolver, SolverConfig};

use cssl_substrate_omega_step::{EffectRow, OmegaError, OmegaStepCtx, OmegaSystem, SystemId};

// ────────────────────────────────────────────────────────────────────────
// § WorldConfig
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorldConfig {
    pub integrator: IntegratorConfig,
    pub solver: SolverConfig,
    /// Optional contact margin : extends each AABB by this amount in the
    /// broadphase, to catch grazing contacts before they fully overlap.
    /// Default 0.01 m.
    pub broadphase_margin: f64,
    /// Dependency declarations for the OmegaSystem. Populated by `with_dependencies`.
    pub dependencies: Vec<SystemId>,
    /// Identifier name for the OmegaSystem. Default "physics".
    pub system_name: String,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            integrator: IntegratorConfig::default(),
            solver: SolverConfig::default(),
            broadphase_margin: 0.01,
            dependencies: Vec::new(),
            system_name: "physics".to_string(),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § PhysicsWorld
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PhysicsWorld {
    pub config: WorldConfig,
    /// All bodies, keyed by `BodyId`. Ordered iteration yields BodyId-sorted.
    pub bodies: BTreeMap<BodyId, RigidBody>,
    /// All joints, keyed by id.
    pub joints: BTreeMap<crate::joint::JointId, Joint>,
    /// Broadphase implementation. Stage-0 is BVH-only ; future versions may
    /// boxed-trait this.
    broadphase: BvhBroadPhase,
    /// Cached contacts from the previous frame, keyed by body-pair. Used
    /// to preserve `accumulated_*_impulse` across frames (warm-start).
    previous_contacts: BTreeMap<(BodyId, BodyId), Contact>,
    /// The constraint solver.
    solver: ConstraintSolver,
    /// Counter for next BodyId.
    next_body_id: u64,
    /// Counter for next JointId.
    next_joint_id: u64,
}

impl PhysicsWorld {
    #[must_use]
    pub fn new(config: WorldConfig) -> Self {
        let solver = ConstraintSolver::new(config.solver);
        Self {
            config,
            bodies: BTreeMap::new(),
            joints: BTreeMap::new(),
            broadphase: BvhBroadPhase::new(),
            previous_contacts: BTreeMap::new(),
            solver,
            next_body_id: 0,
            next_joint_id: 0,
        }
    }

    /// Insert a body into the world. Returns its `BodyId`.
    pub fn insert(&mut self, body: RigidBody) -> BodyId {
        let id = BodyId(self.next_body_id);
        self.next_body_id += 1;
        self.bodies.insert(id, body);
        id
    }

    /// Insert a joint between two bodies. Returns its `JointId`.
    pub fn insert_joint(&mut self, joint: Joint) -> crate::joint::JointId {
        let id = crate::joint::JointId(self.next_joint_id);
        self.next_joint_id += 1;
        self.joints.insert(id, joint);
        id
    }

    /// Remove a body from the world.
    pub fn remove(&mut self, id: BodyId) -> Option<RigidBody> {
        self.bodies.remove(&id)
    }

    /// Remove a joint.
    pub fn remove_joint(&mut self, id: crate::joint::JointId) -> Option<Joint> {
        self.joints.remove(&id)
    }

    /// Get a body by id.
    #[must_use]
    pub fn body(&self, id: BodyId) -> Option<&RigidBody> {
        self.bodies.get(&id)
    }

    /// Get a body by id (mutable).
    pub fn body_mut(&mut self, id: BodyId) -> Option<&mut RigidBody> {
        self.bodies.get_mut(&id)
    }

    /// Number of bodies.
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Number of joints.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    /// Advance the simulation by `dt` seconds.
    ///
    /// § STEP ORDER (see module docs).
    pub fn step(&mut self, dt: f64) {
        // 1. Integrate velocities (gravity, forces, damping).
        let mut bodies_vec: Vec<(BodyId, RigidBody)> =
            self.bodies.iter().map(|(id, b)| (*id, b.clone())).collect();
        // Vec is body-id-sorted by virtue of BTreeMap iteration order.

        let mut bodies_only: Vec<RigidBody> = bodies_vec.iter().map(|(_, b)| b.clone()).collect();
        let body_id_index: Vec<(BodyId, usize)> = bodies_vec
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i))
            .collect();

        integrate_velocities(&mut bodies_only, &self.config.integrator, dt);

        // 2. Compute AABBs.
        let mut aabbs: Vec<(BodyId, Aabb)> = Vec::with_capacity(bodies_only.len());
        for (i, body) in bodies_only.iter().enumerate() {
            let id = bodies_vec[i].0;
            let aabb = body
                .shape
                .world_aabb(body.position, body.orientation)
                .expand(self.config.broadphase_margin);
            aabbs.push((id, aabb));
        }

        // 3. Broadphase rebuild + query.
        self.broadphase.build(&aabbs);
        let candidate_pairs = self.broadphase.query_pairs();

        // 4. Narrowphase : generate contacts.
        let mut contacts: Vec<Contact> = Vec::new();
        for (id_a, id_b) in candidate_pairs.iter() {
            let idx_a = match body_id_index.binary_search_by_key(id_a, |(i, _)| *i) {
                Ok(p) => body_id_index[p].1,
                Err(_) => continue,
            };
            let idx_b = match body_id_index.binary_search_by_key(id_b, |(i, _)| *i) {
                Ok(p) => body_id_index[p].1,
                Err(_) => continue,
            };
            // Skip pairs of two static bodies.
            let body_a = &bodies_only[idx_a];
            let body_b = &bodies_only[idx_b];
            if body_a.is_static() && body_b.is_static() {
                continue;
            }
            // Generate contact-points.
            if let Some(points) = shape_pair_contact(
                &body_a.shape,
                body_a.position,
                body_a.orientation,
                &body_b.shape,
                body_b.position,
                body_b.orientation,
            ) {
                if points.is_empty() {
                    continue;
                }
                let friction = (body_a.friction * body_b.friction).sqrt();
                let restitution = body_a.restitution.max(body_b.restitution);
                let mut contact = Contact::new(*id_a, *id_b, points, friction, restitution);
                // Restore warm-start impulses from previous frame.
                if let Some(prev) = self.previous_contacts.get(&contact.pair_key()) {
                    let prev_pts = &prev.points;
                    for (i, pt) in contact.points.iter_mut().enumerate() {
                        if let Some(prev_pt) = prev_pts.get(i) {
                            pt.accumulated_normal_impulse = prev_pt.accumulated_normal_impulse;
                            pt.accumulated_tangent_impulse_1 =
                                prev_pt.accumulated_tangent_impulse_1;
                            pt.accumulated_tangent_impulse_2 =
                                prev_pt.accumulated_tangent_impulse_2;
                        }
                    }
                }
                contacts.push(contact);
            }
        }

        // Wake any sleeping bodies that have new contacts.
        for contact in &contacts {
            if let Ok(p) = body_id_index.binary_search_by_key(&contact.body_a, |(i, _)| *i) {
                let idx = body_id_index[p].1;
                bodies_only[idx].wake();
            }
            if let Ok(p) = body_id_index.binary_search_by_key(&contact.body_b, |(i, _)| *i) {
                let idx = body_id_index[p].1;
                bodies_only[idx].wake();
            }
        }

        // 5. Solver.
        let mut joints_vec: Vec<Joint> = self.joints.values().copied().collect();
        self.solver.solve(
            &mut bodies_only,
            &body_id_index,
            &mut contacts,
            &mut joints_vec,
            dt,
        );

        // 6. Integrate positions.
        integrate_positions(&mut bodies_only, dt);

        // 7. Clear force accumulators.
        clear_force_accumulators(&mut bodies_only);

        // 8. Update sleeping.
        update_sleeping(&mut bodies_only, &self.config.integrator);

        // 9. Save contacts for warm-start next frame.
        self.previous_contacts.clear();
        for contact in &contacts {
            self.previous_contacts
                .insert(contact.pair_key(), contact.clone());
        }

        // Write back bodies to BTreeMap.
        for (i, body) in bodies_only.into_iter().enumerate() {
            bodies_vec[i].1 = body;
        }
        for (id, body) in bodies_vec {
            self.bodies.insert(id, body);
        }

        // Write back joints (warm-start impulse may have been updated).
        for (i, (_, joint)) in self.joints.iter_mut().enumerate() {
            if i < joints_vec.len() {
                *joint = joints_vec[i];
            }
        }
    }

    /// Number of contacts in the previous frame (for telemetry / debugging).
    #[must_use]
    pub fn previous_contact_count(&self) -> usize {
        self.previous_contacts.len()
    }

    /// Builder : set dependencies for OmegaSystem.
    #[must_use]
    pub fn with_dependencies(mut self, deps: Vec<SystemId>) -> Self {
        self.config.dependencies = deps;
        self
    }

    /// Builder : set system name for OmegaSystem.
    #[must_use]
    pub fn with_system_name(mut self, name: impl Into<String>) -> Self {
        self.config.system_name = name.into();
        self
    }

    /// Apply a force to the body — convenience helper. Wakes the body if asleep.
    pub fn apply_force(&mut self, id: BodyId, force: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.apply_force(force);
        }
    }

    /// Apply a torque to the body. Wakes if asleep.
    pub fn apply_torque(&mut self, id: BodyId, torque: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.apply_torque(torque);
        }
    }

    /// Apply an impulse to the body's center of mass. Wakes if asleep.
    pub fn apply_impulse(&mut self, id: BodyId, impulse: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.apply_linear_impulse(impulse);
        }
    }

    /// Total kinetic energy of the world. Used for energy-conservation tests.
    #[must_use]
    pub fn total_kinetic_energy(&self) -> f64 {
        let mut e = 0.0;
        for body in self.bodies.values() {
            if body.kind != BodyKind::Dynamic {
                continue;
            }
            // KE_linear = 0.5 m v²
            e += 0.5 * body.mass * body.linear_velocity.length_sq();
            // KE_angular = 0.5 ω · I ω
            // Inertia in world-space = R I_local R^T.
            let r = body.orientation.to_mat3();
            let i_world = r.mul_mat3(body.inertia_local).mul_mat3(r.transpose());
            e += 0.5
                * body
                    .angular_velocity
                    .dot(i_world.mul_vec3(body.angular_velocity));
        }
        e
    }

    /// Capture a serialized snapshot of all body state, suitable for
    /// determinism testing (compare `snapshot()` outputs across runs).
    #[must_use]
    pub fn snapshot(&self) -> Vec<(BodyId, Vec3, crate::math::Quat, Vec3, Vec3)> {
        self.bodies
            .iter()
            .map(|(id, b)| {
                (
                    *id,
                    b.position,
                    b.orientation,
                    b.linear_velocity,
                    b.angular_velocity,
                )
            })
            .collect()
    }
}

// ────────────────────────────────────────────────────────────────────────
// § OmegaSystem impl
// ────────────────────────────────────────────────────────────────────────

impl OmegaSystem for PhysicsWorld {
    fn step(&mut self, _ctx: &mut OmegaStepCtx<'_>, dt: f64) -> Result<(), OmegaError> {
        self.step(dt);
        Ok(())
    }

    fn dependencies(&self) -> &[SystemId] {
        &self.config.dependencies
    }

    fn name(&self) -> &str {
        &self.config.system_name
    }

    fn effect_row(&self) -> EffectRow {
        EffectRow::sim()
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Shape;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    fn vec3_approx(a: Vec3, b: Vec3) -> bool {
        approx_eq(a.x, b.x) && approx_eq(a.y, b.y) && approx_eq(a.z, b.z)
    }

    // ─── PhysicsWorld basic API ───

    #[test]
    fn empty_world_zero_bodies() {
        let w = PhysicsWorld::new(WorldConfig::default());
        assert_eq!(w.body_count(), 0);
        assert_eq!(w.joint_count(), 0);
    }

    #[test]
    fn insert_body_returns_id() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        assert_eq!(id, BodyId(0));
        assert_eq!(w.body_count(), 1);
    }

    #[test]
    fn insert_two_bodies_distinct_ids() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let a = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        let b = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        assert_ne!(a, b);
    }

    #[test]
    fn remove_body_decreases_count() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        assert!(w.remove(id).is_some());
        assert_eq!(w.body_count(), 0);
        assert!(w.remove(id).is_none());
    }

    #[test]
    fn body_lookup_by_id() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(1.0, 2.0, 3.0)),
        );
        assert!(w.body(id).is_some());
        assert!(vec3_approx(
            w.body(id).unwrap().position,
            Vec3::new(1.0, 2.0, 3.0)
        ));
    }

    #[test]
    fn insert_joint_returns_id() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let a = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        let b = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        let j = w.insert_joint(Joint::ball_socket(a, b, Vec3::ZERO, Vec3::ZERO));
        assert_eq!(j, crate::joint::JointId(0));
        assert_eq!(w.joint_count(), 1);
    }

    // ─── Step tests ───

    #[test]
    fn step_with_no_bodies_does_not_panic() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        w.step(1.0 / 60.0);
    }

    #[test]
    fn step_falling_sphere_under_gravity() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(0.0, 10.0, 0.0)),
        );
        // Step 60 times at 1/60 sec = 1 second of fall.
        for _ in 0..60 {
            w.step(1.0 / 60.0);
        }
        // Free-fall for 1s at 9.81 → distance ≈ 4.9 m. Position should be around 10 - 4.9 = 5.1.
        let body = w.body(id).expect("body present");
        // Allow integration error.
        assert!(body.position.y < 6.0 && body.position.y > 4.0);
    }

    #[test]
    fn step_sphere_on_plane_settles() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let sphere_id = w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(0.0, 5.0, 0.0))
                .with_restitution(0.0),
        );
        let _plane_id = w.insert(RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        }));
        // Step long enough for the sphere to fall + settle.
        for _ in 0..300 {
            w.step(1.0 / 60.0);
        }
        let body = w.body(sphere_id).expect("body present");
        // Y position should be near sphere radius (0.5).
        assert!(
            body.position.y > 0.3 && body.position.y < 0.8,
            "expected y near 0.5, got {}",
            body.position.y
        );
        // Velocity should be near zero (settled).
        assert!(body.linear_velocity.length() < 1.0);
    }

    #[test]
    fn step_bouncing_ball_loses_energy_with_low_restitution() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(0.0, 5.0, 0.0))
                .with_restitution(0.5),
        );
        w.insert(RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        }));
        let initial_pe = 1.0 * 9.81 * 5.0; // m·g·h
        for _ in 0..300 {
            w.step(1.0 / 60.0);
        }
        let final_ke = w.total_kinetic_energy();
        // After many bounces, energy should be much less than initial PE.
        assert!(final_ke < initial_pe * 0.5);
    }

    // ─── Determinism tests ───

    #[test]
    fn determinism_two_worlds_same_init_bit_equal_after_n_steps() {
        let setup = |w: &mut PhysicsWorld| {
            w.insert(
                RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                    .with_position(Vec3::new(0.0, 10.0, 0.0)),
            );
            w.insert(
                RigidBody::new_dynamic(
                    2.0,
                    Shape::Box {
                        half_extents: Vec3::new(0.5, 0.5, 0.5),
                    },
                )
                .with_position(Vec3::new(1.0, 8.0, 0.0))
                .with_linear_velocity(Vec3::new(-0.1, 0.0, 0.0)),
            );
            w.insert(RigidBody::new_static(Shape::Plane {
                normal: Vec3::Y,
                d: 0.0,
            }));
        };
        let mut w1 = PhysicsWorld::new(WorldConfig::default());
        let mut w2 = PhysicsWorld::new(WorldConfig::default());
        setup(&mut w1);
        setup(&mut w2);
        for _ in 0..1000 {
            w1.step(1.0 / 60.0);
            w2.step(1.0 / 60.0);
        }
        assert_eq!(w1.snapshot(), w2.snapshot());
    }

    #[test]
    fn determinism_solver_warm_start_consistent() {
        let setup = |w: &mut PhysicsWorld| {
            // Stack of 3 spheres on a plane.
            for i in 0..3 {
                w.insert(
                    RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                        .with_position(Vec3::new(0.0, 1.0 + i as f64 * 1.1, 0.0)),
                );
            }
            w.insert(RigidBody::new_static(Shape::Plane {
                normal: Vec3::Y,
                d: 0.0,
            }));
        };
        let mut w1 = PhysicsWorld::new(WorldConfig::default());
        let mut w2 = PhysicsWorld::new(WorldConfig::default());
        setup(&mut w1);
        setup(&mut w2);
        for _ in 0..500 {
            w1.step(1.0 / 60.0);
            w2.step(1.0 / 60.0);
        }
        assert_eq!(w1.snapshot(), w2.snapshot());
    }

    // ─── OmegaSystem impl ───

    #[test]
    fn omega_system_name_default_is_physics() {
        let w = PhysicsWorld::new(WorldConfig::default());
        assert_eq!(<PhysicsWorld as OmegaSystem>::name(&w), "physics");
    }

    #[test]
    fn omega_system_name_custom() {
        let w = PhysicsWorld::new(WorldConfig::default()).with_system_name("custom-physics");
        assert_eq!(<PhysicsWorld as OmegaSystem>::name(&w), "custom-physics");
    }

    #[test]
    fn omega_system_dependencies_empty_default() {
        let w = PhysicsWorld::new(WorldConfig::default());
        assert!(<PhysicsWorld as OmegaSystem>::dependencies(&w).is_empty());
    }

    #[test]
    fn omega_system_dependencies_custom() {
        let deps = vec![SystemId(7), SystemId(42)];
        let w = PhysicsWorld::new(WorldConfig::default()).with_dependencies(deps.clone());
        assert_eq!(
            <PhysicsWorld as OmegaSystem>::dependencies(&w),
            deps.as_slice()
        );
    }

    #[test]
    fn omega_system_effect_row_includes_sim() {
        let w = PhysicsWorld::new(WorldConfig::default());
        let row = <PhysicsWorld as OmegaSystem>::effect_row(&w);
        assert!(row.contains(cssl_substrate_omega_step::SubstrateEffect::Sim));
    }

    // ─── Force application via world ───

    #[test]
    fn world_apply_force_routes_to_body() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        w.apply_force(id, Vec3::X);
        assert_eq!(w.body(id).unwrap().force_accum, Vec3::X);
    }

    #[test]
    fn world_apply_torque_routes_to_body() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
        w.apply_torque(id, Vec3::Y);
        assert_eq!(w.body(id).unwrap().torque_accum, Vec3::Y);
    }

    #[test]
    fn world_apply_impulse_changes_velocity() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(RigidBody::new_dynamic(2.0, Shape::Sphere { radius: 1.0 }));
        w.apply_impulse(id, Vec3::new(4.0, 0.0, 0.0));
        // J/m = 4/2 = 2
        assert!(approx_eq(w.body(id).unwrap().linear_velocity.x, 2.0));
    }

    // ─── Total kinetic energy ───

    #[test]
    fn ke_zero_for_static_world() {
        let w = PhysicsWorld::new(WorldConfig::default());
        assert!(approx_eq(w.total_kinetic_energy(), 0.0));
    }

    #[test]
    fn ke_correct_for_moving_body() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        w.insert(
            RigidBody::new_dynamic(2.0, Shape::Sphere { radius: 1.0 })
                .with_linear_velocity(Vec3::new(3.0, 0.0, 0.0)),
        );
        // KE = 0.5 * 2 * 9 = 9
        assert!(approx_eq(w.total_kinetic_energy(), 9.0));
    }

    #[test]
    fn ke_includes_angular_term() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        // Sphere mass=1, r=1 → I = 0.4. ω = 1 rad/s about Y.
        w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_angular_velocity(Vec3::Y),
        );
        // KE_rot = 0.5 * 0.4 * 1 = 0.2
        assert!(approx_eq(w.total_kinetic_energy(), 0.2));
    }

    // ─── Snapshot ───

    #[test]
    fn snapshot_captures_state() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        let id = w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(1.0, 2.0, 3.0))
                .with_linear_velocity(Vec3::new(0.5, 0.0, 0.0)),
        );
        let snap = w.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].0, id);
        assert!(vec3_approx(snap[0].1, Vec3::new(1.0, 2.0, 3.0)));
        assert!(vec3_approx(snap[0].3, Vec3::new(0.5, 0.0, 0.0)));
    }

    #[test]
    fn snapshot_sorted_by_id() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        for i in 0..5 {
            w.insert(
                RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                    .with_position(Vec3::new(i as f64, 0.0, 0.0)),
            );
        }
        let snap = w.snapshot();
        for i in 1..snap.len() {
            assert!(snap[i].0 > snap[i - 1].0);
        }
    }

    // ─── previous_contact_count ───

    #[test]
    fn contact_count_zero_initially() {
        let w = PhysicsWorld::new(WorldConfig::default());
        assert_eq!(w.previous_contact_count(), 0);
    }

    #[test]
    fn contact_count_grows_after_step_with_contact() {
        let mut w = PhysicsWorld::new(WorldConfig::default());
        w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(0.0, 0.5, 0.0)),
        );
        w.insert(RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        }));
        w.step(1.0 / 60.0);
        assert!(w.previous_contact_count() >= 1);
    }
}
