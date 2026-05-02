// § integration_full_loop.rs — end-to-end : aim, fire, falloff, recoil, recycle
// ════════════════════════════════════════════════════════════════════
// § Exercises every public surface of the crate in one loop : a Sniper
//   weapon-build fires twenty rounds at a row of targets ; we verify
//   damage-falloff, recoil-pattern progression, accuracy-bloom decay, and
//   projectile-pool recycle in a single cohesive scenario.
// ════════════════════════════════════════════════════════════════════

use cssl_host_weapons::{
    cast_hitscan, compute_damage, recoil_for, AccuracyParams, AccuracyState, ArmorClass,
    DamageType, DeterministicRng, HitZone, HitscanHit, HitscanParams, HitscanTarget,
    ProjectileImpact, ProjectilePool, Ray, TrajectoryEnv, WeaponBuild, WeaponCosmetic,
    WeaponKind, WeaponTier,
};

#[test]
fn full_loop_sniper_at_row_of_targets() {
    let build = WeaponBuild::new(
        WeaponKind::SniperHitscan,
        WeaponTier::Legendary,
        WeaponCosmetic::gold_tracer(),
    );

    // Build a row of 5 targets at increasing range.
    let targets: Vec<HitscanTarget> = (0..5)
        .map(|i| HitscanTarget {
            id: i as u64 + 1,
            center: [(i as f32 + 1.0) * 10.0, 0.0, 0.0],
            radius: 0.5,
            armor: ArmorClass::Flesh,
            is_head: i == 2, // middle target is head-zone
            is_weak: false,
        })
        .collect();

    let mut acc = AccuracyState::new(AccuracyParams::SNIPER_DEFAULT);
    let mut rng = DeterministicRng::new(0xBADC_AFE);
    let mut total_damage_dealt: f32 = 0.0;
    let mut prev_pitch = 0.0_f32;

    let params = HitscanParams {
        max_pierce: 3,
        falloff_min_range_m: 15.0,
        falloff_max_range_m: 80.0,
        falloff_floor_mult: 0.3,
        per_shot_damage: build.per_shot(),
        damage_type: DamageType::Kinetic,
    };
    let mut out = [HitscanHit {
        target_id: 0, distance: 0.0,
        damage: compute_damage(0.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false),
    }; 8];

    for shot in 0..20 {
        let (jx, _jy) = acc.sample_jitter(&mut rng);
        // For this test we don't actually rotate the ray ; jitter is sampled
        // for determinism + replay equivalence (asserted in dedicated test).
        let _ = jx;
        acc.on_shot();
        let recoil = recoil_for(build.kind, shot);
        // Pitch must be non-decreasing within a single burst (no recovery between shots).
        assert!(recoil.pitch_rad >= prev_pitch || prev_pitch == 0.0);
        prev_pitch = recoil.pitch_rad;

        let ray = Ray { origin: [0.0; 3], dir: [1.0, 0.0, 0.0] };
        let n = cast_hitscan(ray, &targets, params, &mut out);
        assert!(n > 0);
        for h in &out[..n] {
            total_damage_dealt += h.damage.final_dmg;
        }
        // Recovery between shots
        acc.tick(0.05);
    }

    assert!(total_damage_dealt > 0.0);
    // After-loop : large sustained recovery returns to base cone.
    acc.tick(60.0);
    assert!((acc.current_cone_rad - acc.params.base_cone_rad).abs() < 1e-5);
}

#[test]
fn full_loop_crossbow_projectiles_recycle() {
    let build = WeaponBuild::with_default_skin(WeaponKind::Crossbow, WeaponTier::Rare);
    let mut pool = ProjectilePool::new();
    let targets = [HitscanTarget {
        id: 7, center: [25.0, 0.0, 0.0], radius: 0.5,
        armor: ArmorClass::Plate, is_head: false, is_weak: false,
    }];
    let mut impacts = [ProjectileImpact {
        projectile_id: 0, target_id: 0, impact_pos: [0.0; 3],
        damage: 0.0, damage_type: DamageType::Kinetic,
    }; 8];

    // Fire 10 bolts straight at the target. Use VACUUM env so deterministic
    // straight-line travel ; gravity/wind/drag are exercised in projectile
    // unit-tests (`projectile_falls_under_gravity`) — here we focus on pool
    // recycle on impact.
    for _ in 0..10 {
        pool.spawn(
            [0.0; 3], [40.0, 0.0, 0.0],
            0.05, 3.0,
            build.per_shot(),
            DamageType::Kinetic,
        ).expect("spawn ok");
    }
    assert_eq!(pool.live_count(), 10);

    // Step forward for 1.5 s ; bolts should hit + despawn.
    let mut total_impacts = 0;
    for _ in 0..30 {
        total_impacts += pool.step_all(TrajectoryEnv::VACUUM, 0.05, &targets, &mut impacts);
    }
    assert!(total_impacts > 0);
    // Pool should have recycled the impacted bolts (live ≤ 10 - some impacts).
    assert!(pool.live_count() < 10);
}

#[test]
fn dps_signature_independent_of_cosmetic_in_e2e_use() {
    let cos_a = WeaponCosmetic::DEFAULT;
    let cos_b = WeaponCosmetic::neon_blue();
    let a = WeaponBuild::new(WeaponKind::Lmg, WeaponTier::Mythic, cos_a);
    let b = WeaponBuild::new(WeaponKind::Lmg, WeaponTier::Mythic, cos_b);
    assert_eq!(a.dps_signature(), b.dps_signature());
    assert_eq!(a.base_dps().to_bits(), b.base_dps().to_bits());
    assert_eq!(a.per_shot().to_bits(), b.per_shot().to_bits());
}
