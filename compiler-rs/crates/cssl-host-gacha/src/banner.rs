// § banner.rs — Banner schema + Rarity + DropRateTable
// ════════════════════════════════════════════════════════════════════
// § PUBLIC-DISCLOSURE : Banner.disclosed_drop_rate_pct() returns a
//   serializable BTreeMap<Rarity, Decimal-pct-string> for direct
//   transmission to the public banner-detail endpoint (transparency).
// § INVARIANT : sum(rate-bps over rarities) == 100_000 ; constructor
//   rejects any DropRateTable whose sum diverges.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::canonical_drop_rates::TOTAL_BPS;

/// § Rarity — six-tier ladder · ordered by ascending-rarity (Common = lowest).
///
/// Ordering matters for cumulative-probability roll-out in `pull::roll`.
/// `Mythic` is highest-rarity ; pity-system guarantees one within 90 pulls.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
    Mythic,
}

impl Rarity {
    /// All six rarities in ascending-rarity order. Stable iteration order ⇒
    /// reproducible roll-outs across hosts.
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::Common,
            Self::Uncommon,
            Self::Rare,
            Self::Epic,
            Self::Legendary,
            Self::Mythic,
        ]
    }

    /// Stable display-string (snake-case · matches serde rename).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
            Self::Legendary => "legendary",
            Self::Mythic => "mythic",
        }
    }
}

/// § DropRateTable — basis-points (1 bp = 0.01% · 10_000 bps = 100.0%) per rarity.
///
/// We use bps × 1000 (totals to 100_000) for sub-bp precision · a Mythic
/// tier of 100 bps = 0.1% is the rarest drop. Sum MUST equal TOTAL_BPS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropRateTable {
    /// BTreeMap-keyed for deterministic-iteration order.
    pub rates_bps: BTreeMap<Rarity, u32>,
}

impl DropRateTable {
    /// Construct a canonical six-tier table from the constants in `canonical_drop_rates`.
    #[must_use]
    pub fn canonical() -> Self {
        use crate::canonical_drop_rates as r;
        let mut rates = BTreeMap::new();
        rates.insert(Rarity::Common, r::COMMON_BPS);
        rates.insert(Rarity::Uncommon, r::UNCOMMON_BPS);
        rates.insert(Rarity::Rare, r::RARE_BPS);
        rates.insert(Rarity::Epic, r::EPIC_BPS);
        rates.insert(Rarity::Legendary, r::LEGENDARY_BPS);
        rates.insert(Rarity::Mythic, r::MYTHIC_BPS);
        Self { rates_bps: rates }
    }

    /// Validate sum-equals-TOTAL_BPS invariant. Returns BannerErr::SumDivergent
    /// when the table cannot represent a valid probability distribution.
    pub fn validate(&self) -> Result<(), BannerErr> {
        let sum: u64 = self.rates_bps.values().map(|&b| u64::from(b)).sum();
        if sum != u64::from(TOTAL_BPS) {
            return Err(BannerErr::SumDivergent {
                actual: sum,
                expected: u64::from(TOTAL_BPS),
            });
        }
        // Also ensure all six rarities present (else cumulative-probability
        // walk would be ambiguous).
        for r in Rarity::all() {
            if !self.rates_bps.contains_key(&r) {
                return Err(BannerErr::MissingRarity(r));
            }
        }
        Ok(())
    }

    /// Cumulative basis-point thresholds for rarity-roll. Returns ascending
    /// sequence : [common_cum, uncommon_cum, rare_cum, epic_cum, legendary_cum, mythic_cum=TOTAL_BPS]
    /// such that a u32-roll < N is in-rarity-N's-band.
    #[must_use]
    pub fn cumulative_thresholds(&self) -> [u32; 6] {
        let mut t = [0u32; 6];
        let mut acc: u32 = 0;
        for (i, r) in Rarity::all().iter().enumerate() {
            acc = acc.saturating_add(*self.rates_bps.get(r).unwrap_or(&0));
            t[i] = acc;
        }
        t
    }

    /// Public-disclosure view — rarity-name → "X.XX%" string. Used by the
    /// `/api/gacha/banners/:id` transparency endpoint.
    #[must_use]
    pub fn disclosed_drop_rate_pct(&self) -> BTreeMap<&'static str, String> {
        let mut out = BTreeMap::new();
        for r in Rarity::all() {
            let bps = self.rates_bps.get(&r).copied().unwrap_or(0);
            // bps is out-of-100_000 ⇒ pct = bps / 1000.0
            let whole = bps / 1000;
            let frac = bps % 1000;
            out.insert(r.as_str(), format!("{whole}.{frac:03}%"));
        }
        out
    }
}

/// § Banner — a published gacha-banner. `id` is opaque · `season` ties to
/// `gacha_banners.season` SQL column · `disclosed_at` MUST be set BEFORE
/// any pull is allowed (transparency-precondition).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Banner {
    pub id: String,
    pub season: u32,
    pub drop_rates: DropRateTable,
    /// Pity threshold in pulls — must equal the canonical PITY_THRESHOLD constant.
    pub pity_threshold: u32,
    /// ISO-8601 timestamp when the drop-rates were publicly disclosed.
    /// `None` ⇒ banner not yet eligible to-pull.
    pub disclosed_at: Option<String>,
}

impl Banner {
    /// Construct a freshly-disclosed canonical banner. Returns `BannerErr::*`
    /// if the drop-rates fail the sum-invariant.
    pub fn canonical(id: String, season: u32, disclosed_at: String) -> Result<Self, BannerErr> {
        use crate::pity::PITY_THRESHOLD;
        let rates = DropRateTable::canonical();
        rates.validate()?;
        Ok(Self {
            id,
            season,
            drop_rates: rates,
            pity_threshold: PITY_THRESHOLD,
            disclosed_at: Some(disclosed_at),
        })
    }

    /// Predicate : may a player pull this banner? Disclosure-timestamp MUST
    /// be set (transparency-precondition · structural enforcement).
    #[must_use]
    pub fn is_pullable(&self) -> bool {
        self.disclosed_at.is_some()
    }
}

/// § BannerErr — public error-enum.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BannerErr {
    #[error("drop-rate sum divergent : actual={actual} expected={expected} bps")]
    SumDivergent { actual: u64, expected: u64 },
    #[error("missing rarity in drop-rate table : {0:?}")]
    MissingRarity(Rarity),
    #[error("banner not yet disclosed (transparency-precondition unmet)")]
    NotDisclosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_table_sum_is_100pct() {
        let t = DropRateTable::canonical();
        assert!(t.validate().is_ok());
    }

    #[test]
    fn cumulative_thresholds_terminate_at_total() {
        let t = DropRateTable::canonical();
        let cum = t.cumulative_thresholds();
        assert_eq!(cum[5], TOTAL_BPS);
        // strictly-monotonic-non-decreasing
        for i in 1..6 {
            assert!(cum[i] >= cum[i - 1]);
        }
    }

    #[test]
    fn disclosed_drop_rate_format_pct_strings() {
        let t = DropRateTable::canonical();
        let d = t.disclosed_drop_rate_pct();
        // mythic = 100 bps = 0.100%
        assert_eq!(d.get("mythic").unwrap(), "0.100%");
        // common = 60_000 bps = 60.000%
        assert_eq!(d.get("common").unwrap(), "60.000%");
    }

    #[test]
    fn divergent_table_rejects() {
        let mut t = DropRateTable::canonical();
        // Bump common by 1 bp → sum != TOTAL_BPS
        *t.rates_bps.get_mut(&Rarity::Common).unwrap() += 1;
        let err = t.validate().unwrap_err();
        assert!(matches!(err, BannerErr::SumDivergent { .. }));
    }

    #[test]
    fn missing_rarity_rejects() {
        let mut t = DropRateTable::canonical();
        t.rates_bps.remove(&Rarity::Mythic);
        // After removing mythic, sum is < TOTAL_BPS · so SumDivergent fires first.
        assert!(t.validate().is_err());
    }

    #[test]
    fn banner_not_pullable_until_disclosed() {
        let mut b = Banner::canonical("alpha".into(), 1, "2026-05-01T00:00:00Z".into()).unwrap();
        assert!(b.is_pullable());
        b.disclosed_at = None;
        assert!(!b.is_pullable());
    }

    #[test]
    fn rarity_ord_is_ascending_rarity() {
        assert!(Rarity::Common < Rarity::Mythic);
        assert!(Rarity::Epic < Rarity::Legendary);
    }
}
