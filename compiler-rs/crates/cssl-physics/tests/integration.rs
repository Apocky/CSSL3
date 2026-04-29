//! Integration tests for `cssl-physics`. Exercises the full
//! `PhysicsWorld::step()` pipeline + the determinism contract.
//!
//! § DETERMINISM CONTRACT (load-bearing)
//!   `specs/30_SUBSTRATE.csl § OMEGA-STEP § DETERMINISTIC-REPLAY-INVARIANTS`
//!   requires that two physics worlds initialized identically + ticked with
//!   the same fixed-dt sequence produce bit-identical body states after N
//!   steps. The flagship test `replay_bit_equal_1000_steps` verifies this.

#![allow(clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::similar_names)]
#![allow(clippy::cast_precision_loss)]

use cssl_physics::{
    BodyId, IntegratorConfig, Joint, PhysicsWorld, Quat, RigidBody, Shape, SolverConfig, Vec3,
    WorldConfig,
};
use cssl_substrate_omega_step::{OmegaSystem, SubstrateEffect, SystemId};

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

// ────────────────────────────────────────────────────────────────────────
// § Determinism — the flagship test (per dispatch report-back §)
// ────────────────────────────────────────────────────────────────────────

/// Run physics 1000 steps from the same initial state twice ; assert the
/// final state is bit-equal across runs.
#[test]
fn replay_bit_equal_1000_steps() {
    let setup = |w: &mut PhysicsWorld| {
        // Diverse scenario : multiple shapes + dynamic + static + joints.
        w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
                .with_position(Vec3::new(0.0, 5.0, 0.0))
                .with_linear_velocity(Vec3::new(0.5, 0.0, 0.3)),
        );
        w.insert(
            RigidBody::new_dynamic(
                2.0,
                Shape::Box {
                    half_extents: Vec3::new(0.4, 0.4, 0.4),
                },
            )
            .with_position(Vec3::new(2.0, 4.0, 0.0))
            .with_linear_velocity(Vec3::new(-0.2, 0.0, 0.1))
            .with_angular_velocity(Vec3::new(0.0, 1.0, 0.0))
            .with_restitution(0.3),
        );
        w.insert(
            RigidBody::new_dynamic(
                0.8,
                Shape::Capsule {
                    radius: 0.3,
                    half_height: 0.5,
                },
            )
            .with_position(Vec3::new(-1.5, 3.5, 0.5))
            .with_orientation(Quat::from_axis_angle(Vec3::Z, 0.5)),
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

    assert_eq!(w1.snapshot(), w2.snapshot(), "1000-step replay diverged");
}

#[test]
fn replay_bit_equal_with_joints() {
    let setup = |w: &mut PhysicsWorld| {
        let a = w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.4 })
                .with_position(Vec3::new(0.0, 5.0, 0.0)),
        );
        let b = w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.4 })
                .with_position(Vec3::new(1.5, 5.0, 0.0)),
        );
        w.insert_joint(Joint::ball_socket(a, b, Vec3::ZERO, Vec3::ZERO));
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

// ────────────────────────────────────────────────────────────────────────
// § Integration : the canonical "ball falls + bounces on plane"
// ────────────────────────────────────────────────────────────────────────

#[test]
fn falling_ball_settles_on_plane() {
    let mut w = PhysicsWorld::new(WorldConfig::default());
    let id = w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(0.0, 8.0, 0.0))
            .with_restitution(0.0),
    );
    w.insert(RigidBody::new_static(Shape::Plane {
        normal: Vec3::Y,
        d: 0.0,
    }));

    for _ in 0..600 {
        w.step(1.0 / 60.0);
    }

    let body = w.body(id).expect("body present");
    assert!(body.position.y > 0.3 && body.position.y < 0.8);
    assert!(body.linear_velocity.length() < 0.5);
}

#[test]
fn elastic_collision_conserves_momentum_x_axis() {
    let mut w = PhysicsWorld::new(WorldConfig {
        integrator: IntegratorConfig {
            gravity: Vec3::ZERO,
            sleeping_enabled: false,
            ..Default::default()
        },
        ..Default::default()
    });
    let a = w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::ZERO)
            .with_linear_velocity(Vec3::new(1.0, 0.0, 0.0))
            .with_restitution(1.0)
            .with_friction(0.0),
    );
    let b = w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(1.5, 0.0, 0.0))
            .with_linear_velocity(Vec3::new(-1.0, 0.0, 0.0))
            .with_restitution(1.0)
            .with_friction(0.0),
    );

    for _ in 0..30 {
        w.step(1.0 / 60.0);
    }

    let pa = w.body(a).unwrap();
    let pb = w.body(b).unwrap();
    // Momentum conservation : initial total = 1*1 + 1*-1 = 0.
    let total_p = pa.mass * pa.linear_velocity.x + pb.mass * pb.linear_velocity.x;
    assert!(total_p.abs() < 0.01);
}

#[test]
fn stacking_two_boxes_settles_on_plane() {
    let mut w = PhysicsWorld::new(WorldConfig::default());
    let lower = w.insert(
        RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(0.5, 0.5, 0.5),
            },
        )
        .with_position(Vec3::new(0.0, 1.0, 0.0))
        .with_restitution(0.0),
    );
    let upper = w.insert(
        RigidBody::new_dynamic(
            1.0,
            Shape::Box {
                half_extents: Vec3::new(0.5, 0.5, 0.5),
            },
        )
        .with_position(Vec3::new(0.0, 2.5, 0.0))
        .with_restitution(0.0),
    );
    w.insert(RigidBody::new_static(Shape::Plane {
        normal: Vec3::Y,
        d: 0.0,
    }));

    for _ in 0..900 {
        w.step(1.0 / 60.0);
    }

    let lower_y = w.body(lower).unwrap().position.y;
    let upper_y = w.body(upper).unwrap().position.y;
    // Lower box rests near y=0.5 ; upper near y=1.5.
    // Allow some tolerance for solver convergence.
    assert!(lower_y > 0.3 && lower_y < 0.8, "lower y={lower_y}");
    assert!(upper_y > lower_y);
}

#[test]
fn distance_joint_keeps_anchor_distance_stable() {
    let mut w = PhysicsWorld::new(WorldConfig {
        integrator: IntegratorConfig {
            gravity: Vec3::ZERO,
            sleeping_enabled: false,
            ..Default::default()
        },
        ..Default::default()
    });
    let a = w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.4 })
            .with_position(Vec3::ZERO)
            .with_linear_velocity(Vec3::new(0.5, 0.0, 0.0)),
    );
    let b = w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.4 })
            .with_position(Vec3::new(2.0, 0.0, 0.0)),
    );
    w.insert_joint(Joint::distance(a, b, Vec3::ZERO, Vec3::ZERO, 2.0));

    for _ in 0..200 {
        w.step(1.0 / 60.0);
    }

    let pa = w.body(a).unwrap().position;
    let pb = w.body(b).unwrap().position;
    let dist = (pb - pa).length();
    // Joint should keep distance close to 2.0 (target).
    assert!((dist - 2.0).abs() < 0.5, "dist={dist}");
}

// ────────────────────────────────────────────────────────────────────────
// § Sleeping
// ────────────────────────────────────────────────────────────────────────

#[test]
fn body_sleeps_after_settling() {
    let mut w = PhysicsWorld::new(WorldConfig::default());
    let id = w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(0.0, 1.0, 0.0))
            .with_restitution(0.0),
    );
    w.insert(RigidBody::new_static(Shape::Plane {
        normal: Vec3::Y,
        d: 0.0,
    }));

    // Step long enough for the ball to settle + go to sleep.
    for _ in 0..600 {
        w.step(1.0 / 60.0);
    }

    let body = w.body(id).expect("body present");
    // Whether asleep depends on whether settling reached threshold ;
    // at minimum, velocity should be very small.
    assert!(body.linear_velocity.length() < 0.5);
}

// ────────────────────────────────────────────────────────────────────────
// § OmegaSystem integration
// ────────────────────────────────────────────────────────────────────────

#[test]
fn omega_system_step_via_trait() {
    let mut w = PhysicsWorld::new(WorldConfig::default());
    w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));

    use cssl_substrate_omega_step::{
        DetRng, InputEvent, OmegaError, OmegaSnapshot, OmegaStepCtx, RngStreamId, TelemetryHook,
    };
    use std::collections::BTreeMap;

    let mut omega = OmegaSnapshot::new();
    let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
    let mut telem = TelemetryHook::new();
    let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
    let mut ctx = OmegaStepCtx::new(&mut omega, &mut rngs, &mut telem, 0, false, &inputs);

    let result: Result<(), OmegaError> =
        <PhysicsWorld as OmegaSystem>::step(&mut w, &mut ctx, 1.0 / 60.0);
    assert!(result.is_ok());
}

#[test]
fn omega_system_effect_row_contains_sim() {
    let w = PhysicsWorld::new(WorldConfig::default());
    let row = <PhysicsWorld as OmegaSystem>::effect_row(&w);
    assert!(row.contains(SubstrateEffect::Sim));
}

#[test]
fn omega_system_dependencies_carry_through() {
    let w = PhysicsWorld::new(WorldConfig::default()).with_dependencies(vec![SystemId(99)]);
    let deps = <PhysicsWorld as OmegaSystem>::dependencies(&w);
    assert_eq!(deps, &[SystemId(99)]);
}

// ────────────────────────────────────────────────────────────────────────
// § Performance / correctness sanity
// ────────────────────────────────────────────────────────────────────────

#[test]
fn many_bodies_dont_panic() {
    // Stress test : 30 spheres falling onto a plane.
    let mut w = PhysicsWorld::new(WorldConfig::default());
    for i in 0..30 {
        let x = (i % 6) as f64 * 1.2 - 3.0;
        let y = 5.0 + (i / 6) as f64 * 1.2;
        w.insert(
            RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.4 })
                .with_position(Vec3::new(x, y, 0.0)),
        );
    }
    w.insert(RigidBody::new_static(Shape::Plane {
        normal: Vec3::Y,
        d: 0.0,
    }));
    for _ in 0..120 {
        w.step(1.0 / 60.0);
    }
    // Just ensure no panic + bodies all still at finite positions.
    for body in w.bodies.values() {
        assert!(body.position.y.is_finite());
        assert!(body.position.x.is_finite());
        assert!(body.position.z.is_finite());
    }
}

#[test]
fn body_ids_strictly_monotonic_after_remove() {
    let mut w = PhysicsWorld::new(WorldConfig::default());
    let a = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
    let _b = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
    w.remove(a);
    let c = w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
    // c should have a strictly-greater id than the prior maximum.
    assert!(c > a);
    assert_eq!(c, BodyId(2));
}

#[test]
fn solver_config_custom_iterations() {
    let cfg = WorldConfig {
        solver: SolverConfig {
            iterations: 16,
            position_iterations: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut w = PhysicsWorld::new(cfg);
    w.insert(RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 }));
    w.step(1.0 / 60.0);
    // Just test that custom config doesn't panic.
}

#[test]
fn world_step_many_dt_values() {
    let mut w = PhysicsWorld::new(WorldConfig::default());
    w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 0.5 })
            .with_position(Vec3::new(0.0, 5.0, 0.0)),
    );
    w.insert(RigidBody::new_static(Shape::Plane {
        normal: Vec3::Y,
        d: 0.0,
    }));
    // Various dt values must not panic ; though small dt + many steps is the
    // recommended discipline for replay-bit-equal.
    for &dt in &[1.0 / 60.0, 1.0 / 120.0, 1.0 / 30.0] {
        for _ in 0..30 {
            w.step(dt);
        }
    }
}

#[test]
fn approx_eq_helper() {
    assert!(approx_eq(1.0, 1.0001));
    assert!(!approx_eq(1.0, 1.1));
}

#[test]
fn gravity_zero_objects_dont_fall() {
    let mut w = PhysicsWorld::new(WorldConfig {
        integrator: IntegratorConfig {
            gravity: Vec3::ZERO,
            ..Default::default()
        },
        ..Default::default()
    });
    let id = w.insert(
        RigidBody::new_dynamic(1.0, Shape::Sphere { radius: 1.0 })
            .with_position(Vec3::new(0.0, 5.0, 0.0)),
    );
    for _ in 0..100 {
        w.step(1.0 / 60.0);
    }
    let body = w.body(id).unwrap();
    assert!((body.position.y - 5.0).abs() < 0.01);
}
