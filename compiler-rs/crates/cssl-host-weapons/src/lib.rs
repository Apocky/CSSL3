// ══════════════════════════════════════════════════════════════════════════════
// § cssl-host-weapons · W13-2 · weapons substrate for FPS-looter-shooter
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : Labyrinth/systems/weapons.csl — sibling .csl declares FFI contract.
//
// § Thesis · weapons-tick is a pure-deterministic function from
//     (state, input, dt, rng-seed, environment) → (events, hits, impacts)
//   with replay-bit-equal across hosts. Hitscan = single-frame raycast ;
//   projectile = per-frame Verlet step + sweep-collision ; spline-bullet
//   inherits from projectile with deterministic spline pre-resolution.
//
// § Modules
//   • seed         · splitmix64 RNG (replay-bit-equal cross-host)
//   • weapon_kind  · 16 WeaponKind discriminants + WeaponTier (6) + MechanicClass
//   • damage       · base_dps table (FROZEN) · ArmorClass × DamageType matrix
//                    · HitZone multipliers · damage_falloff
//   • cosmetic     · WeaponCosmetic (skin / sound / particle / animation)
//                    · ZERO impact on damage path (axiom-enforced)
//   • build_spec   · WeaponBuild + dps_signature() (cosmetic-only-axiom proof)
//   • accuracy     · bloom-add per-shot · time-decay recovery · cone-jitter
//   • hitscan      · ray-sphere · multi-target pierce · falloff-applied
//   • projectile   · Verlet step + sweep-collision (line-segment vs sphere)
//   • pool         · 256-cap pre-alloc ring with free-stack reuse
//   • recoil       · pattern emitted as event-stream (W13-5 consumes)
//
// § COSMETIC-ONLY-AXIOM (CENTRAL INVARIANT)
//   Two `WeaponBuild` instances differing ONLY in `cosmetic` MUST produce :
//     • equal `dps_signature()`         ← hash of (kind, tier) only
//     • equal `base_dps()`              ← pure-fn over (kind, tier)
//     • equal `per_shot()`              ← derived from (kind, tier)
//     • equal damage from `compute_damage` for matched mechanic-inputs
//   Tests in `tests/cosmetic_dps_parity.rs` exercise this exhaustively.
//
// § Invariants
//   • weapons-tick = pure-deterministic · seeded-RNG · replay-bit-equal
//   • cosmetic-skin = visual/audio only ; CANNOT alter DPS / accuracy / recoil
//   • projectile-pool ring-buffer fixed-cap 256 ; pre-alloc ; zero heap per shot
//   • hitscan single-frame ; pierce-cap respected ; damage-falloff applied
//   • recoil emitted as data-events ; W13-2 does NOT render screen-kick
//   • integration-points : combat-sim DamageRoll · gear-archetype RarityTier ·
//     audit-emit hook (consumer wires via W13-2 event-stream)
//
// § Discipline · #![forbid(unsafe_code)] · NO `.unwrap()` outside tests ·
//   saturating arithmetic · no .panic!() in library code · public types
//   FFI-friendly (Copy where reasonable so .csl extern decls mirror them).

#![forbid(unsafe_code)]
#![doc = "Weapons substrate for the FPS-looter-shooter layer of LoA — W13-2."]
#![allow(
    clippy::match_same_arms,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    clippy::float_cmp,
    clippy::manual_clamp,
    clippy::unnested_or_patterns,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::struct_excessive_bools
)]

pub mod accuracy;
pub mod build_spec;
pub mod cosmetic;
pub mod damage;
pub mod hitscan;
pub mod pool;
pub mod projectile;
pub mod recoil;
pub mod seed;
pub mod weapon_kind;

// Re-exports : flat top-level surface for FFI callers + .csl extern decls.
pub use accuracy::{AccuracyParams, AccuracyState};
pub use build_spec::WeaponBuild;
pub use cosmetic::WeaponCosmetic;
pub use damage::{
    armor_modifier, base_dps, compute_damage, damage_falloff, ArmorClass, DamageRoll, DamageType,
    HitZone,
};
pub use hitscan::{cast_hitscan, ray_sphere_t, HitscanHit, HitscanParams, HitscanTarget, Ray};
pub use pool::{ProjectilePool, MAX_PROJECTILES};
pub use projectile::{
    step_projectile, sweep_collision, Projectile, ProjectileImpact, TrajectoryEnv,
};
pub use recoil::{push_event, recoil_for, RecoilEvent};
pub use seed::DeterministicRng;
pub use weapon_kind::{MechanicClass, WeaponKind, WeaponTier};

/// Crate-level cosmetic-only-axiom + PRIME-DIRECTIVE attestation banner.
pub const COSMETIC_ONLY_AXIOM_BANNER: &str =
    "consent=OS • DPS=f(kind,tier) only • cosmetic-skin=visual+audio ONLY • W13-2";

/// Crate version — surfaced for replay-manifest headers.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn banner_mentions_cosmetic_axiom() {
        assert!(COSMETIC_ONLY_AXIOM_BANNER.contains("cosmetic-skin"));
        assert!(COSMETIC_ONLY_AXIOM_BANNER.contains("DPS"));
    }

    #[test]
    fn version_matches_pkg() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn weapon_kind_count_at_least_10() {
        assert!(WeaponKind::COUNT >= 10);
    }
}
