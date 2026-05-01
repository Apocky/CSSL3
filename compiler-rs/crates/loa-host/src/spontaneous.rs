//! § spontaneous — text-seeded condensation pipeline (Stage-0).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-WAVE3-SPONT (W-WAVE3-spontaneous-cond)
//!
//! § THESIS
//!   "Spontaneous generation" : the user supplies an INTENT TEXT, the host
//!   converts it to seed-cells in the canonical Ω-field, the substrate
//!   evolves on its own time-step, and when a cell's radiance/density
//!   crosses the manifestation threshold a STRESS OBJECT condenses at that
//!   cell's world position. There is no hand-written object-spawn list ;
//!   the field IS the source-of-truth, and observed objects are byproducts
//!   of cell-state crossing a critical line.
//!
//! § PIPELINE STAGES
//!   1. text  → SeedCells via `intent_to_seed_cells` (keyword match table).
//!   2. seeds → field-cell stamps via `stamp_seed_cells_into_field`.
//!   3. evolve → cfer_render::CferRenderer::step_and_pack (already wired).
//!   4. detect → `ManifestationDetector::scan_rising_edges` flags cells whose
//!      radiance crossed `MANIFESTATION_THRESHOLD` between frames.
//!   5. spawn → window.rs polls the detector each frame and dispatches a
//!      stress-object spawn at the manifested cell's world position.
//!
//! § STAGE-0 vs STAGE-1
//!   Stage-0 : keyword → kind table (cube → 0, sphere → 1, …). Deterministic.
//!   Stage-1 : text → KAN-modulated seed-vector → field. Claude/KAN authoring
//!             of the intent → seed mapping. Will live in cssl-substrate-kan.
//!
//! § PER-FRAME COST
//!   - `intent_to_seed_cells(text)` : O(words · keywords) — small constants.
//!   - `stamp_seed_cells_into_field` : O(seeds) `stamp_cell_bootstrap` calls.
//!   - `ManifestationDetector::scan_rising_edges` : O(active_cells), capped
//!     at 1 manifestation per frame to avoid spawn-spam.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::module_name_repetitions)]

use std::collections::{HashMap, VecDeque};

use cssl_substrate_omega_field::{MortonKey, OmegaField};

use crate::cfer_render::{
    decode_radiance_probe, encode_radiance_probe, world_point_to_morton, WORLD_MAX, WORLD_MIN,
};

// ──────────────────────────────────────────────────────────────────────────
// § Constants — manifestation threshold + spawn cap
// ──────────────────────────────────────────────────────────────────────────

/// Radiance magnitude (sum of r+g+b) above which a cell is considered
/// "manifested". Stage-0 picks a low-but-meaningful threshold that the
/// canonical seed-radiance values clearly exceed.
pub const MANIFESTATION_THRESHOLD: f32 = 0.45;

/// Maximum manifestations dispatched per frame. Keeps the spawn-rate sane
/// even when many cells cross threshold simultaneously (e.g. user seeds a
/// large blob of identical text).
pub const MAX_MANIFESTATIONS_PER_FRAME: usize = 1;

/// Maximum recent manifestation events kept in the in-memory ring (for the
/// `sense.spontaneous_recent` MCP tool).
pub const RECENT_MANIFEST_RING_CAP: usize = 16;

/// Stage-0 seed-cell stamp count cap : an intent text that maps to many
/// seeds is clamped here so a single MCP call cannot exhaust the field.
pub const MAX_SEEDS_PER_INTENT: usize = 24;

/// Manifestation-window — number of frames a seed-cell remains "live" (i.e.
/// eligible to manifest). After the window expires the seed is logged as a
/// quiescent (no-manifest) outcome. Stage-0 stamps don't auto-decay ; this
/// constant is reported back to MCP callers so they know the polling window.
pub const MANIFESTATION_WINDOW_FRAMES: u32 = 60;

/// World-XZ center of the "Spontaneous-Pad" zone inside ScaleRoom NE corner.
/// `room::Room::ScaleRoom` is x ∈ [-30, 30] · z ∈ [-58, -28] · y ∈ [0, 12].
/// The pad sits at the NE corner (x near +25, z near -32) so a player can
/// walk there to test the spontaneous-condensation pipeline.
pub const SPONTANEOUS_PAD_CENTER: [f32; 3] = [25.0, 1.5, -32.0];

/// Spontaneous-pad XZ half-extent (3 m radius zone).
pub const SPONTANEOUS_PAD_HALF_EXTENT: f32 = 3.0;

// ──────────────────────────────────────────────────────────────────────────
// § SeedCell — one stamp into the Ω-field tagged with a kind hint.
// ──────────────────────────────────────────────────────────────────────────

/// One seed-cell : the world-space position the seed lives at, the seed's
/// f32 RGB radiance values, density (0..1), and the stress-object kind hint
/// the manifestation pass uses when condensing the seed into an object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeedCell {
    /// World-space (x, y, z) position the seed lives at. Clamped into the
    /// world envelope before being stamped.
    pub pos: [f32; 3],
    /// Encoded radiance probe (r, g, b) ∈ 0..1.
    pub radiance: [f32; 3],
    /// Density 0..1 (Ω-field cell.density).
    pub density: f32,
    /// Stress-object kind id (0..13). Used at manifestation time.
    pub kind_hint: u32,
    /// Human-readable hint for telemetry/logs (the keyword that produced
    /// this seed). 24 bytes inline (no allocation).
    pub label: SeedLabel,
}

/// Inline 24-byte label for a seed-cell. Storing as a fixed array keeps
/// `SeedCell` `Copy` and avoids a heap allocation per intent-token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedLabel {
    bytes: [u8; 24],
    len: u8,
}

impl SeedLabel {
    /// Construct from a `&str` ; truncates to 24 bytes.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        let mut bytes = [0u8; 24];
        let src = s.as_bytes();
        let n = src.len().min(24);
        bytes[..n].copy_from_slice(&src[..n]);
        Self {
            bytes,
            len: n as u8,
        }
    }

    /// Borrow as a `&str` (UTF-8 safety : if the truncation cut a multibyte
    /// codepoint mid-sequence, falls back to the longest valid prefix).
    #[must_use]
    pub fn as_str(&self) -> &str {
        let n = self.len as usize;
        match std::str::from_utf8(&self.bytes[..n]) {
            Ok(s) => s,
            Err(e) => std::str::from_utf8(&self.bytes[..e.valid_up_to()]).unwrap_or(""),
        }
    }
}

impl Default for SeedLabel {
    fn default() -> Self {
        Self {
            bytes: [0u8; 24],
            len: 0,
        }
    }
}

impl SeedCell {
    /// Construct a seed-cell at `pos` with the given radiance/density/kind.
    /// The position is CLAMPED into the world envelope before storage so
    /// out-of-range inputs are still safely stampable.
    #[must_use]
    pub fn new(
        pos: [f32; 3],
        radiance: [f32; 3],
        density: f32,
        kind_hint: u32,
        label: &str,
    ) -> Self {
        Self {
            pos: clamp_to_world(pos),
            radiance: [
                radiance[0].clamp(0.0, 1.0),
                radiance[1].clamp(0.0, 1.0),
                radiance[2].clamp(0.0, 1.0),
            ],
            density: density.clamp(0.0, 1.0),
            kind_hint: kind_hint.min(13),
            label: SeedLabel::from_str(label),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § World-clamp helper — keeps positions inside the canonical envelope.
// ──────────────────────────────────────────────────────────────────────────

/// Clamp a world-space position into the canonical CFER world envelope.
/// Used by `SeedCell::new` to keep all stamps stamp-safe.
#[must_use]
pub fn clamp_to_world(p: [f32; 3]) -> [f32; 3] {
    // We use a small epsilon away from WORLD_MAX so `world_to_texel` (which
    // is half-open at the upper bound) still accepts the result.
    const EPS: f32 = 0.001;
    [
        p[0].clamp(WORLD_MIN[0], WORLD_MAX[0] - EPS),
        p[1].clamp(WORLD_MIN[1], WORLD_MAX[1] - EPS),
        p[2].clamp(WORLD_MIN[2], WORLD_MAX[2] - EPS),
    ]
}

// ──────────────────────────────────────────────────────────────────────────
// § Keyword → seed-template table (Stage-0 deterministic mapping).
// ──────────────────────────────────────────────────────────────────────────

/// One keyword-driven seed template. The intent → seeds path consults this
/// table : if a token in the lowercased text matches `keyword`, the template
/// produces a `SeedCell` at the caller-supplied origin.
#[derive(Debug, Clone, Copy)]
struct SeedTemplate {
    keyword: &'static str,
    radiance: [f32; 3],
    density: f32,
    kind_hint: u32,
    /// XZ offset from the origin (so multi-keyword intents lay seeds out
    /// in a small spread rather than stacking on the same cell).
    offset: [f32; 3],
}

/// Stage-0 keyword table. Each entry maps an English noun to a seed-cell
/// template. The kind-hint corresponds to the existing 14-stress-object
/// catalog (see `geometry::stress_object_name`) :
///
///   0  glass_cube      ← cube · box · brick
///   1  bronze_sphere   ← sphere · ball · orb
///   2  rough_cylinder  ← cylinder · pillar · column
///   3  black_cone      ← cone · spike · pyramid
///   4  steel_torus     ← torus · ring · donut
///   5  glow_block      ← lamp · glow · light
///   6  raymarched_blob ← blob · cloud · puddle
///   7  raymarched_cylinder ← rod · pipe
///   8  raymarched_sphere ← bubble
///   9  raymarched_torus  ← halo
///  10  raymarched_helix  ← helix · spiral
///  11  raymarched_lattice ← grid · lattice · mesh
///  12  raymarched_fractal ← fractal · tree · branch
///  13  raymarched_metaball ← drop · droplet · slime
const SEED_TEMPLATES: &[SeedTemplate] = &[
    // 0 — glass cube : cool-tinted neutral.
    SeedTemplate {
        keyword: "cube",
        radiance: [0.55, 0.65, 0.85],
        density: 0.80,
        kind_hint: 0,
        offset: [0.0, 0.0, 0.0],
    },
    SeedTemplate {
        keyword: "box",
        radiance: [0.55, 0.65, 0.85],
        density: 0.80,
        kind_hint: 0,
        offset: [0.0, 0.0, 0.0],
    },
    SeedTemplate {
        keyword: "brick",
        radiance: [0.65, 0.45, 0.35],
        density: 0.85,
        kind_hint: 0,
        offset: [0.5, 0.0, 0.5],
    },
    // 1 — bronze sphere : warm-tinted high-density.
    SeedTemplate {
        keyword: "sphere",
        radiance: [0.85, 0.55, 0.30],
        density: 0.70,
        kind_hint: 1,
        offset: [0.0, 0.0, 0.5],
    },
    SeedTemplate {
        keyword: "ball",
        radiance: [0.85, 0.55, 0.30],
        density: 0.70,
        kind_hint: 1,
        offset: [0.0, 0.0, 0.5],
    },
    SeedTemplate {
        keyword: "orb",
        radiance: [0.55, 0.40, 0.85],
        density: 0.65,
        kind_hint: 1,
        offset: [0.0, 0.5, 0.5],
    },
    // 2 — rough cylinder.
    SeedTemplate {
        keyword: "cylinder",
        radiance: [0.40, 0.45, 0.50],
        density: 0.75,
        kind_hint: 2,
        offset: [-0.5, 0.0, 0.0],
    },
    SeedTemplate {
        keyword: "pillar",
        radiance: [0.55, 0.55, 0.55],
        density: 0.85,
        kind_hint: 2,
        offset: [-0.5, 0.5, 0.0],
    },
    SeedTemplate {
        keyword: "column",
        radiance: [0.50, 0.50, 0.55],
        density: 0.85,
        kind_hint: 2,
        offset: [-0.5, 0.5, 0.0],
    },
    // 3 — black cone.
    SeedTemplate {
        keyword: "cone",
        radiance: [0.20, 0.20, 0.20],
        density: 0.70,
        kind_hint: 3,
        offset: [0.5, 0.0, -0.5],
    },
    SeedTemplate {
        keyword: "spike",
        radiance: [0.30, 0.20, 0.40],
        density: 0.60,
        kind_hint: 3,
        offset: [0.5, 0.5, -0.5],
    },
    SeedTemplate {
        keyword: "pyramid",
        radiance: [0.45, 0.40, 0.20],
        density: 0.75,
        kind_hint: 3,
        offset: [0.5, 0.0, -0.5],
    },
    // 4 — steel torus.
    SeedTemplate {
        keyword: "torus",
        radiance: [0.60, 0.65, 0.70],
        density: 0.65,
        kind_hint: 4,
        offset: [0.0, 0.0, -0.5],
    },
    SeedTemplate {
        keyword: "ring",
        radiance: [0.85, 0.75, 0.45],
        density: 0.65,
        kind_hint: 4,
        offset: [0.0, 0.0, -0.5],
    },
    SeedTemplate {
        keyword: "donut",
        radiance: [0.70, 0.50, 0.40],
        density: 0.60,
        kind_hint: 4,
        offset: [0.0, 0.0, -0.5],
    },
    // 5 — glow block (emissive).
    SeedTemplate {
        keyword: "lamp",
        radiance: [0.95, 0.85, 0.55],
        density: 0.55,
        kind_hint: 5,
        offset: [0.0, 1.0, 0.0],
    },
    SeedTemplate {
        keyword: "glow",
        radiance: [0.95, 0.90, 0.60],
        density: 0.50,
        kind_hint: 5,
        offset: [0.0, 1.0, 0.0],
    },
    SeedTemplate {
        keyword: "light",
        radiance: [0.90, 0.90, 0.85],
        density: 0.50,
        kind_hint: 5,
        offset: [0.0, 1.0, 0.0],
    },
    // 6 — raymarched blob (soft cloud).
    SeedTemplate {
        keyword: "blob",
        radiance: [0.50, 0.70, 0.50],
        density: 0.45,
        kind_hint: 6,
        offset: [0.5, 0.0, 0.0],
    },
    SeedTemplate {
        keyword: "cloud",
        radiance: [0.75, 0.80, 0.85],
        density: 0.40,
        kind_hint: 6,
        offset: [0.5, 0.5, 0.0],
    },
    SeedTemplate {
        keyword: "puddle",
        radiance: [0.40, 0.55, 0.70],
        density: 0.50,
        kind_hint: 6,
        offset: [0.0, 0.0, 0.0],
    },
    // 7 — raymarched cylinder/rod.
    SeedTemplate {
        keyword: "rod",
        radiance: [0.55, 0.55, 0.60],
        density: 0.60,
        kind_hint: 7,
        offset: [-0.5, 0.0, 0.5],
    },
    SeedTemplate {
        keyword: "pipe",
        radiance: [0.50, 0.50, 0.55],
        density: 0.55,
        kind_hint: 7,
        offset: [-0.5, 0.0, 0.5],
    },
    // 8 — raymarched sphere (bubble).
    SeedTemplate {
        keyword: "bubble",
        radiance: [0.85, 0.90, 0.95],
        density: 0.30,
        kind_hint: 8,
        offset: [0.5, 1.0, 0.5],
    },
    // 9 — raymarched torus (halo).
    SeedTemplate {
        keyword: "halo",
        radiance: [0.95, 0.85, 0.55],
        density: 0.40,
        kind_hint: 9,
        offset: [0.0, 1.0, 0.0],
    },
    // 10 — raymarched helix.
    SeedTemplate {
        keyword: "helix",
        radiance: [0.55, 0.85, 0.90],
        density: 0.50,
        kind_hint: 10,
        offset: [0.0, 0.5, 0.5],
    },
    SeedTemplate {
        keyword: "spiral",
        radiance: [0.50, 0.85, 0.90],
        density: 0.50,
        kind_hint: 10,
        offset: [0.0, 0.5, 0.5],
    },
    // 11 — raymarched lattice.
    SeedTemplate {
        keyword: "grid",
        radiance: [0.55, 0.95, 0.55],
        density: 0.45,
        kind_hint: 11,
        offset: [0.0, 0.5, 0.0],
    },
    SeedTemplate {
        keyword: "lattice",
        radiance: [0.55, 0.95, 0.55],
        density: 0.45,
        kind_hint: 11,
        offset: [0.0, 0.5, 0.0],
    },
    SeedTemplate {
        keyword: "mesh",
        radiance: [0.50, 0.85, 0.50],
        density: 0.45,
        kind_hint: 11,
        offset: [0.0, 0.5, 0.0],
    },
    // 12 — raymarched fractal/tree.
    SeedTemplate {
        keyword: "fractal",
        radiance: [0.40, 0.85, 0.45],
        density: 0.55,
        kind_hint: 12,
        offset: [0.5, 0.0, -0.5],
    },
    SeedTemplate {
        keyword: "tree",
        radiance: [0.35, 0.55, 0.25],
        density: 0.65,
        kind_hint: 12,
        offset: [0.5, 0.0, -0.5],
    },
    SeedTemplate {
        keyword: "branch",
        radiance: [0.40, 0.45, 0.20],
        density: 0.60,
        kind_hint: 12,
        offset: [0.5, 0.5, -0.5],
    },
    // 13 — raymarched metaball.
    SeedTemplate {
        keyword: "drop",
        radiance: [0.65, 0.85, 0.95],
        density: 0.55,
        kind_hint: 13,
        offset: [-0.5, 0.0, -0.5],
    },
    SeedTemplate {
        keyword: "droplet",
        radiance: [0.65, 0.85, 0.95],
        density: 0.55,
        kind_hint: 13,
        offset: [-0.5, 0.0, -0.5],
    },
    SeedTemplate {
        keyword: "slime",
        radiance: [0.55, 0.95, 0.45],
        density: 0.65,
        kind_hint: 13,
        offset: [-0.5, 0.0, -0.5],
    },
];

// ──────────────────────────────────────────────────────────────────────────
// § Public : intent → seed-cells
// ──────────────────────────────────────────────────────────────────────────

/// Convert an INTENT TEXT to a list of seed-cells anchored at `origin`.
/// Stage-0 implementation : lowercase the text, scan for keywords from the
/// `SEED_TEMPLATES` table, and emit one `SeedCell` per match (with the
/// template's offset applied to the origin so multi-keyword intents lay
/// out as a small spatial cluster rather than stacking on a single cell).
///
/// Returns at most `MAX_SEEDS_PER_INTENT` seeds. Empty / no-keyword text
/// produces an empty Vec — callers should treat this as a quiescent
/// outcome (the intent didn't condense to anything stage-0 understood).
///
/// § STAGE-1 NOTE
///   This is the canonical extension point for KAN/Claude-driven mapping.
///   When that lands, the function signature stays stable but the body is
///   replaced with `crate::cssl_substrate_kan::intent_to_seed_cells(text)`.
#[must_use]
pub fn intent_to_seed_cells(text: &str, origin: [f32; 3]) -> Vec<SeedCell> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let lc = text.to_ascii_lowercase();
    let mut seen_kinds = HashMap::<u32, ()>::new();
    let mut seeds = Vec::new();
    for tmpl in SEED_TEMPLATES {
        // Whole-word match : we don't want "table" to fire on "ball" etc.
        if !word_contains(&lc, tmpl.keyword) {
            continue;
        }
        // Dedup by kind-hint so a sentence with both "cube" and "box"
        // produces a single kind-0 seed (different keyword templates
        // share kind ids by design).
        if seen_kinds.contains_key(&tmpl.kind_hint) {
            continue;
        }
        seen_kinds.insert(tmpl.kind_hint, ());
        let pos = [
            origin[0] + tmpl.offset[0],
            origin[1] + tmpl.offset[1],
            origin[2] + tmpl.offset[2],
        ];
        seeds.push(SeedCell::new(
            pos,
            tmpl.radiance,
            tmpl.density,
            tmpl.kind_hint,
            tmpl.keyword,
        ));
        if seeds.len() >= MAX_SEEDS_PER_INTENT {
            break;
        }
    }
    seeds
}

/// True iff `haystack` contains `needle` as a stand-alone token (delimited
/// by ASCII whitespace, punctuation, or string boundaries). Avoids "cube"
/// matching "cubes" — but DOES match "cube." and "cube,".
fn word_contains(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let nlen = needle.len();
    if nlen == 0 || bytes.len() < nlen {
        return false;
    }
    let mut i = 0;
    while i + nlen <= bytes.len() {
        if &bytes[i..i + nlen] == needle.as_bytes() {
            // Boundary check : prev + next char must be non-alphanum.
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let next_ok =
                i + nlen == bytes.len() || !bytes[i + nlen].is_ascii_alphanumeric();
            if prev_ok && next_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

// ──────────────────────────────────────────────────────────────────────────
// § Stamp seed-cells into the canonical Ω-field.
// ──────────────────────────────────────────────────────────────────────────

/// Stamp each seed-cell into `field` via `stamp_cell_bootstrap` (Σ-bypass at
/// boot/seed time — the Sovereign owns its own seed track). Returns the
/// list of `(MortonKey, kind_hint, label)` so the manifestation detector can
/// later identify which cell originated from which seed.
///
/// Logs are NOT emitted from here (the caller gets the result tuple and
/// is responsible for the structured "spontaneous_seed" event so we don't
/// double-log). The caller must hold the OmegaField mutably.
pub fn stamp_seed_cells_into_field(
    field: &mut OmegaField,
    seeds: &[SeedCell],
) -> Vec<(MortonKey, u32, SeedLabel)> {
    let mut stamped = Vec::with_capacity(seeds.len());
    for s in seeds {
        let Some(key) = world_point_to_morton(s.pos[0], s.pos[1], s.pos[2]) else {
            continue;
        };
        // Merge with any existing cell rather than overwriting : adds
        // density and saturates radiance. This way repeated seedings at
        // the same cell accumulate (intuitive : "say cube three times,
        // get a denser cube").
        let mut cell = field.cell_opt(key).unwrap_or_default();
        let (r0, g0, b0) = decode_radiance_probe(cell.radiance_probe_lo);
        let r = (r0 + s.radiance[0]).min(1.0);
        let g = (g0 + s.radiance[1]).min(1.0);
        let b = (b0 + s.radiance[2]).min(1.0);
        cell.radiance_probe_lo = encode_radiance_probe(r, g, b);
        cell.density = (cell.density + s.density).min(1.0);
        cell.enthalpy = (cell.enthalpy + 1.0).min(2.0);
        if field.stamp_cell_bootstrap(key, cell).is_ok() {
            stamped.push((key, s.kind_hint, s.label));
        }
    }
    stamped
}

// ──────────────────────────────────────────────────────────────────────────
// § ManifestationEvent — what the manifestation-detector emits.
// ──────────────────────────────────────────────────────────────────────────

/// One manifestation : a cell crossed the threshold + a stress-object would
/// be spawned at this world-position with this kind. The window-side host
/// pulls these from `ManifestationDetector::drain_manifestations` each
/// frame and dispatches the actual spawn via the existing FFI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ManifestationEvent {
    /// Frame the manifestation was detected on.
    pub frame: u64,
    /// Frame the seed was originally sown on (used to compute frames-to-
    /// manifest as a Stage-1 metric).
    pub sown_frame: u64,
    /// World-position the cell decoded to (cell-center).
    pub world_pos: [f32; 3],
    /// Stress-object kind id (0..13).
    pub kind: u32,
    /// Radiance magnitude at detect time (sum r+g+b, 0..3).
    pub radiance_mag: f32,
    /// Cell density at detect time (0..1).
    pub density: f32,
    /// Originating seed-label (or empty if the cell wasn't from a known seed).
    pub label: SeedLabel,
}

impl ManifestationEvent {
    /// Frames between sow + manifestation. Useful for telemetry on how
    /// quickly the substrate condenses different seeds.
    #[must_use]
    pub fn frames_to_manifest(&self) -> u64 {
        self.frame.saturating_sub(self.sown_frame)
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § ManifestationDetector — rising-edge poll over the dense field.
// ──────────────────────────────────────────────────────────────────────────

/// Per-frame tracker that detects radiance/density rising-edge crossings
/// over `MANIFESTATION_THRESHOLD`. Internally caches each known seed-cell's
/// last-frame radiance so the next-frame pass can decide which cells are
/// NEW manifestations vs. steady-high (already-manifested, no spawn).
///
/// § THREADING
///   Designed to be owned alongside the `CferRenderer` inside the
///   render-loop's `Renderer`. It is not Send-safe across the wgpu
///   boundary, so MCP queries should observe via the `recent_events`
///   ring mirrored into EngineState.
pub struct ManifestationDetector {
    /// Tracked seeds : Morton-key → (kind_hint, label, last_radiance_mag).
    /// When a tracked cell crosses threshold, we emit a manifestation event
    /// + remove it (one-shot per seed). Untracked cells are ignored.
    tracked: HashMap<MortonKey, TrackedSeed>,
    /// Manifestation events emitted this session (capped ring).
    pub recent_events: VecDeque<ManifestationEvent>,
    /// Total manifestations emitted since startup.
    pub manifests_total: u64,
    /// Total seeds sown since startup.
    pub seeds_total: u64,
}

#[derive(Debug, Clone, Copy)]
struct TrackedSeed {
    kind_hint: u32,
    label: SeedLabel,
    last_radiance_mag: f32,
    sown_frame: u64,
}

impl Default for ManifestationDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ManifestationDetector {
    /// Construct an empty detector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracked: HashMap::new(),
            recent_events: VecDeque::with_capacity(RECENT_MANIFEST_RING_CAP),
            manifests_total: 0,
            seeds_total: 0,
        }
    }

    /// Register a list of just-stamped seed-cells. Each entry is
    /// `(MortonKey, kind_hint, label)` from `stamp_seed_cells_into_field`.
    /// `frame` is the host's current frame counter.
    pub fn register_seeds(
        &mut self,
        sown: &[(MortonKey, u32, SeedLabel)],
        frame: u64,
    ) {
        for (key, kind, label) in sown {
            self.tracked.insert(
                *key,
                TrackedSeed {
                    kind_hint: *kind,
                    label: *label,
                    last_radiance_mag: 0.0,
                    sown_frame: frame,
                },
            );
            self.seeds_total = self.seeds_total.saturating_add(1);
        }
    }

    /// Scan the field for tracked-cell radiance rising-edge crossings.
    /// For each tracked cell that has crossed `MANIFESTATION_THRESHOLD`
    /// since the last scan, emit a `ManifestationEvent`. Caps the per-call
    /// emission at `MAX_MANIFESTATIONS_PER_FRAME` to avoid spawn-spam.
    ///
    /// The detector REMOVES manifested cells from `tracked` so each seed
    /// fires at most once.
    pub fn scan_rising_edges(
        &mut self,
        field: &OmegaField,
        frame: u64,
    ) -> Vec<ManifestationEvent> {
        let mut events = Vec::new();
        let mut to_remove = Vec::new();

        for (key, seed) in self.tracked.iter_mut() {
            let cell = field.cell_opt(*key).unwrap_or_default();
            let (r, g, b) = decode_radiance_probe(cell.radiance_probe_lo);
            let mag = r + g + b;
            // Rising-edge : was below threshold + now above.
            if seed.last_radiance_mag <= MANIFESTATION_THRESHOLD
                && mag > MANIFESTATION_THRESHOLD
            {
                let (mx, my, mz) = key.decode();
                let world_pos = [
                    crate::cfer_render::morton_axis_to_world(mx, 0),
                    crate::cfer_render::morton_axis_to_world(my, 1),
                    crate::cfer_render::morton_axis_to_world(mz, 2),
                ];
                events.push(ManifestationEvent {
                    frame,
                    sown_frame: seed.sown_frame,
                    world_pos,
                    kind: seed.kind_hint,
                    radiance_mag: mag,
                    density: cell.density,
                    label: seed.label,
                });
                to_remove.push(*key);
                if events.len() >= MAX_MANIFESTATIONS_PER_FRAME {
                    break;
                }
            } else {
                seed.last_radiance_mag = mag;
            }
        }

        for k in to_remove {
            self.tracked.remove(&k);
        }

        for ev in &events {
            self.push_recent(*ev);
            self.manifests_total = self.manifests_total.saturating_add(1);
        }

        events
    }

    /// Tracker count (for tests + telemetry).
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.tracked.len()
    }

    /// Recent-events snapshot in oldest-first order.
    #[must_use]
    pub fn recent_events_vec(&self) -> Vec<ManifestationEvent> {
        self.recent_events.iter().copied().collect()
    }

    fn push_recent(&mut self, ev: ManifestationEvent) {
        if self.recent_events.len() >= RECENT_MANIFEST_RING_CAP {
            self.recent_events.pop_front();
        }
        self.recent_events.push_back(ev);
    }

    /// Mark a seed as already at-or-above threshold (used by tests : when a
    /// test stamps a seed that ALREADY exceeds the threshold and we want to
    /// confirm the detector treats subsequent steady-state as no-op).
    pub fn prime_last_mag(&mut self, key: MortonKey, mag: f32) {
        if let Some(t) = self.tracked.get_mut(&key) {
            t.last_radiance_mag = mag;
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § High-level entry point : sow a single intent (used by MCP + FFI).
// ──────────────────────────────────────────────────────────────────────────

/// Outcome of `sow_intent` : seeds-emitted + the originating Morton-key list
/// for the manifestation-detector. Caller logs + emits a structured event.
#[derive(Debug, Clone, Default)]
pub struct SowOutcome {
    pub seeds: Vec<SeedCell>,
    pub stamped: Vec<(MortonKey, u32, SeedLabel)>,
    pub origin: [f32; 3],
    pub manifestation_window_frames: u32,
}

/// Sow an intent text into the field at `origin` :
///   1. Convert text → seeds via `intent_to_seed_cells`.
///   2. Stamp each seed into `field` via `stamp_seed_cells_into_field`.
///   3. Return `SowOutcome` so the caller can register seeds with a
///      detector + emit structured logs.
///
/// This is the canonical Stage-0 entry point. Both the FFI surface
/// (`__cssl_world_spontaneous_seed`) and the MCP tool
/// (`world.spontaneous_seed`) call this.
pub fn sow_intent(
    field: &mut OmegaField,
    text: &str,
    origin: [f32; 3],
) -> SowOutcome {
    let seeds = intent_to_seed_cells(text, origin);
    let stamped = stamp_seed_cells_into_field(field, &seeds);
    SowOutcome {
        seeds,
        stamped,
        origin,
        manifestation_window_frames: MANIFESTATION_WINDOW_FRAMES,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Convenience : check if a world-XZ position is inside the Spontaneous-Pad.
// ──────────────────────────────────────────────────────────────────────────

/// True iff `(wx, wz)` is inside the Spontaneous-Pad zone (NE corner of
/// ScaleRoom). Used by the host to surface a HUD hint when the player
/// stands on the pad.
#[must_use]
pub fn position_is_in_spontaneous_pad(wx: f32, wy: f32, wz: f32) -> bool {
    let pad = SPONTANEOUS_PAD_CENTER;
    let half = SPONTANEOUS_PAD_HALF_EXTENT;
    let dx = wx - pad[0];
    let dz = wz - pad[2];
    dx * dx + dz * dz <= half * half
        && wy >= 0.0
        && wy <= 12.0
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. intent_to_seed_cells_cube_returns_seed_with_cube_hint ──
    #[test]
    fn intent_to_seed_cells_cube_returns_seed_with_cube_hint() {
        let seeds = intent_to_seed_cells("a glass cube on the floor", [0.0, 1.0, 0.0]);
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].kind_hint, 0); // 0 = glass_cube
        assert_eq!(seeds[0].label.as_str(), "cube");
        // Position clamps to world envelope.
        assert!(seeds[0].pos[0] >= WORLD_MIN[0]);
        assert!(seeds[0].pos[0] <= WORLD_MAX[0]);
        // Density + radiance non-zero.
        assert!(seeds[0].density > 0.0);
        assert!(seeds[0].radiance.iter().sum::<f32>() > 0.0);
    }

    // ── 2. intent_to_seed_cells_empty_returns_empty ──
    #[test]
    fn intent_to_seed_cells_empty_returns_empty() {
        assert_eq!(
            intent_to_seed_cells("", [0.0, 1.0, 0.0]).len(),
            0
        );
        assert_eq!(
            intent_to_seed_cells("   \t\n", [0.0, 1.0, 0.0]).len(),
            0
        );
        // No keywords → empty.
        assert_eq!(
            intent_to_seed_cells("the quick brown fox", [0.0, 1.0, 0.0]).len(),
            0
        );
    }

    // ── 3. seed_cell_pos_clamps_to_world_bounds ──
    #[test]
    fn seed_cell_pos_clamps_to_world_bounds() {
        // Origin far outside the envelope — `SeedCell::new` clamps.
        let s = SeedCell::new(
            [9999.0, 9999.0, 9999.0],
            [0.5, 0.5, 0.5],
            0.5,
            0,
            "test",
        );
        assert!(s.pos[0] <= WORLD_MAX[0]);
        assert!(s.pos[1] <= WORLD_MAX[1]);
        assert!(s.pos[2] <= WORLD_MAX[2]);
        // Negative side too.
        let s = SeedCell::new(
            [-9999.0, -9999.0, -9999.0],
            [0.5, 0.5, 0.5],
            0.5,
            0,
            "test",
        );
        assert!(s.pos[0] >= WORLD_MIN[0]);
        assert!(s.pos[1] >= WORLD_MIN[1]);
        assert!(s.pos[2] >= WORLD_MIN[2]);
    }

    // ── 4. manifestation_detector_finds_rising_edge ──
    #[test]
    fn manifestation_detector_finds_rising_edge() {
        let mut field = OmegaField::new();
        let mut det = ManifestationDetector::new();
        // Stamp a sphere (kind 1) at (0, 1.5, 0). After stamping, the
        // cell's radiance magnitude (0.85+0.55+0.30) clearly exceeds the
        // threshold (0.45) so the very first scan should emit a
        // manifestation event.
        let seeds = intent_to_seed_cells("a bronze sphere", [0.0, 1.5, 0.0]);
        let stamped = stamp_seed_cells_into_field(&mut field, &seeds);
        det.register_seeds(&stamped, 100);
        assert_eq!(det.tracked_count(), 1);

        // First scan : radiance went from "0 (last)" → "1.7 (now)" so
        // rising-edge fires.
        let events = det.scan_rising_edges(&field, 101);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, 1);
        assert!(events[0].radiance_mag > MANIFESTATION_THRESHOLD);
        assert!(events[0].label.as_str().contains("sphere"));
        assert_eq!(det.manifests_total, 1);
        // Detector cleared the seed once it manifested.
        assert_eq!(det.tracked_count(), 0);

        // Second scan : nothing tracked → empty.
        let events = det.scan_rising_edges(&field, 102);
        assert_eq!(events.len(), 0);
    }

    // ── 5. manifestation_detector_ignores_steady_high ──
    #[test]
    fn manifestation_detector_ignores_steady_high() {
        let mut field = OmegaField::new();
        let mut det = ManifestationDetector::new();
        let seeds = intent_to_seed_cells("a glass cube", [0.0, 1.5, 0.0]);
        let stamped = stamp_seed_cells_into_field(&mut field, &seeds);
        det.register_seeds(&stamped, 0);

        // Prime the tracked seed's last_radiance to a value ALREADY above
        // threshold so the next scan sees "steady high" rather than a
        // rising edge — no manifestation should fire.
        for (k, _, _) in &stamped {
            det.prime_last_mag(*k, 5.0);
        }
        let events = det.scan_rising_edges(&field, 1);
        assert_eq!(events.len(), 0);
        // Steady-state cell remains tracked (no rising-edge → no removal).
        assert_eq!(det.tracked_count(), 1);
    }

    // ── 6. mcp_world_spontaneous_seed_returns_ok-style smoke ──
    //
    // We don't have a live MCP harness here ; we test the high-level
    // `sow_intent` wrapper that the MCP handler delegates to.
    #[test]
    fn sow_intent_returns_seeds_and_stamps_field() {
        let mut field = OmegaField::new();
        let outcome = sow_intent(&mut field, "a cube and a sphere", [0.0, 1.5, 0.0]);
        assert_eq!(outcome.seeds.len(), 2);
        assert_eq!(outcome.stamped.len(), 2);
        assert_eq!(
            outcome.manifestation_window_frames,
            MANIFESTATION_WINDOW_FRAMES
        );
        assert_eq!(outcome.origin, [0.0, 1.5, 0.0]);
        // Field should now have ≥ 2 cells (perhaps merged if seed offsets
        // collide on the same Morton-cell, but with the default offsets they
        // don't).
        assert!(field.dense_cell_count() >= 2);
    }

    // ── 7. dedup : same kind from two keywords yields one seed ──
    #[test]
    fn intent_dedups_same_kind_across_keywords() {
        let seeds = intent_to_seed_cells("a cube and a box", [0.0, 1.0, 0.0]);
        // "cube" + "box" both map to kind_hint 0 → dedup to 1 seed.
        assert_eq!(seeds.len(), 1);
    }

    // ── 8. word boundary : "cubes" should NOT match "cube" ──
    #[test]
    fn word_boundary_check_avoids_substring_false_positives() {
        assert!(!word_contains("the cubes are stacked", "cube"));
        assert!(word_contains("a cube here", "cube"));
        assert!(word_contains("cube", "cube"));
        assert!(word_contains("cube.", "cube"));
        assert!(word_contains("cube,", "cube"));
        assert!(!word_contains("cubeworld", "cube"));
    }

    // ── 9. spontaneous-pad geometry : center + half-extent ──
    #[test]
    fn spontaneous_pad_position_check() {
        // Center of the pad : inside.
        assert!(position_is_in_spontaneous_pad(
            SPONTANEOUS_PAD_CENTER[0],
            SPONTANEOUS_PAD_CENTER[1],
            SPONTANEOUS_PAD_CENTER[2],
        ));
        // 10m away : outside.
        assert!(!position_is_in_spontaneous_pad(
            SPONTANEOUS_PAD_CENTER[0] + 10.0,
            SPONTANEOUS_PAD_CENTER[1],
            SPONTANEOUS_PAD_CENTER[2],
        ));
    }

    // ── 10. label round-trip ──
    #[test]
    fn seed_label_round_trip() {
        let l = SeedLabel::from_str("sphere");
        assert_eq!(l.as_str(), "sphere");
        // Truncates beyond 24 bytes.
        let long = "a".repeat(50);
        let l = SeedLabel::from_str(&long);
        assert_eq!(l.as_str().len(), 24);
        // Empty label.
        let l = SeedLabel::default();
        assert_eq!(l.as_str(), "");
    }

    // ── 11. recent_events ring caps at RECENT_MANIFEST_RING_CAP ──
    #[test]
    fn recent_events_ring_caps_at_capacity() {
        let mut det = ManifestationDetector::new();
        // Push 20 fake events ; ring should cap at 16.
        for i in 0..20 {
            det.push_recent(ManifestationEvent {
                frame: i,
                sown_frame: 0,
                world_pos: [0.0, 0.0, 0.0],
                kind: 0,
                radiance_mag: 1.0,
                density: 0.5,
                label: SeedLabel::from_str("test"),
            });
        }
        assert_eq!(det.recent_events.len(), RECENT_MANIFEST_RING_CAP);
        // Oldest should have been evicted (frame 0 → frame 4).
        assert_eq!(det.recent_events.front().unwrap().frame, 4);
    }

    // ── 12. multi-keyword intent stamps multiple cells ──
    #[test]
    fn multi_keyword_intent_stamps_multiple_field_cells() {
        let mut field = OmegaField::new();
        let outcome = sow_intent(
            &mut field,
            "a cube and a sphere and a torus and a cone",
            [0.0, 1.5, 0.0],
        );
        // 4 distinct kind-hints → 4 seeds.
        assert_eq!(outcome.seeds.len(), 4);
        assert_eq!(outcome.stamped.len(), 4);
    }
}
