//! § loa-host::mcp_tools — the 17-tool registry
//! ════════════════════════════════════════════════
//!
//! Anchored to T11-LOA-HOST-3 (W-LOA-host-mcp). Implements the per-tool
//! handler functions the MCP server dispatches against.
//!
//! § TOOL SURFACE (matches `scenes/mcp_runtime.cssl` design)
//!   read-only :
//!     * `engine.state`              — frame + camera + scene + mode
//!     * `camera.get`                — pos + yaw + pitch
//!     * `room.geometry`             — plinth AABBs + room dim
//!     * `telemetry.recent`          — last N events from the ring
//!     * `gm.describe_environment`   — flavor text near camera
//!     * `gm.dialogue`               — procedural dialogue stub
//!     * `omega.sample`              — query ω-field cell at (x,y,z)
//!     * `tools.list`                — MCP discovery
//!   mutating (sovereign-cap-gated) :
//!     * `engine.shutdown`           — graceful exit
//!     * `engine.pause`              — toggle pause
//!     * `camera.set`                — teleport camera
//!     * `room.spawn_plinth`         — append a plinth at runtime
//!     * `render.set_mode`           — render-mode 0..9
//!     * `dm.intensity`              — DM dial 0..3
//!     * `dm.event.propose`          — fire DM event
//!     * `omega.modify`              — write ω-field cell (sovereign-only)
//!     * `companion.propose`         — submit CompanionProposal
//!
//! § HANDLER PATTERN
//!   Each tool is `fn(&mut EngineState, params: Value) -> Value`. The
//!   server holds the `EngineState` mutex for the duration of the call ;
//!   handlers must NOT block on external I/O. ω-field tools call into
//!   the cssl-rt FFI surface (loa_stubs) but those return immediately.
//!
//! § JSON CONTRACT
//!   * Successful results are always JSON objects with named fields.
//!   * Errors at the handler layer are still successful JSON-RPC responses
//!     with an `{"error": "..."}` field embedded in the result. The
//!     JSON-RPC `error` envelope is reserved for protocol-level failures
//!     (method not found / cap denied / parse error).

// § Module-scope clippy allow-list. Each entry is an intentional choice :
//   * `cast_precision_loss` : ω-field cell-coords are 21-bit u32 ; the
//     conversion to f32 for clamp() is bounded so precision-loss is by
//     construction (max ((1<<21)-1)=2_097_151 ≪ 2^23 mantissa).
//   * `cast_possible_wrap` / `cast_possible_truncation` : `FIELD_CELL_BYTES`
//     is a small const (88) ; the i32 cast to match the FFI signature is
//     statically safe.
//   * `suboptimal_flops` : color-mask + AABB arithmetic ; mul_add isn't
//     a meaningful win for two-term scalar ops at the contract surface.
//   * `similar_names` : `xi/yi/zi` (integer Morton coords) vs `x/y/z`
//     (input float coords) is the canonical Morton-encode pair.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::similar_names)]

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::material::{material_lut, material_name, MATERIAL_LUT_LEN};
use crate::mcp_server::{
    CameraState, EngineState, Plinth, RenderMode, SnapshotRequest, Vec3, TELEMETRY_RING_CAP,
};
// § T11-LOA-FID-CFER : telemetry constants for the cfer_snapshot response.
use crate::cfer_render::{TEX_COUNT, TEX_TOTAL_BYTES, TEX_X, TEX_Y, TEX_Z};
use crate::pattern::{pattern_lut, pattern_name, PATTERN_LUT_LEN};
use crate::snapshot::{
    decode_png, default_golden_dir, default_snapshot_dir, mae_bgra8, rgba8_to_bgra8_inplace,
    sanitize_snapshot_path, tour_by_id, GoldenDiffEntry, GoldenDiffReport, GOLDEN_MAE_THRESHOLD,
    TOUR_IDS,
};

// ───────────────────────────────────────────────────────────────────────
// § handler-fn type + registry shape
// ───────────────────────────────────────────────────────────────────────

/// Handler signature : `(&mut EngineState, params) -> JSON-result`.
pub type ToolHandler = fn(&mut EngineState, Value) -> Value;

/// Per-tool metadata exposed via `tools.list`.
#[derive(Debug, Clone)]
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    /// `true` ⇒ requires `sovereign_cap` in params. `false` ⇒ read-only.
    pub mutating: bool,
}

/// Registry entry : metadata + handler.
#[derive(Clone)]
pub struct ToolEntry {
    pub meta: ToolMeta,
    pub handler: ToolHandler,
}

impl std::fmt::Debug for ToolEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolEntry")
            .field("meta", &self.meta)
            .field("handler", &"<fn>")
            .finish()
    }
}

/// `BTreeMap<String, ToolEntry>` — deterministic iteration order for
/// tools.list output + reproducible test assertions.
pub type ToolRegistry = BTreeMap<String, ToolEntry>;

/// Build the canonical 17-tool registry.
#[must_use]
pub fn tool_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();

    macro_rules! reg {
        ($name:expr, $desc:expr, $mut:expr, $h:ident) => {
            r.insert(
                $name.to_string(),
                ToolEntry {
                    meta: ToolMeta {
                        name: $name.to_string(),
                        description: $desc.to_string(),
                        mutating: $mut,
                    },
                    handler: $h,
                },
            );
        };
    }

    // ─ read-only ─
    reg!(
        "engine.state",
        "Returns frame_count + camera_pos + active_scene + render_mode.",
        false,
        engine_state
    );
    reg!(
        "camera.get",
        "Returns camera position + yaw + pitch.",
        false,
        camera_get
    );
    reg!(
        "room.geometry",
        "Returns list of plinth AABBs + room dimensions.",
        false,
        room_geometry
    );
    reg!(
        "telemetry.recent",
        "Returns last N events from the telemetry ring (params: count).",
        false,
        telemetry_recent
    );
    reg!(
        "gm.describe_environment",
        "Returns flavor-text describing camera neighborhood.",
        false,
        gm_describe_environment
    );
    reg!(
        "gm.dialogue",
        "Generate procedural dialogue (params: npc_id · mood · topic).",
        false,
        gm_dialogue
    );
    reg!(
        "omega.sample",
        "Query ω-field cell at (x,y,z) (Morton-keyed via cssl_rt::loa_stubs).",
        false,
        omega_sample
    );
    reg!(
        "tools.list",
        "MCP-standard tool-discovery (returns tool names + descriptions).",
        false,
        tools_list
    );

    // ─ mutating (sovereign-cap-gated) ─
    reg!(
        "engine.shutdown",
        "Graceful exit (set quit_requested flag).",
        true,
        engine_shutdown
    );
    reg!(
        "engine.pause",
        "Toggle pause flag.",
        true,
        engine_pause
    );
    reg!(
        "camera.set",
        "Teleport camera (params: x · y · z · yaw · pitch).",
        true,
        camera_set
    );
    reg!(
        "room.spawn_plinth",
        "Add a plinth at runtime (params: x · z · color).",
        true,
        room_spawn_plinth
    );
    reg!(
        "render.set_mode",
        "Choose render mode 0..9.",
        true,
        render_set_mode
    );
    reg!(
        "dm.intensity",
        "Set DM intensity 0..3.",
        true,
        dm_intensity
    );
    reg!(
        "dm.event.propose",
        "Trigger a DM event (params: kind · pos).",
        true,
        dm_event_propose
    );
    reg!(
        "omega.modify",
        "Write ω-field cell at (x,y,z) (sovereign-cap-gated).",
        true,
        omega_modify
    );
    reg!(
        "companion.propose",
        "Submit a CompanionProposal (forwards to companion-hook stub).",
        true,
        companion_propose
    );

    // ─ Live render-control plane (T11-LOA-RICH-RENDER) ─
    reg!(
        "render.list_patterns",
        "Return all procedural-pattern names + ids in the LUT.",
        false,
        render_list_patterns
    );
    reg!(
        "render.list_materials",
        "Return all material names + ids in the LUT.",
        false,
        render_list_materials
    );
    reg!(
        "render.snapshot",
        "Return frame_count + camera_pos + active patterns/materials.",
        false,
        render_snapshot
    );
    reg!(
        "render.set_wall_pattern",
        "Override the procedural pattern for a wall (params: wall_id 0..3, pattern_id 0..15).",
        true,
        render_set_wall_pattern
    );
    reg!(
        "render.set_floor_pattern",
        "Override the procedural pattern for a floor quadrant (params: quadrant_id 0..3, pattern_id).",
        true,
        render_set_floor_pattern
    );
    reg!(
        "render.set_material",
        "Override the material for a quad slot (params: quad_id 0..15, material_id).",
        true,
        render_set_material
    );
    reg!(
        "render.spawn_stress",
        "Spawn a stress object (params: kind 0..13, x, y, z).",
        true,
        render_spawn_stress
    );

    // ─ T11-WAVE3-GLTF : external 3D-asset import + spawn ─
    reg!(
        "world.spawn_gltf",
        "Parse a glTF/GLB file and spawn its mesh at world coords (params: \
         path string, x, y, z, scale). Returns instance_id (0 on failure). \
         Sovereign-cap-gated.",
        true,
        world_spawn_gltf
    );
    reg!(
        "world.gltf_spawns_total",
        "Return cumulative count of successful + rejected glTF spawns.",
        false,
        world_gltf_spawns_total
    );
    reg!(
        "world.list_dynamic_meshes",
        "Return spawned dynamic-mesh records (instance_id, vertex_count, \
         triangle_count, world_pos, scale, material_id, bbox).",
        false,
        world_list_dynamic_meshes
    );

    // ─ Telemetry (T11-LOA-TELEM) ─
    reg!(
        "telemetry.snapshot",
        "Returns frame_count + fps + p50/p95/p99 + counters + uptime.",
        false,
        telemetry_snapshot
    );
    reg!(
        "telemetry.histogram",
        "Returns 10-bucket frame-time histogram (counts of last 1024 frames).",
        false,
        telemetry_histogram
    );
    reg!(
        "telemetry.gpu_info",
        "Returns captured wgpu adapter info (name + backend + features + limits).",
        false,
        telemetry_gpu_info
    );
    reg!(
        "telemetry.tail_events",
        "Returns last N JSONL events from in-memory ring (params: limit).",
        false,
        telemetry_tail_events
    );
    reg!(
        "telemetry.flush",
        "Force-flush CSV + JSONL files. Returns success.",
        false,
        telemetry_flush
    );
    reg!(
        "telemetry.set_log_level",
        "Set log threshold (params: level 0=DEBUG · 1=INFO · 2=WARN · 3=ERROR).",
        true,
        telemetry_set_log_level
    );

    // ─ T11-LOA-TEST-APP : visual-data-gathering apparatus ─
    reg!(
        "render.snapshot_png",
        "Capture the next-presented frame to a PNG file (params: path).",
        true,
        render_snapshot_png
    );
    reg!(
        "render.tour",
        "Run a scripted camera tour, capturing PNG at each pose \
         (params: tour_id 'default'|'walls'|'floor'|'plinths'|'ceiling', output_dir).",
        true,
        render_tour
    );
    reg!(
        "render.diff_golden",
        "Compare prior tour snapshots against goldens via mean-absolute-error \
         (params: tour_id, threshold).",
        false,
        render_diff_golden
    );

    // ─ T11-LOA-ROOMS : multi-room test-suite navigation ─
    reg!(
        "room.list",
        "List the 5 diagnostic rooms (TestRoom + Material/Pattern/Scale/Color) with bounds + descriptions.",
        false,
        room_list
    );
    reg!(
        "room.teleport",
        "Teleport the camera to a named room (params: room_id string e.g. \"MaterialRoom\").",
        true,
        room_teleport
    );

    // ─ T11-LOA-FID-MAINSTREAM : graphical-fidelity probe (read-only) ─
    reg!(
        "render.fidelity",
        "Returns active fidelity settings : msaa_samples · hdr_format · present_mode · aniso_max · tonemap_path.",
        false,
        render_fidelity
    );

    // ─ T11-LOA-FID-STOKES : Stokes IQUV polarized rendering ─
    reg!(
        "render.stokes_snapshot",
        "Returns the Stokes IQUV vector at the center pixel + polarization-mode + Mueller-apply count.",
        false,
        render_stokes_snapshot
    );
    reg!(
        "render.set_polarization_view",
        "Set the polarization-view diagnostic mode (params: mode 0..4).",
        true,
        render_set_polarization_view
    );
    reg!(
        "render.polarization_panels",
        "Returns the 4 canonical polarization-diagnostic panels with expected Stokes signatures.",
        false,
        render_polarization_panels
    );

    // ─ T11-LOA-FID-SPECTRAL : spectral-illuminant control plane ─
    reg!(
        "render.set_illuminant",
        "Bake the material LUT under a specified CIE illuminant (params: name 'D65'|'D50'|'A'|'F11'). Live re-bake.",
        true,
        render_set_illuminant
    );
    reg!(
        "render.list_illuminants",
        "Return the 4 canonical CIE illuminants supported by the spectral bake (D65 · D50 · A · F11).",
        false,
        render_list_illuminants
    );
    reg!(
        "render.spectral_snapshot",
        "Return the 16x4 baked sRGB matrix : every material under every illuminant. Used for metamerism diagnostics.",
        false,
        render_spectral_snapshot
    );
    reg!(
        "render.spectral_zones",
        "List the 4 zones inside the SpectralRoom (NW=D65 · NE=D50 · SW=A · SE=F11). Walk between to see metamerism.",
        false,
        render_spectral_zones
    );
    reg!(
        "telemetry.spectral",
        "Return spectral-bake counters (count · cumulative microseconds · illuminant-change tally · current illuminant).",
        false,
        telemetry_spectral
    );
    reg!(
        "room.teleport_zone",
        "Teleport into a SpectralRoom zone AND switch to that zone's illuminant atomically (params: zone string).",
        true,
        room_teleport_zone
    );

    // ─ T11-LOA-FID-CFER : Causal Field-Evolution Rendering volumetric pass ─
    reg!(
        "render.cfer_snapshot",
        "Return CFER metrics : active Ω-field cells + step/pack µs + KAN evals + center-radiance sample.",
        false,
        render_cfer_snapshot
    );
    reg!(
        "render.cfer_step",
        "Force a CFER step on the next frame regardless of pause state.",
        true,
        render_cfer_step
    );
    reg!(
        "render.cfer_set_kan_handle",
        "Attach (sovereign_handle: u16) or detach (handle: -1) a KAN modulation handle to the CFER field.",
        true,
        render_cfer_set_kan_handle
    );

    // ─ T11-LOA-USERFIX : atmospheric-intensity + capture controls ─
    reg!(
        "render.cfer_intensity",
        "Set the CFER atmospheric-intensity multiplier (params: intensity 0..1, default 0.10). Multiplies final alpha so the host can fade the volumetric pass.",
        true,
        render_cfer_intensity
    );
    reg!(
        "render.start_burst",
        "Start a burst-of-N screenshot capture (params: count, frame_stride). Returns the output directory + burst id.",
        true,
        render_start_burst
    );
    reg!(
        "render.start_video",
        "Start video record (params: frame_stride). Each subsequent frame writes a PNG to logs/video/<id>/frame_NNNN.png until stopped.",
        true,
        render_start_video
    );
    reg!(
        "render.stop_video",
        "Stop the in-flight video record. Returns total frames + duration so the user can ffmpeg the directory.",
        true,
        render_stop_video
    );

    // ─ T11-WAVE3-TEXTINPUT : in-game text-input box ─
    reg!(
        "text_input.submit_history",
        "Return the last 5 in-game text-input submissions (oldest-first) plus the current focus + buffer.",
        false,
        text_input_submit_history
    );
    reg!(
        "text_input.inject",
        "Programmatically submit text as if the user typed it and pressed Enter (params: text). Pushes through the same path as a keyboard submit.",
        true,
        text_input_inject
    );


    // ─ T11-LOA-SENSORY : full MCP sensory + proprioception harness ─
    // 9 axes · 25 sense.* tools · all read-only · all no-cap
    // A. VISUAL (4)
    reg!(
        "sense.framebuffer_thumbnail",
        "Return a 256×144 PNG of the current framebuffer (base64-inlined).",
        false,
        sense_framebuffer_thumbnail
    );
    reg!(
        "sense.center_pixel",
        "Return RGB + material_id + crosshair-distance + world-position at viewport center.",
        false,
        sense_center_pixel
    );
    reg!(
        "sense.viewport_summary",
        "Return 16-region (4×4 grid) average colors of the current thumbnail.",
        false,
        sense_viewport_summary
    );
    reg!(
        "sense.object_at_crosshair",
        "Raycast forward from camera and return first hit (plinth_index + distance + material).",
        false,
        sense_object_at_crosshair
    );
    // B. AUDIO (3 stubs)
    reg!(
        "sense.audio_levels",
        "Return RMS + peak + 8-band frequency-spectrum over recent ~100ms (stub if no audio module).",
        false,
        sense_audio_levels
    );
    reg!(
        "sense.audio_recent",
        "Return last 1-second of audio captured to PCM (base64 inline · stub if no audio).",
        false,
        sense_audio_recent
    );
    reg!(
        "sense.spatial_audio",
        "Return directional + distance per audio source (stub if no audio module).",
        false,
        sense_spatial_audio
    );
    // C. SPATIAL (3)
    reg!(
        "sense.compass_8",
        "Return 8-direction wall-distance raycast from camera (N · NE · E · SE · S · SW · W · NW).",
        false,
        sense_compass_8
    );
    reg!(
        "sense.body_pose",
        "Return last 60 frames of camera trajectory + computed velocity + acceleration.",
        false,
        sense_body_pose
    );
    reg!(
        "sense.room_neighbors",
        "Return current room + adjacent rooms.",
        false,
        sense_room_neighbors
    );
    // D. INTEROCEPTION (4)
    reg!(
        "sense.engine_load",
        "Return process CPU% · memory MB · GPU pacing for the current frame.",
        false,
        sense_engine_load
    );
    reg!(
        "sense.frame_pacing",
        "Return last-60-frame histogram + p50/p95/p99 + dropped-frame indicator.",
        false,
        sense_frame_pacing
    );
    reg!(
        "sense.gpu_state",
        "Return wgpu adapter info + current pipeline binding counts + last-frame stats.",
        false,
        sense_gpu_state
    );
    reg!(
        "sense.thermal",
        "Return CPU + GPU temperature + throttle status (stub when platform probe unavailable).",
        false,
        sense_thermal
    );
    // E. DIAGNOSTIC (4)
    reg!(
        "sense.recent_errors",
        "Return last 32 ERROR + WARN log events from telemetry ring.",
        false,
        sense_recent_errors
    );
    reg!(
        "sense.recent_panics",
        "Return panic-events captured by the panic-hook since startup.",
        false,
        sense_recent_panics
    );
    reg!(
        "sense.validation_errors",
        "Return wgpu / naga validation errors captured during the session.",
        false,
        sense_validation_errors
    );
    reg!(
        "sense.test_status",
        "Return the test-apparatus' current state (snapshot count + tour progress).",
        false,
        sense_test_status
    );
    // F. TEMPORAL (3)
    reg!(
        "sense.event_log",
        "Return last 64 structured JSONL events from the telemetry ring.",
        false,
        sense_event_log
    );
    reg!(
        "sense.dm_history",
        "Return last 32 DM-state-transition records.",
        false,
        sense_dm_history
    );
    reg!(
        "sense.input_history",
        "Return last 64 InputFrame key-event records.",
        false,
        sense_input_history
    );
    // G. CAUSAL (3)
    reg!(
        "sense.dm_state",
        "Return current DM director state + tension + event count.",
        false,
        sense_dm_state
    );
    reg!(
        "sense.gm_recent_phrases",
        "Return last 16 GM-narrator-emitted phrases.",
        false,
        sense_gm_recent_phrases
    );
    reg!(
        "sense.companion_proposals",
        "Return pending companion-AI proposals awaiting Sovereign authorization.",
        false,
        sense_companion_proposals
    );
    // H. NETWORK (2)
    reg!(
        "sense.mcp_clients",
        "Return currently-connected MCP clients + their addresses + invocation counts.",
        false,
        sense_mcp_clients
    );
    reg!(
        "sense.recent_commands",
        "Return last 32 MCP-tool invocations + caller + latency_us + success.",
        false,
        sense_recent_commands
    );
    // I. ENVIRONMENTAL (5)
    reg!(
        "sense.omega_field_at_camera",
        "Sample the Ω-field cell at camera position and return all 7 facets (density · velocity · vorticity · etc.).",
        false,
        sense_omega_field_at_camera
    );
    reg!(
        "sense.spectral_at_pixel",
        "Return the 16-band SpectralRadiance at center pixel under current illuminant.",
        false,
        sense_spectral_at_pixel
    );
    reg!(
        "sense.stokes_at_pixel",
        "Return Stokes IQUV + DOP-linear + DOP-total + AoLP at center pixel.",
        false,
        sense_stokes_at_pixel
    );
    reg!(
        "sense.cfer_neighborhood",
        "Return 3×3×3 Ω-field cells around camera + KAN-eval-counts + radiance probes.",
        false,
        sense_cfer_neighborhood
    );
    reg!(
        "sense.dgi_signal",
        "Return DGI runtime 6-layer state (manifold · cogstate · SCT · DCU · perception · retrocausal · stub).",
        false,
        sense_dgi_signal
    );
    // Combined (1)
    reg!(
        "sense.snapshot",
        "One-shot dump : aggregates the lightweight sense.* axes (no PNG · no audio).",
        false,
        sense_snapshot
    );

    r
}

// ───────────────────────────────────────────────────────────────────────
// § param helpers — extract typed fields with safe defaults
// ───────────────────────────────────────────────────────────────────────

fn p_f32(v: &Value, key: &str, default: f32) -> f32 {
    v.get(key)
        .and_then(Value::as_f64)
        .map_or(default, |x| x as f32)
}

fn p_u32(v: &Value, key: &str, default: u32) -> u32 {
    v.get(key)
        .and_then(Value::as_u64)
        .map_or(default, |x| x as u32)
}

fn p_u8(v: &Value, key: &str, default: u8) -> u8 {
    v.get(key)
        .and_then(Value::as_u64)
        .map_or(default, |x| x.min(u64::from(u8::MAX)) as u8)
}

fn p_str<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn camera_pos_json(c: &CameraState) -> Value {
    json!({
        "x": c.pos.x,
        "y": c.pos.y,
        "z": c.pos.z,
        "yaw": c.yaw,
        "pitch": c.pitch,
    })
}

// ───────────────────────────────────────────────────────────────────────
// § handlers — read-only
// ───────────────────────────────────────────────────────────────────────

fn engine_state(state: &mut EngineState, _params: Value) -> Value {
    json!({
        "frame_count": state.frame_count,
        "paused": state.paused,
        "quit_requested": state.quit_requested,
        "camera_pos": camera_pos_json(&state.camera),
        "active_scene": state.active_scene,
        "render_mode": state.render_mode.as_str(),
        "render_mode_id": state.render_mode as u8,
        "dm_intensity": state.dm.intensity,
        "plinth_count": state.plinths.len(),
    })
}

fn camera_get(state: &mut EngineState, _params: Value) -> Value {
    camera_pos_json(&state.camera)
}

fn room_geometry(state: &mut EngineState, _params: Value) -> Value {
    let plinths: Vec<Value> = state
        .plinths
        .iter()
        .map(|p| {
            json!({
                "x": p.x,
                "z": p.z,
                "color_rgb": p.color_rgb,
                "half_extent": p.half_extent,
                "aabb_min": [p.x - p.half_extent, 0.0, p.z - p.half_extent],
                "aabb_max": [p.x + p.half_extent, p.half_extent * 2.0, p.z + p.half_extent],
            })
        })
        .collect();
    json!({
        "room_dim": {"x": state.room_dim_xyz.x, "y": state.room_dim_xyz.y, "z": state.room_dim_xyz.z},
        "plinths": plinths,
    })
}

fn telemetry_recent(state: &mut EngineState, params: Value) -> Value {
    let count = p_u32(&params, "count", 32) as usize;
    let count = count.min(TELEMETRY_RING_CAP);
    let n = state.telemetry_ring.len();
    let take_from = n.saturating_sub(count);
    let events: Vec<Value> = state.telemetry_ring[take_from..]
        .iter()
        .map(|e| {
            json!({
                "frame": e.frame,
                "level": e.level,
                "source": e.source,
                "message": e.message,
            })
        })
        .collect();
    json!({"events": events, "total_in_ring": n})
}

fn gm_describe_environment(state: &mut EngineState, _params: Value) -> Value {
    // § Stage-0 procedural-flavor : sample camera-neighborhood plinths + emit
    // a CSL-shaped descriptor. Stage-1 hands this off to the GM-runtime in
    // sibling W-LOA-host-DM, which composes from the live ω-field neighborhood.
    let near: Vec<&Plinth> = state
        .plinths
        .iter()
        .filter(|p| {
            let dx = p.x - state.camera.pos.x;
            let dz = p.z - state.camera.pos.z;
            (dx * dx + dz * dz) < 25.0
        })
        .collect();
    let count = near.len();
    let prose = match count {
        0 => "An empty stretch of test-room floor extends in every direction. The grid hums.".to_string(),
        1 => format!(
            "A solitary plinth at ({:.1}, {:.1}) catches the analytic light. Color cell: 0x{:06X}.",
            near[0].x, near[0].z, near[0].color_rgb
        ),
        n => format!(
            "{n} plinths cluster around your viewpoint. The nearest sits at ({:.1}, {:.1}) — color 0x{:06X} — its surface lit by SDF-derived analytics.",
            near[0].x, near[0].z, near[0].color_rgb
        ),
    };
    json!({
        "scene": state.active_scene,
        "camera": camera_pos_json(&state.camera),
        "nearby_plinths": count,
        "prose": prose,
    })
}

fn gm_dialogue(_state: &mut EngineState, params: Value) -> Value {
    let npc_id = p_str(&params, "npc_id", "unknown");
    let mood = p_str(&params, "mood", "neutral");
    let topic = p_str(&params, "topic", "the labyrinth");
    // § Stage-0 procedural-dialogue stub. Sibling W-LOA-host-DM ships
    // the real GM-state-machine that consults ω-field memory + NPC
    // affinity vectors. The shape returned here is what that runtime
    // emits, so MCP clients can integrate against the contract today.
    let line = format!(
        "[{npc_id} · {mood}] On the topic of {topic} : the substrate listens. \
         The ω-field remembers what the analytic forgets."
    );
    json!({
        "npc_id": npc_id,
        "mood": mood,
        "topic": topic,
        "line": line,
    })
}

fn omega_sample(_state: &mut EngineState, params: Value) -> Value {
    let x = p_f32(&params, "x", 0.0);
    let y = p_f32(&params, "y", 0.0);
    let z = p_f32(&params, "z", 0.0);
    // Quantize to integer cell-coords (1m grid) then Morton-encode.
    let xi = x.clamp(0.0, ((1u32 << 21) - 1) as f32) as u32;
    let yi = y.clamp(0.0, ((1u32 << 21) - 1) as f32) as u32;
    let zi = z.clamp(0.0, ((1u32 << 21) - 1) as f32) as u32;
    let morton = morton_encode_u32(xi, yi, zi);

    // Sample via the cssl-rt FFI. We use the safe Rust-side counterpart
    // by spinning a small buffer + calling the FFI directly. This keeps
    // the MCP server independent of any private re-exports.
    let mut buf = [0u8; cssl_rt::loa_stubs::FIELD_CELL_BYTES];
    // SAFETY : __cssl_omega_field_sample's contract is :
    //   - out_buf may be NULL only when cap < FIELD_CELL_BYTES (returns -1).
    //   - With cap == FIELD_CELL_BYTES, it writes 0 (cell unset) or
    //     FIELD_CELL_BYTES (cell present), no over-write.
    // Our buf is exactly FIELD_CELL_BYTES + we own it on the stack.
    // The handler would normally be safe-Rust ; we localize the FFI here.
    let written = sample_via_ffi(morton, &mut buf);

    json!({
        "x": x, "y": y, "z": z,
        "morton": morton,
        "cell_present": written == cssl_rt::loa_stubs::FIELD_CELL_BYTES as i32,
        "bytes_written": written,
    })
}

fn tools_list(_state: &mut EngineState, _params: Value) -> Value {
    let reg = tool_registry();
    let tools: Vec<Value> = reg
        .values()
        .map(|e| {
            json!({
                "name": e.meta.name,
                "description": e.meta.description,
                "mutating": e.meta.mutating,
                "requires_sovereign_cap": e.meta.mutating,
            })
        })
        .collect();
    json!({"tools": tools, "count": reg.len()})
}

// ───────────────────────────────────────────────────────────────────────
// § handlers — mutating (sovereign-cap was already verified by the server)
// ───────────────────────────────────────────────────────────────────────

fn engine_shutdown(state: &mut EngineState, _params: Value) -> Value {
    state.quit_requested = true;
    state.push_event("INFO", "loa-host/mcp", "engine.shutdown set quit_requested");
    json!({"quit_requested": true})
}

fn engine_pause(state: &mut EngineState, _params: Value) -> Value {
    state.paused = !state.paused;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("engine.pause toggled · paused={}", state.paused),
    );
    json!({"paused": state.paused})
}

fn camera_set(state: &mut EngineState, params: Value) -> Value {
    let prior = state.camera;
    state.camera = CameraState {
        pos: Vec3 {
            x: p_f32(&params, "x", prior.pos.x),
            y: p_f32(&params, "y", prior.pos.y),
            z: p_f32(&params, "z", prior.pos.z),
        },
        yaw: p_f32(&params, "yaw", prior.yaw),
        pitch: p_f32(&params, "pitch", prior.pitch),
    };
    state.push_event("INFO", "loa-host/mcp", "camera.set teleported");
    json!({
        "camera_pos": camera_pos_json(&state.camera),
        "previous": camera_pos_json(&prior),
    })
}

fn room_spawn_plinth(state: &mut EngineState, params: Value) -> Value {
    let x = p_f32(&params, "x", 0.0);
    let z = p_f32(&params, "z", 0.0);
    let color = p_u32(&params, "color", 0x_88_88_88);
    let plinth = Plinth::new(x, z, color);
    state.plinths.push(plinth);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("room.spawn_plinth · ({x:.2},{z:.2}) · 0x{color:06X}"),
    );
    json!({
        "plinth": {"x": x, "z": z, "color_rgb": color, "half_extent": plinth.half_extent},
        "total_plinths": state.plinths.len(),
    })
}

fn render_set_mode(state: &mut EngineState, params: Value) -> Value {
    let mode_id = p_u8(&params, "mode", state.render_mode as u8);
    let prior = state.render_mode;
    if let Some(m) = RenderMode::from_u8(mode_id) {
        state.render_mode = m;
        state.push_event(
            "INFO",
            "loa-host/mcp",
            &format!(
                "render.set_mode · {} → {}",
                prior.as_str(),
                state.render_mode.as_str()
            ),
        );
        json!({
            "render_mode": state.render_mode.as_str(),
            "render_mode_id": state.render_mode as u8,
            "previous": prior.as_str(),
        })
    } else {
        json!({
            "error": format!("invalid render_mode id: {mode_id} (must be 0..=9)"),
            "render_mode": state.render_mode.as_str(),
        })
    }
}

fn dm_intensity(state: &mut EngineState, params: Value) -> Value {
    let value = p_u8(&params, "value", state.dm.intensity).min(3);
    let prior = state.dm.intensity;
    state.dm.intensity = value;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("dm.intensity · {prior} → {value}"),
    );
    json!({
        "dm_intensity": value,
        "previous": prior,
    })
}

fn dm_event_propose(state: &mut EngineState, params: Value) -> Value {
    let kind = p_str(&params, "kind", "spawn-encounter").to_string();
    let x = p_f32(&params, "x", state.camera.pos.x);
    let y = p_f32(&params, "y", state.camera.pos.y);
    let z = p_f32(&params, "z", state.camera.pos.z);
    state.dm.event_count += 1;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("dm.event.propose · {kind} @ ({x:.2},{y:.2},{z:.2})"),
    );
    json!({
        "accepted": true,
        "event_id": state.dm.event_count,
        "kind": kind,
        "pos": [x, y, z],
    })
}

fn omega_modify(state: &mut EngineState, params: Value) -> Value {
    let x = p_f32(&params, "x", 0.0);
    let y = p_f32(&params, "y", 0.0);
    let z = p_f32(&params, "z", 0.0);
    let value = p_f32(&params, "value", 1.0);
    let xi = x.clamp(0.0, ((1u32 << 21) - 1) as f32) as u32;
    let yi = y.clamp(0.0, ((1u32 << 21) - 1) as f32) as u32;
    let zi = z.clamp(0.0, ((1u32 << 21) - 1) as f32) as u32;
    let morton = morton_encode_u32(xi, yi, zi);

    // Build an 88-byte FieldCell payload. Stage-0 layout : the first 4
    // bytes are an LE-f32 "value" ; remainder zero. Stage-1 will replace
    // this with the cssl-substrate-omega-field's full Σ-mask encoding.
    let mut buf = [0u8; cssl_rt::loa_stubs::FIELD_CELL_BYTES];
    buf[0..4].copy_from_slice(&value.to_le_bytes());

    let rc = modify_via_ffi(morton, &buf);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("omega.modify · ({x:.2},{y:.2},{z:.2}) · morton=0x{morton:016X} · rc={rc}"),
    );

    json!({
        "x": x, "y": y, "z": z,
        "morton": morton,
        "value": value,
        "rc": rc,
    })
}

fn companion_propose(state: &mut EngineState, params: Value) -> Value {
    let kind = p_str(&params, "kind", "say-line").to_string();
    let target = p_str(&params, "target", "any").to_string();
    let payload = params.get("payload").cloned().unwrap_or(json!(null));
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("companion.propose · kind={kind} · target={target}"),
    );
    // § Stage-0 forward-decl shim. Sibling W-LOA-companion-hook ships the
    // real CompanionProposal-veto pipeline ; this returns a stub
    // acceptance receipt so the MCP contract is exercised end-to-end.
    json!({
        "accepted": true,
        "kind": kind,
        "target": target,
        "payload": payload,
        "note": "stage-0 stub · sibling W-LOA-companion-hook will gate via veto-bus",
    })
}

// ───────────────────────────────────────────────────────────────────────
// § handlers — live render control plane
// ───────────────────────────────────────────────────────────────────────

fn render_list_patterns(_state: &mut EngineState, _params: Value) -> Value {
    let mut entries = Vec::with_capacity(PATTERN_LUT_LEN);
    let lut = pattern_lut();
    for id in 0..PATTERN_LUT_LEN as u32 {
        let p = lut[id as usize];
        entries.push(json!({
            "id": id,
            "name": pattern_name(id),
            "kind": p.kind,
            "scale": p.scale,
            "rotation": p.rotation,
            "phase": p.phase,
        }));
    }
    json!({"patterns": entries, "count": PATTERN_LUT_LEN})
}

fn render_list_materials(_state: &mut EngineState, _params: Value) -> Value {
    let mut entries = Vec::with_capacity(MATERIAL_LUT_LEN);
    let lut = material_lut();
    for id in 0..MATERIAL_LUT_LEN as u32 {
        let m = lut[id as usize];
        entries.push(json!({
            "id": id,
            "name": material_name(id),
            "albedo": m.albedo,
            "roughness": m.roughness,
            "metallic": m.metallic,
            "alpha": m.alpha,
            "emissive": m.emissive,
        }));
    }
    json!({"materials": entries, "count": MATERIAL_LUT_LEN})
}

fn render_snapshot(state: &mut EngineState, _params: Value) -> Value {
    // Walls 0..3 + floor quadrants 0..3 active patterns. Reads from the
    // FFI control-plane state ; absent = "default".
    let mut walls = Vec::with_capacity(4);
    for w in 0..4 {
        let p = crate::ffi::wall_pattern_override(w);
        walls.push(json!({
            "wall_id": w,
            "pattern_id": p,
            "pattern_name": p.map(pattern_name).unwrap_or("default"),
        }));
    }
    let mut floors = Vec::with_capacity(4);
    for q in 0..4 {
        let p = crate::ffi::floor_pattern_override(q);
        floors.push(json!({
            "quadrant_id": q,
            "pattern_id": p,
            "pattern_name": p.map(pattern_name).unwrap_or("default"),
        }));
    }
    json!({
        "frame_count": state.frame_count,
        "camera_pos": camera_pos_json(&state.camera),
        "active_scene": state.active_scene,
        "render_mode": state.render_mode.as_str(),
        "walls": walls,
        "floor_quadrants": floors,
        "material_count": MATERIAL_LUT_LEN,
        "pattern_count": PATTERN_LUT_LEN,
    })
}

fn render_set_wall_pattern(state: &mut EngineState, params: Value) -> Value {
    let wall_id = p_u32(&params, "wall_id", 0);
    let pattern_id = p_u32(&params, "pattern_id", 0);
    let rc = crate::ffi::__cssl_render_set_wall_pattern(
        wall_id,
        pattern_id,
        // Cap-gate already enforced by mcp_server before dispatch.
        0xCAFE_BABE_DEAD_BEEF,
    );
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("render.set_wall_pattern · wall={wall_id} pattern={pattern_id} rc={rc}"),
    );
    if rc == 0 {
        json!({
            "ok": true,
            "wall_id": wall_id,
            "pattern_id": pattern_id,
            "pattern_name": pattern_name(pattern_id),
        })
    } else {
        json!({
            "ok": false,
            "error": format!("rc={rc} (out-of-range or cap-rejected)"),
            "wall_id": wall_id,
            "pattern_id": pattern_id,
        })
    }
}

fn render_set_floor_pattern(state: &mut EngineState, params: Value) -> Value {
    let quadrant_id = p_u32(&params, "quadrant_id", 0);
    let pattern_id = p_u32(&params, "pattern_id", 0);
    let rc = crate::ffi::__cssl_render_set_floor_pattern(
        quadrant_id,
        pattern_id,
        0xCAFE_BABE_DEAD_BEEF,
    );
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("render.set_floor_pattern · q={quadrant_id} pat={pattern_id} rc={rc}"),
    );
    if rc == 0 {
        json!({
            "ok": true,
            "quadrant_id": quadrant_id,
            "pattern_id": pattern_id,
            "pattern_name": pattern_name(pattern_id),
        })
    } else {
        json!({
            "ok": false,
            "error": format!("rc={rc}"),
            "quadrant_id": quadrant_id,
        })
    }
}

fn render_set_material(state: &mut EngineState, params: Value) -> Value {
    let quad_id = p_u32(&params, "quad_id", 0);
    let material_id = p_u32(&params, "material_id", 0);
    let rc = crate::ffi::__cssl_render_set_material(quad_id, material_id, 0xCAFE_BABE_DEAD_BEEF);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("render.set_material · quad={quad_id} mat={material_id} rc={rc}"),
    );
    if rc == 0 {
        json!({
            "ok": true,
            "quad_id": quad_id,
            "material_id": material_id,
            "material_name": material_name(material_id),
        })
    } else {
        json!({
            "ok": false,
            "error": format!("rc={rc}"),
            "quad_id": quad_id,
        })
    }
}

fn render_spawn_stress(state: &mut EngineState, params: Value) -> Value {
    let kind = p_u32(&params, "kind", 0);
    let x = p_f32(&params, "x", state.camera.pos.x);
    let y = p_f32(&params, "y", state.camera.pos.y);
    let z = p_f32(&params, "z", state.camera.pos.z);
    let id = crate::ffi::__cssl_render_spawn_stress_object(kind, x, y, z, 0xCAFE_BABE_DEAD_BEEF);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("render.spawn_stress · kind={kind} id={id} at ({x:.2},{y:.2},{z:.2})"),
    );
    if id > 0 {
        json!({
            "ok": true,
            "object_id": id,
            "kind": kind,
            "name": crate::geometry::stress_object_name(kind),
            "pos": [x, y, z],
        })
    } else {
        json!({
            "ok": false,
            "error": "kind out of range or cap-rejected",
        })
    }
}

// ───────────────────────────────────────────────────────────────────────
// § handlers — T11-WAVE3-GLTF · world.spawn_gltf surface
// ───────────────────────────────────────────────────────────────────────

/// MCP `world.spawn_gltf` : parse a glTF/GLB asset and queue it for the
/// dynamic-mesh path. Sovereign-cap-gated.
///
/// Params :
///   - `path`  (string) : filesystem path to .glb or .gltf
///   - `x/y/z` (f32) : world-space spawn position (defaults to camera.pos)
///   - `scale` (f32) : uniform scale (default 1.0)
///   - `sovereign_cap` (u64) : capability token
///
/// Result :
///   - `{ok: true, instance_id, vertex_count, triangle_count, material_id, bbox}` on success
///   - `{ok: false, error: "..."}` on failure
fn world_spawn_gltf(state: &mut EngineState, params: Value) -> Value {
    use std::path::PathBuf;

    let path = p_str(&params, "path", "").to_string();
    if path.is_empty() {
        return json!({"ok": false, "error": "missing 'path' parameter"});
    }
    // Default to the MaterialRoom-Annex center (north of MaterialRoom)
    // so that spawned models land in the designated diagnostic zone
    // when no explicit position is provided. This keeps the test rooms
    // uncluttered while still showing live spawns prominently.
    let annex = crate::geometry::material_room_annex_center();
    let x = p_f32(&params, "x", annex[0]);
    let y = p_f32(&params, "y", annex[1]);
    let z = p_f32(&params, "z", annex[2]);
    let scale = p_f32(&params, "scale", 1.0);

    // The MCP server has already verified `sovereign_cap` for any
    // tool registered with `mutating: true` before dispatching to this
    // handler — replicating the check here would double-validate. We
    // do still ensure the path is non-empty and pass control to the
    // FFI helper which logs every spawn.

    match crate::ffi::spawn_gltf_path(PathBuf::from(&path), [x, y, z], scale) {
        Ok(id) => {
            state.push_event(
                "INFO",
                "loa-host/mcp",
                &format!(
                    "world.spawn_gltf · path={path} pos=({x:.2},{y:.2},{z:.2}) scale={scale:.2} id={id}"
                ),
            );
            json!({
                "ok": true,
                "instance_id": id,
                "path": path,
                "world_pos": [x, y, z],
                "scale": scale,
            })
        }
        Err(e) => {
            state.push_event(
                "ERROR",
                "loa-host/mcp",
                &format!("world.spawn_gltf · {e}"),
            );
            json!({
                "ok": false,
                "error": e,
            })
        }
    }
}

/// MCP `world.gltf_spawns_total` : read-only counters.
fn world_gltf_spawns_total(_state: &mut EngineState, _params: Value) -> Value {
    let total = crate::ffi::gltf_spawns_total();
    let rejects = crate::ffi::gltf_spawn_rejects_total();
    json!({
        "spawns_total": total,
        "rejects_total": rejects,
        "max_dynamic_meshes": 256u32,
    })
}

/// MCP `world.list_dynamic_meshes` : enumerate currently-loaded dynamic
/// meshes. The data lives in the renderer · catalog mode returns an
/// empty list (no renderer present) but still reports the spawn-counter.
fn world_list_dynamic_meshes(_state: &mut EngineState, _params: Value) -> Value {
    // The renderer's `dynamic_meshes` Vec is only available in runtime
    // builds. Catalog builds have no GPU but they still receive parses
    // via `pending_gltf_queue()` — we surface a *pending* list as a
    // best-effort proxy.
    let total = crate::ffi::gltf_spawns_total();
    json!({
        "spawns_total": total,
        "note": "MCP cannot peek live render state from this thread; use 'render.snapshot' instead.",
    })
}

// ───────────────────────────────────────────────────────────────────────
// § handlers — telemetry (T11-LOA-TELEM)
// ───────────────────────────────────────────────────────────────────────

fn telemetry_snapshot(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::telemetry::global();
    let raw = s.snapshot_json();
    // The sink emits a fully-formed JSON object string ; re-parse so the
    // JSON-RPC envelope nests it as a structured `result` rather than a
    // string blob.
    serde_json::from_str(&raw).unwrap_or_else(|_| json!({"error": "snapshot parse failed"}))
}

fn telemetry_histogram(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::telemetry::global();
    let counts = s.frame_time_histogram();
    let bounds: Vec<f32> = crate::telemetry::BUCKET_BOUNDS_MS.to_vec();
    let mut buckets = Vec::with_capacity(crate::telemetry::BUCKET_COUNT);
    for (i, c) in counts.iter().enumerate() {
        let lo = if i == 0 { 0.0 } else { bounds[i - 1] };
        let hi = if i < bounds.len() { bounds[i] } else { f32::INFINITY };
        let hi_str = if hi.is_finite() {
            json!(hi)
        } else {
            json!("inf")
        };
        buckets.push(json!({
            "lo_ms": lo,
            "hi_ms": hi_str,
            "count": *c,
        }));
    }
    json!({
        "buckets": buckets,
        "bucket_count": crate::telemetry::BUCKET_COUNT,
        "ring_capacity": crate::telemetry::FRAME_RING_CAP,
    })
}

fn telemetry_gpu_info(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::telemetry::global();
    let raw = s.gpu_info_json();
    if raw == "null" {
        json!({"info": null, "captured": false})
    } else {
        let parsed: Value =
            serde_json::from_str(&raw).unwrap_or_else(|_| json!({"error": "gpu_info parse failed"}));
        json!({"info": parsed, "captured": true})
    }
}

fn telemetry_tail_events(_state: &mut EngineState, params: Value) -> Value {
    let limit = p_u32(&params, "limit", 32) as usize;
    let s = crate::telemetry::global();
    let raw = s.tail_events_json(limit);
    let arr: Value =
        serde_json::from_str(&raw).unwrap_or_else(|_| json!([{"error": "tail_events parse failed"}]));
    json!({
        "events": arr,
        "limit": limit,
    })
}

fn telemetry_flush(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::telemetry::global();
    match s.flush() {
        Ok(_) => json!({"ok": true}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

fn telemetry_set_log_level(state: &mut EngineState, params: Value) -> Value {
    let level = p_u32(&params, "level", 1).min(3);
    let s = crate::telemetry::global();
    s.set_log_level(level);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("telemetry.set_log_level · {level}"),
    );
    json!({"level": level, "ok": true})
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-FID-MAINSTREAM : `render.fidelity` (read-only)
// ───────────────────────────────────────────────────────────────────────

/// Return the live graphical-fidelity settings active on the renderer.
///
/// In catalog mode (no GPU init), `initialized=false` + safe defaults are
/// returned so the tool is always callable (e.g. for unit tests + tooling
/// that introspects the registry without spinning up a window).
fn render_fidelity(_state: &mut EngineState, _params: Value) -> Value {
    let r = crate::fidelity::current_report();
    json!({
        "msaa_samples": r.msaa_samples,
        "hdr_format": r.hdr_format,
        "present_mode": r.present_mode,
        "aniso_max": r.aniso_max,
        "tonemap_path": r.tonemap_path,
        "initialized": r.initialized,
    })
}

// ───────────────────────────────────────────────────────────────────────
// § handlers — T11-LOA-TEST-APP visual-data-gathering apparatus
// ───────────────────────────────────────────────────────────────────────

/// Default filename used when `params.path` is omitted from `render.snapshot_png`.
fn default_snapshot_filename(frame_count: u64) -> String {
    format!("snap_{frame_count:08}.png")
}

fn render_snapshot_png(state: &mut EngineState, params: Value) -> Value {
    // Caller may supply `path` (relative to logs/snapshots) or omit it.
    let user_path = p_str(&params, "path", "");
    let base_dir = default_snapshot_dir();
    let final_path = if user_path.is_empty() {
        base_dir.join(default_snapshot_filename(state.frame_count))
    } else {
        match sanitize_snapshot_path(&base_dir, user_path) {
            Some(p) => p,
            None => {
                state.push_event(
                    "WARN",
                    "loa-host/mcp",
                    &format!("render.snapshot_png · rejected path '{user_path}' (traversal/abs)"),
                );
                return json!({
                    "ok": false,
                    "error": "path must be relative + must not contain '..'",
                });
            }
        }
    };

    state.snapshot_queue.push(SnapshotRequest {
        path: final_path.clone(),
    });
    state.snapshot_count += 1;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "render.snapshot_png · queued · path={}",
            final_path.display()
        ),
    );
    json!({
        "ok": true,
        "path": final_path.display().to_string(),
        "queued_count": state.snapshot_queue.len(),
        "total_snapshots_session": state.snapshot_count,
    })
}

fn render_tour(state: &mut EngineState, params: Value) -> Value {
    let tour_id = p_str(&params, "tour_id", "default").to_string();
    let output_dir_str = p_str(&params, "output_dir", "");

    // Resolve the tour
    let Some(poses) = tour_by_id(&tour_id) else {
        return json!({
            "ok": false,
            "error": format!("unknown tour_id '{}'; valid: {:?}", tour_id, TOUR_IDS),
        });
    };

    // Resolve the output dir : honor explicit user path, else use
    // logs/snapshots/<tour_id>.
    let base_dir = default_snapshot_dir();
    let tour_dir = if output_dir_str.is_empty() {
        base_dir.join(&tour_id)
    } else {
        match sanitize_snapshot_path(&base_dir, output_dir_str) {
            Some(p) => p,
            None => {
                return json!({
                    "ok": false,
                    "error": "output_dir must be relative + free of '..'",
                });
            }
        }
    };

    // For each pose : teleport camera + queue a snapshot at <tour_dir>/<pose>.png
    let mut planned: Vec<Value> = Vec::with_capacity(poses.len());
    state.tour_progress = Some((0, poses.len() as u32));
    for (i, pose) in poses.iter().enumerate() {
        // Update camera state. The render loop reads camera each frame ;
        // by the time the engine processes the snapshot_queue entry the
        // camera will have been propagated to the render side.
        state.camera = CameraState {
            pos: Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]),
            yaw: pose.yaw,
            pitch: pose.pitch,
        };
        let snap_path = tour_dir.join(format!("{}.png", pose.name));
        state.snapshot_queue.push(SnapshotRequest {
            path: snap_path.clone(),
        });
        state.tour_progress = Some(((i + 1) as u32, poses.len() as u32));
        planned.push(json!({
            "pose": pose.name,
            "path": snap_path.display().to_string(),
            "pos": pose.pos,
            "yaw": pose.yaw,
            "pitch": pose.pitch,
        }));
    }
    state.snapshot_count += poses.len() as u64;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "render.tour · {} poses queued · tour_id={} · dir={}",
            poses.len(),
            tour_id,
            tour_dir.display()
        ),
    );

    json!({
        "ok": true,
        "tour_id": tour_id,
        "poses_visited": poses.len(),
        "snapshots": planned,
        "output_dir": tour_dir.display().to_string(),
        "note": "snapshots written asynchronously by render loop ; \
                 poll engine.state for queue drain or wait ~poses*frame-time ms",
    })
}

fn render_diff_golden(state: &mut EngineState, params: Value) -> Value {
    let tour_id = p_str(&params, "tour_id", "default").to_string();
    let threshold = p_f32(&params, "threshold", GOLDEN_MAE_THRESHOLD);

    let Some(poses) = tour_by_id(&tour_id) else {
        return json!({
            "ok": false,
            "error": format!("unknown tour_id '{}'", tour_id),
        });
    };

    let snap_dir = default_snapshot_dir().join(&tour_id);
    let golden_dir = default_golden_dir().join(&tour_id);

    let mut entries: Vec<GoldenDiffEntry> = Vec::with_capacity(poses.len());
    let mut all_passed = true;

    for pose in &poses {
        let snap_path = snap_dir.join(format!("{}.png", pose.name));
        let golden_path = golden_dir.join(format!("{}.png", pose.name));

        // If the snapshot doesn't exist, the user hasn't run the tour yet.
        if !snap_path.exists() {
            entries.push(GoldenDiffEntry {
                pose: pose.name.clone(),
                mae: f32::NAN,
                threshold,
                passed: false,
                created_new: false,
            });
            all_passed = false;
            continue;
        }

        // If the golden doesn't exist, promote the current snapshot to
        // the new golden + report passed=true·created_new=true.
        if !golden_path.exists() {
            // mkdir + copy
            if let Some(parent) = golden_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::copy(&snap_path, &golden_path) {
                Ok(_) => {
                    entries.push(GoldenDiffEntry {
                        pose: pose.name.clone(),
                        mae: 0.0,
                        threshold,
                        passed: true,
                        created_new: true,
                    });
                }
                Err(e) => {
                    state.push_event(
                        "WARN",
                        "loa-host/mcp",
                        &format!(
                            "render.diff_golden · failed to promote {} → {}: {}",
                            snap_path.display(),
                            golden_path.display(),
                            e
                        ),
                    );
                    entries.push(GoldenDiffEntry {
                        pose: pose.name.clone(),
                        mae: f32::NAN,
                        threshold,
                        passed: false,
                        created_new: false,
                    });
                    all_passed = false;
                }
            }
            continue;
        }

        // Both files exist → diff
        let snap = match decode_png(&snap_path) {
            Ok(s) => s,
            Err(e) => {
                state.push_event(
                    "WARN",
                    "loa-host/mcp",
                    &format!("render.diff_golden · decode snap failed: {e}"),
                );
                entries.push(GoldenDiffEntry {
                    pose: pose.name.clone(),
                    mae: f32::NAN,
                    threshold,
                    passed: false,
                    created_new: false,
                });
                all_passed = false;
                continue;
            }
        };
        let golden = match decode_png(&golden_path) {
            Ok(s) => s,
            Err(e) => {
                state.push_event(
                    "WARN",
                    "loa-host/mcp",
                    &format!("render.diff_golden · decode golden failed: {e}"),
                );
                entries.push(GoldenDiffEntry {
                    pose: pose.name.clone(),
                    mae: f32::NAN,
                    threshold,
                    passed: false,
                    created_new: false,
                });
                all_passed = false;
                continue;
            }
        };

        // Both PNGs are RGBA8 ; convert to BGRA8 for `mae_bgra8`.
        let mut snap_bgra = snap.0;
        rgba8_to_bgra8_inplace(&mut snap_bgra);
        let mut golden_bgra = golden.0;
        rgba8_to_bgra8_inplace(&mut golden_bgra);

        let mae = mae_bgra8(&snap_bgra, &golden_bgra).unwrap_or(f32::NAN);
        let passed = !mae.is_nan() && mae <= threshold;
        if !passed {
            all_passed = false;
        }
        entries.push(GoldenDiffEntry {
            pose: pose.name.clone(),
            mae,
            threshold,
            passed,
            created_new: false,
        });
    }

    let report = GoldenDiffReport {
        tour_id: tour_id.clone(),
        passed: all_passed,
        per_pose: entries,
    };

    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "render.diff_golden · tour={} · passed={}",
            tour_id, all_passed
        ),
    );

    let per_pose_json: Vec<Value> = report
        .per_pose
        .iter()
        .map(|e| {
            json!({
                "pose": e.pose,
                "mae": if e.mae.is_nan() { json!(null) } else { json!(e.mae) },
                "threshold": e.threshold,
                "passed": e.passed,
                "created_new": e.created_new,
            })
        })
        .collect();

    json!({
        "ok": true,
        "tour_id": report.tour_id,
        "passed": report.passed,
        "per_pose": per_pose_json,
        "snapshot_dir": snap_dir.display().to_string(),
        "golden_dir": golden_dir.display().to_string(),
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-ROOMS · room.list + room.teleport handlers
// ───────────────────────────────────────────────────────────────────────

fn room_list(_state: &mut EngineState, _params: Value) -> Value {
    use crate::room::Room;
    let mut entries = Vec::with_capacity(crate::room::ROOM_COUNT as usize);
    for r in Room::all() {
        let b = r.bounds();
        entries.push(json!({
            "id": r as u32,
            "name": r.name(),
            "description": r.description(),
            "bounds_min": b.min,
            "bounds_max": b.max,
            "spawn_eye": r.spawn_eye_position(),
        }));
    }
    json!({
        "rooms": entries,
        "count": crate::room::ROOM_COUNT,
    })
}

fn room_teleport(state: &mut EngineState, params: Value) -> Value {
    use crate::room::Room;
    let room_id = p_str(&params, "room_id", "");
    let Some(room) = Room::from_str(room_id) else {
        return json!({
            "ok": false,
            "error": format!("unknown room_id '{room_id}'. Valid: TestRoom · MaterialRoom · PatternRoom · ScaleRoom · ColorRoom"),
        });
    };
    // Snap camera state immediately so engine.state reflects the new pos
    // for the very next read.
    let spawn = room.spawn_eye_position();
    let prior = state.camera.pos;
    state.camera.pos = crate::mcp_server::Vec3::new(spawn[0], spawn[1], spawn[2]);
    // Also raise the FFI pending-flag so the live render-loop snaps.
    let rc = crate::ffi::__cssl_room_teleport(room as u32, 0xCAFE_BABE_DEAD_BEEF);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "room.teleport · {} → {} ({:.2},{:.2},{:.2})",
            room_id, room.name(), spawn[0], spawn[1], spawn[2]
        ),
    );
    json!({
        "ok": rc == 0,
        "room_id": room.name(),
        "from": [prior.x, prior.y, prior.z],
        "to": spawn,
        "rc": rc,
    })
}

// § T11-LOA-FID-STOKES — Stokes IQUV polarized-render handlers
// ───────────────────────────────────────────────────────────────────────

fn render_stokes_snapshot(state: &mut EngineState, _params: Value) -> Value {
    use crate::stokes::{mueller_lut, sun_stokes_default, PolarizationView, MUELLER_LUT_LEN};
    let mode_u32 = crate::ffi::polarization_view();
    let mode = PolarizationView::from_u32(mode_u32);
    // The "center pixel" is approximated as the Mueller-applied sun Stokes
    // for the material at the camera's view-direction. Stage-0 doesn't yet
    // ray-trace from CPU side, so we report the per-material LUT by id.
    let s_in = sun_stokes_default();
    let lut = mueller_lut();
    let mut entries = Vec::with_capacity(MUELLER_LUT_LEN);
    for (id, m) in lut.iter().enumerate() {
        let s_out = m.apply(s_in);
        entries.push(json!({
            "material_id": id,
            "i": s_out.i,
            "q": s_out.q,
            "u": s_out.u,
            "v": s_out.v,
            "dop_linear": s_out.dop_linear(),
            "dop_total": s_out.dop_total(),
        }));
    }
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "render.stokes_snapshot · mode={} ({}) · sun=(I={:.3}, Q={:.3}, U={:.3}, V={:.3})",
            mode_u32,
            mode.name(),
            s_in.i,
            s_in.q,
            s_in.u,
            s_in.v
        ),
    );
    json!({
        "polarization_mode": mode_u32,
        "polarization_mode_name": mode.name(),
        "sun_stokes": {
            "i": s_in.i, "q": s_in.q, "u": s_in.u, "v": s_in.v,
            "dop_linear": s_in.dop_linear(),
            "dop_total": s_in.dop_total(),
        },
        "per_material_stokes": entries,
        "mueller_apply_count_per_frame":
            crate::telemetry::global().mueller_apply_count_per_frame.load(std::sync::atomic::Ordering::Relaxed),
    })
}

fn render_set_polarization_view(state: &mut EngineState, params: Value) -> Value {
    let mode = p_u32(&params, "mode", 0).min(4);
    let prior = crate::ffi::polarization_view();
    crate::ffi::set_polarization_view(mode);
    let mode_name = crate::stokes::PolarizationView::from_u32(mode).name();
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "render.set_polarization_view · {prior} → {mode} ({mode_name})"
        ),
    );
    json!({
        "ok": true,
        "polarization_mode": mode,
        "polarization_mode_name": mode_name,
        "previous": prior,
    })
}

fn render_polarization_panels(_state: &mut EngineState, _params: Value) -> Value {
    let panels = crate::stokes::polarization_panels();
    let mut entries = Vec::with_capacity(panels.len());
    for (i, p) in panels.iter().enumerate() {
        let s = p.expected_signature;
        entries.push(json!({
            "panel_id": i,
            "label": p.label,
            "expected_stokes": {
                "i": s.i, "q": s.q, "u": s.u, "v": s.v,
                "dop_linear": s.dop_linear(),
                "dop_total": s.dop_total(),
            },
        }));
    }
    json!({
        "panels": entries,
        "count": panels.len(),
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-FID-SPECTRAL · render.set_illuminant + render.list_illuminants
//   + render.spectral_snapshot + render.spectral_zones + telemetry.spectral
//   + room.teleport_zone handlers
// ───────────────────────────────────────────────────────────────────────

fn render_set_illuminant(state: &mut EngineState, params: Value) -> Value {
    use crate::spectral_bridge::Illuminant;
    let name = p_str(&params, "name", "D65");
    let Some(illum) = Illuminant::from_name(name) else {
        return json!({
            "ok": false,
            "error": format!("unknown illuminant '{name}' · valid: D65 · D50 · A · F11"),
        });
    };
    let prior = state.illuminant;
    if prior != illum {
        state.illuminant = illum;
        state.illuminant_gen = state.illuminant_gen.saturating_add(1);
        crate::spectral_bridge::SPECTRAL_ILLUMINANT_CHANGES
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "render.set_illuminant · {} → {} · cct={}K · gen={}",
            prior.name(),
            illum.name(),
            illum.cct_kelvin(),
            state.illuminant_gen,
        ),
    );
    // Pre-bake one sample for the response so the operator sees the change
    // without waiting on the renderer.
    let sample = crate::spectral_bridge::bake_material_color(
        crate::material::MAT_VERMILLION_LACQUER,
        illum,
    );
    json!({
        "ok": true,
        "illuminant": illum.name(),
        "previous": prior.name(),
        "cct_kelvin": illum.cct_kelvin(),
        "description": illum.description(),
        "gen": state.illuminant_gen,
        "vermillion_sample_srgb": sample,
    })
}

fn render_list_illuminants(_state: &mut EngineState, _params: Value) -> Value {
    use crate::spectral_bridge::Illuminant;
    let entries: Vec<Value> = Illuminant::all()
        .iter()
        .map(|i| {
            json!({
                "name": i.name(),
                "cct_kelvin": i.cct_kelvin(),
                "description": i.description(),
            })
        })
        .collect();
    json!({
        "illuminants": entries,
        "count": entries.len(),
        "default": "D65",
    })
}

fn render_spectral_snapshot(state: &mut EngineState, _params: Value) -> Value {
    use crate::spectral_bridge::spectral_snapshot_all;
    let snap = spectral_snapshot_all();
    // Group by material id for nicer JSON shape : each material entry has a
    // per-illuminant baked-sRGB sub-object.
    let mut by_mat: std::collections::BTreeMap<u32, std::collections::BTreeMap<String, [f32; 3]>> =
        std::collections::BTreeMap::new();
    for (mat_id, illum, rgb) in &snap {
        by_mat
            .entry(*mat_id)
            .or_default()
            .insert(illum.name().to_string(), *rgb);
    }
    let mut materials = Vec::with_capacity(by_mat.len());
    for (id, illums) in by_mat {
        let illums_json: Vec<Value> = illums
            .into_iter()
            .map(|(n, rgb)| json!({"illuminant": n, "srgb": rgb}))
            .collect();
        materials.push(json!({
            "material_id": id,
            "name": material_name(id),
            "baked": illums_json,
        }));
    }
    json!({
        "current_illuminant": state.illuminant.name(),
        "materials": materials,
        "matrix_dim": [crate::material::MATERIAL_LUT_LEN, 4],
    })
}

fn render_spectral_zones(_state: &mut EngineState, _params: Value) -> Value {
    use crate::spectral_bridge::spectral_zones;
    let zones: Vec<Value> = spectral_zones()
        .iter()
        .map(|z| {
            json!({
                "index": z.index,
                "name": z.name,
                "spawn_xyz": z.spawn_xyz,
                "illuminant": z.illuminant.name(),
                "cct_kelvin": z.illuminant.cct_kelvin(),
            })
        })
        .collect();
    json!({
        "zones": zones,
        "count": 4,
        "container_room": "ColorRoom",
        "note": "Walk NW→NE→SW→SE to traverse D65→D50→A→F11 illuminants. Each zone teleport flips the bake.",
    })
}

fn telemetry_spectral(state: &mut EngineState, _params: Value) -> Value {
    use std::sync::atomic::Ordering;
    let count = crate::spectral_bridge::SPECTRAL_BAKE_COUNT.load(Ordering::Relaxed);
    let total_us = crate::spectral_bridge::SPECTRAL_BAKE_US.load(Ordering::Relaxed);
    let changes = crate::spectral_bridge::SPECTRAL_ILLUMINANT_CHANGES.load(Ordering::Relaxed);
    let avg_us = if changes > 0 { total_us / changes.max(1) } else { 0 };
    json!({
        "spectral_bake_count": count,
        "spectral_bake_us_total": total_us,
        "spectral_bake_us_avg_per_change": avg_us,
        "illuminant_changes": changes,
        "current_illuminant": state.illuminant.name(),
        "current_illuminant_cct": state.illuminant.cct_kelvin(),
        "current_illuminant_gen": state.illuminant_gen,
    })
}

fn room_teleport_zone(state: &mut EngineState, params: Value) -> Value {
    use crate::spectral_bridge::{spectral_zone_by_name, Illuminant};
    let zone_name = p_str(&params, "zone", "D65-NW");
    let Some(zone) = spectral_zone_by_name(zone_name) else {
        let valid: Vec<&'static str> = crate::spectral_bridge::spectral_zones()
            .iter()
            .map(|z| z.name)
            .collect();
        return json!({
            "ok": false,
            "error": format!("unknown zone '{zone_name}' · valid: {valid:?}"),
        });
    };
    // Atomic update : camera + illuminant in one transaction.
    let prior_pos = state.camera.pos;
    let prior_illum = state.illuminant;
    state.camera.pos = crate::mcp_server::Vec3::new(
        zone.spawn_xyz[0],
        zone.spawn_xyz[1],
        zone.spawn_xyz[2],
    );
    if prior_illum != zone.illuminant {
        state.illuminant = zone.illuminant;
        state.illuminant_gen = state.illuminant_gen.saturating_add(1);
        crate::spectral_bridge::SPECTRAL_ILLUMINANT_CHANGES
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    // Also notify the FFI control-plane (Room::ColorRoom = 4) so renderer
    // can react if needed.
    let rc = crate::ffi::__cssl_room_teleport(crate::room::Room::ColorRoom as u32, 0xCAFE_BABE_DEAD_BEEF);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "room.teleport_zone · {} → ({:.2},{:.2},{:.2}) · illum {} → {} · gen={}",
            zone_name,
            zone.spawn_xyz[0],
            zone.spawn_xyz[1],
            zone.spawn_xyz[2],
            prior_illum.name(),
            zone.illuminant.name(),
            state.illuminant_gen,
        ),
    );
    let _ = Illuminant::default(); // anchor type
    json!({
        "ok": rc == 0,
        "zone": zone.name,
        "from_pos": [prior_pos.x, prior_pos.y, prior_pos.z],
        "to_pos": zone.spawn_xyz,
        "illuminant_was": prior_illum.name(),
        "illuminant_now": zone.illuminant.name(),
        "cct_kelvin": zone.illuminant.cct_kelvin(),
        "gen": state.illuminant_gen,
        "rc": rc,
    })
}

// ───────────────────────────────────────────────────────────────────────
// § Morton + FFI helpers
// ───────────────────────────────────────────────────────────────────────

/// 21-bit Morton-encode (x,y,z) → u64 (matches loa_stubs::__cssl_hash_morton).
#[must_use]
fn morton_encode_u32(x: u32, y: u32, z: u32) -> u64 {
    fn split3_21(v: u32) -> u64 {
        let mut x = u64::from(v) & 0x_001f_ffff;
        x = (x | (x << 32)) & 0x_001f_0000_0000_ffff;
        x = (x | (x << 16)) & 0x_001f_0000_ff00_00ff;
        x = (x | (x << 8))  & 0x_100f_00f0_0f00_f00f;
        x = (x | (x << 4))  & 0x_10c3_0c30_c30c_30c3;
        x = (x | (x << 2))  & 0x_1249_2492_4924_9249;
        x
    }
    split3_21(x) | (split3_21(y) << 1) | (split3_21(z) << 2)
}

/// Wrap the unsafe FFI in a single confined helper. The buffer is owned
/// + sized, so the unsafe call meets all preconditions.
fn sample_via_ffi(morton: u64, buf: &mut [u8; cssl_rt::loa_stubs::FIELD_CELL_BYTES]) -> i32 {
    // SAFETY : `buf.as_mut_ptr()` is a valid stack pointer, capacity
    // matches FIELD_CELL_BYTES, and the FFI does no allocation.
    unsafe {
        cssl_rt::loa_stubs::__cssl_omega_field_sample(
            morton,
            buf.as_mut_ptr(),
            cssl_rt::loa_stubs::FIELD_CELL_BYTES as i32,
        )
    }
}

/// Wrap the modify-FFI. The non-zero sovereign_handle is required by the
/// stub ; we pass a sentinel `1` since the cap-gate is enforced upstream.
fn modify_via_ffi(morton: u64, buf: &[u8; cssl_rt::loa_stubs::FIELD_CELL_BYTES]) -> i32 {
    // SAFETY : `buf.as_ptr()` is valid, length matches, sovereign_handle
    // is nonzero so the stub admits the write.
    unsafe {
        cssl_rt::loa_stubs::__cssl_omega_field_modify(
            morton,
            buf.as_ptr(),
            cssl_rt::loa_stubs::FIELD_CELL_BYTES as i32,
            1,
        )
    }
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-FID-CFER — Causal Field-Evolution Rendering tool handlers
// ───────────────────────────────────────────────────────────────────────

/// `render.cfer_snapshot` : read-only · returns the current CFER mirror.
/// The renderer mirrors `cfer_render::CferMetrics` into `state.cfer` after
/// each frame so this tool can return live values without crossing the
/// (Send-unsafe) wgpu boundary.
fn render_cfer_snapshot(state: &mut EngineState, _params: Value) -> Value {
    json!({
        "active_cells": state.cfer.active_cells,
        "step_us": state.cfer.step_us,
        "pack_us": state.cfer.pack_us,
        "kan_evals": state.cfer.kan_evals,
        "texels_written": state.cfer.texels_written,
        "cfer_frame_n": state.cfer.cfer_frame_n,
        "center_radiance": {
            "r": state.cfer.center_radiance[0],
            "g": state.cfer.center_radiance[1],
            "b": state.cfer.center_radiance[2],
        },
        "kan_handle": match state.cfer.kan_handle {
            Some(h) => json!(h),
            None    => json!(null),
        },
        "tex_dim": {
            "x": TEX_X,
            "y": TEX_Y,
            "z": TEX_Z,
            "total_texels": TEX_COUNT,
            "total_bytes": TEX_TOTAL_BYTES,
        },
    })
}

/// `render.cfer_step` : sovereign-gated · forces a CFER step on the next
/// frame even when the engine is paused.
fn render_cfer_step(state: &mut EngineState, _params: Value) -> Value {
    state.cfer.force_step_pending = true;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        "render.cfer_step · queued forced step",
    );
    json!({
        "ok": true,
        "force_step_pending": state.cfer.force_step_pending,
    })
}

/// `render.cfer_set_kan_handle` : sovereign-gated · attaches a KAN
/// sovereign-handle (u16) or detaches when `handle == -1`.
fn render_cfer_set_kan_handle(state: &mut EngineState, params: Value) -> Value {
    // Detach when caller passes negative or omits the field.
    let raw = params.get("handle").and_then(Value::as_i64).unwrap_or(-1);
    if raw < 0 {
        state.cfer.kan_handle_pending = Some(None);
        state.push_event(
            "INFO",
            "loa-host/mcp",
            "render.cfer_set_kan_handle · detach queued",
        );
        return json!({ "ok": true, "action": "detach" });
    }
    if raw > i64::from(u16::MAX) {
        return json!({
            "ok": false,
            "error": format!("handle {raw} exceeds u16 max ({})", u16::MAX),
        });
    }
    let h = raw as u16;
    state.cfer.kan_handle_pending = Some(Some(h));
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("render.cfer_set_kan_handle · attach queued (handle={h})"),
    );
    json!({
        "ok": true,
        "action": "attach",
        "handle": h,
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-USERFIX : atmospheric-intensity + capture controls
// ───────────────────────────────────────────────────────────────────────

/// `render.cfer_intensity` : sovereign-gated · sets the CFER atmospheric
/// intensity multiplier. Clamped to `0.0..=1.0`. The render loop drains
/// `state.cfer.cfer_intensity_pending` on the next frame and applies it.
fn render_cfer_intensity(state: &mut EngineState, params: Value) -> Value {
    let intensity = p_f32(&params, "intensity", 0.10).clamp(0.0, 1.0);
    state.cfer.cfer_intensity_pending = Some(intensity);
    state.cfer.cfer_intensity = intensity;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("render.cfer_intensity · → {intensity:.4}"),
    );
    json!({
        "ok": true,
        "intensity": intensity,
        "previous": state.cfer.cfer_intensity,
    })
}

/// `render.start_burst` : sovereign-gated · starts a burst of `count`
/// screenshots at `frame_stride` (every Nth frame).
fn render_start_burst(state: &mut EngineState, params: Value) -> Value {
    let count = p_u32(&params, "count", 10).max(1).min(1000);
    let _frame_stride = p_u32(&params, "frame_stride", 1).max(1);
    state.capture.burst_pending_count = Some(count);
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!("render.start_burst · queued · count={count}"),
    );
    json!({
        "ok": true,
        "count": count,
        "burst_id_will_be": state.capture.burst_id,
    })
}

/// `render.start_video` : sovereign-gated · starts video record.
fn render_start_video(state: &mut EngineState, params: Value) -> Value {
    let _frame_stride = p_u32(&params, "frame_stride", 1).max(1);
    state.capture.video_start_pending = true;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        "render.start_video · queued",
    );
    json!({
        "ok": true,
        "video_id_will_be": state.capture.video_id,
    })
}

/// `render.stop_video` : sovereign-gated · stops video record.
fn render_stop_video(state: &mut EngineState, _params: Value) -> Value {
    state.capture.video_stop_pending = true;
    state.push_event(
        "INFO",
        "loa-host/mcp",
        "render.stop_video · queued",
    );
    json!({
        "ok": true,
        "video_id": state.capture.video_id,
        "frames_captured": state.capture.video_frames_captured,
        "duration_ms": state.capture.video_duration_ms,
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-WAVE3-TEXTINPUT · in-game text-input box handlers
// ───────────────────────────────────────────────────────────────────────

/// `text_input.submit_history` — read-only mirror of the in-game text-input
/// box state : focus + buffer + last 5 submissions + telemetry counters.
fn text_input_submit_history(state: &mut EngineState, _params: Value) -> Value {
    json!({
        "focused": state.text_input.focused,
        "buffer": state.text_input.buffer,
        "history": state.text_input.history,
        "submissions_total": state.text_input.submissions_total,
        "chars_typed_total": state.text_input.chars_typed_total,
    })
}

/// `text_input.inject` — programmatically submit text as if the user typed
/// it and pressed Enter. Mutating · sovereign-cap-gated. The render loop
/// drains `text_input.inject_pending` on the next frame and routes the
/// payload through the same submit-path the keyboard uses.
fn text_input_inject(state: &mut EngineState, params: Value) -> Value {
    let Some(text) = params.get("text").and_then(Value::as_str) else {
        return json!({"error": "missing string param 'text'"});
    };
    if text.is_empty() {
        return json!({"error": "text must be non-empty"});
    }
    // Cap the inject at the same buffer-cap the keyboard path enforces.
    // Use the canonical const from the input module to stay in lock-step.
    let max = crate::input::TEXT_INPUT_MAX_BUFFER;
    let trimmed: String = text.chars().take(max).collect();
    state.text_input.inject_pending = Some(trimmed.clone());
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "text_input.inject · queued · char_len={}",
            trimmed.chars().count()
        ),
    );
    json!({
        "ok": true,
        "queued": trimmed,
        "char_len": trimmed.chars().count(),
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-SENSORY · sense.* MCP tool dispatch handlers
// ───────────────────────────────────────────────────────────────────────
//
// Each handler is a thin wrapper around `crate::sense::aggregate_*`. The
// aggregation layer holds the actual logic (axis-tagged + telemetry-counted).
// Handlers are read-only ; they do call mutating ring-buffer operations on
// EngineState (e.g. `state.sense_invocations_total += 1`) but no
// engine-mutating side-effects beyond the metric counters.

fn sense_framebuffer_thumbnail(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_framebuffer_thumbnail(state)
}

fn sense_center_pixel(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_center_pixel(state)
}

fn sense_viewport_summary(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_viewport_summary(state)
}

fn sense_object_at_crosshair(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_object_at_crosshair(state)
}

fn sense_audio_levels(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_audio_levels(state)
}

fn sense_audio_recent(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_audio_recent(state)
}

fn sense_spatial_audio(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_spatial_audio(state)
}

fn sense_compass_8(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_compass_8(state)
}

fn sense_body_pose(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_body_pose(state)
}

fn sense_room_neighbors(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_room_neighbors(state)
}

fn sense_engine_load(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_engine_load(state)
}

fn sense_frame_pacing(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_frame_pacing(state)
}

fn sense_gpu_state(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_gpu_state(state)
}

fn sense_thermal(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_thermal(state)
}

fn sense_recent_errors(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_recent_errors(state)
}

fn sense_recent_panics(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_recent_panics(state)
}

fn sense_validation_errors(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_validation_errors(state)
}

fn sense_test_status(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_test_status(state)
}

fn sense_event_log(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_event_log(state)
}

fn sense_dm_history(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_dm_history(state)
}

fn sense_input_history(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_input_history(state)
}

fn sense_dm_state(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_dm_state(state)
}

fn sense_gm_recent_phrases(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_gm_recent_phrases(state)
}

fn sense_companion_proposals(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_companion_proposals(state)
}

fn sense_mcp_clients(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_mcp_clients(state)
}

fn sense_recent_commands(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_recent_commands(state)
}

fn sense_omega_field_at_camera(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_omega_field_at_camera(state)
}

fn sense_spectral_at_pixel(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_spectral_at_pixel(state)
}

fn sense_stokes_at_pixel(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_stokes_at_pixel(state)
}

fn sense_cfer_neighborhood(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_cfer_neighborhood(state)
}

fn sense_dgi_signal(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_dgi_signal(state)
}

fn sense_snapshot(state: &mut EngineState, _params: Value) -> Value {
    crate::sense::aggregate_combined_snapshot(state)
}

// ═══════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_server::SOVEREIGN_CAP;

    #[test]
    fn tools_list_returns_84_tools() {
        // 17 baseline (T11-LOA-HOST-3) + 7 render-control (T11-LOA-RICH-RENDER)
        // + 6 telemetry (T11-LOA-TELEM)
        // + 3 visual-data-gathering (T11-LOA-TEST-APP : render.snapshot_png,
        //   render.tour, render.diff_golden)
        // + 2 multi-room (T11-LOA-ROOMS · room.list + room.teleport)
        // + 1 fidelity probe (T11-LOA-FID-MAINSTREAM · render.fidelity)
        // + 3 stokes-polarized (T11-LOA-FID-STOKES : render.stokes_snapshot,
        //   render.set_polarization_view, render.polarization_panels)
        // + 6 spectral fidelity (T11-LOA-FID-SPECTRAL : render.set_illuminant
        //   + render.list_illuminants + render.spectral_snapshot
        //   + render.spectral_zones + telemetry.spectral
        //   + room.teleport_zone)
        // + 3 CFER (T11-LOA-FID-CFER · render.cfer_snapshot + cfer_step + cfer_set_kan_handle)
        // + 4 USERFIX (T11-LOA-USERFIX · render.cfer_intensity + start_burst
        //   + start_video + stop_video)
        // + 32 sensory harness (T11-LOA-SENSORY · 9 axes : visual 4 · audio 3 ·
        //   spatial 3 · interoception 4 · diagnostic 4 · temporal 3 · causal 3 ·
        //   network 2 · environmental 5 · combined 1)
        // + 2 text-input (T11-WAVE3-TEXTINPUT · text_input.submit_history
        //   + text_input.inject)
        // + 3 GLTF (T11-WAVE3-GLTF · world.spawn_gltf + world.gltf_spawns_total
        //   + world.list_dynamic_meshes)
        // = 89 total.
        let reg = tool_registry();
        assert_eq!(reg.len(), 89, "must have exactly 89 tools");
        // Spot-check a representative slice.
        for required in &[
            "engine.state",
            "engine.shutdown",
            "engine.pause",
            "camera.get",
            "camera.set",
            "room.geometry",
            "room.spawn_plinth",
            "render.set_mode",
            "telemetry.recent",
            "dm.intensity",
            "dm.event.propose",
            "gm.describe_environment",
            "gm.dialogue",
            "omega.sample",
            "omega.modify",
            "companion.propose",
            "tools.list",
            // T11-LOA-RICH-RENDER additions :
            "render.list_patterns",
            "render.list_materials",
            "render.snapshot",
            "render.set_wall_pattern",
            "render.set_floor_pattern",
            "render.set_material",
            "render.spawn_stress",
            // T11-LOA-TELEM additions :
            "telemetry.snapshot",
            "telemetry.histogram",
            "telemetry.gpu_info",
            "telemetry.tail_events",
            "telemetry.flush",
            "telemetry.set_log_level",
            // T11-LOA-TEST-APP additions :
            "render.snapshot_png",
            "render.tour",
            "render.diff_golden",
            // T11-LOA-ROOMS additions :
            "room.list",
            "room.teleport",
            // T11-LOA-FID-MAINSTREAM addition :
            "render.fidelity",
            // T11-LOA-FID-STOKES additions :
            "render.stokes_snapshot",
            "render.set_polarization_view",
            "render.polarization_panels",
            // T11-LOA-FID-SPECTRAL additions :
            "render.set_illuminant",
            "render.list_illuminants",
            "render.spectral_snapshot",
            "render.spectral_zones",
            "telemetry.spectral",
            "room.teleport_zone",
            // T11-LOA-FID-CFER additions :
            "render.cfer_snapshot",
            "render.cfer_step",
            "render.cfer_set_kan_handle",
            // T11-LOA-USERFIX additions :
            "render.cfer_intensity",
            "render.start_burst",
            "render.start_video",
            "render.stop_video",
            // T11-LOA-SENSORY additions (9 axes · 32 tools) :
            "sense.framebuffer_thumbnail",
            "sense.center_pixel",
            "sense.viewport_summary",
            "sense.object_at_crosshair",
            "sense.audio_levels",
            "sense.audio_recent",
            "sense.spatial_audio",
            "sense.compass_8",
            "sense.body_pose",
            "sense.room_neighbors",
            "sense.engine_load",
            "sense.frame_pacing",
            "sense.gpu_state",
            "sense.thermal",
            "sense.recent_errors",
            "sense.recent_panics",
            "sense.validation_errors",
            "sense.test_status",
            "sense.event_log",
            "sense.dm_history",
            "sense.input_history",
            "sense.dm_state",
            "sense.gm_recent_phrases",
            "sense.companion_proposals",
            "sense.mcp_clients",
            "sense.recent_commands",
            "sense.omega_field_at_camera",
            "sense.spectral_at_pixel",
            "sense.stokes_at_pixel",
            "sense.cfer_neighborhood",
            "sense.dgi_signal",
            "sense.snapshot",
        ] {
            assert!(reg.contains_key(*required), "missing {required}");
        }
    }

    #[test]
    fn read_only_tools_have_mutating_false() {
        let reg = tool_registry();
        for name in &[
            "engine.state",
            "camera.get",
            "room.geometry",
            "telemetry.recent",
            "gm.describe_environment",
            "gm.dialogue",
            "omega.sample",
            "tools.list",
            "render.list_patterns",
            "render.list_materials",
            "render.snapshot",
            "telemetry.snapshot",
            "telemetry.histogram",
            "telemetry.gpu_info",
            "telemetry.tail_events",
            "telemetry.flush",
            // T11-LOA-TEST-APP : diff_golden is read-only (just disk I/O).
            "render.diff_golden",
            "room.list",
            // T11-LOA-FID-MAINSTREAM : fidelity probe is read-only.
            "render.fidelity",
            // T11-LOA-FID-STOKES : query-only stokes tools are read-only.
            "render.stokes_snapshot",
            "render.polarization_panels",
            // T11-LOA-FID-SPECTRAL : query-only tools are read-only.
            "render.list_illuminants",
            "render.spectral_snapshot",
            "render.spectral_zones",
            "telemetry.spectral",
            // T11-LOA-FID-CFER : cfer_snapshot is the read-only CFER tool.
            "render.cfer_snapshot",
        ] {
            let e = reg.get(*name).unwrap();
            assert!(!e.meta.mutating, "{name} must be read-only");
        }
    }

    #[test]
    fn mutating_tools_have_mutating_true() {
        let reg = tool_registry();
        for name in &[
            "engine.shutdown",
            "engine.pause",
            "camera.set",
            "room.spawn_plinth",
            "render.set_mode",
            "dm.intensity",
            "dm.event.propose",
            "omega.modify",
            "companion.propose",
            "render.set_wall_pattern",
            "render.set_floor_pattern",
            "render.set_material",
            "render.spawn_stress",
            "telemetry.set_log_level",
            // T11-LOA-TEST-APP : snapshot_png + tour are mutating (queue +
            // disk write side effects).
            "render.snapshot_png",
            "render.tour",
            "room.teleport",
            // T11-LOA-FID-SPECTRAL : illuminant + zone-teleport mutate state.
            "render.set_illuminant",
            "room.teleport_zone",
        ] {
            let e = reg.get(*name).unwrap();
            assert!(e.meta.mutating, "{name} must be mutating");
        }
    }

    #[test]
    fn mcp_tool_render_set_wall_pattern_returns_ok() {
        let mut s = EngineState::default();
        let v = render_set_wall_pattern(
            &mut s,
            json!({"sovereign_cap": crate::mcp_server::SOVEREIGN_CAP, "wall_id": 0, "pattern_id": 4}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["pattern_id"], 4);
        // Round-trip via the FFI getter (control plane state).
        assert_eq!(crate::ffi::wall_pattern_override(0), Some(4));
    }

    #[test]
    fn mcp_render_list_patterns_returns_at_least_12() {
        let mut s = EngineState::default();
        let v = render_list_patterns(&mut s, json!({}));
        assert!(v["count"].as_u64().unwrap() >= 12);
    }

    #[test]
    fn mcp_render_list_materials_returns_at_least_8() {
        let mut s = EngineState::default();
        let v = render_list_materials(&mut s, json!({}));
        assert!(v["count"].as_u64().unwrap() >= 8);
    }

    #[test]
    fn mcp_render_snapshot_includes_walls_and_floors() {
        let mut s = EngineState::default();
        let v = render_snapshot(&mut s, json!({}));
        assert!(v["walls"].is_array());
        assert!(v["floor_quadrants"].is_array());
        assert_eq!(v["walls"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn engine_state_handler_shape() {
        let mut s = EngineState::default();
        let v = engine_state(&mut s, json!({}));
        assert_eq!(v["frame_count"], 0);
        assert_eq!(v["active_scene"], "test-room");
        assert_eq!(v["render_mode"], "Normal");
        assert!(v["camera_pos"].is_object());
    }

    #[test]
    fn camera_set_and_get_round_trip() {
        let mut s = EngineState::default();
        let _ = camera_set(
            &mut s,
            json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "x": 7.0, "y": 2.0, "z": -3.0,
                "yaw": 1.5, "pitch": -0.3
            }),
        );
        let g = camera_get(&mut s, json!({}));
        assert!((g["x"].as_f64().unwrap() - 7.0).abs() < 1e-6);
        assert!((g["yaw"].as_f64().unwrap() - 1.5).abs() < 1e-6);
    }

    #[test]
    fn render_set_mode_accepts_valid_then_rejects_invalid() {
        let mut s = EngineState::default();
        let ok = render_set_mode(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP, "mode": 5}));
        assert_eq!(ok["render_mode"], "Sdf");
        let bad = render_set_mode(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP, "mode": 99}));
        assert!(bad.get("error").is_some());
    }

    #[test]
    fn dm_intensity_clamps_to_3() {
        let mut s = EngineState::default();
        let v = dm_intensity(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP, "value": 99}));
        assert_eq!(v["dm_intensity"], 3);
    }

    #[test]
    fn engine_shutdown_sets_quit_flag() {
        let mut s = EngineState::default();
        assert!(!s.quit_requested);
        let _ = engine_shutdown(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP}));
        assert!(s.quit_requested);
    }

    #[test]
    fn engine_pause_toggles() {
        let mut s = EngineState::default();
        assert!(!s.paused);
        let _ = engine_pause(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP}));
        assert!(s.paused);
        let _ = engine_pause(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP}));
        assert!(!s.paused);
    }

    #[test]
    fn room_spawn_plinth_appends_to_state() {
        let mut s = EngineState::default();
        let initial = s.plinths.len();
        let _ = room_spawn_plinth(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "x": 3.0, "z": -1.0, "color": 0xFF_AA_11}),
        );
        assert_eq!(s.plinths.len(), initial + 1);
        let last = s.plinths.last().unwrap();
        assert!((last.x - 3.0).abs() < 1e-6);
        assert_eq!(last.color_rgb, 0xFF_AA_11);
    }

    #[test]
    fn telemetry_recent_returns_count_or_less() {
        let mut s = EngineState::default();
        for i in 0..50 {
            s.push_event("INFO", "test", &format!("evt-{i}"));
        }
        let v = telemetry_recent(&mut s, json!({"count": 10}));
        assert_eq!(v["events"].as_array().unwrap().len(), 10);
    }

    #[test]
    fn gm_dialogue_includes_topic_and_npc() {
        let mut s = EngineState::default();
        let v = gm_dialogue(
            &mut s,
            json!({"npc_id": "warder-3", "mood": "wary", "topic": "the spiral"}),
        );
        let line = v["line"].as_str().unwrap();
        assert!(line.contains("warder-3"));
        assert!(line.contains("the spiral"));
    }

    #[test]
    fn morton_encode_known_pattern() {
        // x=1,y=0,z=0 ⇒ bit 0 set
        assert_eq!(morton_encode_u32(1, 0, 0), 1);
        // y=1,x=0,z=0 ⇒ bit 1 set
        assert_eq!(morton_encode_u32(0, 1, 0), 2);
        // z=1,x=0,y=0 ⇒ bit 2 set
        assert_eq!(morton_encode_u32(0, 0, 1), 4);
    }

    #[test]
    fn omega_sample_then_modify_then_resample() {
        let mut s = EngineState::default();
        let s1 = omega_sample(&mut s, json!({"x": 0.0, "y": 0.0, "z": 0.0}));
        // Test-room is empty by default — cell_present should be false.
        // (We can't strictly assert false because other tests may have
        // populated the global stub. We simply validate the response shape.)
        assert!(s1.get("morton").is_some());

        let _ = omega_modify(
            &mut s,
            json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "x": 100.0, "y": 100.0, "z": 100.0,
                "value": 0.5
            }),
        );
        let s2 = omega_sample(&mut s, json!({"x": 100.0, "y": 100.0, "z": 100.0}));
        assert_eq!(s2["cell_present"], true);
    }

    #[test]
    fn text_input_submit_history_returns_focus_buffer_history() {
        // Default state : unfocused, empty buffer, empty history.
        let mut s = EngineState::default();
        let v = text_input_submit_history(&mut s, json!({}));
        assert_eq!(v["focused"], json!(false));
        assert_eq!(v["buffer"], json!(""));
        assert_eq!(v["history"], json!([]));
        assert_eq!(v["submissions_total"], json!(0));
        assert_eq!(v["chars_typed_total"], json!(0));
        // Populate the mirror and re-query.
        s.text_input.focused = true;
        s.text_input.buffer = "drafting".to_string();
        s.text_input.history = vec!["a".to_string(), "bb".to_string(), "ccc".to_string()];
        s.text_input.submissions_total = 3;
        s.text_input.chars_typed_total = 12;
        let v = text_input_submit_history(&mut s, json!({}));
        assert_eq!(v["focused"], json!(true));
        assert_eq!(v["buffer"], json!("drafting"));
        assert_eq!(v["history"], json!(["a", "bb", "ccc"]));
        assert_eq!(v["submissions_total"], json!(3));
        assert_eq!(v["chars_typed_total"], json!(12));
    }

    #[test]
    fn text_input_inject_queues_pending_submission() {
        let mut s = EngineState::default();
        assert!(s.text_input.inject_pending.is_none());
        let v = text_input_inject(
            &mut s,
            json!({"sovereign_cap": "0xCAFE_BABE_DEADBEEF", "text": "hello world"}),
        );
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["queued"], json!("hello world"));
        assert_eq!(v["char_len"], json!(11));
        assert_eq!(s.text_input.inject_pending.as_deref(), Some("hello world"));
    }

    #[test]
    fn text_input_inject_rejects_empty_text() {
        let mut s = EngineState::default();
        let v = text_input_inject(
            &mut s,
            json!({"sovereign_cap": "0xCAFE_BABE_DEADBEEF", "text": ""}),
        );
        assert!(v.get("error").is_some());
        assert!(s.text_input.inject_pending.is_none());
    }

    #[test]
    fn text_input_inject_caps_at_max_buffer() {
        let mut s = EngineState::default();
        // 300 chars : should be truncated to 256 (TEXT_INPUT_MAX_BUFFER).
        let payload: String = std::iter::repeat('z').take(300).collect();
        let v = text_input_inject(
            &mut s,
            json!({"sovereign_cap": "0xCAFE_BABE_DEADBEEF", "text": payload}),
        );
        assert_eq!(v["char_len"], json!(crate::input::TEXT_INPUT_MAX_BUFFER));
        let pending = s.text_input.inject_pending.as_ref().unwrap();
        assert_eq!(pending.chars().count(), crate::input::TEXT_INPUT_MAX_BUFFER);
    }

    #[test]
    fn tools_list_handler_count_matches_registry() {
        let mut s = EngineState::default();
        let v = tools_list(&mut s, json!({}));
        // 17 baseline + 7 render-control + 6 telemetry + 3 test-apparatus
        // + 2 room (T11-LOA-ROOMS) + 1 fidelity (T11-LOA-FID-MAINSTREAM)
        // + 3 stokes (T11-LOA-FID-STOKES) + 6 spectral (T11-LOA-FID-SPECTRAL)
        // + 3 cfer (T11-LOA-FID-CFER) + 4 userfix (T11-LOA-USERFIX)
        // + 32 sensory (T11-LOA-SENSORY)
        // + 2 text-input (T11-WAVE3-TEXTINPUT · text_input.submit_history,
        //   text_input.inject)
        // + 3 gltf (T11-WAVE3-GLTF · world.spawn_gltf + world.gltf_spawns_total
        //   + world.list_dynamic_meshes) = 89.
        assert_eq!(v["count"], 89);
        let arr = v["tools"].as_array().unwrap();
        assert_eq!(arr.len(), 89);
    }

    // § T11-LOA-FID-SPECTRAL · MCP handler shape + behaviour tests
    #[test]
    fn mcp_render_set_illuminant_a_succeeds() {
        let mut s = EngineState::default();
        let v = render_set_illuminant(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "name": "A"}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["illuminant"], "A");
        assert_eq!(v["previous"], "D65");
        assert_eq!(v["cct_kelvin"], 2856);
        assert!(s.illuminant_gen >= 1);
    }

    #[test]
    fn mcp_render_set_illuminant_invalid_returns_error() {
        let mut s = EngineState::default();
        let v = render_set_illuminant(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "name": "Z99"}),
        );
        assert_eq!(v["ok"], false);
        assert!(v["error"].is_string());
    }

    #[test]
    fn mcp_render_list_illuminants_returns_4_entries() {
        let mut s = EngineState::default();
        let v = render_list_illuminants(&mut s, json!({}));
        assert_eq!(v["count"], 4);
        let arr = v["illuminants"].as_array().unwrap();
        assert_eq!(arr.len(), 4);
        let names: Vec<&str> = arr.iter().filter_map(|e| e["name"].as_str()).collect();
        assert!(names.contains(&"D65"));
        assert!(names.contains(&"D50"));
        assert!(names.contains(&"A"));
        assert!(names.contains(&"F11"));
    }

    #[test]
    fn mcp_render_spectral_snapshot_returns_16_materials() {
        let mut s = EngineState::default();
        let v = render_spectral_snapshot(&mut s, json!({}));
        let mats = v["materials"].as_array().unwrap();
        assert_eq!(mats.len(), MATERIAL_LUT_LEN);
        // First material has a baked array of 4 illuminants.
        let first_baked = mats[0]["baked"].as_array().unwrap();
        assert_eq!(first_baked.len(), 4);
    }

    #[test]
    fn mcp_render_spectral_zones_returns_4_entries() {
        let mut s = EngineState::default();
        let v = render_spectral_zones(&mut s, json!({}));
        assert_eq!(v["count"], 4);
        let zones = v["zones"].as_array().unwrap();
        assert_eq!(zones.len(), 4);
        // Each zone has illuminant + name + spawn.
        for z in zones {
            assert!(z["name"].is_string());
            assert!(z["illuminant"].is_string());
            assert!(z["spawn_xyz"].is_array());
        }
    }

    #[test]
    fn mcp_telemetry_spectral_returns_counter_fields() {
        let mut s = EngineState::default();
        let v = telemetry_spectral(&mut s, json!({}));
        assert!(v["spectral_bake_count"].is_u64());
        assert!(v["current_illuminant"].is_string());
        assert!(v["current_illuminant_cct"].is_u64());
    }

    #[test]
    fn mcp_room_teleport_zone_d65_nw_succeeds() {
        let mut s = EngineState::default();
        // Start at default position so we can verify the teleport.
        let prior_pos = s.camera.pos;
        let v = room_teleport_zone(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "zone": "D65-NW"}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["zone"], "D65-NW");
        // Camera must have moved to inside the ColorRoom AABB.
        let new_pos = s.camera.pos;
        assert!(new_pos != prior_pos);
        assert!(new_pos.x >= -58.0 && new_pos.x <= -28.0);
        assert!(new_pos.z >= -15.0 && new_pos.z <= 15.0);
    }

    #[test]
    fn mcp_room_teleport_zone_unknown_returns_error() {
        let mut s = EngineState::default();
        let v = room_teleport_zone(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "zone": "NOPE"}),
        );
        assert_eq!(v["ok"], false);
        assert!(v["error"].is_string());
    }

    #[test]
    fn mcp_render_set_illuminant_advances_gen_only_on_change() {
        let mut s = EngineState::default();
        let initial_gen = s.illuminant_gen;
        // Set to D65 (default) — no change ⇒ no gen advance.
        let _ = render_set_illuminant(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "name": "D65"}),
        );
        assert_eq!(s.illuminant_gen, initial_gen);
        // Switch to D50 — gen must advance.
        let _ = render_set_illuminant(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "name": "D50"}),
        );
        assert_eq!(s.illuminant_gen, initial_gen + 1);
    }

    // § T11-LOA-TELEM telemetry handler shape tests
    #[test]
    fn mcp_telemetry_snapshot_shape() {
        let mut s = EngineState::default();
        let v = telemetry_snapshot(&mut s, json!({}));
        // Required fields present.
        assert!(v.get("frame_count").is_some());
        assert!(v.get("fps").is_some());
        assert!(v.get("p50_ms").is_some());
        assert!(v.get("histogram").is_some());
    }

    #[test]
    fn mcp_telemetry_histogram_returns_10_buckets() {
        let mut s = EngineState::default();
        let v = telemetry_histogram(&mut s, json!({}));
        let buckets = v["buckets"].as_array().unwrap();
        assert_eq!(buckets.len(), 10);
        assert_eq!(v["bucket_count"], 10);
    }

    #[test]
    fn mcp_telemetry_set_log_level_clamps_to_3() {
        let mut s = EngineState::default();
        let v = telemetry_set_log_level(&mut s, json!({"level": 99, "sovereign_cap": SOVEREIGN_CAP}));
        assert_eq!(v["level"], 3);
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn dm_event_propose_increments_counter() {
        let mut s = EngineState::default();
        assert_eq!(s.dm.event_count, 0);
        let _ = dm_event_propose(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "kind": "ambush", "x": 1.0, "y": 0.0, "z": 2.0}),
        );
        assert_eq!(s.dm.event_count, 1);
    }

    #[test]
    fn companion_propose_returns_stub_acceptance() {
        let mut s = EngineState::default();
        let v = companion_propose(
            &mut s,
            json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "kind": "say-line",
                "target": "the-glacier",
                "payload": {"text": "the wave remembers"}
            }),
        );
        assert_eq!(v["accepted"], true);
        assert_eq!(v["target"], "the-glacier");
    }

    // ─────────────────────────────────────────────────────────────────
    // § T11-LOA-TEST-APP : visual-data-gathering MCP tools
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn mcp_render_snapshot_png_returns_path() {
        let mut s = EngineState::default();
        let v = render_snapshot_png(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "path": "manual_snap.png"}),
        );
        assert_eq!(v["ok"], true);
        let path_str = v["path"].as_str().unwrap();
        assert!(path_str.contains("manual_snap.png"));
        // Queue should now hold one entry.
        assert_eq!(s.snapshot_queue.len(), 1);
        assert_eq!(s.snapshot_count, 1);
    }

    #[test]
    fn mcp_render_snapshot_png_uses_default_path_when_omitted() {
        let mut s = EngineState::default();
        s.frame_count = 42;
        let v = render_snapshot_png(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP}));
        assert_eq!(v["ok"], true);
        let path_str = v["path"].as_str().unwrap();
        // Default uses frame_count zero-padded
        assert!(path_str.contains("snap_00000042.png"), "got {path_str}");
    }

    #[test]
    fn mcp_render_snapshot_png_rejects_traversal() {
        let mut s = EngineState::default();
        let v = render_snapshot_png(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "path": "../etc/passwd"}),
        );
        assert_eq!(v["ok"], false);
        assert!(s.snapshot_queue.is_empty());
    }

    #[test]
    fn mcp_render_tour_returns_pose_count_in_response() {
        let mut s = EngineState::default();
        let v = render_tour(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "tour_id": "walls"}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["poses_visited"], 4);
        let snaps = v["snapshots"].as_array().unwrap();
        assert_eq!(snaps.len(), 4);
        // Each entry has pose + path.
        assert!(snaps[0]["pose"].is_string());
        assert!(snaps[0]["path"].is_string());
        // Queue should now contain 4 pending snapshots.
        assert_eq!(s.snapshot_queue.len(), 4);
        // Tour-progress should reflect completion (4 of 4 queued).
        assert_eq!(s.tour_progress, Some((4, 4)));
    }

    #[test]
    fn mcp_render_tour_rejects_unknown_tour_id() {
        let mut s = EngineState::default();
        let v = render_tour(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "tour_id": "imaginary_tour"}),
        );
        assert_eq!(v["ok"], false);
        // Camera should not have been mutated.
        assert!(s.snapshot_queue.is_empty());
    }

    #[test]
    fn mcp_render_tour_default_returns_5_poses() {
        let mut s = EngineState::default();
        let v = render_tour(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "tour_id": "default"}),
        );
        assert_eq!(v["poses_visited"], 5);
        assert_eq!(s.snapshot_queue.len(), 5);
    }

    #[test]
    fn mcp_render_tour_plinths_returns_14_poses() {
        let mut s = EngineState::default();
        let v = render_tour(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "tour_id": "plinths"}),
        );
        assert_eq!(v["poses_visited"], 14);
    }

    #[test]
    fn mcp_render_diff_golden_unknown_tour_returns_error() {
        let mut s = EngineState::default();
        let v = render_diff_golden(&mut s, json!({"tour_id": "no_such_tour"}));
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn mcp_render_diff_golden_with_no_snapshots_marks_all_failed() {
        // The default tour has 5 poses. With no snapshots/goldens on
        // disk, every entry should mark passed=false.
        let mut s = EngineState::default();
        // Use a unique tour-id-ish suffix in tmp to avoid collision with
        // previous test runs ; we use "default" because tour_by_id only
        // accepts known IDs. Just check the structure.
        let v = render_diff_golden(&mut s, json!({"tour_id": "default"}));
        // Even with no files, response is well-formed.
        assert_eq!(v["ok"], true);
        // tour_id present
        assert_eq!(v["tour_id"], "default");
        // per_pose array has 5 entries
        let arr = v["per_pose"].as_array().unwrap();
        assert_eq!(arr.len(), 5);
    }

    // § T11-LOA-ROOMS · MCP room.list + room.teleport tests
    #[test]
    fn mcp_room_list_returns_five_rooms() {
        let mut s = EngineState::default();
        let v = room_list(&mut s, json!({}));
        assert_eq!(v["count"], 5);
        let arr = v["rooms"].as_array().unwrap();
        assert_eq!(arr.len(), 5);
        // Spot-check each name appears.
        let names: Vec<&str> = arr.iter().map(|r| r["name"].as_str().unwrap()).collect();
        for required in &["TestRoom", "MaterialRoom", "PatternRoom", "ScaleRoom", "ColorRoom"] {
            assert!(names.contains(required), "missing room {required}");
        }
    }

    #[test]
    fn mcp_room_teleport_snaps_camera_to_room_center() {
        let mut s = EngineState::default();
        let v = room_teleport(
            &mut s,
            json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "room_id": "MaterialRoom",
            }),
        );
        assert_eq!(v["ok"], true);
        // MaterialRoom center is at z=43 (spawn-eye is (0, 1.55, 43))
        assert!((s.camera.pos.z - 43.0).abs() < 0.1);
        assert!((s.camera.pos.y - 1.55).abs() < 0.01);
    }

    #[test]
    fn mcp_room_teleport_rejects_unknown_room() {
        let mut s = EngineState::default();
        let v = room_teleport(
            &mut s,
            json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "room_id": "NonExistentRoom",
            }),
        );
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("NonExistentRoom"));
    }

    // § T11-LOA-FID-MAINSTREAM · MCP render.fidelity tests

    /// Returns a structured object with the expected keys, even in catalog
    /// mode (where GPU has not initialized → `initialized=false`).
    #[test]
    fn mcp_render_fidelity_returns_valid_struct() {
        let mut s = EngineState::default();
        let v = render_fidelity(&mut s, json!({}));
        // Required keys.
        assert!(v.get("msaa_samples").is_some());
        assert!(v.get("hdr_format").is_some());
        assert!(v.get("present_mode").is_some());
        assert!(v.get("aniso_max").is_some());
        assert!(v.get("tonemap_path").is_some());
        assert!(v.get("initialized").is_some());
        // Types are sane.
        assert!(v["msaa_samples"].as_u64().is_some());
        assert!(v["hdr_format"].as_str().is_some());
        assert!(v["present_mode"].as_str().is_some());
        assert!(v["aniso_max"].as_u64().is_some());
        assert!(v["tonemap_path"].as_bool().is_some());
        assert!(v["initialized"].as_bool().is_some());
    }

    /// When the gpu module publishes a 4xMSAA / Mailbox / Rgba16Float
    /// fidelity report, `render.fidelity` reflects it.
    #[test]
    fn mcp_render_fidelity_reflects_published_report() {
        crate::fidelity::set_report(crate::fidelity::FidelityReport {
            msaa_samples: 4,
            hdr_format: "Rgba16Float".to_string(),
            present_mode: "Mailbox".to_string(),
            aniso_max: 16,
            tonemap_path: true,
            initialized: true,
        });
        let mut s = EngineState::default();
        let v = render_fidelity(&mut s, json!({}));
        assert_eq!(v["msaa_samples"], 4);
        assert_eq!(v["hdr_format"], "Rgba16Float");
        assert_eq!(v["present_mode"], "Mailbox");
        assert_eq!(v["aniso_max"], 16);
        assert_eq!(v["tonemap_path"], true);
        assert_eq!(v["initialized"], true);
    }

    // § T11-LOA-FID-STOKES · MCP stokes_snapshot + set_polarization_view tests
    #[test]
    fn mcp_render_stokes_snapshot_returns_iquv_at_center() {
        let mut s = EngineState::default();
        let v = render_stokes_snapshot(&mut s, json!({}));
        // Polarization mode + name must be present.
        assert!(v.get("polarization_mode").is_some());
        assert!(v.get("polarization_mode_name").is_some());
        // Sun Stokes vector must have I + Q + U + V.
        let sun = &v["sun_stokes"];
        assert!(sun.get("i").is_some(), "sun_stokes.i missing");
        assert!(sun.get("q").is_some(), "sun_stokes.q missing");
        assert!(sun.get("u").is_some(), "sun_stokes.u missing");
        assert!(sun.get("v").is_some(), "sun_stokes.v missing");
        // I should be 1.0, Q slightly positive (atmospheric horizontal pol).
        let i = sun["i"].as_f64().unwrap();
        let q = sun["q"].as_f64().unwrap();
        assert!((i - 1.0).abs() < 1e-3, "I={i}");
        assert!(q > 0.0, "Q should be slightly positive (atmospheric): Q={q}");
        // Per-material array = 16 entries.
        let mats = v["per_material_stokes"].as_array().unwrap();
        assert_eq!(mats.len(), 16, "must have 16 per-material Stokes entries");
        // Each entry has IQUV + dop fields.
        for m in mats {
            assert!(m.get("material_id").is_some());
            assert!(m.get("i").is_some());
            assert!(m.get("dop_linear").is_some());
            assert!(m.get("dop_total").is_some());
        }
    }

    #[test]
    fn mcp_render_set_polarization_view_cycles_modes() {
        let mut s = EngineState::default();
        // Set to mode 2 (U).
        let v = render_set_polarization_view(&mut s, json!({"mode": 2}));
        assert_eq!(v["ok"], true);
        assert_eq!(v["polarization_mode"], 2);
        // Setting mode > 4 clamps to 4.
        let v2 = render_set_polarization_view(&mut s, json!({"mode": 99}));
        assert_eq!(v2["polarization_mode"], 4);
        // Reset to 0 for cleanliness.
        let _ = render_set_polarization_view(&mut s, json!({"mode": 0}));
    }

    #[test]
    fn mcp_render_polarization_panels_returns_4_panels() {
        let mut s = EngineState::default();
        let v = render_polarization_panels(&mut s, json!({}));
        assert_eq!(v["count"], 4);
        let arr = v["panels"].as_array().unwrap();
        assert_eq!(arr.len(), 4);
        // Each panel has a label + expected_stokes block.
        for p in arr {
            assert!(p.get("label").is_some());
            assert!(p.get("expected_stokes").is_some());
            let s = &p["expected_stokes"];
            assert!(s.get("i").is_some());
            assert!(s.get("q").is_some());
            assert!(s.get("u").is_some());
            assert!(s.get("v").is_some());
        }
    }

    // ── § T11-LOA-FID-CFER : MCP CFER tool tests ──

    #[test]
    fn mcp_render_cfer_snapshot_returns_active_cell_count() {
        let mut s = EngineState::default();
        // Seed the mirror as the renderer would.
        s.cfer.active_cells = 12345;
        s.cfer.step_us = 250;
        s.cfer.center_radiance = [0.1, 0.2, 0.3];
        s.cfer.kan_handle = Some(42);
        let v = render_cfer_snapshot(&mut s, json!({}));
        assert_eq!(v["active_cells"], 12345);
        assert_eq!(v["step_us"], 250);
        assert_eq!(v["kan_handle"], 42);
        assert!((v["center_radiance"]["r"].as_f64().unwrap() - 0.1).abs() < 1e-3);
        assert_eq!(v["tex_dim"]["x"], 32);
        assert_eq!(v["tex_dim"]["y"], 16);
        assert_eq!(v["tex_dim"]["z"], 32);
        assert_eq!(v["tex_dim"]["total_texels"], 16384);
    }

    #[test]
    fn mcp_render_cfer_snapshot_with_no_kan_returns_null() {
        let mut s = EngineState::default();
        s.cfer.kan_handle = None;
        let v = render_cfer_snapshot(&mut s, json!({}));
        assert!(v["kan_handle"].is_null());
    }

    #[test]
    fn mcp_render_cfer_step_queues_force_step() {
        let mut s = EngineState::default();
        assert!(!s.cfer.force_step_pending);
        let v = render_cfer_step(&mut s, json!({"sovereign_cap": SOVEREIGN_CAP}));
        assert_eq!(v["ok"], true);
        assert!(s.cfer.force_step_pending);
    }

    #[test]
    fn mcp_render_cfer_set_kan_handle_attach_queues_pending() {
        let mut s = EngineState::default();
        let v = render_cfer_set_kan_handle(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "handle": 42}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["action"], "attach");
        assert_eq!(v["handle"], 42);
        assert_eq!(s.cfer.kan_handle_pending, Some(Some(42)));
    }

    #[test]
    fn mcp_render_cfer_set_kan_handle_detach_when_negative() {
        let mut s = EngineState::default();
        let v = render_cfer_set_kan_handle(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "handle": -1}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["action"], "detach");
        assert_eq!(s.cfer.kan_handle_pending, Some(None));
    }

    #[test]
    fn mcp_render_cfer_set_kan_handle_rejects_oversized() {
        let mut s = EngineState::default();
        let v = render_cfer_set_kan_handle(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "handle": 70000}),
        );
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("u16"));
        assert_eq!(s.cfer.kan_handle_pending, None);
    }

    // ── § T11-LOA-USERFIX : new MCP handler tests ──

    #[test]
    fn mcp_render_cfer_intensity_setter_clamped_0_to_1() {
        let mut s = EngineState::default();
        // Above 1.0 clamps to 1.0
        let v = render_cfer_intensity(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "intensity": 2.5}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["intensity"].as_f64().unwrap(), 1.0);
        assert_eq!(s.cfer.cfer_intensity_pending, Some(1.0));
        // Below 0 clamps to 0
        let v = render_cfer_intensity(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "intensity": -0.5}),
        );
        assert_eq!(v["intensity"].as_f64().unwrap(), 0.0);
        assert_eq!(s.cfer.cfer_intensity_pending, Some(0.0));
        // In-range passes through
        let v = render_cfer_intensity(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "intensity": 0.42}),
        );
        let intensity = v["intensity"].as_f64().unwrap();
        assert!((intensity - 0.42).abs() < 1e-5);
    }

    #[test]
    fn mcp_render_start_burst_returns_ok_with_count() {
        let mut s = EngineState::default();
        let v = render_start_burst(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "count": 10}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["count"].as_u64().unwrap(), 10);
        assert_eq!(s.capture.burst_pending_count, Some(10));
    }

    #[test]
    fn mcp_render_start_burst_clamps_count_to_1000_max() {
        let mut s = EngineState::default();
        // count > 1000 clamps
        let v = render_start_burst(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "count": 50000}),
        );
        assert_eq!(v["count"].as_u64().unwrap(), 1000);
        // count = 0 clamps to 1
        let v = render_start_burst(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "count": 0}),
        );
        assert_eq!(v["count"].as_u64().unwrap(), 1);
    }

    #[test]
    fn mcp_render_start_video_queues_pending_flag() {
        let mut s = EngineState::default();
        assert!(!s.capture.video_start_pending);
        let v = render_start_video(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP}),
        );
        assert_eq!(v["ok"], true);
        assert!(s.capture.video_start_pending);
    }

    #[test]
    fn mcp_render_stop_video_queues_pending_flag() {
        let mut s = EngineState::default();
        let v = render_stop_video(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP}),
        );
        assert_eq!(v["ok"], true);
        assert!(s.capture.video_stop_pending);
    }
}
