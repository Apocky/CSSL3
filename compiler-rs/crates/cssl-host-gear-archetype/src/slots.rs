//! § GearSlot — 13 distinct equipped-positions per GDD § GEAR-SLOTS.
//!
//! Helm · Chest · Pants · Boots · Gloves · Belt · Cape ·
//! MainHand · OffHand · RingA · RingB · Amulet · Trinket
//!
//! Spec invariants :
//!   - 13 total ; RingA + RingB stack-allowed (mutex check NOT enforced here).
//!   - OffHand is mutex-with two-handed MainHand (enforced at equip-time, not here).
//!   - clamp-to-class-max applied at equipped-set-merge (see `stat_rolling`).
//!
//! `StatKind` lives here (cross-slot-cross-class stat-vocabulary) so that
//! `BaseItem`, `AffixDescriptor`, and `Gear` all reference the same enum.

use serde::{Deserialize, Serialize};

// ───────────────────────────────────────────────────────────────────────
// § GearSlot
// ───────────────────────────────────────────────────────────────────────

/// 13 equipped-positions. Stable discriminants for serde-replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GearSlot {
    /// Head-armor slot.
    Helm,
    /// Torso-armor slot.
    Chest,
    /// Leg-armor slot.
    Pants,
    /// Foot-armor slot.
    Boots,
    /// Hand-armor slot.
    Gloves,
    /// Waist slot.
    Belt,
    /// Back-armor slot.
    Cape,
    /// Primary weapon slot. Two-handed weapons mutex with `OffHand`.
    MainHand,
    /// Secondary weapon / shield slot. Mutex with two-handed `MainHand`.
    OffHand,
    /// First ring slot. Stacks with `RingB`.
    RingA,
    /// Second ring slot. Stacks with `RingA`.
    RingB,
    /// Neck slot.
    Amulet,
    /// Misc slot (consumable-charge holder, utility-effect carrier).
    Trinket,
}

impl GearSlot {
    /// All 13 slots in canonical declaration order. Stable iteration for tests.
    #[must_use]
    pub const fn all() -> [GearSlot; 13] {
        [
            GearSlot::Helm,
            GearSlot::Chest,
            GearSlot::Pants,
            GearSlot::Boots,
            GearSlot::Gloves,
            GearSlot::Belt,
            GearSlot::Cape,
            GearSlot::MainHand,
            GearSlot::OffHand,
            GearSlot::RingA,
            GearSlot::RingB,
            GearSlot::Amulet,
            GearSlot::Trinket,
        ]
    }

    /// Stable name for audit-event payloads.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            GearSlot::Helm => "helm",
            GearSlot::Chest => "chest",
            GearSlot::Pants => "pants",
            GearSlot::Boots => "boots",
            GearSlot::Gloves => "gloves",
            GearSlot::Belt => "belt",
            GearSlot::Cape => "cape",
            GearSlot::MainHand => "main_hand",
            GearSlot::OffHand => "off_hand",
            GearSlot::RingA => "ring_a",
            GearSlot::RingB => "ring_b",
            GearSlot::Amulet => "amulet",
            GearSlot::Trinket => "trinket",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § StatKind  — cross-slot cross-class stat-vocabulary
// ───────────────────────────────────────────────────────────────────────

/// Canonical stat-kind enum. Used by `BaseItem.base_stats` (per-class roots) and
/// `AffixDescriptor.stat_kind` (rolled-affix targets). Closed-set ; extension
/// requires editing this enum + bumping crate version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StatKind {
    // Weapon roots
    /// Base damage roll.
    Damage,
    /// Effective range (m).
    Range,
    /// Stamina cost-per-attack.
    StaminaCost,
    /// Attacks-per-second.
    AttackSpeed,
    /// Crit-chance fraction ∈ [0,1].
    CritChance,
    /// Crit-damage multiplier (∆ on crit).
    CritDamage,
    // Armor roots
    /// Armor-rating (flat).
    ArmorRating,
    /// Physical-damage-resist fraction.
    PhysicalResist,
    /// Elemental-damage-resist fraction.
    ElementalResist,
    /// Item weight (kg).
    Weight,
    // Jewelry roots
    /// Mana-pool flat-add.
    ManaPool,
    /// Cooldown-reduction fraction.
    CooldownReduction,
    /// Affinity-channel binding (encoded as ordinal, see `cssl-host-craft-graph` magic).
    AffinityChannel,
    // Trinket roots
    /// Number of use-charges.
    UseCharges,
    /// Cooldown-time (s).
    Cooldown,
    /// Utility-effect ordinal (closed-set future-extended).
    UtilityEffect,
    // Affix-stats
    /// Fire-damage % bonus.
    FireDamage,
    /// Frost-damage % bonus.
    FrostDamage,
    /// Shock-damage % bonus.
    ShockDamage,
    /// Poison-DOT/sec.
    PoisonDot,
    /// Light-damage % bonus.
    LightDamage,
    /// Shadow-damage % bonus.
    ShadowDamage,
    /// Aether-damage % bonus.
    AetherDamage,
    /// Void-damage % bonus.
    VoidDamage,
    /// Earth-damage % bonus.
    EarthDamage,
    /// Lifesteal fraction.
    Lifesteal,
    /// Poise (stagger-resist).
    Poise,
    /// Stagger-induce.
    Stagger,
    /// Spell-power %.
    SpellPower,
    /// Cast-speed %.
    CastSpeed,
    /// HP-regen flat/sec.
    HpRegen,
    /// Block-rate %.
    BlockRate,
    /// Block-amount flat.
    BlockAmount,
    /// Stamina-regen flat/sec.
    StaminaRegen,
    /// Durability flat.
    Durability,
    /// Resist-Fire %.
    FireResist,
    /// Resist-Frost %.
    FrostResist,
    /// Resist-Light %.
    LightResist,
    /// Resist-Shadow %.
    ShadowResist,
    /// Stamina-regen-in-storm %.
    StormStaminaRegen,
    /// XP-gain %.
    XpGain,
    /// Ranged-damage %.
    RangedDamage,
    /// Damage-while-low-HP %.
    LowHpDamage,
    /// Earth-affinity %.
    EarthAffinity,
    /// Chain-targets count.
    ChainTargets,
    /// AoE-radius m.
    AoeRadius,
    /// Stealth-detection %.
    StealthDetection,
    /// Burn-duration s.
    BurnDuration,
    /// Slow-on-hit %.
    SlowOnHit,
    /// Potion-effectiveness %.
    PotionEffect,
    /// Damage-vs-bosses %.
    BossDamage,
    /// Damage-per-ally-near %.
    AllyDamage,
    /// Echoes-on-floor-clear %.
    EchoesOnClear,
    /// Crit-vs-Beasts %.
    BeastCrit,
    /// Meta-currency-find %.
    CurrencyFind,
    /// Dark-vision-radius m.
    DarkVisionRadius,
    /// Revive-charge per-floor count.
    ReviveCharge,
    /// Kindling : burn-extends s.
    KindlingExtend,
}

impl StatKind {
    /// Stable name for audit + serde-key uses.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            StatKind::Damage => "damage",
            StatKind::Range => "range",
            StatKind::StaminaCost => "stamina_cost",
            StatKind::AttackSpeed => "attack_speed",
            StatKind::CritChance => "crit_chance",
            StatKind::CritDamage => "crit_damage",
            StatKind::ArmorRating => "armor_rating",
            StatKind::PhysicalResist => "physical_resist",
            StatKind::ElementalResist => "elemental_resist",
            StatKind::Weight => "weight",
            StatKind::ManaPool => "mana_pool",
            StatKind::CooldownReduction => "cooldown_reduction",
            StatKind::AffinityChannel => "affinity_channel",
            StatKind::UseCharges => "use_charges",
            StatKind::Cooldown => "cooldown",
            StatKind::UtilityEffect => "utility_effect",
            StatKind::FireDamage => "fire_damage",
            StatKind::FrostDamage => "frost_damage",
            StatKind::ShockDamage => "shock_damage",
            StatKind::PoisonDot => "poison_dot",
            StatKind::LightDamage => "light_damage",
            StatKind::ShadowDamage => "shadow_damage",
            StatKind::AetherDamage => "aether_damage",
            StatKind::VoidDamage => "void_damage",
            StatKind::EarthDamage => "earth_damage",
            StatKind::Lifesteal => "lifesteal",
            StatKind::Poise => "poise",
            StatKind::Stagger => "stagger",
            StatKind::SpellPower => "spell_power",
            StatKind::CastSpeed => "cast_speed",
            StatKind::HpRegen => "hp_regen",
            StatKind::BlockRate => "block_rate",
            StatKind::BlockAmount => "block_amount",
            StatKind::StaminaRegen => "stamina_regen",
            StatKind::Durability => "durability",
            StatKind::FireResist => "fire_resist",
            StatKind::FrostResist => "frost_resist",
            StatKind::LightResist => "light_resist",
            StatKind::ShadowResist => "shadow_resist",
            StatKind::StormStaminaRegen => "storm_stamina_regen",
            StatKind::XpGain => "xp_gain",
            StatKind::RangedDamage => "ranged_damage",
            StatKind::LowHpDamage => "low_hp_damage",
            StatKind::EarthAffinity => "earth_affinity",
            StatKind::ChainTargets => "chain_targets",
            StatKind::AoeRadius => "aoe_radius",
            StatKind::StealthDetection => "stealth_detection",
            StatKind::BurnDuration => "burn_duration",
            StatKind::SlowOnHit => "slow_on_hit",
            StatKind::PotionEffect => "potion_effect",
            StatKind::BossDamage => "boss_damage",
            StatKind::AllyDamage => "ally_damage",
            StatKind::EchoesOnClear => "echoes_on_clear",
            StatKind::BeastCrit => "beast_crit",
            StatKind::CurrencyFind => "currency_find",
            StatKind::DarkVisionRadius => "dark_vision_radius",
            StatKind::ReviveCharge => "revive_charge",
            StatKind::KindlingExtend => "kindling_extend",
        }
    }
}
