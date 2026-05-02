// § build_spec.rs — WeaponBuild + cosmetic-only-axiom DPS-signature
// ════════════════════════════════════════════════════════════════════
// § I> WeaponBuild = (kind, tier, cosmetic). dps_signature() hashes
//      ONLY the (kind, tier) pair → cosmetic-skin SWAP cannot alter
//      DPS-signature. Tests assert this invariant exhaustively.
// § I> per_shot() returns the per-shot damage = base_dps / fire_rate
//      with kind-specific fire-rate constants. NO cosmetic-input.
// § I> NaN-safe ; pure ; const-friendly where possible.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::cosmetic::WeaponCosmetic;
use crate::damage::base_dps;
use crate::weapon_kind::{WeaponKind, WeaponTier};

/// Full per-instance weapon descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WeaponBuild {
    pub kind: WeaponKind,
    pub tier: WeaponTier,
    pub cosmetic: WeaponCosmetic,
}

impl WeaponBuild {
    #[must_use]
    pub const fn new(kind: WeaponKind, tier: WeaponTier, cosmetic: WeaponCosmetic) -> Self {
        Self { kind, tier, cosmetic }
    }

    /// Default-cosmetic constructor (most common path).
    #[must_use]
    pub const fn with_default_skin(kind: WeaponKind, tier: WeaponTier) -> Self {
        Self::new(kind, tier, WeaponCosmetic::DEFAULT)
    }

    /// Per-(kind,tier) base-DPS — pure-fn ; cosmetic-skin agnostic.
    #[must_use]
    pub const fn base_dps(self) -> f32 {
        base_dps(self.kind, self.tier)
    }

    /// Fire-rate (rounds-per-second) per WeaponKind.
    #[must_use]
    pub const fn fire_rate_rps(self) -> f32 {
        match self.kind {
            WeaponKind::Pistol           => 5.0,
            WeaponKind::Rifle            => 8.0,
            WeaponKind::ShotgunSpread    => 1.5,
            WeaponKind::ShotgunSlug      => 1.5,
            WeaponKind::SniperHitscan    => 1.0,
            WeaponKind::SniperProjectile => 0.8,
            WeaponKind::Smg              => 14.0,
            WeaponKind::Lmg              => 10.0,
            WeaponKind::Bow              => 1.5,
            WeaponKind::Crossbow         => 1.0,
            WeaponKind::LaserBeam        => 12.0, // tick-rate per-second for beam damage
            WeaponKind::PlasmaArc        => 6.0,
            WeaponKind::Grenade          => 1.0,
            WeaponKind::Explosive        => 0.7,
            WeaponKind::Melee            => 2.5,
            WeaponKind::Throwable        => 1.0,
        }
    }

    /// Per-shot raw damage = base_dps / fire_rate.
    #[must_use]
    pub fn per_shot(self) -> f32 {
        let dps = self.base_dps();
        let rate = self.fire_rate_rps();
        if rate > 0.0 && rate.is_finite() {
            dps / rate
        } else {
            0.0
        }
    }

    /// COSMETIC-ONLY-AXIOM enforcement : signature hashes ONLY (kind, tier).
    ///
    /// Two builds with same (kind,tier) but different `cosmetic` MUST yield
    /// equal `dps_signature()`. If a future PR routes any cosmetic field
    /// into damage-flow, the equivalence-class tests will fail.
    #[must_use]
    pub const fn dps_signature(self) -> u64 {
        // FNV-1a 64 over (kind_disc:u32, tier_disc:u32). const-fn friendly.
        let kind = self.kind.as_u32() as u64;
        let tier = self.tier.as_u32() as u64;
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        h ^= kind;
        h = h.wrapping_mul(0x100_0000_01b3);
        h ^= tier;
        h = h.wrapping_mul(0x100_0000_01b3);
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dps_signature_invariant_under_cosmetic_swap() {
        // For each (kind,tier), every cosmetic skin must produce identical signature.
        let cosmetics = [
            WeaponCosmetic::DEFAULT,
            WeaponCosmetic::gold_tracer(),
            WeaponCosmetic::neon_blue(),
        ];
        for k in WeaponKind::all() {
            for t in WeaponTier::ALL {
                let baseline = WeaponBuild::new(k, t, cosmetics[0]).dps_signature();
                for c in cosmetics {
                    let sig = WeaponBuild::new(k, t, c).dps_signature();
                    assert_eq!(sig, baseline, "cosmetic-leak detected at {k:?}/{t:?}");
                }
            }
        }
    }

    #[test]
    fn per_shot_pure_function_of_kind_tier() {
        for k in WeaponKind::all() {
            for t in WeaponTier::ALL {
                let a = WeaponBuild::with_default_skin(k, t).per_shot();
                let b = WeaponBuild::new(k, t, WeaponCosmetic::gold_tracer()).per_shot();
                assert_eq!(a.to_bits(), b.to_bits(), "per_shot leak at {k:?}/{t:?}");
            }
        }
    }

    #[test]
    fn fire_rate_positive_for_all_kinds() {
        for k in WeaponKind::all() {
            let b = WeaponBuild::with_default_skin(k, WeaponTier::Common);
            assert!(b.fire_rate_rps() > 0.0);
        }
    }
}
