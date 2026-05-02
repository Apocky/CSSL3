//! § distribution — public 8-tier drop-rate curve (Q-06 Apocky-canonical 2026-05-01)
//!
//! Per Q-06 spec, drop-rates are PUBLIC (not hidden). The 8-tier curve is :
//!
//! | Rarity      | Rate     |
//! |-------------|----------|
//! | Common      | 0.60     |
//! | Uncommon    | 0.25     |
//! | Rare        | 0.10     |
//! | Epic        | 0.04     |
//! | Legendary   | 0.009    |
//! | Mythic      | 0.0009   |
//! | Prismatic   | 0.00009  |
//! | Chaotic     | 0.00001  |
//!
//! Sums to 1.0 (within f32 1e-4 tolerance). Index order matches
//! [`cssl_host_gear_archetype::Rarity::all`] so distribution-vec aligns with
//! rarity-vec.

use cssl_host_gear_archetype::Rarity;
use serde::{Deserialize, Serialize};

/// Public per-rarity drop-rates per Q-06 (Apocky 2026-05-01). Sums to 1.0
/// (modulo f32-precision).
///
/// Index order : `[Common, Uncommon, Rare, Epic, Legendary, Mythic, Prismatic, Chaotic]`
/// matching [`Rarity::all`].
pub const PUBLIC_DROP_RATES: [f32; 8] = [
    0.60,    // Common    60.000%
    0.25,    // Uncommon  25.000%
    0.10,    // Rare      10.000%
    0.04,    // Epic       4.000%
    0.009,   // Legendary  0.900%
    0.0009,  // Mythic     0.090%  (Q-06)
    0.00009, // Prismatic  0.009%  (Q-06 NEW)
    0.00001, // Chaotic    0.001%  (Q-06 NEW)
];

// ───────────────────────────────────────────────────────────────────────
// § DropRateDistribution (Q-06 8-tier)
// ───────────────────────────────────────────────────────────────────────

/// Per-rarity drop-rate distribution. The default and only canonical
/// distribution is [`DropRateDistribution::PUBLIC`] which exposes the rates
/// listed above. KAN-bias modulation is applied **on top** of this base curve
/// (see [`crate::roll`]) — it never replaces the publicly-disclosed rates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DropRateDistribution {
    /// Per-rarity rates indexed by [`Rarity::all`] order (8-tier per Q-06).
    pub rates: [f32; 8],
}

impl DropRateDistribution {
    /// Public canonical distribution per Q-06 spec (8-tier).
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
