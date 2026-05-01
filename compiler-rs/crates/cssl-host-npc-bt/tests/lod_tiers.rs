// § tests/lod_tiers.rs — 4-tier LOD scheduler
// ════════════════════════════════════════════════════════════════════
// § I> 4 tests : 4 distinct tiers · per-tier Hz · should_tick filter · boundary-conditions
// ════════════════════════════════════════════════════════════════════

use cssl_host_npc_bt::lod::{LodTier, should_tick, tick_freq_hz_for_tier, tier_for_distance};

#[test]
fn four_distinct_tiers_via_distance() {
    let tiers = [
        tier_for_distance(0.0),
        tier_for_distance(50.0),
        tier_for_distance(200.0),
        tier_for_distance(1000.0),
    ];
    assert_eq!(tiers[0], LodTier::Close);
    assert_eq!(tiers[1], LodTier::Mid);
    assert_eq!(tiers[2], LodTier::Far);
    assert_eq!(tiers[3], LodTier::Sleep);
}

#[test]
fn per_tier_hz_canonical() {
    // Per spec : Close=60 · Mid=10 · Far=1 · Sleep=0.1
    assert_eq!(tick_freq_hz_for_tier(LodTier::Close), 60.0);
    assert_eq!(tick_freq_hz_for_tier(LodTier::Mid), 10.0);
    assert_eq!(tick_freq_hz_for_tier(LodTier::Far), 1.0);
    assert!((tick_freq_hz_for_tier(LodTier::Sleep) - 0.1).abs() < 1e-6);
}

#[test]
fn should_tick_mid_six_frame_period() {
    // Mid = 10Hz → period = 6 frames @ 60fps
    assert!(should_tick(LodTier::Mid, 0));
    assert!(!should_tick(LodTier::Mid, 1));
    assert!(!should_tick(LodTier::Mid, 5));
    assert!(should_tick(LodTier::Mid, 6));
    assert!(should_tick(LodTier::Mid, 12));
}

#[test]
fn should_tick_sleep_six_hundred_frame_period() {
    assert!(should_tick(LodTier::Sleep, 0));
    assert!(!should_tick(LodTier::Sleep, 599));
    assert!(should_tick(LodTier::Sleep, 600));
    assert!(should_tick(LodTier::Sleep, 1200));
}
