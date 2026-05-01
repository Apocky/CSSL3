//! § drop_table tests — per-context distribution + sample-rarity determinism +
//!   Mythic-floor preservation.

use cssl_host_gear_archetype::{
    distribution_for_context, roll_drop, sample_rarity, Biome, DropContext, GearSlot, Rarity,
};

#[test]
fn distribution_sums_to_unity_after_renorm() {
    let ctx = DropContext { mob_tier: 1, biome: Biome::Dungeon, magic_find: 0.0 };
    let probs = distribution_for_context(&ctx);
    let sum: f32 = probs.iter().sum();
    assert!((sum - 1.0).abs() < 1e-3, "distribution sum {sum} not near 1");
    // Mythic floor preserved.
    assert!(probs[5] >= 0.0001, "mythic floor lost ({})", probs[5]);
}

#[test]
fn higher_mob_tier_shifts_curve_upward() {
    let lo_ctx = DropContext { mob_tier: 1, biome: Biome::Dungeon, magic_find: 0.0 };
    let hi_ctx = DropContext { mob_tier: 5, biome: Biome::Dungeon, magic_find: 0.0 };
    let lo = distribution_for_context(&lo_ctx);
    let hi = distribution_for_context(&hi_ctx);
    // Common decreases from tier-1 to tier-5 ; non-common rises.
    assert!(hi[0] < lo[0], "higher tier should reduce Common");
    let lo_nc: f32 = lo[1..].iter().sum();
    let hi_nc: f32 = hi[1..].iter().sum();
    assert!(hi_nc > lo_nc, "higher tier should raise non-Common total");
}

#[test]
fn sample_rarity_deterministic_and_roll_drop_emits_gear() {
    let ctx = DropContext { mob_tier: 3, biome: Biome::Crypt, magic_find: 0.5 };
    // Same seed + ctx → same rarity.
    let r1 = sample_rarity(&ctx, 0xDEAD_BEEF_u128);
    let r2 = sample_rarity(&ctx, 0xDEAD_BEEF_u128);
    assert_eq!(r1, r2);
    // roll_drop returns Some.
    let g = roll_drop(&ctx, 0xDEAD_BEEF_u128, Some(GearSlot::MainHand)).expect("drop");
    assert_eq!(g.slot, GearSlot::MainHand);
    // Rolled rarity matches sample_rarity (consistency).
    assert_eq!(g.rarity, r1);
    // Material's rarity-floor ≤ rolled rarity.
    assert!(
        g.base.base_mat.rarity_floor() <= g.rarity,
        "mat floor {:?} above rolled {:?}",
        g.base.base_mat.rarity_floor(),
        g.rarity
    );
    // Roll an explicit Common+ to ensure the path doesn't panic on any rarity.
    for seed in 0u128..20 {
        let g = roll_drop(&ctx, seed, Some(GearSlot::Helm)).expect("drop helm");
        assert!(matches!(
            g.rarity,
            Rarity::Common
                | Rarity::Uncommon
                | Rarity::Rare
                | Rarity::Epic
                | Rarity::Legendary
                | Rarity::Mythic
        ));
    }
}
