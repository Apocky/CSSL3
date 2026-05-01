//! § Gear — composed item-instance per GDD § AXIOMS.
//!
//! `gear ≡ (base + N×prefix + N×suffix + glyph-slots) modular-composition`
//!
//! Carries : slot · base · rarity · rolled-prefixes · rolled-suffixes ·
//! glyph-slots · item-level · bond-status · drop-seed.
//!
//! `merged_stats()` walks base + affixes and clamps to class-max.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::base::BaseItem;
use crate::glyph_slots::GlyphSlot;
use crate::rarity::Rarity;
use crate::slots::{GearSlot, StatKind};
use crate::stat_rolling::{clamp_to_class_max, RolledAffix};

// ───────────────────────────────────────────────────────────────────────
// § Gear
// ───────────────────────────────────────────────────────────────────────

/// Composed gear-instance. Drop-product OR craft-product OR transmute-product.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gear {
    /// Equipped-slot — must match `base.slot`.
    pub slot: GearSlot,
    /// Underlying base-item template.
    pub base: BaseItem,
    /// Rolled rarity-tier.
    pub rarity: Rarity,
    /// Rolled prefix-affixes (≤ 2 base-state ; ≤ 3 with Glyph-of-Inscription).
    pub prefixes: Vec<RolledAffix>,
    /// Rolled suffix-affixes.
    pub suffixes: Vec<RolledAffix>,
    /// Glyph-sockets (per-rarity count).
    pub glyph_slots: Vec<GlyphSlot>,
    /// Item-level. Scales with equipped-character-level on `level_up`.
    pub item_level: u32,
    /// Bond-state — true iff bonded to a player. Mythic = always-true post-equip.
    pub bound_to_player: bool,
    /// Drop-seed. Stable across the gear's lifetime ; reroll uses NEW seeds
    /// at the affix-slot level (preserved in `RolledAffix.seed`).
    pub seed: u128,
}

impl Gear {
    /// Build the merged stat-bag : base ⊕ Σ-affix-percents ⊕ class-max-clamp.
    ///
    /// Algorithm :
    ///   for each base-stat `(k, b)` :
    ///     accumulator[k] = b + sum-of-affixes-targeting-k
    ///   for each affix-stat outside base :
    ///     accumulator[stat] = affix-value (no base ; pass-through)
    ///   then clamp_to_class_max.
    #[must_use]
    pub fn merged_stats(&self) -> BTreeMap<StatKind, f32> {
        let mut acc: BTreeMap<StatKind, f32> = self.base.base_stats.clone();
        for r in self.prefixes.iter().chain(self.suffixes.iter()) {
            let k = r.descriptor.stat_kind;
            acc.entry(k).and_modify(|v| *v += r.value).or_insert(r.value);
        }
        clamp_to_class_max(&acc, self.base.item_class, &self.base.base_stats)
    }

    /// Total affix count (prefixes + suffixes ; glyphs separate).
    #[must_use]
    pub fn affix_count(&self) -> usize {
        self.prefixes.len() + self.suffixes.len()
    }

    /// Total filled glyph-slots.
    #[must_use]
    pub fn filled_glyph_count(&self) -> usize {
        self.glyph_slots.iter().filter(|g| g.is_filled()).count()
    }

    /// Display-name : `[prefix-name(s)] base-mat-class [suffix-name(s)]`.
    /// Stable + deterministic ; useful for audit-payloads and UI.
    #[must_use]
    pub fn display_name(&self) -> String {
        let mut parts = Vec::new();
        for p in &self.prefixes {
            if let Some(pre) = crate::stat_rolling::prefix_for_descriptor(&p.descriptor) {
                parts.push(pre.name().to_string());
            }
        }
        parts.push(format!(
            "{} {}",
            self.base.base_mat.name(),
            self.base.item_class.name()
        ));
        for s in &self.suffixes {
            if let Some(suf) = crate::stat_rolling::suffix_for_descriptor(&s.descriptor) {
                parts.push(suf.name().to_string());
            }
        }
        parts.join(" ")
    }
}
