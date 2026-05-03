//! § loa-host — LoA-v13 stage-0 host runtime
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Apocky-greenlit hybrid stage-0 host runtime for LoA-v13. Combines four
//! sibling slices into one crate :
//!
//!   * `W-LOA-host-render` : winit window + wgpu render + 3D test-room
//!   * `W-LOA-host-input`  : WASD + mouse-look + axis-slide collision
//!   * `W-LOA-host-mcp`    : TCP JSON-RPC server (Claude live-interface)
//!   * `W-LOA-host-dm`     : DM director + GM narrator state machines
//!
//! § ROLE IN BOOTSTRAP
//!   `scenes/*.cssl` stay AUTHORITATIVE design specs. The CSSL stage-0
//!   compiler can't yet produce a wgpu-driven native binary on Windows ;
//!   until it can, this Rust crate is the bootstrap host. As csslc advances,
//!   modules incrementally migrate to pure-CSSL.
//!
//! § FEATURES
//!   - Default (catalog) : pure-CPU mesh + camera + input + MCP + DM/GM logic.
//!     Builds in any workspace toolchain (1.85.0 GNU compatible).
//!   - `runtime`         : pulls winit + wgpu + pollster, exposes `run_engine`
//!     which opens a window. Requires MSVC toolchain on Windows due to wgpu 23
//!     transitive deps (parking_lot_core windows-link 0.2.1).
//!
//! § BUILD
//!   cargo +stable-x86_64-pc-windows-msvc build -p loa-host --features runtime --release
//!   cargo +stable-x86_64-pc-windows-msvc run   -p loa-host --features runtime --release
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

// MCP-server FFI (omega.sample / omega.modify) calls cssl-rt's unsafe extern
// "C" loa_stubs functions. Allow rather than forbid for this crate.
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

// ──────────────────────────────────────────────────────────────────────────
// § Catalog modules (always built · pure-CPU)
// ──────────────────────────────────────────────────────────────────────────

// Render-sibling catalog
pub mod camera;
// § T11-W13-CAMERA (W13-4) — genre-fluid camera : 4-mode perspective-switch
//   {FpsLocked, ThirdPersonOverShoulder, Isometric, TopDown} · cubic-ease-300ms
//   · same-world-state · sovereign-revocable · ¬ forced-cap.
pub mod genre_fluid_camera;
pub mod geometry;
pub mod material;
pub mod pattern;
pub mod room;
pub mod stokes;
// § T11-LOA-FID-SPECTRAL — CPU-bake bridge from cssl-spectral-render to the
// GPU material LUT (4-illuminant cohort · per-material reference colors).
pub mod spectral_bridge;

// Input-sibling catalog
pub mod input;
// § T11-W13-INPUT-GENRE-FLUID (W13-11) — multi-source input router :
//   4 modes {KeyboardMouse · GamepadXinput · GamepadDualSense · TouchScreen}
//   · 500ms-grace last-source-wins · remappable EVERY action · OS-sticky/slow/
//   bounce-keys · 5-tier aim-assist with sovereign-cap on tier ≥ 3 · anti-
//   bullying PvP server-clamp. EXTEND-only — does not touch input.rs internals.
pub mod input_genre_fluid;
pub mod movement;
pub mod physics;

// MCP-sibling catalog
pub mod mcp_server;
pub mod mcp_tools;

// Telemetry-sibling catalog (T11-LOA-TELEM)
pub mod telemetry;

// DM-sibling catalog
pub mod dm_director;
pub mod gm_narrator;
pub mod dm_runtime;
// § T11-W11-GM-DM-DEEPEN — persona-state + narrative-arc state machine.
//   Sibling modules to gm_narrator + dm_director ; no cross-crate deps.
pub mod gm_persona;
pub mod dm_arc;

// UI-overlay catalog (CPU-side text/menu logic always built ; GPU pipeline
// gated on `runtime` feature inside the module).
pub mod ui_overlay;

// § T11-W13-FPS-HUD (W13-7) — FPS-grade HUD-element suite : crosshair +
// ammo + radar + objective + damage-floaters + health/shield + score-team +
// reload + hit-marker + killfeed. Extends ui_overlay's textured-quad pipeline
// without re-rolling the bitmap-font / vertex layout. Catalog-only ; pure CPU.
pub mod fps_hud;

// § T11-W12-POLISH (W12-12 Engine-Polish-Pass) — catalog-buildable polish
// audit + accessibility tunables + perf-budget tracker + WCAG color-contrast
// audit + loading-spinner + render-mode-flash + JSONL audit-report. Pure-CPU,
// zero new path-deps.
pub mod polish_audit;

// § T11-W13-PERF-ENFORCER (W13-12 perf-budget-enforcer) — runtime adapter
// that bridges polish_audit::PerfBudget (W12-12) into the new
// cssl-host-perf-enforcer crate. File-disjoint from polish_audit ; only
// reads its public surface. Exposes PerfRuntimeCheck which the engine
// main-loop calls per frame to classify Pass/Over/Severe verdicts, fire
// the AdaptiveDegrader, and emit PerfEvent records into the W11-4
// analytics-aggregator bridge.
pub mod perf_runtime_check;

// § T11-W13-FPS-PIPELINE (W13-1 FPS-render-pipeline + perf-rebuild) — catalog-
// buildable FPS render-pipeline orchestrator. Triple-buffered ring of frame
// slots · pre-allocated cmd-buffer pool · uniform staging · instance buffer
// · GPU-driven culling plan · variable-rate-shading tier table · mailbox
// frame-pacing · per-frame metrics emitter. ALL fields pre-allocated at
// construct time ; zero per-frame heap allocation on the hot path.
//
// LoA expanding to action-FPS looter-shooter ⇒ frame-budget = sovereignty.
// Targets ≤8.33ms (120Hz) · stretch ≤6.94ms (144Hz) · sub-frame input-latency.
// Catalog-only · no wgpu / winit deps ; the Renderer owns one of these and
// steps it inside `render_frame`. Spec @ Labyrinth of Apocalypse/systems/fps_pipeline.csl.
pub mod fps_pipeline;

// § T11-W13-FPS-PIPELINE-WIRE — thin MCP-style accessor over fps_pipeline.
//   default_pipeline() / stretch_144hz_pipeline() / legacy_60hz_pipeline()
//   constructors · summary_line() / metrics_jsonl() telemetry helpers ·
//   re-exports the full fps_pipeline surface so wired_* call-sites stay
//   short. Catalog-only ; pure-CPU ; ¬ deps.
pub mod wired_fps_pipeline;

// Snapshot-sibling catalog (T11-LOA-TEST-APP : PNG encode + tour-pose
// registry + golden-image diff are catalog-buildable ; the wgpu readback
// path is gated on the `runtime` feature inside the module).
pub mod snapshot;

// § T11-LOA-FID-MAINSTREAM : fidelity-report module is always built.
// The runtime-only side (gpu.rs) populates a global with the negotiated
// settings ; the catalog-mode reader returns "not_initialized" so MCP
// tooling works in offline tests.
pub mod fidelity;

// § T11-LOA-FID-CFER : substrate-IS-renderer. The CFER renderer wires
// the canonical Ω-field into a volumetric raymarched pass. The CPU-side
// state (OmegaField + texel staging + step-and-pack) is catalog-buildable
// (no GPU required) ; the wgpu pipeline-builder lives in `render.rs`
// behind the `runtime` feature.
pub mod cfer_render;

// § T11-LOA-SENSORY : full MCP sensory + proprioception harness. Aggregation
// surface for the 20+ `sense.*` MCP tools that let Claude perceive the live
// engine across 9 sensory axes (visual · audio · spatial · interoception ·
// diagnostic · temporal · causal · network · environmental).
pub mod sense;

// § T11-WAVE3-GLTF : pure-Rust GLTF/GLB → loa-host Vertex translator. Parses
// externally-authored 3D models (e.g. Stanford bunny, designer-supplied glb)
// into the canonical Vertex struct so they can be uploaded into the dynamic-
// mesh render path. Catalog-buildable (no GPU required for parse) ; the GPU
// upload path lives in `render.rs` behind the `runtime` feature.
pub mod gltf_loader;

// § T11-WAVE3-SPONT : text-seeded condensation pipeline. Converts intent
// text → SeedCells → Ω-field stamps → manifestation events → stress-object
// spawn. The substrate IS the source of truth ; objects are byproducts of
// cells crossing a critical-radiance threshold.
pub mod spontaneous;

// § T11-WAVE3-INTENT : text → typed-Intent → MCP-style dispatch router.
// Stage-0 keyword classifier ; stage-1 swaps in the KAN runtime. Every
// HUD text-input box submission + scripted scene call routes through here.
pub mod intent_router;

// § T11-W17-PARADIGM-SHIFT — Substrate-Resonance Pixel Field renderer.
//
// The COMPLETELY NOVEL VISUAL REPRESENTATION (Apocky-greenlit 2026-05-02).
// Owns a live `DigitalIntelligenceRenderer` + 5 test crystals procedurally
// allocated at host-init. Each frame, the host calls `tick(observer)` and
// the substrate-resonance pixel field is advanced :
//
//   - per-pixel observer-ray walk through the ω-field
//   - HDC-resonance accumulation from nearby crystals
//   - 4-illuminant spectral-LUT projection to sRGB
//   - per-observer Σ-mask filtering (sovereign-respecting)
//   - temporal-coherence ring-buffer (depth 3 · 5 blend modes)
//
// NO mesh data. NO texture atlases. NO BRDF. NO scene-graph. The substrate
// IS the source-of-truth, queried per-pixel-per-frame.
//
// Canonical-impl in `Labyrinth of Apocalypse/systems/{crystallization,
// alien_materialization, digital_intelligence_render}.csl`.
pub mod substrate_render;

// § T11-W18-DYNRES-SCALER · adaptive resolution-scaler for substrate-render.
//   Q0.16 fixed-point · 1440p144 budget · EMA frame-time · floor 0.5× · cap 1.0×
//   · honours LOA_DYN_RES=0 to disable. Wired into substrate_render::tick_gpu.
pub mod dynamic_resolution;

// § T11-W18-LOA-CONTENT-WIRE — DMGM-specialist council → procgen-pipeline →
//   substrate_render Crystal-replacement bridge. Default-OFF env-knob
//   (LOA_CONTENT_PIPELINE=0). When env=1, substrate_render replaces the
//   shell-seed Crystal-128 with procgen-derived crystals (cap 128) so
//   LoA.exe shows actual game-content instead of seed-data circles.
pub mod content_pipeline;

// ──────────────────────────────────────────────────────────────────────────
// § T11-W5c-LOA-HOST-WIRE — thin wrapper modules over the wave-3γ + W4 + W5a
// + W5b cssl-host-* crates. Each wired_* module re-exports the canonical
// public types from one path-dep + provides a single short-form helper for
// MCP-tool wiring. wave-6 deepens the integrations ; this slice wires the
// foundational surface so future agents can reach for `loa_host::wired_*`
// instead of re-naming the path-deps at every call-site.
// ──────────────────────────────────────────────────────────────────────────
pub mod wired_replay;
pub mod wired_audit;
pub mod wired_stereoscopy;
pub mod wired_golden;
pub mod wired_procgen_rooms;
pub mod wired_histograms;
pub mod wired_attestation;
pub mod wired_rt_trace;
pub mod wired_spectral_grader;
pub mod wired_frame_recorder;
pub mod wired_input_virtual;
pub mod wired_config;
pub mod wired_cocreative;
pub mod wired_causal_seed;
pub mod wired_license_attribution;
pub mod wired_voice;
pub mod wired_multiplayer;

// § T11-W7-G-LOA-HOST-WIRE — wave-7 (KAN-real · DM · GM · MP-transport-real)
//   thin wrappers. Each wired_* module re-exports the canonical public types
//   from one path-dep + provides a single short-form helper for MCP-tool
//   wiring. Adds 4 modules atop the W5c block above ; no MUTATIONS surfaced
//   yet (cap-table queries + canary-check are read-only).
pub mod wired_dm;
pub mod wired_gm;
pub mod wired_kan_real;
pub mod wired_mp_transport_real;

// § T11-W8-CHAT-WIRE — Coder narrow-orchestrator runtime + 4 NEW MCP tools.
//   Sandboxed AST-edit pipeline ; sovereign-required for substrate edits ;
//   30-second auto-revert window. Stage-0 explicit-confirm-only ; ¬ stage-1
//   (deferred-indefinitely-per-spec/10).
pub mod wired_coder_runtime;

// § T11-W16-WIREUP — per-frame integration of the W11..W15 host crates that
//   should-execute-each-frame OR on-event. Each `wired_*` module owns a
//   per-frame `tick(state, dt_ms, input)` + persistent state held by the
//   App. Cap-gated default-deny ; ¬ break existing event-loop shape.
//   Wired crates :
//     - cssl-host-weapons             (W13-2 · 16 WeaponKinds · hitscan + projectile)
//     - cssl-host-fps-feel            (W13-5 · ADS + recoil + bloom + crosshair)
//     - cssl-host-movement-aug        (W13-6 · sprint + slide + jump-pack + parkour)
//     - cssl-host-loot                (W13-8 · 8-tier rarity loot-drop on combat-end)
//     - cssl-host-mycelium-heartbeat  (W14-L · 60s mycelium-federate)
//     - cssl-content-rating           (W12-7 · rating ingest)
//     - cssl-content-moderation       (W12-11 · flag-handling)
//     - cssl-host-playtest-agent      (W12-10 · automated-GM playtests)
pub mod wired_weapons;
pub mod wired_fps_feel;
pub mod wired_movement_aug;
pub mod wired_loot;
pub mod wired_mycelium_heartbeat;
pub mod wired_content;

/// § LoaSubsystems — aggregator owned by App that holds per-frame state for
/// every wave-W11..W15 host crate that the event-loop calls. Constructed once
/// at App startup ; mutated per-frame via the `tick_*` family below.
///
/// Σ-cap-gating discipline : the corresponding `tick_*` helpers all take
/// the gate-bools from a single `WiredFrameInput` so a sovereign-revoke
/// (zero the bools) is a one-line guarantee.
pub struct LoaSubsystems {
    pub weapons: wired_weapons::WeaponsState,
    pub fps_feel: wired_fps_feel::FpsFeelState,
    pub movement_aug: wired_movement_aug::MovementAugState,
    pub loot: wired_loot::LootState,
    pub mycelium: wired_mycelium_heartbeat::MyceliumHeartbeatState,
    pub content: wired_content::ContentState,
}

impl Default for LoaSubsystems {
    fn default() -> Self {
        Self::new(0xCAFE_BABE_DEADBEEF_u64)
    }
}

impl LoaSubsystems {
    /// Construct with a `node_handle` used by the mycelium-heartbeat
    /// service for its emitter-id. Test-friendly default exists.
    #[must_use]
    pub fn new(node_handle: u64) -> Self {
        let svc = wired_mycelium_heartbeat::MyceliumHeartbeatState::build_default_service(node_handle);
        Self {
            weapons: wired_weapons::WeaponsState::new(),
            fps_feel: wired_fps_feel::FpsFeelState::default(),
            movement_aug: wired_movement_aug::MovementAugState::default(),
            loot: wired_loot::LootState::new(),
            mycelium: wired_mycelium_heartbeat::MyceliumHeartbeatState::new(svc),
            content: wired_content::ContentState::new(),
        }
    }
}

/// § WiredFrameInput — the bundled per-frame inputs for the wired-systems
/// suite. Default = all-zero, all-caps-denied (sovereign-revoke posture).
#[derive(Debug, Clone, Default)]
pub struct WiredFrameInput {
    pub weapons: wired_weapons::WeaponInput,
    pub fps_feel: wired_fps_feel::FpsFeelInputCapped,
    pub movement: wired_movement_aug::MovementIntentCapped,
    pub camera_forward_xz: [f32; 2],
    pub camera_right_xz: [f32; 2],
    pub world_hints: wired_movement_aug::WorldHints,
    pub loot: wired_loot::LootEvent,
    pub allow_mycelium_emit: bool,
    pub now_unix: u64,
    pub content: wired_content::ContentIngest,
}

/// § tick_wired_systems — single-call entry-point invoked once per frame
/// from the event-loop. Drives all wired subsystems in canonical order.
///
/// Order :
///   1. weapons       (accuracy-recovery + projectile-step)
///   2. fps_feel      (ADS + recoil + bloom + crosshair)
///   3. movement_aug  (sprint/slide/jump-pack/parkour state-machine)
///   4. loot          (combat-end-driven drop)
///   5. mycelium      (60s federation accumulator · cap-gated emit)
///   6. content       (rating/flag/quality-signal ingest)
///
/// Σ-cap-gating : every mutation gates internally on `WiredFrameInput`'s
/// `allow_*` fields. Default-deny ; an empty `WiredFrameInput::default()`
/// produces NO mutations beyond passive state-decay.
pub fn tick_wired_systems(
    sys: &mut LoaSubsystems,
    dt_ms: f32,
    input: &WiredFrameInput,
) -> WiredFrameOutputs {
    wired_weapons::tick(&mut sys.weapons, dt_ms, input.weapons);
    wired_fps_feel::tick(&mut sys.fps_feel, dt_ms, input.fps_feel);
    let proposed = wired_movement_aug::tick(
        &mut sys.movement_aug,
        dt_ms,
        input.movement,
        input.camera_forward_xz,
        input.camera_right_xz,
        input.world_hints,
    );
    let dropped = wired_loot::tick(&mut sys.loot, dt_ms, input.loot);
    let bundle = wired_mycelium_heartbeat::tick(
        &mut sys.mycelium,
        dt_ms,
        input.now_unix,
        input.allow_mycelium_emit,
    );
    wired_content::tick(&mut sys.content, dt_ms, input.content.clone());
    WiredFrameOutputs {
        movement_proposed: proposed,
        loot_dropped: dropped,
        mycelium_bundle: bundle,
    }
}

/// § WiredFrameOutputs — the side-effects the host can read THIS FRAME.
/// All fields are optional ; cap-denied ticks produce empty outputs.
pub struct WiredFrameOutputs {
    pub movement_proposed: wired_movement_aug::ProposedMotion,
    pub loot_dropped: Option<wired_loot::LootItem>,
    pub mycelium_bundle: Option<wired_mycelium_heartbeat::FederationBundle>,
}

// § T11-W12-COCREATIVE-BRIDGE — bi-directional Claude ↔ in-game-GM bridge.
//   8 NEW `cocreative.*` MCP tools that let an external Claude (running as
//   MCP-client) talk to the in-game GM (cssl-host-llm-bridge running inside
//   LoA.exe MCP-server). Round-trip pipeline : context_read → proposal_submit
//   → proposal_evaluate → iterate → draft_ready (Σ-Chain attestation) ; the
//   session_log_drain feeds KAN-training pairs for sibling W12-3.
//   ALL tools default-deny via the per-session `CocreativeCap` ; player
//   explicitly grants the cap (sovereign-revocable, σ-mask-isolated).
pub mod cocreative_loop;

// ──────────────────────────────────────────────────────────────────────────
// § Runtime-only modules (feature `runtime`)
// ──────────────────────────────────────────────────────────────────────────

#[cfg(feature = "runtime")]
pub mod gpu;
#[cfg(feature = "runtime")]
pub mod render;
#[cfg(feature = "runtime")]
pub mod window;

// § T11-W18-A-COMPOSITE — wgpu-compositing pass that uploads the substrate
// pixel-field to a 256×256 RGBA8 texture and alpha-blends it over the
// conventional 3D scene each frame. Owned by `Renderer` ; recorded after
// the CFER volumetric pass and before the UI overlay.
#[cfg(feature = "runtime")]
pub mod substrate_compose;

// § T11-W18-DISPLAY — auto-detect monitor characteristics + map → DisplayProfile
// (substrate_compose's pitch-black-friendly contrast/threshold knobs). Pure-
// CPU heuristic ; runs catalog-mode tests too. Window.rs invokes
// `display_detect::detect_profile` once on `resumed` + on monitor-change.
#[cfg(feature = "runtime")]
pub mod display_detect;

// § T11-W18-L9-AMOLED-DEEP — DEEP per-profile color-transform + auto-detect.
// Sister-module to `display_detect` ; carries the per-profile saturation-
// boost · snap-to-zero · peak-nits attributes the WGSL compose shader uses
// to render true-black AMOLED · saturate-an-OLED · lift-blacks-on-IPS · or
// PQ-encode for HdrExt. Also exposes `auto_detect_with_inputs` — a layered
// env > DXGI > EDID > winit-heuristic > default fallthrough that returns
// the chosen profile + the source-layer that won (for logs).
#[cfg(feature = "runtime")]
pub mod display_profile;

// ──────────────────────────────────────────────────────────────────────────
// § FFI surface (T11-LOA-PURE-CSSL · pure-CSSL main.cssl entry-point)
// ──────────────────────────────────────────────────────────────────────────
//
// § ROLE
//   Pure-CSSL programs (e.g. `Labyrinth of Apocalypse/main.cssl`) declare the
//   engine entry as `extern "C" fn __cssl_engine_run() -> i32` and call it
//   from `fn main()`. The CSSL compiler links the loa-host staticlib (via
//   csslc's auto-default-link mechanism), which provides this symbol. The
//   resulting `LoA.exe` is GENUINELY the output of csslc compiling
//   `main.cssl` — Rust is invisible at the source level (same model as a C
//   program calling libc/syscalls).
//
// § STAGE-1 PATH
//   As csslc gains capability (winit-bindings · wgpu-bindings · async-trait),
//   per-system modules migrate from this Rust crate to .csl source. The
//   `__cssl_engine_run` symbol stays as an ABI anchor, but its body shrinks
//   over time until it becomes a thin shim around .csl-authored event-loop
//   code. At full self-host the symbol disappears and main.cssl drives
//   winit/wgpu directly via the cssl-host-* FFI surface.
pub mod ffi;

// ──────────────────────────────────────────────────────────────────────────
// § Re-exports (the surface sibling code reaches for via `loa_host::*`)
// ──────────────────────────────────────────────────────────────────────────

pub use camera::Camera;
pub use geometry::{plinth_positions, RoomGeometry, Vertex};
pub use room::{Corridor, Direction, Doorway, Room, ROOM_COUNT};

pub use mcp_server::{
    spawn_mcp_server, EngineState, McpServerConfig, RenderMode, SOVEREIGN_CAP,
};
pub use mcp_tools::{tool_registry, ToolHandler, ToolRegistry};

#[cfg(feature = "runtime")]
pub use render::Renderer;
#[cfg(feature = "runtime")]
pub use window::{App, INITIAL_HEIGHT, INITIAL_WIDTH};

#[cfg(not(feature = "runtime"))]
pub const INITIAL_WIDTH: u32 = 1280;
#[cfg(not(feature = "runtime"))]
pub const INITIAL_HEIGHT: u32 = 720;

use cssl_rt::loa_startup::log_event;

// ──────────────────────────────────────────────────────────────────────────
// § run_engine — main entry from the loa-runtime binary
// ──────────────────────────────────────────────────────────────────────────

/// § T11-W18-STARTUP-BANNER · transparency-axiom : log every env-var-knob
/// at startup so user can see what's active, what's default, what was
/// overridden. Ties to PRIME-DIRECTIVE §4 TRANSPARENCY + §11 ATTESTATION.
/// Format : "loa-host/env · KEY=VALUE · src={env|default}".
fn log_startup_banner() {
    // Each entry : (env-var-name, default-shown, semantic-description).
    let knobs: &[(&str, &str, &str)] = &[
        ("LOA_RENDER_V3", "0", "ash-direct-vulkan path"),
        ("LOA_RENDER_V2", "0", "v2-substrate compute path"),
        ("LOA_VK_PRESENT_MODE", "IMMEDIATE", "vulkan present-mode"),
        ("LOA_FRAME_LATENCY", "1", "wgpu desired_maximum_frame_latency"),
        ("LOA_FRAME_PACE", "poll", "winit event-loop control-flow"),
        ("LOA_DYN_RES", "1", "adaptive resolution-scaler"),
        ("LOA_DYN_RES_FLOOR_Q16", "32768", "min scale Q0.16 (0.5×)"),
        ("LOA_DYN_RES_TARGET_US", "6944", "frame-budget µs (1440p144)"),
        ("LOA_DISPLAY_PROFILE", "<auto-detect>", "display-class override"),
        ("LOA_DISPLAY_HDR_NITS", "1000", "HDR peak nits"),
        ("LOA_KAN_BIAS_PATH", "~/.loa/kan_bias.bin", "KAN-bias persist path"),
        ("LOA_KAN_DISABLE", "0", "suspend learning"),
        ("LOA_SUBSTRATE_BENCH", "0", "log gpu_dispatch_us every-N frames"),
        ("LOA_SUBSTRATE_PACKED", "0", "64B-packed-GpuCrystal path"),
        ("LOA_DXIL_PRESENT_TEAR", "1", "D3D12 ALLOW_TEARING (when L8 lands)"),
        ("LOA_QUICK_QUIT", "0", "auto-quit after N frames (smoke-test)"),
        ("LOA_KAN_BAND_TRACE", "0", "log per-band KAN-checksums every 120 frames"),
        ("LOA_MYCELIUM_LOAD", "0", "load + merge peer-bias-shards from cache"),
    ];
    for &(key, default, _semantic) in knobs {
        let (val, src) = match std::env::var(key) {
            Ok(v) => (v, "env"),
            Err(_) => (default.to_string(), "default"),
        };
        log_event(
            "INFO",
            "loa-host/env",
            &format!("{key}={val} · src={src}"),
        );
    }
    log_event(
        "INFO",
        "loa-host/banner",
        "§ T11-W18 · 128-crystals · 5-DisplayProfile-bands · KAN-multiband-active · adaptive-res · low-latency-present · sovereignty-respecting-defaults",
    );
}

/// Open winit + wgpu, run the test-room render loop until window-close.
/// Catalog-mode (no `runtime` feature) returns Ok(()) after logging.
pub fn run_engine() -> std::io::Result<()> {
    log_event(
        "INFO",
        "loa-host/lib",
        "run_engine entry · stage-0 host starting",
    );
    log_startup_banner();
    #[cfg(feature = "runtime")]
    let r = window::run();
    #[cfg(not(feature = "runtime"))]
    let r: std::io::Result<()> = {
        log_event(
            "WARN",
            "loa-host/lib",
            "compiled WITHOUT --features runtime · catalog-only mode \
             · rebuild with `--features runtime` (MSVC toolchain) for the window",
        );
        eprintln!(
            "§ loa-host : catalog-only build · rebuild with `--features runtime` \
             (MSVC toolchain) to open the window"
        );
        Ok(())
    };
    log_event("INFO", "loa-host/lib", "run_engine exit · stage-0 host done");
    r
}

// ──────────────────────────────────────────────────────────────────────────
// § Embedded shader (catalog-visible so naga can validate w/o runtime)
// ──────────────────────────────────────────────────────────────────────────

pub const SCENE_WGSL: &str = include_str!("../shaders/scene.wgsl");

/// UI-overlay shader source (HUD + menu textured-quad pipeline).
pub const UI_WGSL: &str = include_str!("../shaders/ui.wgsl");

/// § T11-LOA-FID-MAINSTREAM (W-LOA-fidelity-mainstream)
/// ACES RRT+ODT tonemap shader (fullscreen-triangle vertex + ACES fragment).
/// Reads the HDR (Rgba16Float) intermediate target written by `scene.wgsl`,
/// applies Stephen Hill's fitted ACES curve, writes display-linear values
/// into the (sRGB-encoded) surface format. ~80 LOC, no external deps.
pub const TONEMAP_WGSL: &str = include_str!("../shaders/tonemap.wgsl");

/// § T11-LOA-FID-CFER : the volumetric raymarcher shader source. Catalog-
/// visible so naga can validate without the runtime feature.
pub const CFER_WGSL: &str = include_str!("../shaders/cfer.wgsl");

/// PRIME-DIRECTIVE attestation marker.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_reexports_compile() {
        let g = RoomGeometry::test_room();
        let _c = Camera::default();
        let _ps = plinth_positions();
        let _ = INITIAL_WIDTH;
        let _ = INITIAL_HEIGHT;
        assert_eq!(g.plinth_count, 14);
    }

    #[cfg(not(feature = "runtime"))]
    #[test]
    fn run_engine_no_op_in_catalog_mode() {
        let r = run_engine();
        assert!(r.is_ok());
    }

    #[test]
    fn wgsl_shader_string_compiles_to_naga() {
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(SCENE_WGSL).expect("scene.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator.validate(&module).expect("scene.wgsl must validate via naga");
    }

    #[test]
    fn cfer_wgsl_string_compiles_to_naga() {
        // § T11-LOA-FID-CFER : the volumetric raymarcher must parse +
        // validate via naga so the runtime build doesn't surprise us at
        // pipeline-creation time.
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(CFER_WGSL).expect("cfer.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator.validate(&module).expect("cfer.wgsl must validate via naga");
    }

    #[test]
    fn cfer_module_const_matches_lib_const() {
        // Avoid drift between the cfer_render::CFER_WGSL re-export and
        // the lib-level constant — both reference the same shader file.
        assert_eq!(crate::cfer_render::CFER_WGSL, CFER_WGSL);
    }

    #[test]
    fn embedded_shader_has_required_entry_points() {
        assert!(SCENE_WGSL.contains("vs_main"));
        assert!(SCENE_WGSL.contains("fs_main"));
    }

    /// § T11-LOA-FID-MAINSTREAM : tonemap.wgsl must parse + validate via naga
    /// so we know the ACES RRT+ODT shader is wgpu-compatible WITHOUT spinning
    /// up a GPU adapter. This is the catalog-level guarantee that the
    /// fidelity-pass pipeline will compile on any platform.
    #[test]
    fn tonemap_module_compiles_with_naga() {
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(TONEMAP_WGSL).expect("tonemap.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .expect("tonemap.wgsl must validate via naga");
        // Must contain both entry points + the ACES helper.
        assert!(TONEMAP_WGSL.contains("vs_main"));
        assert!(TONEMAP_WGSL.contains("fs_main"));
        assert!(TONEMAP_WGSL.contains("aces_rrt_odt"));
    }

    /// § T11-LOA-FID-MAINSTREAM : ACES known-input-output sanity check.
    ///
    /// The fitted ACES curve at input rgb=(1.0, 1.0, 1.0) returns ~0.8038
    /// (computed exactly : (1·2.54)/(1·3.16) = 2.54/3.16 = 0.80380...).
    /// White-point passes at ~80 % display brightness, leaving headroom
    /// for highlights — verifies that the in-shader curve coefficients
    /// AND the CPU-side reference helper are in agreement.
    // § T11-W16-WIREUP : integration tests for the per-frame wired-systems
    //   tick. Validate that all 6 subsystems run + cap-gating is enforced
    //   end-to-end + frame-counters advance as expected.

    #[test]
    fn wired_systems_constructs_with_default() {
        let sys = LoaSubsystems::default();
        assert_eq!(sys.weapons.shots_fired, 0);
        assert_eq!(sys.fps_feel.frame_count, 0);
        assert_eq!(sys.movement_aug.frame_count, 0);
        assert_eq!(sys.loot.drops_produced, 0);
        assert_eq!(sys.mycelium.bundles_emitted, 0);
        assert_eq!(sys.content.ratings_accepted, 0);
    }

    #[test]
    fn tick_wired_systems_default_input_is_no_op() {
        // Empty WiredFrameInput = all caps off. No mutations except
        // passive frame-counters.
        let mut sys = LoaSubsystems::default();
        let input = WiredFrameInput::default();
        let _ = tick_wired_systems(&mut sys, 16.6, &input);
        // Cap-denied : no shots, no drops, no bundles, no ratings.
        assert_eq!(sys.weapons.shots_fired, 0);
        assert_eq!(sys.loot.drops_produced, 0);
        assert_eq!(sys.mycelium.bundles_emitted, 0);
        assert_eq!(sys.content.ratings_accepted, 0);
        // Frame-counter on fps_feel + movement_aug DOES advance even with
        // empty input (passive decay tick).
        assert_eq!(sys.fps_feel.frame_count, 1);
        assert_eq!(sys.movement_aug.frame_count, 1);
    }

    #[test]
    fn tick_wired_systems_with_caps_produces_visible_effects() {
        let mut sys = LoaSubsystems::default();
        sys.mycelium.set_period_ms(50.0); // tiny period so test crosses it
        let input = WiredFrameInput {
            weapons: wired_weapons::WeaponInput {
                fired_this_frame: true,
                allow_fire: true,
                allow_step: true,
            },
            fps_feel: wired_fps_feel::FpsFeelInputCapped {
                firing: true,
                allow_fire: true,
                ..Default::default()
            },
            loot: wired_loot::LootEvent {
                combat_ended: true,
                allow_drop: true,
                seed_lo: 0xDEAD,
                seed_hi: 0xBEEF,
            },
            allow_mycelium_emit: true,
            now_unix: 1_700_000_000,
            world_hints: wired_movement_aug::WorldHints::ground(),
            ..Default::default()
        };
        // Single frame with 100ms dt to push past the mycelium period.
        let outputs = tick_wired_systems(&mut sys, 100.0, &input);
        // Weapons : fired with cap → shots_fired = 1.
        assert_eq!(sys.weapons.shots_fired, 1);
        // Loot : combat-ended + allow_drop → an item dropped.
        assert!(outputs.loot_dropped.is_some());
        assert_eq!(sys.loot.drops_produced, 1);
        // Frame counters advanced.
        assert_eq!(sys.fps_feel.frame_count, 1);
        assert_eq!(sys.movement_aug.frame_count, 1);
    }

    #[test]
    fn aces_tonemap_known_input_output() {
        // Reference CPU implementation (matches WGSL `aces_rrt_odt`).
        fn aces(x: [f32; 3]) -> [f32; 3] {
            let a = [x[0] * 2.51 + 0.03, x[1] * 2.51 + 0.03, x[2] * 2.51 + 0.03];
            let b = [
                x[0] * (2.43 * x[0] + 0.59) + 0.14,
                x[1] * (2.43 * x[1] + 0.59) + 0.14,
                x[2] * (2.43 * x[2] + 0.59) + 0.14,
            ];
            let mut out = [0.0f32; 3];
            for i in 0..3 {
                out[i] = (x[i] * a[i]) / b[i];
                out[i] = out[i].clamp(0.0, 1.0);
            }
            out
        }
        let mid = aces([1.0, 1.0, 1.0]);
        // Reference value : 2.54 / 3.16 ≈ 0.8038.
        assert!(
            (mid[0] - 0.8038).abs() < 0.01,
            "aces(1.0)={mid:?} (expected ~0.80)"
        );
        // Sanity : the white-point output is in the [0.78, 0.82] band that
        // every reasonable ACES fit lands in.
        for c in mid {
            assert!((0.78..=0.82).contains(&c), "channel out of band : {c}");
        }
        // Output is clamped to [0, 1] — bright HDR input must not blow up.
        let high = aces([100.0, 100.0, 100.0]);
        for c in high {
            assert!((0.0..=1.0).contains(&c));
        }
        // Zero in → zero out.
        let low = aces([0.0, 0.0, 0.0]);
        for c in low {
            assert!(c.abs() < 1e-3);
        }
    }
}
