//! § Linux input backend — evdev + libudev (cfg-gated to target_os = "linux").
//!
//! § ROLE
//!   The Linux path uses :
//!     - `/dev/input/event*` evdev nodes for keyboard / mouse / gamepad
//!       (read via the `read(2)` syscall ; struct layout hand-declared).
//!     - `libudev` for device hot-plug detection (dynamic-loaded via
//!       `libloading` — same pattern as `cssl-host-level-zero`'s
//!       `libze_loader.so` handling).
//!
//!   No 4-controller cap (XInput is Win32-only) ; up to 16 simultaneous
//!   gamepads per [`crate::state::GAMEPAD_SLOT_COUNT`].
//!
//! § STATUS
//!   Apocky's primary host is Windows + Arc A770 ; the Linux path is
//!   structurally tested (the unit tests below exercise the parsing /
//!   translation logic without opening real `/dev/input` nodes). A
//!   future Linux CI runner will add integration tests that require an
//!   actual evdev node.

#![allow(clippy::missing_safety_doc)]

use crate::api::BackendKind;
use crate::backend::{GrabModes, InputBackend, InputBackendBuilder, InputError};
use crate::event::{GamepadAxis, GamepadButton, InputEvent, KeyCode, MouseButton, RepeatCount};
use crate::kill_switch::{KillSwitch, KillSwitchEvent, KillSwitchReason};
use crate::mapping::ActionMap;
use crate::state::{InputState, GAMEPAD_AXIS_COUNT};
use std::collections::VecDeque;

// ───────────────────────────────────────────────────────────────────────
// § evdev struct + constants.
// ───────────────────────────────────────────────────────────────────────

/// `struct input_event` from `<linux/input.h>` (size = 24 on 64-bit).
///
/// Layout :
/// ```text
///   struct timeval time;   // 16 bytes (sec : i64, usec : i64)
///   __u16 type;            // 2 bytes
///   __u16 code;            // 2 bytes
///   __s32 value;           // 4 bytes
/// ```
#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InputEventRaw {
    pub time_sec: i64,
    pub time_usec: i64,
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

/// evdev event-type constants (from `<linux/input-event-codes.h>`).
pub mod ev_type {
    pub const SYN: u16 = 0x00;
    pub const KEY: u16 = 0x01;
    pub const REL: u16 = 0x02;
    pub const ABS: u16 = 0x03;
}

/// evdev key-codes (subset — letters / mouse / gamepad).
pub mod ev_key {
    // Letters.
    pub const A: u16 = 30;
    pub const B: u16 = 48;
    pub const C: u16 = 46;
    pub const D: u16 = 32;
    pub const E: u16 = 18;
    pub const F: u16 = 33;
    pub const G: u16 = 34;
    pub const H: u16 = 35;
    pub const I: u16 = 23;
    pub const J: u16 = 36;
    pub const K: u16 = 37;
    pub const L: u16 = 38;
    pub const M: u16 = 50;
    pub const N: u16 = 49;
    pub const O: u16 = 24;
    pub const P: u16 = 25;
    pub const Q: u16 = 16;
    pub const R: u16 = 19;
    pub const S: u16 = 31;
    pub const T: u16 = 20;
    pub const U: u16 = 22;
    pub const V: u16 = 47;
    pub const W: u16 = 17;
    pub const X: u16 = 45;
    pub const Y: u16 = 21;
    pub const Z: u16 = 44;
    // Numbers.
    pub const NUM1: u16 = 2;
    pub const NUM2: u16 = 3;
    pub const NUM3: u16 = 4;
    pub const NUM4: u16 = 5;
    pub const NUM5: u16 = 6;
    pub const NUM6: u16 = 7;
    pub const NUM7: u16 = 8;
    pub const NUM8: u16 = 9;
    pub const NUM9: u16 = 10;
    pub const NUM0: u16 = 11;
    // Function.
    pub const F1: u16 = 59;
    pub const F2: u16 = 60;
    pub const F3: u16 = 61;
    pub const F4: u16 = 62;
    pub const F5: u16 = 63;
    pub const F6: u16 = 64;
    pub const F7: u16 = 65;
    pub const F8: u16 = 66;
    pub const F9: u16 = 67;
    pub const F10: u16 = 68;
    pub const F11: u16 = 87;
    pub const F12: u16 = 88;
    // Whitespace + control.
    pub const ESC: u16 = 1;
    pub const ENTER: u16 = 28;
    pub const SPACE: u16 = 57;
    pub const TAB: u16 = 15;
    pub const BACKSPACE: u16 = 14;
    // Modifiers.
    pub const LEFTSHIFT: u16 = 42;
    pub const RIGHTSHIFT: u16 = 54;
    pub const LEFTCTRL: u16 = 29;
    pub const RIGHTCTRL: u16 = 97;
    pub const LEFTALT: u16 = 56;
    pub const RIGHTALT: u16 = 100;
    pub const LEFTMETA: u16 = 125;
    pub const RIGHTMETA: u16 = 126;
    // Arrows.
    pub const UP: u16 = 103;
    pub const DOWN: u16 = 108;
    pub const LEFT: u16 = 105;
    pub const RIGHT: u16 = 106;
    // Mouse buttons.
    pub const BTN_LEFT: u16 = 0x110;
    pub const BTN_RIGHT: u16 = 0x111;
    pub const BTN_MIDDLE: u16 = 0x112;
    pub const BTN_SIDE: u16 = 0x113;
    pub const BTN_EXTRA: u16 = 0x114;
    // Gamepad buttons.
    pub const BTN_A: u16 = 0x130;
    pub const BTN_B: u16 = 0x131;
    pub const BTN_X: u16 = 0x133;
    pub const BTN_Y: u16 = 0x134;
    pub const BTN_TL: u16 = 0x136; // left bumper
    pub const BTN_TR: u16 = 0x137; // right bumper
    pub const BTN_SELECT: u16 = 0x13A;
    pub const BTN_START: u16 = 0x13B;
    pub const BTN_MODE: u16 = 0x13C; // guide
    pub const BTN_THUMBL: u16 = 0x13D;
    pub const BTN_THUMBR: u16 = 0x13E;
    pub const BTN_DPAD_UP: u16 = 0x220;
    pub const BTN_DPAD_DOWN: u16 = 0x221;
    pub const BTN_DPAD_LEFT: u16 = 0x222;
    pub const BTN_DPAD_RIGHT: u16 = 0x223;
}

/// evdev relative-axis codes (mouse).
pub mod ev_rel {
    pub const X: u16 = 0;
    pub const Y: u16 = 1;
    pub const WHEEL: u16 = 8;
    pub const HWHEEL: u16 = 6;
}

/// evdev absolute-axis codes (gamepad sticks + triggers).
pub mod ev_abs {
    pub const X: u16 = 0; // left stick X
    pub const Y: u16 = 1; // left stick Y
    pub const Z: u16 = 2; // left trigger
    pub const RX: u16 = 3; // right stick X
    pub const RY: u16 = 4; // right stick Y
    pub const RZ: u16 = 5; // right trigger
    pub const HAT0X: u16 = 0x10; // dpad X
    pub const HAT0Y: u16 = 0x11; // dpad Y
}

// ───────────────────────────────────────────────────────────────────────
// § evdev key-code → KeyCode translation.
// ───────────────────────────────────────────────────────────────────────

/// Translate an evdev key-code to the canonical [`KeyCode`].
pub fn evdev_key_to_key_code(code: u16) -> KeyCode {
    match code {
        ev_key::A => KeyCode::A,
        ev_key::B => KeyCode::B,
        ev_key::C => KeyCode::C,
        ev_key::D => KeyCode::D,
        ev_key::E => KeyCode::E,
        ev_key::F => KeyCode::F,
        ev_key::G => KeyCode::G,
        ev_key::H => KeyCode::H,
        ev_key::I => KeyCode::I,
        ev_key::J => KeyCode::J,
        ev_key::K => KeyCode::K,
        ev_key::L => KeyCode::L,
        ev_key::M => KeyCode::M,
        ev_key::N => KeyCode::N,
        ev_key::O => KeyCode::O,
        ev_key::P => KeyCode::P,
        ev_key::Q => KeyCode::Q,
        ev_key::R => KeyCode::R,
        ev_key::S => KeyCode::S,
        ev_key::T => KeyCode::T,
        ev_key::U => KeyCode::U,
        ev_key::V => KeyCode::V,
        ev_key::W => KeyCode::W,
        ev_key::X => KeyCode::X,
        ev_key::Y => KeyCode::Y,
        ev_key::Z => KeyCode::Z,
        ev_key::NUM0 => KeyCode::Num0,
        ev_key::NUM1 => KeyCode::Num1,
        ev_key::NUM2 => KeyCode::Num2,
        ev_key::NUM3 => KeyCode::Num3,
        ev_key::NUM4 => KeyCode::Num4,
        ev_key::NUM5 => KeyCode::Num5,
        ev_key::NUM6 => KeyCode::Num6,
        ev_key::NUM7 => KeyCode::Num7,
        ev_key::NUM8 => KeyCode::Num8,
        ev_key::NUM9 => KeyCode::Num9,
        ev_key::F1 => KeyCode::F1,
        ev_key::F2 => KeyCode::F2,
        ev_key::F3 => KeyCode::F3,
        ev_key::F4 => KeyCode::F4,
        ev_key::F5 => KeyCode::F5,
        ev_key::F6 => KeyCode::F6,
        ev_key::F7 => KeyCode::F7,
        ev_key::F8 => KeyCode::F8,
        ev_key::F9 => KeyCode::F9,
        ev_key::F10 => KeyCode::F10,
        ev_key::F11 => KeyCode::F11,
        ev_key::F12 => KeyCode::F12,
        ev_key::ESC => KeyCode::Escape,
        ev_key::ENTER => KeyCode::Enter,
        ev_key::SPACE => KeyCode::Space,
        ev_key::TAB => KeyCode::Tab,
        ev_key::BACKSPACE => KeyCode::Backspace,
        ev_key::LEFTSHIFT => KeyCode::LeftShift,
        ev_key::RIGHTSHIFT => KeyCode::RightShift,
        ev_key::LEFTCTRL => KeyCode::LeftCtrl,
        ev_key::RIGHTCTRL => KeyCode::RightCtrl,
        ev_key::LEFTALT => KeyCode::LeftAlt,
        ev_key::RIGHTALT => KeyCode::RightAlt,
        ev_key::LEFTMETA => KeyCode::LeftMeta,
        ev_key::RIGHTMETA => KeyCode::RightMeta,
        ev_key::UP => KeyCode::ArrowUp,
        ev_key::DOWN => KeyCode::ArrowDown,
        ev_key::LEFT => KeyCode::ArrowLeft,
        ev_key::RIGHT => KeyCode::ArrowRight,
        _ => KeyCode::Unknown,
    }
}

/// Translate an evdev mouse button-code to the canonical [`MouseButton`].
pub fn evdev_btn_to_mouse_button(code: u16) -> Option<MouseButton> {
    match code {
        ev_key::BTN_LEFT => Some(MouseButton::Left),
        ev_key::BTN_RIGHT => Some(MouseButton::Right),
        ev_key::BTN_MIDDLE => Some(MouseButton::Middle),
        ev_key::BTN_SIDE => Some(MouseButton::X1),
        ev_key::BTN_EXTRA => Some(MouseButton::X2),
        _ => None,
    }
}

/// Translate an evdev gamepad button-code to the canonical
/// [`GamepadButton`].
pub fn evdev_btn_to_gamepad_button(code: u16) -> Option<GamepadButton> {
    match code {
        ev_key::BTN_A => Some(GamepadButton::A),
        ev_key::BTN_B => Some(GamepadButton::B),
        ev_key::BTN_X => Some(GamepadButton::X),
        ev_key::BTN_Y => Some(GamepadButton::Y),
        ev_key::BTN_TL => Some(GamepadButton::LeftBumper),
        ev_key::BTN_TR => Some(GamepadButton::RightBumper),
        ev_key::BTN_SELECT => Some(GamepadButton::Back),
        ev_key::BTN_START => Some(GamepadButton::Start),
        ev_key::BTN_MODE => Some(GamepadButton::Guide),
        ev_key::BTN_THUMBL => Some(GamepadButton::LeftStick),
        ev_key::BTN_THUMBR => Some(GamepadButton::RightStick),
        ev_key::BTN_DPAD_UP => Some(GamepadButton::DPadUp),
        ev_key::BTN_DPAD_DOWN => Some(GamepadButton::DPadDown),
        ev_key::BTN_DPAD_LEFT => Some(GamepadButton::DPadLeft),
        ev_key::BTN_DPAD_RIGHT => Some(GamepadButton::DPadRight),
        _ => None,
    }
}

/// Translate an evdev absolute-axis code to the canonical [`GamepadAxis`].
pub fn evdev_abs_to_gamepad_axis(code: u16) -> Option<GamepadAxis> {
    match code {
        ev_abs::X => Some(GamepadAxis::LeftStickX),
        ev_abs::Y => Some(GamepadAxis::LeftStickY),
        ev_abs::RX => Some(GamepadAxis::RightStickX),
        ev_abs::RY => Some(GamepadAxis::RightStickY),
        ev_abs::Z => Some(GamepadAxis::LeftTrigger),
        ev_abs::RZ => Some(GamepadAxis::RightTrigger),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § libudev loader (dynamic-load via libloading).
// ───────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod udev {
    use super::InputError;
    use libloading::{Library, Symbol};
    use std::ffi::c_void;
    use std::os::raw::c_char;

    pub type UdevPtr = *mut c_void;

    /// Loaded libudev handle. None of the entry-points are wired into
    /// the backend's hot path yet — this struct is the FFI surface for
    /// future hotplug support. Today the backend opens a static device
    /// list from `/dev/input/by-id` and runs without hot-plug.
    pub struct UdevHandle {
        _lib: Library,
        pub udev_new: Option<unsafe extern "C" fn() -> UdevPtr>,
        pub udev_unref: Option<unsafe extern "C" fn(UdevPtr) -> UdevPtr>,
        #[allow(dead_code)]
        pub udev_monitor_new_from_netlink:
            Option<unsafe extern "C" fn(UdevPtr, *const c_char) -> UdevPtr>,
    }

    impl UdevHandle {
        pub fn try_load() -> Result<Self, InputError> {
            const CANDIDATES: &[&str] = &["libudev.so.1", "libudev.so.0", "libudev.so"];
            for cand in CANDIDATES {
                // SAFETY : `libloading::Library::new` opens a shared
                // library by name. Returns Err on missing-file ; we
                // try the next candidate.
                if let Ok(lib) = unsafe { Library::new(*cand) } {
                    // SAFETY : `lib` is a valid loaded library ; we
                    // request specific entry-points by their stable
                    // libudev symbol names. Missing-symbol returns Err
                    // and we map to None.
                    let udev_new: Option<Symbol<unsafe extern "C" fn() -> UdevPtr>> =
                        unsafe { lib.get(b"udev_new\0").ok() };
                    let udev_unref: Option<Symbol<unsafe extern "C" fn(UdevPtr) -> UdevPtr>> =
                        unsafe { lib.get(b"udev_unref\0").ok() };
                    let udev_monitor_new_from_netlink: Option<
                        Symbol<unsafe extern "C" fn(UdevPtr, *const c_char) -> UdevPtr>,
                    > = unsafe { lib.get(b"udev_monitor_new_from_netlink\0").ok() };

                    return Ok(UdevHandle {
                        udev_new: udev_new.map(|s| *s),
                        udev_unref: udev_unref.map(|s| *s),
                        udev_monitor_new_from_netlink: udev_monitor_new_from_netlink.map(|s| *s),
                        _lib: lib,
                    });
                }
            }
            Err(InputError::LoaderError {
                detail: "libudev not found (tried .so.1, .so.0, .so) — hotplug disabled".into(),
            })
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § LinuxBackend.
// ───────────────────────────────────────────────────────────────────────

/// Linux input backend.
#[derive(Debug)]
pub struct LinuxBackend {
    state: InputState,
    events: VecDeque<InputEvent>,
    action_map: ActionMap,
    deadzone: i16,
    per_axis_deadzone: [Option<i16>; GAMEPAD_AXIS_COUNT],
    kill_switch: KillSwitch,
    /// Per-key auto-repeat counter.
    repeat_counters: [u8; 256],
    /// `true` if libudev was loaded successfully — observability only.
    udev_loaded: bool,
}

impl LinuxBackend {
    /// Construct from a builder.
    #[must_use]
    pub fn from_builder(builder: InputBackendBuilder) -> Self {
        let (deadzone, per_axis, action_map, kill_switch) = builder.into_parts();
        #[cfg(target_os = "linux")]
        let udev_loaded = udev::UdevHandle::try_load().is_ok();
        #[cfg(not(target_os = "linux"))]
        let udev_loaded = false;

        Self {
            state: InputState::default(),
            events: VecDeque::new(),
            action_map,
            deadzone,
            per_axis_deadzone: per_axis,
            kill_switch,
            repeat_counters: [0; 256],
            udev_loaded,
        }
    }

    /// Returns `true` if libudev was loaded. Hotplug events are
    /// available only when this is `true`.
    #[must_use]
    pub fn has_udev(&self) -> bool {
        self.udev_loaded
    }

    /// Process one raw `struct input_event` (called from the F1 message
    /// pump or from the test harness). The slot field maps to the
    /// device-index-of-origin (0..[`crate::state::GAMEPAD_SLOT_COUNT`]).
    pub fn process_event(&mut self, slot: u8, raw: InputEventRaw) {
        match raw.kind {
            ev_type::SYN => {
                // Frame boundary — nothing to do (we already emit per-event).
            }
            ev_type::KEY => self.process_key_event(slot, raw),
            ev_type::REL => self.process_rel_event(raw),
            ev_type::ABS => self.process_abs_event(slot, raw),
            _ => {}
        }
    }

    fn process_key_event(&mut self, slot: u8, raw: InputEventRaw) {
        // value : 0 = up, 1 = first press, 2 = repeat
        if let Some(btn) = evdev_btn_to_mouse_button(raw.code) {
            let pressed = raw.value != 0;
            self.state.mouse.set_button(btn, pressed);
            let ev = if pressed {
                InputEvent::MouseDown {
                    button: btn,
                    x: self.state.mouse.x,
                    y: self.state.mouse.y,
                }
            } else {
                InputEvent::MouseUp {
                    button: btn,
                    x: self.state.mouse.x,
                    y: self.state.mouse.y,
                }
            };
            self.events.push_back(ev);
            return;
        }

        if let Some(btn) = evdev_btn_to_gamepad_button(raw.code) {
            let pressed = raw.value != 0;
            if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                g.set_button(btn, pressed);
                if !g.connected {
                    g.connected = true;
                    self.events.push_back(InputEvent::GamepadConnect { slot });
                }
            }
            self.events.push_back(InputEvent::GamepadButtonChange {
                slot,
                button: btn,
                pressed,
            });
            return;
        }

        // Keyboard.
        let code = evdev_key_to_key_code(raw.code);
        if code == KeyCode::Unknown {
            return;
        }

        match raw.value {
            0 => {
                // Up.
                self.state.keys.set(code, false);
                self.repeat_counters[code as usize] = 0;
                self.events.push_back(InputEvent::KeyUp { code });
            }
            1 => {
                // First press.
                self.state.keys.set(code, true);
                self.repeat_counters[code as usize] = 0;
                let event = InputEvent::KeyDown {
                    code,
                    repeat_count: RepeatCount::FirstPress,
                };
                self.events.push_back(event);
                let prior = self.state.grab_state;
                if let Some(reason) = self.kill_switch.on_event(&event, prior) {
                    self.release_grab_internal(reason);
                }
            }
            2 => {
                // Auto-repeat (evdev signals this directly).
                let n = self.repeat_counters[code as usize].saturating_add(1);
                self.repeat_counters[code as usize] = n;
                let event = InputEvent::KeyDown {
                    code,
                    repeat_count: RepeatCount::AutoRepeat(n),
                };
                self.events.push_back(event);
                let prior = self.state.grab_state;
                if let Some(reason) = self.kill_switch.on_event(&event, prior) {
                    self.release_grab_internal(reason);
                }
            }
            _ => {}
        }
    }

    fn process_rel_event(&mut self, raw: InputEventRaw) {
        match raw.code {
            ev_rel::X => {
                self.state.mouse.x = self.state.mouse.x.saturating_add(raw.value);
                self.events.push_back(InputEvent::MouseMove {
                    x: self.state.mouse.x,
                    y: self.state.mouse.y,
                });
            }
            ev_rel::Y => {
                self.state.mouse.y = self.state.mouse.y.saturating_add(raw.value);
                self.events.push_back(InputEvent::MouseMove {
                    x: self.state.mouse.x,
                    y: self.state.mouse.y,
                });
            }
            ev_rel::WHEEL => {
                self.state
                    .mouse
                    .accumulate_scroll(crate::event::ScrollAxis::Vertical, raw.value);
                self.events.push_back(InputEvent::Scroll {
                    axis: crate::event::ScrollAxis::Vertical,
                    delta: raw.value,
                });
            }
            ev_rel::HWHEEL => {
                self.state
                    .mouse
                    .accumulate_scroll(crate::event::ScrollAxis::Horizontal, raw.value);
                self.events.push_back(InputEvent::Scroll {
                    axis: crate::event::ScrollAxis::Horizontal,
                    delta: raw.value,
                });
            }
            _ => {}
        }
    }

    fn process_abs_event(&mut self, slot: u8, raw: InputEventRaw) {
        let Some(axis) = evdev_abs_to_gamepad_axis(raw.code) else {
            return;
        };
        // evdev abs values are typically `i32` ; clamp to i16 for the
        // canonical surface (XInput uses i16 for sticks).
        let value = raw.value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
            if !g.connected {
                g.connected = true;
                self.events.push_back(InputEvent::GamepadConnect { slot });
            }
            g.set_axis(axis, value);
        }
        self.events
            .push_back(InputEvent::GamepadAxisChange { slot, axis, value });
    }

    /// Notify the backend that an external session-lock signal arrived
    /// (logind `Session.Locked`). PRIME-DIRECTIVE-honouring.
    pub fn on_session_lock_change(&mut self, locked: bool) {
        let prior = self.state.grab_state;
        if let Some(reason) = self.kill_switch.on_session_lock(locked, prior) {
            self.release_grab_internal(reason);
        }
    }

    fn release_grab_internal(&mut self, reason: KillSwitchReason) {
        let prior = self.state.grab_state;
        self.state.grab_state = Default::default();
        self.state.clear_all_inputs();
        self.repeat_counters = [0; 256];
        self.kill_switch.trigger(reason, prior, self.state.tick);
    }
}

impl InputBackend for LinuxBackend {
    fn tick(&mut self) -> Result<usize, InputError> {
        self.state.tick = self.state.tick.saturating_add(1);
        for g in &mut self.state.gamepads {
            if g.connected {
                crate::backend::apply_deadzones_to_gamepad(
                    g,
                    self.deadzone,
                    &self.per_axis_deadzone,
                );
            }
        }
        Ok(0)
    }

    fn poll_events(&mut self) -> Option<InputEvent> {
        self.events.pop_front()
    }

    fn poll_kill_switch_events(&mut self) -> Option<KillSwitchEvent> {
        self.kill_switch.drain_events()
    }

    fn current_state(&self) -> &InputState {
        &self.state
    }

    fn acquire_grab(&mut self, modes: GrabModes) -> Result<(), InputError> {
        if self.state.grab_state.is_grabbed() {
            return Err(InputError::GrabAlreadyAcquired);
        }
        self.state.grab_state = crate::state::GrabState {
            cursor_locked: modes.cursor_lock,
            keyboard_captured: modes.keyboard_capture,
            cursor_hidden: modes.cursor_hide,
        };
        Ok(())
    }

    fn release_grab(&mut self) -> Result<(), InputError> {
        if !self.state.grab_state.is_grabbed() {
            return Err(InputError::NoGrabActive);
        }
        self.release_grab_internal(KillSwitchReason::ApplicationRequested);
        Ok(())
    }

    fn set_action_map(&mut self, map: ActionMap) {
        self.action_map = map;
    }

    fn action_map(&self) -> &ActionMap {
        &self.action_map
    }

    fn set_gamepad_deadzone(&mut self, dz: i16) {
        self.deadzone = dz;
    }

    fn set_gamepad_rumble(&mut self, _slot: u8, _low: u16, _high: u16) -> Result<(), InputError> {
        // evdev FF protocol is theoretically supported but a separate
        // surface ; out of scope for S7-F2.
        Err(InputError::FeatureUnavailable)
    }

    fn kill_switch(&self) -> &KillSwitch {
        &self.kill_switch
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Linux
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_key_to_key_code_letters() {
        assert_eq!(evdev_key_to_key_code(ev_key::A), KeyCode::A);
        assert_eq!(evdev_key_to_key_code(ev_key::W), KeyCode::W);
        assert_eq!(evdev_key_to_key_code(ev_key::Z), KeyCode::Z);
    }

    #[test]
    fn evdev_key_to_key_code_modifiers() {
        assert_eq!(evdev_key_to_key_code(ev_key::LEFTSHIFT), KeyCode::LeftShift);
        assert_eq!(evdev_key_to_key_code(ev_key::RIGHTCTRL), KeyCode::RightCtrl);
        assert_eq!(evdev_key_to_key_code(ev_key::LEFTMETA), KeyCode::LeftMeta);
    }

    #[test]
    fn evdev_btn_translation_mouse() {
        assert_eq!(
            evdev_btn_to_mouse_button(ev_key::BTN_LEFT),
            Some(MouseButton::Left)
        );
        assert_eq!(
            evdev_btn_to_mouse_button(ev_key::BTN_RIGHT),
            Some(MouseButton::Right)
        );
        assert!(evdev_btn_to_mouse_button(ev_key::A).is_none());
    }

    #[test]
    fn evdev_btn_translation_gamepad() {
        assert_eq!(
            evdev_btn_to_gamepad_button(ev_key::BTN_A),
            Some(GamepadButton::A)
        );
        assert_eq!(
            evdev_btn_to_gamepad_button(ev_key::BTN_DPAD_UP),
            Some(GamepadButton::DPadUp)
        );
        assert!(evdev_btn_to_gamepad_button(ev_key::A).is_none());
    }

    #[test]
    fn evdev_abs_translation() {
        assert_eq!(
            evdev_abs_to_gamepad_axis(ev_abs::X),
            Some(GamepadAxis::LeftStickX)
        );
        assert_eq!(
            evdev_abs_to_gamepad_axis(ev_abs::RZ),
            Some(GamepadAxis::RightTrigger)
        );
        assert!(evdev_abs_to_gamepad_axis(0xFF).is_none());
    }

    #[test]
    fn input_event_raw_size_is_24() {
        // Linux guarantees this layout on 64-bit.
        assert_eq!(std::mem::size_of::<InputEventRaw>(), 24);
    }

    #[test]
    fn process_event_keydown_first_press() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        b.process_event(
            0,
            InputEventRaw {
                kind: ev_type::KEY,
                code: ev_key::W,
                value: 1,
                ..Default::default()
            },
        );
        assert!(b.current_state().keys.is_pressed(KeyCode::W));
        let ev = b.poll_events().unwrap();
        assert!(matches!(
            ev,
            InputEvent::KeyDown {
                code: KeyCode::W,
                repeat_count: RepeatCount::FirstPress,
            }
        ));
    }

    #[test]
    fn process_event_keydown_auto_repeat() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        b.process_event(
            0,
            InputEventRaw {
                kind: ev_type::KEY,
                code: ev_key::W,
                value: 1,
                ..Default::default()
            },
        );
        let _ = b.poll_events();
        b.process_event(
            0,
            InputEventRaw {
                kind: ev_type::KEY,
                code: ev_key::W,
                value: 2,
                ..Default::default()
            },
        );
        let ev = b.poll_events().unwrap();
        assert!(matches!(
            ev,
            InputEvent::KeyDown {
                code: KeyCode::W,
                repeat_count: RepeatCount::AutoRepeat(1),
            }
        ));
    }

    #[test]
    fn process_event_mouse_rel_move() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        b.process_event(
            0,
            InputEventRaw {
                kind: ev_type::REL,
                code: ev_rel::X,
                value: 10,
                ..Default::default()
            },
        );
        b.process_event(
            0,
            InputEventRaw {
                kind: ev_type::REL,
                code: ev_rel::Y,
                value: 5,
                ..Default::default()
            },
        );
        assert_eq!(b.current_state().mouse.x, 10);
        assert_eq!(b.current_state().mouse.y, 5);
    }

    #[test]
    fn process_event_gamepad_connect_via_button() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        // Gamepad button press should connect the slot and emit
        // both GamepadConnect + GamepadButtonChange.
        b.process_event(
            2,
            InputEventRaw {
                kind: ev_type::KEY,
                code: ev_key::BTN_A,
                value: 1,
                ..Default::default()
            },
        );
        assert!(b.current_state().gamepads[2].connected);
        assert!(b.current_state().gamepads[2].is_button_pressed(GamepadButton::A));
        let connect = b.poll_events().unwrap();
        assert!(matches!(connect, InputEvent::GamepadConnect { slot: 2 }));
        let btn = b.poll_events().unwrap();
        assert!(matches!(
            btn,
            InputEvent::GamepadButtonChange {
                slot: 2,
                button: GamepadButton::A,
                pressed: true,
            }
        ));
    }

    #[test]
    fn esc_press_during_grab_fires_kill_switch_linux() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.process_event(
            0,
            InputEventRaw {
                kind: ev_type::KEY,
                code: ev_key::ESC,
                value: 1,
                ..Default::default()
            },
        );
        assert!(!b.current_state().grab_state.is_grabbed());
        let ks = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks.reason, KillSwitchReason::EscPressed);
    }

    #[test]
    fn session_lock_during_grab_fires_kill_switch_linux() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.on_session_lock_change(true);
        assert!(!b.current_state().grab_state.is_grabbed());
        let ks = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks.reason, KillSwitchReason::SessionLockChord);
    }

    #[test]
    fn rumble_returns_unavailable_linux() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        let err = b.set_gamepad_rumble(0, 100, 100).unwrap_err();
        assert!(matches!(err, InputError::FeatureUnavailable));
    }

    #[test]
    fn process_abs_axis_clamps_to_i16() {
        let mut b = LinuxBackend::from_builder(InputBackendBuilder::new());
        b.process_event(
            0,
            InputEventRaw {
                kind: ev_type::ABS,
                code: ev_abs::X,
                value: 100_000, // outside i16 range
                ..Default::default()
            },
        );
        assert_eq!(
            b.current_state().gamepads[0].axis(GamepadAxis::LeftStickX),
            i16::MAX
        );
    }
}
