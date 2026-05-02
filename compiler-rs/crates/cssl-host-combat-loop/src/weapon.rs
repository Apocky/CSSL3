//! § weapon — BLAKE3-seeded procgen WeaponStats
//! ══════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!
//! Per `combat_loop.csl` § AXIOMS :
//!
//!   t∞: weapon-stats = procgen via substrate-intelligence (¬ hardcoded-tables)
//!   t∞: same-(archetype × rarity-tier × affix-set) ⇒ same-stats (replay-canon)
//!   t∞: cosmetic-only-axiom holds at-loot-roll · damage-curve from
//!        ARCHETYPE-LAYER ¬ rarity-LAYER (rarity gates affix-COUNT not
//!        stat-magnitude per-Q-06)
//!
//! Stage-0 keeps the surface compatible with the .csl spec's
//! `__cssl_si_procgen_weapon_stats` extern by deriving stats from a BLAKE3-hash
//! of `(archetype ⊕ rarity ⊕ affix-bitfield)`. Same-input ⇒ same-output
//! (replay-determinism axiom).
//!
//! § DESIGN NOTES
//!
//! - Damage is ARCHETYPE-derived, not RARITY-scaled. Rarity at this level
//!   only controls `crit_mult` slightly (skill-reward) and `mag_capacity`
//!   (small ergonomic uplift). The 6-tier→8-tier ladder per Q-06a does NOT
//!   inflate per-shot DPS (cosmetic-only-axiom).
//!
//! - Affix-bitfield layers cosmetic-coefficients (recoil-pattern-seed,
//!   damage-falloff coefficient) but never bumps base-damage outside its
//!   archetype-band. This preserves PvP/PvE balance under Apocky-canon.
//!
//! - All stats are deterministic functions of `(archetype, rarity, affixes)`
//!   — they do NOT depend on tick, wall-clock, or any globals.
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

/// 8-tier rarity ladder per Q-06a (Apocky-canon).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common = 0,
    Uncommon = 1,
    Rare = 2,
    Epic = 3,
    Legendary = 4,
    Mythic = 5,
    Prismatic = 6,
    Chaotic = 7,
}

impl Rarity {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Common,
            1 => Self::Uncommon,
            2 => Self::Rare,
            3 => Self::Epic,
            4 => Self::Legendary,
            5 => Self::Mythic,
            6 => Self::Prismatic,
            _ => Self::Chaotic,
        }
    }

    /// Affix-count gate per rarity. Damage NOT scaled.
    pub const fn affix_count(self) -> u8 {
        match self {
            Self::Common => 0,
            Self::Uncommon => 1,
            Self::Rare => 2,
            Self::Epic => 3,
            Self::Legendary => 4,
            Self::Mythic => 5,
            Self::Prismatic => 6,
            Self::Chaotic => 8,
        }
    }
}

/// Stage-0 archetype catalogue.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archetype {
    Rifle = 0,
    Pistol = 1,
    Smg = 2,
    Shotgun = 3,
    Sniper = 4,
    Rocket = 5,
    Beam = 6,
    Bow = 7,
}

impl Archetype {
    pub const fn from_u32(v: u32) -> Self {
        match v % 8 {
            0 => Self::Rifle,
            1 => Self::Pistol,
            2 => Self::Smg,
            3 => Self::Shotgun,
            4 => Self::Sniper,
            5 => Self::Rocket,
            6 => Self::Beam,
            _ => Self::Bow,
        }
    }

    pub const fn damage_band(self) -> (f32, f32) {
        match self {
            Self::Rifle => (8.0, 12.0),
            Self::Pistol => (6.0, 9.0),
            Self::Smg => (3.0, 5.0),
            Self::Shotgun => (15.0, 25.0),
            Self::Sniper => (45.0, 80.0),
            Self::Rocket => (70.0, 110.0),
            Self::Beam => (4.0, 7.0),
            Self::Bow => (12.0, 22.0),
        }
    }

    pub const fn fire_rate_band(self) -> (f32, f32) {
        match self {
            Self::Rifle => (5.0, 9.0),
            Self::Pistol => (3.0, 5.0),
            Self::Smg => (10.0, 15.0),
            Self::Shotgun => (1.0, 2.0),
            Self::Sniper => (0.5, 1.0),
            Self::Rocket => (0.4, 0.8),
            Self::Beam => (20.0, 30.0),
            Self::Bow => (1.5, 3.0),
        }
    }

    pub const fn mag_capacity_band(self) -> (u32, u32) {
        match self {
            Self::Rifle => (24, 36),
            Self::Pistol => (10, 15),
            Self::Smg => (40, 60),
            Self::Shotgun => (5, 9),
            Self::Sniper => (3, 6),
            Self::Rocket => (1, 3),
            Self::Beam => (60, 100),
            Self::Bow => (1, 1),
        }
    }

    pub const fn reload_secs_band(self) -> (f32, f32) {
        match self {
            Self::Rifle => (1.5, 2.5),
            Self::Pistol => (0.8, 1.4),
            Self::Smg => (1.6, 2.4),
            Self::Shotgun => (2.5, 4.0),
            Self::Sniper => (2.5, 3.5),
            Self::Rocket => (3.0, 5.0),
            Self::Beam => (0.0, 0.0),
            Self::Bow => (0.4, 0.8),
        }
    }

    pub const fn max_range_band(self) -> (f32, f32) {
        match self {
            Self::Rifle => (60.0, 90.0),
            Self::Pistol => (25.0, 40.0),
            Self::Smg => (20.0, 35.0),
            Self::Shotgun => (10.0, 18.0),
            Self::Sniper => (150.0, 250.0),
            Self::Rocket => (80.0, 120.0),
            Self::Beam => (40.0, 60.0),
            Self::Bow => (50.0, 80.0),
        }
    }

    pub const fn proj_speed_band(self) -> (f32, f32) {
        match self {
            Self::Rifle | Self::Pistol | Self::Smg | Self::Sniper | Self::Beam => (0.0, 0.0),
            Self::Shotgun => (0.0, 0.0),
            Self::Rocket => (35.0, 60.0),
            Self::Bow => (50.0, 90.0),
        }
    }
}

/// Procgen-derived weapon stats snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponStats {
    pub archetype: Archetype,
    pub rarity: Rarity,
    pub affix_bitfield: u64,
    pub base_damage: f32,
    pub fire_rate_hz: f32,
    pub mag_capacity: u32,
    pub reload_secs: f32,
    pub max_range_m: f32,
    pub proj_speed_mps: f32,
    pub crit_mult: f32,
    pub damage_falloff: f32,
    pub recoil_seed: u64,
    pub is_valid: bool,
}

impl WeaponStats {
    pub const fn invalid() -> Self {
        Self {
            archetype: Archetype::Rifle,
            rarity: Rarity::Common,
            affix_bitfield: 0,
            base_damage: 0.0,
            fire_rate_hz: 0.0,
            mag_capacity: 0,
            reload_secs: 0.0,
            max_range_m: 0.0,
            proj_speed_mps: 0.0,
            crit_mult: 1.0,
            damage_falloff: 1.0,
            recoil_seed: 0,
            is_valid: false,
        }
    }

    /// Procgen weapon-stats. Replay-deterministic via BLAKE3-of-input.
    pub fn procgen(archetype_code: u32, rarity_tier: u8, affix_bitfield: u64) -> Self {
        if rarity_tier > 7 {
            return Self::invalid();
        }
        let archetype = Archetype::from_u32(archetype_code);
        let rarity = Rarity::from_u8(rarity_tier);

        let mut h = blake3::Hasher::new();
        h.update(b"weapon-procgen-v1");
        h.update(&archetype_code.to_le_bytes());
        h.update(&[rarity_tier]);
        h.update(&affix_bitfield.to_le_bytes());
        let digest: [u8; 32] = h.finalize().into();

        let r0 = u64::from_le_bytes(digest[0..8].try_into().unwrap());
        let r1 = u64::from_le_bytes(digest[8..16].try_into().unwrap());
        let r2 = u64::from_le_bytes(digest[16..24].try_into().unwrap());
        let r3 = u64::from_le_bytes(digest[24..32].try_into().unwrap());

        let t0 = ((r0 >> 40) as f32) / ((1u64 << 24) as f32);
        let t1 = ((r1 >> 40) as f32) / ((1u64 << 24) as f32);
        let t2 = ((r2 >> 40) as f32) / ((1u64 << 24) as f32);
        let t3 = ((r3 >> 40) as f32) / ((1u64 << 24) as f32);

        let (dmg_lo, dmg_hi) = archetype.damage_band();
        let base_damage = lerp(dmg_lo, dmg_hi, t0);

        let (rate_lo, rate_hi) = archetype.fire_rate_band();
        let fire_rate_hz = lerp(rate_lo, rate_hi, t1);

        let (mag_lo, mag_hi) = archetype.mag_capacity_band();
        let rarity_mag_uplift = u32::from(rarity_tier) / 4;
        let mag_base = mag_lo + ((mag_hi - mag_lo) as f32 * t2) as u32;
        let mag_capacity = mag_base.saturating_add(rarity_mag_uplift);

        let (rls_lo, rls_hi) = archetype.reload_secs_band();
        let reload_secs = lerp(rls_lo, rls_hi, 1.0 - t1);

        let (rng_lo, rng_hi) = archetype.max_range_band();
        let max_range_m = lerp(rng_lo, rng_hi, t3);

        let (psp_lo, psp_hi) = archetype.proj_speed_band();
        let proj_speed_mps = lerp(psp_lo, psp_hi, t0);

        let rarity_crit_uplift = (rarity_tier as f32) * 0.05;
        let crit_mult = (lerp(1.5, 2.5, t1) + rarity_crit_uplift).min(3.0);

        let damage_falloff = lerp(0.85, 1.15, t3);
        let recoil_seed = r2.wrapping_add(affix_bitfield);

        Self {
            archetype,
            rarity,
            affix_bitfield,
            base_damage,
            fire_rate_hz,
            mag_capacity,
            reload_secs,
            max_range_m,
            proj_speed_mps,
            crit_mult,
            damage_falloff,
            recoil_seed,
            is_valid: true,
        }
    }

    /// Default starter-weapon (BLAKE3-seeded · replay-stable).
    pub fn starter() -> Self {
        const STARTER_SEED: u64 = 0xC0FFEE_5747_4541;
        Self::procgen(Archetype::Rifle as u32, Rarity::Common as u8, STARTER_SEED)
    }
}

#[inline]
fn lerp(lo: f32, hi: f32, t: f32) -> f32 {
    let t_clamped = t.clamp(0.0, 1.0);
    lo + (hi - lo) * t_clamped
}

// ══════════════════════════════════════════════════════════════════════════
// § TESTS
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_8_tier_canonical() {
        assert_eq!(Rarity::Common as u8, 0);
        assert_eq!(Rarity::Chaotic as u8, 7);
    }

    #[test]
    fn rarity_affix_count_monotone() {
        assert!(Rarity::Common.affix_count() < Rarity::Legendary.affix_count());
        assert!(Rarity::Legendary.affix_count() < Rarity::Chaotic.affix_count());
        assert_eq!(Rarity::Common.affix_count(), 0);
    }

    #[test]
    fn procgen_is_deterministic() {
        let a = WeaponStats::procgen(0, 3, 0xCAFEBABE);
        let b = WeaponStats::procgen(0, 3, 0xCAFEBABE);
        assert_eq!(a.base_damage.to_bits(), b.base_damage.to_bits());
        assert_eq!(a.mag_capacity, b.mag_capacity);
        assert!(a.is_valid);
    }

    #[test]
    fn procgen_varies_with_inputs() {
        let a = WeaponStats::procgen(0, 0, 0xDEADBEEF);
        let b = WeaponStats::procgen(1, 0, 0xDEADBEEF);
        assert_ne!(a.base_damage, b.base_damage);
    }

    #[test]
    fn procgen_invalid_rarity_returns_sentinel() {
        let s = WeaponStats::procgen(0, 8, 0);
        assert!(!s.is_valid);
        assert_eq!(s.base_damage, 0.0);
    }

    #[test]
    fn cosmetic_only_axiom_damage_within_archetype_band() {
        for rarity in 0u8..=7u8 {
            let s = WeaponStats::procgen(Archetype::Rifle as u32, rarity, 0xCAFE_F00D_BA5E_BA11);
            let (lo, hi) = Archetype::Rifle.damage_band();
            assert!(
                s.base_damage >= lo && s.base_damage <= hi,
                "rarity {} damage {} escapes Rifle band [{}, {}]",
                rarity,
                s.base_damage,
                lo,
                hi
            );
        }
    }

    #[test]
    fn starter_weapon_is_stable_across_calls() {
        let s1 = WeaponStats::starter();
        let s2 = WeaponStats::starter();
        assert_eq!(s1.base_damage.to_bits(), s2.base_damage.to_bits());
        assert!(s1.is_valid);
        assert_eq!(s1.archetype, Archetype::Rifle);
    }
}
