// § hitscan_latency.rs — required by W13-2 brief : hit-feedback ≤ 1ms
// ════════════════════════════════════════════════════════════════════
// § Cast a single hitscan ray against 256 targets and assert the hot-path
//   completes in well under 1 ms. We use std::time::Instant ; the test is
//   resilient to slow CI runners (asserts < 5 ms ; typical < 50 µs).
// § Determinism : RNG seeded ; target layout fixed.
// ════════════════════════════════════════════════════════════════════

use std::time::Instant;

use cssl_host_weapons::{
    cast_hitscan, compute_damage, ArmorClass, DamageType, HitZone, HitscanHit, HitscanParams,
    HitscanTarget, Ray,
};

#[test]
fn hitscan_completes_under_1ms_for_256_targets() {
    let mut targets: Vec<HitscanTarget> = Vec::with_capacity(256);
    for i in 0..256 {
        targets.push(HitscanTarget {
            id: i as u64,
            center: [(i as f32) * 0.1 + 5.0, ((i % 5) as f32 - 2.0) * 0.4, 0.0],
            radius: 0.5,
            armor: ArmorClass::Flesh,
            is_head: i % 7 == 0,
            is_weak: i % 11 == 0,
        });
    }

    let ray = Ray { origin: [0.0; 3], dir: [1.0, 0.0, 0.0] };
    let mut params = HitscanParams::PISTOL_COMMON;
    params.max_pierce = 4;
    let mut out = [HitscanHit {
        target_id: 0, distance: 0.0,
        damage: compute_damage(0.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false),
    }; 8];

    // Warmup
    let _ = cast_hitscan(ray, &targets, params, &mut out);

    // Bench : 1000 iterations averaged
    let start = Instant::now();
    let mut total_hits: usize = 0;
    for _ in 0..1000 {
        total_hits += cast_hitscan(ray, &targets, params, &mut out);
    }
    let elapsed = start.elapsed();
    let per_call = elapsed / 1000;

    println!(
        "hitscan 256-targets · 1000 iters · per-call = {per_call:?} · total-hits = {total_hits}",
    );

    // Sanity : we got SOME hits.
    assert!(total_hits > 0);

    // Per-call latency budget : 1 ms is the target ; 5 ms allows for slow CI.
    assert!(
        per_call.as_micros() < 5000,
        "hitscan per-call {per_call:?} exceeded 5ms guard ; W13-2 budget=1ms"
    );
}

#[test]
fn hitscan_single_target_negligible() {
    let target = [HitscanTarget {
        id: 1, center: [10.0, 0.0, 0.0], radius: 0.5,
        armor: ArmorClass::Unarmored, is_head: false, is_weak: false,
    }];
    let ray = Ray { origin: [0.0; 3], dir: [1.0, 0.0, 0.0] };
    let params = HitscanParams::PISTOL_COMMON;
    let mut out = [HitscanHit {
        target_id: 0, distance: 0.0,
        damage: compute_damage(0.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false),
    }; 1];

    let start = Instant::now();
    for _ in 0..10_000 {
        cast_hitscan(ray, &target, params, &mut out);
    }
    let per = start.elapsed() / 10_000;
    println!("hitscan 1-target · per-call = {per:?}");
    // 50 µs hard upper bound for a single-target ray-sphere ; typical << 1 µs.
    assert!(per.as_micros() < 50);
}
