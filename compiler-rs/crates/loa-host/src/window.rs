//! § window — winit Event Loop driver for the LoA-v13 host runtime.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-PURE-CSSL (W-LOA-pure-cssl-engine) — Wires the four sibling
//! slices into one unified application :
//!
//!   - render  : winit window + wgpu surface + scene render
//!   - input   : WASD + mouse-look + Tab pause + F-keys + Esc menu
//!   - physics : axis-slide collision (capsule vs AABBs)
//!   - DM      : director + GM narrator ticked per-frame
//!   - MCP     : TCP JSON-RPC server bound to localhost:3001
//!
//! The application opens at the primary monitor's native resolution in
//! BORDERLESS-FULLSCREEN mode by default, captures the cursor + keyboard
//! immediately, and runs the navigatable test-room until the user closes
//! the window via menu→Quit or Alt-F4.
//!
//! § ENV CONTROLS
//!   `CSSL_LOA_WINDOW=windowed`    → 2560×1440 windowed mode (no fullscreen)
//!   `CSSL_LOA_WINDOW=borderless`  → borderless-fullscreen (default)
//!   `CSSL_LOA_WINDOW=exclusive`   → exclusive-fullscreen at native res
//!   `CSSL_LOA_NO_GRAB=1`          → don't grab the cursor (debugging)
//!   `CSSL_LOA_NO_MCP=1`           → skip MCP server bind (offline mode)
//!
//! § INPUT MAPPING
//!   WASD      → walk + strafe
//!   Space/Ctrl→ vertical (fly-mode)
//!   LShift    → sprint (2× speed)
//!   Mouse     → free-look (FPS-style)
//!   Tab       → toggle pause + release/grab cursor
//!   F1-F10    → render-mode select (0..9)
//!   F11       → toggle borderless fullscreen
//!   Esc       → toggle menu (NOT exit ; menu's Quit is the exit path)
//!   `         → toggle debug overlay
//!
//! § PRIME-DIRECTIVE
//!   Esc opens the menu rather than exiting because the user retains
//!   sovereign control over session-end. Cursor capture is RELEASED
//!   when the menu is open OR when window-focus is lost — the user is
//!   never trapped.

#![allow(clippy::too_many_lines)] // event-loop dispatch is intentionally one big match
#![allow(clippy::cast_precision_loss)] // u32→f32 dimensions/timing
#![allow(clippy::collapsible_match)]
#![allow(clippy::single_match)]

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use cssl_rt::loa_startup::log_event;

use crate::camera::Camera as RenderCamera;
use crate::dm_director::{DmEvent, PlayerState};
use crate::dm_runtime::DmRuntime;
use crate::gpu::GpuContext;
use crate::input::{InputState, RawEvent, VirtualKey};
use crate::mcp_server::{
    spawn_mcp_server, CameraState as McpCamera, EngineState, McpServerConfig,
    RenderMode as McpRenderMode, Vec3 as McpVec3,
};
use crate::movement::Camera as PlayerCamera;
use crate::physics::RoomCollider;
use crate::render::Renderer;
use crate::telemetry as telem;
use crate::ui_overlay::{HudContext, MenuAction, MenuState};

/// Initial windowed-mode dimensions (only used when `CSSL_LOA_WINDOW=windowed`).
/// 2560×1440 = 1440p WQHD ; downsteps gracefully on smaller displays via the
/// surface-resize handler.
pub const INITIAL_WIDTH: u32 = 2560;
pub const INITIAL_HEIGHT: u32 = 1440;

// ──────────────────────────────────────────────────────────────────────────
// § Window-mode selection — env-driven
// ──────────────────────────────────────────────────────────────────────────

/// Run-time selection of the window's display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowMode {
    /// Borderless fullscreen at primary monitor's native resolution. Default.
    Borderless,
    /// Windowed at INITIAL_WIDTH × INITIAL_HEIGHT.
    Windowed,
    /// Exclusive fullscreen at primary monitor's native resolution.
    Exclusive,
}

impl WindowMode {
    /// Parse the `CSSL_LOA_WINDOW` env-var. Default = Borderless.
    fn from_env() -> Self {
        match std::env::var("CSSL_LOA_WINDOW")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "windowed" => Self::Windowed,
            "exclusive" => Self::Exclusive,
            _ => Self::Borderless,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § App — owns every subsystem
// ──────────────────────────────────────────────────────────────────────────

/// Application state — owned by the winit event loop. Each subsystem is
/// wired through `RedrawRequested` per-frame, with shared `EngineState`
/// keeping MCP tools synchronized with the live runtime.
pub struct App {
    // § Subsystems
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    /// Player-side camera (carries pos/yaw/pitch in [f32;3] form).
    pub player: PlayerCamera,
    /// Render-side camera (Vec3 form for matrices). Synced from `player`
    /// each frame.
    pub render_camera: RenderCamera,
    /// Per-frame input accumulator.
    pub input: InputState,
    /// Axis-slide collider against the test-room walls + plinths.
    pub collider: RoomCollider,
    /// DM director + GM narrator state-machine.
    pub dm: DmRuntime,
    /// Shared engine-state mirror for MCP tool dispatch.
    pub engine_state: Arc<Mutex<EngineState>>,

    // § Frame loop bookkeeping
    /// Wall-clock at the start of the previous redraw — drives dt.
    last_frame_at: Option<Instant>,
    /// Monotonic frame counter.
    pub frame_count: u64,
    /// User-toggled pause state. When true, `dm.tick` + `propose_motion`
    /// are skipped so the world holds still while the user reads the
    /// menu / overlay.
    pub paused: bool,
    /// Menu-open state (Esc toggles). When true, cursor is released.
    pub menu_open: bool,
    /// Cursor-grab desire — recomputed per frame based on focus + menu.
    cursor_currently_grabbed: bool,
    /// Window currently in borderless-fullscreen.
    fullscreen_now: bool,
    /// Menu state-machine (T11-LOA-HUD : owns selection/screen/help-scroll).
    pub menu: MenuState,
    /// Recent DM/GM event-text shown on the BOTTOM-LEFT HUD line.
    pub recent_event: String,
    /// Smoothed FPS estimate displayed in the TOP-LEFT HUD.
    pub fps_smoothed: f32,
    /// Initial mode selected by env-var.
    initial_mode: WindowMode,
    /// Cached window-flag : has the focus event for this window fired at
    /// least once? Used to decide whether to grab on resumed.
    has_been_focused: bool,
    /// Cached for tests + headless mode : did we ever bring up the GPU?
    pub gpu_alive: bool,

    // § MCP server handle (spawned in resumed)
    mcp_handle: Option<JoinHandle<()>>,
    mcp_port: Option<u16>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Construct an App with default subsystems but no window/GPU yet
    /// (those come up on `resumed`).
    #[must_use]
    pub fn new() -> Self {
        let engine_state = Arc::new(Mutex::new(EngineState::default()));
        Self {
            window: None,
            gpu: None,
            renderer: None,
            player: PlayerCamera::new(),
            render_camera: RenderCamera::default(),
            input: InputState::new(),
            // T11-LOA-ROOMS : multi-room collider (TestRoom hub + 4 satellites).
            collider: RoomCollider::full_world(),
            dm: DmRuntime::new(),
            engine_state,
            last_frame_at: None,
            frame_count: 0,
            paused: false,
            menu_open: false,
            cursor_currently_grabbed: false,
            fullscreen_now: false,
            menu: MenuState::default(),
            recent_event: String::new(),
            fps_smoothed: 0.0,
            initial_mode: WindowMode::from_env(),
            has_been_focused: false,
            gpu_alive: false,
            mcp_handle: None,
            mcp_port: None,
        }
    }

    /// Returns the current player-camera (read-only).
    #[must_use]
    pub fn camera(&self) -> PlayerCamera {
        self.player
    }

    /// Returns the bound MCP port (if the server is up).
    #[must_use]
    pub fn mcp_port(&self) -> Option<u16> {
        self.mcp_port
    }

    /// Sync the PlayerCamera ([f32;3] form) into the RenderCamera (Vec3 form).
    /// Called once per frame after motion is committed.
    fn sync_render_camera(&mut self) {
        self.render_camera.position = glam::Vec3::new(
            self.player.pos[0],
            self.player.pos[1],
            self.player.pos[2],
        );
        self.render_camera.yaw = self.player.yaw;
        self.render_camera.pitch = self.player.pitch;
    }

    /// Update the shared `EngineState` mirror with this frame's data so
    /// MCP `engine.state` / `camera.get` tools reflect live values.
    fn sync_engine_state(&self) {
        let Ok(mut g) = self.engine_state.lock() else {
            // Poisoned mutex — log + continue. The MCP server's poison-tolerant
            // dispatch will still work once a future call recovers.
            log_event(
                "WARN",
                "loa-host/window",
                "engine-state mutex poisoned · skipping per-frame sync",
            );
            return;
        };
        g.frame_count = self.frame_count;
        g.paused = self.paused;
        g.camera = McpCamera {
            pos: McpVec3::new(self.player.pos[0], self.player.pos[1], self.player.pos[2]),
            yaw: self.player.yaw,
            pitch: self.player.pitch,
        };
        // Render-mode mirror : the input-state holds 0..9 ; convert to MCP enum.
        if let Some(m) = McpRenderMode::from_u8(self.input.render_mode) {
            g.render_mode = m;
        }
        // DM intensity mirror : the DM-runtime returns f32 0..=1 ; quantize
        // to u8 0..3 for the MCP wire-format.
        let intensity = self.dm.intensity();
        let quantized = if intensity < 0.25 {
            0
        } else if intensity < 0.5 {
            1
        } else if intensity < 0.75 {
            2
        } else {
            3
        };
        g.dm.intensity = quantized;
    }

    /// Apply a winit KeyEvent → input.handle_event(RawEvent::Key{...}).
    /// Returns true if the key was consumed by special handlers (F11 toggles
    /// fullscreen, Esc toggles menu) to suppress double-handling.
    fn route_key(&mut self, event_loop: &ActiveEventLoop, key: KeyEvent) {
        let pressed = key.state == ElementState::Pressed;
        let physical = key.physical_key;

        // § Special-handler keys : these don't translate to a movement axis,
        // they trigger window-side actions. Suppress further routing.
        if pressed {
            if let PhysicalKey::Code(code) = physical {
                match code {
                    KeyCode::F11 => {
                        self.toggle_fullscreen();
                        return;
                    }
                    KeyCode::Escape | KeyCode::Tab => {
                        // If menu is on a sub-screen (Help), Esc/Tab pops
                        // that screen off rather than closing the menu
                        // entirely. Otherwise toggles open/closed.
                        if self.menu.open
                            && self.menu.screen != crate::ui_overlay::MenuScreen::Main
                        {
                            let _ = self.menu.back();
                        } else {
                            self.toggle_menu();
                        }
                        // Don't fall through — Esc/Tab open/close menu only.
                        return;
                    }
                    _ => {}
                }
            }
        }

        // § Convert PhysicalKey → VirtualKey (input crate's enum).
        let vk = match physical {
            PhysicalKey::Code(KeyCode::KeyW) => VirtualKey::W,
            PhysicalKey::Code(KeyCode::KeyA) => VirtualKey::A,
            PhysicalKey::Code(KeyCode::KeyS) => VirtualKey::S,
            PhysicalKey::Code(KeyCode::KeyD) => VirtualKey::D,
            PhysicalKey::Code(KeyCode::Space) => VirtualKey::Space,
            PhysicalKey::Code(KeyCode::ControlLeft) => VirtualKey::LCtrl,
            PhysicalKey::Code(KeyCode::ShiftLeft) => VirtualKey::LShift,
            PhysicalKey::Code(KeyCode::Backquote) => VirtualKey::Backtick,
            // § T11-LOA-FID-STOKES : `P` cycles polarization-view diagnostic.
            PhysicalKey::Code(KeyCode::KeyP) => VirtualKey::P,
            PhysicalKey::Code(KeyCode::F1) => VirtualKey::F1,
            PhysicalKey::Code(KeyCode::F2) => VirtualKey::F2,
            PhysicalKey::Code(KeyCode::F3) => VirtualKey::F3,
            PhysicalKey::Code(KeyCode::F4) => VirtualKey::F4,
            PhysicalKey::Code(KeyCode::F5) => VirtualKey::F5,
            PhysicalKey::Code(KeyCode::F6) => VirtualKey::F6,
            PhysicalKey::Code(KeyCode::F7) => VirtualKey::F7,
            PhysicalKey::Code(KeyCode::F8) => VirtualKey::F8,
            PhysicalKey::Code(KeyCode::F9) => VirtualKey::F9,
            PhysicalKey::Code(KeyCode::F10) => VirtualKey::F10,
            // Menu navigation keys
            PhysicalKey::Code(KeyCode::ArrowUp) => VirtualKey::ArrowUp,
            PhysicalKey::Code(KeyCode::ArrowDown) => VirtualKey::ArrowDown,
            PhysicalKey::Code(KeyCode::ArrowLeft) => VirtualKey::ArrowLeft,
            PhysicalKey::Code(KeyCode::ArrowRight) => VirtualKey::ArrowRight,
            PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                VirtualKey::Enter
            }
            _ => VirtualKey::Other,
        };

        // Tab handling : we want pause-toggle behavior PLUS cursor release.
        // The InputState already toggles paused on Tab-press ; we observe
        // the state-after for the cursor-grab side-effect.
        let was_paused = self.paused;
        self.input.handle_event(&RawEvent::Key { vk, pressed });

        // Sync our pause-state mirror from the input-state's toggle.
        self.paused = self.input.paused;
        if self.paused != was_paused {
            // Pause-state changed : refresh cursor grab.
            self.refresh_cursor_grab();
        }
        let _ = event_loop;
    }

    /// Toggle borderless-fullscreen. F11 trigger. Logs the new state.
    fn toggle_fullscreen(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if self.fullscreen_now {
            window.set_fullscreen(None);
            self.fullscreen_now = false;
            log_event("INFO", "loa-host/window", "fullscreen · OFF");
        } else {
            window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            self.fullscreen_now = true;
            log_event("INFO", "loa-host/window", "fullscreen · BORDERLESS");
        }
    }

    /// Toggle menu-open state. Tab/Esc trigger. Releases cursor when menu is up.
    /// Delegates the state-machine to `MenuState::toggle` and mirrors the
    /// resulting `open` flag onto `self.menu_open` for legacy consumers.
    fn toggle_menu(&mut self) {
        self.menu.toggle();
        self.menu_open = self.menu.open;
        log_event(
            "INFO",
            "loa-host/window",
            if self.menu_open {
                "menu · OPEN · cursor released"
            } else {
                "menu · CLOSED · cursor grabbed"
            },
        );
        self.refresh_cursor_grab();
    }

    /// Recompute cursor-grab state based on focus + menu + pause.
    /// Cursor is grabbed iff window has focus AND menu is closed AND not paused
    /// AND the user hasn't opted out via `CSSL_LOA_NO_GRAB`.
    fn refresh_cursor_grab(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if std::env::var_os("CSSL_LOA_NO_GRAB").is_some() {
            // User opted out — never grab.
            if self.cursor_currently_grabbed {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
                self.cursor_currently_grabbed = false;
            }
            return;
        }
        let want_grab = self.has_been_focused && !self.menu_open && !self.paused;
        if want_grab && !self.cursor_currently_grabbed {
            // Try Confined first (Wayland / Linux preferred), fall back to
            // Locked (Windows / macOS). Either is acceptable — both prevent
            // the cursor from leaving the window during FPS-style mouse-look.
            if window.set_cursor_grab(CursorGrabMode::Confined).is_err() {
                let _ = window.set_cursor_grab(CursorGrabMode::Locked);
            }
            window.set_cursor_visible(false);
            self.cursor_currently_grabbed = true;
        } else if !want_grab && self.cursor_currently_grabbed {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.cursor_currently_grabbed = false;
        }
    }

    /// Spawn the MCP TCP server on localhost:3001 (or the env-configured
    /// port). Stores the JoinHandle + actual bound port for diagnostics.
    fn spawn_mcp(&mut self) {
        if std::env::var_os("CSSL_LOA_NO_MCP").is_some() {
            log_event(
                "INFO",
                "loa-host/window",
                "MCP · server skipped ($CSSL_LOA_NO_MCP set)",
            );
            return;
        }
        let cfg = McpServerConfig::default();
        match spawn_mcp_server(cfg, self.engine_state.clone()) {
            Ok((handle, port)) => {
                self.mcp_handle = Some(handle);
                self.mcp_port = Some(port);
                log_event(
                    "INFO",
                    "loa-host/window",
                    &format!("MCP · server listening on localhost:{port}"),
                );
            }
            Err(e) => {
                log_event(
                    "WARN",
                    "loa-host/window",
                    &format!("MCP · bind failed : {e} · continuing without server"),
                );
            }
        }
    }

    /// Compose the per-frame DM input from current player state. The
    /// player_state shape is sibling-defined ; we synthesize plausible
    /// defaults for stage-0 (full HP/stamina, no combat, recent rest).
    /// Future slices wire real combat + HP + stamina from gameplay code.
    fn current_player_state(&self) -> PlayerState {
        PlayerState {
            hp_deficit: 0.0,
            stamina_deficit: 0.0,
            recent_combat_density: 0.0,
            rest_signals: 1.0,
        }
    }

    /// Drain pending snapshot requests from EngineState into the renderer.
    /// Pulled into its own helper so `run_one_frame` stays readable.
    fn drain_snapshot_queue(&mut self) {
        // Take pending requests + tour-progress out of the shared state
        // mutex quickly, hand off to renderer for later processing.
        let drained: Vec<std::path::PathBuf> = match self.engine_state.lock() {
            Ok(mut g) => {
                let q = std::mem::take(&mut g.snapshot_queue);
                q.into_iter().map(|r| r.path).collect()
            }
            Err(poisoned) => {
                let mut g = poisoned.into_inner();
                let q = std::mem::take(&mut g.snapshot_queue);
                q.into_iter().map(|r| r.path).collect()
            }
        };
        // Hand the FIRST one to the renderer ; subsequent ones are
        // re-queued for later frames so each frame writes one PNG.
        // (Multi-snapshot-per-frame would force expensive reconfigures.)
        if let Some(renderer) = self.renderer.as_mut() {
            let mut iter = drained.into_iter();
            if let Some(first) = iter.next() {
                renderer.request_snapshot(first);
            }
            // Re-enqueue the rest.
            let rest: Vec<_> = iter.collect();
            if !rest.is_empty() {
                if let Ok(mut g) = self.engine_state.lock() {
                    for p in rest {
                        g.snapshot_queue.insert(
                            0,
                            crate::mcp_server::SnapshotRequest { path: p },
                        );
                    }
                }
            }
        }
    }

    /// Build the HUD context the renderer reads each frame to populate the
    /// 4-corner text + crosshair.
    fn build_hud_context(&self) -> HudContext {
        // § T11-LOA-RICH-RENDER : facing-info — pick the wall the camera is
        // most-aimed-at by yaw quadrant (-π..π · 4 buckets).
        let pi = std::f32::consts::PI;
        let yaw = self.player.yaw.rem_euclid(2.0 * pi);
        let facing_pattern_name = if yaw < 0.25 * pi || yaw >= 1.75 * pi {
            // Looking +X (east)
            "QR-Code"
        } else if yaw < 0.75 * pi {
            // Looking +Z (north)
            "Macbeth-ColorChart"
        } else if yaw < 1.25 * pi {
            // Looking -X (west)
            "EAN-13-Barcode"
        } else {
            // Looking -Z (south)
            "Snellen-EyeChart"
        };

        // Material under crosshair : fall back to "(none)" if no plinth in
        // a forward 5m cone. Stage-0 keeps it simple.
        let mat = String::from("(none)");

        // Pull the renderer's frame-time histogram if available.
        let frame_times_ms = match &self.renderer {
            Some(r) => r.frame_times_ms,
            None => [16.7; 60],
        };

        // § T11-LOA-TEST-APP : snapshot status pulled from renderer +
        // engine-state for HUD. snapshot_pending = a request is queued for
        // the very next frame ; tour_progress = MCP tour in flight ;
        // snapshot_count = total session snapshots taken.
        let snapshot_pending = self
            .renderer
            .as_ref()
            .map(|r| r.snapshot_pending.is_some())
            .unwrap_or(false);
        let (tour_progress, snapshot_count) = match self.engine_state.lock() {
            Ok(g) => (g.tour_progress, g.snapshot_count),
            Err(_) => (None, 0),
        };

        // § T11-LOA-ROOMS : compute the current room (or corridor label) from
        // the camera's eye-position. Updated every frame so the HUD reflects
        // the player's location in real-time.
        let current_room = crate::room::room_label_at(self.player.pos).to_string();

        HudContext {
            frame: self.frame_count,
            fps: self.fps_smoothed,
            camera_pos: self.player.pos,
            yaw: self.player.yaw,
            pitch: self.player.pitch,
            render_mode: self.input.render_mode,
            dm_phase_label: self.dm.state().label(),
            dm_tension: self.dm.intensity(),
            recent_event: self.recent_event.clone(),
            mcp_port: self.mcp_port,
            fullscreen: self.fullscreen_now,
            facing_material: mat,
            facing_pattern: facing_pattern_name.to_string(),
            frame_times_ms,
            snapshot_pending,
            tour_progress,
            snapshot_count,
            current_room,
        }
    }

    /// Translate a `MenuAction` returned from `MenuState::activate` into a
    /// host-level effect.
    fn handle_menu_action(&mut self, action: MenuAction, event_loop: &ActiveEventLoop) {
        match action {
            MenuAction::None => {}
            MenuAction::Resume => {
                // Already closed by activate() ; just refresh cursor grab.
                self.menu_open = self.menu.open;
                self.paused = self.menu.open;
                self.refresh_cursor_grab();
            }
            MenuAction::CycleRenderMode => {
                self.input.render_mode = self.menu.render_mode;
                log_event(
                    "INFO",
                    "loa-host/window",
                    &format!("menu · render-mode → {}", self.input.render_mode),
                );
            }
            MenuAction::ToggleFullscreen => {
                self.toggle_fullscreen();
                self.menu.fullscreen = self.fullscreen_now;
            }
            MenuAction::Quit => {
                log_event(
                    "INFO",
                    "loa-host/window",
                    "menu · QUIT selected · exiting cleanly",
                );
                event_loop.exit();
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // § Build window attributes per the env-selected mode.
        let mut attrs = Window::default_attributes()
            .with_title("Labyrinth of Apocalypse v13")
            .with_visible(true);

        match self.initial_mode {
            WindowMode::Borderless => {
                attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
                self.fullscreen_now = true;
            }
            WindowMode::Windowed => {
                attrs = attrs.with_inner_size(PhysicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT));
                self.fullscreen_now = false;
            }
            WindowMode::Exclusive => {
                // Exclusive-fullscreen requires a VideoMode handle ; on
                // platforms without one we fall back to Borderless.
                attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
                self.fullscreen_now = true;
            }
        }

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log_event(
                    "ERROR",
                    "loa-host/window",
                    &format!("create_window failed : {e} · exiting cleanly"),
                );
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        log_event(
            "INFO",
            "loa-host/window",
            &format!(
                "window-created · {}x{} · mode={:?}",
                size.width, size.height, self.initial_mode
            ),
        );

        // § Try to bring up the GPU. On failure we keep the window open + log.
        if let Some(gpu) = GpuContext::new(window.clone()) {
            let renderer = Renderer::new(&gpu);
            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
            self.gpu_alive = true;
        } else {
            log_event(
                "WARN",
                "loa-host/window",
                "no GPU context available · window will be blank",
            );
        }

        self.window = Some(window);

        // § Spawn MCP server. Idempotent across resumed events (we check
        // mcp_handle.is_some() inside).
        if self.mcp_handle.is_none() {
            self.spawn_mcp();
        }

        // § Initial cursor state : not grabbed yet (we wait for Focused
        // event), but request hidden so the user knows the engine is alive.
        // Borderless-fullscreen tends to have focus on creation, but
        // `Focused(true)` may fire after `resumed` — refresh_cursor_grab
        // is the right place.
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log_event(
                    "INFO",
                    "loa-host/window",
                    "close-requested · exiting · clean",
                );
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.renderer.as_mut()) {
                    gpu.resize(size.width, size.height);
                    renderer.resize(gpu);
                }
            }

            WindowEvent::Focused(focused) => {
                if focused {
                    self.has_been_focused = true;
                    log_event("INFO", "loa-host/window", "focus · GAINED");
                } else {
                    log_event("INFO", "loa-host/window", "focus · LOST");
                }
                self.refresh_cursor_grab();
            }

            WindowEvent::KeyboardInput { event: key, .. } => {
                self.route_key(event_loop, key);
            }

            WindowEvent::RedrawRequested => {
                self.run_one_frame(event_loop);
            }

            // Cursor moved : we use DeviceEvent::MouseMotion for FPS-style
            // raw deltas, NOT WindowEvent::CursorMoved (which is absolute).
            // Discard cursor-moved events when grabbed.
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        // Only consume mouse-motion when cursor is grabbed (user is in
        // FPS-look mode). Otherwise discard so the menu's mouse pointer
        // works normally.
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.cursor_currently_grabbed && !self.menu_open {
                let (dx, dy) = delta;
                self.input.handle_event(&RawEvent::MouseMotion {
                    dx: dx as f32,
                    dy: dy as f32,
                });
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Per-frame system tick — the heart of the engine
// ──────────────────────────────────────────────────────────────────────────

impl App {
    /// Drive one full frame : input → camera → physics → DM → state-sync →
    /// render. Splits out from the WindowEvent::RedrawRequested arm so it's
    /// reachable from tests + future scripted-tick paths.
    fn run_one_frame(&mut self, event_loop: &ActiveEventLoop) {
        // § 1. dt (clamped to a sane range to survive debugger pauses).
        let now = Instant::now();
        let dt = if let Some(prev) = self.last_frame_at {
            let elapsed = now.duration_since(prev).as_secs_f32();
            // Clamp to [1ms, 100ms] : skips degenerate-long-pause scenarios
            // (debugger held for 5 minutes) without distorting the
            // simulation when held breakpoints resume.
            elapsed.clamp(0.001, 0.1)
        } else {
            1.0 / 60.0
        };
        self.last_frame_at = Some(now);

        // § 2. Drain input frame (zeros mouse-deltas + menu edges).
        let frame = self.input.consume_frame();

        // § 2a. Route menu nav-edges to MenuState while menu is open.
        // Edges fire ONCE per press ; consume_frame already cleared them.
        if self.menu.open {
            if frame.menu_up_pressed {
                self.menu.nav_up();
            }
            if frame.menu_down_pressed {
                self.menu.nav_down();
            }
            if frame.menu_left_pressed {
                self.menu.nav_left();
            }
            if frame.menu_right_pressed {
                self.menu.nav_right();
            }
            if frame.menu_enter_pressed {
                let action = self.menu.activate();
                self.handle_menu_action(action, event_loop);
            }
        }
        // Keep input.paused mirror in sync with menu_open : when the menu is
        // open, the input layer's `paused` reflects that. When menu closes,
        // we leave `paused` as-is (Tab no longer toggles it directly).
        self.input.paused = self.menu.open;
        self.menu_open = self.menu.open;
        self.paused = self.menu.open;
        // Sync menu's view of fullscreen + render_mode (so the menu UI draws
        // with current values).
        self.menu.fullscreen = self.fullscreen_now;
        self.menu.render_mode = self.input.render_mode;

        // § 3. Apply mouse-look (always — paused player can still look).
        if !self.paused {
            self.player.apply_look(&frame);
        }

        // § 4. Compute proposed motion (skip when paused or menu open).
        if !self.paused && !self.menu_open {
            let delta = self.player.propose_motion(&frame, dt);
            // § 5. Validate via axis-slide collision.
            let validated = self.collider.slide(self.player.pos, delta);
            // § 6. Commit the validated delta.
            self.player.commit_motion(validated);
        }

        // § 7. Drive the DM (skip when paused — world holds still).
        if !self.paused {
            let ps = self.current_player_state();
            if let Some(ev) = self.dm.tick(&ps, self.frame_count) {
                self.log_dm_event(&ev);
                self.recent_event = format!("{ev:?}");
            }
        }

        // § 7a. Update smoothed FPS for the HUD.
        let inst_fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        // EMA with 0.10 weight on new sample → ~10-frame settling.
        self.fps_smoothed = self.fps_smoothed * 0.9 + inst_fps * 0.1;

        // § 8. Sync render-camera + engine-state for MCP visibility.
        self.sync_render_camera();
        self.sync_engine_state();

        // § 8a. T11-LOA-TEST-APP : honor any camera teleport pushed by the
        // MCP `camera.set` / `render.tour` tools. The MCP tool has already
        // mutated EngineState.camera ; if it's diverged from our player
        // by more than a small threshold, snap the player there. (Normal
        // play is unaffected — divergence comes only from MCP teleports.)
        if let Ok(g) = self.engine_state.lock() {
            let cam = g.camera;
            let dx = cam.pos.x - self.player.pos[0];
            let dy = cam.pos.y - self.player.pos[1];
            let dz = cam.pos.z - self.player.pos[2];
            let dyaw = cam.yaw - self.player.yaw;
            let dpitch = cam.pitch - self.player.pitch;
            let dist_sq = dx * dx + dy * dy + dz * dz;
            // 0.01m² ≈ player won't notice ; below this it's organic motion.
            // Above, the gap is a teleport from MCP.
            if dist_sq > 0.01
                || dyaw.abs() > 0.001
                || dpitch.abs() > 0.001
            {
                self.player.pos = [cam.pos.x, cam.pos.y, cam.pos.z];
                self.player.yaw = cam.yaw;
                self.player.pitch = cam.pitch;
            }
        }
        // Re-sync render_camera after the teleport so this frame uses the
        // new pose (if any).
        self.sync_render_camera();

        // § 8b. Drain pending snapshot requests into the renderer.
        self.drain_snapshot_queue();

        // § 8c. Build the HUD context this frame.
        let hud = self.build_hud_context();

        // § 9. Render the frame.
        let frame_token = telem::global().frame_begin();
        if let (Some(gpu), Some(renderer), Some(window)) = (
            self.gpu.as_ref(),
            self.renderer.as_mut(),
            self.window.as_ref(),
        ) {
            match renderer.render_frame(gpu, &self.render_camera, window, &hud, &self.menu) {
                Ok(metrics) => {
                    telem::global()
                        .frame_end(frame_token, metrics.draw_calls, metrics.vertices);
                }
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    // Surface stale ; record the frame as "happened" (zero
                    // metrics) so the histogram still reflects wall-clock
                    // pacing rather than going silent during a resize.
                    telem::global().frame_end(frame_token, 0, 0);
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    log_event(
                        "ERROR",
                        "loa-host/render",
                        "surface OOM · exiting cleanly",
                    );
                    telem::global().frame_end(frame_token, 0, 0);
                    event_loop.exit();
                }
                Err(e) => {
                    log_event("ERROR", "loa-host/render", &format!("frame error : {e:?}"));
                    telem::global().frame_end(frame_token, 0, 0);
                }
            }
        } else {
            // No renderer ; still close out the frame so the sink's frame
            // counter advances at wall-clock pace (useful for headless tests).
            telem::global().frame_end(frame_token, 0, 0);
        }

        // § 10. Increment + heartbeat-log every 600 frames (~10s @ 60Hz).
        self.frame_count += 1;
        if self.frame_count % 600 == 0 {
            log_event(
                "INFO",
                "loa-host/window",
                &format!(
                    "heartbeat · frame={} · pos=({:.2},{:.2},{:.2}) · paused={} · menu={}",
                    self.frame_count,
                    self.player.pos[0],
                    self.player.pos[1],
                    self.player.pos[2],
                    self.paused,
                    self.menu_open
                ),
            );
        }
    }

    fn log_dm_event(&self, ev: &DmEvent) {
        log_event(
            "INFO",
            "loa-host/dm",
            &format!("event-proposed · {ev:?}"),
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § run — top-level entry from `run_engine`
// ──────────────────────────────────────────────────────────────────────────

/// Run the engine event loop. Blocks until the window is closed. On
/// platforms where no event loop / display is available, returns
/// `Ok(())` silently after logging the condition.
pub fn run() -> std::io::Result<()> {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            log_event(
                "WARN",
                "loa-host/window",
                &format!("EventLoop::new failed : {e} · running headless"),
            );
            return Ok(());
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    if let Err(e) = event_loop.run_app(&mut app) {
        log_event(
            "ERROR",
            "loa-host/window",
            &format!("event loop terminated abnormally : {e}"),
        );
        // Don't propagate the error — we want clean exit.
    }
    log_event("INFO", "loa-host/exit", "loop-exited · clean");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_default_constructs_subsystems() {
        let app = App::new();
        // Camera at room-origin facing +Z.
        assert_eq!(app.player.pos, [0.0, 1.55, 0.0]);
        assert!(!app.paused);
        assert!(!app.menu_open);
        assert_eq!(app.frame_count, 0);
        assert!(!app.gpu_alive);
        // EngineState mirror is constructed cleanly.
        assert!(app.engine_state.lock().is_ok());
    }

    #[test]
    fn window_mode_default_is_borderless() {
        std::env::remove_var("CSSL_LOA_WINDOW");
        assert_eq!(WindowMode::from_env(), WindowMode::Borderless);
    }

    #[test]
    fn window_mode_windowed_via_env() {
        std::env::set_var("CSSL_LOA_WINDOW", "windowed");
        assert_eq!(WindowMode::from_env(), WindowMode::Windowed);
        std::env::remove_var("CSSL_LOA_WINDOW");
    }

    #[test]
    fn sync_render_camera_mirrors_player() {
        let mut app = App::new();
        app.player.pos = [3.0, 1.55, -2.0];
        app.player.yaw = 0.5;
        app.player.pitch = -0.25;
        app.sync_render_camera();
        assert!((app.render_camera.position.x - 3.0).abs() < 1e-6);
        assert!((app.render_camera.position.z - (-2.0)).abs() < 1e-6);
        assert!((app.render_camera.yaw - 0.5).abs() < 1e-6);
        assert!((app.render_camera.pitch - (-0.25)).abs() < 1e-6);
    }

    #[test]
    fn sync_engine_state_writes_camera_pos() {
        let app = App::new();
        // Default app at (0, 1.55, 0) ; sync should copy.
        app.sync_engine_state();
        let g = app.engine_state.lock().unwrap();
        assert!((g.camera.pos.x - 0.0).abs() < 1e-6);
        assert!((g.camera.pos.y - 1.55).abs() < 1e-6);
        assert!((g.camera.pos.z - 0.0).abs() < 1e-6);
    }

    #[test]
    fn current_player_state_is_calm_default() {
        let app = App::new();
        let ps = app.current_player_state();
        assert_eq!(ps.hp_deficit, 0.0);
        assert_eq!(ps.stamina_deficit, 0.0);
        assert_eq!(ps.recent_combat_density, 0.0);
        assert_eq!(ps.rest_signals, 1.0);
    }
}
