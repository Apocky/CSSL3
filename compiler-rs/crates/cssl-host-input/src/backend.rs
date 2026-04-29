//! § InputBackend trait + builder.
//!
//! § ROLE
//!   The cross-OS interface that source-level CSSLv3 code targets. Each
//!   per-OS module (`win32`, `linux`, `macos`) provides an impl ; the
//!   `stub` module provides a no-op fallback.
//!
//! § BUILDER PATTERN
//!   Per the slice brief, the backend is configurable :
//!     - dead-zone (per-axis or global)
//!     - input-mapping table
//!     - kill-switch (PRIME-DIRECTIVE-required ; non-overridable)
//!   The [`InputBackendBuilder`] provides typed `with_*` methods. The
//!   `kill_switch` field is structurally required (you cannot construct
//!   an `InputBackend` without one) — this is the static guarantee that
//!   §6 PRIME-DIRECTIVE SCOPE ("no flag can disable the kill-switch")
//!   is upheld at the type level.

use crate::event::InputEvent;
use crate::kill_switch::{KillSwitch, KillSwitchEvent};
use crate::mapping::ActionMap;
use crate::state::{GamepadState, GrabState, InputState, GAMEPAD_AXIS_COUNT};
use thiserror::Error;

// ───────────────────────────────────────────────────────────────────────
// § InputBackend trait.
// ───────────────────────────────────────────────────────────────────────

/// The cross-OS input backend interface.
///
/// # PRIME-DIRECTIVE invariants
///
/// Implementations MUST :
/// 1. Honour the kill-switch on every Esc / session-lock / session-end
///    (§1 PROHIBITIONS § entrapment). No bypass-flag exists.
/// 2. Never log keystroke / mouse-position / gamepad data outside the
///    in-process audit (§1 PROHIBITIONS § surveillance).
/// 3. Surface every grab / release transition via [`Self::grab_state`]
///    so source-level code can observe what the backend is doing
///    (§5 CONSENT-ARCHITECTURE).
pub trait InputBackend {
    /// Run one tick : poll OS input sources, accumulate events,
    /// update the [`InputState`] snapshot, evaluate the kill-switch.
    ///
    /// Returns the count of new events accumulated this tick.
    fn tick(&mut self) -> Result<usize, InputError>;

    /// Drain the accumulated event queue. Returns `Some(event)`
    /// repeatedly until empty, then `None`.
    fn poll_events(&mut self) -> Option<InputEvent>;

    /// Drain the kill-switch audit queue. Returns `Some(event)`
    /// repeatedly until empty, then `None`.
    fn poll_kill_switch_events(&mut self) -> Option<KillSwitchEvent>;

    /// Borrow the current frame-coherent [`InputState`] snapshot.
    fn current_state(&self) -> &InputState;

    /// Inspect the current grab-state.
    fn grab_state(&self) -> GrabState {
        self.current_state().grab_state
    }

    /// Acquire input grab : confine cursor + capture keyboard.
    /// MUST honour the kill-switch — Esc + Win+L still release.
    fn acquire_grab(&mut self, modes: GrabModes) -> Result<(), InputError>;

    /// Explicitly release input grab. Records a
    /// [`crate::kill_switch::KillSwitchReason::ApplicationRequested`]
    /// event for symmetric audit.
    fn release_grab(&mut self) -> Result<(), InputError>;

    /// Set the action-mapping table.
    fn set_action_map(&mut self, map: ActionMap);

    /// Borrow the active action-mapping table.
    fn action_map(&self) -> &ActionMap;

    /// Set the dead-zone applied to gamepad analog axes.
    /// Default = 8000 (≈ 24.4 % of full range).
    fn set_gamepad_deadzone(&mut self, dz: i16);

    /// Set rumble (force-feedback) for the gamepad at `slot`. `low` and
    /// `high` are in `0..=u16::MAX` (XInput convention). Returns
    /// [`InputError::FeatureUnavailable`] on backends that don't support
    /// rumble (Linux + macOS today).
    fn set_gamepad_rumble(&mut self, slot: u8, low: u16, high: u16) -> Result<(), InputError>;

    /// Returns the [`KillSwitch`] for read-only audit access.
    fn kill_switch(&self) -> &KillSwitch;

    /// Returns the [`crate::api::BackendKind`] of this implementation.
    fn kind(&self) -> crate::api::BackendKind;
}

// ───────────────────────────────────────────────────────────────────────
// § GrabModes — bitset for [`InputBackend::acquire_grab`].
// ───────────────────────────────────────────────────────────────────────

/// Which OS-level grabs to acquire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrabModes {
    pub cursor_lock: bool,
    pub keyboard_capture: bool,
    pub cursor_hide: bool,
}

impl GrabModes {
    /// Convenience : all three modes engaged (typical FPS / gameplay grab).
    #[must_use]
    pub const fn all() -> Self {
        Self {
            cursor_lock: true,
            keyboard_capture: true,
            cursor_hide: true,
        }
    }

    /// Convenience : cursor lock only (typical mouse-look without
    /// hiding cursor or grabbing keyboard — debug / level-editor mode).
    #[must_use]
    pub const fn cursor_only() -> Self {
        Self {
            cursor_lock: true,
            keyboard_capture: false,
            cursor_hide: false,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § InputBackendBuilder — typed-builder pattern.
// ───────────────────────────────────────────────────────────────────────

/// Builder for constructing the [`crate::api::ActiveBackend`].
///
/// The `kill_switch` field is **structurally required** — there is no
/// `kill_switch : Option<KillSwitch>` (which would allow a None config).
/// The `KillSwitch` is constructed inline by [`Self::new`] and cannot
/// be removed, only re-bound to a fresh instance.
///
/// Per `PRIME_DIRECTIVE.md § 6 SCOPE` : "no flag | config | env-var |
/// cli-arg | api-call | runtime-cond can disable | weaken | circumvent
/// this." This is verified statically via [`KillSwitch::is_overridable`]
/// returning `false` from a `const fn`.
#[derive(Debug)]
pub struct InputBackendBuilder {
    /// Dead-zone applied to gamepad analog axes (default = 8000).
    deadzone: i16,
    /// Per-axis dead-zone overrides ; `None` = use the global `deadzone`.
    per_axis_deadzone: [Option<i16>; GAMEPAD_AXIS_COUNT],
    /// Action-mapping table.
    action_map: ActionMap,
    /// Kill-switch — STRUCTURAL field, not optional. Per PRIME-DIRECTIVE.
    #[allow(dead_code)] // used by per-OS backends when wired up
    kill_switch: KillSwitch,
}

impl Default for InputBackendBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBackendBuilder {
    /// Create a fresh builder with default settings.
    ///
    /// The dead-zone defaults to 8000 (≈ 24.4 % of full range — the
    /// XInput / generic-cardinal-8000 convention).
    #[must_use]
    pub fn new() -> Self {
        Self {
            deadzone: 8000,
            per_axis_deadzone: [None; GAMEPAD_AXIS_COUNT],
            action_map: ActionMap::new(),
            kill_switch: KillSwitch::new(),
        }
    }

    /// Set the global dead-zone applied to all stick axes.
    #[must_use]
    pub fn with_gamepad_deadzone(mut self, dz: i16) -> Self {
        self.deadzone = dz;
        self
    }

    /// Set the dead-zone for one specific axis (overrides the global).
    #[must_use]
    pub fn with_per_axis_deadzone(mut self, axis: crate::event::GamepadAxis, dz: i16) -> Self {
        self.per_axis_deadzone[axis as usize] = Some(dz);
        self
    }

    /// Set the action-mapping table.
    #[must_use]
    pub fn with_action_map(mut self, map: ActionMap) -> Self {
        self.action_map = map;
        self
    }

    /// Returns the configured global dead-zone.
    #[must_use]
    pub fn deadzone(&self) -> i16 {
        self.deadzone
    }

    /// Returns the per-axis dead-zone for `axis`, falling back to the
    /// global if no override is set.
    #[must_use]
    pub fn deadzone_for_axis(&self, axis: crate::event::GamepadAxis) -> i16 {
        self.per_axis_deadzone[axis as usize].unwrap_or(self.deadzone)
    }

    /// Borrow the configured action-mapping table.
    #[must_use]
    pub fn action_map(&self) -> &ActionMap {
        &self.action_map
    }

    /// Take the configured kill-switch (consumes the builder field ;
    /// used internally by per-OS `Backend::from_builder` impls).
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        i16,
        [Option<i16>; GAMEPAD_AXIS_COUNT],
        ActionMap,
        KillSwitch,
    ) {
        (
            self.deadzone,
            self.per_axis_deadzone,
            self.action_map,
            self.kill_switch,
        )
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Errors.
// ───────────────────────────────────────────────────────────────────────

/// Backend-level errors.
#[derive(Debug, Error)]
pub enum InputError {
    /// Feature not implemented on this backend (e.g., rumble on Linux).
    #[error("feature not available on this backend")]
    FeatureUnavailable,
    /// OS call failed with the given error code.
    #[error("OS error : {detail}")]
    OsError { detail: String },
    /// FFI loader failed (Linux libudev not found, etc.).
    #[error("FFI loader error : {detail}")]
    LoaderError { detail: String },
    /// Application called `acquire_grab` while another grab was active.
    #[error("grab already acquired")]
    GrabAlreadyAcquired,
    /// Application called `release_grab` while no grab was active.
    #[error("no grab to release")]
    NoGrabActive,
    /// Application attempted to bypass the kill-switch (recorded for
    /// audit per §5 CONSENT-ARCHITECTURE) — this variant exists so
    /// future surfaces have somewhere to surface a structural-bypass
    /// attempt. The kill-switch itself is non-overridable so this is
    /// only ever returned by future API additions that get rejected at
    /// compile time.
    #[error("kill-switch violation attempted")]
    KillSwitchViolation,
    /// Slot index out of range.
    #[error("gamepad slot {slot} out of range (max {max})")]
    GamepadSlotOutOfRange { slot: u8, max: u8 },
}

// ───────────────────────────────────────────────────────────────────────
// § Helper used by every backend : apply dead-zone to a gamepad's axes.
// ───────────────────────────────────────────────────────────────────────

/// Apply the configured dead-zone (global + per-axis overrides) to every
/// axis on the given gamepad slot. Used by every backend's `tick` path.
pub fn apply_deadzones_to_gamepad(
    g: &mut GamepadState,
    global_dz: i16,
    per_axis_overrides: &[Option<i16>; GAMEPAD_AXIS_COUNT],
) {
    for (axis_idx, override_dz) in per_axis_overrides.iter().enumerate() {
        let dz = override_dz.unwrap_or(global_dz);
        let axis = match axis_idx {
            0 => crate::event::GamepadAxis::LeftStickX,
            1 => crate::event::GamepadAxis::LeftStickY,
            2 => crate::event::GamepadAxis::RightStickX,
            3 => crate::event::GamepadAxis::RightStickY,
            4 => crate::event::GamepadAxis::LeftTrigger,
            5 => crate::event::GamepadAxis::RightTrigger,
            _ => continue,
        };
        g.apply_deadzone(axis, dz);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::GamepadAxis;

    #[test]
    fn builder_default_deadzone() {
        let b = InputBackendBuilder::new();
        assert_eq!(b.deadzone(), 8000);
    }

    #[test]
    fn builder_deadzone_override() {
        let b = InputBackendBuilder::new().with_gamepad_deadzone(4000);
        assert_eq!(b.deadzone(), 4000);
    }

    #[test]
    fn builder_per_axis_deadzone_falls_back_to_global() {
        let b = InputBackendBuilder::new()
            .with_gamepad_deadzone(5000)
            .with_per_axis_deadzone(GamepadAxis::LeftTrigger, 1000);
        assert_eq!(b.deadzone_for_axis(GamepadAxis::LeftTrigger), 1000);
        assert_eq!(b.deadzone_for_axis(GamepadAxis::LeftStickX), 5000);
    }

    #[test]
    fn builder_into_parts_yields_kill_switch() {
        let b = InputBackendBuilder::new();
        let (dz, _, _, ks) = b.into_parts();
        assert_eq!(dz, 8000);
        assert_eq!(ks.fire_count(), 0);
    }

    #[test]
    fn grab_modes_all() {
        let m = GrabModes::all();
        assert!(m.cursor_lock);
        assert!(m.keyboard_capture);
        assert!(m.cursor_hide);
    }

    #[test]
    fn grab_modes_cursor_only() {
        let m = GrabModes::cursor_only();
        assert!(m.cursor_lock);
        assert!(!m.keyboard_capture);
        assert!(!m.cursor_hide);
    }

    #[test]
    fn apply_deadzones_zeros_below_threshold() {
        use crate::event::GamepadAxis;
        let mut g = GamepadState::default();
        g.set_axis(GamepadAxis::LeftStickX, 5000);
        g.set_axis(GamepadAxis::RightStickX, 16000);
        let overrides = [None; GAMEPAD_AXIS_COUNT];
        apply_deadzones_to_gamepad(&mut g, 8000, &overrides);
        assert_eq!(g.axis(GamepadAxis::LeftStickX), 0);
        assert_eq!(g.axis(GamepadAxis::RightStickX), 16000);
    }

    #[test]
    fn apply_deadzones_per_axis_override() {
        use crate::event::GamepadAxis;
        let mut g = GamepadState::default();
        g.set_axis(GamepadAxis::LeftTrigger, 500);
        let mut overrides = [None; GAMEPAD_AXIS_COUNT];
        overrides[GamepadAxis::LeftTrigger as usize] = Some(100);
        apply_deadzones_to_gamepad(&mut g, 8000, &overrides);
        // 500 ≥ 100 → preserved despite global 8000 cap.
        assert_eq!(g.axis(GamepadAxis::LeftTrigger), 500);
    }
}
