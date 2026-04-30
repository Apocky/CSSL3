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
    // Render-mode select (10 modes per scenes/render_pipeline.cssl design)
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
            VirtualKey::F1 => {
                if pressed {
                    self.render_mode = 0;
                }
            }
            VirtualKey::F2 => {
                if pressed {
                    self.render_mode = 1;
                }
            }
            VirtualKey::F3 => {
                if pressed {
                    self.render_mode = 2;
                }
            }
            VirtualKey::F4 => {
                if pressed {
                    self.render_mode = 3;
                }
            }
            VirtualKey::F5 => {
                if pressed {
                    self.render_mode = 4;
                }
            }
            VirtualKey::F6 => {
                if pressed {
                    self.render_mode = 5;
                }
            }
            VirtualKey::F7 => {
                if pressed {
                    self.render_mode = 6;
                }
            }
            VirtualKey::F8 => {
                if pressed {
                    self.render_mode = 7;
                }
            }
            VirtualKey::F9 => {
                if pressed {
                    self.render_mode = 8;
                }
            }
            VirtualKey::F10 => {
                if pressed {
                    self.render_mode = 9;
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
            paused: self.paused,
            debug_overlay: self.debug_overlay,
            quit_requested: self.quit_requested,
        };
        self.yaw_delta = 0.0;
        self.pitch_delta = 0.0;
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
    pub paused: bool,
    pub debug_overlay: bool,
    pub quit_requested: bool,
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
