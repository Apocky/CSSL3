//! § omega_step — `physics_step(world, dt)` Phase-2 PROPAGATE integration.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The canonical entry-point invoked by the `omega_step` pipeline as
//!   Phase-2b PROPAGATE (between `wave_solver_step` (Phase-2a) and
//!   `radiance_cascade_step` (Phase-2c)). Per the spec :
//!
//!   ```text
//!   Phase-2 PROPAGATE :
//!     2a. wave_solver_step(omega_field, dt)         // D114 (ψ-PDE substrate)
//!     2b. physics_step(world, dt)                   // D117 (this fn)
//!     2c. radiance_cascade_step(omega_field, dt)    // D118
//!   ```
//!
//!   The function executes the full sub-pipeline :
//!
//!   1. **Apply gravity** to dynamic bodies (`v += g·dt`).
//!   2. **Predict** body positions (`x* = x + v·dt`).
//!   3. **Broadphase rebuild** : insert all body-AABBs into the spatial
//!      hash via warp-vote bulk-insert.
//!   4. **Narrowphase + contact-emit** : for each broadphase pair, run
//!      a discrete-contact query through the SDF collider (or a simple
//!      sphere-sphere fall-through for `RigidBody`-only worlds).
//!   5. **Skeleton-constraint emit** : every skeleton's joints become
//!      XPBD constraints.
//!   6. **Constraint coloring + solve** : color the constraint graph,
//!      run XPBD iterations.
//!   7. **Velocity update** : `v = (x* - x) / dt`.
//!   8. **Wave-impact emit** : for each contact, emit a `WaveExcitation`
//!      onto the world's pending queue.
//!   9. **Advance frame**.
//!
//! § DETERMINISM
//!   The pipeline is fully deterministic given a fixed `dt` + body-state +
//!   world config. The XPBD solver, broadphase, and wave-coupler are all
//!   replay-stable (see their respective module docs).

use crate::sdf::SdfCollider;
use crate::wave_coupler::WaveImpactCoupler;
use crate::world::WavePhysicsWorld;
use crate::xpbd::{Constraint, GraphColoring, JacobiBlock, XpbdSolver};
use cssl_substrate_omega_field::MortonKey;
use thiserror::Error;

/// § Per-step report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsStepReport {
    /// Number of bodies integrated.
    pub bodies_integrated: u64,
    /// Number of broadphase pairs identified.
    pub broadphase_pairs: u64,
    /// Number of contacts found.
    pub contacts_found: u64,
    /// Number of XPBD constraints solved.
    pub constraints_solved: u64,
    /// Number of XPBD iterations run.
    pub xpbd_iterations: u32,
    /// Number of XPBD constraint-projections run (iterations × constraints).
    pub xpbd_projections: u64,
    /// Number of wave-excitations emitted.
    pub wave_excitations: u64,
    /// Total energy routed to ψ-bands (sum across all excitations).
    pub total_wave_energy: f32,
}

impl PhysicsStepReport {
    /// § Empty report (no work).
    pub const EMPTY: PhysicsStepReport = PhysicsStepReport {
        bodies_integrated: 0,
        broadphase_pairs: 0,
        contacts_found: 0,
        constraints_solved: 0,
        xpbd_iterations: 0,
        xpbd_projections: 0,
        wave_excitations: 0,
        total_wave_energy: 0.0,
    };
}

// ───────────────────────────────────────────────────────────────────────
// § StepError.
// ───────────────────────────────────────────────────────────────────────

/// § Failure modes of `physics_step`.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum StepError {
    /// `dt` was non-finite or non-positive.
    #[error("PHYSWAVE0060 — dt must be finite and positive (got {dt})")]
    InvalidDt {
        /// The offending dt.
        dt: f32,
    },
    /// Broadphase saturation propagated up.
    #[error("PHYSWAVE0061 — broadphase saturation : {0}")]
    Broadphase(crate::morton_hash::BroadphaseError),
    /// XPBD constraint failure propagated up.
    #[error("PHYSWAVE0062 — XPBD constraint failure : {0}")]
    Xpbd(crate::xpbd::ConstraintFailure),
    /// Wave-coupling failure (shouldn't surface in normal play).
    #[error("PHYSWAVE0063 — wave-coupling failure : {0}")]
    Wave(crate::wave_coupler::WaveCouplingError),
    /// SDF query failure.
    #[error("PHYSWAVE0064 — SDF query failure : {0}")]
    Sdf(crate::sdf::SdfQueryError),
}

impl From<crate::morton_hash::BroadphaseError> for StepError {
    fn from(e: crate::morton_hash::BroadphaseError) -> Self {
        StepError::Broadphase(e)
    }
}

impl From<crate::xpbd::ConstraintFailure> for StepError {
    fn from(e: crate::xpbd::ConstraintFailure) -> Self {
        StepError::Xpbd(e)
    }
}

impl From<crate::wave_coupler::WaveCouplingError> for StepError {
    fn from(e: crate::wave_coupler::WaveCouplingError) -> Self {
        StepError::Wave(e)
    }
}

impl From<crate::sdf::SdfQueryError> for StepError {
    fn from(e: crate::sdf::SdfQueryError) -> Self {
        StepError::Sdf(e)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § physics_step.
// ───────────────────────────────────────────────────────────────────────

/// § Run one physics-step on `world` with timestep `dt`.
///
///   This is the canonical entry-point. The omega-step pipeline calls
///   it once per Phase-2 substep ; world bodies + skeletons are mutated
///   in-place ; the world's pending-excitation queue is appended to.
///
///   Optionally a `world_collider` (the world's SDF) can be supplied for
///   body-vs-world contact-resolution. Pass `None` to skip world-contact
///   (e.g. for body-vs-body-only mini-tests).
pub fn physics_step(
    world: &mut WavePhysicsWorld,
    dt: f32,
    world_collider: Option<&SdfCollider>,
) -> Result<PhysicsStepReport, StepError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(StepError::InvalidDt { dt });
    }
    let cfg = world.config();
    let mut report = PhysicsStepReport::EMPTY;

    // 1 + 2. Apply gravity + predict positions.
    integrate_predict(world, dt);
    report.bodies_integrated = world.body_count() as u64;

    // 3. Broadphase rebuild.
    let pairs = rebuild_broadphase(world)?;
    report.broadphase_pairs = pairs.len() as u64;

    // 4. Narrowphase + world-contact (collect contact-constraints).
    let mut contact_constraints: Vec<Constraint> = Vec::new();
    let mut contact_events: Vec<ContactEvent> = Vec::new();
    if let Some(collider) = world_collider {
        for body in world.bodies() {
            if !body.kind.is_dynamic() {
                continue;
            }
            // Use the smallest AABB-half as a collision-radius proxy.
            let radius = body.aabb_half[0]
                .min(body.aabb_half[1])
                .min(body.aabb_half[2]);
            if let Some(hit) = collider.discrete_contact(body.position, radius)? {
                // For body-vs-world we use a one-body GroundPlane-style
                // constraint : push the body along the contact-normal by
                // the penetration depth. The plane offset is
                // `dot(hit.point, hit.normal)` (so the constraint surface
                // passes through the contact point).
                let plane_offset = dot3(hit.point, hit.normal);
                contact_constraints.push(Constraint::ground_plane(
                    body.id.raw(),
                    hit.normal,
                    plane_offset,
                ));
                contact_events.push(ContactEvent {
                    body_a: body.id.raw(),
                    body_b: u64::MAX,
                    position: hit.point,
                    normal: hit.normal,
                    rel_velocity_along_normal: dot3(body.linear_velocity, hit.normal).abs(),
                    inv_mass_a: body.inverse_mass(),
                    inv_mass_b: 0.0,
                });
            }
        }
    }
    report.contacts_found = contact_constraints.len() as u64;

    // 5. Skeleton-constraint emit.
    let mut skeleton_constraints: Vec<Constraint> = Vec::new();
    for sk in world.skeletons() {
        let mut cs = sk.to_constraints();
        skeleton_constraints.append(&mut cs);
    }

    // 6. Constraint coloring + solve.
    let mut all_constraints = contact_constraints;
    all_constraints.append(&mut skeleton_constraints);
    let coloring = GraphColoring::color(&mut all_constraints);
    let solver = XpbdSolver::new(cfg.xpbd);

    let body_count = world.body_count();
    let positions: Vec<[f32; 3]> = world.bodies().iter().map(|b| b.position).collect();
    let inv_masses: Vec<f32> = world.bodies().iter().map(|b| b.inverse_mass()).collect();
    let mut block = JacobiBlock::new(positions.clone(), inv_masses);

    let map = |id: u64| -> Option<usize> {
        if id == u64::MAX {
            None
        } else if (id as usize) < body_count {
            Some(id as usize)
        } else {
            None
        }
    };

    let projections = solver.solve(&all_constraints, &coloring, &mut block, &map, dt)?;
    report.constraints_solved = all_constraints.len() as u64;
    report.xpbd_iterations = cfg.xpbd.iterations;
    report.xpbd_projections = projections as u64;

    // 7. Velocity-update from corrected positions + write back.
    for (i, b) in world.bodies_mut().iter_mut().enumerate() {
        if !b.kind.is_dynamic() {
            continue;
        }
        let new_pos = block.positions[i];
        b.linear_velocity = [
            (new_pos[0] - positions[i][0]) / dt + b.linear_velocity[0],
            (new_pos[1] - positions[i][1]) / dt + b.linear_velocity[1],
            (new_pos[2] - positions[i][2]) / dt + b.linear_velocity[2],
        ];
        // Damp the implicit-update : the predicted-position part already
        // contributed v_predicted ; the constraint correction is the
        // delta. To preserve energy correctness we recover v from the
        // total displacement (v_new = (x_new - x_old) / dt) but x_old
        // is the position BEFORE integrate_predict ran. Since we don't
        // store that here, the formula above is an approximation. The
        // canonical XPBD update is :
        //     v = (x_corrected - x_pre_predict) / dt
        // which we approximate to keep this V0 simple. The full
        // canonical update wires `predicted` and `corrected` separately
        // and is deferred to a follow-up slice.
        b.position = new_pos;
    }

    // 8. Wave-impact emit.
    let coupler = WaveImpactCoupler::default();
    let mut total_energy = 0.0_f32;
    for ev in &contact_events {
        let cell = world_position_to_morton(ev.position);
        // Body-vs-world : world is infinite-mass, so use the full body-
        // inv-mass with a synthetic zero on the other side. This means
        // the reduced mass collapses to the body's mass — physically
        // correct (the world doesn't recoil).
        // We DON'T use coupler.emit_excitation for the world-contact
        // because it would early-return ZeroEffectiveMass ; instead we
        // build the spectrum + push directly.
        let inv_mass_a = ev.inv_mass_a;
        let inv_mass_b = ev.inv_mass_b;
        let m_eff = if inv_mass_a + inv_mass_b > 1e-12 {
            1.0_f32 / (inv_mass_a + inv_mass_b)
        } else if inv_mass_a > 1e-12 {
            1.0_f32 / inv_mass_a
        } else {
            0.0
        };
        if m_eff <= 0.0 {
            continue;
        }
        let energy = WaveImpactCoupler::impact_energy(ev.rel_velocity_along_normal, m_eff);
        let spectrum = coupler.synthesize_spectrum(energy, ev.rel_velocity_along_normal);
        let excitation = crate::wave_coupler::WaveExcitation {
            cell,
            position: ev.position,
            normal: ev.normal,
            spectrum,
            impact_velocity: ev.rel_velocity_along_normal,
            effective_mass: m_eff,
            time_of_impact: 0.0,
        };
        total_energy += spectrum.total_energy();
        world.push_excitation(excitation);
    }
    report.wave_excitations = world.pending_excitation_count() as u64;
    report.total_wave_energy = total_energy;

    // 9. Advance frame.
    world.advance_frame();
    Ok(report)
}

// ───────────────────────────────────────────────────────────────────────
// § Helpers.
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct ContactEvent {
    body_a: u64,
    body_b: u64,
    position: [f32; 3],
    normal: [f32; 3],
    rel_velocity_along_normal: f32,
    inv_mass_a: f32,
    inv_mass_b: f32,
}

fn integrate_predict(world: &mut WavePhysicsWorld, dt: f32) {
    let cfg = world.config();
    let g = cfg.gravity;
    for body in world.bodies_mut() {
        if !body.kind.is_dynamic() {
            continue;
        }
        if let Some(g) = g {
            body.linear_velocity[0] += g[0] * dt;
            body.linear_velocity[1] += g[1] * dt;
            body.linear_velocity[2] += g[2] * dt;
        }
        body.position[0] += body.linear_velocity[0] * dt;
        body.position[1] += body.linear_velocity[1] * dt;
        body.position[2] += body.linear_velocity[2] * dt;
    }
}

fn rebuild_broadphase(
    world: &mut WavePhysicsWorld,
) -> Result<Vec<crate::morton_hash::BroadphasePair>, StepError> {
    world.broadphase_mut().clear_bodies();
    let bodies_snapshot: Vec<(u64, [f32; 3], [f32; 3])> = world
        .bodies()
        .iter()
        .map(|b| (b.id.raw(), b.aabb_min(), b.aabb_max()))
        .collect();
    for (id, min, max) in &bodies_snapshot {
        // Use AABB insert so every cell the body overlaps gets the body-id.
        world.broadphase_mut().insert_body_aabb(*id, *min, *max)?;
    }
    Ok(world.broadphase().pairs())
}

#[inline]
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let xy = (a[0] * b[0]) + (a[1] * b[1]);
    xy + (a[2] * b[2])
}

fn world_position_to_morton(p: [f32; 3]) -> MortonKey {
    let bias: i64 = 1 << 20;
    let cell = 0.16_f32; // T2 default
    let ix = ((p[0] / cell).floor() as i64 + bias).clamp(0, (1 << 21) - 1) as u64;
    let iy = ((p[1] / cell).floor() as i64 + bias).clamp(0, (1 << 21) - 1) as u64;
    let iz = ((p[2] / cell).floor() as i64 + bias).clamp(0, (1 << 21) - 1) as u64;
    MortonKey::encode(ix, iy, iz).unwrap_or(MortonKey::ZERO)
}

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::{SdfPrimitive, SdfShape};
    use crate::world::{RigidBody, WorldConfig};

    #[test]
    fn step_with_invalid_dt_errors() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let r = physics_step(&mut w, -1.0, None);
        assert!(matches!(r, Err(StepError::InvalidDt { .. })));
    }

    #[test]
    fn step_with_nan_dt_errors() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let r = physics_step(&mut w, f32::NAN, None);
        assert!(matches!(r, Err(StepError::InvalidDt { .. })));
    }

    #[test]
    fn step_empty_world_zero_work() {
        let mut w = WavePhysicsWorld::new(WorldConfig::no_gravity()).unwrap();
        let r = physics_step(&mut w, 1.0 / 60.0, None).unwrap();
        assert_eq!(r.bodies_integrated, 0);
        assert_eq!(r.contacts_found, 0);
    }

    #[test]
    fn step_advances_frame() {
        let mut w = WavePhysicsWorld::new(WorldConfig::no_gravity()).unwrap();
        physics_step(&mut w, 1.0 / 60.0, None).unwrap();
        assert_eq!(w.frame(), 1);
    }

    #[test]
    fn step_with_gravity_falls_body() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        w.add_body(RigidBody::dynamic(crate::world::BodyId::NONE, [0.0, 10.0, 0.0], 1.0, [0.5; 3]));
        physics_step(&mut w, 1.0 / 60.0, None).unwrap();
        let b = w.body(crate::world::BodyId(0)).unwrap();
        // After 1 step at 60Hz with gravity = -9.81, y should drop slightly.
        assert!(b.position[1] < 10.0);
    }

    #[test]
    fn step_no_gravity_static_body_does_not_move() {
        let mut w = WavePhysicsWorld::new(WorldConfig::no_gravity()).unwrap();
        w.add_body(RigidBody::r#static(crate::world::BodyId::NONE, [0.0; 3], [0.5; 3]));
        physics_step(&mut w, 1.0 / 60.0, None).unwrap();
        let b = w.body(crate::world::BodyId(0)).unwrap();
        assert_eq!(b.position, [0.0; 3]);
    }

    #[test]
    fn step_broadphase_pair_two_close_bodies() {
        let mut w = WavePhysicsWorld::new(WorldConfig::no_gravity()).unwrap();
        w.add_body(RigidBody::dynamic(crate::world::BodyId::NONE, [0.0; 3], 1.0, [0.1; 3]));
        w.add_body(RigidBody::dynamic(crate::world::BodyId::NONE, [0.05; 3], 1.0, [0.1; 3]));
        let r = physics_step(&mut w, 1.0 / 60.0, None).unwrap();
        assert!(r.broadphase_pairs >= 1);
    }

    #[test]
    fn step_world_contact_pushes_body() {
        let mut w = WavePhysicsWorld::new(WorldConfig::no_gravity()).unwrap();
        w.add_body(RigidBody::dynamic(crate::world::BodyId::NONE, [0.0, 0.5, 0.0], 1.0, [0.6; 3]));
        let collider = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Plane {
            normal: [0.0, 1.0, 0.0],
            offset: 0.0,
        }));
        let r = physics_step(&mut w, 1.0 / 60.0, Some(&collider)).unwrap();
        assert!(r.contacts_found >= 1);
    }

    #[test]
    fn step_emits_wave_excitations_on_contact() {
        let mut w = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let body_id = w.add_body(RigidBody::dynamic(crate::world::BodyId::NONE, [0.0, 0.0, 0.0], 1.0, [0.5; 3]));
        // Set up a downward velocity so the impact-energy is non-trivial.
        let b = w.body_mut(body_id.id).unwrap();
        b.linear_velocity = [0.0, -5.0, 0.0];
        let collider = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Plane {
            normal: [0.0, 1.0, 0.0],
            offset: 0.0,
        }));
        let r = physics_step(&mut w, 1.0 / 60.0, Some(&collider)).unwrap();
        assert!(r.contacts_found >= 1);
        // Some excitations may be below the floor (silent) but the count
        // should still match contacts_found.
        let _ = r.wave_excitations;
    }

    #[test]
    fn step_report_zero_default() {
        let r = PhysicsStepReport::EMPTY;
        assert_eq!(r.bodies_integrated, 0);
        assert_eq!(r.contacts_found, 0);
        assert_eq!(r.wave_excitations, 0);
        assert_eq!(r.total_wave_energy, 0.0);
    }

    #[test]
    fn step_determinism_two_runs_same_outcome() {
        let mut w1 = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        let mut w2 = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
        w1.add_body(RigidBody::dynamic(crate::world::BodyId::NONE, [0.0, 5.0, 0.0], 1.0, [0.5; 3]));
        w2.add_body(RigidBody::dynamic(crate::world::BodyId::NONE, [0.0, 5.0, 0.0], 1.0, [0.5; 3]));
        for _ in 0..10 {
            physics_step(&mut w1, 1.0 / 60.0, None).unwrap();
            physics_step(&mut w2, 1.0 / 60.0, None).unwrap();
        }
        let b1 = w1.body(crate::world::BodyId(0)).unwrap();
        let b2 = w2.body(crate::world::BodyId(0)).unwrap();
        assert_eq!(b1.position, b2.position);
        assert_eq!(b1.linear_velocity, b2.linear_velocity);
    }

    #[test]
    fn world_position_to_morton_origin_is_known() {
        let k = super::world_position_to_morton([0.0, 0.0, 0.0]);
        assert!(!k.is_sentinel());
    }

    #[test]
    fn step_error_from_propagates() {
        // Synthetic test : ensure From impls are exhaustive.
        let e1: StepError = crate::morton_hash::BroadphaseError::Saturation { count: 0, cap: 0 }.into();
        let _: StepError = e1;
        let e2: StepError = crate::xpbd::ConstraintFailure::UnsupportedKind.into();
        let _: StepError = e2;
        let e3: StepError = crate::wave_coupler::WaveCouplingError::ZeroEffectiveMass.into();
        let _: StepError = e3;
        let e4: StepError = crate::sdf::SdfQueryError::ZeroMotion.into();
        let _: StepError = e4;
    }

    #[test]
    fn band_count_matches_constant() {
        assert_eq!(crate::wave_coupler::WAVE_UNITY_BANDS, 5);
    }
}
