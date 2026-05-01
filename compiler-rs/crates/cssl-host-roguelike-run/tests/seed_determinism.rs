// § seed_determinism ← seed-replay-bit-equal tests
// ════════════════════════════════════════════════════════════════════
// § I> same player_id_hash + run_counter → identical seed
// § I> DetRng replay produces bit-identical sequences
// § I> distinct call-sites uncorrelated
// ════════════════════════════════════════════════════════════════════

use cssl_host_roguelike_run::seed::{derive_rng_u32, derive_rng_u64, pin_seed, DetRng};

#[test]
fn pin_seed_deterministic() {
    let a = pin_seed(0xCAFE_BABE, 42);
    let b = pin_seed(0xCAFE_BABE, 42);
    assert_eq!(a, b);
    // Different run_counter → different seed.
    let c = pin_seed(0xCAFE_BABE, 43);
    assert_ne!(a, c);
}

#[test]
fn det_rng_replay_5000_steps_bit_equal() {
    let seed = pin_seed(0x1234_5678, 99);
    let mut a = DetRng::from_seed(seed);
    let mut b = DetRng::from_seed(seed);
    let mut diverged = 0usize;
    for _ in 0..5000 {
        let av = a.next_u64();
        let bv = b.next_u64();
        if av != bv {
            diverged += 1;
        }
    }
    assert_eq!(diverged, 0, "DetRng diverged in {diverged} steps");
    assert_eq!(a.steps, 5000);
    assert_eq!(b.steps, 5000);
}

#[test]
fn derive_call_site_diversity() {
    let seed = pin_seed(0xDEAD_BEEF, 7);
    let mut samples = std::collections::BTreeSet::new();
    for cs in 0u64..256 {
        samples.insert(derive_rng_u64(seed, cs));
    }
    // 256 distinct call-sites → expect 256 distinct outputs (collision-rate ≈ 0).
    assert!(samples.len() >= 250, "got {} unique outputs", samples.len());

    // u32 derivation is deterministic for the same call-site.
    let x1 = derive_rng_u32(seed, 17);
    let x2 = derive_rng_u32(seed, 17);
    assert_eq!(x1, x2);
}
