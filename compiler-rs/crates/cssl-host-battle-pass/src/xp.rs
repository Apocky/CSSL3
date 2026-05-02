//! § xp — anti-grind XP-curve : early-fast, late-gradual.
//!
//! Per `Labyrinth of Apocalypse/systems/battle_pass.csl § tier-progression` :
//!
//! ```text
//!   - 100 tiers per season (1..=100).
//!   - Early tiers fast (low XP).
//!   - Later tiers gradual (higher XP) — but ¬ FOMO ; expired rewards
//!     remain re-purchasable post-season at gift-cost.
//! ```
//!
//! The curve is **piecewise-linear** for determinism + simple inversion.
//! It's calibrated so a ~75-day season at typical play-rates lands
//! around tier 60-70 for free-track and ~100 for premium-track players,
//! with the tail (tier 80-100) being a soft long-haul rather than a
//! grinder's gauntlet. (No exclusives ; if a player misses, gift-cost
//! re-purchase is always available post-season.)
//!
//! Three regimes :
//!   1. Tiers   1..= 30  : 1_000 XP per tier (fast onramp).
//!   2. Tiers  31..= 70  : 2_500 XP per tier (steady mid-game).
//!   3. Tiers  71..=100  : 5_000 XP per tier (gradual long-haul).
//!
//! `MAX_TIER = 100`. Outside `[1, 100]` returns 0-XP (sentinel).

/// First valid tier (inclusive).
pub const MIN_TIER: u32 = 1;

/// Last valid tier (inclusive). Mirrors `tier_count` in the SQL migration.
pub const MAX_TIER: u32 = 100;

const REGIME_1_END: u32 = 30;
const REGIME_2_END: u32 = 70;
// REGIME_3_END = MAX_TIER

const XP_PER_TIER_REGIME_1: u64 = 1_000;
const XP_PER_TIER_REGIME_2: u64 = 2_500;
const XP_PER_TIER_REGIME_3: u64 = 5_000;

/// XP required to advance FROM `tier` TO `tier + 1`. Returns 0 outside
/// the valid `[MIN_TIER, MAX_TIER]` range. Tier 100 returns 0 (cap).
pub fn xp_required_for_tier(tier: u32) -> u64 {
    if tier < MIN_TIER || tier >= MAX_TIER {
        return 0;
    }
    if tier <= REGIME_1_END {
        XP_PER_TIER_REGIME_1
    } else if tier <= REGIME_2_END {
        XP_PER_TIER_REGIME_2
    } else {
        XP_PER_TIER_REGIME_3
    }
}

/// Cumulative XP required to REACH `tier` from tier-1. `cumulative_xp_for_tier(1) = 0`.
pub fn cumulative_xp_for_tier(tier: u32) -> u64 {
    if tier <= MIN_TIER {
        return 0;
    }
    let target = tier.min(MAX_TIER);
    let mut total: u64 = 0;
    let r1_max = REGIME_1_END.min(target.saturating_sub(1));
    total += u64::from(r1_max) * XP_PER_TIER_REGIME_1;
    if target > REGIME_1_END + 1 {
        let r2_count = REGIME_2_END.min(target.saturating_sub(1)) - REGIME_1_END;
        total += u64::from(r2_count) * XP_PER_TIER_REGIME_2;
    }
    if target > REGIME_2_END + 1 {
        let r3_count = target.saturating_sub(1) - REGIME_2_END;
        total += u64::from(r3_count) * XP_PER_TIER_REGIME_3;
    }
    total
}

/// Inverse : given cumulative XP, return the highest tier reached.
/// Saturates at `MAX_TIER`. Returns `MIN_TIER` for input `0`.
pub fn tier_for_cumulative_xp(cumulative_xp: u64) -> u32 {
    let mut tier: u32 = MIN_TIER;
    while tier < MAX_TIER {
        let next_threshold = cumulative_xp_for_tier(tier + 1);
        if cumulative_xp < next_threshold {
            return tier;
        }
        tier += 1;
    }
    MAX_TIER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_required_returns_zero_for_invalid_tiers() {
        assert_eq!(xp_required_for_tier(0), 0);
        assert_eq!(xp_required_for_tier(MAX_TIER), 0);
        assert_eq!(xp_required_for_tier(MAX_TIER + 1), 0);
    }

    #[test]
    fn xp_curve_is_monotone_non_decreasing() {
        // Anti-FOMO + anti-grind both demand monotone-non-decreasing.
        for t in MIN_TIER..MAX_TIER {
            let here = xp_required_for_tier(t);
            let next = xp_required_for_tier(t + 1);
            // We're checking the curve never DROPS as we go up — the regimes only escalate.
            if t + 1 < MAX_TIER {
                assert!(
                    next >= here,
                    "xp curve dropped at tier {t}→{} : {here}→{next}",
                    t + 1
                );
            }
        }
    }

    #[test]
    fn xp_curve_early_fast_late_gradual() {
        // Tier 1 (early) is cheaper than tier 71 (late).
        let early = xp_required_for_tier(1);
        let mid = xp_required_for_tier(50);
        let late = xp_required_for_tier(80);
        assert!(early < mid, "early ({early}) must be cheaper than mid ({mid})");
        assert!(mid < late, "mid ({mid}) must be cheaper than late ({late})");
    }

    #[test]
    fn cumulative_xp_at_tier_1_is_zero() {
        assert_eq!(cumulative_xp_for_tier(MIN_TIER), 0);
    }

    #[test]
    fn cumulative_xp_at_tier_max_is_finite_and_reasonable() {
        // Regime breakdown for cumulative-to-reach-tier-100 :
        //   Regime 1 covers tier 1→2..30→31 = 30 transitions × 1_000 = 30_000.
        //   Regime 2 covers tier 31→32..70→71 = 40 transitions × 2_500 = 100_000.
        //   Regime 3 covers tier 71→72..99→100 = 29 transitions × 5_000 = 145_000.
        //   Total = 275_000. (Not 280_000 — there are 99 transitions to reach tier-100,
        //   not 100 ; the last regime stops at 71→100 = 29 transitions, not 30.)
        let total = cumulative_xp_for_tier(MAX_TIER);
        assert_eq!(total, 275_000);
    }

    #[test]
    fn tier_for_cumulative_xp_inverse_roundtrip() {
        for t in MIN_TIER..=MAX_TIER {
            let cum = cumulative_xp_for_tier(t);
            let recovered = tier_for_cumulative_xp(cum);
            assert_eq!(
                recovered, t,
                "roundtrip failed for tier {t} ({cum} cumulative XP → recovered {recovered})"
            );
        }
    }

    #[test]
    fn tier_for_cumulative_xp_saturates_at_max() {
        assert_eq!(tier_for_cumulative_xp(u64::MAX), MAX_TIER);
    }
}
