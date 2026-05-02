//! § reward — per-tier reward catalog. Cosmetic-only · ¬ stat-affixes.
//!
//! Mirrors `public.battle_pass_rewards` in
//! `cssl-supabase/migrations/0032_battle_pass.sql`.
//!
//! `RewardKind` enumerates the cosmetic categories explicitly so any
//! attempt to add a non-cosmetic reward (XP-boost · stat-affix · ¬
//! cosmetic-channel) requires editing the enum AND surfacing the change
//! at every consumer — fail-loud at compile-time.

use serde::{Deserialize, Serialize};

use crate::season::SeasonId;

/// Track on which a reward sits. `Free` is always-included ; `Premium`
/// requires Stripe purchase. Both tracks are cosmetic-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RewardTrack {
    Free,
    Premium,
}

/// Cosmetic categories. ¬ stat-affixes · ¬ XP-boost · ¬ pay-for-power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardKind {
    /// Character skin / outfit.
    Skin,
    /// Weapon skin / inspect-anim.
    WeaponSkin,
    /// Companion-pet cosmetic.
    PetCosmetic,
    /// Lantern / decor in the Home pocket-dimension.
    HomeDecor,
    /// Emote / pose.
    Emote,
    /// Mycelial-bloom (visual aura · cosmetic-only).
    MycelialBloom,
    /// Memorial-imprint cosmetic (ties into roguelike-run season-memorials).
    MemorialAura,
    /// Echo-shard gift-pouch (in-game cosmetic-only currency · ¬ purchasable elsewhere).
    /// NOTE : echo-shards are gift-economy currency · ¬ pay-for-power per
    /// `systems/economy.csl § AXIOMS` — they buy cosmetics or fund deeds.
    EchoShardGiftPouch,
}

impl RewardKind {
    /// All variants are cosmetic-channel. This is enforced at the type
    /// level — adding a non-cosmetic variant requires editing this enum
    /// and the [`SeasonRules`](crate::SeasonRules) `validate_reward_is_cosmetic`
    /// caller in `lib.rs`.
    pub const fn is_cosmetic(self) -> bool {
        match self {
            Self::Skin
            | Self::WeaponSkin
            | Self::PetCosmetic
            | Self::HomeDecor
            | Self::Emote
            | Self::MycelialBloom
            | Self::MemorialAura
            | Self::EchoShardGiftPouch => true,
        }
    }
}

/// A reward at `(season, tier, track)`. After the season ends, the
/// reward becomes re-purchasable at `re_purchasable_after_unix_s` with
/// gift-economy cost (NOT a Stripe purchase) — this is THE anti-FOMO
/// mechanism in the data-model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reward {
    pub season_id: SeasonId,
    pub tier: u32,
    pub track: RewardTrack,
    pub cosmetic_id: String,
    pub kind: RewardKind,
    /// Unix-seconds at which the reward becomes re-purchasable post-season
    /// at gift-cost. `None` while the season is active. Set by the
    /// service-role on season-archive.
    pub re_purchasable_after_unix_s: Option<i64>,
}

impl Reward {
    /// Construct a reward, refusing any non-cosmetic kind. Anti-pay-for-power.
    pub fn new(
        season_id: SeasonId,
        tier: u32,
        track: RewardTrack,
        cosmetic_id: impl Into<String>,
        kind: RewardKind,
    ) -> Result<Self, RewardErr> {
        if tier < crate::xp::MIN_TIER || tier > crate::xp::MAX_TIER {
            return Err(RewardErr::TierOutOfRange { got: tier });
        }
        if !kind.is_cosmetic() {
            return Err(RewardErr::NonCosmeticKind);
        }
        let cid = cosmetic_id.into();
        if cid.is_empty() || cid.len() > 128 {
            return Err(RewardErr::CosmeticIdLen { got: cid.len() });
        }
        Ok(Self {
            season_id,
            tier,
            track,
            cosmetic_id: cid,
            kind,
            re_purchasable_after_unix_s: None,
        })
    }

    /// Re-purchasable test : true when the gate is set AND now >= gate.
    pub fn is_re_purchasable_at(&self, now_unix_s: i64) -> bool {
        match self.re_purchasable_after_unix_s {
            Some(gate) => now_unix_s >= gate,
            None => false,
        }
    }

    /// Mark the reward re-purchasable from `gate_unix_s` onward.
    /// Called by the service-role on season-archive.
    pub fn mark_re_purchasable_after(&mut self, gate_unix_s: i64) {
        self.re_purchasable_after_unix_s = Some(gate_unix_s);
    }
}

/// Reward construction errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RewardErr {
    /// `tier` outside `[MIN_TIER, MAX_TIER]`.
    TierOutOfRange { got: u32 },
    /// Attempted to construct a non-cosmetic reward.
    /// Currently structurally-impossible (RewardKind enumerates only
    /// cosmetic variants) but kept as defensive guard for the future.
    NonCosmeticKind,
    /// Cosmetic-id is empty or > 128 bytes.
    CosmeticIdLen { got: usize },
}

impl core::fmt::Display for RewardErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TierOutOfRange { got } => {
                write!(f, "reward tier {got} out of range [1, 100]")
            }
            Self::NonCosmeticKind => write!(f, "reward kind must be cosmetic-only"),
            Self::CosmeticIdLen { got } => {
                write!(f, "cosmetic_id length {got} out of range [1, 128]")
            }
        }
    }
}
impl std::error::Error for RewardErr {}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> SeasonId {
        SeasonId(7)
    }

    #[test]
    fn all_reward_kinds_are_cosmetic() {
        let kinds = [
            RewardKind::Skin,
            RewardKind::WeaponSkin,
            RewardKind::PetCosmetic,
            RewardKind::HomeDecor,
            RewardKind::Emote,
            RewardKind::MycelialBloom,
            RewardKind::MemorialAura,
            RewardKind::EchoShardGiftPouch,
        ];
        for k in kinds {
            assert!(k.is_cosmetic(), "reward-kind must be cosmetic : {k:?}");
        }
    }

    #[test]
    fn reward_is_not_re_purchasable_during_active_season() {
        let r = Reward::new(s(), 5, RewardTrack::Free, "cm_lantern_basic", RewardKind::HomeDecor)
            .unwrap();
        assert!(!r.is_re_purchasable_at(1_700_000_000));
    }

    #[test]
    fn reward_becomes_re_purchasable_post_season() {
        let mut r = Reward::new(s(), 50, RewardTrack::Premium, "cm_skin_silver", RewardKind::Skin)
            .unwrap();
        r.mark_re_purchasable_after(1_800_000_000);
        assert!(r.is_re_purchasable_at(1_800_000_000));
        assert!(r.is_re_purchasable_at(1_800_000_001));
        assert!(!r.is_re_purchasable_at(1_799_999_999));
    }

    #[test]
    fn reward_rejects_oob_tier() {
        let too_low = Reward::new(s(), 0, RewardTrack::Free, "x", RewardKind::Skin);
        let too_high = Reward::new(s(), 999, RewardTrack::Free, "x", RewardKind::Skin);
        assert!(matches!(too_low, Err(RewardErr::TierOutOfRange { .. })));
        assert!(matches!(too_high, Err(RewardErr::TierOutOfRange { .. })));
    }

    #[test]
    fn reward_rejects_empty_id() {
        let r = Reward::new(s(), 1, RewardTrack::Free, "", RewardKind::Skin);
        assert!(matches!(r, Err(RewardErr::CosmeticIdLen { .. })));
    }
}
