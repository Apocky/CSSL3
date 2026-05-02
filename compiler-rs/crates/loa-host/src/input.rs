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

use std::collections::VecDeque;

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
    /// § T11-WAVE3-TEXTINPUT : `/` focuses the text-input box. While the
    /// box is focused, ALL other key events route to the text-input state
    /// (camera/menu nav suspended). Esc cancels, Enter submits.
    Slash,
    /// § T11-WAVE3-TEXTINPUT : Backspace deletes the char before the
    /// text-input cursor when the box is focused.
    Backspace,
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
    /// § T11-WAVE3-TEXTINPUT : a printable character was typed. Routed to
    /// `TextInputState::type_char` ONLY when the text-input is focused.
    /// `c` is the UTF-8 codepoint produced by the keypress (after
    /// modifier-folding by the OS / winit).
    TypeChar { c: char },
}

// ──────────────────────────────────────────────────────────────────────────
// § T11-WAVE3-TEXTINPUT · in-game text-input box
// ──────────────────────────────────────────────────────────────────────────

/// Maximum chars accepted into the text-input buffer (single line).
pub const TEXT_INPUT_MAX_BUFFER: usize = 256;
/// Maximum number of past submissions to keep in the visible history.
pub const TEXT_INPUT_MAX_HISTORY: usize = 5;

/// In-game text-input state. Owned by `InputState` ; drained by the host's
/// per-frame routing. While `focused` is true, all key + char events route
/// into this state instead of the camera / menu layer.
///
/// § PRIME-DIRECTIVE
///   The text-input is a Sovereign-facing surface : Esc unfocuses without
///   side-effects, Enter submits explicitly (no auto-submit on focus-loss),
///   buffer is bounded so a stuck-key cannot OOM the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInputState {
    /// True while the user is editing the buffer. Toggled by `/` (focus)
    /// and Esc (unfocus).
    pub focused: bool,
    /// Current edit buffer (UTF-8). Bounded by `max_buffer`.
    pub buffer: String,
    /// Insert-cursor position (in chars, 0..=buffer.chars().count()).
    pub cursor: usize,
    /// Past N submissions, oldest first. New submissions push to the back ;
    /// when at `max_history`, the oldest is evicted.
    pub history: VecDeque<String>,
    /// Buffer cap (default 256). Type-char beyond this no-ops.
    pub max_buffer: usize,
    /// History cap (default 5).
    pub max_history: usize,
    /// Number of submissions THIS frame. Drained by `consume_frame`.
    pub submitted_this_frame: u32,
    /// Number of chars typed THIS frame. Drained by `consume_frame`.
    pub chars_typed_this_frame: u32,
    /// Last submitted text (for the host's per-frame log + telemetry path).
    /// `None` when no submission this frame.
    pub last_submission: Option<String>,
}

impl Default for TextInputState {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInputState {
    /// Construct an empty, unfocused text-input state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            focused: false,
            buffer: String::new(),
            cursor: 0,
            history: VecDeque::with_capacity(TEXT_INPUT_MAX_HISTORY),
            max_buffer: TEXT_INPUT_MAX_BUFFER,
            max_history: TEXT_INPUT_MAX_HISTORY,
            submitted_this_frame: 0,
            chars_typed_this_frame: 0,
            last_submission: None,
        }
    }

    /// Focus the box. Idempotent : focusing while already focused is a no-op.
    pub fn focus(&mut self) {
        if !self.focused {
            self.focused = true;
            log_event(
                "INFO",
                "loa-host/text-input",
                "text-input · FOCUS · accepting characters",
            );
        }
    }

    /// Unfocus the box without clearing the buffer (so a Sovereign can
    /// re-open and edit the same draft). Use `cancel` to clear+unfocus.
    pub fn unfocus(&mut self) {
        if self.focused {
            self.focused = false;
            log_event("INFO", "loa-host/text-input", "text-input · UNFOCUS");
        }
    }

    /// Insert a single printable character at the cursor. No-op if the
    /// buffer is at `max_buffer` chars OR the box is not focused. `\n` and
    /// other control chars are rejected (newline is reserved for submit).
    pub fn type_char(&mut self, c: char) {
        if !self.focused {
            return;
        }
        // Reject control chars (incl '\n', '\r', '\t', '\u{7f}') ; printable
        // ASCII + general Unicode-printable accepted.
        if c.is_control() {
            return;
        }
        if self.buffer.chars().count() >= self.max_buffer {
            return;
        }
        // Insert at cursor (char-index, not byte-index).
        let byte_idx = self
            .buffer
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len());
        self.buffer.insert(byte_idx, c);
        self.cursor = self.cursor.saturating_add(1);
        self.chars_typed_this_frame = self.chars_typed_this_frame.saturating_add(1);
    }

    /// Delete the char before the cursor (Backspace). No-op if at the
    /// beginning of the buffer or the box is not focused.
    pub fn backspace(&mut self) {
        if !self.focused || self.cursor == 0 {
            return;
        }
        let prev = self.cursor - 1;
        let (start, ch) = match self.buffer.char_indices().nth(prev) {
            Some(p) => p,
            None => return,
        };
        let end = start + ch.len_utf8();
        self.buffer.replace_range(start..end, "");
        self.cursor = prev;
    }

    /// Submit the current buffer : push to `history` (evicting oldest if at
    /// cap), clear the buffer, reset cursor to 0, increment per-frame
    /// counter, and store the value in `last_submission` for the host's
    /// per-frame log/telemetry path. The box stays focused so the user can
    /// type another submission immediately.
    ///
    /// Returns `Some(submitted)` on success ; `None` when the box wasn't
    /// focused or the buffer was empty (we don't push empty history rows).
    pub fn submit(&mut self) -> Option<String> {
        if !self.focused || self.buffer.is_empty() {
            return None;
        }
        let submitted = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(submitted.clone());
        self.submitted_this_frame = self.submitted_this_frame.saturating_add(1);
        self.last_submission = Some(submitted.clone());
        log_event(
            "INFO",
            "loa-host/text-input",
            &format!(
                "text-input · SUBMIT · len={} · history-size={}",
                submitted.chars().count(),
                self.history.len()
            ),
        );
        Some(submitted)
    }

    /// Cancel the current edit : clear buffer, reset cursor, unfocus.
    /// History is preserved.
    pub fn cancel(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        if self.focused {
            self.focused = false;
            log_event(
                "INFO",
                "loa-host/text-input",
                "text-input · CANCEL · buffer-cleared · unfocused",
            );
        }
    }

    /// Drain per-frame counters + last_submission into a snapshot. Called
    /// by `InputState::consume_frame`.
    fn consume_frame_state(&mut self) -> TextInputFrame {
        let snap = TextInputFrame {
            focused: self.focused,
            submission: self.last_submission.take(),
            submitted_count: self.submitted_this_frame,
            chars_typed: self.chars_typed_this_frame,
            buffer_len: self.buffer.chars().count() as u32,
            history_len: self.history.len() as u32,
        };
        self.submitted_this_frame = 0;
        self.chars_typed_this_frame = 0;
        snap
    }
}

/// Per-frame snapshot of text-input activity. Drained by the host's
/// per-frame loop and forwarded to telemetry + MCP mirrors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextInputFrame {
    /// True if the text-input is focused at end-of-frame.
    pub focused: bool,
    /// Most-recent submission this frame (None if no submit happened).
    pub submission: Option<String>,
    /// Submission count this frame (almost always 0 or 1).
    pub submitted_count: u32,
    /// Char-typed count this frame (used for `text_input_chars_typed_total`).
    pub chars_typed: u32,
    /// Current buffer length in chars (post-frame).
    pub buffer_len: u32,
    /// Current history length (post-frame).
    pub history_len: u32,
}

/// Held-axis + accumulating deltas + modal toggles. The host updates this
/// from `RawEvent` stream and drains it once per frame via `consume_frame()`.
///
/// Movement axes are HELD : pressing W sets `forward=1.0` ; releasing W sets
/// `forward=0.0`. Releasing W while S is held leaves `forward=-1.0` (the most
/// recent axis-direction wins, then degrades to the still-held opposite).
/// We track WASD as four separate held-bools internally + recompute the
/// signed axis on each event.
#[derive(Debug, Clone, PartialEq)]
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
    /// § T11-W13-MOVEMENT-AUG : Space-press EDGE — set true on key-DOWN of
    /// the Space key, drained by `consume_frame()`. The movement-aug engine
    /// consumes this for double-jump + slide-jump edges (NOT the held_space
    /// state, which drives the vertical axis for fly-mode).
    pub jump_pressed_edge: bool,
    /// § T11-WAVE3-TEXTINPUT : in-game text-input box state. While
    /// `text_input.focused` is true, all key events route here and the
    /// camera/menu layer is suspended.
    pub text_input: TextInputState,
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
            jump_pressed_edge: false,
            text_input: TextInputState::new(),
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
                // § T11-WAVE3-TEXTINPUT : while the text-input is focused we
                // ignore mouse-look so the camera doesn't drift while the
                // user types. The window.rs layer also gates this, but
                // belt-and-suspenders here keeps the InputState shape robust
                // for tests + scripted invocations.
                if self.text_input.focused {
                    return;
                }
                self.yaw_delta += dx;
                self.pitch_delta += dy;
            }
            RawEvent::CloseRequested => {
                self.quit_requested = true;
                log_event("INFO", "loa-host/input", "close-requested · honoring");
            }
            RawEvent::TypeChar { c } => {
                // ONLY consumed by the text-input — printable chars NEVER
                // affect movement axes. The slash that focused the box is
                // absorbed by the window-side router (it doesn't generate a
                // TypeChar event during the focusing keypress).
                self.text_input.type_char(c);
            }
        }
    }

    fn handle_key(&mut self, vk: VirtualKey, pressed: bool) {
        // § T11-WAVE3-TEXTINPUT : while the text-input is focused, ALL
        // game-control keys are suspended. The few keys the text-input
        // itself consumes (Slash, Backspace, Escape, Enter) are dispatched
        // explicitly below ; everything else is dropped before reaching
        // the WASD/F-key/menu arms.
        if self.text_input.focused {
            // Force movement axes to zero so a held WASD key from before
            // focusing doesn't bleed in as drift while typing.
            self.held_w = false;
            self.held_a = false;
            self.held_s = false;
            self.held_d = false;
            self.held_space = false;
            self.held_lctrl = false;
            self.recompute_axes();
            self.sprint = false;
            // Now dispatch the keys the text-input actually consumes.
            match vk {
                VirtualKey::Backspace => {
                    if pressed {
                        self.text_input.backspace();
                    }
                }
                VirtualKey::Escape => {
                    if pressed {
                        // Esc inside text-input cancels the edit (clears
                        // buffer + unfocuses) ; it does NOT propagate to
                        // the engine's quit-path. Sovereign retains
                        // explicit control over session-end.
                        self.text_input.cancel();
                    }
                }
                VirtualKey::Enter => {
                    if pressed {
                        let _ = self.text_input.submit();
                    }
                }
                VirtualKey::Slash => {
                    // A second `/` while already focused is treated as a
                    // literal char to type. The focus-press happened at the
                    // window layer ; here it's just a typed glyph that
                    // arrives via TypeChar.
                }
                _ => {
                    // All other keys are absorbed silently while typing.
                }
            }
            return;
        }
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
                // § T11-W13-MOVEMENT-AUG : detect rising-edge for jump
                // (movement-aug consumes ; held_space still drives `up` axis).
                if pressed && !self.held_space {
                    self.jump_pressed_edge = true;
                }
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
            VirtualKey::Slash => {
                // § T11-WAVE3-TEXTINPUT : `/` focuses the text-input. The
                // press is consumed (does NOT generate a TypeChar) so the
                // box opens empty. Zero held movement flags so a held WASD
                // doesn't bleed into the box's first frame as drift.
                if pressed {
                    self.text_input.focus();
                    self.held_w = false;
                    self.held_a = false;
                    self.held_s = false;
                    self.held_d = false;
                    self.held_space = false;
                    self.held_lctrl = false;
                    self.recompute_axes();
                    self.sprint = false;
                }
            }
            VirtualKey::Backspace => {
                // Backspace outside the text-input is a no-op (we don't
                // bind it to anything else in the LoA host).
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
        let text_input = self.text_input.consume_frame_state();
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
            // § T11-W13-MOVEMENT-AUG :
            //   crouch_held mirrors LCtrl (the existing crouch key) ;
            //   jump_pressed is the per-frame Space-press EDGE (not held).
            crouch_held: self.held_lctrl,
            jump_pressed: self.jump_pressed_edge,
            text_input,
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
        // § T11-W13-MOVEMENT-AUG : jump-edge fires once per Space-press.
        self.jump_pressed_edge = false;
        frame
    }
}

/// Per-frame snapshot consumed by `Camera::apply_frame()`. Mouse-deltas
/// here are CUMULATIVE for the just-completed frame ; held axes are
/// instantaneous-at-frame-end.
#[derive(Debug, Clone, PartialEq)]
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
    /// § T11-W13-MOVEMENT-AUG : crouch held (C / LCtrl). Slides if crouch
    /// pressed while sprinting & grounded ; otherwise crouches the hitbox.
    pub crouch_held: bool,
    /// § T11-W13-MOVEMENT-AUG : jump pressed edge (Space). Consumed by the
    /// movement-augmentation engine for double-jumps + slide-jump combos.
    pub jump_pressed: bool,
    /// § T11-WAVE3-TEXTINPUT : per-frame text-input snapshot
    /// (focus state + any submission).
    pub text_input: TextInputFrame,
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
            crouch_held: false,
            jump_pressed: false,
            text_input: TextInputFrame::default(),
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

    // ───────────────────────────────────────────────────────────────────
    // § T11-WAVE3-TEXTINPUT : in-game text-input box tests
    // ───────────────────────────────────────────────────────────────────

    #[test]
    fn text_input_focus_disables_camera_movement() {
        // Hold W → forward axis = 1.0. Now press `/` to focus the text-
        // input. The held WASD must be suppressed and the axis re-zero.
        // Mouse motion while focused must NOT accumulate into yaw/pitch.
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::W,
            pressed: true,
        });
        assert_eq!(s.forward, 1.0);
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Slash,
            pressed: true,
        });
        assert!(s.text_input.focused);
        // Forward axis dropped to zero ; sprint cleared.
        assert_eq!(s.forward, 0.0);
        assert_eq!(s.right, 0.0);
        assert_eq!(s.up, 0.0);
        assert!(!s.sprint);
        // Even if the renderer/host pushes mouse-motion, the InputState
        // refuses to accumulate it while focused.
        s.handle_event(&RawEvent::MouseMotion { dx: 50.0, dy: -25.0 });
        assert_eq!(s.yaw_delta, 0.0);
        assert_eq!(s.pitch_delta, 0.0);
    }

    #[test]
    fn text_input_type_char_appends_to_buffer() {
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Slash,
            pressed: true,
        });
        s.handle_event(&RawEvent::TypeChar { c: 'h' });
        s.handle_event(&RawEvent::TypeChar { c: 'i' });
        s.handle_event(&RawEvent::TypeChar { c: '!' });
        assert_eq!(s.text_input.buffer, "hi!");
        assert_eq!(s.text_input.cursor, 3);
        assert_eq!(s.text_input.chars_typed_this_frame, 3);
    }

    #[test]
    fn text_input_backspace_deletes_last_char() {
        let mut s = InputState::new();
        s.text_input.focus();
        for c in "hello".chars() {
            s.handle_event(&RawEvent::TypeChar { c });
        }
        assert_eq!(s.text_input.buffer, "hello");
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Backspace,
            pressed: true,
        });
        assert_eq!(s.text_input.buffer, "hell");
        assert_eq!(s.text_input.cursor, 4);
        // Two more backspaces.
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Backspace,
            pressed: true,
        });
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Backspace,
            pressed: true,
        });
        assert_eq!(s.text_input.buffer, "he");
    }

    #[test]
    fn text_input_submit_pushes_to_history_and_clears() {
        let mut s = InputState::new();
        s.text_input.focus();
        for c in "first".chars() {
            s.handle_event(&RawEvent::TypeChar { c });
        }
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Enter,
            pressed: true,
        });
        // Buffer cleared, cursor reset, history grew, last_submission set.
        assert_eq!(s.text_input.buffer, "");
        assert_eq!(s.text_input.cursor, 0);
        assert_eq!(s.text_input.history.len(), 1);
        assert_eq!(s.text_input.history.back().unwrap(), "first");
        assert_eq!(s.text_input.last_submission.as_deref(), Some("first"));
        // Box stays focused so the user can keep typing.
        assert!(s.text_input.focused);
        // Submit again with another payload.
        for c in "second".chars() {
            s.handle_event(&RawEvent::TypeChar { c });
        }
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Enter,
            pressed: true,
        });
        assert_eq!(s.text_input.history.len(), 2);
        assert_eq!(s.text_input.history.front().unwrap(), "first");
        assert_eq!(s.text_input.history.back().unwrap(), "second");
    }

    #[test]
    fn text_input_history_caps_at_5() {
        let mut s = InputState::new();
        s.text_input.focus();
        // Push 7 submissions — the first two must be evicted.
        for n in 0..7u32 {
            let payload = format!("msg-{n}");
            for c in payload.chars() {
                s.handle_event(&RawEvent::TypeChar { c });
            }
            s.handle_event(&RawEvent::Key {
                vk: VirtualKey::Enter,
                pressed: true,
            });
        }
        assert_eq!(s.text_input.history.len(), TEXT_INPUT_MAX_HISTORY);
        assert_eq!(s.text_input.history.front().unwrap(), "msg-2");
        assert_eq!(s.text_input.history.back().unwrap(), "msg-6");
    }

    #[test]
    fn text_input_cancel_clears_and_unfocuses() {
        let mut s = InputState::new();
        s.text_input.focus();
        for c in "draft".chars() {
            s.handle_event(&RawEvent::TypeChar { c });
        }
        // Hit Esc → cancel.
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Escape,
            pressed: true,
        });
        assert_eq!(s.text_input.buffer, "");
        assert_eq!(s.text_input.cursor, 0);
        assert!(!s.text_input.focused);
        // CRITICAL : Esc inside the text-input does NOT propagate to
        // quit_requested. Sovereign retains control over session-end.
        assert!(!s.quit_requested);
    }

    #[test]
    fn text_input_buffer_caps_at_256_chars() {
        let mut s = InputState::new();
        s.text_input.focus();
        // Type 300 chars — the first 256 fit, the rest are silently
        // dropped (no panic, no overflow).
        for _ in 0..300u32 {
            s.handle_event(&RawEvent::TypeChar { c: 'x' });
        }
        assert_eq!(
            s.text_input.buffer.chars().count(),
            TEXT_INPUT_MAX_BUFFER
        );
        assert_eq!(s.text_input.cursor, TEXT_INPUT_MAX_BUFFER);
    }

    #[test]
    fn text_input_consume_frame_drains_submission_and_counters() {
        // Per-frame snapshot must carry the submission OUT and reset the
        // per-frame counters back to zero so the next frame starts clean.
        let mut s = InputState::new();
        s.text_input.focus();
        for c in "abc".chars() {
            s.handle_event(&RawEvent::TypeChar { c });
        }
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Enter,
            pressed: true,
        });
        let f = s.consume_frame();
        assert!(f.text_input.focused);
        assert_eq!(f.text_input.submission.as_deref(), Some("abc"));
        assert_eq!(f.text_input.submitted_count, 1);
        assert_eq!(f.text_input.chars_typed, 3);
        // Counters reset for the next frame.
        assert_eq!(s.text_input.submitted_this_frame, 0);
        assert_eq!(s.text_input.chars_typed_this_frame, 0);
        assert!(s.text_input.last_submission.is_none());
    }

    #[test]
    fn text_input_slash_only_focuses_when_unfocused() {
        // First `/` press focuses ; subsequent `/` chars (TypeChar) become
        // literal slashes inside the buffer.
        let mut s = InputState::new();
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Slash,
            pressed: true,
        });
        assert!(s.text_input.focused);
        assert_eq!(s.text_input.buffer, "");
        // Now a TypeChar('/') is a literal slash.
        s.handle_event(&RawEvent::TypeChar { c: '/' });
        assert_eq!(s.text_input.buffer, "/");
    }

    #[test]
    fn text_input_submit_empty_is_noop() {
        let mut s = InputState::new();
        s.text_input.focus();
        // Enter on empty buffer → no submission, no history push.
        s.handle_event(&RawEvent::Key {
            vk: VirtualKey::Enter,
            pressed: true,
        });
        assert_eq!(s.text_input.history.len(), 0);
        assert!(s.text_input.last_submission.is_none());
        assert!(s.text_input.focused);
    }

    #[test]
    fn text_input_rejects_control_chars() {
        let mut s = InputState::new();
        s.text_input.focus();
        s.handle_event(&RawEvent::TypeChar { c: 'a' });
        s.handle_event(&RawEvent::TypeChar { c: '\n' }); // ignored
        s.handle_event(&RawEvent::TypeChar { c: '\t' }); // ignored
        s.handle_event(&RawEvent::TypeChar { c: 'b' });
        assert_eq!(s.text_input.buffer, "ab");
    }
}
