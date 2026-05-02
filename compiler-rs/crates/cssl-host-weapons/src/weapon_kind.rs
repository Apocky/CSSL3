// § weapon_kind.rs — 16 WeaponKind enumerated per W13-2 brief
// ════════════════════════════════════════════════════════════════════
// § I> WeaponKind = TAXONOMY (mechanic + archetype) ; NOT cosmetic.
//      Two pistols with identical kind+tier have IDENTICAL DPS. Skin
//      differs only via WeaponCosmetic (tracer-color/sound/particle/anim).
// § I> Mechanic-class derives from kind : Hitscan / Projectile / Spline-Bullet
//      / Beam / Melee / Throwable / Explosive — drives weapon-tick dispatch.
// § I> 16 kinds @ session-start ; FROZEN list (additions = additive only).
// § I> All discriminants stable u32 (FFI-mirror in .csl spec).
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// 16 weapon-kinds covering the FPS-looter-shooter taxonomy per W13-2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum WeaponKind {
    Pistol           = 0,
    Rifle            = 1,
    ShotgunSpread    = 2,
    ShotgunSlug      = 3,
    SniperHitscan    = 4,
    SniperProjectile = 5,
    Smg              = 6,
    Lmg              = 7,
    Bow              = 8,
    Crossbow         = 9,
    LaserBeam        = 10,
    PlasmaArc        = 11,
    Grenade          = 12,
    Explosive        = 13,
    Melee            = 14,
    Throwable        = 15,
}

impl WeaponKind {
    /// Total count — load-bearing for table-iteration tests.
    pub const COUNT: usize = 16;

    /// Stable discriminant (mirrors `#[repr(u32)]`). Used for FFI export.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Decode from FFI u32 ; returns None on out-of-range.
    #[must_use]
    pub const fn from_u32(v: u32) -> Option<Self> {
        match v {
            0  => Some(Self::Pistol),
            1  => Some(Self::Rifle),
            2  => Some(Self::ShotgunSpread),
            3  => Some(Self::ShotgunSlug),
            4  => Some(Self::SniperHitscan),
            5  => Some(Self::SniperProjectile),
            6  => Some(Self::Smg),
            7  => Some(Self::Lmg),
            8  => Some(Self::Bow),
            9  => Some(Self::Crossbow),
            10 => Some(Self::LaserBeam),
            11 => Some(Self::PlasmaArc),
            12 => Some(Self::Grenade),
            13 => Some(Self::Explosive),
            14 => Some(Self::Melee),
            15 => Some(Self::Throwable),
            _  => None,
        }
    }

    /// Returns iterator over all 16 kinds in stable discriminant-order.
    /// Const-friendly array form — used by tests + table-builders.
    #[must_use]
    pub const fn all() -> [Self; 16] {
        [
            Self::Pistol,
            Self::Rifle,
            Self::ShotgunSpread,
            Self::ShotgunSlug,
            Self::SniperHitscan,
            Self::SniperProjectile,
            Self::Smg,
            Self::Lmg,
            Self::Bow,
            Self::Crossbow,
            Self::LaserBeam,
            Self::PlasmaArc,
            Self::Grenade,
            Self::Explosive,
            Self::Melee,
            Self::Throwable,
        ]
    }
}

/// Mechanic-class : drives per-tick dispatch in `tick.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MechanicClass {
    /// Single-frame raycast ; hit-feedback ≤1ms.
    Hitscan,
    /// Spawned projectile with spline-trajectory.
    Projectile,
    /// Like projectile but rides a deterministic spline (predicted-trajectory bows).
    SplineBullet,
    /// Continuous beam (laser/plasma) ; per-tick damage tick.
    Beam,
    /// Close-range swept melee (rare in FPS but kept for cleave-cosmetic-skin parity).
    Melee,
    /// Spawned thrown object with arc + on-impact effect (grenades / throwables).
    Throwable,
    /// Direct-hit explosive (rocket / RPG) ; projectile with AoE on impact.
    Explosive,
}

impl WeaponKind {
    /// Pure-fn dispatch : kind → mechanic-class.
    #[must_use]
    pub const fn mechanic(self) -> MechanicClass {
        match self {
            Self::Pistol
            | Self::Rifle
            | Self::ShotgunSpread
            | Self::ShotgunSlug
            | Self::SniperHitscan
            | Self::Smg
            | Self::Lmg => MechanicClass::Hitscan,
            Self::SniperProjectile | Self::Crossbow => MechanicClass::Projectile,
            Self::Bow => MechanicClass::SplineBullet,
            Self::LaserBeam | Self::PlasmaArc => MechanicClass::Beam,
            Self::Melee => MechanicClass::Melee,
            Self::Throwable | Self::Grenade => MechanicClass::Throwable,
            Self::Explosive => MechanicClass::Explosive,
        }
    }
}

/// Tier classification — drives DPS ladder. Cosmetic-only-axiom locks DPS
/// to (kind, tier) ; visual affixes do NOT modify DPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u32)]
pub enum WeaponTier {
    Common    = 0,
    Uncommon  = 1,
    Rare      = 2,
    Epic      = 3,
    Legendary = 4,
    Mythic    = 5,
}

impl WeaponTier {
    /// All 6 tiers in ascending order.
    pub const ALL: [Self; 6] = [
        Self::Common,
        Self::Uncommon,
        Self::Rare,
        Self::Epic,
        Self::Legendary,
        Self::Mythic,
    ];

    /// FFI u32.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Multiplier applied to base-DPS at this tier (mechanic, NOT cosmetic).
    /// Mythic ≈ 2× Common ; smooth curve.
    #[must_use]
    pub const fn dps_multiplier(self) -> f32 {
        match self {
            Self::Common    => 1.00,
            Self::Uncommon  => 1.15,
            Self::Rare      => 1.32,
            Self::Epic      => 1.52,
            Self::Legendary => 1.75,
            Self::Mythic    => 2.00,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_matches_array() {
        assert_eq!(WeaponKind::COUNT, WeaponKind::all().len());
    }

    #[test]
    fn discriminants_round_trip() {
        for k in WeaponKind::all() {
            assert_eq!(WeaponKind::from_u32(k.as_u32()), Some(k));
        }
    }

    #[test]
    fn mechanic_class_assigned_for_all() {
        for k in WeaponKind::all() {
            // smoke : just call ; unreachable would panic.
            let _ = k.mechanic();
        }
    }

    #[test]
    fn tier_dps_monotone_increasing() {
        let mut prev = 0.0_f32;
        for t in WeaponTier::ALL {
            let m = t.dps_multiplier();
            assert!(m >= prev);
            prev = m;
        }
    }
}
