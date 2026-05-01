//! § nutrient — region-scoped query for spores
//!
//! ⊑ NutrientQuery { region · kind · since_ts · max_count · caller_tier }
//! ⊑ poll filters : region-eq · kind-eq · ts ≥ since_ts · spore-tier ≤ caller-tier

use crate::privacy::{OptInTier, RegionTag};
use crate::spore::{Spore, SporeKind};
use serde::{Deserialize, Serialize};

/// § NutrientQuery — selection-criteria for [`crate::TransportAdapter::poll`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NutrientQuery {
    pub region: RegionTag,
    pub kind: SporeKind,
    /// § Lower-bound (inclusive) timestamp.
    pub since_ts: u64,
    /// § Upper-bound on returned spore count (per-region).
    pub max_count: usize,
    /// § Caller's consent-tier — caps which spores may be returned.
    pub caller_tier: OptInTier,
}

impl NutrientQuery {
    /// § matches — does this spore satisfy the (region · kind · since_ts)
    /// filter? Tier filter is applied separately so callers can audit
    /// blocked-on-escalation events.
    #[must_use]
    pub fn matches(&self, spore: &Spore) -> bool {
        spore.region == self.region
            && spore.kind == self.kind
            && spore.ts >= self.since_ts
    }
}

/// § NutrientResponse — list of spores plus the query echo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NutrientResponse {
    pub region: RegionTag,
    pub kind: SporeKind,
    pub spores: Vec<Spore>,
}

impl NutrientResponse {
    pub const fn empty(region: RegionTag, kind: SporeKind) -> Self {
        Self {
            region,
            kind,
            spores: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.spores.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spores.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spore::SporeBuilder;

    fn mk(ts: u64, region: RegionTag, kind: SporeKind, tier: OptInTier) -> Spore {
        SporeBuilder {
            region,
            kind,
            ts,
            opt_in_tier: tier,
            emitter_pubkey: [4_u8; 32],
            payload: serde_json::json!({"v": ts}),
        }
        .build(OptInTier::Public)
        .unwrap()
    }

    #[test]
    fn matches_region_kind_since() {
        let q = NutrientQuery {
            region: RegionTag::new(1),
            kind: SporeKind::BiasNudge,
            since_ts: 100,
            max_count: 10,
            caller_tier: OptInTier::Public,
        };
        let s_match = mk(150, RegionTag::new(1), SporeKind::BiasNudge, OptInTier::Public);
        let s_old = mk(50, RegionTag::new(1), SporeKind::BiasNudge, OptInTier::Public);
        let s_other_region =
            mk(150, RegionTag::new(2), SporeKind::BiasNudge, OptInTier::Public);
        let s_other_kind =
            mk(150, RegionTag::new(1), SporeKind::CombatOutcome, OptInTier::Public);
        assert!(q.matches(&s_match));
        assert!(!q.matches(&s_old));
        assert!(!q.matches(&s_other_region));
        assert!(!q.matches(&s_other_kind));
    }

    #[test]
    fn response_helpers() {
        let r = NutrientResponse::empty(RegionTag::new(0), SporeKind::BiasNudge);
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn nutrient_query_serde_round_trip() {
        let q = NutrientQuery {
            region: RegionTag::new(11),
            kind: SporeKind::ProcgenSeed,
            since_ts: 9_999,
            max_count: 32,
            caller_tier: OptInTier::Anonymized,
        };
        let json = serde_json::to_string(&q).unwrap();
        let q2: NutrientQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(q, q2);
    }
}
