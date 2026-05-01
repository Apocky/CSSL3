//! § BaseItem — pre-affix template per GDD § BASE-COMPONENT.
//!
//! `ItemClass` partitions into Weapon / Armor / Jewelry / Trinket. Each class
//! has a canonical root-stat-set (per GDD § BASE-STATS).
//!
//! `BaseMat` aligns with `cssl-host-craft-graph` METALS pool : Iron · Silver
//! · Mithril · Adamant · Voidsteel · Soulbound. Each mat sets a rarity-floor :
//! rolled-rarity ≥ floor (per GDD § BASE-MATERIAL).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::rarity::Rarity;
use crate::slots::{GearSlot, StatKind};

// ───────────────────────────────────────────────────────────────────────
// § ItemClass
// ───────────────────────────────────────────────────────────────────────

/// Four-class partition. Each class has its own root-stat-set and class-max-clamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ItemClass {
    /// Weapons : MainHand / OffHand. Damage-bearing.
    Weapon,
    /// Armors : Helm / Chest / Pants / Boots / Gloves / Belt / Cape. Defense-bearing.
    Armor,
    /// Jewelry : RingA / RingB / Amulet. Mana / cooldown / affinity.
    Jewelry,
    /// Trinket : Trinket. Utility-effect carrier.
    Trinket,
}

impl ItemClass {
    /// Stable name for audit-event payloads.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            ItemClass::Weapon => "weapon",
            ItemClass::Armor => "armor",
            ItemClass::Jewelry => "jewelry",
            ItemClass::Trinket => "trinket",
        }
    }

    /// Class-max-clamp ceiling per stat. Per GDD § STAT-ROLLING : final-stat
    /// = base × (1 + Σ-affix-percents) clamped-to-class-max. Anti-power-creep
    /// invariant : ≤ 1.50× base (cf. cssl-host-craft-graph § INVARIANT).
    #[must_use]
    pub const fn class_max_multiplier(self) -> f32 {
        // 1.50× cap is universal anti-power-creep ceiling per CRAFT_DECONSTRUCT_ALCHEMY.
        // Future-extension : per-class differential caps go here.
        match self {
            ItemClass::Weapon | ItemClass::Armor | ItemClass::Jewelry | ItemClass::Trinket => 1.50,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § BaseMat — aligned with cssl-host-craft-graph METALS pool
// ───────────────────────────────────────────────────────────────────────

/// Six-tier base-material. Sets rarity-floor : rolled-rarity ≥ `rarity_floor()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BaseMat {
    /// T1 Iron — rarity-floor Common.
    Iron,
    /// T2 Silver — rarity-floor Uncommon.
    Silver,
    /// T3 Mithril — rarity-floor Rare.
    Mithril,
    /// T4 Adamant — rarity-floor Epic.
    Adamant,
    /// T5 Voidsteel — rarity-floor Legendary.
    Voidsteel,
    /// T6 Soulbound — rarity-floor Mythic ; binds-to-character-on-equip.
    Soulbound,
}

impl BaseMat {
    /// Per-mat rarity-floor. Rolled-rarity must be ≥ this. Never downgrade.
    #[must_use]
    pub const fn rarity_floor(self) -> Rarity {
        match self {
            BaseMat::Iron => Rarity::Common,
            BaseMat::Silver => Rarity::Uncommon,
            BaseMat::Mithril => Rarity::Rare,
            BaseMat::Adamant => Rarity::Epic,
            BaseMat::Voidsteel => Rarity::Legendary,
            BaseMat::Soulbound => Rarity::Mythic,
        }
    }

    /// Stable name for audit + serde-key.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            BaseMat::Iron => "iron",
            BaseMat::Silver => "silver",
            BaseMat::Mithril => "mithril",
            BaseMat::Adamant => "adamant",
            BaseMat::Voidsteel => "voidsteel",
            BaseMat::Soulbound => "soulbound",
        }
    }

    /// All six mats in canonical tier-order.
    #[must_use]
    pub const fn all() -> [BaseMat; 6] {
        [
            BaseMat::Iron,
            BaseMat::Silver,
            BaseMat::Mithril,
            BaseMat::Adamant,
            BaseMat::Voidsteel,
            BaseMat::Soulbound,
        ]
    }
}

// ───────────────────────────────────────────────────────────────────────
// § BaseItem
// ───────────────────────────────────────────────────────────────────────

/// Pre-affix template. Slot + class + mat + base-stat-bag + affix-allowance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseItem {
    /// Equipped-slot.
    pub slot: GearSlot,
    /// Item-class (Weapon / Armor / Jewelry / Trinket).
    pub item_class: ItemClass,
    /// Base material — sets rarity-floor.
    pub base_mat: BaseMat,
    /// Root-stat-bag : per-class canonical stats (BTreeMap for determinism).
    pub base_stats: BTreeMap<StatKind, f32>,
    /// Maximum total affixes allowed (prefixes + suffixes ; glyphs separate).
    /// Default 4 (≤2P + ≤2S) ; ≤7 with Glyph-of-Inscription per GDD.
    pub allowed_affixes: u8,
}

impl BaseItem {
    /// Construct a Weapon base. `damage` + `attack_speed` populated as roots.
    /// Slot must be MainHand or OffHand (not enforced here ; caller-discipline).
    #[must_use]
    pub fn weapon(slot: GearSlot, mat: BaseMat, damage: f32, attack_speed: f32) -> Self {
        let mut base_stats = BTreeMap::new();
        base_stats.insert(StatKind::Damage, damage);
        base_stats.insert(StatKind::AttackSpeed, attack_speed);
        base_stats.insert(StatKind::Range, 1.5);
        base_stats.insert(StatKind::StaminaCost, 10.0);
        base_stats.insert(StatKind::CritChance, 0.05);
        Self {
            slot,
            item_class: ItemClass::Weapon,
            base_mat: mat,
            base_stats,
            allowed_affixes: 4,
        }
    }

    /// Construct an Armor base.
    #[must_use]
    pub fn armor(slot: GearSlot, mat: BaseMat, armor_rating: f32) -> Self {
        let mut base_stats = BTreeMap::new();
        base_stats.insert(StatKind::ArmorRating, armor_rating);
        base_stats.insert(StatKind::PhysicalResist, 0.05);
        base_stats.insert(StatKind::ElementalResist, 0.02);
        base_stats.insert(StatKind::Weight, 5.0);
        Self {
            slot,
            item_class: ItemClass::Armor,
            base_mat: mat,
            base_stats,
            allowed_affixes: 4,
        }
    }

    /// Construct a Jewelry base.
    #[must_use]
    pub fn jewelry(slot: GearSlot, mat: BaseMat, mana_pool: f32) -> Self {
        let mut base_stats = BTreeMap::new();
        base_stats.insert(StatKind::ManaPool, mana_pool);
        base_stats.insert(StatKind::CooldownReduction, 0.0);
        base_stats.insert(StatKind::AffinityChannel, 0.0);
        Self {
            slot,
            item_class: ItemClass::Jewelry,
            base_mat: mat,
            base_stats,
            allowed_affixes: 4,
        }
    }

    /// Construct a Trinket base.
    #[must_use]
    pub fn trinket(slot: GearSlot, mat: BaseMat, charges: f32) -> Self {
        let mut base_stats = BTreeMap::new();
        base_stats.insert(StatKind::UseCharges, charges);
        base_stats.insert(StatKind::Cooldown, 30.0);
        base_stats.insert(StatKind::UtilityEffect, 0.0);
        Self {
            slot,
            item_class: ItemClass::Trinket,
            base_mat: mat,
            base_stats,
            allowed_affixes: 4,
        }
    }
}
