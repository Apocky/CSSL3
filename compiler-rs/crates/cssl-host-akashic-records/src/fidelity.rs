// § fidelity.rs · 5-tier FidelityTier · shard-cost-config (¬ hardcoded top-level)
// § per spec/18 § THE TIERS (free vs premium · cosmetic-channel-only)

use serde::{Deserialize, Serialize};

/// 5-tier fidelity · per spec/18 § THE TIERS.
///
/// - `Basic` (FREE) : event-class + date + brief-summary · always-available
/// - `HighFidelity` (50-200 shards) : 16-band-spectral-rendered scene-snapshot
/// - `Commissioned` (200-500 shards) : GM-narrated permanent-flavor-text
/// - `EternalAttribution` (1000 shards · ONE-TIME-per author@scene)
/// - `HistoricalReconstructionTour` (50 shards · 30-min TTL token)
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum FidelityTier {
    Basic = 0,
    HighFidelity = 1,
    Commissioned = 2,
    EternalAttribution = 3,
    HistoricalReconstructionTour = 4,
}

impl FidelityTier {
    /// Iteration-order over all 5 tiers (stable for tests).
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::Basic,
            Self::HighFidelity,
            Self::Commissioned,
            Self::EternalAttribution,
            Self::HistoricalReconstructionTour,
        ]
    }

    /// `true` iff tier is free-of-charge (Basic only).
    #[must_use]
    pub fn is_free(self) -> bool {
        matches!(self, Self::Basic)
    }

    /// `true` iff this tier permanently-burns shards-once-per-author-per-scene
    /// and CAN-NEVER be revoked (eternal-attribution axiom).
    #[must_use]
    pub fn is_eternal(self) -> bool {
        matches!(self, Self::EternalAttribution)
    }

    /// `true` iff this tier issues a TTL token (historical-reconstruction tour).
    #[must_use]
    pub fn issues_ttl_token(self) -> bool {
        matches!(self, Self::HistoricalReconstructionTour)
    }
}

/// Shard-cost configuration · passed-in (NOT hardcoded at top-level).
///
/// Per spec/18 lines 58-68 and landmine "shard-cost-config : do NOT hardcode".
/// Default values match the spec ranges' lower-bounds; integrators MAY tune.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShardCostConfig {
    /// Cost for `HighFidelity` (spec range 50-200).
    pub high_fidelity: u32,
    /// Cost for `Commissioned` (spec range 200-500).
    pub commissioned: u32,
    /// Cost for `EternalAttribution` (spec : 1000, one-time).
    pub eternal_attribution: u32,
    /// Cost for `HistoricalReconstructionTour` (spec : 50).
    pub historical_tour: u32,
    /// TTL for historical-tour tokens, in seconds (spec : 30 min = 1800 s).
    pub historical_tour_ttl_secs: u64,
}

impl Default for ShardCostConfig {
    fn default() -> Self {
        Self {
            high_fidelity: 50,
            commissioned: 200,
            eternal_attribution: 1000,
            historical_tour: 50,
            historical_tour_ttl_secs: 30 * 60,
        }
    }
}

impl ShardCostConfig {
    /// Resolve shard-cost for a given tier · returns `0` for Basic (FREE).
    #[must_use]
    pub fn cost_for(&self, tier: FidelityTier) -> u32 {
        match tier {
            FidelityTier::Basic => 0,
            FidelityTier::HighFidelity => self.high_fidelity,
            FidelityTier::Commissioned => self.commissioned,
            FidelityTier::EternalAttribution => self.eternal_attribution,
            FidelityTier::HistoricalReconstructionTour => self.historical_tour,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fidelity_basic_is_free() {
        assert!(FidelityTier::Basic.is_free());
        assert!(!FidelityTier::HighFidelity.is_free());
        assert!(!FidelityTier::Commissioned.is_free());
        assert!(!FidelityTier::EternalAttribution.is_free());
        assert!(!FidelityTier::HistoricalReconstructionTour.is_free());
    }

    #[test]
    fn fidelity_eternal_only_one_is_eternal() {
        assert!(FidelityTier::EternalAttribution.is_eternal());
        for t in FidelityTier::all() {
            if t != FidelityTier::EternalAttribution {
                assert!(!t.is_eternal(), "tier {t:?} must NOT be eternal");
            }
        }
    }

    #[test]
    fn fidelity_only_tour_issues_ttl() {
        assert!(FidelityTier::HistoricalReconstructionTour.issues_ttl_token());
        for t in FidelityTier::all() {
            if t != FidelityTier::HistoricalReconstructionTour {
                assert!(!t.issues_ttl_token());
            }
        }
    }

    #[test]
    fn fidelity_all_5_tiers_distinct() {
        let all = FidelityTier::all();
        // count distinct discriminants
        let mut seen = std::collections::BTreeSet::new();
        for t in all {
            seen.insert(t as u8);
        }
        assert_eq!(seen.len(), 5);
    }

    #[test]
    fn fidelity_default_config_matches_spec_lower_bounds() {
        let c = ShardCostConfig::default();
        assert_eq!(c.cost_for(FidelityTier::Basic), 0);
        assert_eq!(c.cost_for(FidelityTier::HighFidelity), 50);
        assert_eq!(c.cost_for(FidelityTier::Commissioned), 200);
        assert_eq!(c.cost_for(FidelityTier::EternalAttribution), 1000);
        assert_eq!(c.cost_for(FidelityTier::HistoricalReconstructionTour), 50);
        assert_eq!(c.historical_tour_ttl_secs, 30 * 60);
    }

    #[test]
    fn fidelity_config_tunable() {
        let c = ShardCostConfig {
            high_fidelity: 200,
            commissioned: 500,
            ..Default::default()
        };
        assert_eq!(c.cost_for(FidelityTier::HighFidelity), 200);
        assert_eq!(c.cost_for(FidelityTier::Commissioned), 500);
    }

    #[test]
    fn fidelity_repr_u8_stable() {
        assert_eq!(FidelityTier::Basic as u8, 0);
        assert_eq!(FidelityTier::HighFidelity as u8, 1);
        assert_eq!(FidelityTier::Commissioned as u8, 2);
        assert_eq!(FidelityTier::EternalAttribution as u8, 3);
        assert_eq!(FidelityTier::HistoricalReconstructionTour as u8, 4);
    }
}
