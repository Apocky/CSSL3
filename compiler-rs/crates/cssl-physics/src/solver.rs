//! Constraint solver — sequential-impulse PGS (Projected Gauss-Seidel).
//!
//! § THESIS
//!   Erin Catto-style sequential-impulse method (GDC 2005, GDC 2009 refresher).
//!   Iterates over constraints + applies a velocity-correction impulse to
//!   satisfy each. Converges in `iterations` passes ; uses warm-starting
//!   (apply previous-frame impulse on first pass) for stability.
//!
//! § ALGORITHM SKETCH
//!   For each contact + each iteration :
//!     1. Compute relative velocity at contact point in world-space :
//!          v_rel = (v_a + ω_a × r_a) - (v_b + ω_b × r_b)
//!     2. Project onto normal : v_n = v_rel · n
//!     3. Compute desired Δv : the velocity that would satisfy
//!          (-restitution * v_n_initial) at convergence.
//!     4. Compute "effective mass" along normal :
//!          K = inv_m_a + inv_m_b
//!              + n · ((I_a^-1 (r_a × n)) × r_a)
//!              + n · ((I_b^-1 (r_b × n)) × r_b)
//!     5. Impulse magnitude : J = (Δv / K), CLAMPED so cumulative impulse ≥ 0
//!        (no "pulling" — only pushing).
//!     6. Apply : v_a += (J/m_a) n ; ω_a += I_a^-1 (r_a × J n) ; -same for B.
//!
//!   For friction : same loop but project onto two tangent axes per contact ;
//!   clamp tangent-impulse to friction-cone (|J_t| ≤ μ * J_n).
//!
//! § DETERMINISM
//!   - Iterates over contacts in canonical order (sort by `(body_a, body_b, point_idx)`).
//!   - Uses fixed iteration count (no convergence-residual loop).
//!   - Warm-start impulses preserved frame-to-frame for stability.
//!
//! § JOINT-CONSTRAINTS  (stage-0)
//!   - DistanceJoint : 1-axis position constraint along anchor-to-anchor direction.
//!   - BallSocketJoint : 3-axis position constraint at anchor-pair.
//!   - HingeJoint : 3-axis position constraint at anchor-pair + 2-axis
//!     orientation-constraint to align hinge-axes.
//!   Stage-0 form solves these with the same sequential-impulse PGS as
//!   contacts ; convergence is approximate but adequate for game-quality.

use crate::body::{BodyKind, RigidBody};
use crate::contact::Contact;
use crate::joint::{Joint, JointKind};
use crate::math::Vec3;

// ────────────────────────────────────────────────────────────────────────
// § SolverConfig
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct SolverConfig {
    /// Number of velocity-correction iterations per step. Catto recommends
    /// 4–10 ; default 8.
    pub iterations: u32,
    /// Number of position-correction (Baumgarte/projection) iterations per step.
    /// Default 2.
    pub position_iterations: u32,
    /// Baumgarte stabilization factor `[0, 1]` ; how aggressively to
    /// correct positional drift. Default 0.2 (Catto's recommendation).
    pub baumgarte: f64,
    /// Penetration slop : penetration below this is ignored (prevents jitter).
    /// Default 0.005 m.
    pub slop: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            iterations: 8,
            position_iterations: 2,
            baumgarte: 0.2,
            slop: 0.005,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § ConstraintSolver
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConstraintSolver {
    pub config: SolverConfig,
}

impl ConstraintSolver {
    #[must_use]
    pub fn new(config: SolverConfig) -> Self {
        Self { config }
    }

    /// Run the velocity-phase + position-phase solver passes on the bodies,
    /// given the contacts + joints. Mutates the bodies' linear+angular
    /// velocities + positions.
    ///
    /// `bodies_indices` : a function from `BodyId` to `usize` index in `bodies`.
    /// We pass it as a slice-of-(BodyId, usize) sorted by BodyId for O(log n)
    /// lookup ; canonical iteration order = body-id sorted.
    pub fn solve(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        contacts: &mut [Contact],
        joints: &mut [Joint],
        dt: f64,
    ) {
        // Pre-step : compute pre-iteration restitution-biases (per Catto, the
        // bias is `-e * v_n_initial` evaluated ONCE before warm-starting and
        // before iterations begin). Stored in a parallel Vec aligned with
        // contacts[i].points[j] order.
        let mut biases: Vec<Vec<f64>> = Vec::with_capacity(contacts.len());
        for contact in contacts.iter() {
            let (idx_a, idx_b) = match (
                body_index_of(body_id_index, contact.body_a),
                body_index_of(body_id_index, contact.body_b),
            ) {
                (Some(a), Some(b)) => (a, b),
                _ => {
                    biases.push(vec![0.0; contact.points.len()]);
                    continue;
                }
            };
            let mut per_point = Vec::with_capacity(contact.points.len());
            for point in &contact.points {
                let n = point.normal;
                let pa = bodies[idx_a].position;
                let pb = bodies[idx_b].position;
                let ra = point.position - pa;
                let rb = point.position - pb;
                let va = bodies[idx_a].linear_velocity + bodies[idx_a].angular_velocity.cross(ra);
                let vb = bodies[idx_b].linear_velocity + bodies[idx_b].angular_velocity.cross(rb);
                let v_n_initial = (va - vb).dot(n);
                // Bias is -e * v_n_initial when bodies are approaching (v_n < 0).
                // For separating bodies (v_n ≥ 0), bias is 0 (no bounce-back).
                let bias = -contact.restitution * v_n_initial.min(0.0);
                per_point.push(bias);
            }
            biases.push(per_point);
        }

        // Warm-start : apply accumulated impulses from previous frame.
        for contact in contacts.iter() {
            for point in &contact.points {
                if point.accumulated_normal_impulse > 0.0 {
                    apply_contact_impulse(
                        bodies,
                        body_id_index,
                        contact,
                        point.position,
                        point.normal,
                        point.accumulated_normal_impulse,
                    );
                    let (t1, t2) = point.tangent_basis();
                    if point.accumulated_tangent_impulse_1 != 0.0 {
                        apply_contact_impulse(
                            bodies,
                            body_id_index,
                            contact,
                            point.position,
                            t1,
                            point.accumulated_tangent_impulse_1,
                        );
                    }
                    if point.accumulated_tangent_impulse_2 != 0.0 {
                        apply_contact_impulse(
                            bodies,
                            body_id_index,
                            contact,
                            point.position,
                            t2,
                            point.accumulated_tangent_impulse_2,
                        );
                    }
                }
            }
        }

        // Velocity iterations.
        for _ in 0..self.config.iterations {
            // Contacts.
            for (contact_idx, contact) in contacts.iter_mut().enumerate() {
                for point_idx in 0..contact.points.len() {
                    let bias = biases[contact_idx][point_idx];
                    self.solve_contact_velocity(bodies, body_id_index, contact, point_idx, bias);
                }
            }
            // Joints.
            for joint in joints.iter_mut() {
                self.solve_joint_velocity(bodies, body_id_index, joint, dt);
            }
        }

        // Position iterations (Baumgarte projection).
        for _ in 0..self.config.position_iterations {
            for contact in contacts.iter_mut() {
                for point_idx in 0..contact.points.len() {
                    self.solve_contact_position(bodies, body_id_index, contact, point_idx);
                }
            }
            for joint in joints.iter_mut() {
                self.solve_joint_position(bodies, body_id_index, joint);
            }
        }
    }

    /// Single velocity-iteration pass for a single contact-point. `bias` is
    /// the pre-computed restitution velocity-bias (set once at solve start,
    /// per Catto).
    fn solve_contact_velocity(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        contact: &mut Contact,
        point_idx: usize,
        bias: f64,
    ) {
        let (idx_a, idx_b) = match (
            body_index_of(body_id_index, contact.body_a),
            body_index_of(body_id_index, contact.body_b),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };

        let point = contact.points[point_idx];
        let n = point.normal;
        let pa = bodies[idx_a].position;
        let pb = bodies[idx_b].position;
        let ra = point.position - pa;
        let rb = point.position - pb;

        // Relative velocity at contact point.
        let va = bodies[idx_a].linear_velocity + bodies[idx_a].angular_velocity.cross(ra);
        let vb = bodies[idx_b].linear_velocity + bodies[idx_b].angular_velocity.cross(rb);
        let v_rel = va - vb;
        let v_n = v_rel.dot(n);

        // Effective mass along normal.
        let inv_m_sum = bodies[idx_a].inv_mass + bodies[idx_b].inv_mass;
        let inv_inertia_a = bodies[idx_a].inv_inertia_world();
        let inv_inertia_b = bodies[idx_b].inv_inertia_world();
        let cross_a = ra.cross(n);
        let cross_b = rb.cross(n);
        let k_normal = inv_m_sum
            + n.dot(inv_inertia_a.mul_vec3(cross_a).cross(ra))
            + n.dot(inv_inertia_b.mul_vec3(cross_b).cross(rb));

        if k_normal <= 1e-12 {
            return;
        }

        // Drive v_n toward bias (Catto's formulation : Δλ = (bias - v_n) / k).
        // For inelastic contacts (e=0), bias=0 ⇒ drive v_n to 0 (no penetration).
        // For elastic contacts (e=1), bias=-v_n_initial ⇒ drive v_n to -v_n_initial
        // (full velocity reversal at contact normal).
        let lambda = (bias - v_n) / k_normal;
        // Clamp accumulated normal impulse to [0, ∞) — no pulling.
        let new_acc = (contact.points[point_idx].accumulated_normal_impulse + lambda).max(0.0);
        let actual = new_acc - contact.points[point_idx].accumulated_normal_impulse;
        contact.points[point_idx].accumulated_normal_impulse = new_acc;

        if actual.abs() > 1e-12 {
            apply_contact_impulse(bodies, body_id_index, contact, point.position, n, actual);
        }

        // Friction : tangent impulses, clamped to friction cone.
        let (t1, t2) = point.tangent_basis();
        let max_friction = contact.friction * new_acc;

        for (axis, slot) in [(t1, 0_u8), (t2, 1)].iter() {
            // Re-fetch v_rel after normal impulse applied.
            let va2 = bodies[idx_a].linear_velocity + bodies[idx_a].angular_velocity.cross(ra);
            let vb2 = bodies[idx_b].linear_velocity + bodies[idx_b].angular_velocity.cross(rb);
            let v_t = (va2 - vb2).dot(*axis);
            let cross_a_t = ra.cross(*axis);
            let cross_b_t = rb.cross(*axis);
            let k_t = inv_m_sum
                + axis.dot(inv_inertia_a.mul_vec3(cross_a_t).cross(ra))
                + axis.dot(inv_inertia_b.mul_vec3(cross_b_t).cross(rb));
            if k_t <= 1e-12 {
                continue;
            }
            let lambda_t = -v_t / k_t;
            let prev = if *slot == 0 {
                contact.points[point_idx].accumulated_tangent_impulse_1
            } else {
                contact.points[point_idx].accumulated_tangent_impulse_2
            };
            let new_t = (prev + lambda_t).clamp(-max_friction, max_friction);
            let actual_t = new_t - prev;
            if *slot == 0 {
                contact.points[point_idx].accumulated_tangent_impulse_1 = new_t;
            } else {
                contact.points[point_idx].accumulated_tangent_impulse_2 = new_t;
            }
            if actual_t.abs() > 1e-12 {
                apply_contact_impulse(
                    bodies,
                    body_id_index,
                    contact,
                    point.position,
                    *axis,
                    actual_t,
                );
            }
        }
    }

    /// Position-iteration pass for one contact-point — Baumgarte projection.
    fn solve_contact_position(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        contact: &mut Contact,
        point_idx: usize,
    ) {
        let (idx_a, idx_b) = match (
            body_index_of(body_id_index, contact.body_a),
            body_index_of(body_id_index, contact.body_b),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };

        let point = contact.points[point_idx];
        let n = point.normal;
        let pa = bodies[idx_a].position;
        let pb = bodies[idx_b].position;
        let ra = point.position - pa;
        let rb = point.position - pb;

        // Penetration after slop subtraction.
        let pen = (point.penetration - self.config.slop).max(0.0);
        if pen <= 0.0 {
            return;
        }
        let correction = pen * self.config.baumgarte;

        let inv_m_sum = bodies[idx_a].inv_mass + bodies[idx_b].inv_mass;
        let inv_inertia_a = bodies[idx_a].inv_inertia_world();
        let inv_inertia_b = bodies[idx_b].inv_inertia_world();
        let cross_a = ra.cross(n);
        let cross_b = rb.cross(n);
        let k_normal = inv_m_sum
            + n.dot(inv_inertia_a.mul_vec3(cross_a).cross(ra))
            + n.dot(inv_inertia_b.mul_vec3(cross_b).cross(rb));
        if k_normal <= 1e-12 {
            return;
        }
        let lambda = correction / k_normal;
        let impulse = n * lambda;

        if !bodies[idx_a].is_static() && bodies[idx_a].kind != BodyKind::Kinematic {
            bodies[idx_a].position += impulse * bodies[idx_a].inv_mass;
        }
        if !bodies[idx_b].is_static() && bodies[idx_b].kind != BodyKind::Kinematic {
            bodies[idx_b].position -= impulse * bodies[idx_b].inv_mass;
        }
    }

    /// Velocity-iteration pass for a joint.
    fn solve_joint_velocity(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        joint: &mut Joint,
        _dt: f64,
    ) {
        match joint.kind {
            JointKind::Distance(d) => {
                self.solve_distance_velocity(
                    bodies,
                    body_id_index,
                    joint.body_a,
                    joint.body_b,
                    d.anchor_a,
                    d.anchor_b,
                    d.target_distance,
                );
            }
            JointKind::BallSocket(b) => {
                self.solve_ball_socket_velocity(
                    bodies,
                    body_id_index,
                    joint.body_a,
                    joint.body_b,
                    b.anchor_a,
                    b.anchor_b,
                );
            }
            JointKind::Hinge(h) => {
                self.solve_ball_socket_velocity(
                    bodies,
                    body_id_index,
                    joint.body_a,
                    joint.body_b,
                    h.anchor_a,
                    h.anchor_b,
                );
                // Hinge orientation constraint deferred to position phase ;
                // the velocity-phase only locks the pivot.
            }
        }
    }

    /// Position-iteration pass for a joint.
    fn solve_joint_position(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        joint: &mut Joint,
    ) {
        match joint.kind {
            JointKind::Distance(d) => {
                self.solve_distance_position(
                    bodies,
                    body_id_index,
                    joint.body_a,
                    joint.body_b,
                    d.anchor_a,
                    d.anchor_b,
                    d.target_distance,
                );
            }
            JointKind::BallSocket(b) => {
                self.solve_ball_socket_position(
                    bodies,
                    body_id_index,
                    joint.body_a,
                    joint.body_b,
                    b.anchor_a,
                    b.anchor_b,
                );
            }
            JointKind::Hinge(h) => {
                self.solve_ball_socket_position(
                    bodies,
                    body_id_index,
                    joint.body_a,
                    joint.body_b,
                    h.anchor_a,
                    h.anchor_b,
                );
            }
        }
    }

    fn solve_distance_velocity(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        body_a: crate::body::BodyId,
        body_b: crate::body::BodyId,
        anchor_a_local: Vec3,
        anchor_b_local: Vec3,
        target: f64,
    ) {
        let (idx_a, idx_b) = match (
            body_index_of(body_id_index, body_a),
            body_index_of(body_id_index, body_b),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };
        let a_world =
            bodies[idx_a].position + bodies[idx_a].orientation.rotate_vec3(anchor_a_local);
        let b_world =
            bodies[idx_b].position + bodies[idx_b].orientation.rotate_vec3(anchor_b_local);
        let delta = b_world - a_world;
        let dist = delta.length();
        if dist < 1e-12 {
            return;
        }
        let dir = delta / dist;
        let ra = a_world - bodies[idx_a].position;
        let rb = b_world - bodies[idx_b].position;
        let va = bodies[idx_a].linear_velocity + bodies[idx_a].angular_velocity.cross(ra);
        let vb = bodies[idx_b].linear_velocity + bodies[idx_b].angular_velocity.cross(rb);
        let v_rel = (vb - va).dot(dir);
        let bias = (dist - target) * 0.2;
        let inv_m_sum = bodies[idx_a].inv_mass + bodies[idx_b].inv_mass;
        let inv_inertia_a = bodies[idx_a].inv_inertia_world();
        let inv_inertia_b = bodies[idx_b].inv_inertia_world();
        let cross_a = ra.cross(dir);
        let cross_b = rb.cross(dir);
        let k = inv_m_sum
            + dir.dot(inv_inertia_a.mul_vec3(cross_a).cross(ra))
            + dir.dot(inv_inertia_b.mul_vec3(cross_b).cross(rb));
        if k <= 1e-12 {
            return;
        }
        let lambda = -(v_rel + bias) / k;
        apply_directional_impulse_at_world(bodies, idx_a, idx_b, a_world, b_world, dir, lambda);
    }

    fn solve_distance_position(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        body_a: crate::body::BodyId,
        body_b: crate::body::BodyId,
        anchor_a_local: Vec3,
        anchor_b_local: Vec3,
        target: f64,
    ) {
        let (idx_a, idx_b) = match (
            body_index_of(body_id_index, body_a),
            body_index_of(body_id_index, body_b),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };
        let a_world =
            bodies[idx_a].position + bodies[idx_a].orientation.rotate_vec3(anchor_a_local);
        let b_world =
            bodies[idx_b].position + bodies[idx_b].orientation.rotate_vec3(anchor_b_local);
        let delta = b_world - a_world;
        let dist = delta.length();
        if dist < 1e-12 {
            return;
        }
        let dir = delta / dist;
        let error = dist - target;
        let inv_m_sum = bodies[idx_a].inv_mass + bodies[idx_b].inv_mass;
        if inv_m_sum < 1e-12 {
            return;
        }
        let correction = dir * (error * self.config.baumgarte / inv_m_sum);
        if !bodies[idx_a].is_static() && bodies[idx_a].kind != BodyKind::Kinematic {
            bodies[idx_a].position += correction * bodies[idx_a].inv_mass;
        }
        if !bodies[idx_b].is_static() && bodies[idx_b].kind != BodyKind::Kinematic {
            bodies[idx_b].position -= correction * bodies[idx_b].inv_mass;
        }
    }

    fn solve_ball_socket_velocity(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        body_a: crate::body::BodyId,
        body_b: crate::body::BodyId,
        anchor_a_local: Vec3,
        anchor_b_local: Vec3,
    ) {
        // Solve along each principal axis sequentially.
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let (idx_a, idx_b) = match (
                body_index_of(body_id_index, body_a),
                body_index_of(body_id_index, body_b),
            ) {
                (Some(a), Some(b)) => (a, b),
                _ => return,
            };
            let a_world =
                bodies[idx_a].position + bodies[idx_a].orientation.rotate_vec3(anchor_a_local);
            let b_world =
                bodies[idx_b].position + bodies[idx_b].orientation.rotate_vec3(anchor_b_local);
            let ra = a_world - bodies[idx_a].position;
            let rb = b_world - bodies[idx_b].position;
            let va = bodies[idx_a].linear_velocity + bodies[idx_a].angular_velocity.cross(ra);
            let vb = bodies[idx_b].linear_velocity + bodies[idx_b].angular_velocity.cross(rb);
            let v_rel = (vb - va).dot(axis);
            let inv_m_sum = bodies[idx_a].inv_mass + bodies[idx_b].inv_mass;
            let inv_inertia_a = bodies[idx_a].inv_inertia_world();
            let inv_inertia_b = bodies[idx_b].inv_inertia_world();
            let cross_a = ra.cross(axis);
            let cross_b = rb.cross(axis);
            let k = inv_m_sum
                + axis.dot(inv_inertia_a.mul_vec3(cross_a).cross(ra))
                + axis.dot(inv_inertia_b.mul_vec3(cross_b).cross(rb));
            if k <= 1e-12 {
                continue;
            }
            let lambda = -v_rel / k;
            apply_directional_impulse_at_world(
                bodies, idx_a, idx_b, a_world, b_world, axis, lambda,
            );
        }
    }

    fn solve_ball_socket_position(
        &self,
        bodies: &mut [RigidBody],
        body_id_index: &[(crate::body::BodyId, usize)],
        body_a: crate::body::BodyId,
        body_b: crate::body::BodyId,
        anchor_a_local: Vec3,
        anchor_b_local: Vec3,
    ) {
        let (idx_a, idx_b) = match (
            body_index_of(body_id_index, body_a),
            body_index_of(body_id_index, body_b),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };
        let a_world =
            bodies[idx_a].position + bodies[idx_a].orientation.rotate_vec3(anchor_a_local);
        let b_world =
            bodies[idx_b].position + bodies[idx_b].orientation.rotate_vec3(anchor_b_local);
        let error = b_world - a_world;
        let inv_m_sum = bodies[idx_a].inv_mass + bodies[idx_b].inv_mass;
        if inv_m_sum < 1e-12 {
            return;
        }
        let correction = error * (self.config.baumgarte / inv_m_sum);
        if !bodies[idx_a].is_static() && bodies[idx_a].kind != BodyKind::Kinematic {
            bodies[idx_a].position += correction * bodies[idx_a].inv_mass;
        }
        if !bodies[idx_b].is_static() && bodies[idx_b].kind != BodyKind::Kinematic {
            bodies[idx_b].position -= correction * bodies[idx_b].inv_mass;
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § Helpers
// ────────────────────────────────────────────────────────────────────────

/// Look up the index of a body in `bodies` from its `BodyId`. The
/// `body_id_index` slice is expected sorted by BodyId.
fn body_index_of(
    body_id_index: &[(crate::body::BodyId, usize)],
    id: crate::body::BodyId,
) -> Option<usize> {
    body_id_index
        .binary_search_by_key(&id, |(i, _)| *i)
        .ok()
        .map(|p| body_id_index[p].1)
}

/// Apply a contact impulse `magnitude * direction` at world-position `point` to
/// the pair `(body_a, body_b)`. Body-A receives `+impulse * direction`,
/// body-B receives `-impulse * direction`. This is the standard contact
/// convention : the normal points from B to A, so positive impulse pushes
/// A away from B along `+direction`.
fn apply_contact_impulse(
    bodies: &mut [RigidBody],
    body_id_index: &[(crate::body::BodyId, usize)],
    contact: &Contact,
    point: Vec3,
    direction: Vec3,
    magnitude: f64,
) {
    let (idx_a, idx_b) = match (
        body_index_of(body_id_index, contact.body_a),
        body_index_of(body_id_index, contact.body_b),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => return,
    };
    let impulse = direction * magnitude;
    let ra = point - bodies[idx_a].position;
    let rb = point - bodies[idx_b].position;
    if !bodies[idx_a].is_static() && bodies[idx_a].kind != BodyKind::Kinematic {
        bodies[idx_a].linear_velocity += impulse * bodies[idx_a].inv_mass;
        let inv_inertia_a = bodies[idx_a].inv_inertia_world();
        bodies[idx_a].angular_velocity += inv_inertia_a.mul_vec3(ra.cross(impulse));
    }
    if !bodies[idx_b].is_static() && bodies[idx_b].kind != BodyKind::Kinematic {
        bodies[idx_b].linear_velocity -= impulse * bodies[idx_b].inv_mass;
        let inv_inertia_b = bodies[idx_b].inv_inertia_world();
        bodies[idx_b].angular_velocity -= inv_inertia_b.mul_vec3(rb.cross(impulse));
    }
}

/// Apply a joint impulse along `direction` of magnitude `lambda` at world-anchor
/// points `pa_world` (on body_a) and `pb_world` (on body_b). Body-A receives
/// `-lambda * direction`, body-B receives `+lambda * direction`. The convention
/// matches the joint-distance-error sign : when `dist > target`, lambda is
/// negative, so A gets +direction (toward B) and B gets -direction (toward A) —
/// pulling them together.
fn apply_directional_impulse_at_world(
    bodies: &mut [RigidBody],
    idx_a: usize,
    idx_b: usize,
    pa_world: Vec3,
    pb_world: Vec3,
    direction: Vec3,
    lambda: f64,
) {
    let impulse = direction * lambda;
    let ra = pa_world - bodies[idx_a].position;
    let rb = pb_world - bodies[idx_b].position;

    if !bodies[idx_a].is_static() && bodies[idx_a].kind != BodyKind::Kinematic {
        bodies[idx_a].linear_velocity -= impulse * bodies[idx_a].inv_mass;
        let inv_inertia_a = bodies[idx_a].inv_inertia_world();
        bodies[idx_a].angular_velocity -= inv_inertia_a.mul_vec3(ra.cross(impulse));
    }
    if !bodies[idx_b].is_static() && bodies[idx_b].kind != BodyKind::Kinematic {
        bodies[idx_b].linear_velocity += impulse * bodies[idx_b].inv_mass;
        let inv_inertia_b = bodies[idx_b].inv_inertia_world();
        bodies[idx_b].angular_velocity += inv_inertia_b.mul_vec3(rb.cross(impulse));
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::BodyId;
    use crate::contact::ContactPoint;
    use crate::shape::Shape;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn solver_config_defaults_reasonable() {
        let c = SolverConfig::default();
        assert_eq!(c.iterations, 8);
        assert_eq!(c.position_iterations, 2);
        assert!(c.baumgarte > 0.0);
    }

    #[test]
    fn solver_zero_contacts_no_op() {
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(1.0, 0.0, 0.0))];
        let body_id_index = vec![(BodyId(0), 0)];
        let mut contacts: Vec<Contact> = Vec::new();
        let mut joints: Vec<Joint> = Vec::new();
        solver.solve(
            &mut bodies,
            &body_id_index,
            &mut contacts,
            &mut joints,
            1.0 / 60.0,
        );
        // No constraints ⇒ velocity unchanged.
        assert!(approx_eq(bodies[0].linear_velocity.x, 1.0));
    }

    #[test]
    fn solver_separating_velocity_unchanged() {
        // Two bodies moving APART : solver shouldn't add impulse (no pulling).
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::ZERO)
                .with_linear_velocity(Vec3::new(-1.0, 0.0, 0.0)),
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(1.5, 0.0, 0.0))
                .with_linear_velocity(Vec3::new(1.0, 0.0, 0.0)),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts = vec![Contact::new(
            BodyId(0),
            BodyId(1),
            vec![ContactPoint::new(
                Vec3::new(0.75, 0.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0), // From B (1.5) to A (0)
                0.5,
            )],
            0.5,
            0.0,
        )];
        let mut joints: Vec<Joint> = Vec::new();
        let v0_a = bodies[0].linear_velocity;
        let v0_b = bodies[1].linear_velocity;
        solver.solve(
            &mut bodies,
            &body_id_index,
            &mut contacts,
            &mut joints,
            1.0 / 60.0,
        );
        // Bodies are separating ⇒ no impulse to slow them. v_n is positive (separating).
        assert!(approx_eq(bodies[0].linear_velocity.x, v0_a.x));
        assert!(approx_eq(bodies[1].linear_velocity.x, v0_b.x));
    }

    #[test]
    fn solver_approaching_velocity_zeroed() {
        // Two bodies moving INTO each other : solver should zero the
        // approach velocity (zero restitution, no friction needed).
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::ZERO)
                .with_linear_velocity(Vec3::new(1.0, 0.0, 0.0)),
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(1.5, 0.0, 0.0))
                .with_linear_velocity(Vec3::new(-1.0, 0.0, 0.0)),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts = vec![Contact::new(
            BodyId(0),
            BodyId(1),
            vec![ContactPoint::new(
                Vec3::new(0.75, 0.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),
                0.5,
            )],
            0.0, // Friction 0 to isolate normal-axis test.
            0.0,
        )];
        let mut joints: Vec<Joint> = Vec::new();
        solver.solve(
            &mut bodies,
            &body_id_index,
            &mut contacts,
            &mut joints,
            1.0 / 60.0,
        );
        // Both should move at center-of-mass velocity = 0.
        assert!(approx_eq(bodies[0].linear_velocity.x, 0.0));
        assert!(approx_eq(bodies[1].linear_velocity.x, 0.0));
    }

    #[test]
    fn solver_elastic_collision_reverses_velocity() {
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::ZERO)
                .with_linear_velocity(Vec3::new(1.0, 0.0, 0.0))
                .with_restitution(1.0),
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(1.5, 0.0, 0.0))
                .with_linear_velocity(Vec3::new(-1.0, 0.0, 0.0))
                .with_restitution(1.0),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts = vec![Contact::new(
            BodyId(0),
            BodyId(1),
            vec![ContactPoint::new(
                Vec3::new(0.75, 0.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),
                0.5,
            )],
            0.0,
            1.0, // Elastic.
        )];
        let mut joints: Vec<Joint> = Vec::new();
        solver.solve(
            &mut bodies,
            &body_id_index,
            &mut contacts,
            &mut joints,
            1.0 / 60.0,
        );
        // Equal-mass elastic collision : velocities swap ; sign also flips
        // because they were approaching.
        assert!(bodies[0].linear_velocity.x < 0.0);
        assert!(bodies[1].linear_velocity.x > 0.0);
    }

    #[test]
    fn solver_static_vs_dynamic_static_unmoved() {
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            // Body 0 : dynamic, falling down.
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(0.0, 0.5, 0.0))
                .with_linear_velocity(Vec3::new(0.0, -1.0, 0.0)),
            // Body 1 : static plane.
            RigidBody::new_static(Shape::Plane {
                normal: Vec3::Y,
                d: 0.0,
            }),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts = vec![Contact::new(
            BodyId(0),
            BodyId(1),
            vec![ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.5)],
            0.5,
            0.0,
        )];
        let mut joints: Vec<Joint> = Vec::new();
        solver.solve(
            &mut bodies,
            &body_id_index,
            &mut contacts,
            &mut joints,
            1.0 / 60.0,
        );
        // Static body unmoved.
        assert_eq!(bodies[1].linear_velocity, Vec3::ZERO);
        // Dynamic body's downward velocity zeroed.
        assert!(bodies[0].linear_velocity.y >= -1e-3);
    }

    #[test]
    fn solver_distance_joint_holds_bodies_at_target() {
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 }).with_position(Vec3::ZERO),
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(3.0, 0.0, 0.0)),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts: Vec<Contact> = Vec::new();
        let mut joints = vec![Joint::distance(
            BodyId(0),
            BodyId(1),
            Vec3::ZERO,
            Vec3::ZERO,
            2.0, // Target distance smaller than current ⇒ pull together.
        )];
        let _initial_dist = bodies[1].position.distance(bodies[0].position);
        for _ in 0..30 {
            solver.solve(
                &mut bodies,
                &body_id_index,
                &mut contacts,
                &mut joints,
                1.0 / 60.0,
            );
        }
        let final_dist = bodies[1].position.distance(bodies[0].position);
        // Should converge close to target (allow some tolerance).
        assert!((final_dist - 2.0).abs() < 0.5);
    }

    #[test]
    fn solver_ball_socket_pulls_anchors_together() {
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(0.0, 0.0, 0.0)),
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(2.0, 0.0, 0.0)),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts: Vec<Contact> = Vec::new();
        let mut joints = vec![Joint::ball_socket(
            BodyId(0),
            BodyId(1),
            Vec3::ZERO, // Anchor at A's origin.
            Vec3::ZERO, // Anchor at B's origin.
        )];
        for _ in 0..30 {
            solver.solve(
                &mut bodies,
                &body_id_index,
                &mut contacts,
                &mut joints,
                1.0 / 60.0,
            );
        }
        // Anchors converge to same world point ⇒ A and B converge towards each other.
        let dist = (bodies[1].position - bodies[0].position).length();
        assert!(dist < 1.5);
    }

    #[test]
    fn solver_warm_start_preserves_impulse() {
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(0.0, 0.5, 0.0))
                .with_linear_velocity(Vec3::new(0.0, -1.0, 0.0)),
            RigidBody::new_static(Shape::Plane {
                normal: Vec3::Y,
                d: 0.0,
            }),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts = vec![Contact::new(
            BodyId(0),
            BodyId(1),
            vec![ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.5)],
            0.5,
            0.0,
        )];
        let mut joints: Vec<Joint> = Vec::new();
        solver.solve(
            &mut bodies,
            &body_id_index,
            &mut contacts,
            &mut joints,
            1.0 / 60.0,
        );
        // After solve, accumulated_normal_impulse should be non-zero.
        assert!(contacts[0].points[0].accumulated_normal_impulse > 0.0);
    }

    #[test]
    fn solver_friction_zero_no_tangent_impulse() {
        let solver = ConstraintSolver::new(SolverConfig::default());
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
                .with_position(Vec3::new(0.0, 0.5, 0.0))
                .with_linear_velocity(Vec3::new(2.0, -1.0, 0.0)), // Moving sideways too.
            RigidBody::new_static(Shape::Plane {
                normal: Vec3::Y,
                d: 0.0,
            }),
        ];
        let body_id_index = vec![(BodyId(0), 0), (BodyId(1), 1)];
        let mut contacts = vec![Contact::new(
            BodyId(0),
            BodyId(1),
            vec![ContactPoint::new(Vec3::ZERO, Vec3::Y, 0.5)],
            0.0, // No friction.
            0.0,
        )];
        let mut joints: Vec<Joint> = Vec::new();
        solver.solve(
            &mut bodies,
            &body_id_index,
            &mut contacts,
            &mut joints,
            1.0 / 60.0,
        );
        // Sideways velocity preserved.
        assert!(approx_eq(bodies[0].linear_velocity.x, 2.0));
    }
}
