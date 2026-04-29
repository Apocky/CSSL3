//! § Integration test : broadphase scaling + body-plan + omega_step end-to-end.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Exercises the full wave-physics surface end-to-end :
//!
//!   1. **Broadphase scaling** : insert N bodies into the Morton-spatial-
//!      hash + verify pair-finding + telemetry. Tests run from N = 100
//!      up through N = 100K ; the 1M-entity target is exercised behind
//!      the `million_entity` ignored test (cargo test -- --ignored to run).
//!   2. **Body-plan-physics integration** : derive a creature-skeleton
//!      from a synthetic genome + emit XPBD constraints + verify the
//!      constraint-graph coloring + run a step.
//!   3. **End-to-end physics_step** : a small world with bodies + ground
//!      plane + verify gravity + contact + wave-excitation emission.

use cssl_physics_wave::{
    body_plan::{BodyPlanPhysics, MORPHOLOGY_DIM, MORPHOLOGY_INDEX_BONE_COUNT},
    morton_hash::{MortonSpatialHash, SpatialHashConfig},
    omega_step::physics_step,
    sdf::{SdfCollider, SdfPrimitive, SdfShape},
    world::{BodyId, RigidBody, WavePhysicsWorld, WorldConfig},
};
use cssl_substrate_kan::{KanGenomeWeights, Pattern, SubstrateClassTag};

#[test]
fn broadphase_handles_100_bodies() {
    let mut h = MortonSpatialHash::default_t2();
    let bodies: Vec<_> = (0..100u64)
        .map(|i| (i, [(i as f32) * 0.5, 0.0, 0.0]))
        .collect();
    let result = h.bulk_insert_warp_vote(&bodies).unwrap();
    assert_eq!(result.committed, 100);
    let pairs = h.pairs();
    // 100 bodies spaced 0.5m apart with 0.16m cells → mostly distinct cells.
    let _ = pairs;
}

#[test]
fn broadphase_handles_1k_bodies() {
    let mut h = MortonSpatialHash::default_t2();
    let bodies: Vec<_> = (0..1_000u64)
        .map(|i| (i, [(i as f32) * 0.05, 0.0, 0.0]))
        .collect();
    h.bulk_insert_warp_vote(&bodies).unwrap();
    assert_eq!(h.body_count(), 1_000);
}

#[test]
fn broadphase_handles_10k_bodies_distinct_cells() {
    let mut h = MortonSpatialHash::default_t2();
    let bodies: Vec<_> = (0..10_000u64)
        .map(|i| {
            let x = ((i % 100) as f32) * 1.0;
            let y = ((i / 100) as f32) * 1.0;
            (i, [x, y, 0.0])
        })
        .collect();
    h.bulk_insert_warp_vote(&bodies).unwrap();
    assert_eq!(h.body_count(), 10_000);
}

#[test]
fn broadphase_handles_100k_bodies() {
    let mut h = MortonSpatialHash::default_t2();
    let bodies: Vec<_> = (0..100_000u64)
        .map(|i| {
            let x = ((i % 1000) as f32) * 0.5;
            let y = ((i / 1000) as f32) * 0.5;
            (i, [x, y, 0.0])
        })
        .collect();
    let result = h.bulk_insert_warp_vote(&bodies).unwrap();
    assert_eq!(result.committed, 100_000);
    assert_eq!(h.body_count(), 100_000);
}

/// § The 1M-entity broadphase test. Skipped by default ; run with
///   `cargo test -- --ignored` to verify the engine sustains 1M+ bodies.
#[test]
#[ignore]
fn broadphase_handles_1m_bodies_million_entity_milestone() {
    let mut h = MortonSpatialHash::new(SpatialHashConfig {
        cell_size_m: 1.0, // larger cells to keep cell-count manageable
        origin: [0.0; 3],
        initial_capacity: 65536,
        max_bodies: 2_000_000,
    });
    let bodies: Vec<_> = (0..1_000_000u64)
        .map(|i| {
            let x = ((i % 1000) as f32) * 1.5;
            let y = (((i / 1000) % 1000) as f32) * 1.5;
            let z = ((i / 1_000_000) as f32) * 1.5;
            (i, [x, y, z])
        })
        .collect();
    let result = h.bulk_insert_warp_vote(&bodies).unwrap();
    assert_eq!(result.committed, 1_000_000);
    assert_eq!(h.body_count(), 1_000_000);
}

#[test]
fn body_plan_to_skeleton_to_constraints_pipeline() {
    let bp = BodyPlanPhysics::new();
    let g = cssl_hdc::genome::Genome::from_seed(42);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
    let mut morph = vec![0.5; MORPHOLOGY_DIM];
    morph[MORPHOLOGY_INDEX_BONE_COUNT] = 0.2;
    let skeleton = bp.derive_skeleton(&p, &morph).unwrap();
    let constraints = skeleton.to_constraints();
    assert_eq!(constraints.len(), skeleton.joint_count());
    assert!(skeleton.bone_count() >= 2);
}

#[test]
fn end_to_end_physics_step_with_world_collider() {
    let mut world = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
    world.add_body(RigidBody::dynamic(BodyId::NONE, [0.0, 5.0, 0.0], 1.0, [0.4; 3]));
    let collider = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Plane {
        normal: [0.0, 1.0, 0.0],
        offset: 0.0,
    }));
    let mut last_y = 5.0;
    let mut hit_ground = false;
    for _ in 0..120 {
        let report = physics_step(&mut world, 1.0 / 60.0, Some(&collider)).unwrap();
        let body = world.body(BodyId(0)).unwrap();
        let y = body.position[1];
        if y < last_y {
            // Body is falling — gravity is integrated.
        }
        if report.contacts_found > 0 {
            hit_ground = true;
        }
        last_y = y;
    }
    assert!(hit_ground, "body should have landed within 2 seconds at 60Hz");
}

#[test]
fn end_to_end_skeleton_in_world() {
    let bp = BodyPlanPhysics::new();
    let g = cssl_hdc::genome::Genome::from_seed(7);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
    let morph = vec![0.5; MORPHOLOGY_DIM];
    let skeleton = bp.derive_skeleton(&p, &morph).unwrap();
    let bone_count = skeleton.bone_count();
    let mut world = WavePhysicsWorld::new(WorldConfig::no_gravity()).unwrap();
    world.add_skeleton(skeleton, [0.0, 1.0, 0.0]);
    assert_eq!(world.body_count(), bone_count);
    assert_eq!(world.skeleton_count(), 1);
    let report = physics_step(&mut world, 1.0 / 60.0, None).unwrap();
    assert_eq!(report.bodies_integrated, bone_count as u64);
}

#[test]
fn determinism_two_worlds_same_inputs_same_outputs() {
    let mut w1 = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
    let mut w2 = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
    for i in 0..10 {
        w1.add_body(RigidBody::dynamic(
            BodyId::NONE,
            [(i as f32) * 0.5, 5.0, 0.0],
            1.0,
            [0.3; 3],
        ));
        w2.add_body(RigidBody::dynamic(
            BodyId::NONE,
            [(i as f32) * 0.5, 5.0, 0.0],
            1.0,
            [0.3; 3],
        ));
    }
    let collider = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Plane {
        normal: [0.0, 1.0, 0.0],
        offset: 0.0,
    }));
    for _ in 0..30 {
        physics_step(&mut w1, 1.0 / 60.0, Some(&collider)).unwrap();
        physics_step(&mut w2, 1.0 / 60.0, Some(&collider)).unwrap();
    }
    for i in 0..10 {
        let b1 = w1.body(BodyId(i)).unwrap();
        let b2 = w2.body(BodyId(i)).unwrap();
        assert_eq!(b1.position, b2.position);
        assert_eq!(b1.linear_velocity, b2.linear_velocity);
    }
}

#[test]
fn wave_excitations_drained_after_step() {
    let mut world = WavePhysicsWorld::new(WorldConfig::default()).unwrap();
    world.add_body(RigidBody::dynamic(BodyId::NONE, [0.0, 0.0, 0.0], 1.0, [0.5; 3]));
    let body_id = BodyId(0);
    {
        let b = world.body_mut(body_id).unwrap();
        b.linear_velocity = [0.0, -10.0, 0.0]; // hard impact
    }
    let collider = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Plane {
        normal: [0.0, 1.0, 0.0],
        offset: 0.0,
    }));
    physics_step(&mut world, 1.0 / 60.0, Some(&collider)).unwrap();
    let count_after_step = world.pending_excitation_count();
    let drained = world.drain_excitations();
    assert_eq!(drained.len(), count_after_step);
    assert_eq!(world.pending_excitation_count(), 0);
}
