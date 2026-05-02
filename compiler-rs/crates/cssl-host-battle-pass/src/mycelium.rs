//! § mycelium — federation-decision for cross-player progress visibility.
//!
//! Sovereign-rule per `Labyrinth of Apocalypse/systems/battle_pass.csl
//! § sovereign-rules` :
//!
//! ```text
//!   - Default-DENY : others cannot see your progress unless you grant a cap.
//!   - Aggregates only via Σ-mask k-anonymous bucket (k >= 5).
//!   - ¬ leaderboard-by-default ; opt-in only.
//! ```
//!
//! This module exposes the *decision*, not the federation transport.
//! Wiring lives in `cssl-edge` endpoints + `cssl-substrate-mycelium`
//! (out-of-scope here).

use serde::{Deserialize, Serialize};

use crate::rules::SeasonRules;

/// Cap-bits the player can grant to allow specific federation modes.
/// Default-DENY when none granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FederationCap {
    /// Allow inclusion in k-anonymous aggregate (k >= 5).
    AggregateKAnon,
    /// Allow exact-progress visibility to a specific friend-list. (Future.)
    PerFriend,
    /// Allow appearance on opt-in leaderboards.
    OptInLeaderboard,
}

/// Federation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FederationDecision {
    /// Federation allowed under the supplied cap.
    Allow { cap: FederationCap },
    /// Default-DENY ; no cap presented or k-anon floor not satisfied.
    Deny { reason: DenyReason },
}

/// Reasons for federation-deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    /// No federation-cap was presented.
    DefaultDeny,
    /// Cap presented but k-anon floor (k >= 5) not satisfied for the bucket.
    KAnonFloorNotMet,
    /// Player has explicitly paused federation (sovereign-revoke).
    PlayerOptOut,
}

impl FederationDecision {
    pub const fn is_allow(self) -> bool {
        matches!(self, Self::Allow { .. })
    }
    pub const fn is_deny(self) -> bool {
        matches!(self, Self::Deny { .. })
    }
}

/// `default_federation_decision()` is the value used when the caller did
/// NOT present a cap. Per PRIME-DIRECTIVE : default-DENY.
pub const fn default_federation_decision() -> FederationDecision {
    FederationDecision::Deny {
        reason: DenyReason::DefaultDeny,
    }
}

/// Sub-routine for an aggregate-cap decision : `bucket_size >= 5` is
/// required ; otherwise deny.
pub const fn k_anon_aggregate_decision(bucket_size: u32) -> FederationDecision {
    const K_ANON_FLOOR: u32 = 5;
    if bucket_size >= K_ANON_FLOOR {
        FederationDecision::Allow {
            cap: FederationCap::AggregateKAnon,
        }
    } else {
        FederationDecision::Deny {
            reason: DenyReason::KAnonFloorNotMet,
        }
    }
}

/// Compile-time cross-check : the canonical default-DENY constant must
/// agree with the [`SeasonRules::MYCELIUM_FEDERATION_DEFAULT_DENY`] flag.
pub const MYCELIUM_FEDERATION_DEFAULT_DENY: bool =
    SeasonRules::MYCELIUM_FEDERATION_DEFAULT_DENY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_decision_is_deny() {
        let d = default_federation_decision();
        assert!(d.is_deny());
        assert!(matches!(
            d,
            FederationDecision::Deny {
                reason: DenyReason::DefaultDeny
            }
        ));
    }

    #[test]
    fn aggregate_below_floor_denied() {
        let d = k_anon_aggregate_decision(3);
        assert!(d.is_deny());
        assert!(matches!(
            d,
            FederationDecision::Deny {
                reason: DenyReason::KAnonFloorNotMet
            }
        ));
    }

    #[test]
    fn aggregate_at_or_above_floor_allowed() {
        let d5 = k_anon_aggregate_decision(5);
        let d100 = k_anon_aggregate_decision(100);
        assert!(d5.is_allow());
        assert!(d100.is_allow());
    }

    #[test]
    fn rules_const_matches_module_const() {
        assert!(MYCELIUM_FEDERATION_DEFAULT_DENY);
    }
}
