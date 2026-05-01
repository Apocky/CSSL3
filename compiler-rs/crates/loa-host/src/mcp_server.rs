//! § loa-host::mcp_server — TCP JSON-RPC 2.0 control-plane server
//! ════════════════════════════════════════════════════════════════
//!
//! Anchored to T11-LOA-HOST-3 (W-LOA-host-mcp). Implements the runtime
//! side of `scenes/mcp_runtime.cssl` — a Model-Context-Protocol-shaped
//! server so Claude (or any MCP client) can connect to a LIVE running
//! LoA-v13 binary and issue commands : query state, spawn objects,
//! change render-modes, move the player, sample/modify ω-field cells.
//!
//! Apocky directive (2026-04-30) :
//!   "are you able to fully interface with the game live?"
//!
//! § DESIGN
//!   * Bind `0.0.0.0:3001` by default (override via env `CSSL_MCP_PORT`).
//!     Localhost-by-default in production-deploy is enforced upstream
//!     — this slice keeps the bind permissive so an Apocky-side test
//!     `nc localhost 3001` works out-of-the-box.
//!   * JSON-RPC 2.0 over newline-delimited JSON : one request per line,
//!     one response per line.
//!   * Per-client thread : the accept loop spawns a dedicated reader
//!     thread per connection, all sharing `Arc<Mutex<EngineState>>`.
//!   * `tools.list` returns the 17-tool registry (MCP discovery).
//!   * Mutating tools (camera.set / room.spawn_plinth / dm.intensity /
//!     dm.event.propose / omega.modify / engine.shutdown / engine.pause /
//!     render.set_mode / companion.propose) require a `sovereign_cap`
//!     field with the constant `SOVEREIGN_CAP` ; default-deny.
//!   * Read-only tools (engine.state / camera.get / room.geometry /
//!     telemetry.recent / gm.describe_environment / gm.dialogue /
//!     omega.sample / tools.list) require no cap.
//!
//! § THREADING + STATE
//!   `EngineState` is the single shared data model. Sibling slices
//!   (W-LOA-host-render = render-loop ; W-LOA-host-DM = DM/GM machine)
//!   take an `Arc<Mutex<EngineState>>` clone and read/write the same
//!   fields. The MCP server holds its own clone for tool dispatch.
//!
//! § PRIME-DIRECTIVE
//!   The MCP server is consent-architected : every mutating tool has a
//!   default-deny gate, every invocation logs cap-status to the
//!   loa_runtime.log ring, and the server runs entirely in-process —
//!   no third-party MCP-broker, no off-machine relay.

// § Module-scope clippy allow-list. The `option_if_let_else` variants in
// this module make the poison-recovery + parse-fallback flows clearer
// in if-let-Err form than as `map_or`. The `significant_drop_tightening`
// target lives in `accept_loop` where the listener is held for the
// process lifetime (early-drop is the wrong semantic).
#![allow(clippy::option_if_let_else)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::manual_let_else)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use cssl_rt::loa_startup::log_event;

use crate::mcp_tools::{tool_registry, ToolMeta, ToolRegistry};
use crate::spectral_bridge::Illuminant;

// ───────────────────────────────────────────────────────────────────────
// § public constants
// ───────────────────────────────────────────────────────────────────────

/// Default TCP port the server listens on (override via `CSSL_MCP_PORT`).
pub const DEFAULT_MCP_PORT: u16 = 3001;

/// Sovereign capability constant. A mutating tool's `params.sovereign_cap`
/// field MUST equal this string for the call to be admitted.
///
/// § PRIME-DIRECTIVE : the constant is published inline rather than read
/// from an env var so the cap-gate can't be bypassed by the runtime
/// environment. Stage-1 will replace this with a per-session capability
/// minted at consent-time ; stage-0 ships the static constant so the live
/// MCP control-plane is operational from frame zero.
pub const SOVEREIGN_CAP: &str = "0xCAFE_BABE_DEADBEEF";

/// JSON-RPC 2.0 protocol-version literal.
pub const JSON_RPC_VERSION: &str = "2.0";

/// JSON-RPC 2.0 standard error code : method not found.
pub const ERR_METHOD_NOT_FOUND: i32 = -32_601;
/// JSON-RPC 2.0 standard error code : invalid params.
pub const ERR_INVALID_PARAMS: i32 = -32_602;
/// JSON-RPC 2.0 standard error code : parse error.
pub const ERR_PARSE_ERROR: i32 = -32_700;
/// JSON-RPC 2.0 application-defined : sovereign-cap missing or invalid.
pub const ERR_NO_SOVEREIGN: i32 = -32_001;

// ───────────────────────────────────────────────────────────────────────
// § render-mode enum (mirrors the .csl scene's render-mode-id contract)
// ───────────────────────────────────────────────────────────────────────

/// LoA-v13 render-mode discriminant. The wire-format id (`0..9`) is
/// what the MCP `render.set_mode` tool accepts and what the renderer
/// reads from `EngineState.render_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RenderMode {
    /// Final-color path (default).
    Normal           = 0,
    /// Albedo-only debug pass.
    Albedo           = 1,
    /// Linear-depth visualization.
    Depth            = 2,
    /// World-space normals.
    Normals          = 3,
    /// Surface-type discriminant overlay.
    SurfType         = 4,
    /// Raw SDF distance-isolines.
    Sdf              = 5,
    /// Raymarcher step-count heatmap.
    Steps            = 6,
    /// W-coordinate distance (4D Substrate slice).
    WDistance        = 7,
    /// Reference grid overlay.
    Grid             = 8,
    /// Field-vs-analytic-SDF differential.
    FieldVsAnalytic  = 9,
}

impl RenderMode {
    /// Convert a wire-format `u8` to a `RenderMode`. Returns `None` on
    /// out-of-range input.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Normal),
            1 => Some(Self::Albedo),
            2 => Some(Self::Depth),
            3 => Some(Self::Normals),
            4 => Some(Self::SurfType),
            5 => Some(Self::Sdf),
            6 => Some(Self::Steps),
            7 => Some(Self::WDistance),
            8 => Some(Self::Grid),
            9 => Some(Self::FieldVsAnalytic),
            _ => None,
        }
    }

    /// Stringify for telemetry / debug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Albedo => "Albedo",
            Self::Depth => "Depth",
            Self::Normals => "Normals",
            Self::SurfType => "SurfType",
            Self::Sdf => "Sdf",
            Self::Steps => "Steps",
            Self::WDistance => "WDistance",
            Self::Grid => "Grid",
            Self::FieldVsAnalytic => "FieldVsAnalytic",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § shared engine-state — the live process's data model
// ───────────────────────────────────────────────────────────────────────

/// 3D world-space coordinate (host-side mirror of LoA's `Vec3`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// Origin (0, 0, 0).
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// Camera state : position + Euler angles (yaw + pitch).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pos: Vec3::new(0.0, 1.5, -3.0),
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// Plinth instance — a single render-able pillar in the test-room.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Plinth {
    pub x: f32,
    pub z: f32,
    /// Packed 0xRRGGBB color.
    pub color_rgb: u32,
    /// AABB half-extents (height/width are uniform per the scene spec).
    pub half_extent: f32,
}

impl Plinth {
    #[must_use]
    pub const fn new(x: f32, z: f32, color_rgb: u32) -> Self {
        Self { x, z, color_rgb, half_extent: 0.5 }
    }
}

/// DM intensity dial — clamped to `0..=3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmState {
    pub intensity: u8,
    pub event_count: u32,
}

impl Default for DmState {
    fn default() -> Self {
        Self { intensity: 1, event_count: 0 }
    }
}

/// Telemetry-event ring entry. Last `TELEMETRY_RING_CAP` entries kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub frame: u64,
    pub level: String,
    pub source: String,
    pub message: String,
}

/// Bounded ring-cap. Any-newer push evicts the oldest entry.
pub const TELEMETRY_RING_CAP: usize = 256;

/// Snapshot request enqueued by the MCP `render.snapshot_png` tool.
/// Drained by the render loop each frame ; one PNG is written per
/// drained entry.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotRequest {
    /// Absolute or relative path the PNG will be written to.
    pub path: std::path::PathBuf,
}

/// § T11-WAVE3-TEXTINPUT : in-game text-input state mirror.
///
/// The InputState owns the canonical TextInputState ; per-frame the host
/// copies the focus flag + buffer + history (last 5) into this mirror so
/// MCP tools (`text_input.submit_history` + `text_input.inject`) can read +
/// program the box without reaching across the input-thread boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInputMirror {
    /// True while the box is focused (key-input routing into the box).
    pub focused: bool,
    /// Live edit buffer (current draft, post-frame).
    pub buffer: String,
    /// Last 5 submissions oldest-first.
    pub history: Vec<String>,
    /// Total submissions from start-of-process (matches the telemetry counter).
    pub submissions_total: u64,
    /// Total chars typed (post-cap) from start-of-process.
    pub chars_typed_total: u64,
    /// MCP-pending : submit this text on the next frame as if the user
    /// typed it and pressed Enter. Drained by the render loop.
    pub inject_pending: Option<String>,
}

/// § T11-LOA-USERFIX : capture-pipeline state mirror.
///
/// Tracks burst + video sessions in a form that survives across MCP-tool
/// invocations. The render loop drains pending requests and updates the
/// counters every frame ; MCP read-only tools query this mirror to surface
/// progress in the HUD + telemetry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaptureStateMirror {
    /// Burst : true while a burst is in progress.
    pub burst_active: bool,
    /// Burst : frames captured so far.
    pub burst_frames_captured: u32,
    /// Burst : frames remaining in the current sequence.
    pub burst_frames_remaining: u32,
    /// Burst : monotonic id (so MCP responses can tag each session).
    pub burst_id: u32,
    /// Video : true while video record is active.
    pub video_recording: bool,
    /// Video : frames written to disk so far in current session.
    pub video_frames_captured: u32,
    /// Video : monotonic id.
    pub video_id: u32,
    /// Video : duration of the current session in milliseconds (live tally).
    pub video_duration_ms: u64,
    /// MCP-pending : `Some(n)` to start a burst of n frames on next frame.
    /// Drained by the render loop. None if no pending request.
    pub burst_pending_count: Option<u32>,
    /// MCP-pending : true to start video record on next frame.
    pub video_start_pending: bool,
    /// MCP-pending : true to stop video record on next frame.
    pub video_stop_pending: bool,
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-SENSORY · sensory-harness ring-buffers
// ───────────────────────────────────────────────────────────────────────
// All caps below sized for a few seconds of @60Hz history. Per the
// directive, the harness exposes "every possible data type and stream
// you'd want fed back" — the ring sizes balance recall depth with the
// memory budget of holding the structures on the EngineState mutex.

/// Capacity of the body-pose history ring (60 frames ≈ 1 second @ 60Hz).
pub const SENSE_POSE_RING_CAP: usize = 60;
/// Capacity of the DM-history ring.
pub const SENSE_DM_HISTORY_CAP: usize = 32;
/// Capacity of the GM-recent-phrase ring.
pub const SENSE_GM_PHRASE_CAP: usize = 16;
/// Capacity of the input-history ring.
pub const SENSE_INPUT_HISTORY_CAP: usize = 64;
/// Capacity of the validation-error ring.
pub const SENSE_VALIDATION_ERR_CAP: usize = 16;
/// Capacity of the panic-event ring.
pub const SENSE_PANIC_RING_CAP: usize = 8;
/// Capacity of the MCP-client-tracking map (keyed by addr).
pub const SENSE_MCP_CLIENT_CAP: usize = 16;
/// Capacity of the recent MCP-command ring.
pub const SENSE_MCP_CMD_CAP: usize = 32;
/// Capacity of the framebuffer-thumbnail buffer (256×144 RGBA8 = ~144KiB).
pub const SENSE_THUMB_W: u32 = 256;
pub const SENSE_THUMB_H: u32 = 144;

/// One body-pose sample : camera position + orientation + frame + timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoseSample {
    pub frame: u64,
    pub time_ms: u64,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

/// One DM-state-transition record.
#[derive(Debug, Clone, PartialEq)]
pub struct DmHistoryEntry {
    pub frame: u64,
    pub time_ms: u64,
    pub from_state: String,
    pub to_state: String,
    pub tension: f32,
    pub event_kind: Option<String>,
}

/// One GM-narrator phrase emit record.
#[derive(Debug, Clone, PartialEq)]
pub struct GmPhraseEntry {
    pub frame: u64,
    pub time_ms: u64,
    pub topic: String,
    pub mood: String,
    pub line: String,
}

/// One input-event record.
#[derive(Debug, Clone, PartialEq)]
pub struct InputHistoryEntry {
    pub frame: u64,
    pub time_ms: u64,
    pub kind: String,
    pub key: String,
    pub pressed: bool,
}

/// One validation-error record (wgpu uncaptured-error or naga validation).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationErrorEntry {
    pub frame: u64,
    pub time_ms: u64,
    pub source: String,
    pub message: String,
}

/// One MCP-client connection record.
#[derive(Debug, Clone, PartialEq)]
pub struct McpClientEntry {
    pub addr: String,
    pub connected_ms: u64,
    pub invocations: u64,
}

/// One MCP-tool-invocation record.
#[derive(Debug, Clone, PartialEq)]
pub struct McpCommandEntry {
    pub frame: u64,
    pub time_ms: u64,
    pub caller: String,
    pub tool: String,
    pub latency_us: u64,
    pub success: bool,
}

/// Companion-AI proposal (T11-LOA-SENSORY surfaces these via sense.companion_proposals).
#[derive(Debug, Clone, PartialEq)]
pub struct CompanionProposalEntry {
    pub frame: u64,
    pub time_ms: u64,
    pub kind: String,
    pub payload: String,
    pub authorized: bool,
}

/// § T11-LOA-SENSORY : framebuffer-thumbnail mirror.
///
/// The Renderer downsamples the just-presented framebuffer to a fixed-size
/// thumbnail (256×144 RGBA8) once per N frames. The bytes live here so the
/// MCP `sense.framebuffer_thumbnail` tool can base64-encode + return inline
/// without spinning up a fresh GPU readback per query.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FramebufferThumbMirror {
    pub frame: u64,
    pub width: u32,
    pub height: u32,
    /// RGBA8 bytes (width*height*4). Empty when no thumbnail captured yet.
    pub rgba: Vec<u8>,
    /// True when an MCP client has requested a fresh capture on the next frame.
    pub capture_pending: bool,
    /// 16-region (4×4 grid) average colors of the most recent thumbnail.
    /// Each region : [r, g, b] in 0..1. Populated alongside `rgba`.
    pub regions_4x4: [[f32; 3]; 16],
    /// Center-pixel sample : RGB + crosshair-distance + material_id.
    /// distance is in meters (from camera to first hit) ; -1.0 if no hit.
    pub center_rgb: [f32; 3],
    pub center_distance: f32,
    pub center_material_id: i32,
    pub center_world_pos: [f32; 3],
}

/// § T11-WAVE3-SPONT : one manifestation-event mirror (MCP wire-format).
///
/// The runtime-side `spontaneous::ManifestationEvent` is mirrored here
/// per-frame for MCP `sense.spontaneous_recent` consumption.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpontaneousManifestEntry {
    /// Frame the manifestation fired on.
    pub frame: u64,
    /// World position the cell decoded to (cell-center).
    pub world_pos: [f32; 3],
    /// Stress-object kind id (0..13).
    pub kind: u32,
    /// Radiance magnitude at detect time.
    pub radiance_mag: f32,
    /// Cell density at detect time.
    pub density: f32,
    /// Originating seed-label (the keyword that produced this seed).
    pub label: String,
    /// Stress-object id returned from the spawn FFI (>0 on success).
    pub spawned_object_id: u32,
}

/// § T11-WAVE3-SPONT : MCP-mirror of the runtime `ManifestationDetector`.
///
/// The render loop copies the detector's recent-events ring + counters
/// into this struct each frame so MCP read-only tools return live values.
/// Pending requests (intent-sow) flow IN via `sow_pending`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpontaneousStateMirror {
    /// Last 16 manifestation events (oldest-first).
    pub recent_events: Vec<SpontaneousManifestEntry>,
    /// Total seeds sown since startup.
    pub seeds_total: u64,
    /// Total manifestations dispatched since startup.
    pub manifests_total: u64,
    /// Tracked-but-not-yet-manifested seed count.
    pub tracked_count: u32,
    /// Pending `world.spontaneous_seed` requests : (text, origin).
    /// Drained by the render loop on the next frame.
    pub sow_pending: Vec<SpontaneousSowRequest>,
}

/// § T11-WAVE3-SPONT : one queued sow-request from MCP.
#[derive(Debug, Clone, PartialEq)]
pub struct SpontaneousSowRequest {
    pub text: String,
    pub origin: [f32; 3],
}

/// § T11-LOA-FID-CFER : MCP-mirrored CFER state.
///
/// The Renderer (held in the runtime event-loop) owns the canonical
/// `cfer_render::CferRenderer`. After every frame, the render loop copies
/// the CFER metrics + center radiance into this struct on the EngineState
/// mutex so MCP read-only tools can return live values without crossing
/// the (Send-unsafe) wgpu boundary.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CferStateMirror {
    /// Active dense cells in the Ω-field (sample at last frame).
    pub active_cells: u64,
    /// Wallclock duration of the last `evolve()` step (microseconds).
    pub step_us: u64,
    /// Wallclock duration of the last pack-to-3D-texture step (microseconds).
    pub pack_us: u64,
    /// KAN per-cell evaluations performed last step.
    pub kan_evals: u64,
    /// Texels written to the 3D-texture buffer last frame.
    pub texels_written: u32,
    /// Frame counter (CFER-side).
    pub cfer_frame_n: u64,
    /// Sample radiance at the world envelope center (rgb, 0..1).
    pub center_radiance: [f32; 3],
    /// Currently-attached KAN sovereign handle (None ⇒ no KAN attached).
    pub kan_handle: Option<u16>,
    /// Pending KAN-handle request that the render loop will apply on the
    /// next frame. Cleared after application. `Some(Some(h))` ⇒ attach `h`,
    /// `Some(None)` ⇒ detach, `None` ⇒ no pending change.
    pub kan_handle_pending: Option<Option<u16>>,
    /// Force a CFER step on the next frame regardless of pause state.
    pub force_step_pending: bool,
    /// § T11-LOA-USERFIX : current atmospheric-intensity multiplier (0..1).
    pub cfer_intensity: f32,
    /// § T11-LOA-USERFIX : pending intensity change requested by the C-key
    /// or MCP. Drained by the render loop on the next frame.
    pub cfer_intensity_pending: Option<f32>,
}

/// Live, per-process engine state. Shared across the MCP server, the
/// render loop (sibling W-LOA-host-render), and the DM/GM machine
/// (sibling W-LOA-host-DM) via `Arc<Mutex<EngineState>>`.
///
/// § FIELD SEMANTICS
///   `frame_count`        ← incremented by the render loop per tick.
///   `quit_requested`     ← set by `engine.shutdown` ; main loop polls.
///   `paused`             ← toggled by `engine.pause`.
///   `camera`             ← read by render, written by `camera.set`.
///   `render_mode`        ← read by render, written by `render.set_mode`.
///   `active_scene`       ← string key from the scene-bundle table.
///   `room_dim_xyz`       ← test-room dimensions from .csl scene init.
///   `plinths`            ← live AABB list ; `room.spawn_plinth` appends.
///   `dm`                 ← DM intensity + event-count ; sibling DM-runtime
///                          drives this, MCP can read/nudge it.
///   `telemetry_ring`     ← bounded ring of recent log-events for the
///                          `telemetry.recent` tool.
///   `snapshot_queue`     ← T11-LOA-TEST-APP : pending snapshot requests
///                          drained by the render loop.
///   `tour_progress`      ← `Some((current, total))` while a tour is
///                          executing ; `None` when idle. HUD reads.
///   `snapshot_count`     ← total successful snapshots written this session
///                          (telemetry).
///   `cfer`               ← T11-LOA-FID-CFER : mirror of the render-side
///                          CferRenderer's metrics (read by MCP) + pending
///                          KAN-handle request (drained by renderer).
#[derive(Debug, Clone)]
pub struct EngineState {
    pub frame_count: u64,
    pub quit_requested: bool,
    pub paused: bool,
    pub camera: CameraState,
    pub render_mode: RenderMode,
    pub active_scene: String,
    pub room_dim_xyz: Vec3,
    pub plinths: Vec<Plinth>,
    pub dm: DmState,
    pub telemetry_ring: Vec<TelemetryEvent>,
    pub snapshot_queue: Vec<SnapshotRequest>,
    pub tour_progress: Option<(u32, u32)>,
    pub snapshot_count: u64,
    /// § T11-LOA-FID-SPECTRAL : active CIE-illuminant for the spectrally-
    /// baked material LUT. The render loop reads this each frame and rebuilds
    /// the per-material albedo via `spectral_bridge::bake_material_lut(illum)`.
    /// `render.set_illuminant` MCP tool mutates this ; default = D65.
    pub illuminant: Illuminant,
    /// § T11-LOA-FID-SPECTRAL : monotonic counter incremented every time
    /// `illuminant` is mutated. The render loop uses this as a cheap dirty-
    /// flag : when its observed gen != EngineState.illuminant_gen, re-bake.
    pub illuminant_gen: u64,
    /// § T11-LOA-FID-CFER : mirror of the runtime-side CFER metrics +
    /// pending KAN handle request.
    pub cfer: CferStateMirror,
    /// § T11-LOA-USERFIX : capture (burst + video) state mirror.
    pub capture: CaptureStateMirror,
    /// § T11-WAVE3-TEXTINPUT : in-game text-input box mirror.
    pub text_input: TextInputMirror,
    /// § T11-WAVE3-SPONT : spontaneous-condensation mirror (recent-events ring +
    /// pending intent-sow requests + counters).
    pub spontaneous: SpontaneousStateMirror,

    // ───────────────────────────────────────────────────────────────────
    // § T11-LOA-SENSORY · sensory-harness ring-buffers
    // ───────────────────────────────────────────────────────────────────
    /// Ring-buffer of body-pose samples (60 entries · 1 sec @60Hz).
    pub pose_history: Vec<PoseSample>,
    /// Ring-buffer of DM state transitions (32 entries).
    pub dm_history: Vec<DmHistoryEntry>,
    /// Ring-buffer of GM narrator phrases (16 entries).
    pub gm_phrase_history: Vec<GmPhraseEntry>,
    /// Ring-buffer of input events (64 entries).
    pub input_history: Vec<InputHistoryEntry>,
    /// Ring-buffer of validation errors captured from wgpu/naga (16 entries).
    pub validation_errors: Vec<ValidationErrorEntry>,
    /// Ring-buffer of panic events captured by the panic-hook (8 entries).
    pub panic_events: Vec<ValidationErrorEntry>,
    /// MCP-clients currently connected, keyed by addr.
    pub mcp_clients: Vec<McpClientEntry>,
    /// Ring-buffer of recent MCP commands (32 entries).
    pub mcp_command_history: Vec<McpCommandEntry>,
    /// Pending companion-AI proposals awaiting Sovereign authorization.
    pub companion_proposals: Vec<CompanionProposalEntry>,
    /// Mirror of the most-recent framebuffer thumbnail (256×144 RGBA8).
    pub fb_thumb: FramebufferThumbMirror,
    /// Compass-8 distances in meters · written by render-loop each frame.
    /// Order : [N, NE, E, SE, S, SW, W, NW]. 50.0 if no hit within MAX_RAY_M.
    pub compass_distances_m: [f32; 8],
    /// Engine-load metrics (cpu%, gpu%, memory MB, etc.) · cheap once-per-second sample.
    pub engine_load: EngineLoadMirror,
    /// Total `sense.*` invocations since startup (per-tool counts in telemetry).
    pub sense_invocations_total: u64,
    /// Total `sense.framebuffer_thumbnail` invocations since startup.
    pub sense_thumbnails_captured_total: u64,

    // ───────────────────────────────────────────────────────────────────
    // § T11-W8-CHAT-WIRE : in-game chat-log mirror
    // ───────────────────────────────────────────────────────────────────
    /// Recent chat-log entries (player submissions + GM/DM/Coder/System
    /// responses). Cap = `CHAT_LOG_CAP` (8) ; oldest dropped when full.
    /// Drawn ABOVE the chat-hint pill in the HUD overlay.
    pub chat_log: std::collections::VecDeque<ChatLogEntry>,
}

/// § T11-W8-CHAT-WIRE : role-tag for a chat-log entry.
///
/// Drives the HUD overlay's color-by-role rendering :
/// - `Player`  → white
/// - `GM`      → cyan       (default-route · narrative-emit)
/// - `DM`      → violet     (scene-arbitration)
/// - `Coder`   → amber      (runtime-mutate)
/// - `System`  → dim white  (cap-denied · routing-info · attestation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    /// Player typed the line in the text-input box.
    Player,
    /// GameMaster narrative response.
    Gm,
    /// DirectorMaster scene-arbiter response.
    Dm,
    /// Coder runtime-mutate response.
    Coder,
    /// System message (cap-denied · routing-error · attestation).
    System,
}

impl ChatRole {
    /// Stable string label for telemetry + JSON serialization.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ChatRole::Player => "player",
            ChatRole::Gm => "gm",
            ChatRole::Dm => "dm",
            ChatRole::Coder => "coder",
            ChatRole::System => "system",
        }
    }
}

/// § T11-W8-CHAT-WIRE : single row in the chat-log VecDeque.
///
/// Captures everything the HUD + MCP-tooling needs to display + replay a
/// chat-line. `frame` aligns with `EngineState::frame_count` so an MCP
/// reader can correlate to other rings.
#[derive(Debug, Clone)]
pub struct ChatLogEntry {
    /// Role-tag (drives HUD color).
    pub role: ChatRole,
    /// The text content (player-submitted OR orchestrator-emitted).
    pub text: String,
    /// Frame at which the entry was pushed.
    pub frame: u64,
    /// Wall-clock millis-since-epoch (best-effort ; 0 if SystemTime fails).
    pub ts_ms: u64,
}

/// Maximum chat-log entries retained. The HUD draws the most recent 3 above
/// the chat-hint pill ; the deeper history is exposed via `chat.recent` MCP
/// tool (queued for a follow-up wave).
pub const CHAT_LOG_CAP: usize = 8;

/// § T11-LOA-SENSORY : engine-load (interoception) mirror.
///
/// The window/render loop samples coarse engine-process metrics and writes
/// them here once per second. MCP `sense.engine_load` reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineLoadMirror {
    /// Last sample timestamp (Unix milliseconds).
    pub sampled_ms: u64,
    /// Process CPU percent (0..100). 0 when unavailable.
    pub cpu_percent: f32,
    /// Process resident memory (megabytes). 0 when unavailable.
    pub memory_mb: f32,
    /// GPU resolve time (microseconds, last frame).
    pub gpu_resolve_us: u64,
    /// Tonemap pass time (microseconds, last frame).
    pub tonemap_us: u64,
    /// Last frame's draw-call count.
    pub draw_calls: u32,
    /// Last frame's vertex count.
    pub vertices: u64,
    /// Last frame's pipeline-switch count.
    pub pipeline_switches: u32,
    /// Last frame-time (milliseconds).
    pub last_frame_ms: f32,
    /// Smoothed FPS estimate.
    pub fps_smoothed: f32,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            frame_count: 0,
            quit_requested: false,
            paused: false,
            camera: CameraState::default(),
            render_mode: RenderMode::Normal,
            active_scene: "test-room".to_string(),
            room_dim_xyz: Vec3::new(10.0, 4.0, 10.0),
            plinths: vec![
                Plinth::new(-2.0, -2.0, 0x_88_44_22),
                Plinth::new(2.0, -2.0, 0x_44_88_22),
                Plinth::new(-2.0, 2.0, 0x_22_44_88),
                Plinth::new(2.0, 2.0, 0x_88_22_44),
            ],
            dm: DmState::default(),
            telemetry_ring: Vec::with_capacity(TELEMETRY_RING_CAP),
            snapshot_queue: Vec::new(),
            tour_progress: None,
            snapshot_count: 0,
            illuminant: Illuminant::default(),
            illuminant_gen: 0,
            cfer: CferStateMirror {
                cfer_intensity: 0.10,
                ..Default::default()
            },
            capture: CaptureStateMirror::default(),
            text_input: TextInputMirror::default(),
            spontaneous: SpontaneousStateMirror::default(),
            pose_history: Vec::with_capacity(SENSE_POSE_RING_CAP),
            dm_history: Vec::with_capacity(SENSE_DM_HISTORY_CAP),
            gm_phrase_history: Vec::with_capacity(SENSE_GM_PHRASE_CAP),
            input_history: Vec::with_capacity(SENSE_INPUT_HISTORY_CAP),
            validation_errors: Vec::with_capacity(SENSE_VALIDATION_ERR_CAP),
            panic_events: Vec::with_capacity(SENSE_PANIC_RING_CAP),
            mcp_clients: Vec::with_capacity(SENSE_MCP_CLIENT_CAP),
            mcp_command_history: Vec::with_capacity(SENSE_MCP_CMD_CAP),
            companion_proposals: Vec::new(),
            fb_thumb: FramebufferThumbMirror::default(),
            compass_distances_m: [50.0; 8],
            engine_load: EngineLoadMirror::default(),
            sense_invocations_total: 0,
            sense_thumbnails_captured_total: 0,
            chat_log: std::collections::VecDeque::with_capacity(CHAT_LOG_CAP),
        }
    }
}

impl EngineState {
    /// Push a telemetry event. Drops the oldest if at capacity.
    pub fn push_event(&mut self, level: &str, source: &str, message: &str) {
        if self.telemetry_ring.len() >= TELEMETRY_RING_CAP {
            self.telemetry_ring.remove(0);
        }
        self.telemetry_ring.push(TelemetryEvent {
            frame: self.frame_count,
            level: level.to_string(),
            source: source.to_string(),
            message: message.to_string(),
        });
    }

    /// § T11-W8-CHAT-WIRE : push a chat-log entry. Drops oldest at cap.
    ///
    /// Used by `window.rs` chat-routing to surface player submissions +
    /// GM/DM/Coder/System responses on the HUD overlay. The 3 most-recent
    /// entries are drawn above the chat-hint pill ; deeper history is
    /// exposed via the future `chat.recent` MCP tool.
    pub fn push_chat_response(&mut self, role: ChatRole, text: String) {
        if self.chat_log.len() >= CHAT_LOG_CAP {
            self.chat_log.pop_front();
        }
        let ts_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.chat_log.push_back(ChatLogEntry {
            role,
            text,
            frame: self.frame_count,
            ts_ms,
        });
    }

    /// § T11-LOA-SENSORY : push a body-pose sample. Drops oldest if at cap.
    pub fn push_pose_sample(&mut self, sample: PoseSample) {
        if self.pose_history.len() >= SENSE_POSE_RING_CAP {
            self.pose_history.remove(0);
        }
        self.pose_history.push(sample);
    }

    /// § T11-LOA-SENSORY : push a DM-history record. Drops oldest if at cap.
    pub fn push_dm_history(&mut self, entry: DmHistoryEntry) {
        if self.dm_history.len() >= SENSE_DM_HISTORY_CAP {
            self.dm_history.remove(0);
        }
        self.dm_history.push(entry);
    }

    /// § T11-LOA-SENSORY : push a GM-phrase emit. Drops oldest if at cap.
    pub fn push_gm_phrase(&mut self, entry: GmPhraseEntry) {
        if self.gm_phrase_history.len() >= SENSE_GM_PHRASE_CAP {
            self.gm_phrase_history.remove(0);
        }
        self.gm_phrase_history.push(entry);
    }

    /// § T11-LOA-SENSORY : push an input-event record. Drops oldest if at cap.
    pub fn push_input_event(&mut self, entry: InputHistoryEntry) {
        if self.input_history.len() >= SENSE_INPUT_HISTORY_CAP {
            self.input_history.remove(0);
        }
        self.input_history.push(entry);
    }

    /// § T11-LOA-SENSORY : push a validation-error record. Drops oldest if at cap.
    pub fn push_validation_error(&mut self, entry: ValidationErrorEntry) {
        if self.validation_errors.len() >= SENSE_VALIDATION_ERR_CAP {
            self.validation_errors.remove(0);
        }
        self.validation_errors.push(entry);
    }

    /// § T11-WAVE3-SPONT : push one manifestation-event into the mirror's
    /// recent-events ring (drops oldest at 16-element cap). Used by the
    /// per-frame sync that drains `Renderer::scan_spontaneous_manifestations`.
    pub fn push_spontaneous_event(&mut self, entry: SpontaneousManifestEntry) {
        const CAP: usize = 16;
        if self.spontaneous.recent_events.len() >= CAP {
            self.spontaneous.recent_events.remove(0);
        }
        self.spontaneous.recent_events.push(entry);
    }

    /// § T11-LOA-SENSORY : push a panic record. Drops oldest if at cap.
    pub fn push_panic(&mut self, entry: ValidationErrorEntry) {
        if self.panic_events.len() >= SENSE_PANIC_RING_CAP {
            self.panic_events.remove(0);
        }
        self.panic_events.push(entry);
    }

    /// § T11-LOA-SENSORY : record / refresh an MCP-client connection record.
    pub fn record_mcp_client_connect(&mut self, addr: &str, time_ms: u64) {
        if let Some(found) = self.mcp_clients.iter_mut().find(|c| c.addr == addr) {
            found.connected_ms = time_ms;
            return;
        }
        if self.mcp_clients.len() >= SENSE_MCP_CLIENT_CAP {
            self.mcp_clients.remove(0);
        }
        self.mcp_clients.push(McpClientEntry {
            addr: addr.to_string(),
            connected_ms: time_ms,
            invocations: 0,
        });
    }

    /// § T11-LOA-SENSORY : remove a disconnected MCP-client record.
    pub fn record_mcp_client_disconnect(&mut self, addr: &str) {
        self.mcp_clients.retain(|c| c.addr != addr);
    }

    /// § T11-LOA-SENSORY : record an MCP-tool invocation. Updates per-client
    /// counter + appends to the recent-command ring.
    pub fn record_mcp_command(&mut self, entry: McpCommandEntry) {
        // Update per-client invocation count.
        if let Some(found) = self
            .mcp_clients
            .iter_mut()
            .find(|c| c.addr == entry.caller)
        {
            found.invocations = found.invocations.saturating_add(1);
        }
        if self.mcp_command_history.len() >= SENSE_MCP_CMD_CAP {
            self.mcp_command_history.remove(0);
        }
        self.mcp_command_history.push(entry);
    }

    /// § T11-LOA-SENSORY : push a companion-AI proposal awaiting authorization.
    pub fn push_companion_proposal(&mut self, entry: CompanionProposalEntry) {
        // Cap at 32 ; oldest evicted (the SOVEREIGN can also drain explicitly).
        if self.companion_proposals.len() >= 32 {
            self.companion_proposals.remove(0);
        }
        self.companion_proposals.push(entry);
    }
}

// ───────────────────────────────────────────────────────────────────────
// § server config + bind
// ───────────────────────────────────────────────────────────────────────

/// MCP-server bind configuration.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        let port = std::env::var("CSSL_MCP_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(DEFAULT_MCP_PORT);
        Self {
            host: "0.0.0.0".to_string(),
            port,
        }
    }
}

impl McpServerConfig {
    #[must_use]
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § JSON-RPC frame model
// ───────────────────────────────────────────────────────────────────────

/// Parsed JSON-RPC 2.0 request. `id` may be a number, string, or null.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    /// Optional in JSON-RPC 2.0 (omit ⇒ notification, no response).
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Outgoing JSON-RPC 2.0 response — `result` XOR `error` set.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error body.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    /// Construct a success response.
    #[must_use]
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSON_RPC_VERSION,
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Construct an error response.
    #[must_use]
    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSON_RPC_VERSION,
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § cap-gate + dispatch
// ───────────────────────────────────────────────────────────────────────

/// Returns true iff `params.sovereign_cap == SOVEREIGN_CAP`.
#[must_use]
pub fn has_sovereign_cap(params: &Value) -> bool {
    params
        .get("sovereign_cap")
        .and_then(Value::as_str)
        .is_some_and(|s| s == SOVEREIGN_CAP)
}

/// Dispatch a single request against the registry + state.
///
/// Returns the response body (success or error). This is the pure
/// dispatch surface ; thread-loop wraps it in I/O.
pub fn dispatch(
    state: &Arc<Mutex<EngineState>>,
    registry: &ToolRegistry,
    req: &JsonRpcRequest,
) -> JsonRpcResponse {
    let id = req.id.clone();

    let Some(tool) = registry.get(&req.method) else {
        log_event(
            "WARN",
            "loa-host/mcp",
            &format!("tool-unknown · method={}", req.method),
        );
        return JsonRpcResponse::err(
            id,
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {}", req.method),
        );
    };

    // Cap-gate : default-deny on mutating tools without sovereign_cap.
    if tool.meta.mutating && !has_sovereign_cap(&req.params) {
        log_event(
            "WARN",
            "loa-host/mcp",
            &format!("tool-denied · no-sovereign-cap · name={}", tool.meta.name),
        );
        return JsonRpcResponse::err(
            id,
            ERR_NO_SOVEREIGN,
            format!(
                "no sovereign-cap for mutating tool '{}' \
                 (params.sovereign_cap missing or invalid)",
                tool.meta.name
            ),
        );
    }

    log_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "tool-invoked · name={} · cap-status={}",
            tool.meta.name,
            if tool.meta.mutating { "sovereign-OK" } else { "read-only" }
        ),
    );

    // Dispatch with the held lock. Each handler is short-running ; long
    // ω-field walks are bounded by the per-call AABB caller-supplied
    // limits in the underlying loa_stubs FFI.
    let result = match state.lock() {
        Ok(mut g) => (tool.handler)(&mut g, req.params.clone()),
        Err(poisoned) => {
            // Poison-tolerant : the prior lock-holder panicked. Take
            // the inner state, log, and continue rather than cascading.
            log_event(
                "ERROR",
                "loa-host/mcp",
                "engine-state-mutex-poisoned · recovering · prior holder panicked",
            );
            let mut inner = poisoned.into_inner();
            (tool.handler)(&mut inner, req.params.clone())
        }
    };

    JsonRpcResponse::ok(id, result)
}

// ───────────────────────────────────────────────────────────────────────
// § per-line frame parse
// ───────────────────────────────────────────────────────────────────────

/// Parse a single JSON-RPC 2.0 line. On parse failure returns a synthesized
/// error response with `id = null`.
pub fn parse_request_line(line: &str) -> Result<JsonRpcRequest, JsonRpcResponse> {
    serde_json::from_str::<JsonRpcRequest>(line).map_err(|e| {
        JsonRpcResponse::err(
            Value::Null,
            ERR_PARSE_ERROR,
            format!("parse error: {e}"),
        )
    })
}

/// Encode a response to a newline-terminated UTF-8 line.
#[must_use]
pub fn encode_response(resp: &JsonRpcResponse) -> String {
    // serde_json::to_string never fails on Value-typed responses.
    let mut s = serde_json::to_string(resp).unwrap_or_else(|_| {
        // Fallback : hand-encoded minimal error frame.
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":{ERR_PARSE_ERROR},\"message\":\"encode-failure\"}}}}"
        )
    });
    s.push('\n');
    s
}

// ───────────────────────────────────────────────────────────────────────
// § thread loops : accept + per-client
// ───────────────────────────────────────────────────────────────────────

/// Spawn the accept loop on a dedicated thread. Returns the join-handle
/// + the bound port (which may differ from `cfg.port` if the OS auto-
/// assigned). Listener errors are logged ; the thread exits cleanly on
/// any I/O fault and the caller can re-spawn.
pub fn spawn_mcp_server(
    cfg: McpServerConfig,
    state: Arc<Mutex<EngineState>>,
) -> std::io::Result<(JoinHandle<()>, u16)> {
    let listener = TcpListener::bind(cfg.bind_addr())?;
    let bound = listener.local_addr()?.port();

    log_event(
        "INFO",
        "loa-host/mcp",
        &format!("listening on {}:{}", cfg.host, bound),
    );

    let registry = Arc::new(tool_registry());

    let handle = thread::Builder::new()
        .name("loa-host/mcp-accept".to_string())
        .spawn(move || {
            accept_loop(listener, state, registry);
        })?;

    Ok((handle, bound))
}

fn accept_loop(
    listener: TcpListener,
    state: Arc<Mutex<EngineState>>,
    registry: Arc<ToolRegistry>,
) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .map_or_else(|_| "unknown".to_string(), |a| a.to_string());
                log_event(
                    "INFO",
                    "loa-host/mcp",
                    &format!("client-accepted · addr={peer}"),
                );
                // § T11-LOA-SENSORY : record connect into the client ring.
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                if let Ok(mut g) = state.lock() {
                    g.record_mcp_client_connect(&peer, now_ms);
                }
                let st = state.clone();
                let reg = registry.clone();
                let _ = thread::Builder::new()
                    .name(format!("loa-host/mcp-client-{peer}"))
                    .spawn(move || {
                        client_loop(stream, st, reg);
                    });
            }
            Err(e) => {
                log_event(
                    "ERROR",
                    "loa-host/mcp",
                    &format!("accept-error · {e}"),
                );
                // On accept failure (e.g. listener closed), exit the loop
                // rather than spin. Caller may re-spawn via spawn_mcp_server.
                break;
            }
        }
    }
}

fn client_loop(
    stream: TcpStream,
    state: Arc<Mutex<EngineState>>,
    registry: Arc<ToolRegistry>,
) {
    let peer = stream
        .peer_addr()
        .map_or_else(|_| "unknown".to_string(), |a| a.to_string());

    let writer_clone = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            log_event(
                "ERROR",
                "loa-host/mcp",
                &format!("client-clone-error · addr={peer} · {e}"),
            );
            return;
        }
    };
    let mut writer = writer_clone;
    let reader = BufReader::new(stream);

    for line_res in reader.lines() {
        let line = match line_res {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(e) => {
                log_event(
                    "WARN",
                    "loa-host/mcp",
                    &format!("client-read-error · addr={peer} · {e}"),
                );
                break;
            }
        };

        let response = match parse_request_line(&line) {
            Ok(req) => {
                // JSON-RPC notifications (id absent / null) get no response.
                let is_notification = req.id.is_null();
                let method = req.method.clone();
                // § T11-LOA-SENSORY : measure dispatch latency + record into ring.
                let t0 = std::time::Instant::now();
                let resp = dispatch(&state, &registry, &req);
                let latency_us = t0.elapsed().as_micros() as u64;
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                if let Ok(mut g) = state.lock() {
                    let frame = g.frame_count;
                    let success = resp.error.is_none();
                    g.record_mcp_command(McpCommandEntry {
                        frame,
                        time_ms: now_ms,
                        caller: peer.clone(),
                        tool: method,
                        latency_us,
                        success,
                    });
                }
                if is_notification {
                    continue;
                }
                resp
            }
            Err(err_resp) => err_resp,
        };

        let encoded = encode_response(&response);
        if let Err(e) = writer.write_all(encoded.as_bytes()) {
            log_event(
                "WARN",
                "loa-host/mcp",
                &format!("client-write-error · addr={peer} · {e}"),
            );
            break;
        }
        let _ = writer.flush();
    }

    // § T11-LOA-SENSORY : record disconnect.
    if let Ok(mut g) = state.lock() {
        g.record_mcp_client_disconnect(&peer);
    }

    log_event(
        "INFO",
        "loa-host/mcp",
        &format!("client-disconnected · addr={peer}"),
    );
}

// ───────────────────────────────────────────────────────────────────────
// § convenience : tool-meta lookup helper
// ───────────────────────────────────────────────────────────────────────

/// Return the metadata of every registered tool.
#[must_use]
pub fn registered_tool_meta() -> HashMap<String, ToolMeta> {
    tool_registry()
        .iter()
        .map(|(k, v)| (k.clone(), v.meta.clone()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(clippy::similar_names)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shared_state() -> Arc<Mutex<EngineState>> {
        Arc::new(Mutex::new(EngineState::default()))
    }

    #[test]
    fn json_rpc_request_parses_correctly() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"engine.state","params":{}}"#;
        let req = parse_request_line(line).expect("parse ok");
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "engine.state");
        assert_eq!(req.id, Value::from(1));
    }

    #[test]
    fn parse_invalid_json_returns_error_response() {
        let bad = "{not json";
        let resp = parse_request_line(bad).expect_err("must error");
        let err = resp.error.as_ref().expect("error body");
        assert_eq!(err.code, ERR_PARSE_ERROR);
    }

    #[test]
    fn unknown_method_returns_method_not_found_error() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(7),
            method: "no.such.tool".to_string(),
            params: json!({}),
        };
        let resp = dispatch(&st, &reg, &req);
        let err = resp.error.expect("must be error");
        assert_eq!(err.code, ERR_METHOD_NOT_FOUND);
    }

    #[test]
    fn engine_state_handler_returns_valid_json() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(2),
            method: "engine.state".to_string(),
            params: json!({}),
        };
        let resp = dispatch(&st, &reg, &req);
        let result = resp.result.expect("ok result");
        assert!(result.get("frame_count").is_some());
        assert!(result.get("camera_pos").is_some());
        assert!(result.get("active_scene").is_some());
        assert!(result.get("render_mode").is_some());
    }

    #[test]
    fn mutating_tool_no_cap_returns_error() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(3),
            method: "camera.set".to_string(),
            // No sovereign_cap → must default-deny.
            params: json!({"x": 1.0, "y": 2.0, "z": 3.0}),
        };
        let resp = dispatch(&st, &reg, &req);
        let err = resp.error.expect("must error");
        assert_eq!(err.code, ERR_NO_SOVEREIGN);
    }

    #[test]
    fn mutating_tool_with_sovereign_cap_succeeds() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(4),
            method: "camera.set".to_string(),
            params: json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "x": 5.0, "y": 1.5, "z": -2.0,
                "yaw": 0.5, "pitch": 0.0
            }),
        };
        let resp = dispatch(&st, &reg, &req);
        assert!(resp.error.is_none());
        let result = resp.result.expect("ok");
        assert_eq!(result["camera_pos"]["x"].as_f64(), Some(5.0));
        // Verify state was actually mutated.
        let g = st.lock().unwrap();
        assert!((g.camera.pos.x - 5.0).abs() < 1e-6);
    }

    #[test]
    fn render_mode_round_trip() {
        for v in 0u8..=9 {
            let m = RenderMode::from_u8(v).expect("valid");
            assert_eq!(m as u8, v);
        }
        assert!(RenderMode::from_u8(10).is_none());
    }

    #[test]
    fn engine_state_default_has_seed_plinths() {
        let s = EngineState::default();
        assert_eq!(s.plinths.len(), 4);
        assert_eq!(s.active_scene, "test-room");
        assert_eq!(s.render_mode, RenderMode::Normal);
    }

    #[test]
    fn telemetry_ring_evicts_at_cap() {
        let mut s = EngineState::default();
        for i in 0..(TELEMETRY_RING_CAP + 50) {
            s.push_event("INFO", "test", &format!("evt-{i}"));
        }
        assert_eq!(s.telemetry_ring.len(), TELEMETRY_RING_CAP);
        // Oldest entries must have been evicted.
        assert!(s.telemetry_ring[0].message.starts_with("evt-50"));
    }

    #[test]
    fn config_default_uses_3001_when_env_unset() {
        // Save + clear env to test default.
        let prior = std::env::var("CSSL_MCP_PORT").ok();
        std::env::remove_var("CSSL_MCP_PORT");
        let cfg = McpServerConfig::default();
        assert_eq!(cfg.port, DEFAULT_MCP_PORT);
        if let Some(v) = prior {
            std::env::set_var("CSSL_MCP_PORT", v);
        }
    }

    #[test]
    fn has_sovereign_cap_only_true_for_exact_match() {
        assert!(has_sovereign_cap(&json!({"sovereign_cap": SOVEREIGN_CAP})));
        assert!(!has_sovereign_cap(&json!({"sovereign_cap": "nope"})));
        assert!(!has_sovereign_cap(&json!({})));
        assert!(!has_sovereign_cap(&json!({"sovereign_cap": 42})));
    }

    #[test]
    fn registered_tool_meta_matches_registry() {
        let meta = registered_tool_meta();
        let reg = tool_registry();
        assert_eq!(meta.len(), reg.len());
        // Every registry-key has matching meta.
        for k in reg.keys() {
            assert!(meta.contains_key(k), "missing meta for {k}");
        }
    }
}
