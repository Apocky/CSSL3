//! Symplectic-Euler integrator + sleeping logic.
//!
//! § THESIS
//!   Symplectic-Euler advances state as :
//!     v(t+dt) = v(t) + a(t) dt
//!     x(t+dt) = x(t) + v(t+dt) dt
//!   This ordering — VELOCITY FIRST, then POSITION using the new velocity —
//!   is "symplectic" : it preserves phase-space volume (energy doesn't
//!   drift over long sims). Other schemes (forward-Euler, backward-Euler)
//!   diverge or contract phase-space.
//!
//! § INTEGRATION ORDER (per Catto-Bender consensus)
//!   1. apply external forces (gravity, user-applied) to compute acceleration
//!   2. integrate velocities (linear + angular)
//!   3. apply velocity-damping
//!   4. solver runs (modifies velocities to satisfy contacts + joints)
//!   5. integrate positions using post-solver velocities
//!   6. clear force accumulators
//!   This module covers steps 1, 2, 3, 5, 6 ; the solver crate covers step 4.
//!   The PhysicsWorld orchestrates the order.
//!
//! § SLEEPING
//!   A body whose `linear_velocity.length_sq() + angular_velocity.length_sq()`
//!   stays below `sleep_velocity_threshold_sq` for `sleep_frames` consecutive
//!   ticks transitions from `BodyKind::Dynamic` to `BodyKind::Sleeping`.
//!   Sleeping bodies have integration skipped (saves work) but are still
//!   eligible for broadphase queries (for waking on touch). Touching them
//!   wakes them.

use crate::body::{BodyKind, RigidBody};
use crate::math::Vec3;

// ────────────────────────────────────────────────────────────────────────
// § IntegratorConfig
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct IntegratorConfig {
    /// World-space gravity acceleration. Default `(0, -9.81, 0)`.
    pub gravity: Vec3,
    /// Linear velocity threshold (squared) for sleep candidacy.
    /// Default `0.001`.
    pub sleep_linear_threshold_sq: f64,
    /// Angular velocity threshold (squared) for sleep candidacy.
    /// Default `0.001`.
    pub sleep_angular_threshold_sq: f64,
    /// Number of consecutive frames below threshold before sleeping.
    /// Default `60`.
    pub sleep_frames: u32,
    /// Whether to enable sleeping. Default `true`.
    pub sleeping_enabled: bool,
}

impl Default for IntegratorConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            sleep_linear_threshold_sq: 0.001,
            sleep_angular_threshold_sq: 0.001,
            sleep_frames: 60,
            sleeping_enabled: true,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § integrate_symplectic — top-level entry
// ────────────────────────────────────────────────────────────────────────

/// Apply gravity + integrate velocities. Called BEFORE the solver.
pub fn integrate_velocities(bodies: &mut [RigidBody], config: &IntegratorConfig, dt: f64) {
    for body in bodies.iter_mut() {
        if !body.kind.integrates() {
            continue;
        }

        // Gravity contributes a force = mass * g.
        let gravity_force = config.gravity * body.mass;
        body.force_accum += gravity_force;

        // Linear velocity : v += a dt = (F/m) dt
        let linear_accel = body.force_accum * body.inv_mass;
        body.linear_velocity += linear_accel * dt;

        // Angular velocity : ω += I^-1 τ dt
        let inv_inertia_world = body.inv_inertia_world();
        let angular_accel = inv_inertia_world.mul_vec3(body.torque_accum);
        body.angular_velocity += angular_accel * dt;

        // Damping (multiplicative per-frame ; raised to power dt for time-step independence).
        // Stage-0 form : simple per-tick multiplier.
        body.linear_velocity = body.linear_velocity * body.linear_damping;
        body.angular_velocity = body.angular_velocity * body.angular_damping;
    }
}

/// Integrate positions + orientations using the post-solver velocities.
/// Called AFTER the solver.
pub fn integrate_positions(bodies: &mut [RigidBody], dt: f64) {
    for body in bodies.iter_mut() {
        if !body.kind.is_movable() {
            continue;
        }
        if body.kind == BodyKind::Sleeping {
            continue;
        }
        body.position += body.linear_velocity * dt;
        body.orientation = body.orientation.integrate(body.angular_velocity, dt);
    }
}

/// Update sleep timers + transition bodies to/from sleep.
pub fn update_sleeping(bodies: &mut [RigidBody], config: &IntegratorConfig) {
    if !config.sleeping_enabled {
        return;
    }
    for body in bodies.iter_mut() {
        if body.kind != BodyKind::Dynamic {
            continue;
        }
        let lin_sq = body.linear_velocity.length_sq();
        let ang_sq = body.angular_velocity.length_sq();
        if lin_sq < config.sleep_linear_threshold_sq && ang_sq < config.sleep_angular_threshold_sq {
            body.sleep_timer = body.sleep_timer.saturating_add(1);
            if body.sleep_timer >= config.sleep_frames {
                body.kind = BodyKind::Sleeping;
                body.linear_velocity = Vec3::ZERO;
                body.angular_velocity = Vec3::ZERO;
            }
        } else {
            body.sleep_timer = 0;
        }
    }
}

/// Clear accumulators (force + torque) at end-of-step.
pub fn clear_force_accumulators(bodies: &mut [RigidBody]) {
    for body in bodies.iter_mut() {
        body.clear_forces();
    }
}

/// Convenience : run the FULL no-solver integration sweep on a single body
/// (force-application, velocity integration, damping, position integration,
/// force-clear). Used by tests + scenarios that don't need the full solver.
pub fn integrate_symplectic(body: &mut RigidBody, gravity: Vec3, dt: f64) {
    if !body.kind.integrates() {
        return;
    }
    let gravity_force = gravity * body.mass;
    body.force_accum += gravity_force;
    let linear_accel = body.force_accum * body.inv_mass;
    body.linear_velocity += linear_accel * dt;
    let inv_inertia_world = body.inv_inertia_world();
    let angular_accel = inv_inertia_world.mul_vec3(body.torque_accum);
    body.angular_velocity += angular_accel * dt;
    body.linear_velocity = body.linear_velocity * body.linear_damping;
    body.angular_velocity = body.angular_velocity * body.angular_damping;
    body.position += body.linear_velocity * dt;
    body.orientation = body.orientation.integrate(body.angular_velocity, dt);
    body.clear_forces();
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Shape;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn vec3_approx(a: Vec3, b: Vec3) -> bool {
        approx_eq(a.x, b.x) && approx_eq(a.y, b.y) && approx_eq(a.z, b.z)
    }

    // ─── IntegratorConfig ───

    #[test]
    fn integrator_config_default_gravity_y_neg_9_81() {
        let c = IntegratorConfig::default();
        assert!(approx_eq(c.gravity.y, -9.81));
        assert!(approx_eq(c.gravity.x, 0.0));
        assert!(approx_eq(c.gravity.z, 0.0));
    }

    #[test]
    fn integrator_config_sleeping_enabled_default() {
        let c = IntegratorConfig::default();
        assert!(c.sleeping_enabled);
    }

    // ─── integrate_velocities ───

    #[test]
    fn gravity_accelerates_falling_body() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })];
        let config = IntegratorConfig::default();
        integrate_velocities(&mut bodies, &config, 1.0);
        // After 1 second of -9.81 gravity : v_y = -9.81
        assert!(approx_eq(bodies[0].linear_velocity.y, -9.81));
    }

    #[test]
    fn no_gravity_zero_force_no_velocity_change() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })];
        let config = IntegratorConfig {
            gravity: Vec3::ZERO,
            ..Default::default()
        };
        integrate_velocities(&mut bodies, &config, 1.0);
        assert_eq!(bodies[0].linear_velocity, Vec3::ZERO);
    }

    #[test]
    fn applied_force_translates_to_velocity() {
        let mut bodies = vec![RigidBody::new_dynamic(2.0, Shape::Sphere { radius: 1.0 })];
        bodies[0].apply_force(Vec3::new(10.0, 0.0, 0.0));
        let config = IntegratorConfig {
            gravity: Vec3::ZERO,
            ..Default::default()
        };
        integrate_velocities(&mut bodies, &config, 0.5);
        // F=10, m=2, a=5 ; after dt=0.5 → v = 2.5
        assert!(approx_eq(bodies[0].linear_velocity.x, 2.5));
    }

    #[test]
    fn applied_torque_translates_to_angular_velocity() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })];
        // Sphere I = 0.4 ; I^-1 = 2.5
        bodies[0].apply_torque(Vec3::new(0.0, 0.4, 0.0));
        let config = IntegratorConfig {
            gravity: Vec3::ZERO,
            ..Default::default()
        };
        integrate_velocities(&mut bodies, &config, 1.0);
        // τ=0.4, I=0.4 → α=1, after 1s → ω=1
        assert!(approx_eq(bodies[0].angular_velocity.y, 1.0));
    }

    #[test]
    fn linear_damping_reduces_velocity() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(10.0, 0.0, 0.0))
            .with_damping(0.5, 1.0)];
        let config = IntegratorConfig {
            gravity: Vec3::ZERO,
            ..Default::default()
        };
        integrate_velocities(&mut bodies, &config, 0.0);
        assert!(approx_eq(bodies[0].linear_velocity.x, 5.0));
    }

    #[test]
    fn static_bodies_do_not_integrate() {
        let mut bodies = vec![RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        })];
        let config = IntegratorConfig::default();
        integrate_velocities(&mut bodies, &config, 1.0);
        assert_eq!(bodies[0].linear_velocity, Vec3::ZERO);
    }

    #[test]
    fn kinematic_bodies_do_not_integrate() {
        let mut bodies = vec![RigidBody::new_kinematic(Shape::Sphere { radius: 1.0 })];
        bodies[0].linear_velocity = Vec3::new(5.0, 0.0, 0.0);
        let config = IntegratorConfig::default();
        integrate_velocities(&mut bodies, &config, 1.0);
        // Kinematic velocity preserved (no force integration ; user controls v).
        assert!(approx_eq(bodies[0].linear_velocity.x, 5.0));
    }

    // ─── integrate_positions ───

    #[test]
    fn position_advances_with_velocity() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(2.0, 0.0, 0.0))];
        integrate_positions(&mut bodies, 0.5);
        assert!(approx_eq(bodies[0].position.x, 1.0));
    }

    #[test]
    fn orientation_advances_with_angular_velocity() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_angular_velocity(Vec3::Y)];
        integrate_positions(&mut bodies, 0.1);
        // After 0.1 rad about Y, X-axis rotated by ~0.1 rad ⇒ small Z component.
        let rotated_x = bodies[0].orientation.rotate_vec3(Vec3::X);
        assert!(rotated_x.x < 1.0);
        // sin(0.1) ≈ 0.0998 ; expect rotated vector to have small -Z component.
        assert!(rotated_x.z.abs() < 0.2);
    }

    #[test]
    fn sleeping_body_position_unchanged_by_integrate_positions() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(5.0, 0.0, 0.0))];
        bodies[0].kind = BodyKind::Sleeping;
        integrate_positions(&mut bodies, 1.0);
        assert_eq!(bodies[0].position, Vec3::ZERO);
    }

    #[test]
    fn static_body_position_unchanged() {
        let mut bodies = vec![RigidBody::new_static(Shape::Plane {
            normal: Vec3::Y,
            d: 0.0,
        })];
        // Static body has linear_velocity == 0 ; should stay at origin regardless.
        integrate_positions(&mut bodies, 1.0);
        assert_eq!(bodies[0].position, Vec3::ZERO);
    }

    // ─── update_sleeping ───

    #[test]
    fn slow_body_sleeps_after_n_frames() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })];
        let config = IntegratorConfig::default();
        for _ in 0..config.sleep_frames {
            update_sleeping(&mut bodies, &config);
        }
        assert_eq!(bodies[0].kind, BodyKind::Sleeping);
    }

    #[test]
    fn fast_body_does_not_sleep() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(5.0, 0.0, 0.0))];
        let config = IntegratorConfig::default();
        for _ in 0..200 {
            update_sleeping(&mut bodies, &config);
        }
        assert_eq!(bodies[0].kind, BodyKind::Dynamic);
    }

    #[test]
    fn sleeping_disabled_no_transition() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })];
        let config = IntegratorConfig {
            sleeping_enabled: false,
            ..Default::default()
        };
        for _ in 0..200 {
            update_sleeping(&mut bodies, &config);
        }
        assert_eq!(bodies[0].kind, BodyKind::Dynamic);
    }

    #[test]
    fn velocity_above_threshold_resets_sleep_timer() {
        let mut bodies = vec![RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })];
        let config = IntegratorConfig::default();
        // First, accumulate some sleep frames.
        for _ in 0..30 {
            update_sleeping(&mut bodies, &config);
        }
        // Now wake by setting velocity.
        bodies[0].linear_velocity = Vec3::new(5.0, 0.0, 0.0);
        update_sleeping(&mut bodies, &config);
        assert_eq!(bodies[0].sleep_timer, 0);
    }

    // ─── integrate_symplectic ───

    #[test]
    fn symplectic_round_trip_no_force_constant_velocity() {
        let mut body = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(1.0, 0.0, 0.0));
        for _ in 0..100 {
            integrate_symplectic(&mut body, Vec3::ZERO, 0.01);
        }
        // After 1 second at 1 m/s (no force) : x = 1.0
        assert!((body.position.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn symplectic_gravity_x_y_freefall() {
        let mut body = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        let g = Vec3::new(0.0, -9.81, 0.0);
        for _ in 0..100 {
            integrate_symplectic(&mut body, g, 0.01);
        }
        // After 1 second of -9.81 gravity, should have v_y ≈ -9.81 + small correction.
        assert!(body.linear_velocity.y < -9.0);
    }

    #[test]
    fn symplectic_clears_force_accum() {
        let mut body = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 });
        body.apply_force(Vec3::X);
        integrate_symplectic(&mut body, Vec3::ZERO, 0.01);
        assert_eq!(body.force_accum, Vec3::ZERO);
        assert_eq!(body.torque_accum, Vec3::ZERO);
    }

    // ─── clear_force_accumulators ───

    #[test]
    fn clear_force_accumulators_works_on_all_bodies() {
        let mut bodies = vec![
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }),
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }),
        ];
        bodies[0].apply_force(Vec3::X);
        bodies[1].apply_torque(Vec3::Y);
        clear_force_accumulators(&mut bodies);
        assert_eq!(bodies[0].force_accum, Vec3::ZERO);
        assert_eq!(bodies[1].torque_accum, Vec3::ZERO);
    }

    // ─── Determinism ───

    #[test]
    fn determinism_two_runs_same_input_same_output() {
        let mut a = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_linear_velocity(Vec3::new(1.0, 2.0, 3.0));
        let mut b = a.clone();
        for _ in 0..100 {
            integrate_symplectic(&mut a, Vec3::new(0.1, -9.81, 0.0), 0.01);
            integrate_symplectic(&mut b, Vec3::new(0.1, -9.81, 0.0), 0.01);
        }
        assert_eq!(a.position, b.position);
        assert_eq!(a.linear_velocity, b.linear_velocity);
        assert_eq!(a.orientation, b.orientation);
    }

    #[test]
    fn determinism_orientation_bit_equal() {
        let mut a = RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_angular_velocity(Vec3::new(1.0, 2.0, 0.5));
        let mut b = a.clone();
        for _ in 0..1000 {
            integrate_symplectic(&mut a, Vec3::ZERO, 0.001);
            integrate_symplectic(&mut b, Vec3::ZERO, 0.001);
        }
        // Strict bit-equality.
        assert_eq!(a.orientation, b.orientation);
    }

    #[test]
    fn vec3_approx_helper_works() {
        assert!(vec3_approx(Vec3::ZERO, Vec3::ZERO));
        assert!(!vec3_approx(Vec3::ZERO, Vec3::X));
    }
}
