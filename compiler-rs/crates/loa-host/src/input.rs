// § T11-LOA-HOST-2 (W-LOA-host-input) · input.rs ─────────────────────────
// Input capture for LoA-v13 host. Maps winit-event-SHAPED `RawEvent` enum
// to an `InputState` accumulator. Per-frame the host drains the accumulator
// via `consume_frame()` which returns an `InputFrame` (deltas + held-axes)
// and zeroes the mouse-deltas (axes stay held until key-up).
//
// § design-notes ───────────────────────────────────────────────────────
// The input layer is winit-event-shape-COMPATIBLE but DOES NOT depend on
// winit directly (see Cargo.toml comment). The render-sibling owns the
// winit dep ; the integration commit will wire :
//
//     fn adapt_winit_event(e: &winit::event::Event<()>) -> Option<RawEvent>
//
// matching one arm per variant we care about :
//   • Event::WindowEvent { event: WindowEvent::KeyboardInput { .. }, .. }
//       → RawEvent::Key { vk, pressed }
//   • Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. }
//       → RawEvent::MouseMotion { dx, dy }
//   • Event::WindowEvent { event: WindowEvent::CloseRequested, .. }
//       → RawEvent::CloseRequested
//
// § PRIME-DIRECTIVE ────────────────────────────────────────────────────
// Esc is HONORED : no override, no "are you sure" prompt — Esc sets
// `quit_requested=true` and the host MUST exit promptly. This respects
// user agency-axiom (consent=OS).

use cssl_rt::loa_startup::log_event;

/// Virtual-key enum. Shape-compatible with winit's logical-key set ; we name
/// only the keys the LoA-v13 host actually consumes. Adding a key requires :
///   1. add a variant here
///   2. add a match-arm in `InputState::handle_event`
///   3. (integration) add a winit→RawEvent translation arm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VirtualKey {
    // Movement
    W,
    A,
    S,
    D,
    Space,
    LCtrl,
    LShift, // sprint modifier
    // Modal
    Escape,
    Tab,
    Backtick,
    /// § T11-LOA-FID-STOKES : `P` cycles the polarization-view mode
    /// (Intensity → Q → U → V → DOP → Intensity). Persistent setting on the
    /// global atomic ; each press advances by one.
    P,
    // Render-mode select (10 modes per scenes/render_pipeline.cssl design).
    //
    // § T11-LOA-USERFIX : F1-F10 now apply IMMEDIATELY (no menu-Enter
    //   required). The host reads `render_mode_changed` once per frame in
    //   the InputFrame to push the new mode into the renderer's uniforms.
    //   F7-F10 are time-shared : F7 also runs the 5-tour suite ; F8 toggles
    //   video record ; F9 starts a burst ; F12 single screenshot. Render-
    //   mode 7 (Substrate) and 8 (SpectralKan) and 9 (Debug) keep their
    //   bindings · F7 advances render mode AND requests a tour-run
    //   (handled host-side via dedicated `tour_requested` edge).
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    // § T11-LOA-USERFIX : capture + atmospheric-toggle keys.
    //   F12 → single screenshot · F11 reserved (fullscreen toggle in window.rs).
    //   F9  → burst-of-10 · F8 → video-toggle · F7 → tour-run (5 tours).
    //   C   → CFER atmospheric toggle (intensity 0 ↔ default).
    F12,
    C,
    // Menu navigation (T11-LOA-HUD : MenuState consumer reads
    // `menu_*_pressed` edges on each frame's `consume_frame()`).
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
    // Catch-all : keys we received but don't consume (no-op match arm)
    Other,
}

/// Render-mode discriminant. 10 modes per `scenes/render_pipeline.cssl` design.
/// Stored in `InputState` ; the renderer reads it each frame to switch passes.
/// We do NOT switch render state here — that is render-sibling territory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RenderMode {
    Default = 0,
    Wireframe = 1,
    Normals = 2,
    Depth = 3,
    Albedo = 4,
    Lighting = 5,
    Compass = 6, // visualize 8-ray proprioception
    Substrate = 7, // ω-field visualization
    SpectralKan = 8,
    Debug = 9,
}

impl RenderMode {
    pub fn from_index(i: u8) -> Self {
        match i {
            0 => Self::Default,
            1 => Self::Wireframe,
            2 => Self::Normals,
            3 => Self::Depth,
            4 => Self::Albedo,
            5 => Self::Lighting,
            6 => Self::Compass,
            7 => Self::Substrate,
            8 => Self::SpectralKan,
            _ => Self::Debug,
        }
    }
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Raw event shape matching the winit::event::Event variants we consume.
/// Constructed by the winit-adapter (integration commit) or directly by
/// tests. Keeping the shape thin makes the adapter trivial.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RawEvent {
    /// Keyboard key state change (press or release).
    Key { vk: VirtualKey, pressed: bool },
    /// Raw mouse-motion delta from the mouse device. NOT cursor-position ;
    /// this is the relative delta winit reports via `DeviceEvent::MouseMotion`.
    MouseMotion { dx: f32, dy: f32 },
    /// Window-close request (Alt-F4 / titlebar-X). Treated identically to
    /// Esc by the host.
    CloseRequested,
}

/// Held-axis + accumulating deltas + modal toggles. The host updates this
/// from `RawEvent` stream and drains it once per frame via `consume_frame()`.
///
/// Movement axes are HELD : pressing W sets `forward=1.0` ; releasing W sets
/// `forward=0.0`. Releasing W while S is held leaves `forward=-1.0` (the most
/// recent axis-direction wins, then degrades to the still-held opposite).
/// We track WASD as four separate held-bools internally + recompute the
/// signed axis on each event.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // intentional input-state shape
pub struct InputState {
    // Public-API : what consumers read.
    pub forward: f32, // -1..1
    pub right: f32,   // -1..1
    pub up: f32,      // -1..1
    pub yaw_delta: f32,
    pub pitch_delta: f32,
    pub render_mode: u8, // 0..9
    pub paused: bool,
    pub debug_overlay: bool,
    pub quit_requested: bool,
    pub sprint: bool,
    // Menu-navigation press-edges. Set on key-DOWN ; consumed (zeroed) on
    // `consume_frame()`. The host's MenuState reads these once per frame.
    pub menu_up_pressed: bool,
    pub menu_down_pressed: bool,
    pub menu_left_pressed: bool,
    pub menu_right_pressed: bool,
    pub menu_enter_pressed: bool,
    // § T11-LOA-USERFIX : single-frame edges for capture + render-mode +
    //   CFER atmospheric toggle. All set on key-DOWN, drained by
    //   `consume_frame()`. The host's per-frame logic acts on each ;
    //   render_mode_changed propagates the new mode value into the renderer's
    //   uniforms · the capture edges feed snapshot/burst/video state machines.
    /// Set when an F1-F10 press changed `render_mode` THIS frame.
    pub render_mode_changed: bool,
    /// Set when F12 was pressed (single screenshot).
    pub screenshot_requested: bool,
    /// Set when F9 was pressed (start a 10-frame burst).
    pub burst_requested: bool,
    /// Set when F8 was pressed (toggle video record).
    pub video_toggle_requested: bool,
    /// Set when F7 was pressed (run all 5 tours).
    pub tour_requested: bool,
    /// Set when C was pressed (toggle CFER atmospheric pass).
    pub cfer_toggle_pressed: bool,
    // Internal : per-key held state for axis recomputation. Not part of the
    // public API but pub(crate) for unit-tests in this module.
    pub(crate) held_w: bool,
    pub(crate) held_a: bool,
    pub(crate) held_s: bool,
    pub(crate) held_d: bool,
    pub(crate) held_space: bool,
    pub(crate) held_lctrl: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        log_event(
            "INFO",
            "loa-host/input",
            "input-state-init · WASD + mouse-look + Esc + F1-F10 + Tab + backtick",
        );
        Self {
            forward: 0.0,
            right: 0.0,
            up: 0.0,
            yaw_delta: 0.0,
            pitch_delta: 0.0,
            render_mode: 0,
            paused: false,
            debug_overlay: false,
            quit_requested: false,
            sprint: false,
            menu_up_pressed: false,
            menu_down_pressed: false,
            menu_left_pressed: false,
            menu_right_pressed: false,
            menu_enter_pressed: false,
            render_mode_changed: false,
            screenshot_requested: false,
            burst_requested: false,
            video_toggle_requested: false,
            tour_requested: false,
            cfer_toggle_pressed: false,
            held_w: false,
            held_a: false,
            held_s: false,
            held_d: false,
            held_space: false,
            held_lctrl: false,
        }
    }

    /// Recompute signed axes from held-keys. Called after every key event.
    fn recompute_axes(&mut self) {
        self.forward = (self.held_w as i8 - self.held_s as i8) as f32;
        self.right = (self.held_d as i8 - self.held_a as i8) as f32;
        self.up = (self.held_space as i8 - self.held_lctrl as i8) as f32;
    }

    /// Apply a single raw event to the state. Idempotent for press-while-held
    /// (no double-counting). Mouse-deltas ACCUMULATE until `consume_frame()`.
    pub fn handle_event(&mut self, ev: &RawEvent) {
        match *ev {
            RawEvent::Key { vk, pressed } => self.handle_key(vk, pressed),
            RawEvent::MouseMotion { dx, dy } => {
                self.yaw_delta += dx;
                self.pitch_delta += dy;
            }
            RawEvent::CloseRequested => {
                self.quit_requested = true;
                log_event("INFO", "loa-host/input", "close-requested · honoring");
            }
        }
    }

    fn handle_key(&mut self, vk: VirtualKey, pressed: bool) {
        match vk {
            VirtualKey::W => {
                self.held_w = pressed;
                self.recompute_axes();
            }
            VirtualKey::A => {
                self.held_a = pressed;
                self.recompute_axes();
            }
            VirtualKey::S => {
                self.held_s = pressed;
                self.recompute_axes();
            }
            VirtualKey::D => {
                self.held_d = pressed;
                self.recompute_axes();
            }
            VirtualKey::Space => {
                self.held_space = pressed;
                self.recompute_axes();
            }
            VirtualKey::LCtrl => {
                self.held_lctrl = pressed;
                self.recompute_axes();
            }
            VirtualKey::LShift => {
                self.sprint = pressed;
            }
            VirtualKey::Escape => {
                if pressed {
                    self.quit_requested = true;
                    log_event("INFO", "loa-host/input", "esc-pressed · quit-requested");
                }
            }
            VirtualKey::Tab => {
                if pressed {
                    self.paused = !self.paused;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        if self.paused { "paused" } else { "resumed" },
                    );
                }
            }
            VirtualKey::Backtick => {
                if pressed {
                    self.debug_overlay = !self.debug_overlay;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        if self.debug_overlay {
                            "debug-overlay · ON"
                        } else {
                            "debug-overlay · OFF"
                        },
                    );
                }
            }
            VirtualKey::P => {
                // § T11-LOA-FID-STOKES : cycle polarization-view diagnostic
                // mode (Intensity → Q → U → V → DOP → Intensity).
                if pressed {
                    let new_mode = crate::ffi::cycle_polarization_view();
                    log_event(
                        "INFO",
                        "loa-host/input",
                        &format!(
                            "p-pressed · polarization-view → {} ({})",
                            new_mode,
                            crate::stokes::PolarizationView::from_u32(new_mode).name()
                        ),
                    );
                }
            }
            // § T11-LOA-USERFIX : F1-F6 set the render-mode AND set the
            //   `render_mode_changed` edge so the host applies it directly
            //   to the renderer this frame (no menu round-trip needed).
            //   F7-F10 still set their assigned modes but ALSO emit a
            //   capture/tour edge — they're double-bound for utility.
            VirtualKey::F1 => {
                if pressed && self.render_mode != 0 {
                    self.render_mode = 0;
                    self.render_mode_changed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F1 · render-mode → 0 Default (direct apply)",
                    );
                } else if pressed {
                    self.render_mode = 0;
                    self.render_mode_changed = true;
                }
            }
            VirtualKey::F2 => {
                if pressed {
                    self.render_mode = 1;
                    self.render_mode_changed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F2 · render-mode → 1 Wireframe/Albedo (direct apply)",
                    );
                }
            }
            VirtualKey::F3 => {
                if pressed {
                    self.render_mode = 2;
                    self.render_mode_changed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F3 · render-mode → 2 Depth/Normals (direct apply)",
                    );
                }
            }
            VirtualKey::F4 => {
                if pressed {
                    self.render_mode = 3;
                    self.render_mode_changed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F4 · render-mode → 3 Depth (direct apply)",
                    );
                }
            }
            VirtualKey::F5 => {
                if pressed {
                    self.render_mode = 4;
                    self.render_mode_changed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F5 · render-mode → 4 Albedo (direct apply)",
                    );
                }
            }
            VirtualKey::F6 => {
                if pressed {
                    self.render_mode = 5;
                    self.render_mode_changed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F6 · render-mode → 5 SDF (direct apply)",
                    );
                }
            }
            VirtualKey::F7 => {
                if pressed {
                    // Render-mode 6 (Compass / Steps) AND tour-request.
                    self.render_mode = 6;
                    self.render_mode_changed = true;
                    self.tour_requested = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F7 · render-mode → 6 Steps · tour-request fired",
                    );
                }
            }
            VirtualKey::F8 => {
                if pressed {
                    // Render-mode 7 (Substrate / WDistance) AND video-toggle.
                    self.render_mode = 7;
                    self.render_mode_changed = true;
                    self.video_toggle_requested = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F8 · render-mode → 7 WDistance · video-toggle fired",
                    );
                }
            }
            VirtualKey::F9 => {
                if pressed {
                    // Render-mode 8 (SpectralKan / Grid) AND burst-request.
                    self.render_mode = 8;
                    self.render_mode_changed = true;
                    self.burst_requested = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F9 · render-mode → 8 Grid · burst-request fired (10 frames)",
                    );
                }
            }
            VirtualKey::F10 => {
                if pressed {
                    self.render_mode = 9;
                    self.render_mode_changed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F10 · render-mode → 9 FieldVsAnalytic (direct apply)",
                    );
                }
            }
            VirtualKey::F12 => {
                if pressed {
                    self.screenshot_requested = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "F12 · screenshot-request fired",
                    );
                }
            }
            VirtualKey::C => {
                if pressed {
                    self.cfer_toggle_pressed = true;
                    log_event(
                        "INFO",
                        "loa-host/input",
                        "C · cfer-atmospheric-toggle fired",
                    );
                }
            }
            VirtualKey::ArrowUp => {
                if pressed {
                    self.menu_up_pressed = true;
                }
            }
            VirtualKey::ArrowDown => {
                if pressed {
                    self.menu_down_pressed = true;
                }
            }
            VirtualKey::ArrowLeft => {
                if pressed {
                    self.menu_left_pressed = true;
                }
            }
            VirtualKey::ArrowRight => {
                if pressed {
                    self.menu_right_pressed = true;
                }
            }
            VirtualKey::Enter => {
                if pressed {
                    self.menu_enter_pressed = true;
                }
            }
            VirtualKey::Other => {}
        }
    }

    /// Drain per-frame deltas into an `InputFrame` and zero the mouse-deltas.
    /// Held axes (forward/right/up) PERSIST across frames — only deltas reset.
    pub fn consume_frame(&mut self) -> InputFrame {
        let frame = InputFrame {
            forward: self.forward,
            right: self.right,
            up: self.up,
            yaw_delta: self.yaw_delta,
            pitch_delta: self.pitch_delta,
            sprint: self.sprint,
            render_mode: self.render_mode,
            render_mode_changed: self.render_mode_changed,
            paused: self.paused,
            debug_overlay: self.debug_overlay,
            quit_requested: self.quit_requested,
            menu_up_pressed: self.menu_up_pressed,
            menu_down_pressed: self.menu_down_pressed,
            menu_left_pressed: self.menu_left_pressed,
            menu_right_pressed: self.menu_right_pressed,
            menu_enter_pressed: self.menu_enter_pressed,
            screenshot_requested: self.screenshot_requested,
            burst_requested: self.burst_requested,
            video_toggle_requested: self.video_toggle_requested,
            tour_requested: self.tour_requested,
            cfer_toggle_pressed: self.cfer_toggle_pressed,
        };
        self.yaw_delta = 0.0;
        self.pitch_delta = 0.0;
        // Menu edges fire ONCE per press — clear after consume.
        self.menu_up_pressed = false;
        self.menu_down_pressed = false;
        self.menu_left_pressed = false;
        self.menu_right_pressed = false;
        self.menu_enter_pressed = false;
        // § T11-LOA-USERFIX : capture/render-mode edges also fire once
        //   per press — clear after consume so the host's per-frame
        //   handler sees each event exactly once.
        self.render_mode_changed = false;
        self.screenshot_requested = false;
        self.burst_requested = false;
        self.video_toggle_requested = false;
        self.tour_requested = false;
        self.cfer_toggle_pressed = false;
        frame
    }
}

/// Per-frame snapshot consumed by `Camera::apply_frame()`. Mouse-deltas
/// here are CUMULATIVE for the just-completed frame ; held axes are
/// instantaneous-at-frame-end.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // intentional per-frame snapshot shape
pub struct InputFrame {
    pub forward: f32,
    pub right: f32,
    pub up: f32,
    pub yaw_delta: f32,
    pub pitch_delta: f32,
    pub sprint: bool,
    pub render_mode: u8,
    /// § T11-LOA-USERFIX : true on the frame an F1-F10 key was pressed.
    /// Host reads this and applies the new render_mode to the renderer's
    /// uniforms immediately — no menu round-trip.
    pub render_mode_changed: bool,
    pub paused: bool,
    pub debug_overlay: bool,
    pub quit_requested: bool,
    pub menu_up_pressed: bool,
    pub menu_down_pressed: bool,
    pub menu_left_pressed: bool,
    pub menu_right_pressed: bool,
    pub menu_enter_pressed: bool,
    /// § T11-LOA-USERFIX : F12 single-screenshot edge.
    pub screenshot_requested: bool,
    /// § T11-LOA-USERFIX : F9 burst-of-10 edge.
    pub burst_requested: bool,
    /// § T11-LOA-USERFIX : F8 video-toggle edge.
    pub video_toggle_requested: bool,
    /// § T11-LOA-USERFIX : F7 5-tour-suite edge.
    pub tour_requested: bool,
    /// § T11-LOA-USERFIX : C cfer-atmospheric-toggle edge.
    pub cfer_toggle_pressed: bool,
}

impl Default for InputFrame {
    /// Zero-valued frame — used in tests + as a base for `..Default::default()`
    /// spread in literal constructors.
    fn default() -> Self {
        Self {
            forward: 0.0,
            right: 0.0,
            up: 0.0,
            yaw_delta: 0.0,
            pitch_delta: 0.0,
            sprint: false,
            render_mode: 0,
            render_mode_changed: false,
            paused: false,
            debug_overlay: false,
            quit_requested: false,
            menu_up_pressed: false,
            menu_down_pressed: false,
            menu_left_pressed: false,
            menu_right_pressed: false,
            menu_enter_pressed: false,
            screenshot_requested: false,
            burst_requested: false,
            video_toggle_requested: false,
            tour_requested: false,
            cfer_toggle_pressed: false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn input_state_zeros_on_new() {
        let s = InputState::new();
        assert_eq!(s.forward, 0.0);
        assert_eq!(s.right, 0.0);
        assert_eq!(s.up, 0.0);
        assert_eq!(s.yaw_delta, 0.0);
        assert_eq!(s.pitch_delta, 0.0);
        assert_eq!(s.render_mode, 0);
        assert!(!s.paused);
        assert!(!s.debug_overlay);
        assert!(!s.quit_requested);
        assert!(!s.sprint);
        // Internal held-bools all false too.
        assert!(!s.held_w);
        assert!(!s.held_a);
        assert!(!s.held_s);
        assert!(!s.held_d);
        assert!(!s.held_space);
        assert!(!s.held_lctrl);
    }

    #[test]
    fn wasd_press_sets_movement_axes() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::W,
            pressed: true,
        });
        assert_eq!(s.forward, 1.0);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::D,
            pressed: true,
        });
        assert_eq!(s.right, 1.0);
        // Pressing S while W held → cancels (forward = 1 - 1 = 0).
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::S,
            pressed: true,
        });
        assert_eq!(s.forward, 0.0);
        // Releasing W while S held → forward = -1.
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::W,
            pressed: false,
        });
        assert_eq!(s.forward, -1.0);
        // Up + down via Space/LCtrl
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Space,
            pressed: true,
        });
        assert_eq!(s.up, 1.0);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::LCtrl,
            pressed: true,
        });
        assert_eq!(s.up, 0.0);
    }

    #[test]
    fn mouse_delta_accumulates_into_yaw() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::MouseMotion { dx: 10.0, dy: 5.0 });
        s.handle_event(&RawEvent::MouseMotion { dx: 3.5, dy: -1.0 });
        // Pre-consume : accumulated.
        assert!((s.yaw_delta - 13.5).abs() < 1e-6);
        assert!((s.pitch_delta - 4.0).abs() < 1e-6);
        // Consume zeros mouse-deltas.
        let frame = s.consume_frame();
        assert!((frame.yaw_delta - 13.5).abs() < 1e-6);
        assert!((frame.pitch_delta - 4.0).abs() < 1e-6);
        assert_eq!(s.yaw_delta, 0.0);
        assert_eq!(s.pitch_delta, 0.0);
    }

    #[test]
    fn esc_sets_quit_requested() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Escape,
            pressed: true,
        });
        assert!(s.quit_requested);
    }

    #[test]
    fn close_requested_sets_quit() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::CloseRequested);
        assert!(s.quit_requested);
    }

    #[test]
    fn tab_toggles_pause() {
        let mut s = InputState::new();
        assert!(!s.paused);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Tab,
            pressed: true,
        });
        assert!(s.paused);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Tab,
            pressed: true,
        });
        assert!(!s.paused);
    }

    #[test]
    fn backtick_toggles_overlay() {
        let mut s = InputState::new();
        assert!(!s.debug_overlay);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Backtick,
            pressed: true,
        });
        assert!(s.debug_overlay);
    }

    #[test]
    fn f_keys_set_render_mode() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::F3,
            pressed: true,
        });
        assert_eq!(s.render_mode, 2);
        assert_eq!(RenderMode::from_index(s.render_mode), RenderMode::Normals);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::F10,
            pressed: true,
        });
        assert_eq!(s.render_mode, 9);
        assert_eq!(RenderMode::from_index(s.render_mode), RenderMode::Debug);
    }

    #[test]
    fn arrow_keys_latch_menu_press_edges() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::ArrowDown,
            pressed: true,
        });
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Enter,
            pressed: true,
        });
        // Latched
        assert!(s.menu_down_pressed);
        assert!(s.menu_enter_pressed);
        // consume_frame drains them
        let frame = s.consume_frame();
        assert!(frame.menu_down_pressed);
        assert!(frame.menu_enter_pressed);
        assert!(!s.menu_down_pressed);
        assert!(!s.menu_enter_pressed);
        // Releases don't re-latch
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::ArrowDown,
            pressed: false,
        });
        assert!(!s.menu_down_pressed);
    }

    // ── § T11-LOA-USERFIX : direct render-mode + capture-key tests ──

    #[test]
    fn f_key_press_emits_render_mode_changed_event() {
        // F1-F10 must set both render_mode and the edge-flag exactly
        // once per press, then the edge clears on consume_frame.
        let mut s = InputState::new();
        assert!(!s.render_mode_changed);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::F3,
            pressed: true,
        });
        assert_eq!(s.render_mode, 2);
        assert!(s.render_mode_changed);
        let frame = s.consume_frame();
        assert_eq!(frame.render_mode, 2);
        assert!(frame.render_mode_changed);
        // Edge cleared after consume.
        assert!(!s.render_mode_changed);
    }

    #[test]
    fn c_key_toggles_cfer_intensity_atomic() {
        // C-key sets the cfer_toggle_pressed edge ONCE per press. Two
        // separate presses fire the edge twice (host's logic flips a
        // persistent intensity-on bool each time).
        let mut s = InputState::new();
        assert!(!s.cfer_toggle_pressed);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::C,
            pressed: true,
        });
        assert!(s.cfer_toggle_pressed);
        let frame = s.consume_frame();
        assert!(frame.cfer_toggle_pressed);
        assert!(!s.cfer_toggle_pressed);
        // Re-press → fires again
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::C,
            pressed: true,
        });
        assert!(s.cfer_toggle_pressed);
    }

    #[test]
    fn f12_sets_screenshot_requested() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::F12,
            pressed: true,
        });
        assert!(s.screenshot_requested);
        let frame = s.consume_frame();
        assert!(frame.screenshot_requested);
        // Edge cleared after consume.
        assert!(!s.screenshot_requested);
        // Burst / video / tour edges NOT set by F12.
        assert!(!frame.burst_requested);
        assert!(!frame.video_toggle_requested);
        assert!(!frame.tour_requested);
    }

    #[test]
    fn f9_starts_burst_request_and_render_mode_8() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::F9,
            pressed: true,
        });
        assert!(s.burst_requested);
        assert_eq!(s.render_mode, 8);
        assert!(s.render_mode_changed);
    }

    #[test]
    fn f8_toggles_video_request_and_render_mode_7() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::F8,
            pressed: true,
        });
        assert!(s.video_toggle_requested);
        assert_eq!(s.render_mode, 7);
        assert!(s.render_mode_changed);
    }

    #[test]
    fn f7_runs_tour_request_and_render_mode_6() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::F7,
            pressed: true,
        });
        assert!(s.tour_requested);
        assert_eq!(s.render_mode, 6);
        assert!(s.render_mode_changed);
    }

    #[test]
    fn shift_sets_sprint() {
        let mut s = InputState::new();
        assert!(!s.sprint);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::LShift,
            pressed: true,
        });
        assert!(s.sprint);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::LShift,
            pressed: false,
        });
        assert!(!s.sprint);
    }
}
