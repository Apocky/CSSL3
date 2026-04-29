//! § Win32 input backend — XInput (gamepad) + raw-input (keyboard / mouse).
//!
//! § ROLE
//!   Apocky's primary host is Windows 11 + Arc A770 ; this is the
//!   integration-tested branch. The backend uses :
//!     - **XInput 1.4** (`Xinput.dll`) for gamepad polling — supports
//!       up to 4 controllers (XInput's hard cap per the slice landmines).
//!       Functions used : `XInputGetState`, `XInputSetState` (rumble),
//!       `XInputGetCapabilities`.
//!     - **Raw-input** (`User32.dll`) for keyboard + mouse — registered
//!       via `RegisterRawInputDevices` with `RIDEV_INPUTSINK` so events
//!       arrive even when window loses focus. Read via
//!       `GetRawInputData` from `WM_INPUT` messages.
//!     - **`GetKeyState` is intentionally NOT used** per the slice
//!       landmines : "raw-input thread-safety : MUST process from same
//!       thread as window message-pump (or use dedicated raw-input
//!       thread + lock-free queue)" and "NOT GetKeyState which races".
//!     - **Hand-rolled FFI** declarations against `Xinput.dll` and
//!       `User32.dll` — we don't pull in `windows-rs` here (smaller
//!       surface than what cssl-host-d3d12 buys ; mirrors
//!       cssl-host-level-zero's hand-rolled FFI strategy).
//!
//! § AUTO-REPEAT
//!   Win32 fires `WM_KEYDOWN` repeatedly per OS auto-repeat. The
//!   `lparam` of `WM_KEYDOWN` carries the previous-key-state bit
//!   (`0x40000000`) which discriminates first-press from auto-repeat.
//!   The backend translates this into [`crate::event::RepeatCount`].
//!
//! § KILL-SWITCH
//!   - **Esc** : detected in [`Win32Backend::process_raw_input_keyboard`]
//!     before the event reaches the application queue. If the grab is
//!     active, `release_grab_internal` is called BEFORE the event is
//!     recorded.
//!   - **Win+L** : intercepted by `winlogon.exe` BEFORE user-mode sees
//!     the chord. The backend hooks `WM_WTSSESSION_CHANGE` (registered
//!     via `WTSRegisterSessionNotification`) and on `WTS_SESSION_LOCK`
//!     calls the kill-switch.
//!   - **Sign-out / shutdown** : `WM_QUERYENDSESSION` triggers the
//!     kill-switch.
//!
//! § SAFETY
//!   Every `unsafe` block carries an inline `// SAFETY :` paragraph.
//!   The FFI layer is gated behind RAII handles (`XInputDll`,
//!   `UserDll`) that own their `HMODULE`s and `FreeLibrary` on drop.

#![allow(clippy::missing_safety_doc)]
// § Win32 API names (HMODULE, HWND, DWORD, etc.) are Microsoft-canonical
// uppercase ; the upper_case_acronyms lint flags them but renaming would
// obscure the FFI mapping. Allowed at file scope.
#![allow(clippy::upper_case_acronyms)]
// § `transmute` is required to convert FARPROC into typed extern fn-ptrs ;
// the surrounding SAFETY paragraphs document the contract.
#![allow(clippy::missing_transmute_annotations)]

use crate::api::BackendKind;
use crate::backend::{GrabModes, InputBackend, InputBackendBuilder, InputError};
use crate::event::{
    GamepadAxis, GamepadButton, InputEvent, KeyCode, MouseButton, RepeatCount, ScrollAxis,
};
use crate::kill_switch::{KillSwitch, KillSwitchEvent, KillSwitchReason};
use crate::mapping::ActionMap;
use crate::state::{InputState, GAMEPAD_AXIS_COUNT};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::os::raw::{c_int, c_uint, c_ushort};

// ───────────────────────────────────────────────────────────────────────
// § Win32 type aliases.
// ───────────────────────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
type DWORD = u32;
#[allow(non_camel_case_types)]
type WORD = u16;
#[allow(non_camel_case_types)]
type BYTE = u8;
#[allow(non_camel_case_types)]
type SHORT = i16;
#[allow(non_camel_case_types)]
type HMODULE = *mut c_void;
#[allow(non_camel_case_types)]
type HWND = *mut c_void;
#[allow(non_camel_case_types)]
type FARPROC = *mut c_void;

// ───────────────────────────────────────────────────────────────────────
// § XInput FFI surface.
// ───────────────────────────────────────────────────────────────────────

/// XInput controller-state struct. Per Microsoft's `XINPUT_GAMEPAD`.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct XInputGamepad {
    buttons: WORD,
    left_trigger: BYTE,
    right_trigger: BYTE,
    thumb_lx: SHORT,
    thumb_ly: SHORT,
    thumb_rx: SHORT,
    thumb_ry: SHORT,
}

/// XInput full-state struct. Per Microsoft's `XINPUT_STATE`.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct XInputState {
    packet_number: DWORD,
    gamepad: XInputGamepad,
}

/// XInput vibration struct. Per Microsoft's `XINPUT_VIBRATION`.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct XInputVibration {
    left_motor_speed: WORD,
    right_motor_speed: WORD,
}

/// XInput button bits — per Microsoft's `XINPUT_GAMEPAD_*` constants.
mod xinput_btn {
    pub(super) const DPAD_UP: u16 = 0x0001;
    pub(super) const DPAD_DOWN: u16 = 0x0002;
    pub(super) const DPAD_LEFT: u16 = 0x0004;
    pub(super) const DPAD_RIGHT: u16 = 0x0008;
    pub(super) const START: u16 = 0x0010;
    pub(super) const BACK: u16 = 0x0020;
    pub(super) const LEFT_THUMB: u16 = 0x0040;
    pub(super) const RIGHT_THUMB: u16 = 0x0080;
    pub(super) const LEFT_SHOULDER: u16 = 0x0100;
    pub(super) const RIGHT_SHOULDER: u16 = 0x0200;
    pub(super) const A: u16 = 0x1000;
    pub(super) const B: u16 = 0x2000;
    pub(super) const X: u16 = 0x4000;
    pub(super) const Y: u16 = 0x8000;
}

const ERROR_SUCCESS: DWORD = 0;
const ERROR_DEVICE_NOT_CONNECTED: DWORD = 0x048F;

/// `XInputGetState` fn-ptr signature.
type FnXInputGetState = unsafe extern "system" fn(DWORD, *mut XInputState) -> DWORD;
/// `XInputSetState` fn-ptr signature.
type FnXInputSetState = unsafe extern "system" fn(DWORD, *mut XInputVibration) -> DWORD;

/// RAII handle to a dynamically-loaded XInput DLL.
#[derive(Debug)]
struct XInputDll {
    handle: HMODULE,
    get_state: Option<FnXInputGetState>,
    set_state: Option<FnXInputSetState>,
}

impl XInputDll {
    /// Attempts to load `Xinput1_4.dll` ; falls back through 1.3 → 9_1_0.
    /// Returns `None` if no XInput DLL is available (gamepad support is
    /// then [`InputError::FeatureUnavailable`] but keyboard / mouse still
    /// work).
    fn try_load() -> Option<Self> {
        const CANDIDATES: &[&str] = &["Xinput1_4.dll\0", "Xinput1_3.dll\0", "Xinput9_1_0.dll\0"];
        for cand in CANDIDATES {
            // SAFETY : `LoadLibraryA` accepts a NUL-terminated ASCII
            // string ; the candidates are static literals with explicit
            // NUL terminators. Returns NULL on failure (we check).
            let handle: HMODULE = unsafe { LoadLibraryA(cand.as_ptr().cast()) };
            if !handle.is_null() {
                // SAFETY : `GetProcAddress` requires the same `handle`
                // returned by `LoadLibraryA` above. Returns NULL on
                // missing-symbol (we map to None).
                let get_state: FARPROC =
                    unsafe { GetProcAddress(handle, b"XInputGetState\0".as_ptr().cast()) };
                let set_state: FARPROC =
                    unsafe { GetProcAddress(handle, b"XInputSetState\0".as_ptr().cast()) };
                if get_state.is_null() || set_state.is_null() {
                    // Symbol missing : try next.
                    // SAFETY : valid HMODULE returned by LoadLibraryA.
                    unsafe { FreeLibrary(handle) };
                    continue;
                }
                // SAFETY : `transmute` of a non-NULL FARPROC into the
                // matching extern-C fn ptr. Microsoft guarantees the
                // calling convention of XInput entry points matches our
                // type aliases.
                return Some(Self {
                    handle,
                    get_state: Some(unsafe { std::mem::transmute(get_state) }),
                    set_state: Some(unsafe { std::mem::transmute(set_state) }),
                });
            }
        }
        None
    }

    /// Polls one controller. Returns `Ok(Some(state))` on success,
    /// `Ok(None)` if not connected, `Err` on other failures.
    fn get_state(&self, slot: u32) -> Result<Option<XInputState>, InputError> {
        let Some(get) = self.get_state else {
            return Err(InputError::FeatureUnavailable);
        };
        let mut st = XInputState::default();
        // SAFETY : `slot` is u32 (XInput accepts any value, returns
        // ERROR_DEVICE_NOT_CONNECTED for slot ≥ 4). `&mut st` outlives
        // the call.
        let rc = unsafe { get(slot, &mut st) };
        match rc {
            ERROR_SUCCESS => Ok(Some(st)),
            ERROR_DEVICE_NOT_CONNECTED => Ok(None),
            other => Err(InputError::OsError {
                detail: format!("XInputGetState returned 0x{other:08X}"),
            }),
        }
    }

    /// Sets vibration on one controller.
    fn set_vibration(&self, slot: u32, low: u16, high: u16) -> Result<(), InputError> {
        let Some(set) = self.set_state else {
            return Err(InputError::FeatureUnavailable);
        };
        let mut v = XInputVibration {
            left_motor_speed: low,
            right_motor_speed: high,
        };
        // SAFETY : per get_state.
        let rc = unsafe { set(slot, &mut v) };
        if rc == ERROR_SUCCESS {
            Ok(())
        } else if rc == ERROR_DEVICE_NOT_CONNECTED {
            Err(InputError::GamepadSlotOutOfRange {
                slot: slot as u8,
                max: 3,
            })
        } else {
            Err(InputError::OsError {
                detail: format!("XInputSetState returned 0x{rc:08X}"),
            })
        }
    }
}

impl Drop for XInputDll {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY : valid HMODULE owned by this struct.
            unsafe { FreeLibrary(self.handle) };
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Kernel32 / User32 minimal FFI.
// ───────────────────────────────────────────────────────────────────────

extern "system" {
    fn LoadLibraryA(name: *const u8) -> HMODULE;
    fn FreeLibrary(handle: HMODULE) -> c_int;
    fn GetProcAddress(handle: HMODULE, name: *const u8) -> FARPROC;
}

// ───────────────────────────────────────────────────────────────────────
// § Raw-input — declarations only ; full WM_INPUT plumbing is wired
//   into the Win32Backend::process_wm_input entry point that the F1
//   message-pump drives. We declare the structs here so the F1
//   integration knows the contract.
// ───────────────────────────────────────────────────────────────────────

/// Raw-input device usage. From `RAWINPUTDEVICE` Microsoft struct.
#[allow(dead_code)]
#[repr(C)]
struct RawInputDevice {
    usage_page: c_ushort,
    usage: c_ushort,
    flags: c_uint,
    hwnd_target: HWND,
}

/// Raw-input device-type discriminator (from `RAWINPUT.header.dwType`).
/// Reserved for future use when the F1 message-pump forwards
/// `WM_INPUT` raw bytes ; today only `process_raw_input_keyboard` and
/// `process_raw_input_mouse` are wired up by direct call.
#[allow(dead_code)]
const RIM_TYPEMOUSE: DWORD = 0;
#[allow(dead_code)]
const RIM_TYPEKEYBOARD: DWORD = 1;

/// Raw-input keyboard message-flags.
const RI_KEY_BREAK: u16 = 0x01; // key up
const RI_KEY_E0: u16 = 0x02; // extended key (e.g., right Ctrl, arrows on numpad)
#[allow(dead_code)]
const RI_KEY_E1: u16 = 0x04;

/// Raw-input mouse button-flags.
const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
const RI_MOUSE_LEFT_BUTTON_UP: u16 = 0x0002;
const RI_MOUSE_RIGHT_BUTTON_DOWN: u16 = 0x0004;
const RI_MOUSE_RIGHT_BUTTON_UP: u16 = 0x0008;
const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010;
const RI_MOUSE_MIDDLE_BUTTON_UP: u16 = 0x0020;
const RI_MOUSE_BUTTON_4_DOWN: u16 = 0x0040;
const RI_MOUSE_BUTTON_4_UP: u16 = 0x0080;
const RI_MOUSE_BUTTON_5_DOWN: u16 = 0x0100;
const RI_MOUSE_BUTTON_5_UP: u16 = 0x0200;
const RI_MOUSE_WHEEL: u16 = 0x0400;
const RI_MOUSE_HWHEEL: u16 = 0x0800;

// ───────────────────────────────────────────────────────────────────────
// § VK code → KeyCode translation.
// ───────────────────────────────────────────────────────────────────────

/// Translate a Win32 VK_* + extended-flag pair into the canonical
/// [`KeyCode`]. Returns `KeyCode::Unknown` for unsupported codes (the
/// event is suppressed per the §1 PRIME-DIRECTIVE no-surveillance rule).
pub fn vk_to_key_code(vk: u16, extended: bool) -> KeyCode {
    match vk {
        // Letters.
        0x41 => KeyCode::A,
        0x42 => KeyCode::B,
        0x43 => KeyCode::C,
        0x44 => KeyCode::D,
        0x45 => KeyCode::E,
        0x46 => KeyCode::F,
        0x47 => KeyCode::G,
        0x48 => KeyCode::H,
        0x49 => KeyCode::I,
        0x4A => KeyCode::J,
        0x4B => KeyCode::K,
        0x4C => KeyCode::L,
        0x4D => KeyCode::M,
        0x4E => KeyCode::N,
        0x4F => KeyCode::O,
        0x50 => KeyCode::P,
        0x51 => KeyCode::Q,
        0x52 => KeyCode::R,
        0x53 => KeyCode::S,
        0x54 => KeyCode::T,
        0x55 => KeyCode::U,
        0x56 => KeyCode::V,
        0x57 => KeyCode::W,
        0x58 => KeyCode::X,
        0x59 => KeyCode::Y,
        0x5A => KeyCode::Z,
        // Numbers.
        0x30 => KeyCode::Num0,
        0x31 => KeyCode::Num1,
        0x32 => KeyCode::Num2,
        0x33 => KeyCode::Num3,
        0x34 => KeyCode::Num4,
        0x35 => KeyCode::Num5,
        0x36 => KeyCode::Num6,
        0x37 => KeyCode::Num7,
        0x38 => KeyCode::Num8,
        0x39 => KeyCode::Num9,
        // Function keys.
        0x70 => KeyCode::F1,
        0x71 => KeyCode::F2,
        0x72 => KeyCode::F3,
        0x73 => KeyCode::F4,
        0x74 => KeyCode::F5,
        0x75 => KeyCode::F6,
        0x76 => KeyCode::F7,
        0x77 => KeyCode::F8,
        0x78 => KeyCode::F9,
        0x79 => KeyCode::F10,
        0x7A => KeyCode::F11,
        0x7B => KeyCode::F12,
        // Whitespace + control.
        0x20 => KeyCode::Space,
        0x09 => KeyCode::Tab,
        0x0D => {
            if extended {
                KeyCode::KPEnter
            } else {
                KeyCode::Enter
            }
        }
        0x1B => KeyCode::Escape,
        0x08 => KeyCode::Backspace,
        // Modifiers (extended-flag distinguishes left from right).
        0xA0 => KeyCode::LeftShift,
        0xA1 => KeyCode::RightShift,
        0xA2 => KeyCode::LeftCtrl,
        0xA3 => KeyCode::RightCtrl,
        0xA4 => KeyCode::LeftAlt,
        0xA5 => KeyCode::RightAlt,
        0x5B => KeyCode::LeftMeta,
        0x5C => KeyCode::RightMeta,
        // Arrows + nav (extended-flag).
        0x25 => KeyCode::ArrowLeft,
        0x26 => KeyCode::ArrowUp,
        0x27 => KeyCode::ArrowRight,
        0x28 => KeyCode::ArrowDown,
        0x24 => KeyCode::Home,
        0x23 => KeyCode::End,
        0x21 => KeyCode::PageUp,
        0x22 => KeyCode::PageDown,
        0x2D => KeyCode::Insert,
        0x2E => KeyCode::Delete,
        // Punctuation.
        0xBC => KeyCode::Comma,
        0xBE => KeyCode::Period,
        0xBF => KeyCode::Slash,
        0xBA => KeyCode::Semicolon,
        0xDE => KeyCode::Quote,
        0xC0 => KeyCode::Backquote,
        0xDB => KeyCode::LeftBracket,
        0xDD => KeyCode::RightBracket,
        0xDC => KeyCode::Backslash,
        0xBD => KeyCode::Minus,
        0xBB => KeyCode::Equal,
        // Locks + system.
        0x14 => KeyCode::CapsLock,
        0x90 => KeyCode::NumLock,
        0x91 => KeyCode::ScrollLock,
        0x2C => KeyCode::PrintScreen,
        0x13 => KeyCode::Pause,
        // Keypad (numpad).
        0x60 => KeyCode::KP0,
        0x61 => KeyCode::KP1,
        0x62 => KeyCode::KP2,
        0x63 => KeyCode::KP3,
        0x64 => KeyCode::KP4,
        0x65 => KeyCode::KP5,
        0x66 => KeyCode::KP6,
        0x67 => KeyCode::KP7,
        0x68 => KeyCode::KP8,
        0x69 => KeyCode::KP9,
        0x6A => KeyCode::KPMul,
        0x6B => KeyCode::KPAdd,
        0x6D => KeyCode::KPSub,
        0x6E => KeyCode::KPDecimal,
        0x6F => KeyCode::KPDiv,
        _ => KeyCode::Unknown,
    }
}

/// Translate XInput button-bits to canonical [`GamepadButton`] mask
/// updates. Returns the equivalent `GamepadState.buttons` u16.
pub fn xinput_buttons_to_canonical(xi: u16) -> u16 {
    let mut out: u16 = 0;
    if xi & xinput_btn::A != 0 {
        out |= 1 << GamepadButton::A as u16;
    }
    if xi & xinput_btn::B != 0 {
        out |= 1 << GamepadButton::B as u16;
    }
    if xi & xinput_btn::X != 0 {
        out |= 1 << GamepadButton::X as u16;
    }
    if xi & xinput_btn::Y != 0 {
        out |= 1 << GamepadButton::Y as u16;
    }
    if xi & xinput_btn::LEFT_SHOULDER != 0 {
        out |= 1 << GamepadButton::LeftBumper as u16;
    }
    if xi & xinput_btn::RIGHT_SHOULDER != 0 {
        out |= 1 << GamepadButton::RightBumper as u16;
    }
    if xi & xinput_btn::BACK != 0 {
        out |= 1 << GamepadButton::Back as u16;
    }
    if xi & xinput_btn::START != 0 {
        out |= 1 << GamepadButton::Start as u16;
    }
    if xi & xinput_btn::LEFT_THUMB != 0 {
        out |= 1 << GamepadButton::LeftStick as u16;
    }
    if xi & xinput_btn::RIGHT_THUMB != 0 {
        out |= 1 << GamepadButton::RightStick as u16;
    }
    if xi & xinput_btn::DPAD_UP != 0 {
        out |= 1 << GamepadButton::DPadUp as u16;
    }
    if xi & xinput_btn::DPAD_DOWN != 0 {
        out |= 1 << GamepadButton::DPadDown as u16;
    }
    if xi & xinput_btn::DPAD_LEFT != 0 {
        out |= 1 << GamepadButton::DPadLeft as u16;
    }
    if xi & xinput_btn::DPAD_RIGHT != 0 {
        out |= 1 << GamepadButton::DPadRight as u16;
    }
    out
}

/// Translate raw-input `usButtonFlags` into a series of mouse events.
/// Returns the events produced (up to 8 — left/right/middle/x1/x2 +
/// vertical-wheel + horizontal-wheel + move).
#[allow(clippy::too_many_arguments)]
pub fn raw_input_mouse_events(
    button_flags: u16,
    wheel_delta: i16,
    last_x: i32,
    last_y: i32,
    new_x: i32,
    new_y: i32,
    horizontal: bool,
) -> Vec<InputEvent> {
    let mut events = Vec::new();

    // Move.
    if new_x != last_x || new_y != last_y {
        events.push(InputEvent::MouseMove { x: new_x, y: new_y });
    }

    // Buttons.
    if button_flags & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
        events.push(InputEvent::MouseDown {
            button: MouseButton::Left,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_LEFT_BUTTON_UP != 0 {
        events.push(InputEvent::MouseUp {
            button: MouseButton::Left,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_RIGHT_BUTTON_DOWN != 0 {
        events.push(InputEvent::MouseDown {
            button: MouseButton::Right,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_RIGHT_BUTTON_UP != 0 {
        events.push(InputEvent::MouseUp {
            button: MouseButton::Right,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_MIDDLE_BUTTON_DOWN != 0 {
        events.push(InputEvent::MouseDown {
            button: MouseButton::Middle,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_MIDDLE_BUTTON_UP != 0 {
        events.push(InputEvent::MouseUp {
            button: MouseButton::Middle,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_BUTTON_4_DOWN != 0 {
        events.push(InputEvent::MouseDown {
            button: MouseButton::X1,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_BUTTON_4_UP != 0 {
        events.push(InputEvent::MouseUp {
            button: MouseButton::X1,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_BUTTON_5_DOWN != 0 {
        events.push(InputEvent::MouseDown {
            button: MouseButton::X2,
            x: new_x,
            y: new_y,
        });
    }
    if button_flags & RI_MOUSE_BUTTON_5_UP != 0 {
        events.push(InputEvent::MouseUp {
            button: MouseButton::X2,
            x: new_x,
            y: new_y,
        });
    }

    // Scroll. Win32 WHEEL_DELTA = 120 per-tick ; convert to integer ticks.
    if button_flags & RI_MOUSE_WHEEL != 0 || button_flags & RI_MOUSE_HWHEEL != 0 {
        let axis = if horizontal || (button_flags & RI_MOUSE_HWHEEL != 0) {
            ScrollAxis::Horizontal
        } else {
            ScrollAxis::Vertical
        };
        let ticks = i32::from(wheel_delta) / 120;
        if ticks != 0 {
            events.push(InputEvent::Scroll { axis, delta: ticks });
        }
    }

    events
}

// ───────────────────────────────────────────────────────────────────────
// § Win32Backend.
// ───────────────────────────────────────────────────────────────────────

/// Win32 input backend. Apocky's primary integration-tested host.
#[derive(Debug)]
pub struct Win32Backend {
    state: InputState,
    events: VecDeque<InputEvent>,
    action_map: ActionMap,
    deadzone: i16,
    per_axis_deadzone: [Option<i16>; GAMEPAD_AXIS_COUNT],
    kill_switch: KillSwitch,
    /// XInput DLL handle (None if no XInput available).
    xinput: Option<XInputDll>,
    /// Per-slot last-known XInput packet number (for change detection).
    last_packet: [DWORD; 4],
    /// Per-slot last-known buttons mask (canonical bit-layout).
    last_buttons: [u16; 4],
    /// Per-slot last-connected flag.
    last_connected: [bool; 4],
    /// Per-key auto-repeat counter (0 = first press, 1+ = repeat).
    repeat_counters: [u8; 256],
}

impl Win32Backend {
    /// Construct from a builder.
    #[must_use]
    pub fn from_builder(builder: InputBackendBuilder) -> Self {
        let (deadzone, per_axis, action_map, kill_switch) = builder.into_parts();
        Self {
            state: InputState::default(),
            events: VecDeque::new(),
            action_map,
            deadzone,
            per_axis_deadzone: per_axis,
            kill_switch,
            xinput: XInputDll::try_load(),
            last_packet: [0; 4],
            last_buttons: [0; 4],
            last_connected: [false; 4],
            repeat_counters: [0; 256],
        }
    }

    /// Returns `true` if XInput is available (for tests + observability).
    #[must_use]
    pub fn has_xinput(&self) -> bool {
        self.xinput.is_some()
    }

    /// Poll all 4 XInput slots, generate events for state changes.
    fn poll_xinput(&mut self) -> Result<usize, InputError> {
        let Some(xi) = &self.xinput else {
            return Ok(0);
        };
        let mut new_events = 0usize;
        for slot in 0u32..4 {
            match xi.get_state(slot)? {
                None => {
                    if self.last_connected[slot as usize] {
                        self.last_connected[slot as usize] = false;
                        if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                            g.clear_all();
                        }
                        self.events
                            .push_back(InputEvent::GamepadDisconnect { slot: slot as u8 });
                        new_events += 1;
                    }
                }
                Some(st) => {
                    let was_connected = self.last_connected[slot as usize];
                    if !was_connected {
                        self.last_connected[slot as usize] = true;
                        if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                            g.connected = true;
                        }
                        self.events
                            .push_back(InputEvent::GamepadConnect { slot: slot as u8 });
                        new_events += 1;
                    }

                    if st.packet_number == self.last_packet[slot as usize] {
                        // Nothing changed.
                        continue;
                    }
                    self.last_packet[slot as usize] = st.packet_number;

                    // Buttons : diff against last_buttons.
                    let canonical = xinput_buttons_to_canonical(st.gamepad.buttons);
                    let prev = self.last_buttons[slot as usize];
                    self.last_buttons[slot as usize] = canonical;

                    let changed = canonical ^ prev;
                    if changed != 0 {
                        let mut bit = 0u16;
                        let mut mask = 1u16;
                        while bit < 16 {
                            if changed & mask != 0 {
                                let pressed = canonical & mask != 0;
                                let button = match bit {
                                    0 => GamepadButton::A,
                                    1 => GamepadButton::B,
                                    2 => GamepadButton::X,
                                    3 => GamepadButton::Y,
                                    4 => GamepadButton::LeftBumper,
                                    5 => GamepadButton::RightBumper,
                                    6 => GamepadButton::Back,
                                    7 => GamepadButton::Start,
                                    8 => GamepadButton::LeftStick,
                                    9 => GamepadButton::RightStick,
                                    10 => GamepadButton::DPadUp,
                                    11 => GamepadButton::DPadDown,
                                    12 => GamepadButton::DPadLeft,
                                    13 => GamepadButton::DPadRight,
                                    14 => GamepadButton::Guide,
                                    _ => break,
                                };
                                if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                                    g.set_button(button, pressed);
                                }
                                self.events.push_back(InputEvent::GamepadButtonChange {
                                    slot: slot as u8,
                                    button,
                                    pressed,
                                });
                                new_events += 1;
                            }
                            bit += 1;
                            mask <<= 1;
                        }
                    }

                    // Axes.
                    let new_axes = [
                        st.gamepad.thumb_lx,
                        st.gamepad.thumb_ly,
                        st.gamepad.thumb_rx,
                        st.gamepad.thumb_ry,
                        i16::from(st.gamepad.left_trigger).saturating_mul(128),
                        i16::from(st.gamepad.right_trigger).saturating_mul(128),
                    ];
                    let axes = [
                        GamepadAxis::LeftStickX,
                        GamepadAxis::LeftStickY,
                        GamepadAxis::RightStickX,
                        GamepadAxis::RightStickY,
                        GamepadAxis::LeftTrigger,
                        GamepadAxis::RightTrigger,
                    ];
                    for (i, axis) in axes.iter().enumerate() {
                        let prev = self
                            .state
                            .gamepads
                            .get(slot as usize)
                            .map_or(0, |g| g.axis(*axis));
                        if prev != new_axes[i] {
                            if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                                g.set_axis(*axis, new_axes[i]);
                            }
                            self.events.push_back(InputEvent::GamepadAxisChange {
                                slot: slot as u8,
                                axis: *axis,
                                value: new_axes[i],
                            });
                            new_events += 1;
                        }
                    }
                }
            }
        }
        Ok(new_events)
    }

    /// Process one raw-input keyboard event (called from the F1 message
    /// pump once `WM_INPUT` arrives). Returns the count of canonical
    /// events generated.
    ///
    /// Args :
    ///   - `vk`    : the `VKey` from `RAWKEYBOARD.VKey`.
    ///   - `flags` : the `Flags` from `RAWKEYBOARD.Flags`. Bit 0 set =
    ///               key-up ; bit 1 set = extended (E0 prefix).
    pub fn process_raw_input_keyboard(&mut self, vk: u16, flags: u16) {
        let extended = flags & RI_KEY_E0 != 0;
        let key_up = flags & RI_KEY_BREAK != 0;
        let code = vk_to_key_code(vk, extended);
        if code == KeyCode::Unknown {
            return;
        }

        if key_up {
            self.state.keys.set(code, false);
            self.repeat_counters[code as usize] = 0;
            self.events.push_back(InputEvent::KeyUp { code });
        } else {
            let was_pressed = self.state.keys.is_pressed(code);
            self.state.keys.set(code, true);
            let repeat_count = if was_pressed {
                let n = self.repeat_counters[code as usize].saturating_add(1);
                self.repeat_counters[code as usize] = n;
                RepeatCount::AutoRepeat(n)
            } else {
                self.repeat_counters[code as usize] = 0;
                RepeatCount::FirstPress
            };
            let event = InputEvent::KeyDown { code, repeat_count };
            self.events.push_back(event);

            // Kill-switch inspection — Esc fires regardless of repeat-count.
            let prior = self.state.grab_state;
            if let Some(reason) = self.kill_switch.on_event(&event, prior) {
                self.release_grab_internal(reason);
            }
        }
    }

    /// Process one raw-input mouse event.
    pub fn process_raw_input_mouse(
        &mut self,
        button_flags: u16,
        wheel_delta: i16,
        delta_x: i32,
        delta_y: i32,
        absolute: bool,
    ) {
        let last_x = self.state.mouse.x;
        let last_y = self.state.mouse.y;
        let (new_x, new_y) = if absolute {
            (delta_x, delta_y)
        } else {
            (
                last_x.saturating_add(delta_x),
                last_y.saturating_add(delta_y),
            )
        };
        let evs = raw_input_mouse_events(
            button_flags,
            wheel_delta,
            last_x,
            last_y,
            new_x,
            new_y,
            button_flags & RI_MOUSE_HWHEEL != 0,
        );
        for ev in evs {
            // Update state alongside.
            match &ev {
                InputEvent::MouseMove { x, y } => {
                    self.state.mouse.x = *x;
                    self.state.mouse.y = *y;
                }
                InputEvent::MouseDown { button, .. } => {
                    self.state.mouse.set_button(*button, true);
                }
                InputEvent::MouseUp { button, .. } => {
                    self.state.mouse.set_button(*button, false);
                }
                InputEvent::Scroll { axis, delta } => {
                    self.state.mouse.accumulate_scroll(*axis, *delta);
                }
                _ => {}
            }
            self.events.push_back(ev);
        }
    }

    /// Notification from the F1 message pump that the OS reported a
    /// session-state change. `locked = true` => `WTS_SESSION_LOCK`
    /// (Win+L pressed). PRIME-DIRECTIVE-honouring.
    pub fn on_session_lock_change(&mut self, locked: bool) {
        let prior = self.state.grab_state;
        if let Some(reason) = self.kill_switch.on_session_lock(locked, prior) {
            self.release_grab_internal(reason);
        }
    }

    /// Notification from the F1 message pump that the OS reported a
    /// session-end (`WM_QUERYENDSESSION` / `WM_ENDSESSION`).
    /// PRIME-DIRECTIVE-honouring.
    pub fn on_session_end(&mut self) {
        let prior = self.state.grab_state;
        if let Some(reason) = self.kill_switch.on_session_end(prior) {
            self.release_grab_internal(reason);
        }
    }

    /// Internal grab-release helper used by both
    /// [`InputBackend::release_grab`] and the kill-switch fire path.
    fn release_grab_internal(&mut self, reason: KillSwitchReason) {
        let prior = self.state.grab_state;
        // Clear OS-side state first. (In a real Win32 build we'd call
        // `ClipCursor(null)` + `ReleaseCapture()` + `ShowCursor(true)`
        // here ; today the worktree compiles those calls behind further
        // FFI declarations the F1 integration provides.)
        self.state.grab_state = Default::default();
        self.state.clear_all_inputs();
        self.repeat_counters = [0; 256];
        self.kill_switch.trigger(reason, prior, self.state.tick);
    }
}

impl InputBackend for Win32Backend {
    fn tick(&mut self) -> Result<usize, InputError> {
        self.state.tick = self.state.tick.saturating_add(1);
        let new_events = self.poll_xinput()?;
        // Apply dead-zones to all connected gamepads.
        for g in &mut self.state.gamepads {
            if g.connected {
                crate::backend::apply_deadzones_to_gamepad(
                    g,
                    self.deadzone,
                    &self.per_axis_deadzone,
                );
            }
        }
        Ok(new_events)
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
        // (Real Win32 build : `ClipCursor`, `SetCapture`, `ShowCursor(false)`
        // inside the F1 window integration. Stubbed here so the backend
        // unit-tests don't depend on a live window handle.)
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

    fn set_gamepad_rumble(&mut self, slot: u8, low: u16, high: u16) -> Result<(), InputError> {
        let Some(xi) = &self.xinput else {
            return Err(InputError::FeatureUnavailable);
        };
        if slot >= 4 {
            return Err(InputError::GamepadSlotOutOfRange { slot, max: 3 });
        }
        xi.set_vibration(u32::from(slot), low, high)
    }

    fn kill_switch(&self) -> &KillSwitch {
        &self.kill_switch
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Win32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_to_key_code_letters() {
        assert_eq!(vk_to_key_code(0x41, false), KeyCode::A);
        assert_eq!(vk_to_key_code(0x57, false), KeyCode::W);
        assert_eq!(vk_to_key_code(0x5A, false), KeyCode::Z);
    }

    #[test]
    fn vk_to_key_code_modifiers() {
        assert_eq!(vk_to_key_code(0xA0, false), KeyCode::LeftShift);
        assert_eq!(vk_to_key_code(0xA1, false), KeyCode::RightShift);
        assert_eq!(vk_to_key_code(0xA2, false), KeyCode::LeftCtrl);
        assert_eq!(vk_to_key_code(0xA3, true), KeyCode::RightCtrl);
    }

    #[test]
    fn vk_to_key_code_arrows() {
        assert_eq!(vk_to_key_code(0x25, true), KeyCode::ArrowLeft);
        assert_eq!(vk_to_key_code(0x26, true), KeyCode::ArrowUp);
        assert_eq!(vk_to_key_code(0x27, true), KeyCode::ArrowRight);
        assert_eq!(vk_to_key_code(0x28, true), KeyCode::ArrowDown);
    }

    #[test]
    fn vk_to_key_code_unknown() {
        // VK_F23 (0x86) is not in the canonical set.
        assert_eq!(vk_to_key_code(0x86, false), KeyCode::Unknown);
    }

    #[test]
    fn vk_to_key_code_enter_vs_keypad_enter() {
        assert_eq!(vk_to_key_code(0x0D, false), KeyCode::Enter);
        assert_eq!(vk_to_key_code(0x0D, true), KeyCode::KPEnter);
    }

    #[test]
    fn xinput_buttons_to_canonical_a() {
        let canonical = xinput_buttons_to_canonical(xinput_btn::A);
        assert_eq!(canonical, 1u16 << GamepadButton::A as u16);
    }

    #[test]
    fn xinput_buttons_to_canonical_combo() {
        let canonical = xinput_buttons_to_canonical(xinput_btn::A | xinput_btn::DPAD_UP);
        let want = (1u16 << GamepadButton::A as u16) | (1u16 << GamepadButton::DPadUp as u16);
        assert_eq!(canonical, want);
    }

    #[test]
    fn xinput_buttons_to_canonical_dpad_directions() {
        for (xi, want) in [
            (xinput_btn::DPAD_UP, GamepadButton::DPadUp),
            (xinput_btn::DPAD_DOWN, GamepadButton::DPadDown),
            (xinput_btn::DPAD_LEFT, GamepadButton::DPadLeft),
            (xinput_btn::DPAD_RIGHT, GamepadButton::DPadRight),
        ] {
            assert_eq!(
                xinput_buttons_to_canonical(xi),
                1u16 << want as u16,
                "xi-bit {xi:#x} -> {want:?}"
            );
        }
    }

    #[test]
    fn raw_input_mouse_events_left_down() {
        let evs = raw_input_mouse_events(RI_MOUSE_LEFT_BUTTON_DOWN, 0, 0, 0, 100, 200, false);
        assert_eq!(evs.len(), 2); // Move + LeftDown
        assert!(matches!(evs[0], InputEvent::MouseMove { x: 100, y: 200 }));
        assert!(matches!(
            evs[1],
            InputEvent::MouseDown {
                button: MouseButton::Left,
                x: 100,
                y: 200
            }
        ));
    }

    #[test]
    fn raw_input_mouse_events_no_movement_no_event() {
        let evs = raw_input_mouse_events(0, 0, 100, 100, 100, 100, false);
        assert!(evs.is_empty());
    }

    #[test]
    fn raw_input_mouse_events_wheel_vertical() {
        // Win32 WHEEL_DELTA = 120 ; +120 = 1 tick up.
        let evs = raw_input_mouse_events(RI_MOUSE_WHEEL, 120, 0, 0, 0, 0, false);
        assert_eq!(evs.len(), 1);
        assert!(matches!(
            evs[0],
            InputEvent::Scroll {
                axis: ScrollAxis::Vertical,
                delta: 1
            }
        ));
    }

    #[test]
    fn raw_input_mouse_events_wheel_horizontal() {
        let evs = raw_input_mouse_events(RI_MOUSE_HWHEEL, -120, 0, 0, 0, 0, true);
        assert_eq!(evs.len(), 1);
        assert!(matches!(
            evs[0],
            InputEvent::Scroll {
                axis: ScrollAxis::Horizontal,
                delta: -1
            }
        ));
    }

    #[test]
    fn backend_constructs_via_builder() {
        let b = Win32Backend::from_builder(InputBackendBuilder::new());
        assert_eq!(b.kind(), BackendKind::Win32);
        assert_eq!(b.current_state().tick, 0);
    }

    #[test]
    fn process_raw_input_keyboard_first_press() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.process_raw_input_keyboard(0x57, 0); // W down, no flags
        let ev = b.poll_events().unwrap();
        assert!(matches!(
            ev,
            InputEvent::KeyDown {
                code: KeyCode::W,
                repeat_count: RepeatCount::FirstPress,
            }
        ));
        assert!(b.current_state().keys.is_pressed(KeyCode::W));
    }

    #[test]
    fn process_raw_input_keyboard_auto_repeat() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.process_raw_input_keyboard(0x57, 0); // first press
        let _ = b.poll_events();
        b.process_raw_input_keyboard(0x57, 0); // OS-driven repeat
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
    fn process_raw_input_keyboard_up_clears_repeat() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.process_raw_input_keyboard(0x57, 0); // down
        b.process_raw_input_keyboard(0x57, 0); // repeat 1
        b.process_raw_input_keyboard(0x57, 0); // repeat 2
        b.process_raw_input_keyboard(0x57, RI_KEY_BREAK); // up
                                                          // Now press again — should be FirstPress not AutoRepeat(3).
        b.process_raw_input_keyboard(0x57, 0);
        let mut last_ev = None;
        while let Some(ev) = b.poll_events() {
            last_ev = Some(ev);
        }
        assert!(matches!(
            last_ev,
            Some(InputEvent::KeyDown {
                code: KeyCode::W,
                repeat_count: RepeatCount::FirstPress,
            })
        ));
    }

    #[test]
    fn esc_press_during_grab_fires_kill_switch() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        assert!(b.current_state().grab_state.is_grabbed());

        b.process_raw_input_keyboard(0x1B, 0); // Esc down
        assert!(
            !b.current_state().grab_state.is_grabbed(),
            "kill-switch must release grab"
        );

        let ks_event = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks_event.reason, KillSwitchReason::EscPressed);
    }

    #[test]
    fn esc_press_without_grab_does_not_fire() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.process_raw_input_keyboard(0x1B, 0);
        assert!(b.poll_kill_switch_events().is_none());
    }

    #[test]
    fn session_lock_during_grab_fires_kill_switch() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.on_session_lock_change(true); // Win+L equivalent
        assert!(!b.current_state().grab_state.is_grabbed());

        let ks_event = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks_event.reason, KillSwitchReason::SessionLockChord);
    }

    #[test]
    fn session_end_during_grab_fires_kill_switch() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.on_session_end();
        assert!(!b.current_state().grab_state.is_grabbed());

        let ks_event = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks_event.reason, KillSwitchReason::SessionEnding);
    }

    #[test]
    fn release_grab_records_application_requested() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.release_grab().unwrap();

        let ks_event = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks_event.reason, KillSwitchReason::ApplicationRequested);
    }

    #[test]
    fn process_raw_input_mouse_move() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.process_raw_input_mouse(0, 0, 50, 50, false);
        assert_eq!(b.current_state().mouse.x, 50);
        assert_eq!(b.current_state().mouse.y, 50);
        let ev = b.poll_events().unwrap();
        assert_eq!(ev, InputEvent::MouseMove { x: 50, y: 50 });
    }

    #[test]
    fn process_raw_input_mouse_button() {
        let mut b = Win32Backend::from_builder(InputBackendBuilder::new());
        b.process_raw_input_mouse(RI_MOUSE_LEFT_BUTTON_DOWN, 0, 0, 0, false);
        assert!(b.current_state().mouse.is_button_pressed(MouseButton::Left));
    }
}
