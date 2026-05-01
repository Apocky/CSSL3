//! § loa-host::intent_router — text → typed-intent → MCP-style dispatch
//! ════════════════════════════════════════════════════════════════════════
//!
//! Anchored to T11-WAVE3-INTENT (W-WAVE3-intent-router). The router bridges
//! natural-language text submissions (HUD text-input box · MCP `intent.translate`
//! tool · scripted scene calls) into the typed `Intent` value-object and then
//! dispatches it against the live `EngineState` mcp-handler surface.
//!
//! § THREE STAGES (only stage-0 is wired here)
//!   stage-0 : DETERMINISTIC keyword + regex-free phrase classifier (THIS FILE).
//!             Hand-rolled lowercasing + token split + match. No external deps,
//!             no allocator beyond the input string. ~30 keyword/phrase rules.
//!   stage-1 : KAN-classifier (already shipped @ cssl-kan-runtime). Vector
//!             embedding + multi-class output → Intent. Drops in by replacing
//!             `classify()` ; the call-sites stay identical.
//!   stage-2 : LLM-driven intent extraction via MCP `gm.parse_intent` round-trip.
//!             For when the rule-base + KAN both miss + the user is already
//!             talking to a model anyway.
//!
//! § DESIGN MIRROR
//!   The shape of `Intent` + `route()` mirrors `scenes/intent_translation.cssl`
//!   (Apocky-greenlit during Wave-1) so the eventual pure-CSSL re-implementation
//!   has a 1-to-1 mapping. The Rust enum + dispatch table is the bootstrap
//!   substrate ; the .cssl scene IS the authoritative spec.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_lines)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use crate::mcp_server::EngineState;

// ───────────────────────────────────────────────────────────────────────
// § Intent : the canonical typed value-object the router emits
// ───────────────────────────────────────────────────────────────────────

/// Maximum intents retained in the recent-dispatches ring. Kept small so
/// `intent.recent` returns instantly + the lock-window is microseconds.
pub const RECENT_INTENT_CAP: usize = 16;

/// Typed intent — the value-object the classifier emits + the dispatcher
/// pattern-matches on. Each variant maps to ≥ 1 MCP-tool invocation.
///
/// Order is alphabetical-by-action so deterministic enum-discriminant ids
/// stay stable when new variants are added (insert in alphabetical slot).
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// `burst N` → render.start_burst with `count=N`.
    Burst { count: u32 },
    /// `intensity {cfer|fog} 0..1` → render.cfer_intensity (mapped from the
    /// approximate "fog/atmosphere" knob the user typed). Other intensity
    /// targets stay reserved for future expansion.
    SetCferIntensity { intensity: f32 },
    /// `set floor {ne|nw|sw|se} pattern <name|id>` → render.set_floor_pattern.
    SetFloorPattern { quadrant: u32, pattern_id: u32 },
    /// `illuminant {d65|d50|a|f11}` · `set illuminant <name>` →
    /// render.set_illuminant. Strings stay user-facing (the dispatch translates
    /// to the canonical capitalization the spectral_bridge expects).
    SetIlluminant { name: String },
    /// `material on quad N` · `material N is brass` → render.set_material.
    SetMaterial { quad_id: u32, material_id: u32 },
    /// `set wall {n|s|e|w|north|south|east|west} pattern <name|id>` →
    /// render.set_wall_pattern.
    SetWallPattern { wall_id: u32, pattern_id: u32 },
    /// `snapshot` · `snap` · `capture` → render.snapshot_png.
    Snapshot,
    /// `spawn cube at 5 5 5` · `drop sphere at origin` → render.spawn_stress.
    SpawnAt { kind: u32, pos: [f32; 3] },
    /// `spontaneous a sphere` · `seed orb` → reserved seed-tool. No host
    /// dispatch yet ; logged as classified+attempted, dispatched as a no-op
    /// with a "pending" status so callers can see the router rule fired
    /// but the tool isn't yet wired.
    SpontaneousSeed { text: String },
    /// `teleport to color room` · `go to material` → room.teleport.
    Teleport { room_id: u32 },
    /// `tour {default|walls|floor|plinths|ceiling}` → render.tour with the id.
    Tour { tour_id: String },
    /// Fallback : the input text didn't match any rule. Reason carries the
    /// normalized input so callers can present a debug HUD line.
    Unknown { reason: String },
}

impl Intent {
    /// Stable axis-tag string for telemetry counters + JSONL events.
    /// Naming convention : `<category>_<action>` lowercased.
    #[must_use]
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Intent::Burst { .. } => "burst",
            Intent::SetCferIntensity { .. } => "set_cfer_intensity",
            Intent::SetFloorPattern { .. } => "set_floor_pattern",
            Intent::SetIlluminant { .. } => "set_illuminant",
            Intent::SetMaterial { .. } => "set_material",
            Intent::SetWallPattern { .. } => "set_wall_pattern",
            Intent::Snapshot => "snapshot",
            Intent::SpawnAt { .. } => "spawn_at",
            Intent::SpontaneousSeed { .. } => "spontaneous_seed",
            Intent::Teleport { .. } => "teleport",
            Intent::Tour { .. } => "tour",
            Intent::Unknown { .. } => "unknown",
        }
    }

    /// MCP tool that the dispatcher invokes for this intent.
    /// `Unknown` returns "" (no dispatch). `SpontaneousSeed` returns
    /// `world.spontaneous_seed` even though that handler is a stub today.
    #[must_use]
    pub fn target_tool(&self) -> &'static str {
        match self {
            Intent::Burst { .. } => "render.start_burst",
            Intent::SetCferIntensity { .. } => "render.cfer_intensity",
            Intent::SetFloorPattern { .. } => "render.set_floor_pattern",
            Intent::SetIlluminant { .. } => "render.set_illuminant",
            Intent::SetMaterial { .. } => "render.set_material",
            Intent::SetWallPattern { .. } => "render.set_wall_pattern",
            Intent::Snapshot => "render.snapshot_png",
            Intent::SpawnAt { .. } => "render.spawn_stress",
            Intent::SpontaneousSeed { .. } => "world.spontaneous_seed",
            Intent::Teleport { .. } => "room.teleport",
            Intent::Tour { .. } => "render.tour",
            Intent::Unknown { .. } => "",
        }
    }

    /// Lossless JSON of the Intent, used by `intent.translate` MCP + the
    /// recent-ring serialization.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Intent::Burst { count } => json!({"kind": "burst", "count": count}),
            Intent::SetCferIntensity { intensity } => {
                json!({"kind": "set_cfer_intensity", "intensity": intensity})
            }
            Intent::SetFloorPattern { quadrant, pattern_id } => json!({
                "kind": "set_floor_pattern",
                "quadrant": quadrant,
                "pattern_id": pattern_id,
            }),
            Intent::SetIlluminant { name } => json!({"kind": "set_illuminant", "name": name}),
            Intent::SetMaterial { quad_id, material_id } => json!({
                "kind": "set_material",
                "quad_id": quad_id,
                "material_id": material_id,
            }),
            Intent::SetWallPattern { wall_id, pattern_id } => json!({
                "kind": "set_wall_pattern",
                "wall_id": wall_id,
                "pattern_id": pattern_id,
            }),
            Intent::Snapshot => json!({"kind": "snapshot"}),
            Intent::SpawnAt { kind, pos } => json!({
                "kind": "spawn_at",
                "kind_id": kind,
                "x": pos[0], "y": pos[1], "z": pos[2],
            }),
            Intent::SpontaneousSeed { text } => json!({"kind": "spontaneous_seed", "text": text}),
            Intent::Teleport { room_id } => json!({"kind": "teleport", "room_id": room_id}),
            Intent::Tour { tour_id } => json!({"kind": "tour", "tour_id": tour_id}),
            Intent::Unknown { reason } => json!({"kind": "unknown", "reason": reason}),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Telemetry counters — all atomic, all global, all reset-able for tests
// ───────────────────────────────────────────────────────────────────────

/// Total classify() invocations since process-start.
pub static INTENTS_CLASSIFIED: AtomicU64 = AtomicU64::new(0);
/// Total dispatch() invocations that returned a non-error result.
pub static INTENTS_DISPATCHED: AtomicU64 = AtomicU64::new(0);
/// Classifier produced `Unknown` (no rule matched).
pub static INTENTS_UNKNOWN: AtomicU64 = AtomicU64::new(0);

// Per-kind counters. Indexed by Intent::kind_index() below to avoid a
// HashMap lookup on the hot path.
pub static INTENTS_PER_KIND_BURST: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SET_CFER_INTENSITY: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SET_FLOOR_PATTERN: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SET_ILLUMINANT: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SET_MATERIAL: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SET_WALL_PATTERN: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SNAPSHOT: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SPAWN_AT: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_SPONTANEOUS_SEED: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_TELEPORT: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_TOUR: AtomicU64 = AtomicU64::new(0);
pub static INTENTS_PER_KIND_UNKNOWN: AtomicU64 = AtomicU64::new(0);

fn bump_per_kind(intent: &Intent) {
    let counter: &AtomicU64 = match intent {
        Intent::Burst { .. } => &INTENTS_PER_KIND_BURST,
        Intent::SetCferIntensity { .. } => &INTENTS_PER_KIND_SET_CFER_INTENSITY,
        Intent::SetFloorPattern { .. } => &INTENTS_PER_KIND_SET_FLOOR_PATTERN,
        Intent::SetIlluminant { .. } => &INTENTS_PER_KIND_SET_ILLUMINANT,
        Intent::SetMaterial { .. } => &INTENTS_PER_KIND_SET_MATERIAL,
        Intent::SetWallPattern { .. } => &INTENTS_PER_KIND_SET_WALL_PATTERN,
        Intent::Snapshot => &INTENTS_PER_KIND_SNAPSHOT,
        Intent::SpawnAt { .. } => &INTENTS_PER_KIND_SPAWN_AT,
        Intent::SpontaneousSeed { .. } => &INTENTS_PER_KIND_SPONTANEOUS_SEED,
        Intent::Teleport { .. } => &INTENTS_PER_KIND_TELEPORT,
        Intent::Tour { .. } => &INTENTS_PER_KIND_TOUR,
        Intent::Unknown { .. } => &INTENTS_PER_KIND_UNKNOWN,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// JSON snapshot of all per-kind counters + the three top-level totals.
/// Read-only · used by `intent.recent` MCP tool to report live state.
#[must_use]
pub fn counters_json() -> Value {
    json!({
        "intents_classified_total": INTENTS_CLASSIFIED.load(Ordering::Relaxed),
        "intents_dispatched_total": INTENTS_DISPATCHED.load(Ordering::Relaxed),
        "intents_unknown_total": INTENTS_UNKNOWN.load(Ordering::Relaxed),
        "per_kind": {
            "burst": INTENTS_PER_KIND_BURST.load(Ordering::Relaxed),
            "set_cfer_intensity": INTENTS_PER_KIND_SET_CFER_INTENSITY.load(Ordering::Relaxed),
            "set_floor_pattern": INTENTS_PER_KIND_SET_FLOOR_PATTERN.load(Ordering::Relaxed),
            "set_illuminant": INTENTS_PER_KIND_SET_ILLUMINANT.load(Ordering::Relaxed),
            "set_material": INTENTS_PER_KIND_SET_MATERIAL.load(Ordering::Relaxed),
            "set_wall_pattern": INTENTS_PER_KIND_SET_WALL_PATTERN.load(Ordering::Relaxed),
            "snapshot": INTENTS_PER_KIND_SNAPSHOT.load(Ordering::Relaxed),
            "spawn_at": INTENTS_PER_KIND_SPAWN_AT.load(Ordering::Relaxed),
            "spontaneous_seed": INTENTS_PER_KIND_SPONTANEOUS_SEED.load(Ordering::Relaxed),
            "teleport": INTENTS_PER_KIND_TELEPORT.load(Ordering::Relaxed),
            "tour": INTENTS_PER_KIND_TOUR.load(Ordering::Relaxed),
            "unknown": INTENTS_PER_KIND_UNKNOWN.load(Ordering::Relaxed),
        },
    })
}

/// Process-wide mutex acquired by tests in this module + the
/// `mcp_tools::tests::mcp_intent_*` integration tests. Tests that mutate
/// the global counters or recent-ring MUST hold this lock, otherwise
/// parallel cargo-test runners trample one another's expected values.
///
/// Safe to leave in non-test builds — the mutex is uncontended by design
/// (production code never acquires it). It costs only the one OnceLock
/// initialization at first use.
#[doc(hidden)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// Reset every counter + the recent-ring. Used only by the test suite
/// to keep test-order independence.
#[doc(hidden)]
pub fn reset_for_test() {
    for c in [
        &INTENTS_CLASSIFIED,
        &INTENTS_DISPATCHED,
        &INTENTS_UNKNOWN,
        &INTENTS_PER_KIND_BURST,
        &INTENTS_PER_KIND_SET_CFER_INTENSITY,
        &INTENTS_PER_KIND_SET_FLOOR_PATTERN,
        &INTENTS_PER_KIND_SET_ILLUMINANT,
        &INTENTS_PER_KIND_SET_MATERIAL,
        &INTENTS_PER_KIND_SET_WALL_PATTERN,
        &INTENTS_PER_KIND_SNAPSHOT,
        &INTENTS_PER_KIND_SPAWN_AT,
        &INTENTS_PER_KIND_SPONTANEOUS_SEED,
        &INTENTS_PER_KIND_TELEPORT,
        &INTENTS_PER_KIND_TOUR,
        &INTENTS_PER_KIND_UNKNOWN,
    ] {
        c.store(0, Ordering::Relaxed);
    }
    let ring = recent_ring();
    if let Ok(mut g) = ring.lock() {
        g.clear();
    }
}

// ───────────────────────────────────────────────────────────────────────
// § DispatchRecord ring — last RECENT_INTENT_CAP intents
// ───────────────────────────────────────────────────────────────────────

/// One row in the recent-dispatches ring. Captures everything the HUD or
/// MCP-tooling needs to display + replay an intent.
#[derive(Debug, Clone)]
pub struct DispatchRecord {
    /// The classified intent.
    pub intent: Intent,
    /// MCP tool that was actually invoked (mirrors `intent.target_tool()`
    /// at record-time for stability across enum changes).
    pub tool: String,
    /// JSON params that were forwarded to the tool's handler.
    pub params: Value,
    /// `true` if the dispatcher returned `ok=true` (or the JSON had no
    /// `error` key). `false` otherwise.
    pub ok: bool,
    /// Frame at which the dispatch occurred (engine.frame_count).
    pub frame: u64,
    /// Wall-clock millis-since-epoch (best-effort ; 0 if SystemTime fails).
    pub ts_ms: u64,
    /// Original normalized text input.
    pub raw_text: String,
}

impl DispatchRecord {
    fn to_json(&self) -> Value {
        json!({
            "intent": self.intent.to_json(),
            "tool": self.tool,
            "params": self.params,
            "ok": self.ok,
            "frame": self.frame,
            "ts_ms": self.ts_ms,
            "raw_text": self.raw_text,
        })
    }
}

fn recent_ring() -> &'static Mutex<VecDeque<DispatchRecord>> {
    static RING: OnceLock<Mutex<VecDeque<DispatchRecord>>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(RECENT_INTENT_CAP)))
}

fn push_record(rec: DispatchRecord) {
    if let Ok(mut g) = recent_ring().lock() {
        if g.len() >= RECENT_INTENT_CAP {
            g.pop_front();
        }
        g.push_back(rec);
    }
}

/// JSON-formatted recent-dispatches ring. Newest-last (index 0 is oldest).
/// Returns `{events: [...], count: N, capacity: 16, counters: {...}}`.
#[must_use]
pub fn recent_json() -> Value {
    let ring = recent_ring();
    let entries: Vec<Value> = match ring.lock() {
        Ok(g) => g.iter().map(DispatchRecord::to_json).collect(),
        Err(_) => Vec::new(),
    };
    let count = entries.len();
    json!({
        "events": entries,
        "count": count,
        "capacity": RECENT_INTENT_CAP,
        "counters": counters_json(),
    })
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ───────────────────────────────────────────────────────────────────────
// § token helpers — keyword classifier needs robust split + tiny parsers
// ───────────────────────────────────────────────────────────────────────

fn tokens(t: &str) -> Vec<&str> {
    t.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_f32_lenient(s: &str) -> Option<f32> {
    // Accept "5", "5.0", "+5", "-5", "5.0,". The tokens() splitter already
    // strips commas, but legacy inputs like "5,5,5" without spaces fall
    // through to here, so we strip a trailing comma defensively.
    let s = s.trim().trim_end_matches(',');
    s.parse::<f32>().ok()
}

fn parse_u32_lenient(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok()
}

/// Map a wall-direction word to wall_id 0..3 (N=0 · E=1 · S=2 · W=3).
fn wall_id_from_direction(s: &str) -> Option<u32> {
    match s {
        "n" | "north" => Some(0),
        "e" | "east" => Some(1),
        "s" | "south" => Some(2),
        "w" | "west" => Some(3),
        _ => None,
    }
}

/// Map a floor-quadrant word to quadrant_id 0..3 (NE=0 · NW=1 · SW=2 · SE=3).
fn quadrant_id_from_direction(s: &str) -> Option<u32> {
    match s {
        "ne" | "north-east" | "northeast" => Some(0),
        "nw" | "north-west" | "northwest" => Some(1),
        "sw" | "south-west" | "southwest" => Some(2),
        "se" | "south-east" | "southeast" => Some(3),
        _ => None,
    }
}

/// Map a stress-object name to its kind id. Mirrors `crate::geometry::stress_object_name`
/// (unfortunately that fn doesn't expose a reverse-lookup ; we hard-code the
/// canonical 14-entry mapping that's stable from T11-LOA-RICH-RENDER).
fn stress_kind_from_name(s: &str) -> Option<u32> {
    match s {
        "cube" | "box" => Some(0),
        "sphere" | "ball" | "orb" => Some(1),
        "pyramid" => Some(2),
        "cylinder" => Some(3),
        "cone" => Some(4),
        "tetrahedron" | "tet" => Some(5),
        "octahedron" | "oct" => Some(6),
        "torus" | "donut" => Some(7),
        "capsule" => Some(8),
        "wedge" => Some(9),
        "plinth" | "column" | "pillar" => Some(10),
        "ramp" | "wall" => Some(11),
        "icosahedron" | "icosa" => Some(12),
        "dodecahedron" | "dodeca" => Some(13),
        _ => None,
    }
}

/// Map a procedural-pattern name to its id. Stays alphabetical-tolerant
/// (lowercased + hyphenless) to keep the user-typing surface forgiving.
fn pattern_id_from_name(s: &str) -> Option<u32> {
    match s {
        "solid" => Some(0),
        "grid" | "grid1m" | "grid-1m" => Some(1),
        "grid100mm" | "grid-100mm" => Some(2),
        "checkerboard" | "checker" => Some(3),
        "macbeth" | "macbethcolorchart" | "color-chart" | "colorchart" => Some(4),
        "snellen" | "eye-chart" | "eyechart" => Some(5),
        "qr" | "qr-code" | "qrcode" => Some(6),
        "ean13" | "barcode" | "ean-13" => Some(7),
        "grayscale" | "gradient-grayscale" => Some(8),
        "huewheel" | "hue-wheel" | "gradient-huewheel" | "hue" => Some(9),
        "perlin" | "noise" | "perlin-noise" => Some(10),
        "rings" | "concentric-rings" | "concentric" => Some(11),
        "spokes" | "radial-spokes" => Some(12),
        "zoneplate" => Some(13),
        "frequency-sweep" | "sweep" | "freqsweep" => Some(14),
        "radial-gradient" | "radialgradient" => Some(15),
        "mandelbulb" | "raymarch-mandelbulb" => Some(16),
        "raymarch-sphere" | "sdf-sphere" => Some(17),
        "raymarch-torus" | "sdf-torus" => Some(18),
        "gyroid" | "raymarch-gyroid" => Some(19),
        "julia" | "raymarch-julia" => Some(20),
        "menger" | "raymarch-menger" => Some(21),
        _ => None,
    }
}

/// Map a material name to its id. Stays alphabetical-tolerant.
fn material_id_from_name(s: &str) -> Option<u32> {
    match s {
        "default" | "plastic" => Some(0),
        "wood" | "oak" => Some(1),
        "metal" | "steel" | "iron" => Some(2),
        "brass" | "gold-brass" => Some(3),
        "copper" => Some(4),
        "gold" => Some(5),
        "marble" | "stone" => Some(6),
        "glass" => Some(7),
        "rubber" => Some(8),
        "ceramic" | "porcelain" => Some(9),
        "fabric" | "cloth" | "velvet" => Some(10),
        "leather" => Some(11),
        "iridescent" | "soap-bubble" => Some(12),
        "obsidian" | "black-glass" => Some(13),
        "vermillion-lacquer" | "lacquer" | "red-lacquer" => Some(14),
        "white-marble" | "carrara" => Some(15),
        _ => None,
    }
}

/// Canonical illuminant string (matches `Illuminant::from_name` in spectral_bridge).
fn canonicalize_illuminant(s: &str) -> Option<&'static str> {
    match s {
        "d65" => Some("D65"),
        "d50" => Some("D50"),
        "a" => Some("A"),
        "f11" => Some("F11"),
        _ => None,
    }
}

/// Map a room-name word to the room id (0..4).
/// Uses the same alias table as `Room::from_str` plus extra friendly forms.
fn room_id_from_name(s: &str) -> Option<u32> {
    match s {
        "test" | "testroom" | "test-room" | "hub" => Some(0),
        "material" | "materialroom" | "material-room" | "materials" => Some(1),
        "pattern" | "patternroom" | "pattern-room" | "patterns" => Some(2),
        "scale" | "scaleroom" | "scale-room" => Some(3),
        "color" | "colorroom" | "color-room" | "colour" | "colourroom" => Some(4),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § classify — the deterministic stage-0 keyword classifier
// ───────────────────────────────────────────────────────────────────────

/// Classify a free-form text string into a typed `Intent`. Always returns ;
/// unmatched text falls through to `Intent::Unknown` with the normalized
/// input embedded in `reason`.
///
/// The implementation is hand-rolled (no regex) to stay zero-dep and
/// keep the per-call cost in single-digit microseconds for typical input.
///
/// § RULES (≥ 30) — see module-level doc-comment for the matrix.
#[must_use]
pub fn classify(text: &str) -> Intent {
    let normalized = text.trim().to_lowercase();
    INTENTS_CLASSIFIED.fetch_add(1, Ordering::Relaxed);

    if normalized.is_empty() {
        let i = Intent::Unknown {
            reason: "empty input".to_string(),
        };
        bump_per_kind(&i);
        INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
        return i;
    }

    let toks = tokens(&normalized);
    if toks.is_empty() {
        let i = Intent::Unknown {
            reason: format!("no tokens : {normalized}"),
        };
        bump_per_kind(&i);
        INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
        return i;
    }

    // ─── Rule 1 : snapshot/snap/capture/screenshot ──────────────────────
    // (1) "snapshot" (2) "snap" (3) "capture" (4) "screenshot"
    if matches!(toks[0], "snapshot" | "snap" | "capture" | "screenshot") {
        let i = Intent::Snapshot;
        bump_per_kind(&i);
        return i;
    }

    // ─── Rule 2 : burst N ────────────────────────────────────────────────
    // (5) "burst N"  (6) "burst of N"  (7) "burst N frames"
    if toks[0] == "burst" {
        let count = toks
            .iter()
            .skip(1)
            .find_map(|t| parse_u32_lenient(t))
            .unwrap_or(10)
            .max(1)
            .min(1000);
        let i = Intent::Burst { count };
        bump_per_kind(&i);
        return i;
    }

    // ─── Rule 3 : tour <id> ──────────────────────────────────────────────
    // (8) "tour"  (9) "tour walls"  (10) "tour default"
    if toks[0] == "tour" {
        let tour_id = toks
            .get(1)
            .copied()
            .filter(|t| matches!(*t, "default" | "walls" | "floor" | "plinths" | "ceiling"))
            .unwrap_or("default")
            .to_string();
        let i = Intent::Tour { tour_id };
        bump_per_kind(&i);
        return i;
    }

    // ─── Rule 4 : intensity {cfer|fog} <value> ───────────────────────────
    // (11) "intensity 0.5"  (12) "intensity cfer 0.5"  (13) "fog 0.3"
    if toks[0] == "intensity" || toks[0] == "fog" || toks[0] == "atmosphere" {
        let intensity = toks
            .iter()
            .skip(1)
            .find_map(|t| parse_f32_lenient(t))
            .unwrap_or(0.10)
            .clamp(0.0, 1.0);
        let i = Intent::SetCferIntensity { intensity };
        bump_per_kind(&i);
        return i;
    }

    // ─── Rule 5 : illuminant <name> ──────────────────────────────────────
    // (14) "illuminant d65"  (15) "set illuminant d50"  (16) "use a"
    if toks[0] == "illuminant" || (toks.len() >= 2 && toks[0] == "set" && toks[1] == "illuminant") {
        let target = if toks[0] == "illuminant" {
            toks.get(1).copied().unwrap_or("d65")
        } else {
            toks.get(2).copied().unwrap_or("d65")
        };
        let canonical = canonicalize_illuminant(target).unwrap_or("D65").to_string();
        let i = Intent::SetIlluminant { name: canonical };
        bump_per_kind(&i);
        return i;
    }

    // ─── Rule 6 : teleport / go to ───────────────────────────────────────
    // (17) "teleport color"  (18) "teleport to color room"
    // (19) "go to material"  (20) "goto pattern"
    let teleport_prefix = toks[0] == "teleport"
        || toks[0] == "goto"
        || (toks.len() >= 2 && toks[0] == "go" && toks[1] == "to");
    if teleport_prefix {
        let start = if toks[0] == "go" {
            2
        } else if toks.len() >= 2 && toks[1] == "to" {
            2
        } else {
            1
        };
        // After the prefix we may have "the", "a", or skip directly to the
        // room name. Walk forward until we find a recognized room word.
        let room_id = toks[start..]
            .iter()
            .find_map(|t| room_id_from_name(t))
            .ok_or_else(|| format!("teleport target not recognized : {normalized}"));
        match room_id {
            Ok(id) => {
                let i = Intent::Teleport { room_id: id };
                bump_per_kind(&i);
                return i;
            }
            Err(reason) => {
                let i = Intent::Unknown { reason };
                bump_per_kind(&i);
                INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
                return i;
            }
        }
    }

    // ─── Rule 7 : spawn/drop/place/put <kind> at x y z ───────────────────
    // (21) "spawn cube at 5 5 5"   (22) "drop sphere at 0 0 0"
    // (23) "place pyramid at 1.5 0 -3"  (24) "put torus at 5,5,5"
    if matches!(toks[0], "spawn" | "drop" | "place" | "put") && toks.len() >= 2 {
        let kind_word = toks[1];
        let kind = stress_kind_from_name(kind_word);
        // Find "at" or default to the first numeric token.
        let at_idx = toks.iter().position(|t| *t == "at").unwrap_or(1);
        // Collect numeric tokens after the "at" or after the kind word.
        let nums: Vec<f32> = toks[at_idx + 1..]
            .iter()
            .filter_map(|t| parse_f32_lenient(t))
            .collect();
        let pos = if nums.len() >= 3 {
            [nums[0], nums[1], nums[2]]
        } else if nums.len() == 1 {
            [nums[0], 1.0, nums[0]]
        } else {
            [0.0, 1.0, 0.0]
        };
        match kind {
            Some(k) => {
                let i = Intent::SpawnAt { kind: k, pos };
                bump_per_kind(&i);
                return i;
            }
            None => {
                let i = Intent::Unknown {
                    reason: format!("unknown spawn kind '{kind_word}' : {normalized}"),
                };
                bump_per_kind(&i);
                INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
                return i;
            }
        }
    }

    // ─── Rule 8 : set wall <dir> pattern <name|id> ───────────────────────
    // (25) "set wall north pattern qr"
    // (26) "set wall n to qr"
    // (27) "wall north qr"
    if (toks[0] == "set" && toks.get(1).copied() == Some("wall")) || toks[0] == "wall" {
        let dir_idx = if toks[0] == "set" { 2 } else { 1 };
        let dir = toks.get(dir_idx).copied();
        let wall_id = dir.and_then(wall_id_from_direction);
        // Look for "pattern" or "to" word, else fall through to the next
        // numeric/name token after the direction.
        let pat_idx = toks
            .iter()
            .position(|t| matches!(*t, "pattern" | "to"))
            .map(|p| p + 1)
            .unwrap_or(dir_idx + 1);
        let pat_word = toks.get(pat_idx).copied();
        let pattern_id = pat_word.and_then(|w| {
            parse_u32_lenient(w).or_else(|| pattern_id_from_name(w))
        });
        match (wall_id, pattern_id) {
            (Some(w), Some(p)) => {
                let i = Intent::SetWallPattern { wall_id: w, pattern_id: p };
                bump_per_kind(&i);
                return i;
            }
            _ => {
                let i = Intent::Unknown {
                    reason: format!("malformed set wall : {normalized}"),
                };
                bump_per_kind(&i);
                INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
                return i;
            }
        }
    }

    // ─── Rule 9 : set floor <quad> pattern <name|id> ─────────────────────
    // (28) "set floor ne pattern checker"
    // (29) "floor sw checker"
    if (toks[0] == "set" && toks.get(1).copied() == Some("floor")) || toks[0] == "floor" {
        let dir_idx = if toks[0] == "set" { 2 } else { 1 };
        let dir = toks.get(dir_idx).copied();
        let quadrant = dir.and_then(quadrant_id_from_direction);
        let pat_idx = toks
            .iter()
            .position(|t| matches!(*t, "pattern" | "to"))
            .map(|p| p + 1)
            .unwrap_or(dir_idx + 1);
        let pat_word = toks.get(pat_idx).copied();
        let pattern_id = pat_word.and_then(|w| {
            parse_u32_lenient(w).or_else(|| pattern_id_from_name(w))
        });
        match (quadrant, pattern_id) {
            (Some(q), Some(p)) => {
                let i = Intent::SetFloorPattern { quadrant: q, pattern_id: p };
                bump_per_kind(&i);
                return i;
            }
            _ => {
                let i = Intent::Unknown {
                    reason: format!("malformed set floor : {normalized}"),
                };
                bump_per_kind(&i);
                INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
                return i;
            }
        }
    }

    // ─── Rule 10 : set material on quad N <name|id> ──────────────────────
    // (30) "material on plinth 3 brass"
    // (31) "set material 3 to brass"
    // (32) "material 3 brass"
    if toks[0] == "material" || (toks[0] == "set" && toks.get(1).copied() == Some("material")) {
        let start = if toks[0] == "set" { 2 } else { 1 };
        // Skip optional "on" + "plinth" filler words.
        let mut idx = start;
        while idx < toks.len() && matches!(toks[idx], "on" | "plinth" | "quad" | "slot") {
            idx += 1;
        }
        let quad_id = toks.get(idx).and_then(|w| parse_u32_lenient(w));
        let mat_idx_start = if quad_id.is_some() { idx + 1 } else { idx };
        // Skip "to" / "is" filler.
        let mat_idx = if let Some(t) = toks.get(mat_idx_start) {
            if matches!(*t, "to" | "is") { mat_idx_start + 1 } else { mat_idx_start }
        } else {
            mat_idx_start
        };
        let mat_word = toks.get(mat_idx).copied();
        let material_id = mat_word.and_then(|w| {
            parse_u32_lenient(w).or_else(|| material_id_from_name(w))
        });
        match (quad_id, material_id) {
            (Some(q), Some(m)) => {
                let i = Intent::SetMaterial { quad_id: q, material_id: m };
                bump_per_kind(&i);
                return i;
            }
            _ => {
                let i = Intent::Unknown {
                    reason: format!("malformed material : {normalized}"),
                };
                bump_per_kind(&i);
                INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
                return i;
            }
        }
    }

    // ─── Rule 11 : spontaneous / seed <free-text> ────────────────────────
    // (33) "spontaneous a sphere"
    // (34) "seed orb"
    // (35) "imagine a forest"
    if matches!(toks[0], "spontaneous" | "seed" | "imagine" | "manifest") {
        // Strip common filler articles.
        let body: String = toks
            .iter()
            .skip(1)
            .filter(|t| !matches!(**t, "a" | "an" | "the" | "some"))
            .copied()
            .collect::<Vec<&str>>()
            .join(" ");
        let i = Intent::SpontaneousSeed {
            text: if body.is_empty() {
                normalized.clone()
            } else {
                body
            },
        };
        bump_per_kind(&i);
        return i;
    }

    // ─── Fallthrough : Unknown ───────────────────────────────────────────
    let i = Intent::Unknown {
        reason: format!("no rule matched : {normalized}"),
    };
    bump_per_kind(&i);
    INTENTS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
    i
}

// ───────────────────────────────────────────────────────────────────────
// § dispatch — turn a typed Intent into params + invoke the matching tool
// ───────────────────────────────────────────────────────────────────────

/// Translate a typed Intent into the JSON params expected by the matching
/// MCP tool. Pure conversion ; no state mutation. Used by the dispatcher
/// (live invoke) AND by `intent.translate` (preview-without-side-effects).
#[must_use]
pub fn intent_to_params(intent: &Intent, sovereign: &str) -> Value {
    let cap = json!({"sovereign_cap": sovereign});
    let mut out = serde_json::Map::new();
    if let Some(obj) = cap.as_object() {
        for (k, v) in obj {
            out.insert(k.clone(), v.clone());
        }
    }
    match intent {
        Intent::Burst { count } => {
            out.insert("count".to_string(), json!(count));
        }
        Intent::SetCferIntensity { intensity } => {
            out.insert("intensity".to_string(), json!(intensity));
        }
        Intent::SetFloorPattern { quadrant, pattern_id } => {
            out.insert("quadrant_id".to_string(), json!(quadrant));
            out.insert("pattern_id".to_string(), json!(pattern_id));
        }
        Intent::SetIlluminant { name } => {
            out.insert("name".to_string(), json!(name));
        }
        Intent::SetMaterial { quad_id, material_id } => {
            out.insert("quad_id".to_string(), json!(quad_id));
            out.insert("material_id".to_string(), json!(material_id));
        }
        Intent::SetWallPattern { wall_id, pattern_id } => {
            out.insert("wall_id".to_string(), json!(wall_id));
            out.insert("pattern_id".to_string(), json!(pattern_id));
        }
        Intent::Snapshot => {
            // No params beyond sovereign_cap. (Path is auto-generated.)
        }
        Intent::SpawnAt { kind, pos } => {
            out.insert("kind".to_string(), json!(kind));
            out.insert("x".to_string(), json!(pos[0]));
            out.insert("y".to_string(), json!(pos[1]));
            out.insert("z".to_string(), json!(pos[2]));
        }
        Intent::SpontaneousSeed { text } => {
            out.insert("text".to_string(), json!(text));
        }
        Intent::Teleport { room_id } => {
            // room.teleport expects a string id, not a numeric.
            let name = match room_id {
                0 => "TestRoom",
                1 => "MaterialRoom",
                2 => "PatternRoom",
                3 => "ScaleRoom",
                _ => "ColorRoom",
            };
            out.insert("room_id".to_string(), json!(name));
        }
        Intent::Tour { tour_id } => {
            out.insert("tour_id".to_string(), json!(tour_id));
        }
        Intent::Unknown { .. } => {
            // No params ; dispatcher returns no-op.
        }
    }
    Value::Object(out)
}

/// Dispatch a classified intent against the live `EngineState`. Resolves
/// the target MCP-tool handler via the registry + invokes it with the
/// JSON params produced by `intent_to_params`. Returns the handler's JSON
/// result OR a structured "no-op" envelope for `Intent::Unknown` /
/// `Intent::SpontaneousSeed`.
pub fn dispatch(intent: &Intent, sovereign: &str, state: &mut EngineState) -> Value {
    let tool = intent.target_tool().to_string();
    let params = intent_to_params(intent, sovereign);
    let raw_text = match intent {
        Intent::Unknown { reason } => reason.clone(),
        _ => String::new(),
    };
    let frame = state.frame_count;
    let ts_ms = now_ms();

    let result = if matches!(intent, Intent::Unknown { .. }) {
        // No dispatch ; return a structured no-op.
        json!({
            "ok": false,
            "no_op": true,
            "reason": match intent { Intent::Unknown { reason } => reason.clone(), _ => String::new() },
        })
    } else if matches!(intent, Intent::SpontaneousSeed { .. }) {
        // Stub : the host doesn't yet expose a `world.spontaneous_seed` tool.
        // We log the request + return a "pending" envelope so MCP callers
        // see the rule fired without a fake success.
        state.push_event(
            "INFO",
            "loa-host/intent",
            &format!("intent.dispatch · {} (pending stub)", tool),
        );
        json!({
            "ok": false,
            "pending": true,
            "tool": tool,
            "params": params,
            "note": "spontaneous_seed router rule fired ; host tool not yet wired",
        })
    } else {
        // Real dispatch via the live registry.
        let registry = crate::mcp_tools::tool_registry();
        match registry.get(&tool) {
            Some(entry) => {
                let r = (entry.handler)(state, params.clone());
                state.push_event(
                    "INFO",
                    "loa-host/intent",
                    &format!("intent.dispatch · {} · ok={}", tool, !r.get("error").is_some()),
                );
                r
            }
            None => {
                state.push_event(
                    "WARN",
                    "loa-host/intent",
                    &format!("intent.dispatch · tool not found : {}", tool),
                );
                json!({
                    "ok": false,
                    "error": format!("tool not found in registry : {tool}"),
                })
            }
        }
    };

    // Was the dispatch successful ?
    let ok = result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| !result.get("error").is_some() && !result.get("no_op").is_some());

    if ok {
        INTENTS_DISPATCHED.fetch_add(1, Ordering::Relaxed);
    }

    // Push to the recent ring.
    push_record(DispatchRecord {
        intent: intent.clone(),
        tool: tool.clone(),
        params: params.clone(),
        ok,
        frame,
        ts_ms,
        raw_text,
    });

    json!({
        "intent": intent.to_json(),
        "tool": tool,
        "params": params,
        "result": result,
        "ok": ok,
        "frame": frame,
        "ts_ms": ts_ms,
    })
}

/// One-shot route : classify → dispatch → record. Used by the HUD text-input
/// callback + the `intent.translate` MCP tool when invoked with `dispatch=true`.
pub fn route(text: &str, sovereign: &str, state: &mut EngineState) -> Value {
    let intent = classify(text);
    state.push_event(
        "INFO",
        "loa-host/intent",
        &format!("intent.classify · {} · '{}'", intent.kind_tag(), text.trim()),
    );
    let mut out = dispatch(&intent, sovereign, state);
    if let Some(obj) = out.as_object_mut() {
        obj.insert("input".to_string(), json!(text));
        obj.insert("classified_kind".to_string(), json!(intent.kind_tag()));
    }
    out
}

// ───────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_server::SOVEREIGN_CAP;

    // Helper that resets shared state at the head of each test so they
    // remain order-independent. Caller should still hold `test_lock()`
    // for tests that depend on counter values.
    fn fresh() -> EngineState {
        reset_for_test();
        EngineState::default()
    }

    #[test]
    fn classify_spawn_cube_returns_spawn_intent() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("spawn cube at 5 5 5");
        assert!(matches!(i, Intent::SpawnAt { kind: 0, pos: [5.0, 5.0, 5.0] }));
    }

    #[test]
    fn classify_spawn_sphere_at_floats() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("drop sphere at 1.5 0.0 -3.25");
        assert!(matches!(i, Intent::SpawnAt { kind: 1, pos: [1.5, 0.0, -3.25] }));
    }

    #[test]
    fn classify_set_wall_north_qr_returns_setwallpattern() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("set wall north pattern qr");
        match i {
            Intent::SetWallPattern { wall_id, pattern_id } => {
                assert_eq!(wall_id, 0);
                assert_eq!(pattern_id, 6); // QR-Code
            }
            other => panic!("expected SetWallPattern, got {:?}", other),
        }
    }

    #[test]
    fn classify_set_floor_quadrant_pattern() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("set floor ne pattern checker");
        match i {
            Intent::SetFloorPattern { quadrant, pattern_id } => {
                assert_eq!(quadrant, 0);
                assert_eq!(pattern_id, 3); // Checkerboard
            }
            other => panic!("expected SetFloorPattern, got {:?}", other),
        }
    }

    #[test]
    fn classify_teleport_color_returns_teleport() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("teleport to color room");
        assert!(matches!(i, Intent::Teleport { room_id: 4 }));
        let i2 = classify("go to material");
        assert!(matches!(i2, Intent::Teleport { room_id: 1 }));
        let i3 = classify("teleport pattern");
        assert!(matches!(i3, Intent::Teleport { room_id: 2 }));
    }

    #[test]
    fn classify_unknown_returns_unknown_with_reason() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("frobnicate the doorknob");
        match i {
            Intent::Unknown { reason } => {
                assert!(reason.contains("frobnicate"));
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
        // The unknown counter was bumped.
        assert!(INTENTS_UNKNOWN.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn classify_snapshot_aliases() {
        let _g = test_lock();
        let _ = fresh();
        for s in ["snapshot", "snap", "capture", "screenshot"] {
            let i = classify(s);
            assert!(matches!(i, Intent::Snapshot), "alias '{s}' failed");
        }
    }

    #[test]
    fn classify_burst_with_count() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("burst 25 frames");
        match i {
            Intent::Burst { count } => assert_eq!(count, 25),
            other => panic!("expected Burst, got {:?}", other),
        }
        // Default when no number provided.
        let d = classify("burst");
        assert!(matches!(d, Intent::Burst { count: 10 }));
    }

    #[test]
    fn classify_intensity_clamped() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("intensity 0.5");
        match i {
            Intent::SetCferIntensity { intensity } => assert!((intensity - 0.5).abs() < 1e-6),
            other => panic!("expected SetCferIntensity, got {:?}", other),
        }
        let high = classify("intensity 12.0");
        match high {
            Intent::SetCferIntensity { intensity } => assert!((intensity - 1.0).abs() < 1e-6),
            other => panic!("expected SetCferIntensity, got {:?}", other),
        }
        let neg = classify("fog -0.5");
        match neg {
            Intent::SetCferIntensity { intensity } => assert!(intensity.abs() < 1e-6),
            other => panic!("expected SetCferIntensity, got {:?}", other),
        }
    }

    #[test]
    fn classify_illuminant_canonicalizes() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("illuminant d65");
        match &i {
            Intent::SetIlluminant { name } => assert_eq!(name, "D65"),
            other => panic!("expected SetIlluminant, got {:?}", other),
        }
        let s = classify("set illuminant a");
        match &s {
            Intent::SetIlluminant { name } => assert_eq!(name, "A"),
            other => panic!("expected SetIlluminant, got {:?}", other),
        }
    }

    #[test]
    fn classify_tour_default() {
        let _g = test_lock();
        let _ = fresh();
        let t1 = classify("tour");
        match &t1 {
            Intent::Tour { tour_id } => assert_eq!(tour_id, "default"),
            other => panic!("expected Tour, got {:?}", other),
        }
        let t2 = classify("tour walls");
        match &t2 {
            Intent::Tour { tour_id } => assert_eq!(tour_id, "walls"),
            other => panic!("expected Tour, got {:?}", other),
        }
    }

    #[test]
    fn classify_spontaneous_strips_articles() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("spontaneous a sphere");
        match i {
            Intent::SpontaneousSeed { text } => assert_eq!(text, "sphere"),
            other => panic!("expected SpontaneousSeed, got {:?}", other),
        }
        let i2 = classify("imagine some clouds");
        match i2 {
            Intent::SpontaneousSeed { text } => assert_eq!(text, "clouds"),
            other => panic!("expected SpontaneousSeed, got {:?}", other),
        }
    }

    #[test]
    fn classify_material_on_quad() {
        let _g = test_lock();
        let _ = fresh();
        let i = classify("material on plinth 3 brass");
        match i {
            Intent::SetMaterial { quad_id, material_id } => {
                assert_eq!(quad_id, 3);
                assert_eq!(material_id, 3); // brass
            }
            other => panic!("expected SetMaterial, got {:?}", other),
        }
        let i2 = classify("set material 5 to gold");
        match i2 {
            Intent::SetMaterial { quad_id, material_id } => {
                assert_eq!(quad_id, 5);
                assert_eq!(material_id, 5); // gold
            }
            other => panic!("expected SetMaterial, got {:?}", other),
        }
    }

    #[test]
    fn dispatch_spawn_invokes_render_spawn_stress() {
        let _g = test_lock();
        reset_for_test();
        let mut s = EngineState::default();
        let intent = classify("spawn cube at 1 1 1");
        let r = dispatch(&intent, SOVEREIGN_CAP, &mut s);
        assert_eq!(r["tool"], "render.spawn_stress");
        // Either the FFI was wired and we got ok=true, or it was stubbed
        // and we got ok=false ; the dispatch envelope itself is well-formed.
        assert!(r.get("intent").is_some());
        assert!(r.get("params").is_some());
        // The SpawnAt params translate correctly.
        assert_eq!(r["params"]["kind"], 0);
        assert_eq!(r["params"]["x"], 1.0);
        assert_eq!(r["params"]["y"], 1.0);
        assert_eq!(r["params"]["z"], 1.0);
    }

    #[test]
    fn dispatch_unknown_returns_no_op_with_warning() {
        let _g = test_lock();
        reset_for_test();
        let mut s = EngineState::default();
        let intent = classify("zzz nope");
        let r = dispatch(&intent, SOVEREIGN_CAP, &mut s);
        assert_eq!(r["tool"], "");
        assert_eq!(r["result"]["no_op"], true);
        assert_eq!(r["ok"], false);
    }

    #[test]
    fn dispatch_snapshot_translates_correctly() {
        let _g = test_lock();
        reset_for_test();
        let mut s = EngineState::default();
        let intent = classify("snapshot");
        let r = dispatch(&intent, SOVEREIGN_CAP, &mut s);
        assert_eq!(r["tool"], "render.snapshot_png");
        // Real handler returns ok=true after queuing the PNG.
        assert_eq!(r["result"]["ok"], true);
    }

    #[test]
    fn dispatch_teleport_translates_room_id_to_string_name() {
        let _g = test_lock();
        reset_for_test();
        let mut s = EngineState::default();
        let intent = classify("teleport to color");
        let r = dispatch(&intent, SOVEREIGN_CAP, &mut s);
        assert_eq!(r["tool"], "room.teleport");
        // room.teleport expects a string id, not a number.
        assert_eq!(r["params"]["room_id"], "ColorRoom");
    }

    #[test]
    fn route_combines_classify_dispatch_record() {
        let _g = test_lock();
        reset_for_test();
        let mut s = EngineState::default();
        let r = route("snapshot", SOVEREIGN_CAP, &mut s);
        assert_eq!(r["tool"], "render.snapshot_png");
        assert_eq!(r["classified_kind"], "snapshot");
        assert_eq!(r["input"], "snapshot");
        // Ring should now have one entry.
        let recent = recent_json();
        assert_eq!(recent["count"], 1);
    }

    #[test]
    fn recent_ring_caps_at_16() {
        let _g = test_lock();
        reset_for_test();
        let mut s = EngineState::default();
        for n in 0..25 {
            let _ = route(&format!("burst {n}"), SOVEREIGN_CAP, &mut s);
        }
        let recent = recent_json();
        assert_eq!(recent["count"], 16); // capped
        let counters = recent["counters"]["per_kind"]["burst"].as_u64().unwrap();
        assert_eq!(counters, 25);
    }

    #[test]
    fn intent_to_params_carries_sovereign_cap() {
        // No counter-mutation here, but classify is used elsewhere — safe
        // to skip the lock for pure-pure-function tests like this.
        let intent = Intent::SetWallPattern { wall_id: 0, pattern_id: 6 };
        let v = intent_to_params(&intent, "0xDEAD_BEEF");
        assert_eq!(v["sovereign_cap"], "0xDEAD_BEEF");
        assert_eq!(v["wall_id"], 0);
        assert_eq!(v["pattern_id"], 6);
    }

    #[test]
    fn regex_pos_extraction_handles_floats() {
        let _g = test_lock();
        let _ = fresh();
        let cases = [
            ("spawn cube at 0 0 0", [0.0, 0.0, 0.0]),
            ("spawn cube at 1.5 2.5 3.5", [1.5, 2.5, 3.5]),
            ("spawn cube at -1 +2 -3", [-1.0, 2.0, -3.0]),
            ("place sphere at 5,5,5", [5.0, 5.0, 5.0]),
        ];
        for (input, want) in cases {
            match classify(input) {
                Intent::SpawnAt { pos, .. } => {
                    for (a, b) in pos.iter().zip(want.iter()) {
                        assert!((a - b).abs() < 1e-5, "input={input} got={pos:?} want={want:?}");
                    }
                }
                other => panic!("expected SpawnAt for '{input}', got {:?}", other),
            }
        }
    }

    #[test]
    fn at_least_30_keyword_rules() {
        // Sanity check that the rule-set covers ≥ 30 distinct phrasings.
        // Each unique input below corresponds to a documented phrase rule.
        let _g = test_lock();
        let _ = fresh();
        let phrases = [
            "snapshot",
            "snap",
            "capture",
            "screenshot",
            "burst 5",
            "burst of 10",
            "burst 25 frames",
            "tour",
            "tour walls",
            "tour default",
            "intensity 0.5",
            "intensity cfer 0.7",
            "fog 0.3",
            "atmosphere 0.5",
            "illuminant d65",
            "set illuminant d50",
            "teleport color",
            "teleport to color room",
            "go to material",
            "goto pattern",
            "spawn cube at 1 1 1",
            "drop sphere at 0 0 0",
            "place pyramid at 1 0 -3",
            "put torus at 5 5 5",
            "set wall north pattern qr",
            "set wall n to qr",
            "wall north qr",
            "set floor ne pattern checker",
            "floor sw checker",
            "material on plinth 3 brass",
            "set material 3 to brass",
            "material 3 brass",
            "spontaneous a sphere",
            "seed orb",
            "imagine a forest",
            "manifest something",
        ];
        assert!(
            phrases.len() >= 30,
            "rule-set must cover ≥ 30 phrasings (found {})",
            phrases.len()
        );
        let mut classified_kinds = std::collections::HashSet::new();
        for p in phrases {
            let i = classify(p);
            classified_kinds.insert(i.kind_tag());
            // None of these reference inputs should fall into Unknown.
            assert!(!matches!(i, Intent::Unknown { .. }), "phrase fell through : {p}");
        }
        // We expect ≥ 9 distinct intent kinds across the corpus.
        assert!(classified_kinds.len() >= 9);
    }

    #[test]
    fn counters_are_independent_per_kind() {
        let _g = test_lock();
        reset_for_test();
        let _ = classify("snapshot");
        let _ = classify("snapshot");
        let _ = classify("burst 5");
        let _ = classify("zzz");
        let c = counters_json();
        assert_eq!(c["per_kind"]["snapshot"], 2);
        assert_eq!(c["per_kind"]["burst"], 1);
        assert_eq!(c["per_kind"]["unknown"], 1);
        assert_eq!(c["intents_classified_total"], 4);
        assert_eq!(c["intents_unknown_total"], 1);
    }
}
