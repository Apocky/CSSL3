//! § distribution — public 6-tier drop-rate curve
//!
//! Per W13-8 spec, drop-rates are PUBLIC (not hidden). The curve is :
//!
//! | Rarity      | Rate    |
//! |-------------|---------|
//! | Common      | 0.60    |
//! | Uncommon    | 0.25    |
//! | Rare        | 0.10    |
//! | Epic        | 0.04    |
//! | Legendary   | 0.009   |
//! | Mythic      | 0.001   |
//!
//! Sums to 1.0. Index order matches [`cssl_host_gear_archetype::Rarity::all`]
//! so distribution-vec aligns with rarity-vec.

use cssl_host_gear_archetype::Rarity;
use serde::{Deserialize, Serialize};

/// Public per-rarity drop-rates per W13-8. Sums to 1.0 (modulo f32-precision).
///
/// Index order : `[Common, Uncommon, Rare, Epic, Legendary, Mythic]` matching
/// [`Rarity::all`].
pub const PUBLIC_DROP_RATES: [f32; 6] = [0.60, 0.25, 0.10, 0.04, 0.009, 0.001];

// ───────────────────────────────────────────────────────────────────────
// § DropRateDistribution
// ───────────────────────────────────────────────────────────────────────

/// Per-rarity drop-rate distribution. The default and only canonical
/// distribution is [`DropRateDistribution::PUBLIC`] which exposes the rates
/// listed above. KAN-bias modulation is applied **on top** of this base curve
/// (see [`crate::roll`]) — it never replaces the publicly-disclosed rates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DropRateDistribution {
    /// Per-rarity rates indexed by [`Rarity::all`] order.
    pub rates: [f32; 6],
}

impl DropRateDistribution {
    /// Public canonical distribution per W13-8 spec.
    pub const PUBLIC: DropRateDistribution = DropRateDistribution { rates: PUBLIC_DROP_RATES };

    /// Returns the drop-rate for the given rarity.
    #[must_use]
    pub fn rate(&self, r: Rarity) -> f32 {
        self.rates[Self::index_of(r)]
    }

    /// Sum of all rates ; should always be ~1.0 for canonical distributions.
    #[must_use]
    pub fn total(&self) -> f32 {
        self.rates.iter().sum()
    }

    /// True iff distribution is normalized (sum ∈ [0.999, 1.001]).
    #[must_use]
    pub fn is_normalized(&self) -> bool {
        let t = self.total();
        (0.999..=1.001).contains(&t)
    }

    /// Index of `r` in the canonical `[Common..Mythic]` ordering.
    #[must_use]
    pub fn index_of(r: Rarity) -> usize {
        Rarity::all().iter().position(|&x| x == r).unwrap_or(0)
    }

    /// Renormalize (in-place) so rates sum to 1.0. No-op if already-normalized.
    /// If sum is 0.0 (degenerate input) returns the PUBLIC distribution.
    #[must_use]
    pub fn renormalized(self) -> Self {
        let t = self.total();
        if t <= 0.0 {
            return Self::PUBLIC;
        }
        let mut rates = self.rates;
        for v in &mut rates {
            *v /= t;
        }
        Self { rates }
    }
}

impl Default for DropRateDistribution {
    fn default() -> Self {
        Self::PUBLIC
    }
}
