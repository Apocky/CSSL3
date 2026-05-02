// ══════════════════════════════════════════════════════════════════════════════
// § cssl-host-fps-feel · POD-2 · FPS-feel : ADS + recoil + bloom + crosshair
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : Labyrinth of Apocalypse/systems/fps_feel.csl  (W13-5)
//
// § Thesis · the fps-feel-tick is a pure-deterministic function from
//     (state, input, dt, rng-seed) → (next-state, events)
//   with replay-bit-equal across hosts. ADS, recoil, bloom, crosshair are
//   composable surfaces — each owns its own state and exposes deterministic
//   step() + reset() entry-points consumed by the master `FpsFeelTick`.
//
// § Modules
//   • seed       · splitmix64 deterministic RNG (matches sibling weapons/combat)
//   • ads        · `AdsState` zoom-FOV cubic-ease 90→55 over 150ms + speed-mod
//   • recoil     · `RecoilState` per-archetype kick + 300ms recovery + skill-counter
//   • bloom      · `BloomState` cone-of-fire growth + 200ms grace + exp-decay
//   • crosshair  · `CrosshairState` 4-way-pip + 80ms red hit-flash + cosmetic-skin
//   • tick       · `FpsFeelTick` master per-actor state aggregator + tick(input,dt)
//
// § Invariants (per spec)
//   • cosmetic-only-axiom : recoil-PATTERN + zoom-CURVE + bloom-MAX frozen ∀ player
//     ; visual-skin overrides allowed but never affect mechanics (DPS-parity)
//   • deterministic : seeded-RNG ; replay-bit-equal across hosts
//   • saturating arithmetic ; clamp-not-panic ; no .unwrap() outside tests
//   • 8 WeaponArchetype kinds enumerated (mirror weapons-crate first-8)
//
// § Discipline · #![forbid(unsafe_code)] · NO `.unwrap()` outside tests ·
//   saturating arithmetic ·  no .panic!() in library code · public types
//   FFI-friendly Copy where reasonable so .csl extern decls can mirror them.

#![forbid(unsafe_code)]
#![doc = "Pure-deterministic FPS-feel substrate (ADS + recoil + bloom + crosshair) — POD-2 consumer of Labyrinth/systems/fps_feel.csl."]
// Crate-level lint allows ; rationale per allow :
//   • cast_precision_loss   : RNG bit-reduction (24 mantissa) is intentional ;
//                             determinism load-bearing > precision-tail
//   • float_cmp             : tests use absolute-tolerance assertions OR exact
//                             pure-math results ; clippy false-positives in tests
//   • manual_clamp          : clamp() panics on NaN-bounds ; defensive .max/.min
//                             chain is preferred for NaN-safe sat-arithmetic
//   • suboptimal_flops      : `mul_add` requires hardware-fused FMA which is NOT
//                             bit-equal cross-host ; we explicitly avoid it for
//                             replay-bit-equal axiom (see combat-sim sibling)
//   • match_same_arms       : per-archetype recoil-table arms kept explicit
#![allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::manual_clamp,
    clippy::suboptimal_flops,
    clippy::match_same_arms,
    clippy::module_name_repetitions
)]

pub mod seed;
pub mod ads;
pub mod recoil;
pub mod bloom;
pub mod crosshair;
pub mod tick;

// re-exports : flat top-level surface for FFI callers + .csl extern decls
pub use seed::DeterministicRng;
pub use ads::{
    AdsState, FOV_HIPFIRE_DEG, FOV_ADS_DEG, ADS_TRANSITION_MS, ADS_WALK_SPEED_MULT,
};
pub use recoil::{
    RecoilState, RecoilPattern, WeaponArchetype, RecoilEvent, RECOIL_RECOVERY_MS,
};
pub use bloom::{
    BloomState, BLOOM_GRACE_MS, BLOOM_DECAY_PER_SEC, BLOOM_MAX_HIPFIRE_RAD,
    BLOOM_MAX_ADS_RAD, BLOOM_PER_SHOT_RAD,
};
pub use crosshair::{
    CrosshairState, CrosshairSkin, HitFlashKind, HIT_FLASH_DURATION_MS,
};
pub use tick::{FpsFeelInput, FpsFeelTick};
