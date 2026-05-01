// ══════════════════════════════════════════════════════════════════════════════
// § cssl-host-combat-sim · POD-2-B1 · pure-deterministic combat-tick simulator
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : GDDs/COMBAT_SYSTEM.csl — Souls-style action-RPG combat layer
//
// § Thesis · the combat-tick is a pure-deterministic function from
//     (state, input, dt, rng-seed) → (next-state, events, damage-dealt)
//   with replay-bit-equal across hosts. Hit-detection is SDF-vs-SDF distance-min ≤ ε
//   in the engine ; stage-0 here uses sphere-vs-sphere math with explicit comments
//   that stage-1 swaps in `cssl_render_v2::sdf_eval` once that crate is wired.
//
// § Modules
//   • state_machine  · `CombatState` enum + table-driven `CombatTransition::step`
//   • stamina        · `StaminaPool` saturating-arithmetic regen / drain
//   • weapons        · `WeaponArchetype` (8) + `WeaponStats{reach,dmg,stam,spd,...}`
//   • damage_types   · `DamageType` (8) + 9-row × 8-col affinity-matrix
//   • status_effects · `StatusEffect` (16) + `StatusInstance` + stack-policy
//   • hit_detection  · `HitSample` + `weapon_path_samples` + `sdf_distance_min`
//   • seed           · splitmix64 deterministic RNG (no std::rand)
//   • tick           · `CombatTick` master per-actor state + `tick(input,dt)`
//
// § Invariants (per GDD)
//   • combat-tick = pure-deterministic · seeded-RNG · replay-bit-equal
//   • stamina-economy ALWAYS-bound (no infinite-spam ; saturating clamp-to-zero)
//   • hit-detection = SDF-distance-min ≤ ε  (stage-0 = sphere-vs-sphere)
//   • damage-affinity 8×8 col + 9-row target-class table FROZEN
//   • 16 status-effects enumerated · 8 weapon-archetypes · 8 damage-types
//
// § Discipline · #![forbid(unsafe_code)] · NO `.unwrap()` outside tests ·
//   saturating arithmetic ·  no .panic!() in library code · BTreeMap for any
//   serde-maps · public types FFI-friendly (Copy where reasonable so .csl extern
//   decls can mirror them).

#![forbid(unsafe_code)]
#![doc = "Pure-deterministic combat-tick simulator (Souls-style) — POD-2-B1 consumer of GDDs/COMBAT_SYSTEM."]
// Crate-level lint allows ; rationale per allow :
//   • match_same_arms       : table-driven state-machine + stack-policy dispatch
//                             keeps arms explicit ∀ FFI-mirror parity ¬ collapsed
//   • cast_precision_loss   : RNG bit-reduction (24 / 53 mantissa) is intentional ;
//                             determinism load-bearing > precision-tail
//   • suboptimal_flops      : `mul_add` requires hardware-fused FMA which is NOT
//                             bit-equal cross-host ; we explicitly avoid it for
//                             replay-bit-equal axiom (GDD § DETERMINISM)
//   • float_cmp             : tests use absolute-tolerance assertions OR exact
//                             pure-math results ; clippy false-positives in tests
//   • manual_clamp          : clamp() panics on NaN-bounds ; defensive .max/.min
//                             chain is preferred for NaN-safe sat-arithmetic
//   • unnested_or_patterns  : explicit per-state arms keep table-readable
//   • cast_precision_loss-usize : sample-count is human-scale (≤ 32) ; tail < ε
#![allow(
    clippy::match_same_arms,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::float_cmp,
    clippy::manual_clamp,
    clippy::unnested_or_patterns
)]

pub mod state_machine;
pub mod stamina;
pub mod weapons;
pub mod damage_types;
pub mod status_effects;
pub mod hit_detection;
pub mod seed;
pub mod tick;

// re-exports : flat top-level surface for FFI callers + .csl extern decls
pub use state_machine::{CombatInput, CombatState, CombatTransition};
pub use stamina::{StaminaAction, StaminaPool};
pub use weapons::{RangeClass, SpecialMoveId, WeaponArchetype, WeaponStats};
pub use damage_types::{
    apply_affinity, ArmorClass, DamageRoll, DamageType, AFFINITY_ROWS, AFFINITY_COLS,
};
pub use status_effects::{StackPolicy, StatusEffect, StatusInstance};
pub use hit_detection::{
    sdf_distance_min, weapon_path_samples, HitSample, TargetSphere, EPSILON_GLANCE, EPSILON_HIT,
    EPSILON_MAX,
};
pub use seed::DeterministicRng;
pub use tick::{CombatEvent, CombatOutput, CombatTick};

/// Crate-level PRIME-DIRECTIVE attestation banner (per `PRIME_DIRECTIVE.md` §11).
///
/// Combat damages enemy-AI only ; Charm-vs-Player + Charm-vs-Companion are
/// structurally forbidden at the type-system level (`StatusEffect::Charm`
/// MUST be gated by caller against sovereign-claim ; see `GDDs/COMBAT_SYSTEM`
/// § STATUS-EFFECTS § INTERACTION). This crate emits no surveillance ; no
/// network ; no biometric ; pure-math only.
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • combat-tick=pure-deterministic • Charm-vs-sovereign=FORBIDDEN";

/// Crate version (matches `Cargo.toml`) — surfaced for replay-manifest headers.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn banner_nonempty_and_mentions_consent() {
        assert!(!PRIME_DIRECTIVE_BANNER.is_empty());
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
    }

    #[test]
    fn version_matches_pkg() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
