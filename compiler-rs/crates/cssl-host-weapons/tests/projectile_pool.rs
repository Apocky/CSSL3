// § projectile_pool.rs — pool capacity + reuse + sweep-collision integration
// ════════════════════════════════════════════════════════════════════
// § Required by W13-2 brief : projectile-pool-recycle test.
// ════════════════════════════════════════════════════════════════════

use cssl_host_weapons::{
    ArmorClass, DamageType, HitscanTarget, ProjectileImpact, ProjectilePool, TrajectoryEnv,
    MAX_PROJECTILES,
};

#[test]
fn pool_capacity_is_256_pre_alloc() {
    let pool = ProjectilePool::new();
    assert_eq!(pool.capacity(), 256);
    assert_eq!(MAX_PROJECTILES, 256);
}

#[test]
fn pool_recycles_after_collision() {
    let mut pool = ProjectilePool::new();
    let targets = [HitscanTarget {
        id: 42,
        center: [3.0, 0.0, 0.0],
        radius: 0.5,
        armor: ArmorClass::Unarmored,
        is_head: false,
        is_weak: false,
    }];
    let mut impacts = [ProjectileImpact {
        projectile_id: 0,
        target_id: 0,
        impact_pos: [0.0; 3],
        damage: 0.0,
        damage_type: DamageType::Kinetic,
    }; 16];

    // Spawn 10 projectiles ; aim them at the target.
    for _ in 0..10 {
        pool.spawn([0.0; 3], [10.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic)
            .expect("spawn ok");
    }
    assert_eq!(pool.live_count(), 10);

    // Step 1 second ; all should hit + despawn.
    let n = pool.step_all(TrajectoryEnv::VACUUM, 1.0, &targets, &mut impacts);
    assert_eq!(n, 10);
    assert_eq!(pool.live_count(), 0);

    // Re-fill : pool should accept fresh spawns up to cap.
    for _ in 0..10 {
        assert!(pool
            .spawn([0.0; 3], [10.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic)
            .is_some());
    }
    assert_eq!(pool.live_count(), 10);
}

#[test]
fn pool_full_returns_none_then_recycles() {
    let mut pool = ProjectilePool::new();
    // Fill pool.
    for _ in 0..MAX_PROJECTILES {
        assert!(pool
            .spawn([0.0; 3], [10.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic)
            .is_some());
    }
    // Over-spawn rejected.
    assert!(pool
        .spawn([0.0; 3], [10.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic)
        .is_none());

    // Despawn all via TTL expiry.
    let mut impacts = [ProjectileImpact {
        projectile_id: 0,
        target_id: 0,
        impact_pos: [0.0; 3],
        damage: 0.0,
        damage_type: DamageType::Kinetic,
    }; 0];
    for _ in 0..600 {
        pool.step_all(TrajectoryEnv::VACUUM, 0.1, &[], &mut impacts);
    }
    assert_eq!(pool.live_count(), 0);

    // Pool should accept again.
    assert!(pool
        .spawn([0.0; 3], [10.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic)
        .is_some());
}

#[test]
fn pool_state_iter_only_alive() {
    let mut pool = ProjectilePool::new();
    pool.spawn([0.0; 3], [1.0, 0.0, 0.0], 0.05, 5.0, 30.0, DamageType::Energy);
    pool.spawn([1.0, 0.0, 0.0], [2.0, 0.0, 0.0], 0.05, 5.0, 30.0, DamageType::Thermal);

    let live: Vec<_> = pool.live_iter().collect();
    assert_eq!(live.len(), 2);
    for p in live {
        assert!(p.alive);
    }
}
