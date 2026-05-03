//! § content_pipeline — wire DMGM-specialist council into procgen-pipeline
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-LOA-CONTENT-WIRE · Apocky-greenlit (2026-05-02)
//!
//! § APOCKY-OBSERVATION
//!   "LoA.exe currently shows concentric multicolored circles — the
//!    SUBSTRATE on its own seed-data · NOT actual game-content."
//!
//! § WHAT THIS MODULE DOES
//!   Per-frame entry-point that runs the 4 DMGM-specialists (DungeonMaster,
//!   GameMaster, Collaborator, Coder) through the
//!   `cssl-host-dm-procgen-bridge::run_council_and_generate` end-to-end :
//!
//!   ```text
//!   frame_n  ──▶  blake3-hash  ──▶  prompt_hash  ──┐
//!                                                  │
//!                                                  ▼
//!   specialists.observe(prompt_hash_bytes)  →  KAN-bias-update (per-band)
//!                                                  │
//!                                                  ▼
//!   run_council_and_generate(specialists,           ┌──────────────────────┐
//!                            prompt_hash,           │ ProcgenOutput        │
//!                            observer_pos)  ───────▶│   crystals : Vec<C>  │
//!                                                  │   asset_uris : Vec<S>│
//!                                                  │   fingerprint : u64  │
//!                                                  └──────────────────────┘
//!                                                  │
//!                                                  ▼
//!   adapt_procgen_to_loa  →  Vec<loa_host::Crystal> (capped 128)
//!   ```
//!
//! § ENV-GATE
//!   `LOA_CONTENT_PIPELINE` (default `0` · OFF) ─ when set to `1`, the
//!   substrate_render path replaces the deterministic shell-seed crystal
//!   array with the procgen-derived crystals on every Nth frame (default
//!   N=120 ≈ 1 second at 120 Hz). Default-OFF preserves the current
//!   shell-seed visual + zero behavior change for catalog/CI runs.
//!
//! § CRYSTAL-IMPEDANCE-MISMATCH (DOCUMENTED)
//!   The two crates use DIFFERENT `Crystal` types :
//!
//!   - `cssl_host_procgen_pipeline::Crystal` :
//!       { pos: ObserverCoord{x,y,z f32 metres}, kind: String, seed: u64 }
//!     Light-weight host-side projection ; metres / no spectral / no HDC.
//!
//!   - `cssl_host_crystallization::Crystal` (the type substrate_render uses) :
//!       { handle, class: CrystalClass, world_pos: WorldPos{i32 mm},
//!         extent_mm, curves, spectral, hdc, seed, fingerprint, sigma_mask }
//!     Full substrate-side type — derives all aspect curves + spectral LUT
//!     + HDC vector deterministically from `(class, seed, pos)` via
//!     `Crystal::allocate`.
//!
//!   We adapt by mapping the procgen-Crystal's `kind` string to a
//!   `CrystalClass` enum + scaling the `pos` from f32-metres to i32-mm
//!   then calling `Crystal::allocate(class, seed, pos)`. This means :
//!
//!   - The procgen-Crystal's `seed` IS preserved (drives KAN-bias evolution).
//!   - The procgen-Crystal's `pos` IS preserved (after f32→i32 conversion).
//!   - The procgen-Crystal's `kind` is HASHED into a class-bucket (we
//!     don't lose it — same kind → same bucket → same allocation).
//!   - All other fields (curves/spectral/hdc/fingerprint) are DERIVED by
//!     `Crystal::allocate` from the preserved seed+class+pos. Replay-
//!     determinism survives.
//!
//! § DETERMINISM
//!   - prompt_hash from `frame_n` via BLAKE3 → stable across hosts
//!   - specialists.decide is deterministic per dmgm-specialists doc
//!   - run_council_and_generate end-to-end deterministic
//!   - kind→class mapping is a pure-fn lookup (BLAKE3-low-byte mod 8)
//!   - Crystal::allocate is deterministic given (class, seed, pos)
//!   ⇒ identical (frame_n, observer_pos) → identical Vec<Crystal>
//!
//! § PRIME-DIRECTIVE alignment
//!   - No I/O. No network. No telemetry. Pure CPU translation.
//!   - Default-OFF env-knob ; explicit-opt-in to replace shell-seed.
//!   - Specialists' `observe(payload)` calls feed KAN-bias learning per
//!     their assigned band — matches the substrate-intelligence design.
//!   - Cap of 128 crystals matches `STARTUP_CRYSTAL_COUNT` so the GPU
//!     compute-shader's pre-allocated per-crystal buffers never overflow.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use cssl_host_crystallization::{Crystal as LoaCrystal, CrystalClass, WorldPos};
use cssl_host_dm_procgen_bridge::run_council_and_generate;
use cssl_host_dmgm_specialists::{
    CoderSpecialist, CollaboratorSpecialist, DmSpecialist, GmSpecialist, Specialist,
};
use cssl_host_procgen_pipeline::{Crystal as ProcgenCrystal, ObserverCoord, ProcgenOutput};

// ════════════════════════════════════════════════════════════════════════════
// § Constants — env-knobs + caps + cadence
// ════════════════════════════════════════════════════════════════════════════

/// Env-var that gates the entire content-pipeline integration. Default-OFF.
pub const ENV_CONTENT_PIPELINE: &str = "LOA_CONTENT_PIPELINE";

/// Cap on emitted-crystals per `tick_once` call. Matches
/// `substrate_render::STARTUP_CRYSTAL_COUNT` so the GPU compute-shader's
/// pre-allocated per-crystal storage buffer is never overflowed.
pub const PIPELINE_CRYSTAL_CAP: usize = 128;

/// Conversion-factor : 1 metre = 1000 mm. Procgen-pipeline's
/// `ObserverCoord` is f32-metres ; cssl-host-crystallization's
/// `WorldPos` is i32-mm. We multiply by 1000 + cast to i32 to bridge.
const METRES_TO_MM: f32 = 1000.0;

// ════════════════════════════════════════════════════════════════════════════
// § Public API
// ════════════════════════════════════════════════════════════════════════════

/// Returns `true` iff `LOA_CONTENT_PIPELINE=1` exactly. Any other value
/// (unset / empty / "0" / "true" / etc.) → `false` (default-off).
#[must_use]
pub fn is_pipeline_enabled() -> bool {
    matches!(std::env::var(ENV_CONTENT_PIPELINE).as_deref(), Ok("1"))
}

/// Per-frame DMGM-council → procgen-pipeline driver.
///
/// Owns 4 default Specialist trait-objects (DM/GM/Collaborator/Coder) +
/// tracks the last successfully-generated prompt-hash so callers can
/// detect content-changes.
pub struct LoaContentPipeline {
    /// 4 default specialists ; one per role. Box<dyn Specialist> so we
    /// can hold a heterogeneous set (DmSpecialist / GmSpecialist /
    /// CollaboratorSpecialist / CoderSpecialist) in one Vec.
    pub specialists: Vec<Box<dyn Specialist>>,
    /// The prompt-hash from the most recent `tick_once` that returned
    /// Some. Zero before the first `tick_once`. Useful for callers to
    /// detect "did this frame produce new content".
    pub last_prompt_hash: u64,
    /// Frame-counter local to this pipeline. Independent of the
    /// substrate_render frame_count so callers can reset it without
    /// disturbing the renderer's diagnostics.
    pub frames_observed: u64,
}

impl Default for LoaContentPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl LoaContentPipeline {
    /// Construct a pipeline with the 4 default DMGM-specialists.
    ///
    /// Each Specialist's KAN-bias band (0..=4) is determined by its role
    /// (see dmgm-specialists::SpecialistContext::with_role). DM=band 0,
    /// GM=band 1, Collaborator=band 2, Coder=band 3.
    #[must_use]
    pub fn new() -> Self {
        let dm: Box<dyn Specialist> = Box::new(DmSpecialist::new());
        let gm: Box<dyn Specialist> = Box::new(GmSpecialist::new());
        let coll: Box<dyn Specialist> = Box::new(CollaboratorSpecialist::new());
        let coder: Box<dyn Specialist> = Box::new(CoderSpecialist::new());
        Self {
            specialists: vec![dm, gm, coll, coder],
            last_prompt_hash: 0,
            frames_observed: 0,
        }
    }

    /// Per-frame entry-point. Returns `Some(Vec<LoaCrystal>)` when the
    /// content-pipeline is env-enabled AND the council yielded a procgen-
    /// actionable Decision ; `None` otherwise.
    ///
    /// § ALGORITHM
    ///   1. Increment `frames_observed`.
    ///   2. Early-return None if `LOA_CONTENT_PIPELINE != "1"`.
    ///   3. Compute `prompt_hash` = first 8 bytes of BLAKE3(frame_n.to_le_bytes()).
    ///   4. Feed `prompt_hash_bytes` to each specialist's `observe` (KAN-learn).
    ///   5. Call `run_council_and_generate(&specialists, prompt_hash, observer_pos)`.
    ///   6. Adapt ProcgenOutput.crystals → Vec<LoaCrystal> via `adapt_procgen_to_loa`.
    ///   7. Cap at `PIPELINE_CRYSTAL_CAP` (128) ; cache `prompt_hash` ; return.
    ///
    /// § DETERMINISM
    ///   Identical (`frame_n`, `observer_pos`, prior-observation history) →
    ///   identical Vec<LoaCrystal>.
    ///
    /// § GRACEFUL-DEGRADATION
    ///   If `run_council_and_generate` returns None (e.g. all specialists
    ///   Pass), this returns Some(empty Vec) — env-gate-hot caller sees
    ///   "active-but-quiet" instead of mistaking it for "disabled". Callers
    ///   that want to keep shell-seed crystals when empty can simply not
    ///   replace their `crystals` field on empty-vec.
    pub fn tick_once(
        &mut self,
        observer_pos: ObserverCoord,
        frame_n: u64,
    ) -> Option<Vec<LoaCrystal>> {
        self.frames_observed = self.frames_observed.wrapping_add(1);

        if !is_pipeline_enabled() {
            return None;
        }

        let prompt_hash = derive_prompt_hash(frame_n);
        let prompt_bytes = prompt_hash.to_le_bytes();

        // Feed observation into every specialist for KAN-bias learning.
        for s in &mut self.specialists {
            s.observe(&prompt_bytes);
        }

        // Borrow-stack : `run_council_and_generate` wants &[&dyn Specialist].
        let specs: Vec<&dyn Specialist> =
            self.specialists.iter().map(AsRef::as_ref).collect();

        let output = run_council_and_generate(&specs, prompt_hash, observer_pos);
        self.last_prompt_hash = prompt_hash;

        // Council mediated to Pass/Question → no procgen ; return Some(empty).
        let Some(out) = output else {
            return Some(Vec::new());
        };

        Some(adapt_procgen_to_loa(&out, PIPELINE_CRYSTAL_CAP))
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Public helpers (kept pub-fn so unit-tests + diagnostics can call directly)
// ════════════════════════════════════════════════════════════════════════════

/// Stable 64-bit prompt-hash from a frame number.
///
/// Computes `BLAKE3(frame_n.to_le_bytes())` and returns the first 8 bytes
/// as a little-endian u64. Same `frame_n` → same `prompt_hash` across hosts.
#[must_use]
pub fn derive_prompt_hash(frame_n: u64) -> u64 {
    let bytes = frame_n.to_le_bytes();
    let digest: [u8; 32] = blake3::hash(&bytes).into();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(head)
}

/// Map a procgen-Crystal's `kind` string into one of the 8 CrystalClass
/// values via BLAKE3-modulo. Pure-fn ; deterministic ; same kind → same
/// class always.
#[must_use]
pub fn kind_to_class(kind: &str) -> CrystalClass {
    let digest: [u8; 32] = blake3::hash(kind.as_bytes()).into();
    let bucket = digest[0] & 0b0111;
    match bucket {
        0 => CrystalClass::Object,
        1 => CrystalClass::Entity,
        2 => CrystalClass::Environment,
        3 => CrystalClass::Behavior,
        4 => CrystalClass::Event,
        5 => CrystalClass::Aura,
        6 => CrystalClass::Recipe,
        _ => CrystalClass::Inherit,
    }
}

/// Convert a procgen-pipeline ObserverCoord (f32-metres) into a
/// crystallization WorldPos (i32-millimetres).
///
/// Saturating-cast handles non-finite values + values beyond i32 range
/// without panicking. Pure-fn ; deterministic.
#[must_use]
pub fn obs_coord_to_world_pos(c: ObserverCoord) -> WorldPos {
    WorldPos {
        x_mm: f32_to_i32_mm(c.x),
        y_mm: f32_to_i32_mm(c.y),
        z_mm: f32_to_i32_mm(c.z),
    }
}

fn f32_to_i32_mm(metres: f32) -> i32 {
    if !metres.is_finite() {
        return 0;
    }
    let mm = metres * METRES_TO_MM;
    if mm >= i32::MAX as f32 {
        i32::MAX
    } else if mm <= i32::MIN as f32 {
        i32::MIN
    } else {
        mm as i32
    }
}

/// Adapt a single procgen `Crystal` into a loa-host `Crystal` via the
/// canonical `Crystal::allocate(class, seed, pos)` constructor.
///
/// IMPEDANCE-NOTE : the procgen-Crystal's `pos` is `f32-metres relative
/// to the observer` ; we convert to absolute-WorldPos in `i32-mm`. The
/// caller's observer_pos was already used by procgen-pipeline when it
/// computed `pos` (see `cssl_host_procgen_pipeline::create_crystals`),
/// so the f32 we receive is already in world-frame.
#[must_use]
pub fn adapt_one_crystal(c: &ProcgenCrystal) -> LoaCrystal {
    let class = kind_to_class(&c.kind);
    let pos = obs_coord_to_world_pos(c.pos);
    LoaCrystal::allocate(class, c.seed, pos)
}

/// Adapt the procgen-pipeline output into a Vec<LoaCrystal>, capping
/// the result at `cap` to match the substrate_render's pre-allocated
/// crystal-storage size (default 128).
#[must_use]
pub fn adapt_procgen_to_loa(out: &ProcgenOutput, cap: usize) -> Vec<LoaCrystal> {
    let take = out.crystals.len().min(cap);
    out.crystals[..take].iter().map(adapt_one_crystal).collect()
}

// ════════════════════════════════════════════════════════════════════════════
// § TESTS — env-gate · default-OFF · enabled-Some · prompt-hash-deterministic ·
//          crystal-cap-respected · graceful-degrade · adapter-correctness
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Per-test serialization for the LOA_CONTENT_PIPELINE env-var.
    /// Same shape as the procgen-pipeline's FETCH_ENV_LOCK.
    static PIPELINE_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn take_env() -> Self {
            let lock = PIPELINE_ENV_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let g = Self {
                _lock: lock,
                prev: std::env::var(ENV_CONTENT_PIPELINE).ok(),
            };
            std::env::remove_var(ENV_CONTENT_PIPELINE);
            g
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var(ENV_CONTENT_PIPELINE, v),
                None => std::env::remove_var(ENV_CONTENT_PIPELINE),
            }
        }
    }

    // ── env-gate respected (default-OFF) ──────────────────────────────────

    #[test]
    fn default_env_disables_pipeline() {
        let _g = EnvGuard::take_env();
        assert!(!is_pipeline_enabled(), "unset env-var → disabled");
        std::env::set_var(ENV_CONTENT_PIPELINE, "0");
        assert!(!is_pipeline_enabled(), "explicit '0' → disabled");
        std::env::set_var(ENV_CONTENT_PIPELINE, "true");
        assert!(!is_pipeline_enabled(), "'true' is NOT '1' → disabled");
        std::env::set_var(ENV_CONTENT_PIPELINE, "");
        assert!(!is_pipeline_enabled(), "empty → disabled");
        std::env::set_var(ENV_CONTENT_PIPELINE, "1");
        assert!(is_pipeline_enabled(), "exactly '1' → enabled");
    }

    // ── default-OFF preserves shell-seed (tick_once → None) ───────────────

    #[test]
    fn tick_once_default_off_returns_none() {
        let _g = EnvGuard::take_env();
        let mut p = LoaContentPipeline::new();
        let r = p.tick_once(ObserverCoord::ORIGIN, 42);
        assert!(r.is_none(), "default-OFF must return None");
        // frames_observed STILL increments so callers can track activity.
        assert_eq!(p.frames_observed, 1);
        // last_prompt_hash MUST stay zero (we never touched it).
        assert_eq!(p.last_prompt_hash, 0);
    }

    // ── pipeline returns Some when enabled ────────────────────────────────

    #[test]
    fn tick_once_enabled_returns_some() {
        let _g = EnvGuard::take_env();
        std::env::set_var(ENV_CONTENT_PIPELINE, "1");
        let mut p = LoaContentPipeline::new();
        let r = p.tick_once(ObserverCoord::new(1.0, 0.0, 0.0), 0xCAFE_BABE);
        assert!(r.is_some(), "enabled env → Some");
        // last_prompt_hash MUST advance from 0 (BLAKE3 of frame_n).
        assert_ne!(p.last_prompt_hash, 0);
        assert_eq!(p.frames_observed, 1);
    }

    // ── prompt-hash deterministic ─────────────────────────────────────────

    #[test]
    fn prompt_hash_deterministic_same_frame_n() {
        // Pure-fn — no env state needed.
        let h1 = derive_prompt_hash(0xDEAD_BEEF);
        let h2 = derive_prompt_hash(0xDEAD_BEEF);
        assert_eq!(h1, h2, "same frame_n must yield same prompt_hash");
        // Different frame_n → different hash (statistical-collision-free).
        let h3 = derive_prompt_hash(0xDEAD_BEEE);
        assert_ne!(h1, h3, "frame_n perturbation must perturb hash");
        // Hash is non-zero for non-zero input (sanity).
        let h_zero = derive_prompt_hash(0);
        assert_ne!(h_zero, 0, "BLAKE3(0u64) must not produce all-zero head");
    }

    // ── crystal cap respected ─────────────────────────────────────────────

    #[test]
    fn crystal_cap_respected_via_adapt() {
        // Synthesize a 200-crystal ProcgenOutput ; cap at 128.
        let crystals: Vec<ProcgenCrystal> = (0..200)
            .map(|i| ProcgenCrystal {
                pos: ObserverCoord::new(i as f32, 0.0, 0.0),
                kind: format!("k{i}"),
                seed: i as u64,
            })
            .collect();
        let out = ProcgenOutput {
            crystals,
            asset_uris: Vec::new(),
            fingerprint: 0xABCD,
        };
        let adapted = adapt_procgen_to_loa(&out, PIPELINE_CRYSTAL_CAP);
        assert_eq!(adapted.len(), PIPELINE_CRYSTAL_CAP, "must cap at 128");
        // Sub-cap remains unaffected.
        let adapted_small = adapt_procgen_to_loa(&out, 4);
        assert_eq!(adapted_small.len(), 4);
        // Empty output → empty adapted (graceful).
        let empty_out = ProcgenOutput {
            crystals: Vec::new(),
            asset_uris: Vec::new(),
            fingerprint: 0,
        };
        assert!(adapt_procgen_to_loa(&empty_out, 128).is_empty());
    }

    // ── graceful-degradation : pipeline tolerates Pass/Question consensus ─

    #[test]
    fn graceful_degrade_returns_some_empty_when_no_consensus() {
        let _g = EnvGuard::take_env();
        std::env::set_var(ENV_CONTENT_PIPELINE, "1");
        let mut p = LoaContentPipeline::new();
        // We can't easily force a Pass-consensus from real specialists,
        // but we CAN check that the wrapper never returns None when
        // env=1 — it always returns Some (possibly empty), so callers
        // can distinguish "active-but-quiet" from "disabled".
        let r = p.tick_once(ObserverCoord::ORIGIN, 1);
        assert!(r.is_some(), "enabled env always returns Some");
    }

    // ── kind→class mapping is stable + deterministic ──────────────────────

    #[test]
    fn kind_to_class_is_stable_and_deterministic() {
        // Pure-fn : same input → same class always.
        assert_eq!(kind_to_class("dragon"), kind_to_class("dragon"));
        assert_eq!(kind_to_class(""), kind_to_class(""));
        // Different inputs likely → different classes (BLAKE3 spread).
        let a = kind_to_class("dm-arc");
        let b = kind_to_class("gm-narration");
        let c = kind_to_class("modify:scene");
        // At least two of them differ (collision in 8 buckets is rare
        // but possible ; we don't claim all-three-distinct).
        let all_same = a == b && b == c;
        assert!(!all_same, "kind→class must spread (BLAKE3 dispersion)");
    }

    // ── obs_coord_to_world_pos handles edge cases ─────────────────────────

    #[test]
    fn obs_coord_to_world_pos_metres_to_mm() {
        let c = ObserverCoord::new(1.5, 0.0, -2.0);
        let w = obs_coord_to_world_pos(c);
        assert_eq!(w.x_mm, 1500);
        assert_eq!(w.y_mm, 0);
        assert_eq!(w.z_mm, -2000);
        // Non-finite → 0.
        let nan = ObserverCoord::new(f32::NAN, f32::INFINITY, f32::NEG_INFINITY);
        let wn = obs_coord_to_world_pos(nan);
        assert_eq!(wn.x_mm, 0);
        assert_eq!(wn.y_mm, 0);
        assert_eq!(wn.z_mm, 0);
        // Saturating : huge value → i32::MAX (not panic).
        let big = ObserverCoord::new(1e20, 0.0, 0.0);
        let wb = obs_coord_to_world_pos(big);
        assert_eq!(wb.x_mm, i32::MAX);
    }

    // ── adapt_one_crystal preserves seed ──────────────────────────────────

    #[test]
    fn adapt_one_crystal_preserves_seed() {
        let pc = ProcgenCrystal {
            pos: ObserverCoord::new(2.5, -1.0, 3.0),
            kind: "lantern".to_string(),
            seed: 0xFEED_FACE_DEAD_BEEF,
        };
        let lc = adapt_one_crystal(&pc);
        assert_eq!(lc.seed, 0xFEED_FACE_DEAD_BEEF);
        assert_eq!(lc.world_pos.x_mm, 2500);
        assert_eq!(lc.world_pos.y_mm, -1000);
        assert_eq!(lc.world_pos.z_mm, 3000);
        assert_eq!(lc.sigma_mask, 0xFF, "default = all-aspects-permitted");
    }

    // ── tick_once frames_observed increments under both env states ────────

    #[test]
    fn tick_once_frames_increments_under_both_env_states() {
        let _g = EnvGuard::take_env();
        // OFF state.
        let mut p = LoaContentPipeline::new();
        let _ = p.tick_once(ObserverCoord::ORIGIN, 1);
        let _ = p.tick_once(ObserverCoord::ORIGIN, 2);
        assert_eq!(p.frames_observed, 2);
        // Flip ON.
        std::env::set_var(ENV_CONTENT_PIPELINE, "1");
        let _ = p.tick_once(ObserverCoord::ORIGIN, 3);
        assert_eq!(p.frames_observed, 3);
    }

    // ── default constructor exposes 4 specialists ─────────────────────────

    #[test]
    fn default_constructor_has_4_specialists() {
        let p = LoaContentPipeline::default();
        assert_eq!(p.specialists.len(), 4, "DM + GM + Coll + Coder");
        assert_eq!(p.last_prompt_hash, 0);
        assert_eq!(p.frames_observed, 0);
    }

    // ── deterministic between two pipelines with same frame_n ─────────────

    #[test]
    fn two_pipelines_same_frame_yield_same_prompt_hash() {
        // Even before run_council, two fresh pipelines fed the same
        // frame_n compute the same prompt_hash. Determinism check.
        let _g = EnvGuard::take_env();
        std::env::set_var(ENV_CONTENT_PIPELINE, "1");
        let mut p1 = LoaContentPipeline::new();
        let mut p2 = LoaContentPipeline::new();
        let _ = p1.tick_once(ObserverCoord::ORIGIN, 0xABCD);
        let _ = p2.tick_once(ObserverCoord::ORIGIN, 0xABCD);
        assert_eq!(p1.last_prompt_hash, p2.last_prompt_hash);
        assert_ne!(p1.last_prompt_hash, 0);
    }
}
