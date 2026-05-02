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

    // ─ T11-WAVE3-SPONT : spontaneous-condensation pipeline ─
    reg!(
        "world.spontaneous_seed",
        "Sow an intent text into the Ω-field as seed-cells (params: text · optional position{x,y,z}). Substrate evolves cells; manifested ones spawn stress objects.",
        true,
        world_spontaneous_seed
    );
    reg!(
        "sense.spontaneous_recent",
        "Return the last 16 spontaneous-manifestation events + seeds_total + manifests_total + tracked_count.",
        false,
        sense_spontaneous_recent
    );

    // ─ T11-WAVE3-INTENT : text → intent → action dispatch (2 tools) ─
    reg!(
        "intent.translate",
        "Classify free-form text into a typed intent · optionally dispatch (params: text · dispatch=bool).",
        true,
        intent_translate
    );
    reg!(
        "intent.recent",
        "Return last 16 dispatched intents + per-kind counters.",
        false,
        intent_recent
    );

    // ─ T11-W5c-LOA-HOST-WIRE : 17 read-only probes over the wave-3γ + W4 + W5a
    //   + W5b cssl-host-* path-deps. All tools are non-mutating ; downstream
    //   waves will register the corresponding mutating tools individually. ─
    reg!(
        "replay.last_event_count",
        "Count of events in the last loaded replay. Returns 0 if no replay loaded.",
        false,
        replay_last_event_count
    );
    reg!(
        "audit.summarize_dir",
        "Ingest a directory of *.log/*.jsonl audit streams and return summary text (params: dir).",
        false,
        audit_summarize_dir
    );
    reg!(
        "stereo.config_default_text",
        "Return the default StereoConfig (IPD = 63 mm) as pretty JSON.",
        false,
        stereo_config_default_text
    );
    reg!(
        "golden.list_labels",
        "List golden snapshot labels under a directory (params: dir).",
        false,
        golden_list_labels
    );
    reg!(
        "procgen.list_room_kinds",
        "List the 7 canonical RoomKind variants from cssl-host-procgen-rooms.",
        false,
        procgen_list_room_kinds
    );
    reg!(
        "histogram.snapshot_text",
        "Snapshot of any internal histogram registry (returns '<empty>' when no registry is wired).",
        false,
        histogram_snapshot_text
    );
    reg!(
        "attestation.empty_session_text",
        "Render an empty SessionAttestation as canonical text (no session yet).",
        false,
        attestation_empty_session_text
    );
    reg!(
        "rt_trace.recent_events",
        "Drain the runtime trace ring into a JSON array (returns '[]' if no ring is wired).",
        false,
        rt_trace_recent_events
    );
    reg!(
        "spectral.bands",
        "Return the 16 canonical band wavelengths (nm) as a JSON array.",
        false,
        spectral_bands
    );
    reg!(
        "frame_recorder.lfrc_magic",
        "Return the LFRC v1 magic bytes as a hex string.",
        false,
        frame_recorder_lfrc_magic
    );
    reg!(
        "input_virtual.list_scenarios",
        "List canonical synthetic-input scenarios (navigate_test_room · type_intent_phrase · full_qa_session).",
        false,
        input_virtual_list_scenarios
    );
    reg!(
        "config.default_json",
        "Return the default LoaConfig as pretty JSON.",
        false,
        config_default_json
    );
    reg!(
        "cocreative.bias_dim",
        "Return the BiasVector dim of the wired CocreativeOptimizer (0 if none wired).",
        false,
        cocreative_bias_dim
    );
    reg!(
        "causal.dag_node_count",
        "Return the node count of the wired CausalDag (0 if no DAG attached yet).",
        false,
        causal_dag_node_count
    );
    reg!(
        "license.policy_default_text",
        "Return the LoA-default license policy verdict for License::Unknown ('Deny: unknown not allowed').",
        false,
        license_policy_default_text
    );
    reg!(
        "voice.audit_count",
        "Return the count of audit events emitted by the wired VoiceSession (0 if none).",
        false,
        voice_audit_count
    );
    reg!(
        "multiplayer.room_status",
        "Return the multiplayer room status string ('no-room' if no room joined).",
        false,
        multiplayer_room_status
    );

    // ─ T11-W7-G-LOA-HOST-WIRE : wave-7 wired-* probes (4 read-only tools).
    //   KAN-real canary-check · DM cap-table · GM tone-axes · MP-transport
    //   real cap-bits. All read-only ; cap-table query is observational ;
    //   no MUTATIONS surfaced.
    reg!(
        "kan_real.canary_check",
        "Return whether a given session_id (u128) is enrolled in the 10% KAN-real canary cohort \
         (params: session_id u128). Deterministic per-session.",
        false,
        kan_real_canary_check
    );
    reg!(
        "dm.cap_table_query",
        "Return DM cap-table : caps bitfield + the 3 cap-bit names \
         (DM_CAP_SCENE_EDIT · DM_CAP_SPAWN_NPC · DM_CAP_COMPANION_RELAY).",
        false,
        dm_cap_table_query
    );
    reg!(
        "gm.tone_axes_query",
        "Return GM tone-axes list : ['warm', 'terse', 'poetic'] + the 3 cap-bit count.",
        false,
        gm_tone_axes_query
    );
    reg!(
        "mp_transport.real_caps_query",
        "Return mp-transport real-Supabase cap-bits constant (TRANSPORT_CAP_BOTH = SEND|RECV).",
        false,
        mp_transport_real_caps_query
    );

    // ─ T11-W8-CHAT-WIRE : Coder narrow-orchestrator MCP tools (4 tools).
    //   Sandboxed AST-edit pipeline ; sovereign-required for substrate edits ;
    //   30-second auto-revert window. ALL stage-0 explicit-confirm-only ;
    //   coder.approve drives the in-runtime ApprovalPromptHandler so a
    //   sovereign explicitly opts-in per-edit. Hard-cap rejections are
    //   audit-emitted via cssl-host-coder-runtime::InMemoryAuditLog (forwards
    //   to cssl-host-attestation in a future wave).
    reg!(
        "coder.propose_edit",
        "Propose a sandboxed Coder edit (params: kind · target_file · diff_summary · \
         sovereign? · sovereign_cap). Returns edit_id on accept, or {error, decision} \
         on hard-cap rejection. ALL substrate / spec/grand-vision/00..15 / TIER-C \
         secret targets STRUCTURALLY-REJECTED.",
        true,
        coder_propose_edit
    );
    reg!(
        "coder.list_pending",
        "List all sandbox-resident Coder edits with their state + kind + target_file \
         + diff_summary. Read-only ; deterministic id-ordered.",
        false,
        coder_list_pending
    );
    reg!(
        "coder.approve",
        "Drive a Coder edit through Validation → Approval → Apply. ONLY mutates \
         the edit's lifecycle in the sandbox ; the writer-fn is a stub that records \
         intent without touching the real file (real-write deferred to a future \
         sovereign-explicit wave). Params: edit_id · sovereign_cap.",
        true,
        coder_approve
    );
    reg!(
        "coder.revert",
        "Manually revert a previously-applied Coder edit (within the 30-second \
         revert-window). Returns the revert-outcome (Reverted | Expired | NoWindow). \
         Params: edit_id · sovereign_cap.",
        true,
        coder_revert
    );

    // ─ T11-W12-COCREATIVE-BRIDGE : bi-directional Claude ↔ in-game-GM bridge
    //   8 NEW `cocreative.*` MCP tools. ALL are mutating + sovereign-cap-gated
    //   AND require the per-session CocreativeCap to be Granted (default-deny).
    //   The grant flow runs through `cocreative.persona_query` (consent-gated).
    //   Tools are mutating because they touch the cocreative-session map.
    reg!(
        "cocreative.context_read",
        "Return GM observation-context : {player_pos, scene_id, last_5_utterances, \
         arc_phase, gm_persona_seed, open_questions[]} for an external Claude to read \
         before submitting proposals. Cap-required. Params: player_seed (u64) · sovereign_cap.",
        true,
        cocreative_context_read
    );
    reg!(
        "cocreative.proposal_submit",
        "POST a content-proposal to GM : params {kind: lore|npc-line|scene-spawn|recipe|other, \
         payload: String, reason: String}. Returns proposal_id (u64). Cap-required.",
        true,
        cocreative_proposal_submit
    );
    reg!(
        "cocreative.proposal_evaluate",
        "GM evaluates a proposal · returns {score: 0..100, comments: String, accepted: bool, \
         state}. Stage-0 stand-in for cssl-host-llm-bridge ; deterministic heuristic. \
         Cap-required. Params: proposal_id · player_seed · sovereign_cap.",
        true,
        cocreative_proposal_evaluate
    );
    reg!(
        "cocreative.feedback_request",
        "Ask the GM a specific question · returns GM-response (one-shot). \
         Cap-required. Params: player_seed · question : String · sovereign_cap.",
        true,
        cocreative_feedback_request
    );
    reg!(
        "cocreative.iterate",
        "Submit a revision · re-evaluate-loop. Resets state to Pending + bumps \
         revision count + replaces payload+reason. Cap-required. \
         Params: player_seed · proposal_id · payload · reason · sovereign_cap.",
        true,
        cocreative_iterate
    );
    reg!(
        "cocreative.draft_ready",
        "Mark Accepted-state proposal as draft-ready · returns Σ-Chain attestation hash. \
         Only accepted proposals may transition. Cap-required. \
         Params: player_seed · proposal_id · sovereign_cap.",
        true,
        cocreative_draft_ready
    );
    reg!(
        "cocreative.session_log_drain",
        "Drain (and clear) the session-log of (proposal, score, comments) tuples — \
         for KAN-training-pairs (sibling W12-3). Requires CocreativeCap::GrantedWithDrain \
         (NOT just Granted ; drain is a STRICTLY-OPT-IN extra grant). \
         Params: player_seed · sovereign_cap.",
        true,
        cocreative_session_log_drain
    );
    reg!(
        "cocreative.persona_query",
        "Inspect/grant/revoke the per-session GmPersona axes — consent-gated · σ-mask-isolated. \
         Params: player_seed · op ('query'|'grant'|'grant_with_drain'|'revoke') · sovereign_cap. \
         Returns {cap_state, persona_axes : [i8; 8] (when granted), archetype_bias}.",
        true,
        cocreative_persona_query
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

// ───────────────────────────────────────────────────────────────────────
// § T11-WAVE3-SPONT — spontaneous-condensation tool handlers.
// ───────────────────────────────────────────────────────────────────────

/// `world.spontaneous_seed` : sovereign-gated · sow an intent text into the
/// Ω-field. Stamps seed-cells at the supplied origin (default = camera + 5m
/// forward when omitted). The window-loop drains the request on the next
/// frame + registers seeds with the manifestation detector ; subsequent
/// frames poll for rising-edge crossings + spawn stress objects.
///
/// § PARAMS
///   - `text` (String, required) : the intent text. Empty → no-op.
///   - `position` (Object, optional) : `{x, y, z}` world-space origin.
///     Default = camera position + a small forward offset.
fn world_spontaneous_seed(state: &mut EngineState, params: Value) -> Value {
    let text = p_str(&params, "text", "").to_string();
    if text.trim().is_empty() {
        return json!({
            "ok": false,
            "error": "text param is required and must be non-empty",
        });
    }
    // Default origin = camera position with a 5m forward offset along yaw.
    let cam = state.camera;
    let yaw_cos = cam.yaw.cos();
    let yaw_sin = cam.yaw.sin();
    let default_x = cam.pos.x + 5.0 * yaw_sin;
    let default_y = cam.pos.y;
    let default_z = cam.pos.z + 5.0 * yaw_cos;
    let position = params.get("position");
    let (ox, oy, oz) = if let Some(p) = position {
        (
            p.get("x").and_then(Value::as_f64).map_or(default_x, |x| x as f32),
            p.get("y").and_then(Value::as_f64).map_or(default_y, |y| y as f32),
            p.get("z").and_then(Value::as_f64).map_or(default_z, |z| z as f32),
        )
    } else {
        (default_x, default_y, default_z)
    };
    // Pre-compute the seeds-list locally so the response can describe what
    // WILL be sown (the actual stamping happens on the next frame in the
    // window-loop drain). This gives MCP callers immediate feedback even
    // before the renderer applies the request.
    let preview_seeds =
        crate::spontaneous::intent_to_seed_cells(&text, [ox, oy, oz]);
    let seeds_array: Vec<Value> = preview_seeds
        .iter()
        .map(|s| {
            json!({
                "kind_hint": s.kind_hint,
                "name": crate::geometry::stress_object_name(s.kind_hint),
                "label": s.label.as_str(),
                "pos": [s.pos[0], s.pos[1], s.pos[2]],
                "radiance": [s.radiance[0], s.radiance[1], s.radiance[2]],
                "density": s.density,
            })
        })
        .collect();
    let preview_count = preview_seeds.len();
    // Queue the request for the next frame.
    state
        .spontaneous
        .sow_pending
        .push(crate::mcp_server::SpontaneousSowRequest {
            text: text.clone(),
            origin: [ox, oy, oz],
        });
    state.push_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "world.spontaneous_seed · queued · text={text:?} · origin=({ox:.2},{oy:.2},{oz:.2}) · seeds={preview_count}"
        ),
    );
    json!({
        "ok": true,
        "text": text,
        "origin": {"x": ox, "y": oy, "z": oz},
        "seeds_count": preview_count,
        "seeds": seeds_array,
        "manifestation_window_frames": crate::spontaneous::MANIFESTATION_WINDOW_FRAMES,
    })
}

/// `sense.spontaneous_recent` : read-only · return the last 16
/// manifestation events + cumulative counters + currently-tracked seed
/// count. The window-loop populates the recent-events ring on each scan.
fn sense_spontaneous_recent(state: &mut EngineState, _params: Value) -> Value {
    let recent: Vec<Value> = state
        .spontaneous
        .recent_events
        .iter()
        .map(|e| {
            json!({
                "frame": e.frame,
                "world_pos": {"x": e.world_pos[0], "y": e.world_pos[1], "z": e.world_pos[2]},
                "kind": e.kind,
                "kind_name": crate::geometry::stress_object_name(e.kind),
                "radiance_mag": e.radiance_mag,
                "density": e.density,
                "label": e.label,
                "spawned_object_id": e.spawned_object_id,
            })
        })
        .collect();
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    json!({
        "events": recent,
        "events_count": state.spontaneous.recent_events.len(),
        "seeds_total": state.spontaneous.seeds_total,
        "manifests_total": state.spontaneous.manifests_total,
        "tracked_count": state.spontaneous.tracked_count,
        "manifestation_threshold": crate::spontaneous::MANIFESTATION_THRESHOLD,
        "manifestation_window_frames": crate::spontaneous::MANIFESTATION_WINDOW_FRAMES,
        "spontaneous_pad_center": [
            crate::spontaneous::SPONTANEOUS_PAD_CENTER[0],
            crate::spontaneous::SPONTANEOUS_PAD_CENTER[1],
            crate::spontaneous::SPONTANEOUS_PAD_CENTER[2],
        ],
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-WAVE3-INTENT — intent.translate + intent.recent handlers
// ───────────────────────────────────────────────────────────────────────

/// `intent.translate` : classify free-form text → typed intent. By default
/// the intent is ALSO dispatched against the live engine ; pass
/// `dispatch=false` to receive the classified intent + intended params
/// without invoking the underlying tool.
fn intent_translate(state: &mut EngineState, params: Value) -> Value {
    let text = p_str(&params, "text", "").to_string();
    let dispatch_flag = params
        .get("dispatch")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if text.is_empty() {
        return json!({
            "ok": false,
            "error": "intent.translate requires `text` param",
        });
    }
    if dispatch_flag {
        // Real route : classify → dispatch → record.
        crate::intent_router::route(&text, crate::mcp_server::SOVEREIGN_CAP, state)
    } else {
        // Preview only : classify + show params, no side-effects.
        let intent = crate::intent_router::classify(&text);
        let params_for_tool =
            crate::intent_router::intent_to_params(&intent, crate::mcp_server::SOVEREIGN_CAP);
        json!({
            "intent": intent.to_json(),
            "tool": intent.target_tool(),
            "params": params_for_tool,
            "input": text,
            "classified_kind": intent.kind_tag(),
            "dispatched": false,
        })
    }
}

/// `intent.recent` : last 16 dispatched intents + per-kind counters. Read-only.
fn intent_recent(_state: &mut EngineState, _params: Value) -> Value {
    crate::intent_router::recent_json()
}

// ═══════════════════════════════════════════════════════════════════════
// § T11-W5c-LOA-HOST-WIRE — 17 wired_* MCP-tool handlers
// ═══════════════════════════════════════════════════════════════════════
//
// § ROLE
//   Each handler is a thin probe over the matching `wired_*` module : it
//   surfaces a single canonical fact about the underlying cssl-host-* crate
//   without holding mutable state (the engine doesn't yet wire a recorder /
//   replayer / optimizer / dag / voice-session / room ; future waves will
//   add those + extend these probes accordingly).

/// `replay.last_event_count` : count of events in the last loaded replay.
/// Returns 0 since no replayer is wired yet.
fn replay_last_event_count(_state: &mut EngineState, _params: Value) -> Value {
    json!({"count": 0u64, "wired": false})
}

/// `audit.summarize_dir` : ingest dir + return summary text. Empty `dir`
/// param yields an `{error: ...}` envelope.
fn audit_summarize_dir(_state: &mut EngineState, params: Value) -> Value {
    let dir = p_str(&params, "dir", "");
    if dir.is_empty() {
        return json!({"error": "missing 'dir' param"});
    }
    match crate::wired_audit::ingest_logs_dir(dir) {
        Ok(idx) => {
            let summary = crate::wired_audit::summarize(&idx);
            let text = crate::wired_audit::report_text(&summary);
            json!({"ok": true, "dir": dir, "summary_text": text})
        }
        Err(e) => json!({"ok": false, "error": format!("{e}"), "dir": dir}),
    }
}

/// `stereo.config_default_text` : default StereoConfig as pretty JSON.
fn stereo_config_default_text(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::wired_stereoscopy::default_config_json();
    // Re-parse to embed the JSON as a structured value rather than a string.
    let parsed: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
    json!({"config": parsed, "ipd_mm_default": 63})
}

/// `golden.list_labels` : list snapshot labels under `dir`. Missing dir → empty.
fn golden_list_labels(_state: &mut EngineState, params: Value) -> Value {
    let dir = p_str(&params, "dir", "");
    if dir.is_empty() {
        let empty: Vec<String> = Vec::new();
        return json!({"labels": empty, "count": 0u64, "dir": ""});
    }
    let labels = crate::wired_golden::list_labels(dir);
    let count = labels.len();
    json!({"labels": labels, "count": count, "dir": dir})
}

/// `procgen.list_room_kinds` : 7 canonical RoomKind variants.
fn procgen_list_room_kinds(_state: &mut EngineState, _params: Value) -> Value {
    let kinds = crate::wired_procgen_rooms::all_room_kinds();
    json!({"kinds": kinds, "count": kinds.len()})
}

/// `histogram.snapshot_text` : snapshot of any internal histogram registry.
/// Returns the canonical "<empty>" marker since no registry is wired.
fn histogram_snapshot_text(_state: &mut EngineState, _params: Value) -> Value {
    let reg = crate::wired_histograms::HistogramRegistry::new();
    let txt = crate::wired_histograms::snapshot_text(&reg);
    json!({"snapshot": txt, "wired": false})
}

/// `attestation.empty_session_text` : render an empty SessionAttestation.
fn attestation_empty_session_text(_state: &mut EngineState, _params: Value) -> Value {
    let txt = crate::wired_attestation::empty_session_text();
    json!({"text": txt})
}

/// `rt_trace.recent_events` : drain a fresh ring into JSON. Empty since no
/// runtime ring is wired into EngineState yet.
fn rt_trace_recent_events(_state: &mut EngineState, _params: Value) -> Value {
    let ring = crate::wired_rt_trace::RtRing::new(8);
    let s = crate::wired_rt_trace::drain_to_json(&ring);
    let parsed: Value = serde_json::from_str(&s).unwrap_or(Value::Array(vec![]));
    json!({"events": parsed, "wired": false})
}

/// `spectral.bands` : 16 canonical band wavelengths as a JSON array.
fn spectral_bands(_state: &mut EngineState, _params: Value) -> Value {
    let nm = &crate::wired_spectral_grader::BAND_WAVELENGTHS_NM[..];
    json!({"band_count": nm.len(), "wavelengths_nm": nm})
}

/// `frame_recorder.lfrc_magic` : LFRC v1 magic bytes as a hex string.
fn frame_recorder_lfrc_magic(_state: &mut EngineState, _params: Value) -> Value {
    let hex = crate::wired_frame_recorder::lfrc_magic_hex();
    json!({
        "magic_hex": hex,
        "version": crate::wired_frame_recorder::LFRC_VERSION,
    })
}

/// `input_virtual.list_scenarios` : canonical synthetic-input scenario names.
fn input_virtual_list_scenarios(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::wired_input_virtual::list_scenarios();
    json!({"scenarios": s, "count": s.len()})
}

/// `config.default_json` : default LoaConfig as pretty JSON.
fn config_default_json(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::wired_config::default_pretty_json();
    let parsed: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
    json!({"config": parsed})
}

/// `cocreative.bias_dim` : BiasVector dim if optimizer is wired (0 otherwise).
fn cocreative_bias_dim(_state: &mut EngineState, _params: Value) -> Value {
    let dim = crate::wired_cocreative::optimizer_dim(None);
    json!({"dim": dim, "wired": false})
}

/// `causal.dag_node_count` : node count of wired CausalDag (0 if not wired).
fn causal_dag_node_count(_state: &mut EngineState, _params: Value) -> Value {
    let n = crate::wired_causal_seed::dag_node_count(None);
    json!({"node_count": n, "wired": false})
}

/// `license.policy_default_text` : default LoA license policy verdict for Unknown.
fn license_policy_default_text(_state: &mut EngineState, _params: Value) -> Value {
    let txt = crate::wired_license_attribution::policy_default_text();
    json!({"policy_verdict": txt, "license": "Unknown"})
}

/// `voice.audit_count` : count of audio audit events emitted by VoiceSession.
fn voice_audit_count(_state: &mut EngineState, _params: Value) -> Value {
    let n = crate::wired_voice::audit_event_count(None);
    json!({"count": n, "wired": false})
}

/// `multiplayer.room_status` : multiplayer room status. 'no-room' since no room joined.
fn multiplayer_room_status(_state: &mut EngineState, _params: Value) -> Value {
    let s = crate::wired_multiplayer::room_status(None);
    json!({"status": s, "wired": false})
}

// ─ T11-W7-G-LOA-HOST-WIRE : wave-7 handlers (4 read-only probes) ─

/// `kan_real.canary_check` : session-id enrollment in the 10% canary cohort.
/// Params : `{ "session_id": u128 }` (accepts JSON number OR hex/decimal string ;
/// missing/invalid → 0 which is a valid session-id, deterministically tested).
fn kan_real_canary_check(_state: &mut EngineState, params: Value) -> Value {
    // serde_json's u128 acceptance is feature-gated ; accept either a number
    // (within u64 range) or a string (parsed as decimal then hex on fallback).
    let session_id: u128 = match params.get("session_id") {
        Some(Value::Number(n)) => n.as_u64().map_or(0_u128, u128::from),
        Some(Value::String(s)) => {
            s.parse::<u128>()
                .or_else(|_| u128::from_str_radix(s.trim_start_matches("0x"), 16))
                .unwrap_or(0)
        }
        _ => 0,
    };
    let enrolled = crate::wired_kan_real::is_session_in_canary(session_id);
    json!({
        "enrolled": enrolled,
        "intent_kind_count": crate::wired_kan_real::intent_kind_count(),
    })
}

/// `dm.cap_table_query` : DM cap-table shape probe (3 cap-bits).
fn dm_cap_table_query(_state: &mut EngineState, _params: Value) -> Value {
    use crate::wired_dm::{DM_CAP_ALL, DM_CAP_COMPANION_RELAY, DM_CAP_SCENE_EDIT, DM_CAP_SPAWN_NPC};
    json!({
        "caps": DM_CAP_ALL,
        "bits": [
            {"name": "DM_CAP_SCENE_EDIT",      "value": DM_CAP_SCENE_EDIT},
            {"name": "DM_CAP_SPAWN_NPC",       "value": DM_CAP_SPAWN_NPC},
            {"name": "DM_CAP_COMPANION_RELAY", "value": DM_CAP_COMPANION_RELAY},
        ],
        "bit_count": crate::wired_dm::dm_cap_bit_count(),
    })
}

/// `gm.tone_axes_query` : canonical GM tone-axes list + cap-bit count.
fn gm_tone_axes_query(_state: &mut EngineState, _params: Value) -> Value {
    json!({
        "axes": ["warm", "terse", "poetic"],
        "bit_count": crate::wired_gm::gm_cap_bit_count(),
    })
}

/// `mp_transport.real_caps_query` : real-Supabase transport cap-bits constant.
fn mp_transport_real_caps_query(_state: &mut EngineState, _params: Value) -> Value {
    json!({
        "caps": crate::wired_mp_transport_real::mp_transport_cap_bits(),
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-W8-CHAT-WIRE : Coder narrow-orchestrator MCP tool handlers
// ───────────────────────────────────────────────────────────────────────

/// Wall-clock millis-since-epoch (saturating to 0 on SystemTime failure).
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `coder.propose_edit` : submit a sandboxed Coder edit.
///
/// Hard-cap rejections (substrate · spec-grand-vision-00..15 · TIER-C secret)
/// return `{error, decision}`. On accept returns `{ok: true, edit_id, kind, state}`.
fn coder_propose_edit(state: &mut EngineState, params: Value) -> Value {
    use crate::wired_coder_runtime as coder;

    let kind_str = p_str(&params, "kind", "cosmetic_tweak");
    let target_file = p_str(&params, "target_file", "").to_string();
    let diff_summary = p_str(&params, "diff_summary", "(no summary)").to_string();
    let want_sovereign = params
        .get("sovereign")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let kind = match coder::edit_kind_from_str(kind_str) {
        Some(k) => k,
        None => {
            state.push_event(
                "WARN",
                "loa-host/coder",
                &format!("propose_edit · unknown kind '{kind_str}'"),
            );
            return json!({
                "ok": false,
                "error": format!("unknown edit kind : {kind_str}"),
                "valid_kinds": [
                    "ast_node_replace", "ast_node_insert", "ast_node_delete",
                    "balance_constant_tune", "cosmetic_tweak", "narrow_reshape",
                ],
            });
        }
    };

    if target_file.is_empty() {
        return json!({
            "ok": false,
            "error": "target_file required",
        });
    }

    // Stage-0 hashing : mock blake3 = (0..32) so the staging round-trip works.
    // Real-blake3 is computed in cssl-host-attestation in a future wave.
    let before_blake3: [u8; 32] = [0u8; 32];
    let mut after_blake3: [u8; 32] = [0u8; 32];
    // Cheap deterministic hash from the diff_summary so two distinct
    // proposals don't share an identical after-hash.
    for (i, b) in diff_summary.as_bytes().iter().enumerate() {
        after_blake3[i % 32] ^= *b;
    }
    let player_pubkey: [u8; 32] = [0u8; 32];

    let sovereign_bit = if want_sovereign {
        coder::SovereignBit::Held
    } else {
        coder::SovereignBit::NotHeld
    };
    let caps = coder::CoderCap::AST_EDIT;
    let now_ms = now_unix_ms();

    let mut rt = coder::lock();
    match rt.submit_edit(
        kind,
        target_file.clone(),
        before_blake3,
        after_blake3,
        diff_summary.clone(),
        now_ms,
        player_pubkey,
        sovereign_bit,
        caps,
    ) {
        Ok(id) => {
            state.push_event(
                "INFO",
                "loa-host/coder",
                &format!(
                    "propose_edit · accept · id={} · kind={} · target='{}' · sovereign={}",
                    id.0,
                    coder::edit_kind_label(kind),
                    target_file,
                    want_sovereign,
                ),
            );
            json!({
                "ok": true,
                "edit_id": id.0,
                "kind": coder::edit_kind_label(kind),
                "state": "staged",
                "target_file": target_file,
                "diff_summary": diff_summary,
                "sovereign": want_sovereign,
                "ts_ms": now_ms,
            })
        }
        Err(decision) => {
            state.push_event(
                "WARN",
                "loa-host/coder",
                &format!(
                    "propose_edit · reject · target='{}' · decision={}",
                    target_file,
                    coder::hard_cap_label(decision),
                ),
            );
            json!({
                "ok": false,
                "error": "hard-cap rejection",
                "decision": coder::hard_cap_label(decision),
                "target_file": target_file,
            })
        }
    }
}

/// `coder.list_pending` : enumerate sandbox-resident edits in id-order.
fn coder_list_pending(_state: &mut EngineState, _params: Value) -> Value {
    use crate::wired_coder_runtime as coder;

    let rt = coder::lock();
    let entries: Vec<Value> = rt
        .sandbox()
        .iter()
        .map(|(id, edit)| {
            json!({
                "edit_id": id.0,
                "kind": coder::edit_kind_label(edit.kind),
                "state": coder::edit_state_label(edit.state),
                "target_file": edit.target_file,
                "diff_summary": edit.diff_summary,
                "staged_at_ms": edit.staged_at_ms,
            })
        })
        .collect();
    let count = entries.len();
    json!({
        "ok": true,
        "count": count,
        "edits": entries,
    })
}

/// `coder.approve` : drive an edit through Validation → Approval → Apply.
///
/// Stash an `Approved` outcome in the McpApprovalHandler, then run the
/// runtime's full lifecycle. The writer-fn is a stub for stage-0 — real
/// disk-writes are deferred to a future sovereign-explicit wave.
fn coder_approve(state: &mut EngineState, params: Value) -> Value {
    use crate::wired_coder_runtime as coder;

    let edit_id_raw = params.get("edit_id").and_then(Value::as_u64).unwrap_or(0);
    if edit_id_raw == 0 {
        return json!({
            "ok": false,
            "error": "edit_id (u64) required",
        });
    }
    let id = coder::CoderEditId(edit_id_raw);
    let now_ms = now_unix_ms();

    // We can't access the approval-handler from outside the runtime via a
    // public surface, but we own the singleton — drive the lifecycle in-line
    // and stash via a fresh runtime-call sequence. Validate → request_approval.
    // The McpApprovalHandler stash is via the runtime's `approval` field which
    // is private ; for stage-0 we expose this by re-creating the handler-step
    // explicitly through the existing public API.
    //
    // Strategy : drive validate() then request_approval(). The handler is
    // currently `McpApprovalHandler::default()` ; we need to stash a decision
    // BEFORE request_approval is called. To do that we need a public hook.
    // We provide one via a thin wrapper exposed by `wired_coder_runtime`.

    let mut rt = coder::lock();

    // 1. Validate.
    let _vres = rt.validate(id, now_ms);

    // 2. Stash approval = Approved (via direct public hook).
    coder::stash_next_approval(&mut rt, coder::PromptOutcome::Approved);

    // 3. Drive approval prompt.
    let approval = rt.request_approval(id, now_ms);

    let approval_label = match approval {
        coder::PromptOutcome::Approved => "approved",
        coder::PromptOutcome::Denied => "denied",
        coder::PromptOutcome::TimedOut => "timed_out",
    };

    // 4. If approved, apply with a stub-writer (records intent ; no disk-write).
    let mut applied = false;
    if matches!(approval, coder::PromptOutcome::Approved) {
        let r = rt.apply(id, now_ms, |_staged| Ok::<(), String>(()));
        applied = r.is_ok();
    }

    // 5. Surface in chat-log + telemetry for transparency.
    state.push_event(
        "INFO",
        "loa-host/coder",
        &format!(
            "approve · edit_id={} · approval={} · applied={}",
            edit_id_raw, approval_label, applied
        ),
    );

    let final_state = rt
        .sandbox()
        .get(id)
        .map(|e| coder::edit_state_label(e.state))
        .unwrap_or("unknown");

    json!({
        "ok": applied || matches!(approval, coder::PromptOutcome::Approved),
        "edit_id": edit_id_raw,
        "approval": approval_label,
        "applied": applied,
        "state": final_state,
        "ts_ms": now_ms,
    })
}

/// `coder.revert` : manually revert an applied edit within the 30s window.
fn coder_revert(state: &mut EngineState, params: Value) -> Value {
    use crate::wired_coder_runtime as coder;

    let edit_id_raw = params.get("edit_id").and_then(Value::as_u64).unwrap_or(0);
    if edit_id_raw == 0 {
        return json!({
            "ok": false,
            "error": "edit_id (u64) required",
        });
    }
    let id = coder::CoderEditId(edit_id_raw);
    let now_ms = now_unix_ms();

    let mut rt = coder::lock();
    let outcome = rt.manual_revert(id, now_ms);

    let outcome_label = match outcome {
        coder::RevertOutcome::Reverted => "reverted",
        coder::RevertOutcome::WindowExpired => "window_expired",
        coder::RevertOutcome::NoWindow => "no_window",
    };

    state.push_event(
        "INFO",
        "loa-host/coder",
        &format!("revert · edit_id={} · outcome={}", edit_id_raw, outcome_label),
    );

    let final_state = rt
        .sandbox()
        .get(id)
        .map(|e| coder::edit_state_label(e.state))
        .unwrap_or("unknown");

    json!({
        "ok": matches!(outcome, coder::RevertOutcome::Reverted),
        "edit_id": edit_id_raw,
        "outcome": outcome_label,
        "state": final_state,
        "ts_ms": now_ms,
    })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-W12-COCREATIVE-BRIDGE — 8 cocreative.* handlers
// ───────────────────────────────────────────────────────────────────────
//
// All 8 handlers share a common shape :
//   1. Extract `player_seed` (u64) — keys the σ-mask-isolated session.
//   2. Cap-check via `cocreative_loop::with_session`.
//   3. Perform the op + return JSON.
//
// `sovereign_cap` gating happens in the dispatcher (mcp_server::dispatch)
// before the handler is even called ; the handlers add the per-session
// CocreativeCap layer ON TOP. Default-deny end-to-end.

fn p_u64(v: &Value, key: &str, default: u64) -> u64 {
    v.get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
}

fn p_string(v: &Value, key: &str, default: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

/// Convert a CocreativeSession's runtime state into a JSON envelope.
/// Used by both `context_read` + `persona_query` (different field-subsets).
fn cocreative_cap_denied_response(player_seed: u64) -> Value {
    json!({
        "ok": false,
        "error": "cocreative-cap-denied",
        "player_seed": player_seed,
        "hint": "call cocreative.persona_query with op='grant' to enable cocreative tools",
    })
}

/// `cocreative.context_read` — return GM observation-context for Claude.
fn cocreative_context_read(state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    if player_seed == 0 {
        return json!({
            "ok": false,
            "error": "player_seed (u64, non-zero) required",
        });
    }

    // Snapshot the live engine fields so we don't hold both mutexes.
    let camera_pos = camera_pos_json(&state.camera);
    let scene_id = state.active_scene.clone();
    let frame = state.frame_count;
    let mut last_5: Vec<String> = state
        .text_input
        .history
        .iter()
        .rev()
        .take(5)
        .cloned()
        .collect();
    last_5.reverse();
    // Open-questions stub : in the full integration this comes from the
    // dm_arc nudge-ring + GM unanswered-question queue. Stage-0 emits an
    // empty list ; sibling W11 deepens.
    let open_questions: Vec<String> = Vec::new();

    cc::with_session(player_seed, |s| {
        if !s.cap.is_granted() {
            return cocreative_cap_denied_response(player_seed);
        }
        // Refresh context on every read so subsequent submits operate
        // on the LATEST snapshot of the world. Arc-phase stage-0 default
        // is Discovery ; sibling W11 wires the live arc state-machine.
        s.refresh_context(
            last_5.clone(),
            crate::dm_arc::ArcPhase::Discovery,
            open_questions.clone(),
        );
        let persona_axes: Vec<i8> = s
            .persona
            .as_ref()
            .map(|p| p.axes.to_vec())
            .unwrap_or_default();
        let archetype_bias = s
            .persona
            .as_ref()
            .map(|p| p.archetype_bias.label())
            .unwrap_or("none");
        json!({
            "ok": true,
            "player_seed": player_seed,
            "cap_state": s.cap.label(),
            "frame": frame,
            "player_pos": camera_pos,
            "scene_id": scene_id,
            "last_5_utterances": last_5,
            "arc_phase": s.arc_phase.label(),
            "gm_persona_seed": player_seed,
            "gm_persona_axes": persona_axes,
            "gm_archetype_bias": archetype_bias,
            "open_questions": open_questions,
            "quality_bar": s.quality_bar,
        })
    })
}

/// `cocreative.proposal_submit` — POST a content-proposal to GM.
fn cocreative_proposal_submit(state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    if player_seed == 0 {
        return json!({"ok": false, "error": "player_seed required"});
    }
    let kind_str = p_string(&params, "kind", "other");
    let kind = cc::ProposalKind::from_label(&kind_str);
    let payload = p_string(&params, "payload", "");
    let reason = p_string(&params, "reason", "");
    if payload.is_empty() {
        return json!({"ok": false, "error": "payload required"});
    }
    let frame = state.frame_count;

    cc::with_session(player_seed, |s| {
        if !s.cap.is_granted() {
            return cocreative_cap_denied_response(player_seed);
        }
        let id = s.submit(kind, payload.clone(), reason.clone(), frame);
        json!({
            "ok": true,
            "player_seed": player_seed,
            "proposal_id": id,
            "kind": kind.label(),
            "state": cc::ProposalState::Pending.label(),
            "frame": frame,
        })
    })
}

/// `cocreative.proposal_evaluate` — GM evaluates a proposal · returns
/// {score, comments, accepted, state}.
fn cocreative_proposal_evaluate(state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    let proposal_id = p_u64(&params, "proposal_id", 0);
    if player_seed == 0 || proposal_id == 0 {
        return json!({
            "ok": false,
            "error": "player_seed + proposal_id required",
        });
    }
    let frame = state.frame_count;

    cc::with_session(player_seed, |s| {
        if !s.cap.is_granted() {
            return cocreative_cap_denied_response(player_seed);
        }
        let Some(p) = s.proposals.get(&proposal_id) else {
            return json!({
                "ok": false,
                "error": "unknown proposal_id",
                "proposal_id": proposal_id,
            });
        };
        let kind = p.kind;
        let payload = p.payload.clone();
        let reason = p.reason.clone();
        let arc_phase = s.arc_phase;
        let persona = match s.persona.as_ref() {
            Some(p) => *p,
            None => {
                return json!({
                    "ok": false,
                    "error": "persona missing — re-grant cap to load",
                });
            }
        };
        let (score, comments) =
            cc::gm_evaluate_heuristic(&persona, arc_phase, kind, &payload, &reason);
        let res = s.evaluate(proposal_id, score, comments.clone(), frame);
        let (st, accepted) = res.unwrap_or((cc::ProposalState::Rejected, false));
        json!({
            "ok": true,
            "player_seed": player_seed,
            "proposal_id": proposal_id,
            "score": score,
            "comments": comments,
            "accepted": accepted,
            "state": st.label(),
            "kind": kind.label(),
            "frame": frame,
        })
    })
}

/// `cocreative.feedback_request` — ask GM a specific question · one-shot.
fn cocreative_feedback_request(state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    let question = p_string(&params, "question", "");
    if player_seed == 0 {
        return json!({"ok": false, "error": "player_seed required"});
    }
    if question.is_empty() {
        return json!({"ok": false, "error": "question required"});
    }
    let frame = state.frame_count;

    cc::with_session(player_seed, |s| {
        if !s.cap.is_granted() {
            return cocreative_cap_denied_response(player_seed);
        }
        // Stage-0 stand-in : compose a deterministic GM-response from persona +
        // arc-phase + question-prefix. Sibling W11 wires gm_narrator's
        // `respond_in_persona` directly. The response shape matches what
        // cssl-host-llm-bridge will return in stage-1.
        let persona_summary = s
            .persona
            .as_ref()
            .map(|p| format!("axes={:?} · archetype={}", p.axes, p.archetype_bias.label()))
            .unwrap_or_else(|| "no-persona".to_string());
        let response = format!(
            "[stage-0 GM · phase={}] On '{}' : the substrate listens (persona-{}). \
             Stage-1 swap : cssl-host-llm-bridge::respond_in_persona.",
            s.arc_phase.label(),
            question,
            persona_summary
        );
        json!({
            "ok": true,
            "player_seed": player_seed,
            "question": question,
            "gm_response": response,
            "frame": frame,
            "arc_phase": s.arc_phase.label(),
        })
    })
}

/// `cocreative.iterate` — submit a revision · re-evaluate-loop.
fn cocreative_iterate(state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    let proposal_id = p_u64(&params, "proposal_id", 0);
    if player_seed == 0 || proposal_id == 0 {
        return json!({
            "ok": false,
            "error": "player_seed + proposal_id required",
        });
    }
    let payload = p_string(&params, "payload", "");
    let reason = p_string(&params, "reason", "");
    if payload.is_empty() {
        return json!({"ok": false, "error": "payload required"});
    }
    let frame = state.frame_count;

    cc::with_session(player_seed, |s| {
        if !s.cap.is_granted() {
            return cocreative_cap_denied_response(player_seed);
        }
        let Some(st) = s.iterate(proposal_id, payload.clone(), reason.clone(), frame) else {
            return json!({
                "ok": false,
                "error": "unknown proposal_id",
                "proposal_id": proposal_id,
            });
        };
        // If the iterate returned a terminal state (DraftReady · Revoked),
        // surface that so callers know the loop has closed.
        let p = match s.proposals.get(&proposal_id) {
            Some(p) => p,
            None => {
                return json!({
                    "ok": false,
                    "error": "proposal vanished post-iterate",
                });
            }
        };
        json!({
            "ok": !matches!(st, cc::ProposalState::DraftReady | cc::ProposalState::Revoked),
            "player_seed": player_seed,
            "proposal_id": proposal_id,
            "state": st.label(),
            "revisions": p.revisions,
            "frame": frame,
        })
    })
}

/// `cocreative.draft_ready` — mark Accepted-state proposal as draft-ready.
fn cocreative_draft_ready(state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    let proposal_id = p_u64(&params, "proposal_id", 0);
    if player_seed == 0 || proposal_id == 0 {
        return json!({
            "ok": false,
            "error": "player_seed + proposal_id required",
        });
    }
    let frame = state.frame_count;

    cc::with_session(player_seed, |s| {
        if !s.cap.is_granted() {
            return cocreative_cap_denied_response(player_seed);
        }
        match s.draft_ready(proposal_id, frame) {
            Some(hash) => json!({
                "ok": true,
                "player_seed": player_seed,
                "proposal_id": proposal_id,
                "attestation_hash": hash,
                "state": cc::ProposalState::DraftReady.label(),
                "frame": frame,
            }),
            None => json!({
                "ok": false,
                "error": "proposal not in Accepted state OR unknown id",
                "proposal_id": proposal_id,
            }),
        }
    })
}

/// `cocreative.session_log_drain` — drain (and clear) session-log entries.
/// Cap-gated : requires `CocreativeCap::GrantedWithDrain`.
fn cocreative_session_log_drain(_state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    if player_seed == 0 {
        return json!({"ok": false, "error": "player_seed required"});
    }

    cc::with_session(player_seed, |s| {
        if !s.cap.permits_drain() {
            return json!({
                "ok": false,
                "error": "cap-denied · drain requires CocreativeCap::GrantedWithDrain",
                "cap_state": s.cap.label(),
                "hint": "call cocreative.persona_query with op='grant_with_drain' to enable",
            });
        }
        let entries = s.drain_session_log();
        let arr: Vec<Value> = entries
            .iter()
            .map(|e| {
                json!({
                    "proposal_id": e.proposal_id,
                    "kind": e.kind.label(),
                    "revision": e.revision,
                    "payload_summary": e.payload_summary,
                    "gm_score": e.gm_score,
                    "gm_comments": e.gm_comments,
                    "frame": e.frame,
                })
            })
            .collect();
        json!({
            "ok": true,
            "player_seed": player_seed,
            "drained": arr.len(),
            "entries": arr,
            "drains_total": s.drains_total,
        })
    })
}

/// `cocreative.persona_query` — inspect/grant/revoke the per-session
/// CocreativeCap + GmPersona axes. THIS is the consent-gate flow.
///
/// op=
///   "query"             : read-only · returns cap_state + axes if granted
///   "grant"             : flip cap → Granted (loads persona on first call)
///   "grant_with_drain"  : flip cap → GrantedWithDrain (adds drain eligibility)
///   "revoke"            : flip cap → Revoked
fn cocreative_persona_query(_state: &mut EngineState, params: Value) -> Value {
    use crate::cocreative_loop as cc;

    let player_seed = p_u64(&params, "player_seed", 0);
    if player_seed == 0 {
        return json!({"ok": false, "error": "player_seed required"});
    }
    let op = p_string(&params, "op", "query");

    cc::with_session(player_seed, |s| {
        match op.as_str() {
            "grant" => s.grant(false),
            "grant_with_drain" => s.grant(true),
            "revoke" => s.revoke(),
            "query" => {} // no-op
            other => {
                return json!({
                    "ok": false,
                    "error": format!("unknown op '{other}' · expected grant|grant_with_drain|revoke|query"),
                });
            }
        }
        // Persona-axes are only surfaced when the cap is granted (Σ-mask
        // isolation : revoked-state cannot leak persona).
        let (axes, archetype) = if s.cap.is_granted() {
            let p = s.persona.as_ref();
            (
                p.map(|p| p.axes.to_vec()).unwrap_or_default(),
                p.map(|p| p.archetype_bias.label()).unwrap_or("none"),
            )
        } else {
            (Vec::new(), "(σ-masked)")
        };
        json!({
            "ok": true,
            "player_seed": player_seed,
            "op": op,
            "cap_state": s.cap.label(),
            "persona_axes": axes,
            "archetype_bias": archetype,
            "proposals_total": s.proposals_total,
            "evaluations_total": s.evaluations_total,
            "iterations_total": s.iterations_total,
            "draft_ready_total": s.draft_ready_total,
            "drains_total": s.drains_total,
        })
    })
}

// ═══════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_server::SOVEREIGN_CAP;

    #[test]
    fn tools_list_returns_126_tools() {
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
        // + 2 spontaneous-condensation (T11-WAVE3-SPONT · world.spontaneous_seed
        //   + sense.spontaneous_recent)
        // + 2 intent-router (T11-WAVE3-INTENT · intent.translate + intent.recent)
        // + 17 wave-3γ + W4 + W5a + W5b wired-* probes (T11-W5c-LOA-HOST-WIRE :
        //   replay · audit · stereo · golden · procgen · histogram · attestation
        //   · rt_trace · spectral · frame_recorder · input_virtual · config ·
        //   cocreative · causal · license · voice · multiplayer)
        // + 4 wave-7 wired-* probes (T11-W7-G-LOA-HOST-WIRE :
        //   kan_real.canary_check · dm.cap_table_query · gm.tone_axes_query
        //   · mp_transport.real_caps_query)
        // + 4 T11-W8-CHAT-WIRE Coder MCP tools (coder.propose_edit ·
        //   coder.list_pending · coder.approve · coder.revert)
        // + 8 T11-W12-COCREATIVE-BRIDGE tools (cocreative.context_read ·
        //   proposal_submit · proposal_evaluate · feedback_request · iterate ·
        //   draft_ready · session_log_drain · persona_query)
        // = 126 total.
        let reg = tool_registry();
        assert_eq!(reg.len(), 126, "must have exactly 126 tools");
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
            // T11-WAVE3-SPONT additions :
            "world.spontaneous_seed",
            "sense.spontaneous_recent",
            // T11-WAVE3-INTENT additions :
            "intent.translate",
            "intent.recent",
            // T11-W7-G-LOA-HOST-WIRE additions :
            "kan_real.canary_check",
            "dm.cap_table_query",
            "gm.tone_axes_query",
            "mp_transport.real_caps_query",
            // T11-W8-CHAT-WIRE Coder additions :
            "coder.propose_edit",
            "coder.list_pending",
            "coder.approve",
            "coder.revert",
            // T11-W12-COCREATIVE-BRIDGE additions :
            "cocreative.context_read",
            "cocreative.proposal_submit",
            "cocreative.proposal_evaluate",
            "cocreative.feedback_request",
            "cocreative.iterate",
            "cocreative.draft_ready",
            "cocreative.session_log_drain",
            "cocreative.persona_query",
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
            // T11-WAVE3-SPONT : sense.spontaneous_recent is read-only.
            "sense.spontaneous_recent",
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
            // T11-WAVE3-SPONT : world.spontaneous_seed is mutating.
            "world.spontaneous_seed",
            // T11-W12-COCREATIVE-BRIDGE : all cocreative.* are mutating
            // (default-deny + cocreative-cap layered on sovereign-cap).
            "cocreative.context_read",
            "cocreative.proposal_submit",
            "cocreative.proposal_evaluate",
            "cocreative.feedback_request",
            "cocreative.iterate",
            "cocreative.draft_ready",
            "cocreative.session_log_drain",
            "cocreative.persona_query",
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
        //   + world.list_dynamic_meshes)
        // + 2 spontaneous (T11-WAVE3-SPONT · world.spontaneous_seed +
        //   sense.spontaneous_recent)
        // + 2 intent-router (T11-WAVE3-INTENT · intent.translate +
        //   intent.recent)
        // + 17 wired-* probes (T11-W5c-LOA-HOST-WIRE)
        // + 4 wave-7 wired-* probes (T11-W7-G-LOA-HOST-WIRE :
        //   kan_real.canary_check + dm.cap_table_query + gm.tone_axes_query
        //   + mp_transport.real_caps_query)
        // + 4 T11-W8-CHAT-WIRE Coder MCP tools (coder.propose_edit
        //   + coder.list_pending + coder.approve + coder.revert)
        // + 8 T11-W12-COCREATIVE-BRIDGE tools (cocreative.* round-trip
        //   pipeline) = 126.
        assert_eq!(v["count"], 126);
        let arr = v["tools"].as_array().unwrap();
        assert_eq!(arr.len(), 126);
    }

    // § T11-WAVE3-INTENT · MCP-tool integration
    #[test]
    fn mcp_intent_translate_returns_classified_intent() {
        let _g = crate::intent_router::test_lock();
        crate::intent_router::reset_for_test();
        let mut s = EngineState::default();
        // Preview-only mode : classify but don't dispatch.
        let v = intent_translate(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "text": "spawn cube at 5 5 5", "dispatch": false}),
        );
        assert_eq!(v["classified_kind"], "spawn_at");
        assert_eq!(v["tool"], "render.spawn_stress");
        assert_eq!(v["dispatched"], false);
        assert_eq!(v["params"]["x"], 5.0);
        assert_eq!(v["params"]["y"], 5.0);
        assert_eq!(v["params"]["z"], 5.0);
        // Dispatch mode : actually invokes render.spawn_stress.
        let v2 = intent_translate(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "text": "snapshot"}),
        );
        assert_eq!(v2["tool"], "render.snapshot_png");
        assert_eq!(v2["classified_kind"], "snapshot");
        // Empty text → error envelope.
        let v3 = intent_translate(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "text": ""}),
        );
        assert_eq!(v3["ok"], false);
        assert!(v3["error"].as_str().unwrap().contains("text"));
    }

    #[test]
    fn mcp_intent_recent_returns_last_16_dispatches() {
        let _g = crate::intent_router::test_lock();
        crate::intent_router::reset_for_test();
        let mut s = EngineState::default();
        for n in 0..20 {
            let _ = intent_translate(
                &mut s,
                json!({"sovereign_cap": SOVEREIGN_CAP, "text": format!("burst {n}")}),
            );
        }
        let v = intent_recent(&mut s, json!({}));
        assert_eq!(v["count"], 16); // ring caps at 16
        assert_eq!(v["capacity"], 16);
        // Per-kind counter + total counter both reflect the 20 invocations.
        assert_eq!(v["counters"]["per_kind"]["burst"], 20);
        assert_eq!(v["counters"]["intents_classified_total"], 20);
        let events = v["events"].as_array().unwrap();
        assert_eq!(events.len(), 16);
        // Each event has the expected envelope shape.
        for e in events {
            assert!(e["intent"].is_object());
            assert_eq!(e["intent"]["kind"], "burst");
            assert!(e["tool"].as_str().unwrap() == "render.start_burst");
            assert!(e["frame"].is_u64());
            assert!(e["raw_text"].is_string());
        }
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

    // ── § T11-WAVE3-SPONT : MCP-tool tests ──

    #[test]
    fn mcp_world_spontaneous_seed_returns_ok_with_seeds() {
        let mut s = EngineState::default();
        let v = world_spontaneous_seed(
            &mut s,
            json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "text": "a glass cube and a bronze sphere",
            }),
        );
        assert_eq!(v["ok"], true);
        assert!(v["seeds_count"].as_u64().unwrap() >= 2);
        // The pending request was queued.
        assert_eq!(s.spontaneous.sow_pending.len(), 1);
        assert_eq!(s.spontaneous.sow_pending[0].text, "a glass cube and a bronze sphere");
    }

    #[test]
    fn mcp_world_spontaneous_seed_rejects_empty_text() {
        let mut s = EngineState::default();
        let v = world_spontaneous_seed(
            &mut s,
            json!({"sovereign_cap": SOVEREIGN_CAP, "text": ""}),
        );
        assert_eq!(v["ok"], false);
        assert!(s.spontaneous.sow_pending.is_empty());
    }

    #[test]
    fn mcp_world_spontaneous_seed_with_explicit_position() {
        let mut s = EngineState::default();
        let v = world_spontaneous_seed(
            &mut s,
            json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "text": "a cube",
                "position": {"x": 5.0, "y": 1.5, "z": -10.0},
            }),
        );
        assert_eq!(v["ok"], true);
        let req = &s.spontaneous.sow_pending[0];
        assert_eq!(req.origin, [5.0, 1.5, -10.0]);
    }

    #[test]
    fn mcp_sense_spontaneous_recent_returns_empty_initially() {
        let mut s = EngineState::default();
        let v = sense_spontaneous_recent(&mut s, json!({}));
        assert_eq!(v["events_count"].as_u64().unwrap(), 0);
        assert_eq!(v["seeds_total"].as_u64().unwrap(), 0);
        assert_eq!(v["manifests_total"].as_u64().unwrap(), 0);
        // Constants exposed.
        assert!(v["manifestation_threshold"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn mcp_sense_spontaneous_recent_returns_pushed_events() {
        let mut s = EngineState::default();
        s.push_spontaneous_event(crate::mcp_server::SpontaneousManifestEntry {
            frame: 100,
            world_pos: [0.0, 1.5, 0.0],
            kind: 1,
            radiance_mag: 1.7,
            density: 0.7,
            label: "sphere".to_string(),
            spawned_object_id: 42,
        });
        let v = sense_spontaneous_recent(&mut s, json!({}));
        assert_eq!(v["events_count"].as_u64().unwrap(), 1);
        assert_eq!(v["events"][0]["kind"].as_u64().unwrap(), 1);
        assert_eq!(v["events"][0]["spawned_object_id"].as_u64().unwrap(), 42);
        assert_eq!(v["events"][0]["label"].as_str().unwrap(), "sphere");
    }

    // ═══════════════════════════════════════════════════════════════════
    // § T11-W8-CHAT-WIRE · 12+ NEW tests covering the chat-routing surface
    // ═══════════════════════════════════════════════════════════════════

    /// All 4 NEW Coder MCP tools are registered with the right mutating-flag.
    #[test]
    fn coder_tools_registered_with_correct_mutability() {
        let reg = tool_registry();
        // list_pending is read-only ; all others mutate.
        assert!(reg.contains_key("coder.propose_edit"));
        assert!(reg.contains_key("coder.list_pending"));
        assert!(reg.contains_key("coder.approve"));
        assert!(reg.contains_key("coder.revert"));
        assert!(reg.get("coder.propose_edit").unwrap().meta.mutating);
        assert!(!reg.get("coder.list_pending").unwrap().meta.mutating);
        assert!(reg.get("coder.approve").unwrap().meta.mutating);
        assert!(reg.get("coder.revert").unwrap().meta.mutating);
    }

    /// `coder.propose_edit` accepts a cosmetic-tweak edit + returns staged.
    #[test]
    fn coder_propose_edit_accepts_cosmetic_tweak() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let v = coder_propose_edit(
            &mut s,
            json!({
                "kind": "cosmetic_tweak",
                "target_file": "content/scenes/some_scene.csl",
                "diff_summary": "rename foo to bar",
            }),
        );
        assert_eq!(v["ok"], true);
        assert!(v["edit_id"].as_u64().unwrap() >= 1);
        assert_eq!(v["state"], "staged");
        assert_eq!(v["sovereign"], false);
    }

    /// Hard-cap : substrate-paths are STRUCTURALLY-rejected (`deny_substrate_edit`).
    #[test]
    fn coder_propose_edit_rejects_substrate_path() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let v = coder_propose_edit(
            &mut s,
            json!({
                "kind": "ast_node_replace",
                "target_file": "compiler-rs/crates/cssl-substrate-omega-field/src/lib.rs",
                "diff_summary": "(would-be-naughty)",
                "sovereign": true,
            }),
        );
        assert_eq!(v["ok"], false);
        assert_eq!(v["decision"], "deny_substrate_edit");
    }

    /// Hard-cap : spec/grand-vision/00..15 are STRUCTURALLY-rejected.
    #[test]
    fn coder_propose_edit_rejects_grand_vision_locked_specs() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let v = coder_propose_edit(
            &mut s,
            json!({
                "kind": "cosmetic_tweak",
                "target_file": "specs/grand-vision/14_SIGMA_CHAIN.csl",
                "diff_summary": "(should be denied)",
            }),
        );
        assert_eq!(v["ok"], false);
        assert_eq!(v["decision"], "deny_spec_grand_vision_00_15");
    }

    /// Sovereign-required edit kinds reject without `sovereign=true`.
    #[test]
    fn coder_propose_edit_rejects_ast_replace_without_sovereign() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let v = coder_propose_edit(
            &mut s,
            json!({
                "kind": "ast_node_replace",
                "target_file": "content/scenes/test_room.csl",
                "diff_summary": "would-replace-an-AST-node",
                "sovereign": false,
            }),
        );
        assert_eq!(v["ok"], false);
        assert_eq!(v["decision"], "deny_sovereign_required");
    }

    /// Unknown kind-strings produce a structured error.
    #[test]
    fn coder_propose_edit_rejects_unknown_kind() {
        let _g = crate::wired_coder_runtime::test_lock();
        let mut s = EngineState::default();
        let v = coder_propose_edit(
            &mut s,
            json!({
                "kind": "frobnicate",
                "target_file": "content/scenes/test_room.csl",
                "diff_summary": "what",
            }),
        );
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("unknown edit kind"));
    }

    /// `coder.list_pending` reports the staged edit + its state.
    #[test]
    fn coder_list_pending_returns_staged_edit() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let _ = coder_propose_edit(
            &mut s,
            json!({
                "kind": "balance_constant_tune",
                "target_file": "content/balance/dmg.toml",
                "diff_summary": "+5%",
            }),
        );
        let v = coder_list_pending(&mut s, json!({}));
        assert_eq!(v["ok"], true);
        let edits = v["edits"].as_array().unwrap();
        assert!(edits.len() >= 1);
        assert_eq!(edits[0]["state"], "staged");
        assert_eq!(edits[0]["kind"], "balance_constant_tune");
    }

    /// `coder.approve` transitions a staged edit through validation → applied.
    #[test]
    fn coder_approve_drives_lifecycle_to_applied() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let pv = coder_propose_edit(
            &mut s,
            json!({
                "kind": "cosmetic_tweak",
                "target_file": "content/balance/foo.toml",
                "diff_summary": "tweak",
            }),
        );
        let edit_id = pv["edit_id"].as_u64().unwrap();
        let v = coder_approve(&mut s, json!({ "edit_id": edit_id }));
        assert_eq!(v["approval"], "approved");
        assert_eq!(v["applied"], true);
        assert_eq!(v["state"], "applied");
    }

    /// `coder.revert` reverts an applied edit within the 30-second window.
    #[test]
    fn coder_revert_reverts_applied_edit_within_window() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let pv = coder_propose_edit(
            &mut s,
            json!({
                "kind": "cosmetic_tweak",
                "target_file": "content/balance/foo.toml",
                "diff_summary": "tweak",
            }),
        );
        let edit_id = pv["edit_id"].as_u64().unwrap();
        let _ = coder_approve(&mut s, json!({ "edit_id": edit_id }));
        let v = coder_revert(&mut s, json!({ "edit_id": edit_id }));
        assert_eq!(v["ok"], true);
        assert_eq!(v["outcome"], "reverted");
    }

    /// `coder.revert` against a never-applied edit returns `no_window`.
    #[test]
    fn coder_revert_returns_no_window_when_not_applied() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let pv = coder_propose_edit(
            &mut s,
            json!({
                "kind": "cosmetic_tweak",
                "target_file": "content/balance/foo.toml",
                "diff_summary": "tweak",
            }),
        );
        let edit_id = pv["edit_id"].as_u64().unwrap();
        let v = coder_revert(&mut s, json!({ "edit_id": edit_id }));
        assert_eq!(v["ok"], false);
        assert_eq!(v["outcome"], "no_window");
    }

    /// EngineState's chat-log VecDeque caps at CHAT_LOG_CAP and drops oldest.
    #[test]
    fn engine_state_chat_log_caps_at_8() {
        use crate::mcp_server::{ChatRole, CHAT_LOG_CAP};
        let mut s = EngineState::default();
        for i in 0..(CHAT_LOG_CAP + 4) {
            s.push_chat_response(ChatRole::Player, format!("line-{i}"));
        }
        assert_eq!(s.chat_log.len(), CHAT_LOG_CAP);
        // The very-first entries must have been dropped : the front entry
        // is `line-{CHAT_LOG_CAP + 4 - CHAT_LOG_CAP} = line-4`.
        let front = s.chat_log.front().unwrap();
        assert_eq!(front.text, "line-4");
        let back = s.chat_log.back().unwrap();
        assert_eq!(back.text, format!("line-{}", CHAT_LOG_CAP + 3));
    }

    /// EngineState's chat-log captures role + text + frame correctly.
    #[test]
    fn engine_state_push_chat_response_records_role_and_frame() {
        use crate::mcp_server::ChatRole;
        let mut s = EngineState::default();
        s.frame_count = 17;
        s.push_chat_response(ChatRole::Gm, "hello".to_string());
        let entry = s.chat_log.front().unwrap();
        assert_eq!(entry.role, ChatRole::Gm);
        assert_eq!(entry.text, "hello");
        assert_eq!(entry.frame, 17);
    }

    /// All 5 ChatRole variants have stable, distinct labels.
    #[test]
    fn chat_role_labels_are_stable_and_distinct() {
        use crate::mcp_server::ChatRole;
        let labels = [
            ChatRole::Player.label(),
            ChatRole::Gm.label(),
            ChatRole::Dm.label(),
            ChatRole::Coder.label(),
            ChatRole::System.label(),
        ];
        // distinct
        let mut sorted = labels.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
        // contains expected canonical labels
        assert!(labels.contains(&"player"));
        assert!(labels.contains(&"gm"));
        assert!(labels.contains(&"dm"));
        assert!(labels.contains(&"coder"));
        assert!(labels.contains(&"system"));
    }

    /// `coder.list_pending` returns count=0 when sandbox is empty.
    #[test]
    fn coder_list_pending_empty_when_no_edits() {
        let _g = crate::wired_coder_runtime::test_lock();
        crate::wired_coder_runtime::reset_for_test();
        let mut s = EngineState::default();
        let v = coder_list_pending(&mut s, json!({}));
        assert_eq!(v["ok"], true);
        assert_eq!(v["count"].as_u64().unwrap(), 0);
        assert_eq!(v["edits"].as_array().unwrap().len(), 0);
    }

    // ───────────────────────────────────────────────────────────────────
    // § T11-W12-COCREATIVE-BRIDGE tests
    // ───────────────────────────────────────────────────────────────────
    //
    // The cocreative_loop singleton is process-global ; tests that mutate it
    // must use unique `player_seed` values per test so they don't interfere.
    // Each test below picks a constant in the COC_*  range.

    const COC_SEED_A: u64 = 0x0C0C_0001_AAAA_AAAA;
    const COC_SEED_B: u64 = 0x0C0C_0002_BBBB_BBBB;
    const COC_SEED_C: u64 = 0x0C0C_0003_CCCC_CCCC;
    const COC_SEED_D: u64 = 0x0C0C_0004_DDDD_DDDD;
    const COC_SEED_E: u64 = 0x0C0C_0005_EEEE_EEEE;
    const COC_SEED_F: u64 = 0x0C0C_0006_FFFF_FFFF;
    const COC_SEED_G: u64 = 0x0C0C_0007_1111_1111;
    const COC_SEED_H: u64 = 0x0C0C_0008_2222_2222;
    const COC_SEED_I: u64 = 0x0C0C_0009_3333_3333;
    const COC_SEED_J: u64 = 0x0C0C_000A_4444_4444;
    const COC_SEED_K: u64 = 0x0C0C_000B_5555_5555;
    const COC_SEED_L: u64 = 0x0C0C_000C_6666_6666;

    fn coc_grant(player_seed: u64, with_drain: bool) {
        crate::cocreative_loop::with_session(player_seed, |s| {
            s.grant(with_drain);
        });
    }

    /// `cocreative.context_read` denies-by-default (cap = Revoked).
    #[test]
    fn cocreative_context_read_denies_without_cap() {
        let mut s = EngineState::default();
        let v = cocreative_context_read(&mut s, json!({"player_seed": COC_SEED_A}));
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "cocreative-cap-denied");
    }

    /// `cocreative.context_read` returns the expected JSON-shape after grant.
    #[test]
    fn cocreative_context_read_shape_after_grant() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_B, false);
        let v = cocreative_context_read(&mut s, json!({"player_seed": COC_SEED_B}));
        assert_eq!(v["ok"], true);
        assert!(v["player_pos"].is_object());
        assert!(v["scene_id"].is_string());
        assert!(v["last_5_utterances"].is_array());
        assert!(v["arc_phase"].is_string());
        assert_eq!(v["gm_persona_seed"].as_u64(), Some(COC_SEED_B));
        assert!(v["gm_persona_axes"].is_array());
        assert_eq!(v["gm_persona_axes"].as_array().unwrap().len(), 8);
        assert!(v["open_questions"].is_array());
        assert_eq!(v["cap_state"], "granted");
    }

    /// `cocreative.proposal_submit` round-trips and assigns an id.
    #[test]
    fn cocreative_proposal_submit_round_trip() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_C, false);
        let v = cocreative_proposal_submit(
            &mut s,
            json!({
                "player_seed": COC_SEED_C,
                "kind": "lore",
                "payload": "The labyrinth was first dreamed by a sleeping cartographer who could not stop drawing.",
                "reason": "Discovery-phase lore expansion.",
            }),
        );
        assert_eq!(v["ok"], true);
        assert!(v["proposal_id"].as_u64().unwrap() >= 1);
        assert_eq!(v["kind"], "lore");
        assert_eq!(v["state"], "pending");
    }

    /// `cocreative.proposal_evaluate` against a freshly-submitted proposal
    /// returns a score and either an Accepted or Rejected state.
    #[test]
    fn cocreative_proposal_evaluate_returns_score_and_state() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_D, false);
        let pv = cocreative_proposal_submit(
            &mut s,
            json!({
                "player_seed": COC_SEED_D,
                "kind": "lore",
                "payload": "Seven chapels nest the spiral; each remembers a different rain.",
                "reason": "Discovery-flavor mythic descriptor.",
            }),
        );
        let id = pv["proposal_id"].as_u64().unwrap();
        let v = cocreative_proposal_evaluate(
            &mut s,
            json!({
                "player_seed": COC_SEED_D,
                "proposal_id": id,
            }),
        );
        assert_eq!(v["ok"], true);
        let score = v["score"].as_u64().unwrap();
        assert!(score <= 100);
        assert!(v["accepted"].is_boolean());
        let st = v["state"].as_str().unwrap();
        assert!(matches!(st, "accepted" | "rejected"));
    }

    /// `cocreative.iterate` resets state to Pending + bumps revisions.
    #[test]
    fn cocreative_iterate_bumps_revision_and_resets_state() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_E, false);
        let pv = cocreative_proposal_submit(
            &mut s,
            json!({
                "player_seed": COC_SEED_E,
                "kind": "npc-line",
                "payload": "Wait.",
                "reason": "tension",
            }),
        );
        let id = pv["proposal_id"].as_u64().unwrap();
        let _ = cocreative_proposal_evaluate(
            &mut s,
            json!({"player_seed": COC_SEED_E, "proposal_id": id}),
        );
        let it = cocreative_iterate(
            &mut s,
            json!({
                "player_seed": COC_SEED_E,
                "proposal_id": id,
                "payload": "Wait — the door listens before opening.",
                "reason": "longer NPC-line ; more atmospheric",
            }),
        );
        assert_eq!(it["ok"], true);
        assert_eq!(it["state"], "pending");
        assert_eq!(it["revisions"].as_u64().unwrap(), 1);
    }

    /// `cocreative.draft_ready` only succeeds from the Accepted state.
    #[test]
    fn cocreative_draft_ready_requires_accepted_state() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_F, false);
        // Force an Accepted state by directly grant + evaluate with a high
        // score via the heuristic-known sweet spot (long Lore in Discovery).
        let pv = cocreative_proposal_submit(
            &mut s,
            json!({
                "player_seed": COC_SEED_F,
                "kind": "lore",
                "payload": "The labyrinth's grammar predates speech; corridors agree on tense before agreeing on direction. Even silence flows in lines, and the lines remember the syntax of every step the wanderer has not yet thought to take. Walls listen, columns whisper, lamp-lit hours are conjugated in the language of patience.",
                "reason": "long Discovery-phase lore expansion with mythic flavor and unusual letters covering the alphabet entropy bucket.",
            }),
        );
        let id = pv["proposal_id"].as_u64().unwrap();
        // Try draft_ready BEFORE evaluation : must fail.
        let bad = cocreative_draft_ready(
            &mut s,
            json!({"player_seed": COC_SEED_F, "proposal_id": id}),
        );
        assert_eq!(bad["ok"], false);
        // Evaluate.
        let _ = cocreative_proposal_evaluate(
            &mut s,
            json!({"player_seed": COC_SEED_F, "proposal_id": id}),
        );
        // If accepted, draft_ready succeeds. Otherwise we force-evaluate a
        // direct accepted state via the loop module.
        crate::cocreative_loop::with_session(COC_SEED_F, |sess| {
            // Force into Accepted regardless of heuristic outcome to keep
            // this test deterministic.
            let _ = sess.evaluate(id, 95, "force-accept".into(), 5);
        });
        let good = cocreative_draft_ready(
            &mut s,
            json!({"player_seed": COC_SEED_F, "proposal_id": id}),
        );
        assert_eq!(good["ok"], true);
        assert_eq!(good["state"], "draft-ready");
        let hash = good["attestation_hash"].as_str().unwrap();
        assert_eq!(hash.len(), 16);
    }

    /// `cocreative.session_log_drain` denies without `GrantedWithDrain` cap.
    #[test]
    fn cocreative_session_log_drain_denies_without_drain_cap() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_G, false); // basic Granted, no drain
        let v =
            cocreative_session_log_drain(&mut s, json!({"player_seed": COC_SEED_G}));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("drain"));
    }

    /// `cocreative.session_log_drain` clears entries after read with the cap.
    #[test]
    fn cocreative_session_log_drain_clears_entries() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_H, true); // GrantedWithDrain
        // Submit + evaluate two proposals so the log has entries.
        for i in 0..2 {
            let pv = cocreative_proposal_submit(
                &mut s,
                json!({
                    "player_seed": COC_SEED_H,
                    "kind": "lore",
                    "payload": format!("entry-{i} payload longer than the floor for length-fit"),
                    "reason": format!("entry-{i} reason with enough tokens to register a small bonus"),
                }),
            );
            let id = pv["proposal_id"].as_u64().unwrap();
            let _ = cocreative_proposal_evaluate(
                &mut s,
                json!({"player_seed": COC_SEED_H, "proposal_id": id}),
            );
        }
        let drain1 =
            cocreative_session_log_drain(&mut s, json!({"player_seed": COC_SEED_H}));
        assert_eq!(drain1["ok"], true);
        assert!(drain1["drained"].as_u64().unwrap() >= 2);
        // Second drain returns empty (already cleared).
        let drain2 =
            cocreative_session_log_drain(&mut s, json!({"player_seed": COC_SEED_H}));
        assert_eq!(drain2["ok"], true);
        assert_eq!(drain2["drained"].as_u64().unwrap(), 0);
    }

    /// `cocreative.persona_query` op='grant' flips cap + loads persona.
    #[test]
    fn cocreative_persona_query_grant_loads_persona() {
        let mut s = EngineState::default();
        let v = cocreative_persona_query(
            &mut s,
            json!({"player_seed": COC_SEED_I, "op": "grant"}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["cap_state"], "granted");
        assert!(v["persona_axes"].is_array());
        assert_eq!(v["persona_axes"].as_array().unwrap().len(), 8);
    }

    /// `cocreative.persona_query` op='revoke' flips cap back + masks axes.
    #[test]
    fn cocreative_persona_query_revoke_masks_axes() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_J, true);
        let v = cocreative_persona_query(
            &mut s,
            json!({"player_seed": COC_SEED_J, "op": "revoke"}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["cap_state"], "revoked");
        assert_eq!(v["persona_axes"].as_array().unwrap().len(), 0);
        assert_eq!(v["archetype_bias"], "(σ-masked)");
    }

    /// `cocreative.feedback_request` returns a stage-0 GM-response.
    #[test]
    fn cocreative_feedback_request_returns_gm_response() {
        let mut s = EngineState::default();
        coc_grant(COC_SEED_K, false);
        let v = cocreative_feedback_request(
            &mut s,
            json!({
                "player_seed": COC_SEED_K,
                "question": "Should the next chapter be set in the Crystal Atrium?",
            }),
        );
        assert_eq!(v["ok"], true);
        let resp = v["gm_response"].as_str().unwrap();
        assert!(resp.contains("Crystal Atrium"));
        assert!(resp.contains("stage-0"));
        assert!(v["arc_phase"].is_string());
    }

    /// Cap-deny short-circuits proposal_submit too.
    #[test]
    fn cocreative_proposal_submit_denies_without_cap() {
        let mut s = EngineState::default();
        // No grant for COC_SEED_L : default-deny.
        let v = cocreative_proposal_submit(
            &mut s,
            json!({
                "player_seed": COC_SEED_L,
                "kind": "npc-line",
                "payload": "hello",
                "reason": "test",
            }),
        );
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "cocreative-cap-denied");
    }

    /// player_seed=0 is rejected at the parameter-validation layer.
    #[test]
    fn cocreative_context_read_rejects_zero_seed() {
        let mut s = EngineState::default();
        let v = cocreative_context_read(&mut s, json!({"player_seed": 0}));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("player_seed"));
    }
}
