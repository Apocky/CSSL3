//! Window event-types — the API surface scoped at F1 for F2 to populate.
//!
//! § SCOPE NOTE
//!   F1 (this slice) defines the SHAPE — enums + structs that user-code
//!   matches against. The dispatch logic that fires keyboard / mouse events
//!   lands at F2 (input slice). Win32 close + resize + focus events ARE
//!   wired live at F1 because those are window-lifecycle critical (the
//!   Close event is PRIME-DIRECTIVE-load-bearing).
//!
//! § DESIGN
//!   `WindowEvent` carries a kind-tagged shape so F2..F5 can extend without
//!   breaking match arms (we use `#[non_exhaustive]` on the enums where
//!   future extension is anticipated). Timestamps are explicit `u64`
//!   millis-since-window-open so events are reorderable across threads if
//!   later slices spread the pump across worker threads.

use core::fmt;

/// A single window event.
///
/// Drained from [`crate::Window::pump_events`]. The event-pump returns these
/// in FIFO order ; user-code should drain to empty per frame to keep the
/// queue bounded.
#[derive(Debug, Clone)]
pub struct WindowEvent {
    /// Milliseconds since the window was constructed. Stored as `u64` so
    /// the value is monotonic for the lifetime of the window. Used by F2
    /// for input-debounce + by F3 for audio-sync.
    pub timestamp_ms: u64,

    /// Kind-discriminated payload.
    pub kind: WindowEventKind,
}

/// Kind-discriminated payload on a [`WindowEvent`].
///
/// `#[non_exhaustive]` so F2..F5 can grow this without API breakage.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WindowEventKind {
    /// User requested window close (clicked the X / Alt-F4 / system-menu).
    ///
    /// PRIME-DIRECTIVE : this event MUST be observable by user-code. The
    /// default pump never silently consumes it — see
    /// [`crate::consent::CloseRequestState`].
    Close,

    /// Window was resized. New client-area dimensions in physical pixels.
    Resize { width: u32, height: u32 },

    /// Window gained input focus.
    FocusGain,

    /// Window lost input focus.
    FocusLoss,

    /// A keyboard key was pressed.
    ///
    /// API shape only at F1 ; populated by F2.
    KeyDown {
        key: KeyCode,
        modifiers: ModifierKeys,
        /// `true` if this is an OS-driven repeat (key held).
        repeat: bool,
    },

    /// A keyboard key was released.
    ///
    /// API shape only at F1 ; populated by F2.
    KeyUp {
        key: KeyCode,
        modifiers: ModifierKeys,
    },

    /// Mouse cursor moved within the client area. Coordinates are in
    /// physical pixels relative to the window's top-left.
    MouseMove { x: i32, y: i32 },

    /// A mouse button was pressed.
    MouseDown {
        button: MouseButton,
        x: i32,
        y: i32,
        modifiers: ModifierKeys,
    },

    /// A mouse button was released.
    MouseUp {
        button: MouseButton,
        x: i32,
        y: i32,
        modifiers: ModifierKeys,
    },

    /// Mouse wheel scrolled.
    Scroll {
        delta: ScrollDelta,
        x: i32,
        y: i32,
        modifiers: ModifierKeys,
    },

    /// DPI changed (e.g. window dragged to a different-DPI monitor).
    /// `scale` is the new DPI scale factor (1.0 = 96 DPI baseline).
    DpiChanged { scale: f32 },
}

/// Scroll delta — pixels for trackpads, lines for mouse wheels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDelta {
    /// Line-discrete delta (mouse-wheel ticks). Positive = up / forward.
    Lines { x: f32, y: f32 },
    /// Pixel-precise delta (trackpads). Positive = up / forward.
    Pixels { x: f32, y: f32 },
}

/// Mouse-button discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    /// Out-of-band button index (XButton1+).
    Other(u16),
}

bitflags::bitflags! {
    /// Modifier-key bitset on a keyboard / mouse event.
    ///
    /// `repr(transparent)` over a `u8` so the enum is FFI-friendly for the
    /// future native x86-64 backend (Phase G).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ModifierKeys: u8 {
        /// Either Shift key pressed.
        const SHIFT   = 1 << 0;
        /// Either Ctrl key pressed.
        const CTRL    = 1 << 1;
        /// Either Alt key pressed.
        const ALT     = 1 << 2;
        /// Either Win / Cmd key pressed.
        const SUPER   = 1 << 3;
        /// CapsLock toggled on.
        const CAPS    = 1 << 4;
        /// NumLock toggled on.
        const NUM     = 1 << 5;
    }
}

impl fmt::Display for ModifierKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}

/// USB-HID-style virtual key code, OS-independent.
///
/// `#[non_exhaustive]` so F2 can grow the table without API breakage.
/// The variants cover the standard 105-key keyboard ; OS-specific keys
/// route through [`KeyCode::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyCode {
    // Letters
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
    // Digits (top row)
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    // Function keys
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
    // Navigation
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    // Whitespace / control
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    // Modifiers (released-as-key)
    LShift,
    RShift,
    LCtrl,
    RCtrl,
    LAlt,
    RAlt,
    LSuper,
    RSuper,
    CapsLock,
    NumLock,
    ScrollLock,
    // Punctuation
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Comma,
    Period,
    Slash,
    Grave,
    // Numpad
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadEnter,
    NumpadDecimal,
    // Out-of-band — F2 may extend the table ; until then OS-specific keys
    // surface here with their native scan-code.
    Other(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_code_eq_round_trip() {
        assert_eq!(KeyCode::A, KeyCode::A);
        assert_ne!(KeyCode::A, KeyCode::B);
        assert_eq!(KeyCode::Other(42), KeyCode::Other(42));
        assert_ne!(KeyCode::Other(42), KeyCode::Other(43));
    }

    #[test]
    fn modifier_keys_combine() {
        let m = ModifierKeys::SHIFT | ModifierKeys::CTRL;
        assert!(m.contains(ModifierKeys::SHIFT));
        assert!(m.contains(ModifierKeys::CTRL));
        assert!(!m.contains(ModifierKeys::ALT));
    }

    #[test]
    fn modifier_keys_display_includes_set_flags() {
        let m = ModifierKeys::SHIFT | ModifierKeys::ALT;
        let s = format!("{m}");
        assert!(s.contains("SHIFT"), "display = {s}");
        assert!(s.contains("ALT"), "display = {s}");
    }

    #[test]
    fn mouse_button_other_carries_index() {
        let b = MouseButton::Other(7);
        match b {
            MouseButton::Other(n) => assert_eq!(n, 7),
            _ => panic!("expected Other(7)"),
        }
    }

    #[test]
    fn scroll_delta_lines_and_pixels_are_distinct_variants() {
        let l = ScrollDelta::Lines { x: 0.0, y: 1.0 };
        let p = ScrollDelta::Pixels { x: 0.0, y: 16.0 };
        assert_ne!(l, p);
    }

    #[test]
    fn window_event_close_default_round_trip() {
        let e = WindowEvent {
            timestamp_ms: 0,
            kind: WindowEventKind::Close,
        };
        match e.kind {
            WindowEventKind::Close => (),
            _ => panic!("expected Close"),
        }
    }

    #[test]
    fn window_event_resize_carries_dims() {
        let e = WindowEvent {
            timestamp_ms: 16,
            kind: WindowEventKind::Resize {
                width: 1920,
                height: 1080,
            },
        };
        if let WindowEventKind::Resize { width, height } = e.kind {
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
        } else {
            panic!("expected Resize");
        }
    }

    #[test]
    fn window_event_dpi_change_carries_scale() {
        let e = WindowEvent {
            timestamp_ms: 100,
            kind: WindowEventKind::DpiChanged { scale: 1.5 },
        };
        if let WindowEventKind::DpiChanged { scale } = e.kind {
            assert!((scale - 1.5).abs() < f32::EPSILON);
        } else {
            panic!("expected DpiChanged");
        }
    }
}
