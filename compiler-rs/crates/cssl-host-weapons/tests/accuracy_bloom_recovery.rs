// § accuracy_bloom_recovery.rs — required by W13-2 brief
// ════════════════════════════════════════════════════════════════════

use cssl_host_weapons::{AccuracyParams, AccuracyState, DeterministicRng};

#[test]
fn bloom_increases_per_shot_recovers_with_time() {
    let mut s = AccuracyState::new(AccuracyParams::PISTOL_DEFAULT);
    let initial = s.current_cone_rad;
    s.on_shot();
    s.on_shot();
    s.on_shot();
    let bloomed = s.current_cone_rad;
    assert!(bloomed > initial);

    // Long pause ⇒ back to base.
    s.tick(60.0);
    assert!((s.current_cone_rad - s.params.base_cone_rad).abs() < 1e-5);
}

#[test]
fn shotgun_blooms_faster_than_pistol() {
    let mut p = AccuracyState::new(AccuracyParams::PISTOL_DEFAULT);
    let mut g = AccuracyState::new(AccuracyParams::SHOTGUN_DEFAULT);
    p.on_shot();
    g.on_shot();
    // Shotgun base-cone is wider AND has bigger per-shot bloom-add ;
    // current cone should exceed pistol's.
    assert!(g.current_cone_rad > p.current_cone_rad);
}

#[test]
fn jitter_seeded_replay_equal() {
    let mut rng_a = DeterministicRng::new(0xC0FF_EE);
    let mut rng_b = DeterministicRng::new(0xC0FF_EE);
    let s = AccuracyState::new(AccuracyParams::SNIPER_DEFAULT);
    for _ in 0..32 {
        let a = s.sample_jitter(&mut rng_a);
        let b = s.sample_jitter(&mut rng_b);
        assert_eq!(a.0.to_bits(), b.0.to_bits());
        assert_eq!(a.1.to_bits(), b.1.to_bits());
    }
}
