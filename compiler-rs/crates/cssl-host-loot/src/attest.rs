//! § attest — `attest_no_pay_for_power` structural attestation
//!
//! The COSMETIC-ONLY-AXIOM is **structurally** enforced by the [`crate::affix::LootAffix`]
//! sum-type having no stat-modifying variant. This module's job is to make
//! that fact explicit + audit-friendly.
//!
//! [`attest_no_pay_for_power`] is total — it returns `true` for every
//! [`crate::LootItem`] this crate can construct. The exhaustive `match` on
//! [`AffixCategory`] guarantees that if a future spec-update adds a `StatBuff`
//! category to the enum, this function will fail to compile (the compile-error
//! catches the violation **before** any stat-affecting item ships).
//!
//! For the rare error-paths (deserialized-bad-data, etc.) we return a typed
//! [`PayForPowerError`] explaining the violation — but no shipped item ever
//! produces one.

use crate::affix::{AffixCategory, LootAffix};
use crate::item::LootItem;

// ───────────────────────────────────────────────────────────────────────
// § PayForPowerError
// ───────────────────────────────────────────────────────────────────────

/// A reason a [`LootItem`] failed the pay-for-power attestation.
///
/// **No shipped LootItem produces any of these variants.** They exist purely
/// for diagnosis when ingesting external (e.g. modder, rogue-server) data
/// that might otherwise smuggle in a stat-modifying affix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayForPowerError {
    /// Affix category was outside the canonical four (Visual / Audio / Particle / Attribution).
    /// In current builds, unreachable — the type-system already forbids it.
    NonCosmeticCategory(AffixCategory),
    /// Item carried more affixes than the rarity's band permits.
    /// (Soft-cap = 16 ; hard-rejected above.)
    AffixCountOverflow {
        /// Reported affix-count.
        count: usize,
        /// Hard-cap.
        cap: usize,
    },
}

impl core::fmt::Display for PayForPowerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PayForPowerError::NonCosmeticCategory(c) => {
                write!(f, "non-cosmetic affix-category: {}", c.name())
            }
            PayForPowerError::AffixCountOverflow { count, cap } => {
                write!(f, "affix-count {count} exceeds hard-cap {cap}")
            }
        }
    }
}

impl std::error::Error for PayForPowerError {}

/// Hard-cap on affix-count per item — defensive against ingest of malformed
/// external data. Shipped items use ≤8 (Mythic ceiling).
pub const AFFIX_COUNT_HARD_CAP: usize = 16;

// ───────────────────────────────────────────────────────────────────────
// § attest_no_pay_for_power
// ───────────────────────────────────────────────────────────────────────

/// Returns `true` iff `item` honors the COSMETIC-ONLY-AXIOM.
///
/// **By construction** every [`LootItem`] produced by [`crate::roll`]
/// satisfies this — the affix-bag carries only [`LootAffix::Visual`],
/// [`LootAffix::Audio`], [`LootAffix::Particle`], [`LootAffix::Attribution`]
/// variants, all of which are cosmetic-categorized. The exhaustive match
/// on [`AffixCategory`] in this function will block compilation if a
/// stat-modifying category is ever added.
///
/// For external data ingest, [`attest_no_pay_for_power_strict`] returns
/// `Result<(), PayForPowerError>` with the specific violation.
#[must_use]
pub fn attest_no_pay_for_power(item: &LootItem) -> bool {
    attest_no_pay_for_power_strict(item).is_ok()
}

/// Strict variant — returns the specific violation when `item` fails.
///
/// # Errors
/// - [`PayForPowerError::NonCosmeticCategory`] if a non-cosmetic category is encountered
///   (currently unreachable — type-system blocks this at the variant level).
/// - [`PayForPowerError::AffixCountOverflow`] if the affix-bag exceeds [`AFFIX_COUNT_HARD_CAP`].
pub fn attest_no_pay_for_power_strict(item: &LootItem) -> Result<(), PayForPowerError> {
    if item.affix_count() > AFFIX_COUNT_HARD_CAP {
        return Err(PayForPowerError::AffixCountOverflow {
            count: item.affix_count(),
            cap: AFFIX_COUNT_HARD_CAP,
        });
    }
    for a in &item.affixes {
        // Exhaustive match — adding a stat-modifying category would force this
        // function to fail compile, which is the structural enforcement.
        let cat = a.category();
        match cat {
            AffixCategory::Visual
            | AffixCategory::Audio
            | AffixCategory::Particle
            | AffixCategory::Attribution => {
                // ✓ cosmetic — proceed
            }
        }
        // Per-variant defensive check : payloads carry only opaque IDs / colors /
        // strings — no `damage` / `reload_speed` / `accuracy` fields exist on
        // the variant types. This is a tautology given the type definitions
        // but makes the invariant audit-explicit.
        match a {
            LootAffix::Visual(_) | LootAffix::Audio(_) | LootAffix::Particle(_) | LootAffix::Attribution(_) => {
                // ✓ all four sub-types are cosmetic by construction
            }
        }
    }
    Ok(())
}

/// Convenience — attest the entire batch produced by a multi-drop roll.
/// Returns `Ok(())` iff every item passes ; on first failure returns the
/// failing index + the specific error.
///
/// # Errors
/// - `(usize, PayForPowerError)` — the index of the first failing item plus the
///   specific [`PayForPowerError`] explaining why.
pub fn attest_batch(items: &[LootItem]) -> Result<(), (usize, PayForPowerError)> {
    for (i, item) in items.iter().enumerate() {
        if let Err(e) = attest_no_pay_for_power_strict(item) {
            return Err((i, e));
        }
    }
    Ok(())
}
