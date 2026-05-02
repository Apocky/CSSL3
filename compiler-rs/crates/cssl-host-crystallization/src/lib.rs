//! § cssl-host-crystallization — STAGE-0-BOOTSTRAP-SHIM for crystallization.csl
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W17-L-IMPL · canonical-implementation : `Labyrinth of Apocalypse/systems/crystallization.csl`
//!
//! § THESIS
//!
//! Where a conventional engine has `Mesh` (vertex+index buffers) + `Material`
//! (texture albedo + normal + spec) + `Animation` (rigged-skeletal keyframes),
//! LoA has `Crystal` — an ω-field-cell-rooted structure that holds :
//!
//!   - 8 aspect-splines (silhouette · surface · luminance · motion ·
//!                       sound · aura · echo · bloom)
//!   - A 16-band × 4-illuminant spectral-LUT (replaces texture atlases)
//!   - A 256-bit HDC vector (semantic identity · for resonance-binding)
//!   - A causal-seed propagator (replaces animation-rigs)
//!
//! There are NO triangles. There is NO texture data. There is NO mesh. The
//! crystal is queried at-frame-time by alien_materialization to produce
//! perceivable form for the current observer.
//!
//! § STAGE-0 SCOPE
//!
//! Per Apocky-axiom (memory/feedback_no_external_llm_for_loa_intelligence) :
//! the stage-0 implementation here is fully working, deterministic, and
//! canonical-mirroring. It exposes :
//!
//!   - `Crystal` struct (no Vec — fixed-size arrays for FFI-stability)
//!   - `CrystalClass` enum (8 classes per the .csl spec)
//!   - `allocate(class, intent_seed, world_pos) -> Crystal`
//!   - `aspect_eval_silhouette / surface / luminance` — sub-microsecond evaluators
//!   - `spectral_project(crystal, illuminant) -> [u8; 3]` — sRGB output
//!   - `hdc_vector(crystal) -> [u64; 4]` — 256-bit semantic vector
//!   - `revoke(crystal)` — frees + rolls-back KAN bias
//!
//! Aspect splines are stored as 16-control-point procedural curves (NOT
//! mesh data). Splines are evaluated by deterministic axis-weighted basis
//! function sums. Spectral-LUT is 16 wavelength bands × 4 illuminants ×
//! 1 byte intensity = 64 bytes per crystal.
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod aspect;
pub mod hdc;
pub mod soa;
pub mod spectral;

use crate::aspect::AspectCurves;
use crate::hdc::HdcVec256;
use crate::spectral::SpectralLut;

// ══════════════════════════════════════════════════════════════════════════
// § CrystalClass — 8 classes per crystallization.csl
// ══════════════════════════════════════════════════════════════════════════

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrystalClass {
    Object = 0,      // sword · chest · lore-stone (static-form)
    Entity = 1,      // NPC · creature (animated · dialoguing)
    Environment = 2, // forest · cathedral · void (volumetric)
    Behavior = 3,    // door-responds-to-whispers (rule-shaped)
    Event = 4,       // a sound · a flash · a transient phenomenon
    Aura = 5,        // an emotional/atmospheric field-modifier
    Recipe = 6,      // a craftable composition (alchemy/forge)
    Inherit = 7,     // derives from prior-crystal (revise-flow)
}

impl CrystalClass {
    pub fn from_u32(c: u32) -> Option<Self> {
        match c {
            0 => Some(Self::Object),
            1 => Some(Self::Entity),
            2 => Some(Self::Environment),
            3 => Some(Self::Behavior),
            4 => Some(Self::Event),
            5 => Some(Self::Aura),
            6 => Some(Self::Recipe),
            7 => Some(Self::Inherit),
            _ => None,
        }
    }
}

/// 3D position in world-coords (millimeters, fixed-point). Stage-0 uses i32
/// for FFI-stability + replay-determinism (no f32 NaN pitfalls).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldPos {
    pub x_mm: i32,
    pub y_mm: i32,
    pub z_mm: i32,
}

impl WorldPos {
    pub const fn new(x_mm: i32, y_mm: i32, z_mm: i32) -> Self {
        Self { x_mm, y_mm, z_mm }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § Crystal — the ω-field-rooted perceivable-form-spec
// ══════════════════════════════════════════════════════════════════════════

/// Maximum extent (in millimeters) of any crystal from its `world_pos`.
/// Used for visibility-cull. Stage-0 default = 2 meters ; Crystal::Environment
/// gets 32 meters because environments are volumetric.
pub const CRYSTAL_DEFAULT_EXTENT_MM: i32 = 2_000;
pub const CRYSTAL_ENV_EXTENT_MM: i32 = 32_000;

/// A crystal — substrate-state representation of a perceivable form.
///
/// NO mesh. NO texture. NO rigged skeleton. Every visible aspect emerges
/// from `curves` evaluation at frame-time. Every color comes from `spectral`
/// projection. Every animation propagates from `causal_seed`.
#[derive(Debug, Clone)]
pub struct Crystal {
    /// Stable handle — used by alien_materialization as opaque crystal-id.
    pub handle: u32,
    /// One of 8 classes.
    pub class: CrystalClass,
    /// World-position (millimeters fixed-point).
    pub world_pos: WorldPos,
    /// Bounding extent (used for ray-cull). Stage-0 isotropic.
    pub extent_mm: i32,
    /// 8 aspect curves (silhouette · surface · luminance · motion · sound ·
    /// aura · echo · bloom). Each curve is 16 control points × 4 axis-
    /// modulators · evaluated by `aspect::eval_*` functions.
    pub curves: AspectCurves,
    /// 16-band × 4-illuminant spectral-LUT. 64 bytes total.
    pub spectral: SpectralLut,
    /// 256-bit HDC semantic-identity vector. Used for resonance-binding
    /// in alien_materialization's pixel-field algorithm.
    pub hdc: HdcVec256,
    /// Deterministic seed used at allocation. Replay-fingerprint input.
    pub seed: u64,
    /// Fingerprint of all the above (BLAKE3-truncated). Equality ⇒
    /// identical perceivable form.
    pub fingerprint: u32,
    /// Σ-mask permission (bitmask). Bit i set = aspect i is permitted to
    /// be observed. Default 0xFF (all 8 aspects). Player can revoke.
    pub sigma_mask: u8,
}

impl Crystal {
    /// Allocate a crystal procedurally from `(class, intent_seed, pos)`. The
    /// allocation derives every field deterministically — no globals, no
    /// rng. Replay-safe by construction.
    pub fn allocate(class: CrystalClass, intent_seed: u64, pos: WorldPos) -> Self {
        // Mix all inputs into a per-crystal hash for downstream derivation.
        let mut h = blake3::Hasher::new();
        h.update(b"crystal-alloc-v1");
        h.update(&(class as u32).to_le_bytes());
        h.update(&intent_seed.to_le_bytes());
        h.update(&pos.x_mm.to_le_bytes());
        h.update(&pos.y_mm.to_le_bytes());
        h.update(&pos.z_mm.to_le_bytes());
        let digest: [u8; 32] = h.finalize().into();

        let handle = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) | 0x8000_0000;
        let curves = AspectCurves::derive(&digest, class);
        let spectral = SpectralLut::derive(&digest, class);
        let hdc = HdcVec256::derive(&digest);

        let extent_mm = if matches!(class, CrystalClass::Environment) {
            CRYSTAL_ENV_EXTENT_MM
        } else {
            CRYSTAL_DEFAULT_EXTENT_MM
        };

        // Fingerprint over the derived state.
        let mut fh = blake3::Hasher::new();
        fh.update(&digest);
        fh.update(&handle.to_le_bytes());
        fh.update(&extent_mm.to_le_bytes());
        let fpd: [u8; 32] = fh.finalize().into();
        let fingerprint = u32::from_le_bytes([fpd[0], fpd[1], fpd[2], fpd[3]]);

        Self {
            handle,
            class,
            world_pos: pos,
            extent_mm,
            curves,
            spectral,
            hdc,
            seed: intent_seed,
            fingerprint,
            sigma_mask: 0xFF,
        }
    }

    /// Σ-mask check : is aspect `aspect_idx` (0..8) permitted ?
    pub fn aspect_permitted(&self, aspect_idx: u8) -> bool {
        if aspect_idx >= 8 {
            return false;
        }
        (self.sigma_mask & (1u8 << aspect_idx)) != 0
    }

    /// Sovereign-revoke an aspect (e.g., player Σ-mask change).
    pub fn revoke_aspect(&mut self, aspect_idx: u8) {
        if aspect_idx < 8 {
            self.sigma_mask &= !(1u8 << aspect_idx);
        }
    }

    /// Re-grant an aspect (player opts in).
    pub fn grant_aspect(&mut self, aspect_idx: u8) {
        if aspect_idx < 8 {
            self.sigma_mask |= 1u8 << aspect_idx;
        }
    }

    /// Squared distance (in mm²) from `observer` for ray-cull / sort.
    pub fn dist_sq_mm(&self, observer: WorldPos) -> i64 {
        let dx = (self.world_pos.x_mm - observer.x_mm) as i64;
        let dy = (self.world_pos.y_mm - observer.y_mm) as i64;
        let dz = (self.world_pos.z_mm - observer.z_mm) as i64;
        dx * dx + dy * dy + dz * dz
    }

    /// Cheap visibility test : is this crystal within `max_dist_mm` of the
    /// observer (squared comparison · no sqrt). Returns false if denied
    /// by Σ-mask on the silhouette aspect.
    pub fn visible_to(&self, observer: WorldPos, max_dist_mm: i32) -> bool {
        if !self.aspect_permitted(0) {
            return false; // silhouette denied → cannot be seen
        }
        let max_sq = (max_dist_mm as i64).saturating_mul(max_dist_mm as i64);
        self.dist_sq_mm(observer) <= max_sq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_is_deterministic() {
        let a = Crystal::allocate(CrystalClass::Object, 42, WorldPos::new(0, 0, 0));
        let b = Crystal::allocate(CrystalClass::Object, 42, WorldPos::new(0, 0, 0));
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_eq!(a.handle, b.handle);
    }

    #[test]
    fn allocate_varies_with_inputs() {
        let a = Crystal::allocate(CrystalClass::Object, 42, WorldPos::new(0, 0, 0));
        let b = Crystal::allocate(CrystalClass::Object, 43, WorldPos::new(0, 0, 0));
        let c = Crystal::allocate(CrystalClass::Entity, 42, WorldPos::new(0, 0, 0));
        let d = Crystal::allocate(CrystalClass::Object, 42, WorldPos::new(1, 0, 0));
        assert_ne!(a.fingerprint, b.fingerprint);
        assert_ne!(a.fingerprint, c.fingerprint);
        assert_ne!(a.fingerprint, d.fingerprint);
    }

    #[test]
    fn handle_high_bit_set() {
        let a = Crystal::allocate(CrystalClass::Object, 42, WorldPos::new(0, 0, 0));
        assert!(a.handle & 0x8000_0000 != 0, "handle should have high bit set");
    }

    #[test]
    fn extent_for_environment_is_larger() {
        let obj = Crystal::allocate(CrystalClass::Object, 1, WorldPos::default());
        let env = Crystal::allocate(CrystalClass::Environment, 1, WorldPos::default());
        assert!(env.extent_mm > obj.extent_mm);
    }

    #[test]
    fn sigma_mask_revoke_grant() {
        let mut c = Crystal::allocate(CrystalClass::Object, 1, WorldPos::default());
        assert!(c.aspect_permitted(0));
        c.revoke_aspect(0);
        assert!(!c.aspect_permitted(0));
        c.grant_aspect(0);
        assert!(c.aspect_permitted(0));
    }

    #[test]
    fn visibility_respects_silhouette_revoke() {
        let mut c = Crystal::allocate(CrystalClass::Object, 1, WorldPos::default());
        let observer = WorldPos::new(0, 0, 100);
        assert!(c.visible_to(observer, 1_000_000));
        c.revoke_aspect(0); // silhouette
        assert!(!c.visible_to(observer, 1_000_000));
    }

    #[test]
    fn dist_sq_is_correct() {
        let c = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(3, 4, 0));
        let observer = WorldPos::new(0, 0, 0);
        assert_eq!(c.dist_sq_mm(observer), 25);
    }
}
