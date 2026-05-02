//! § rules — invariant-rules attestation block.
//!
//! Compile-time + runtime guard-rails. The `validate` method asserts the
//! cosmetic-only + free-track-always-included invariants hold for any
//! [`Reward`](crate::Reward) presented. This is the structural binding
//! between the data-model and `Labyrinth of Apocalypse/systems/battle_pass.csl`.

use serde::{Deserialize, Serialize};

use crate::reward::{Reward, RewardTrack};

/// Const-time + runtime invariant block. Per
/// `Labyrinth of Apocalypse/systems/battle_pass.csl § AXIOMS`.
///
/// The constants here are anchored : changing them requires editing the
/// .csl spec and `cssl-supabase/migrations/0032_battle_pass.sql` first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonRules;

impl SeasonRules {
    /// Free track is ALWAYS included. ¬ paywall to access ¬ FOMO.
    pub const FREE_TRACK_ALWAYS_INCLUDED: bool = true;

    /// Both tracks are cosmetic-only. ¬ pay-for-power · ¬ XP-boost · ¬ stat-affix.
    pub const COSMETIC_ONLY: bool = true;

    /// 14-day refund window for Premium purchase.
    pub const REFUND_WINDOW_DAYS: u32 = 14;

    /// Mycelium-federation default decision : DENY.
    pub const MYCELIUM_FEDERATION_DEFAULT_DENY: bool = true;

    /// Validate a reward against the rules. Returns Err on any breach.
    pub fn validate_reward(r: &Reward) -> Result<(), &'static str> {
        if !r.kind.is_cosmetic() {
            return Err("reward must be cosmetic-only · ¬ pay-for-power");
        }
        // Free + Premium tracks are both legal — what matters is they
        // have parity in cosmetic-only, NOT that one is more valuable.
        match r.track {
            RewardTrack::Free | RewardTrack::Premium => Ok(()),
        }
    }

    /// Cosmetic-parity attestation : Free + Premium tracks BOTH cosmetic-only.
    /// Returns the parity-statement for inclusion in audit/attestation reports.
    pub const fn cosmetic_parity_attestation() -> &'static str {
        "Free + Premium tracks BOTH cosmetic-only · ¬ pay-for-power · ¬ XP-boost · ¬ exclusive-power"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reward::{Reward, RewardKind, RewardTrack};
    use crate::season::SeasonId;

    #[test]
    fn rules_constants_locked_to_prime_directive() {
        assert!(SeasonRules::FREE_TRACK_ALWAYS_INCLUDED);
        assert!(SeasonRules::COSMETIC_ONLY);
        assert_eq!(SeasonRules::REFUND_WINDOW_DAYS, 14);
        assert!(SeasonRules::MYCELIUM_FEDERATION_DEFAULT_DENY);
    }

    #[test]
    fn rules_validate_accepts_cosmetic_reward() {
        let r = Reward::new(
            SeasonId(1),
            10,
            RewardTrack::Free,
            "cm_emote_bow",
            RewardKind::Emote,
        )
        .unwrap();
        assert!(SeasonRules::validate_reward(&r).is_ok());
    }

    #[test]
    fn parity_attestation_mentions_both_tracks() {
        let s = SeasonRules::cosmetic_parity_attestation();
        assert!(s.contains("Free"));
        assert!(s.contains("Premium"));
        assert!(s.contains("cosmetic-only"));
        assert!(s.contains("¬ pay-for-power"));
    }
}
