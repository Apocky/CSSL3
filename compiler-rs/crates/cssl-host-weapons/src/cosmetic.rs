// § cosmetic.rs — visual/audio affixes that DO NOT touch DPS
// ════════════════════════════════════════════════════════════════════
// § I> Cosmetic-affixes are SKIN-LAYER ; tracer-color · muzzle-flash ·
//      impact-particle · weapon-sound · idle-anim. Affixes here are
//      DECORATIVE-ONLY. The damage path NEVER reads `WeaponCosmetic`.
//      Compile-time + runtime gates prevent leakage.
// § I> Two `WeaponBuild` instances with same (kind,tier) but different
//      `cosmetic` MUST produce equal `dps_signature()` (test-asserted).
// § I> Identifiers are stable u32 → string-resolution lives upstream
//      (asset-fetch tier ; not a host-weapons concern).
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Cosmetic-only affix bundle ; ZERO impact on damage/accuracy/recoil.
///
/// Adding a field here must NEVER plug into damage-flow. Reviewers please
/// reject any PR that reads any of these fields from `damage.rs` /
/// `hitscan.rs` / `projectile.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WeaponCosmetic {
    /// Tracer-color (RGB 24-bit packed ; 0 = no tracer).
    pub tracer_rgb: u32,
    /// Muzzle-flash particle-id (0 = default).
    pub muzzle_flash_id: u32,
    /// Impact-particle particle-id (0 = default).
    pub impact_particle_id: u32,
    /// Weapon-sound sfx-id (0 = default).
    pub fire_sound_id: u32,
    /// Idle-animation clip-id (0 = default).
    pub idle_anim_id: u32,
    /// Skin-id (0 = default ; non-zero = unlock-id from gear/battle-pass/gacha).
    pub skin_id: u32,
}

impl WeaponCosmetic {
    /// Stage-0 default cosmetic (all-zero ; engine renders defaults).
    pub const DEFAULT: Self = Self {
        tracer_rgb: 0,
        muzzle_flash_id: 0,
        impact_particle_id: 0,
        fire_sound_id: 0,
        idle_anim_id: 0,
        skin_id: 0,
    };

    /// Constructor for a gold-tracer skin (used by tests + Apocky-com gallery).
    #[must_use]
    pub const fn gold_tracer() -> Self {
        Self {
            tracer_rgb: 0xFFD7_00,
            muzzle_flash_id: 1,
            impact_particle_id: 1,
            fire_sound_id: 1,
            idle_anim_id: 1,
            skin_id: 1,
        }
    }

    /// Constructor for a neon-blue plasma skin.
    #[must_use]
    pub const fn neon_blue() -> Self {
        Self {
            tracer_rgb: 0x00B0_FF,
            muzzle_flash_id: 2,
            impact_particle_id: 2,
            fire_sound_id: 2,
            idle_anim_id: 2,
            skin_id: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmetics_distinct() {
        assert_ne!(WeaponCosmetic::DEFAULT, WeaponCosmetic::gold_tracer());
        assert_ne!(WeaponCosmetic::gold_tracer(), WeaponCosmetic::neon_blue());
    }

    #[test]
    fn default_all_zero() {
        let d = WeaponCosmetic::DEFAULT;
        assert_eq!(d.tracer_rgb, 0);
        assert_eq!(d.skin_id, 0);
    }

    #[test]
    fn round_trip_serde() {
        let c = WeaponCosmetic::gold_tracer();
        let s = serde_json::to_string(&c).expect("serialize");
        let back: WeaponCosmetic = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }
}
