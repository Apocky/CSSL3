//! § progression — per-player season-progression state-machine.
//!
//! Mirrors `public.battle_pass_progression` in
//! `cssl-supabase/migrations/0032_battle_pass.sql`.
//!
//! State-machine :
//!
//! ```text
//!     Started ──xp──▶ Tier-up ──xp──▶ ... ──tier=100──▶ Capped
//!                          │                                │
//!                          ├──pause────────────▶ Paused────┘
//!                          │                       │
//!                          └──resume───────────────┘
//! ```
//!
//! Pause is sovereign-revocable : a paused row no longer accumulates XP.
//! `is_premium` is set when Stripe-checkout completes (see endpoint
//! `cssl-edge/pages/api/battle-pass/unlock.ts`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::reward::RewardTrack;
use crate::season::SeasonId;
use crate::xp::{cumulative_xp_for_tier, tier_for_cumulative_xp, MAX_TIER, MIN_TIER};

/// Pause flag for the per-player progression. Sovereign-revocable :
/// a paused row no longer accumulates XP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseState {
    pub paused: bool,
    /// Unix-seconds when the player paused, `None` when never paused
    /// or when resumed after a pause.
    pub paused_at_unix_s: Option<i64>,
}

impl PauseState {
    pub const fn unpaused() -> Self {
        Self {
            paused: false,
            paused_at_unix_s: None,
        }
    }

    pub const fn paused_at(unix_s: i64) -> Self {
        Self {
            paused: true,
            paused_at_unix_s: Some(unix_s),
        }
    }
}

impl Default for PauseState {
    fn default() -> Self {
        Self::unpaused()
    }
}

/// Per-player per-season progression row.
///
/// Determinism : `redeemed_tiers` is `BTreeMap` (not HashMap) so the
/// JSON serialization is stable + diffable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progression {
    pub player_id_hash: u64,
    pub season_id: SeasonId,
    pub cumulative_xp: u64,
    pub tier: u32,
    pub is_premium: bool,
    pub pause: PauseState,
    /// Map of `tier → track` recording which rewards have been redeemed.
    /// Anti-double-claim guard.
    pub redeemed_tiers: BTreeMap<u32, RewardTrack>,
}

impl Progression {
    /// Fresh progression at season start.
    pub fn new(player_id_hash: u64, season_id: SeasonId, is_premium: bool) -> Self {
        Self {
            player_id_hash,
            season_id,
            cumulative_xp: 0,
            tier: MIN_TIER,
            is_premium,
            pause: PauseState::unpaused(),
            redeemed_tiers: BTreeMap::new(),
        }
    }

    /// Add XP to the row, advancing the tier-counter accordingly. No-op
    /// when paused (returns `Ignored::Paused`). Saturates at MAX_TIER ;
    /// further XP-additions still increment `cumulative_xp` for stat-tracking
    /// but the `tier` field caps.
    pub fn award_xp(&mut self, delta_xp: u64) -> ProgressionUpdate {
        if self.pause.paused {
            return ProgressionUpdate::Ignored {
                reason: ProgressionEvent::Paused,
            };
        }
        let prev_tier = self.tier;
        self.cumulative_xp = self.cumulative_xp.saturating_add(delta_xp);
        self.tier = tier_for_cumulative_xp(self.cumulative_xp);
        if self.tier == prev_tier {
            ProgressionUpdate::Accepted {
                tier_before: prev_tier,
                tier_after: self.tier,
                tier_changed: false,
            }
        } else {
            ProgressionUpdate::Accepted {
                tier_before: prev_tier,
                tier_after: self.tier,
                tier_changed: true,
            }
        }
    }

    /// Pause progression. Idempotent : pausing-while-paused returns Ok.
    pub fn pause(&mut self, now_unix_s: i64) -> Result<(), BattlePassErr> {
        self.pause = PauseState::paused_at(now_unix_s);
        Ok(())
    }

    /// Resume progression. Idempotent.
    pub fn resume(&mut self) -> Result<(), BattlePassErr> {
        self.pause = PauseState::unpaused();
        Ok(())
    }

    /// Mark the player as premium-track. Triggered by Stripe-webhook.
    pub fn upgrade_to_premium(&mut self) {
        self.is_premium = true;
    }

    /// Revoke premium-track (e.g. on full-refund). The `redeemed_tiers`
    /// map is RETAINED — anti-clawback per gift-economy axiom — but
    /// future-tiers may only redeem from the Free track.
    pub fn downgrade_from_premium(&mut self) {
        self.is_premium = false;
    }

    /// Attempt to redeem the reward at `tier` on `track`. Returns Err on
    /// double-claim, oob-tier, locked-track, or unreached-tier.
    pub fn try_redeem(
        &mut self,
        tier: u32,
        track: RewardTrack,
    ) -> Result<(), BattlePassErr> {
        if !(MIN_TIER..=MAX_TIER).contains(&tier) {
            return Err(BattlePassErr::TierOutOfRange { got: tier });
        }
        if tier > self.tier {
            return Err(BattlePassErr::TierNotReached {
                tier,
                current: self.tier,
            });
        }
        if matches!(track, RewardTrack::Premium) && !self.is_premium {
            return Err(BattlePassErr::PremiumLocked);
        }
        if self.redeemed_tiers.contains_key(&tier) {
            return Err(BattlePassErr::AlreadyRedeemed { tier });
        }
        self.redeemed_tiers.insert(tier, track);
        Ok(())
    }

    /// XP needed to reach the next tier (0 if at MAX_TIER).
    pub fn xp_to_next_tier(&self) -> u64 {
        if self.tier >= MAX_TIER {
            return 0;
        }
        let next_threshold = cumulative_xp_for_tier(self.tier + 1);
        next_threshold.saturating_sub(self.cumulative_xp)
    }
}

/// Outcome of an `award_xp` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressionUpdate {
    Accepted {
        tier_before: u32,
        tier_after: u32,
        tier_changed: bool,
    },
    Ignored {
        reason: ProgressionEvent,
    },
}

/// Reason for an `Ignored` update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressionEvent {
    Paused,
}

/// Errors from progression operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BattlePassErr {
    /// Tier outside `[MIN_TIER, MAX_TIER]`.
    TierOutOfRange { got: u32 },
    /// Player tried to redeem a tier they have not yet reached.
    TierNotReached { tier: u32, current: u32 },
    /// Player tried to redeem from Premium track but is not premium.
    PremiumLocked,
    /// Reward at this tier was already claimed.
    AlreadyRedeemed { tier: u32 },
}

impl core::fmt::Display for BattlePassErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TierOutOfRange { got } => {
                write!(f, "tier {got} out of range [1, 100]")
            }
            Self::TierNotReached { tier, current } => {
                write!(f, "tier {tier} not reached (current = {current})")
            }
            Self::PremiumLocked => write!(f, "premium track locked · purchase required"),
            Self::AlreadyRedeemed { tier } => {
                write!(f, "tier {tier} already redeemed · ¬ double-claim")
            }
        }
    }
}
impl std::error::Error for BattlePassErr {}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(is_premium: bool) -> Progression {
        Progression::new(0xCAFE_F00D, SeasonId(1), is_premium)
    }

    #[test]
    fn award_xp_advances_tier_when_threshold_crossed() {
        let mut p = fresh(false);
        let r = p.award_xp(1_500); // 1500 XP > tier-2 threshold of 1000.
        match r {
            ProgressionUpdate::Accepted {
                tier_before,
                tier_after,
                tier_changed,
            } => {
                assert_eq!(tier_before, 1);
                assert!(tier_after >= 2);
                assert!(tier_changed);
            }
            _ => panic!("expected Accepted"),
        }
    }

    #[test]
    fn pause_blocks_xp_award() {
        let mut p = fresh(false);
        p.pause(1_700_000_000).unwrap();
        let r = p.award_xp(10_000);
        assert!(matches!(
            r,
            ProgressionUpdate::Ignored {
                reason: ProgressionEvent::Paused
            }
        ));
        assert_eq!(p.cumulative_xp, 0);
        assert_eq!(p.tier, 1);
    }

    #[test]
    fn resume_unblocks_xp() {
        let mut p = fresh(false);
        p.pause(1_700_000_000).unwrap();
        p.resume().unwrap();
        let r = p.award_xp(2_000);
        assert!(matches!(r, ProgressionUpdate::Accepted { .. }));
    }

    #[test]
    fn premium_lock_denied_redemption() {
        let mut p = fresh(false);
        p.award_xp(50_000);
        let err = p.try_redeem(5, RewardTrack::Premium);
        assert!(matches!(err, Err(BattlePassErr::PremiumLocked)));
    }

    #[test]
    fn premium_unlocked_after_purchase() {
        let mut p = fresh(false);
        p.upgrade_to_premium();
        p.award_xp(50_000);
        assert!(p.try_redeem(5, RewardTrack::Premium).is_ok());
    }

    #[test]
    fn double_claim_denied() {
        let mut p = fresh(false);
        p.award_xp(50_000);
        assert!(p.try_redeem(5, RewardTrack::Free).is_ok());
        let err = p.try_redeem(5, RewardTrack::Free);
        assert!(matches!(err, Err(BattlePassErr::AlreadyRedeemed { .. })));
    }

    #[test]
    fn tier_not_reached_denied() {
        let mut p = fresh(true);
        let err = p.try_redeem(50, RewardTrack::Premium);
        assert!(matches!(err, Err(BattlePassErr::TierNotReached { .. })));
    }

    #[test]
    fn xp_to_next_tier_decreases_with_award() {
        let mut p = fresh(false);
        let before = p.xp_to_next_tier();
        p.award_xp(500);
        let after = p.xp_to_next_tier();
        assert!(after < before, "xp_to_next_tier should decrease : {before} → {after}");
    }
}
