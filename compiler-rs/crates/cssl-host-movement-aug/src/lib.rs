//! § cssl-host-movement-aug — Apex/Titanfall-style movement augmentations.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W13-MOVEMENT-AUG (POD-2 ; sibling W13-6 of W13-{1..12})
//!
//! § ROLE
//!   Pure-deterministic state-machine for the movement-augmentation suite :
//!     - SPRINT     : 1.6× walk · stamina-budget 5s drain / 3s recover
//!     - SLIDE      : crouch-while-sprint · 1s duration · friction-decel
//!     - JUMP-PACK  : double-jump (2 free for-everyone) · 30% air-control
//!     - PARKOUR    : wall-run 2s max · auto-mantle low-ledges · slide-jump
//!     - GENRE-SHIFT: FPS-locked · Third-momentum · Iso-grid-snap
//!
//!   Apex Legends + Titanfall 2 inspirations — fluid-traversal feel without
//!   pay-for-power. Mechanics IDENTICAL across cosmetic-skins ; affixes only
//!   mutate visual-trail-color / footstep-audio-cue / hip-thrust-VFX.
//!
//! § COSMETIC-ONLY-AXIOM  (PRIME-DIRECTIVE alignment)
//!   ∀ skin-affix W! → only render-channel parameters mutate.
//!   ∀ stamina · ∀ sprint-mult · ∀ slide-duration · ∀ jump-count · ∀ wall-run-sec
//!     are FROZEN to canonical values regardless of skin.
//!   The `BoostAffix` struct surfaces only `trail_hue`, `audio_pack_id`,
//!   `vfx_density` — none of which feed back into `MovementAug::tick`.
//!   Test : `cosmetic_only_axiom_distance_invariant_across_skins`.
//!
//! § INTEGRATION (consumer = loa-host)
//!   Per-frame :
//!     `let intent = MovementIntent::from_input_frame(&input_frame);`
//!     `let proposed = aug.tick(&intent, &camera, dt, &world_probe);`
//!     `camera.commit_motion(proposed.delta);`
//!
//!   `world_probe` is a callback the host passes (returns `WorldHints` :
//!   on-ground · wall-on-left · wall-on-right · ledge-ahead). Movement-aug
//!   doesn't depend on the physics crate — it consumes hints and produces
//!   a proposed delta + state-update.
//!
//! § GENRE-SHIFT (W13-4 sibling integration)
//!   `CameraMode::set` rebinds the input-translation layer :
//!     - FPS       : direct WASD → first-person delta
//!     - ThirdPerson: WASD → momentum-arrow (visible UI · same delta math)
//!     - Iso       : WASD → grid-snap (45° rotated · 1m steps · still-stamina-budgeted)
//!     - TopDown   : same as Iso (no axis-rotation)
//!
//! § DETERMINISM
//!   - `#![forbid(unsafe_code)]`.
//!   - Fixed-step tick : caller passes `dt` and is responsible for
//!     fixed-timestep accumulation.
//!   - All RNG funneled through a `MovementRngHook` trait; default impl is
//!     deterministic splitmix64 seeded from the AUG-handle. The cosmetic
//!     channel uses RNG (footstep variation · trail-hue dither) ; the
//!     mechanical channel does NOT.
//!   - Snapshots are bit-equal across hosts.
//!
//! § PRIME-DIRECTIVE
//!   - `consent = OS` : sovereign-toggle `infinite_sprint` is an accessibility
//!     opt-in, NOT a paid upgrade. Never gated behind currency or battle-pass.
//!   - `¬ pay-for-power` : test-suite fails compile if any cosmetic-affix
//!     field couples to mechanical state (enforced by-construction via
//!     separate structs — see `BoostAffix` vs `MovementParams`).

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
// § T11-W13-MOVEMENT-AUG : pedantic-suite allowances ─────────────────────
//   The augmentation state-machine is a single coherent fn (cognitive-complexity)
//   with conventional axis-naming (similar_names dx/dy/dz · f_in/r_in) and
//   per-frame edge-flag struct shapes (struct_excessive_bools intentional).
//   These match the existing crate posture in cssl-host-weapons + neighbours.
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::imprecise_flops)]
#![allow(clippy::trivially_copy_pass_by_ref)] // physics-shape: prefer & for forward-compat
#![allow(clippy::partialeq_to_none)] // `next_phase != prev_phase` reads cleaner
#![allow(clippy::match_same_arms)] // explicit per-phase match preferred for discoverability
#![allow(clippy::tuple_array_conversions)] // `[dx, dy, dz]` literal preferred over `.into()`

pub mod aug;
pub mod genre;
pub mod intent;
pub mod params;
pub mod skin;
pub mod state;

pub use aug::{MovementAug, ProposedMotion, WorldHints};
pub use genre::{CameraGenre, GenreTranslator};
pub use intent::MovementIntent;
pub use params::{MovementParams, StaminaPolicy};
pub use skin::{BoostAffix, BoostSkinId};
pub use state::{LocomotionPhase, MovementState};

/// Library version surface — useful for the loa-host attestation registry.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
