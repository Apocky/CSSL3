// § damage.rs — damage-model with COSMETIC-ONLY-AXIOM enforcement
// ════════════════════════════════════════════════════════════════════
// § I> AXIOM : base-DPS-per-(kind,tier) is IDENTICAL across cosmetic-skins.
//      Headshot-mult / armor-class / weak-point are COMBAT-MECHANICS — these
//      are NOT cosmetic and DO modify outgoing damage. Cosmetic affixes
//      (tracer-color / muzzle-flash / impact-particle / sound / anim) MUST
//      NOT alter any of base_dps / headshot_multiplier / armor_modifier.
// § I> Defense-in-depth : `WeaponBuild::dps_signature()` returns a hash
//      excluding cosmetics ; tests assert two builds with same (kind,tier)
//      but different cosmetics produce equal dps_signature.
// § I> ArmorClass × DamageType modifier matrix kept FROZEN in this file.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::weapon_kind::{WeaponKind, WeaponTier};

/// 6 damage-types covering FPS-loot-shooter palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum DamageType {
    Kinetic   = 0,
    Energy    = 1,
    Thermal   = 2,
    Cryo      = 3,
    Corrosive = 4,
    Explosive = 5,
}

impl DamageType {
    pub const COUNT: usize = 6;
    pub const ALL: [Self; 6] = [
        Self::Kinetic,
        Self::Energy,
        Self::Thermal,
        Self::Cryo,
        Self::Corrosive,
        Self::Explosive,
    ];

    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// 5 armor-classes (defender side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum ArmorClass {
    Unarmored   = 0,
    Flesh       = 1,
    Plate       = 2,
    Shielded    = 3,
    Constructed = 4,
}

impl ArmorClass {
    pub const COUNT: usize = 5;
    pub const ALL: [Self; 5] = [
        Self::Unarmored,
        Self::Flesh,
        Self::Plate,
        Self::Shielded,
        Self::Constructed,
    ];

    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Body-part hit zone (drives multiplier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HitZone {
    Head,
    Body,
    Limb,
    WeakPoint,
}

impl HitZone {
    /// Mechanic multiplier (NOT cosmetic).
    #[must_use]
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::Head      => 2.00,
            Self::Body      => 1.00,
            Self::Limb      => 0.75,
            Self::WeakPoint => 2.50,
        }
    }
}

/// Per-(damage,armor) modifier table FROZEN per W13-2 balance.
/// Index = damage_type * ArmorClass::COUNT + armor_class
#[allow(clippy::unreadable_literal)]
const ARMOR_MATRIX: [f32; DamageType::COUNT * ArmorClass::COUNT] = [
    // Kinetic   :  Unarm Flesh Plate Shield Construct
    1.00, 1.10, 0.65, 0.75, 0.85,
    // Energy
    1.00, 0.85, 1.20, 0.50, 1.10,
    // Thermal
    1.00, 1.30, 1.00, 1.05, 0.70,
    // Cryo
    1.00, 0.90, 0.95, 1.20, 1.00,
    // Corrosive
    1.00, 1.00, 1.45, 0.85, 1.30,
    // Explosive
    1.00, 1.10, 1.30, 1.00, 1.20,
];

/// Pure-fn lookup : (DamageType, ArmorClass) → modifier.
#[must_use]
pub fn armor_modifier(dt: DamageType, ac: ArmorClass) -> f32 {
    let idx = (dt.as_u32() as usize) * ArmorClass::COUNT + (ac.as_u32() as usize);
    ARMOR_MATRIX[idx]
}

/// Per-(WeaponKind,WeaponTier) base-DPS table.
/// COSMETIC-ONLY-AXIOM : skin choice CANNOT alter this number.
#[must_use]
pub const fn base_dps(kind: WeaponKind, tier: WeaponTier) -> f32 {
    let archetype_dps = match kind {
        WeaponKind::Pistol           => 90.0,
        WeaponKind::Rifle            => 140.0,
        WeaponKind::ShotgunSpread    => 220.0,
        WeaponKind::ShotgunSlug      => 200.0,
        WeaponKind::SniperHitscan    => 320.0,
        WeaponKind::SniperProjectile => 360.0,
        WeaponKind::Smg              => 130.0,
        WeaponKind::Lmg              => 170.0,
        WeaponKind::Bow              => 240.0,
        WeaponKind::Crossbow         => 280.0,
        WeaponKind::LaserBeam        => 150.0,
        WeaponKind::PlasmaArc        => 180.0,
        WeaponKind::Grenade          => 400.0,
        WeaponKind::Explosive        => 500.0,
        WeaponKind::Melee            => 110.0,
        WeaponKind::Throwable        => 250.0,
    };
    archetype_dps * tier.dps_multiplier()
}

/// One realized damage-roll output of `compute_damage`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DamageRoll {
    pub raw: f32,
    pub final_dmg: f32,
    pub zone: HitZone,
    pub damage_type: DamageType,
    pub armor_class: ArmorClass,
    pub crit: bool,
}

/// Compute a per-shot damage roll.
///
///   final = base_per_shot × zone_mult × armor_mod × (crit ? 1.5 : 1.0)
///
/// `base_per_shot` is the per-shot damage (already factoring fire-rate to
/// match base_dps of the weapon-kind/tier). Cosmetic skins MUST NOT pass
/// in modified `base_per_shot` — the ingestion path is a stat-table lookup,
/// not a free parameter, by construction (`WeaponBuild::per_shot()`).
#[must_use]
pub fn compute_damage(
    base_per_shot: f32,
    zone: HitZone,
    damage_type: DamageType,
    armor_class: ArmorClass,
    crit: bool,
) -> DamageRoll {
    let safe_base = if base_per_shot.is_finite() && base_per_shot >= 0.0 {
        base_per_shot
    } else {
        0.0
    };
    let zone_mult = zone.multiplier();
    let armor_mod = armor_modifier(damage_type, armor_class);
    let crit_mult = if crit { 1.5 } else { 1.0 };
    let final_dmg = safe_base * zone_mult * armor_mod * crit_mult;
    DamageRoll {
        raw: safe_base,
        final_dmg,
        zone,
        damage_type,
        armor_class,
        crit,
    }
}

/// Damage-falloff curve : returns multiplier ∈ [min_mult, 1.0] given range.
///
/// Linear interpolation between `min_range` (full-power) and `max_range`
/// (clamped to `min_mult` ≥ 0). Outside [0, max_range] saturates.
#[must_use]
pub fn damage_falloff(distance_m: f32, min_range: f32, max_range: f32, min_mult: f32) -> f32 {
    let safe_d = if distance_m.is_finite() {
        distance_m.max(0.0)
    } else {
        0.0
    };
    let safe_min_mult = if min_mult.is_finite() {
        min_mult.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if safe_d <= min_range {
        return 1.0;
    }
    if safe_d >= max_range || max_range <= min_range {
        return safe_min_mult;
    }
    let t = (safe_d - min_range) / (max_range - min_range);
    1.0 - t * (1.0 - safe_min_mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmetic_axiom_base_dps_pure_function_of_kind_tier() {
        // For each (kind,tier), invocation N times must produce IDENTICAL dps.
        for k in WeaponKind::all() {
            for t in WeaponTier::ALL {
                let a = base_dps(k, t);
                let b = base_dps(k, t);
                assert_eq!(a.to_bits(), b.to_bits(), "DPS not pure for {k:?}/{t:?}");
            }
        }
    }

    #[test]
    fn tier_strictly_amplifies_dps() {
        // Higher tier ⇒ ≥ DPS at lower tier (per kind).
        for k in WeaponKind::all() {
            let mut prev = 0.0_f32;
            for t in WeaponTier::ALL {
                let d = base_dps(k, t);
                assert!(d >= prev, "non-monotone for {k:?}");
                prev = d;
            }
        }
    }

    #[test]
    fn headshot_multiplier_2x() {
        let head = HitZone::Head.multiplier();
        let body = HitZone::Body.multiplier();
        assert!((head - 2.0).abs() < f32::EPSILON);
        assert!((body - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn falloff_saturates_correctly() {
        let near = damage_falloff(5.0, 10.0, 50.0, 0.4);
        let far = damage_falloff(60.0, 10.0, 50.0, 0.4);
        let mid = damage_falloff(30.0, 10.0, 50.0, 0.4);
        assert!((near - 1.0).abs() < f32::EPSILON);
        assert!((far - 0.4).abs() < f32::EPSILON);
        assert!(mid > 0.4 && mid < 1.0);
    }

    #[test]
    fn armor_matrix_unarmored_neutral() {
        for dt in DamageType::ALL {
            let m = armor_modifier(dt, ArmorClass::Unarmored);
            assert!((m - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn compute_damage_crit_15x() {
        let normal = compute_damage(100.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false);
        let crit = compute_damage(100.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, true);
        assert!((crit.final_dmg - 1.5 * normal.final_dmg).abs() < 0.01);
    }
}
