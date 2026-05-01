//! § Affixes — 24 prefixes + 24 suffixes per GDD § PREFIX-AFFIXES + § SUFFIX-AFFIXES.
//!
//! Each affix descriptor carries : kind · stat-kind · range (min, max) · tier-band.
//! Range is interpreted by `crate::stat_rolling::roll_affix` against the rarity-tier-band.

use serde::{Deserialize, Serialize};

use crate::slots::StatKind;

// ───────────────────────────────────────────────────────────────────────
// § AffixKind
// ───────────────────────────────────────────────────────────────────────

/// Two-pole : prefix vs suffix. Glyphs are a separate axis (`crate::glyph_slots`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AffixKind {
    /// Prefix — placed before base-name in display.
    Prefix,
    /// Suffix — placed after base-name in display.
    Suffix,
}

// ───────────────────────────────────────────────────────────────────────
// § Prefix enum  — 24 distinct entries per GDD
// ───────────────────────────────────────────────────────────────────────

/// 24 prefix-affixes. `descriptor()` yields the rolling parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Prefix {
    /// «Burning» +Fire-damage 5..15%.
    Burning,
    /// «Vampiric» lifesteal 1..3%.
    Vampiric,
    /// «Swift» +attack-speed 5..12%.
    Swift,
    /// «Resolute» +poise 8..20.
    Resolute,
    /// «Cruel» +crit-damage 10..25%.
    Cruel,
    /// «Heavy» +stagger 5..18.
    Heavy,
    /// «Shocking» +Shock-damage 5..15%.
    Shocking,
    /// «Frozen» +Frost-damage 5..15%.
    Frozen,
    /// «Venomous» +Poison-DOT 3..10/s.
    Venomous,
    /// «Radiant» +Light-damage 5..15%.
    Radiant,
    /// «Tenebrous» +Shadow-damage 5..15%.
    Tenebrous,
    /// «Aetheric» +Aether-damage 5..15%.
    Aetheric,
    /// «Voidtouched» +Void-damage 5..15%.
    Voidtouched,
    /// «Hardened» +armor 4..14%.
    Hardened,
    /// «Reinforced» +durability 10..30%.
    Reinforced,
    /// «Featherlight» -weight 10..25%.
    Featherlight,
    /// «Empowered» +spell-power 5..15%.
    Empowered,
    /// «Channeling» +cast-speed 3..10%.
    Channeling,
    /// «Glassblade» +crit-chance 3..8%.
    Glassblade,
    /// «Brutal» +damage 4..12%.
    Brutal,
    /// «Mending» +HP-regen 0.5..2/s.
    Mending,
    /// «Defiant» +block-rate 3..8%.
    Defiant,
    /// «Howling» +stamina-regen 0.4..1.2/s.
    Howling,
    /// «Verdant» +Earth-damage 5..15%.
    Verdant,
}

impl Prefix {
    /// All 24 prefixes in canonical declaration-order.
    #[must_use]
    pub const fn all() -> [Prefix; 24] {
        [
            Prefix::Burning, Prefix::Vampiric, Prefix::Swift, Prefix::Resolute,
            Prefix::Cruel, Prefix::Heavy, Prefix::Shocking, Prefix::Frozen,
            Prefix::Venomous, Prefix::Radiant, Prefix::Tenebrous, Prefix::Aetheric,
            Prefix::Voidtouched, Prefix::Hardened, Prefix::Reinforced, Prefix::Featherlight,
            Prefix::Empowered, Prefix::Channeling, Prefix::Glassblade, Prefix::Brutal,
            Prefix::Mending, Prefix::Defiant, Prefix::Howling, Prefix::Verdant,
        ]
    }

    /// Stable display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Prefix::Burning => "Burning",
            Prefix::Vampiric => "Vampiric",
            Prefix::Swift => "Swift",
            Prefix::Resolute => "Resolute",
            Prefix::Cruel => "Cruel",
            Prefix::Heavy => "Heavy",
            Prefix::Shocking => "Shocking",
            Prefix::Frozen => "Frozen",
            Prefix::Venomous => "Venomous",
            Prefix::Radiant => "Radiant",
            Prefix::Tenebrous => "Tenebrous",
            Prefix::Aetheric => "Aetheric",
            Prefix::Voidtouched => "Voidtouched",
            Prefix::Hardened => "Hardened",
            Prefix::Reinforced => "Reinforced",
            Prefix::Featherlight => "Featherlight",
            Prefix::Empowered => "Empowered",
            Prefix::Channeling => "Channeling",
            Prefix::Glassblade => "Glassblade",
            Prefix::Brutal => "Brutal",
            Prefix::Mending => "Mending",
            Prefix::Defiant => "Defiant",
            Prefix::Howling => "Howling",
            Prefix::Verdant => "Verdant",
        }
    }

    /// Per-prefix descriptor : (stat-kind, range-min, range-max). All ranges
    /// in absolute units consistent with `StatKind` semantics (% as 0.05 = 5%).
    #[must_use]
    pub const fn descriptor(self) -> AffixDescriptor {
        let (sk, lo, hi) = match self {
            Prefix::Burning      => (StatKind::FireDamage,   0.05, 0.15),
            Prefix::Vampiric     => (StatKind::Lifesteal,    0.01, 0.03),
            Prefix::Swift        => (StatKind::AttackSpeed,  0.05, 0.12),
            Prefix::Resolute     => (StatKind::Poise,        8.0,  20.0),
            Prefix::Cruel        => (StatKind::CritDamage,   0.10, 0.25),
            Prefix::Heavy        => (StatKind::Stagger,      5.0,  18.0),
            Prefix::Shocking     => (StatKind::ShockDamage,  0.05, 0.15),
            Prefix::Frozen       => (StatKind::FrostDamage,  0.05, 0.15),
            Prefix::Venomous     => (StatKind::PoisonDot,    3.0,  10.0),
            Prefix::Radiant      => (StatKind::LightDamage,  0.05, 0.15),
            Prefix::Tenebrous    => (StatKind::ShadowDamage, 0.05, 0.15),
            Prefix::Aetheric     => (StatKind::AetherDamage, 0.05, 0.15),
            Prefix::Voidtouched  => (StatKind::VoidDamage,   0.05, 0.15),
            Prefix::Hardened     => (StatKind::ArmorRating,  0.04, 0.14),
            Prefix::Reinforced   => (StatKind::Durability,   0.10, 0.30),
            Prefix::Featherlight => (StatKind::Weight,      -0.25, -0.10),
            Prefix::Empowered    => (StatKind::SpellPower,   0.05, 0.15),
            Prefix::Channeling   => (StatKind::CastSpeed,    0.03, 0.10),
            Prefix::Glassblade   => (StatKind::CritChance,   0.03, 0.08),
            Prefix::Brutal       => (StatKind::Damage,       0.04, 0.12),
            Prefix::Mending      => (StatKind::HpRegen,      0.5,  2.0),
            Prefix::Defiant      => (StatKind::BlockRate,    0.03, 0.08),
            Prefix::Howling      => (StatKind::StaminaRegen, 0.4,  1.2),
            Prefix::Verdant      => (StatKind::EarthDamage,  0.05, 0.15),
        };
        AffixDescriptor {
            kind: AffixKind::Prefix,
            stat_kind: sk,
            range: (lo, hi),
            tier_band: 1,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Suffix enum  — 24 distinct entries per GDD
// ───────────────────────────────────────────────────────────────────────

/// 24 suffix-affixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Suffix {
    /// «of-the-Phoenix» revive 1× per-floor.
    OfThePhoenix,
    /// «of-the-Tide» +Frost-resist 8..25%.
    OfTheTide,
    /// «of-Hunting» +crit-vs-Beasts 10..30%.
    OfHunting,
    /// «of-Echoes» +meta-currency-find 3..8%.
    OfEchoes,
    /// «of-the-Forge» +Fire-resist 8..25%.
    OfTheForge,
    /// «of-the-Abyss» +dark-vision-radius 4..10.
    OfTheAbyss,
    /// «of-the-Sanctum» +Light-resist 8..25%.
    OfTheSanctum,
    /// «of-the-Crypt» +Shadow-resist 8..25%.
    OfTheCrypt,
    /// «of-the-Maelstrom» +stamina-regen-in-storm 30..70%.
    OfTheMaelstrom,
    /// «of-the-Endless» +XP-gain 5..12%.
    OfTheEndless,
    /// «of-Wardens» +block-amount 15..40.
    OfWardens,
    /// «of-the-Hunter» +ranged-damage 6..18%.
    OfTheHunter,
    /// «of-the-Mage» +mana-pool 20..60.
    OfTheMage,
    /// «of-the-Berserker» +damage-while-low-HP 10..30%.
    OfTheBerserker,
    /// «of-the-Druid» +Earth-affinity 5..15%.
    OfTheDruid,
    /// «of-Lightning» +chain-targets 1..3.
    OfLightning,
    /// «of-Reaping» +AoE-radius 0.5..1.5m.
    OfReaping,
    /// «of-Whispering» +stealth-detection 5..15%.
    OfWhispering,
    /// «of-Kindling» +burn-duration 1..3s.
    OfKindling,
    /// «of-Frostbite» +slow-on-hit 8..20%.
    OfFrostbite,
    /// «of-Healing» +potion-effectiveness 10..25%.
    OfHealing,
    /// «of-Slaughter» +damage-vs-bosses 5..15%.
    OfSlaughter,
    /// «of-the-Pack» +damage-per-ally-near 2..6%.
    OfThePack,
    /// «of-Returns» +Echoes-on-floor-clear 5..12%.
    OfReturns,
}

impl Suffix {
    /// All 24 suffixes in canonical declaration-order.
    #[must_use]
    pub const fn all() -> [Suffix; 24] {
        [
            Suffix::OfThePhoenix, Suffix::OfTheTide, Suffix::OfHunting, Suffix::OfEchoes,
            Suffix::OfTheForge, Suffix::OfTheAbyss, Suffix::OfTheSanctum, Suffix::OfTheCrypt,
            Suffix::OfTheMaelstrom, Suffix::OfTheEndless, Suffix::OfWardens, Suffix::OfTheHunter,
            Suffix::OfTheMage, Suffix::OfTheBerserker, Suffix::OfTheDruid, Suffix::OfLightning,
            Suffix::OfReaping, Suffix::OfWhispering, Suffix::OfKindling, Suffix::OfFrostbite,
            Suffix::OfHealing, Suffix::OfSlaughter, Suffix::OfThePack, Suffix::OfReturns,
        ]
    }

    /// Stable display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Suffix::OfThePhoenix => "of the Phoenix",
            Suffix::OfTheTide => "of the Tide",
            Suffix::OfHunting => "of Hunting",
            Suffix::OfEchoes => "of Echoes",
            Suffix::OfTheForge => "of the Forge",
            Suffix::OfTheAbyss => "of the Abyss",
            Suffix::OfTheSanctum => "of the Sanctum",
            Suffix::OfTheCrypt => "of the Crypt",
            Suffix::OfTheMaelstrom => "of the Maelstrom",
            Suffix::OfTheEndless => "of the Endless",
            Suffix::OfWardens => "of Wardens",
            Suffix::OfTheHunter => "of the Hunter",
            Suffix::OfTheMage => "of the Mage",
            Suffix::OfTheBerserker => "of the Berserker",
            Suffix::OfTheDruid => "of the Druid",
            Suffix::OfLightning => "of Lightning",
            Suffix::OfReaping => "of Reaping",
            Suffix::OfWhispering => "of Whispering",
            Suffix::OfKindling => "of Kindling",
            Suffix::OfFrostbite => "of Frostbite",
            Suffix::OfHealing => "of Healing",
            Suffix::OfSlaughter => "of Slaughter",
            Suffix::OfThePack => "of the Pack",
            Suffix::OfReturns => "of Returns",
        }
    }

    /// Per-suffix descriptor : (stat-kind, range-min, range-max).
    #[must_use]
    pub const fn descriptor(self) -> AffixDescriptor {
        let (sk, lo, hi) = match self {
            Suffix::OfThePhoenix    => (StatKind::ReviveCharge,      1.0,  1.0),
            Suffix::OfTheTide       => (StatKind::FrostResist,       0.08, 0.25),
            Suffix::OfHunting       => (StatKind::BeastCrit,         0.10, 0.30),
            Suffix::OfEchoes        => (StatKind::CurrencyFind,      0.03, 0.08),
            Suffix::OfTheForge      => (StatKind::FireResist,        0.08, 0.25),
            Suffix::OfTheAbyss      => (StatKind::DarkVisionRadius,  4.0,  10.0),
            Suffix::OfTheSanctum    => (StatKind::LightResist,       0.08, 0.25),
            Suffix::OfTheCrypt      => (StatKind::ShadowResist,      0.08, 0.25),
            Suffix::OfTheMaelstrom  => (StatKind::StormStaminaRegen, 0.30, 0.70),
            Suffix::OfTheEndless    => (StatKind::XpGain,            0.05, 0.12),
            Suffix::OfWardens       => (StatKind::BlockAmount,       15.0, 40.0),
            Suffix::OfTheHunter     => (StatKind::RangedDamage,      0.06, 0.18),
            Suffix::OfTheMage       => (StatKind::ManaPool,          20.0, 60.0),
            Suffix::OfTheBerserker  => (StatKind::LowHpDamage,       0.10, 0.30),
            Suffix::OfTheDruid      => (StatKind::EarthAffinity,     0.05, 0.15),
            Suffix::OfLightning     => (StatKind::ChainTargets,      1.0,  3.0),
            Suffix::OfReaping       => (StatKind::AoeRadius,         0.5,  1.5),
            Suffix::OfWhispering    => (StatKind::StealthDetection,  0.05, 0.15),
            Suffix::OfKindling      => (StatKind::BurnDuration,      1.0,  3.0),
            Suffix::OfFrostbite     => (StatKind::SlowOnHit,         0.08, 0.20),
            Suffix::OfHealing       => (StatKind::PotionEffect,      0.10, 0.25),
            Suffix::OfSlaughter     => (StatKind::BossDamage,        0.05, 0.15),
            Suffix::OfThePack       => (StatKind::AllyDamage,        0.02, 0.06),
            Suffix::OfReturns       => (StatKind::EchoesOnClear,     0.05, 0.12),
        };
        AffixDescriptor {
            kind: AffixKind::Suffix,
            stat_kind: sk,
            range: (lo, hi),
            tier_band: 1,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § AffixDescriptor
// ───────────────────────────────────────────────────────────────────────

/// Compile-time descriptor : kind + stat-target + value-range + tier-band-floor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AffixDescriptor {
    /// Prefix or Suffix.
    pub kind: AffixKind,
    /// Stat targeted.
    pub stat_kind: StatKind,
    /// Inclusive range : (min, max). `roll_affix` interpolates by tier.
    pub range: (f32, f32),
    /// Tier-band base ∈ ⟦1..6⟧. Rarity-driven adjustment in `stat_rolling`.
    pub tier_band: u8,
}
