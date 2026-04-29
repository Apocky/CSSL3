//! § Frame-coherent input state snapshot.
//!
//! § ROLE
//!
//!   Source-level CSSLv3 code reads input via two complementary
//!   abstractions :
//!
//!   ```text
//!   1. Event stream ([crate::event::InputEvent]) — push-style,
//!      every state-change is observable in the order it occurred.
//!      Suits text-input + UI handlers + gameplay-action hooks.
//!   2. State snapshot (this module) — pull-style, the current
//!      held-down set as of the most-recent backend tick. Suits
//!      tight game loops + simulation code that reads "is W held
//!      right now" rather than "did W go down 2 frames ago".
//!   ```
//!
//!   The backend maintains the snapshot incrementally as events arrive —
//!   `KeyDown` sets a bit, `KeyUp` clears it. Source-level code reads
//!   [`InputState`] without further synchronization (the snapshot is
//!   updated atomically by the backend before being handed out — see
//!   [`crate::backend::InputBackend::current_state`]).
//!
//! § BITMAP LAYOUT
//!
//!   The keyboard bitmap is 256 bits = 32 bytes ([`KEYBOARD_KEY_COUNT`]).
//!   Each [`crate::event::KeyCode`] discriminant is the bit-index. Per
//!   the slice brief : "keyboard-key bitmap (256-key)".
//!
//!   Mouse buttons are an 8-bit mask ([`crate::event::MouseButton::COUNT`]
//!   ≤ 5 today, room for X3..X4 + horizontal-buttons via future variants).
//!
//!   Gamepad slots are an array of [`GamepadState`] sized to
//!   [`GAMEPAD_SLOT_COUNT`] = 16 (XInput exposes 4 ; Linux + macOS expose
//!   up to 16 ; we size the array uniformly).

use crate::event::{GamepadAxis, GamepadButton, KeyCode, MouseButton, ScrollAxis};

// ───────────────────────────────────────────────────────────────────────
// § Sizing constants.
// ───────────────────────────────────────────────────────────────────────

/// Number of canonical keyboard keys tracked = 256.
///
/// Per the slice brief : "keyboard-key bitmap (256-key)". The bitmap
/// is sized so every [`KeyCode`] discriminant fits unambiguously (today
/// ~100 variants are populated — the rest of the 256 bits are reserved
/// for IME / dead-key / system / future-vendor extensions).
pub const KEYBOARD_KEY_COUNT: usize = 256;

/// Number of mouse buttons tracked = 8.
///
/// Sized to one byte for cheap mask operations ; today
/// [`MouseButton::COUNT`] = 5 ; the upper 3 bits are reserved.
pub const MOUSE_BUTTON_COUNT: usize = 8;

/// Number of simultaneous gamepad slots tracked = 16.
///
/// XInput's hard cap is 4 ; Linux evdev / macOS IOKit can address more
/// (industrial gamepad farms, multi-arcade-cabinet setups). We size
/// uniformly at 16 so the same `InputState` struct works on every host.
pub const GAMEPAD_SLOT_COUNT: usize = 16;

/// Per-gamepad axis count = 6 ([`GamepadAxis::COUNT`]).
pub const GAMEPAD_AXIS_COUNT: usize = GamepadAxis::COUNT;

/// Per-gamepad button count = 15 ([`GamepadButton::COUNT`]) ; rounded
/// up to 16 so the bitmap is one `u16`.
pub const GAMEPAD_BUTTON_COUNT: usize = 16;

// ───────────────────────────────────────────────────────────────────────
// § Keyboard state.
// ───────────────────────────────────────────────────────────────────────

/// Held-down state of every tracked keyboard key.
///
/// Bit `i` corresponds to the [`KeyCode`] whose discriminant ordinal is
/// `i` (so `bits[KeyCode::Escape as usize]` is the Esc held-state).
#[derive(Clone, Debug)]
pub struct KeyState {
    /// 256-bit bitmap, packed as 32 `u8`s. Little-endian-by-byte
    /// (bit 0 of byte 0 = `KeyCode::Unknown` = 0).
    bits: [u8; KEYBOARD_KEY_COUNT / 8],
}

impl Default for KeyState {
    fn default() -> Self {
        Self {
            bits: [0; KEYBOARD_KEY_COUNT / 8],
        }
    }
}

impl KeyState {
    /// Returns `true` if the given key is currently held down.
    #[must_use]
    pub fn is_pressed(&self, code: KeyCode) -> bool {
        let idx = code as usize;
        let byte = idx / 8;
        let bit = idx % 8;
        (self.bits[byte] & (1 << bit)) != 0
    }

    /// Sets the held-down state for the given key.
    ///
    /// Called by the backend on `KeyDown` / `KeyUp` event arrival ; not
    /// part of the source-level surface (which is read-only).
    pub fn set(&mut self, code: KeyCode, pressed: bool) {
        let idx = code as usize;
        let byte = idx / 8;
        let bit = idx % 8;
        if pressed {
            self.bits[byte] |= 1 << bit;
        } else {
            self.bits[byte] &= !(1 << bit);
        }
    }

    /// Clears every held-state bit.
    ///
    /// Called by the kill-switch on Esc / Win+L unbind so the application
    /// doesn't see "stuck" keys after grab-release.
    pub fn clear_all(&mut self) {
        self.bits = [0; KEYBOARD_KEY_COUNT / 8];
    }

    /// Returns the count of currently-held-down keys (popcnt).
    #[must_use]
    pub fn pressed_count(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Mouse state.
// ───────────────────────────────────────────────────────────────────────

/// Mouse pointer + button + scroll state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseState {
    /// Absolute window-space pixel position, top-left origin.
    pub x: i32,
    /// Absolute window-space pixel position, top-left origin.
    pub y: i32,
    /// Bitmask of held-down mouse buttons. Bit `i` = `MouseButton`
    /// whose discriminant ordinal is `i`.
    pub buttons: u8,
    /// Accumulated scroll delta along [`ScrollAxis::Vertical`] since
    /// the last `tick`. Cleared per tick by the backend.
    pub scroll_vertical: i32,
    /// Accumulated scroll delta along [`ScrollAxis::Horizontal`] since
    /// the last `tick`. Cleared per tick by the backend.
    pub scroll_horizontal: i32,
}

impl MouseState {
    /// Returns `true` if the given mouse button is currently held down.
    #[must_use]
    pub fn is_button_pressed(&self, button: MouseButton) -> bool {
        let bit = button as u8;
        (self.buttons & (1 << bit)) != 0
    }

    /// Sets the held-down state for the given mouse button.
    pub fn set_button(&mut self, button: MouseButton, pressed: bool) {
        let bit = button as u8;
        if pressed {
            self.buttons |= 1 << bit;
        } else {
            self.buttons &= !(1 << bit);
        }
    }

    /// Accumulates a scroll delta along the given axis.
    pub fn accumulate_scroll(&mut self, axis: ScrollAxis, delta: i32) {
        match axis {
            ScrollAxis::Vertical => {
                self.scroll_vertical = self.scroll_vertical.saturating_add(delta);
            }
            ScrollAxis::Horizontal => {
                self.scroll_horizontal = self.scroll_horizontal.saturating_add(delta);
            }
        }
    }

    /// Clears the accumulated scroll-deltas (called per-tick by backend).
    pub fn clear_scroll(&mut self) {
        self.scroll_vertical = 0;
        self.scroll_horizontal = 0;
    }

    /// Clears the entire state — used by kill-switch unbind.
    pub fn clear_all(&mut self) {
        *self = Self::default();
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Gamepad state.
// ───────────────────────────────────────────────────────────────────────

/// Per-gamepad-slot state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GamepadState {
    /// `true` if a controller is currently connected at this slot.
    pub connected: bool,
    /// Per-axis values. Stick axes: `i16::MIN..=i16::MAX` ; trigger
    /// axes: `0..=i16::MAX`.
    pub axes: [i16; GAMEPAD_AXIS_COUNT],
    /// Bitmask of held-down buttons. Bit `i` = `GamepadButton` whose
    /// discriminant ordinal is `i`.
    pub buttons: u16,
}

impl Default for GamepadState {
    fn default() -> Self {
        Self {
            connected: false,
            axes: [0; GAMEPAD_AXIS_COUNT],
            buttons: 0,
        }
    }
}

impl GamepadState {
    /// Returns `true` if the given button is currently held down.
    #[must_use]
    pub fn is_button_pressed(&self, button: GamepadButton) -> bool {
        let bit = button as u16;
        (self.buttons & (1u16 << bit)) != 0
    }

    /// Sets the held-down state for the given button.
    pub fn set_button(&mut self, button: GamepadButton, pressed: bool) {
        let bit = button as u16;
        if pressed {
            self.buttons |= 1u16 << bit;
        } else {
            self.buttons &= !(1u16 << bit);
        }
    }

    /// Returns the current value for the given axis.
    #[must_use]
    pub fn axis(&self, axis: GamepadAxis) -> i16 {
        self.axes[axis as usize]
    }

    /// Sets the value for the given axis.
    pub fn set_axis(&mut self, axis: GamepadAxis, value: i16) {
        self.axes[axis as usize] = value;
    }

    /// Apply a dead-zone : axis values whose magnitude is below `dz` are
    /// clamped to zero. Returns the clamped value (also writes back).
    ///
    /// Per the slice landmines : "Gamepad axis dead-zones : standard
    /// 8-bit cardinal (8000 / 32767) ; expose as configurable." Default
    /// dead-zone is 8000 (≈ 24.4 % of full range) — see
    /// [`crate::backend::InputBackendBuilder::with_gamepad_deadzone`].
    pub fn apply_deadzone(&mut self, axis: GamepadAxis, dz: i16) -> i16 {
        let v = self.axes[axis as usize];
        let dz_pos = dz.unsigned_abs();
        if v.unsigned_abs() < dz_pos {
            self.axes[axis as usize] = 0;
            0
        } else {
            v
        }
    }

    /// Clears the entire state — called when controller disconnects or
    /// the kill-switch unbinds.
    pub fn clear_all(&mut self) {
        *self = Self::default();
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Grab state.
// ───────────────────────────────────────────────────────────────────────

/// State of input grab — i.e. whether the cursor / keyboard are confined
/// to the host window.
///
/// Per the slice brief : "kill-switch-honoring — Esc + Win+L MUST always
/// exit input-grab mode." The state is observable to the user (per
/// §5 CONSENT-ARCHITECTURE — "if the system observes, it can be observed
/// back") via [`InputState::grab_state`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrabState {
    /// `true` if the OS cursor is confined (Win32 `ClipCursor`,
    /// X11 grab-pointer, macOS `CGEventTapCreate`).
    pub cursor_locked: bool,
    /// `true` if keyboard focus is captured exclusively (Win32
    /// `SetCapture`, etc.).
    pub keyboard_captured: bool,
    /// `true` if the cursor is hidden via the host (independent of
    /// grab — some apps want hidden + free, or visible + locked).
    pub cursor_hidden: bool,
}

impl GrabState {
    /// Returns `true` if any kind of input is currently grabbed —
    /// the §1 PRIME-DIRECTIVE kill-switch fires only when `is_grabbed()`
    /// is true (no grab to release otherwise).
    #[must_use]
    pub fn is_grabbed(&self) -> bool {
        self.cursor_locked || self.keyboard_captured
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Top-level frame snapshot.
// ───────────────────────────────────────────────────────────────────────

/// Frame-coherent snapshot of the entire input subsystem.
#[derive(Clone, Debug)]
pub struct InputState {
    /// Per-key held-down bitmap.
    pub keys: KeyState,
    /// Mouse pointer + buttons + accumulated scroll.
    pub mouse: MouseState,
    /// Per-slot gamepad state.
    pub gamepads: [GamepadState; GAMEPAD_SLOT_COUNT],
    /// Grab + cursor-visibility state.
    pub grab_state: GrabState,
    /// Monotonically-increasing tick counter — incremented by the
    /// backend on each `tick` call. Useful for detecting whether the
    /// snapshot has advanced since the last application read.
    pub tick: u64,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            keys: KeyState::default(),
            mouse: MouseState::default(),
            gamepads: [GamepadState::default(); GAMEPAD_SLOT_COUNT],
            grab_state: GrabState::default(),
            tick: 0,
        }
    }
}

impl InputState {
    /// Returns the gamepad state at the given slot, or `None` if `slot`
    /// is out of range.
    #[must_use]
    pub fn gamepad(&self, slot: usize) -> Option<&GamepadState> {
        self.gamepads.get(slot)
    }

    /// Returns the count of currently-connected gamepads.
    #[must_use]
    pub fn connected_gamepad_count(&self) -> usize {
        self.gamepads.iter().filter(|g| g.connected).count()
    }

    /// Clears every input state — called by the kill-switch on
    /// Esc / Win+L unbind so no "stuck" inputs leak through grab-release.
    /// Preserves the `tick` counter (monotonicity invariant).
    pub fn clear_all_inputs(&mut self) {
        self.keys.clear_all();
        self.mouse.clear_all();
        for g in &mut self.gamepads {
            g.clear_all();
        }
        // grab_state cleared separately by the kill-switch (the actual
        // OS-level grab-release call) — clear_all_inputs does NOT touch
        // grab_state because grab-release is a sequence-of-steps :
        // [release-OS-grab → clear-inputs → fire-kill-switch-event].
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_state_set_and_query() {
        let mut s = KeyState::default();
        assert!(!s.is_pressed(KeyCode::A));
        s.set(KeyCode::A, true);
        assert!(s.is_pressed(KeyCode::A));
        assert!(!s.is_pressed(KeyCode::B));
        s.set(KeyCode::A, false);
        assert!(!s.is_pressed(KeyCode::A));
    }

    #[test]
    fn key_state_high_index_works() {
        let mut s = KeyState::default();
        // KeyCode::Pause is one of the highest-numbered variants.
        s.set(KeyCode::Pause, true);
        assert!(s.is_pressed(KeyCode::Pause));
        assert!(!s.is_pressed(KeyCode::A));
    }

    #[test]
    fn key_state_pressed_count() {
        let mut s = KeyState::default();
        assert_eq!(s.pressed_count(), 0);
        s.set(KeyCode::A, true);
        s.set(KeyCode::W, true);
        s.set(KeyCode::S, true);
        assert_eq!(s.pressed_count(), 3);
        s.clear_all();
        assert_eq!(s.pressed_count(), 0);
    }

    #[test]
    fn mouse_state_button_mask() {
        let mut m = MouseState::default();
        assert!(!m.is_button_pressed(MouseButton::Left));
        m.set_button(MouseButton::Left, true);
        assert!(m.is_button_pressed(MouseButton::Left));
        m.set_button(MouseButton::Right, true);
        assert!(m.is_button_pressed(MouseButton::Left));
        assert!(m.is_button_pressed(MouseButton::Right));
        m.set_button(MouseButton::Left, false);
        assert!(!m.is_button_pressed(MouseButton::Left));
        assert!(m.is_button_pressed(MouseButton::Right));
    }

    #[test]
    fn mouse_state_scroll_accumulate() {
        let mut m = MouseState::default();
        m.accumulate_scroll(ScrollAxis::Vertical, 1);
        m.accumulate_scroll(ScrollAxis::Vertical, 2);
        assert_eq!(m.scroll_vertical, 3);
        m.accumulate_scroll(ScrollAxis::Horizontal, -1);
        assert_eq!(m.scroll_horizontal, -1);
        m.clear_scroll();
        assert_eq!(m.scroll_vertical, 0);
        assert_eq!(m.scroll_horizontal, 0);
    }

    #[test]
    fn gamepad_state_button_set() {
        let mut g = GamepadState::default();
        assert!(!g.is_button_pressed(GamepadButton::A));
        g.set_button(GamepadButton::A, true);
        assert!(g.is_button_pressed(GamepadButton::A));
        g.set_button(GamepadButton::Y, true);
        assert!(g.is_button_pressed(GamepadButton::A));
        assert!(g.is_button_pressed(GamepadButton::Y));
        assert!(!g.is_button_pressed(GamepadButton::B));
    }

    #[test]
    fn gamepad_state_axis_set() {
        let mut g = GamepadState::default();
        g.set_axis(GamepadAxis::LeftStickX, 16384);
        assert_eq!(g.axis(GamepadAxis::LeftStickX), 16384);
        assert_eq!(g.axis(GamepadAxis::LeftStickY), 0);
    }

    #[test]
    fn gamepad_deadzone_below() {
        let mut g = GamepadState::default();
        g.set_axis(GamepadAxis::LeftStickX, 5000);
        let v = g.apply_deadzone(GamepadAxis::LeftStickX, 8000);
        assert_eq!(v, 0);
        assert_eq!(g.axis(GamepadAxis::LeftStickX), 0);
    }

    #[test]
    fn gamepad_deadzone_above() {
        let mut g = GamepadState::default();
        g.set_axis(GamepadAxis::LeftStickX, 16000);
        let v = g.apply_deadzone(GamepadAxis::LeftStickX, 8000);
        assert_eq!(v, 16000);
        assert_eq!(g.axis(GamepadAxis::LeftStickX), 16000);
    }

    #[test]
    fn gamepad_deadzone_negative() {
        let mut g = GamepadState::default();
        g.set_axis(GamepadAxis::LeftStickX, -7000);
        let v = g.apply_deadzone(GamepadAxis::LeftStickX, 8000);
        assert_eq!(v, 0);
    }

    #[test]
    fn grab_state_is_grabbed() {
        let mut gs = GrabState::default();
        assert!(!gs.is_grabbed());
        gs.cursor_locked = true;
        assert!(gs.is_grabbed());
        gs.cursor_locked = false;
        gs.keyboard_captured = true;
        assert!(gs.is_grabbed());
    }

    #[test]
    fn input_state_default() {
        let s = InputState::default();
        assert_eq!(s.tick, 0);
        assert_eq!(s.connected_gamepad_count(), 0);
        assert!(!s.grab_state.is_grabbed());
        assert_eq!(s.keys.pressed_count(), 0);
    }

    #[test]
    fn input_state_clear_all_preserves_tick() {
        let mut s = InputState {
            tick: 42,
            ..InputState::default()
        };
        s.keys.set(KeyCode::A, true);
        s.mouse.x = 100;
        s.gamepads[0].connected = true;
        s.gamepads[0].set_button(GamepadButton::A, true);

        s.clear_all_inputs();

        assert_eq!(s.tick, 42, "tick monotonicity preserved across clear");
        assert!(!s.keys.is_pressed(KeyCode::A));
        assert_eq!(s.mouse.x, 0);
        assert!(!s.gamepads[0].connected);
    }

    #[test]
    fn gamepad_slot_count_is_16() {
        let s = InputState::default();
        assert_eq!(s.gamepads.len(), 16);
    }
}
