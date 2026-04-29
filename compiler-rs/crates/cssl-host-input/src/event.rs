//! § Input event sum-type.
//!
//! § ROLE
//!   The platform-neutral event vocabulary that source-level CSSLv3 code
//!   consumes. Each per-OS backend translates its native event stream
//!   (XInput packet, evdev `struct input_event`, IOKit `IOHIDValueRef`)
//!   into this canonical sum-type so application code is OS-agnostic.
//!
//! § AUTO-REPEAT contract
//!   [`InputEvent::KeyDown`] carries a `repeat_count : RepeatCount` field
//!   so source code can distinguish first-press from OS auto-repeat fires.
//!   Per the slice landmines : "Win32 fires WM_KEYDOWN repeatedly per OS
//!   auto-repeat ; emit `repeat_count` in InputEvent for compatibility
//!   (spec MUST distinguish 'key first pressed' from 'key auto-repeated')."
//!
//!   - `RepeatCount::FirstPress`        — initial down-edge.
//!   - `RepeatCount::AutoRepeat(n : u8)` — `n`-th OS-driven auto-repeat
//!                                          fire while the key is held.
//!     (`u8` clamped at 255 ; if you're at 255 auto-repeats, frame-rate
//!     is the bigger problem.)
//!
//! § STABILITY
//!   The discriminant ordering is stable from S7-F2 forward. New variants
//!   may be appended at the end ; renumbering breaks downstream consumers
//!   that match by ordinal (e.g., the source-level `match` lowering).

use core::fmt;

// ───────────────────────────────────────────────────────────────────────
// § Key codes.
// ───────────────────────────────────────────────────────────────────────

/// Physical-key identifier in the canonical CSSLv3 input vocabulary.
///
/// Mirrors the W3C UI-Events `KeyboardEvent.code` semantic (physical key
/// position) rather than `key` (the produced character). This is the
/// right abstraction for game input — `WASD` should be `WASD` regardless
/// of keyboard layout.
///
/// 256 distinct codes are reserved (the source-level `KeyState` bitmap
/// is sized to 256 bits = 32 bytes per the slice brief). Each backend
/// translates from its native scancode to one of these values ; codes
/// outside the canonical set lower to [`KeyCode::Unknown`] and are
/// dropped on the floor (event suppressed) per the §1 PRIME-DIRECTIVE
/// "no surveillance" rule — we don't accumulate undecoded scancodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum KeyCode {
    Unknown = 0,
    // Letters (A..Z map to canonical positions in QWERTY layout).
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    // Number row (top of keyboard, NOT keypad).
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    // Function keys.
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
    F11,
    F12,
    // Whitespace + control.
    Space,
    Tab,
    Enter,
    Escape,
    Backspace,
    // Modifiers.
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftMeta,
    RightMeta,
    // Arrows + navigation.
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    // Punctuation.
    Comma,
    Period,
    Slash,
    Semicolon,
    Quote,
    Backquote,
    LeftBracket,
    RightBracket,
    Backslash,
    Minus,
    Equal,
    // Keypad.
    KP0,
    KP1,
    KP2,
    KP3,
    KP4,
    KP5,
    KP6,
    KP7,
    KP8,
    KP9,
    KPAdd,
    KPSub,
    KPMul,
    KPDiv,
    KPEnter,
    KPDecimal,
    // Caps lock + locks.
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,
}

impl KeyCode {
    /// Total count of canonical key codes (size of the `KeyState`
    /// bitmap — see [`crate::state::KEYBOARD_KEY_COUNT`]).
    pub const COUNT: usize = 256;

    /// Returns `true` if the key is one of the modifier keys
    /// (Shift / Ctrl / Alt / Meta — left or right).
    #[must_use]
    pub fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::LeftShift
                | Self::RightShift
                | Self::LeftCtrl
                | Self::RightCtrl
                | Self::LeftAlt
                | Self::RightAlt
                | Self::LeftMeta
                | Self::RightMeta
        )
    }

    /// Returns `true` if pressing this key triggers a §1 PRIME-DIRECTIVE
    /// kill-switch unbind (Esc unconditionally ; Win+L is the chord
    /// detected by the OS lock-screen — handled separately in
    /// `kill_switch::KillSwitch::on_chord`).
    #[must_use]
    pub fn is_kill_switch(self) -> bool {
        matches!(self, Self::Escape)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Mouse buttons + scroll axes.
// ───────────────────────────────────────────────────────────────────────

/// Mouse button identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
    X1 = 3,
    X2 = 4,
}

impl MouseButton {
    /// Total count of canonical mouse buttons.
    pub const COUNT: usize = 5;

    /// Convert from a 0..=4 ordinal (the on-the-wire u8 representation).
    #[must_use]
    pub fn from_ordinal(ord: u8) -> Option<Self> {
        match ord {
            0 => Some(Self::Left),
            1 => Some(Self::Right),
            2 => Some(Self::Middle),
            3 => Some(Self::X1),
            4 => Some(Self::X2),
            _ => None,
        }
    }
}

/// Scroll axis identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ScrollAxis {
    Vertical = 0,
    Horizontal = 1,
}

// ───────────────────────────────────────────────────────────────────────
// § Gamepad buttons + axes.
// ───────────────────────────────────────────────────────────────────────

/// Gamepad button identifier (XInput-aligned vocabulary ; evdev / IOKit
/// translate from their native button code into this canonical enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GamepadButton {
    /// Bottom face button (Xbox A / PS Cross / Switch B).
    A = 0,
    /// Right face button (Xbox B / PS Circle / Switch A).
    B = 1,
    /// Left face button (Xbox X / PS Square / Switch Y).
    X = 2,
    /// Top face button (Xbox Y / PS Triangle / Switch X).
    Y = 3,
    LeftBumper = 4,
    RightBumper = 5,
    Back = 6,
    Start = 7,
    LeftStick = 8,
    RightStick = 9,
    DPadUp = 10,
    DPadDown = 11,
    DPadLeft = 12,
    DPadRight = 13,
    /// Xbox guide / PS home / Switch home button.
    Guide = 14,
}

impl GamepadButton {
    /// Total count of canonical gamepad buttons.
    pub const COUNT: usize = 15;
}

/// Gamepad analog axis identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GamepadAxis {
    LeftStickX = 0,
    LeftStickY = 1,
    RightStickX = 2,
    RightStickY = 3,
    LeftTrigger = 4,
    RightTrigger = 5,
}

impl GamepadAxis {
    /// Total count of canonical gamepad axes.
    pub const COUNT: usize = 6;
}

// ───────────────────────────────────────────────────────────────────────
// § Auto-repeat counter.
// ───────────────────────────────────────────────────────────────────────

/// Auto-repeat discriminator on `KeyDown` events.
///
/// Per the slice landmines, source-level CSSLv3 MUST distinguish
/// "key first pressed" (an actual user-driven down-edge) from "key
/// auto-repeated" (the OS firing repeated WM_KEYDOWN per its
/// auto-repeat-rate setting). UI code typically treats only
/// [`RepeatCount::FirstPress`] as a meaningful action ; text-input code
/// honours both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RepeatCount {
    /// Initial down-edge — user-driven press.
    FirstPress,
    /// `n`-th OS-driven auto-repeat fire (1-indexed, clamped at 255).
    AutoRepeat(u8),
}

impl RepeatCount {
    /// Returns `true` if this is the first press (not an auto-repeat).
    #[must_use]
    pub fn is_first_press(self) -> bool {
        matches!(self, Self::FirstPress)
    }

    /// Returns the `n`-th auto-repeat number, or `0` for the first press.
    #[must_use]
    pub fn count(self) -> u8 {
        match self {
            Self::FirstPress => 0,
            Self::AutoRepeat(n) => n,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § The InputEvent sum-type.
// ───────────────────────────────────────────────────────────────────────

/// Slot index for a connected gamepad : 0..[`GAMEPAD_SLOT_COUNT`] - 1.
///
/// On Win32 (XInput) `slot` is the controller index 0..3 (XInput's
/// 4-controller cap). On Linux (evdev) and macOS (IOKit) the slot is
/// allocated by connection-order ; up to 16 simultaneous gamepads
/// (clamped at [`crate::state::GAMEPAD_SLOT_COUNT`]).
///
/// [`GAMEPAD_SLOT_COUNT`]: crate::state::GAMEPAD_SLOT_COUNT
pub type GamepadSlot = u8;

/// Frame-coherent input event, OS-neutral.
///
/// Backends accumulate native events into this canonical form and expose
/// them via [`crate::backend::InputBackend::poll_events`] (pull-style)
/// or via the F1 ↔ F2 shared event-loop (push-style — see
/// [`crate::window_integration`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    /// Key transitioned to down-state. `repeat_count` discriminates
    /// first-press from OS auto-repeat (per the slice landmines).
    KeyDown {
        code: KeyCode,
        repeat_count: RepeatCount,
    },
    /// Key transitioned to up-state.
    KeyUp { code: KeyCode },
    /// Mouse pointer moved to absolute window-coordinate (`x`, `y`).
    /// Coordinates are in pixels, top-left origin (matches Win32 +
    /// X11 + macOS NSWindow conventions on backbuffer).
    MouseMove { x: i32, y: i32 },
    /// Mouse button transitioned to down-state at (`x`, `y`).
    MouseDown { button: MouseButton, x: i32, y: i32 },
    /// Mouse button transitioned to up-state at (`x`, `y`).
    MouseUp { button: MouseButton, x: i32, y: i32 },
    /// Scroll wheel moved by `delta` units along `axis`. Units are
    /// "ticks" (Win32 `WHEEL_DELTA = 120`-per-tick is divided ;
    /// evdev `REL_WHEEL` is integer ; IOKit values are decoded).
    Scroll { axis: ScrollAxis, delta: i32 },
    /// Gamepad connected at the given slot.
    GamepadConnect { slot: GamepadSlot },
    /// Gamepad disconnected from the given slot.
    GamepadDisconnect { slot: GamepadSlot },
    /// Gamepad analog axis changed. `value` is normalized to
    /// `i16::MIN..=i16::MAX` for stick axes (XInput convention) ; for
    /// trigger axes `value` is `0..=i16::MAX` (negative half unused).
    GamepadAxisChange {
        slot: GamepadSlot,
        axis: GamepadAxis,
        value: i16,
    },
    /// Gamepad button transitioned to a new pressed-state.
    GamepadButtonChange {
        slot: GamepadSlot,
        button: GamepadButton,
        pressed: bool,
    },
}

impl InputEvent {
    /// Returns `true` if this event is keyboard-related.
    #[must_use]
    pub fn is_keyboard(&self) -> bool {
        matches!(self, Self::KeyDown { .. } | Self::KeyUp { .. })
    }

    /// Returns `true` if this event is mouse-related.
    #[must_use]
    pub fn is_mouse(&self) -> bool {
        matches!(
            self,
            Self::MouseMove { .. }
                | Self::MouseDown { .. }
                | Self::MouseUp { .. }
                | Self::Scroll { .. }
        )
    }

    /// Returns `true` if this event is gamepad-related.
    #[must_use]
    pub fn is_gamepad(&self) -> bool {
        matches!(
            self,
            Self::GamepadConnect { .. }
                | Self::GamepadDisconnect { .. }
                | Self::GamepadAxisChange { .. }
                | Self::GamepadButtonChange { .. }
        )
    }

    /// Returns `true` if this event triggers a §1 PRIME-DIRECTIVE
    /// kill-switch unbind (Esc down-edge — note that the Win+L chord
    /// is detected by the OS itself via `RegisterHotKey`-style
    /// machinery in [`crate::kill_switch`], not by inspecting an
    /// `InputEvent` since the chord may never reach the application).
    #[must_use]
    pub fn is_kill_switch(&self) -> bool {
        matches!(
            self,
            Self::KeyDown {
                code: KeyCode::Escape,
                ..
            }
        )
    }
}

impl fmt::Display for InputEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyDown { code, repeat_count } => {
                write!(f, "KeyDown({code:?}, repeat={})", repeat_count.count())
            }
            Self::KeyUp { code } => write!(f, "KeyUp({code:?})"),
            Self::MouseMove { x, y } => write!(f, "MouseMove({x}, {y})"),
            Self::MouseDown { button, x, y } => {
                write!(f, "MouseDown({button:?}, {x}, {y})")
            }
            Self::MouseUp { button, x, y } => {
                write!(f, "MouseUp({button:?}, {x}, {y})")
            }
            Self::Scroll { axis, delta } => write!(f, "Scroll({axis:?}, {delta})"),
            Self::GamepadConnect { slot } => write!(f, "GamepadConnect({slot})"),
            Self::GamepadDisconnect { slot } => write!(f, "GamepadDisconnect({slot})"),
            Self::GamepadAxisChange { slot, axis, value } => {
                write!(f, "GamepadAxisChange({slot}, {axis:?}, {value})")
            }
            Self::GamepadButtonChange {
                slot,
                button,
                pressed,
            } => write!(f, "GamepadButtonChange({slot}, {button:?}, {pressed})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_code_count_fits_byte() {
        // The source-level KeyState bitmap is 256 bits = 32 bytes.
        // KeyCode discriminants must fit u8.
        assert_eq!(KeyCode::COUNT, 256);
        let unknown = KeyCode::Unknown as u8;
        let pause = KeyCode::Pause as u8;
        assert_eq!(unknown, 0);
        assert!(pause < 128, "well below u8::MAX, room for future variants");
    }

    #[test]
    fn modifier_classification() {
        assert!(KeyCode::LeftShift.is_modifier());
        assert!(KeyCode::RightCtrl.is_modifier());
        assert!(KeyCode::LeftMeta.is_modifier());
        assert!(!KeyCode::Space.is_modifier());
        assert!(!KeyCode::Escape.is_modifier());
    }

    #[test]
    fn kill_switch_classification() {
        assert!(KeyCode::Escape.is_kill_switch());
        assert!(!KeyCode::Space.is_kill_switch());
        assert!(!KeyCode::A.is_kill_switch());
    }

    #[test]
    fn mouse_button_round_trip() {
        for ord in 0..=4u8 {
            let btn = MouseButton::from_ordinal(ord).unwrap();
            assert_eq!(btn as u8, ord);
        }
        assert!(MouseButton::from_ordinal(5).is_none());
    }

    #[test]
    fn gamepad_button_count_matches() {
        // Aligns with the GAMEPAD_BUTTON_COUNT in state.rs.
        assert_eq!(GamepadButton::COUNT, 15);
    }

    #[test]
    fn gamepad_axis_count_matches() {
        assert_eq!(GamepadAxis::COUNT, 6);
    }

    #[test]
    fn repeat_count_first_press() {
        let r = RepeatCount::FirstPress;
        assert!(r.is_first_press());
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn repeat_count_auto_repeat() {
        let r = RepeatCount::AutoRepeat(5);
        assert!(!r.is_first_press());
        assert_eq!(r.count(), 5);
    }

    #[test]
    fn event_classification() {
        let ev = InputEvent::KeyDown {
            code: KeyCode::Space,
            repeat_count: RepeatCount::FirstPress,
        };
        assert!(ev.is_keyboard());
        assert!(!ev.is_mouse());
        assert!(!ev.is_gamepad());
        assert!(!ev.is_kill_switch());

        let ev_kill = InputEvent::KeyDown {
            code: KeyCode::Escape,
            repeat_count: RepeatCount::FirstPress,
        };
        assert!(ev_kill.is_kill_switch());

        let mouse_ev = InputEvent::MouseMove { x: 10, y: 20 };
        assert!(mouse_ev.is_mouse());
        assert!(!mouse_ev.is_keyboard());

        let gp_ev = InputEvent::GamepadConnect { slot: 0 };
        assert!(gp_ev.is_gamepad());
    }

    #[test]
    fn event_display_smoke() {
        let ev = InputEvent::KeyDown {
            code: KeyCode::A,
            repeat_count: RepeatCount::FirstPress,
        };
        let s = format!("{ev}");
        assert!(s.contains("KeyDown"));
        assert!(s.contains('A'));
        assert!(s.contains("repeat=0"));
    }
}
