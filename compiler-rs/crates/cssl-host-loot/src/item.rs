//! § item — `LootItem` produced by a roll
//!
//! `LootItem = (rarity, weapon-kind, vec<LootAffix>, season, drop_seed)`.
//!
//! Rarity is from [`cssl_host_gear_archetype::Rarity`]. The item carries a
//! `weapon_kind_code: u32` opaque-key (consumed by W13-2 weapons crate via
//! WeaponKind look-up) so this crate stays read-only on weapon details.
//!
//! **The struct does NOT carry stat-fields.** No `damage` / `accuracy` /
//! `reload_speed` field — the type-system makes `+10% damage` unrepresentable
//! at the LootItem level.

use cssl_host_gear_archetype::Rarity;
use serde::{Deserialize, Serialize};

use crate::affix::LootAffix;

/// Season-tag for time-based content-gates. Cosmetic-only — does not unlock
/// power. Stored as a small integer (S0 = bootstrap, S1+ = subsequent seasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LootSeason(pub u8);

impl LootSeason {
    /// Bootstrap season (S0).
    pub const BOOTSTRAP: LootSeason = LootSeason(0);
}

impl Default for LootSeason {
    fn default() -> Self {
        Self::BOOTSTRAP
    }
}

// ───────────────────────────────────────────────────────────────────────
// § LootItem
// ───────────────────────────────────────────────────────────────────────

/// Loot item produced by a drop-roll.
///
/// **No stat fields.** Damage / reload / accuracy live on the WeaponKind
/// consumed via `weapon_kind_code` ; per the COSMETIC-ONLY-AXIOM, those
/// stats are identical across rarities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootItem {
    /// Rarity tier. Comes from the [`crate::roll::sample_rarity_with_bias`] call.
    pub rarity: Rarity,
    /// Opaque WeaponKind code (W13-2 weapons crate consumer).
    /// Shape kept as `u32` here so this crate has zero dependency on weapon
    /// internals.
    pub weapon_kind_code: u32,
    /// Cosmetic affix bag. Length bounded in practice (≤16) by the rarity's
    /// affix-count band but NOT structurally enforced here — see
    /// [`crate::roll`] for population.
    pub affixes: Vec<LootAffix>,
    /// Season-tag at drop-time.
    pub season: LootSeason,
    /// Seed that produced this item (for replay + Σ-Chain anchor).
    pub drop_seed: u128,
}

impl LootItem {
    /// Construct a new LootItem. The caller is responsible for generating
    /// `affixes` via the [`crate::roll`] pipeline (which respects the
    /// COSMETIC-ONLY-AXIOM by construction).
    #[must_use]
    pub fn new(rarity: Rarity, weapon_kind_code: u32, affixes: Vec<LootAffix>, season: LootSeason, drop_seed: u128) -> Self {
        Self { rarity, weapon_kind_code, affixes, season, drop_seed }
    }

    /// Number of affixes on this item.
    #[must_use]
    pub fn affix_count(&self) -> usize {
        self.affixes.len()
    }

    /// Canonical bytes for Σ-Chain payload.
    ///
    /// Format : rarity-tag · weapon-kind-code(u32-le) · season(u8) ·
    ///          drop_seed(u128-le) · affix-count(u32-le) · affix*canonical
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(self.rarity.name().as_bytes());
        out.push(0); // null-terminator separates rarity-tag from binary-fields
        out.extend_from_slice(&self.weapon_kind_code.to_le_bytes());
        out.push(self.season.0);
        out.extend_from_slice(&self.drop_seed.to_le_bytes());
        let n = u32::try_from(self.affixes.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&n.to_le_bytes());
        for a in &self.affixes {
            out.extend_from_slice(&a.canonical_bytes());
        }
        out
    }
}
