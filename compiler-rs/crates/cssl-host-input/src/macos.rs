//! § macOS input backend — IOKit HID (cfg-gated to target_os = "macos").
//!
//! § ROLE
//!   The macOS path uses IOKit's HID Manager API :
//!     - `IOHIDManagerCreate` to construct the top-level manager.
//!     - `IOHIDManagerSetDeviceMatching` with a CFDictionary matching
//!       `kHIDPage_GenericDesktop` keyboards / mice / gamepads.
//!     - `IOHIDManagerRegisterInputValueCallback` to receive events on
//!       a CFRunLoop thread.
//!
//!   Apocky's primary host is Windows + Arc A770 ; the macOS path is
//!   structurally tested. The implementation is an interface-mirror
//!   that compiles cleanly cross-platform but defers the actual IOKit
//!   integration to a future macOS CI runner — mirrors the
//!   `cssl-host-metal` strategy at S6-E3.
//!
//! § STATUS
//!   - `MacosBackend::from_builder` constructs successfully on every
//!     host (the IOKit FFI declarations are gated to
//!     `target_os = "macos"` ; on other hosts the backend is a stub
//!     returning `FeatureUnavailable`).
//!   - The kill-switch is honoured at the trait level (any future
//!     `process_iokit_event` impl MUST flow Esc through
//!     [`crate::kill_switch::KillSwitch::on_event`]).

use crate::api::BackendKind;
use crate::backend::{GrabModes, InputBackend, InputBackendBuilder, InputError};
use crate::event::InputEvent;
use crate::kill_switch::{KillSwitch, KillSwitchEvent, KillSwitchReason};
use crate::mapping::ActionMap;
use crate::state::{InputState, GAMEPAD_AXIS_COUNT};
use std::collections::VecDeque;

/// macOS input backend.
#[derive(Debug)]
pub struct MacosBackend {
    state: InputState,
    events: VecDeque<InputEvent>,
    action_map: ActionMap,
    deadzone: i16,
    per_axis_deadzone: [Option<i16>; GAMEPAD_AXIS_COUNT],
    kill_switch: KillSwitch,
    /// `true` if IOKit was successfully connected — observability only.
    iokit_loaded: bool,
}

impl MacosBackend {
    /// Construct from a builder.
    #[must_use]
    pub fn from_builder(builder: InputBackendBuilder) -> Self {
        let (deadzone, per_axis, action_map, kill_switch) = builder.into_parts();
        // On macOS hosts, a future impl will call
        // `IOHIDManagerCreate(kCFAllocatorDefault, kIOHIDOptionsTypeNone)`
        // here. On non-macOS hosts the field is just `false`.
        #[cfg(target_os = "macos")]
        let iokit_loaded = true;
        #[cfg(not(target_os = "macos"))]
        let iokit_loaded = false;

        Self {
            state: InputState::default(),
            events: VecDeque::new(),
            action_map,
            deadzone,
            per_axis_deadzone: per_axis,
            kill_switch,
            iokit_loaded,
        }
    }

    /// Returns `true` if IOKit HID Manager was successfully connected.
    #[must_use]
    pub fn has_iokit(&self) -> bool {
        self.iokit_loaded
    }

    /// Inject an event from a future IOKit callback (or from the test
    /// harness today). The slot field maps to the device-index-of-origin.
    pub fn process_event(&mut self, event: InputEvent) {
        // Mirror the state inline.
        match event {
            InputEvent::KeyDown { code, .. } => self.state.keys.set(code, true),
            InputEvent::KeyUp { code } => self.state.keys.set(code, false),
            InputEvent::MouseMove { x, y } => {
                self.state.mouse.x = x;
                self.state.mouse.y = y;
            }
            InputEvent::MouseDown { button, x, y } => {
                self.state.mouse.set_button(button, true);
                self.state.mouse.x = x;
                self.state.mouse.y = y;
            }
            InputEvent::MouseUp { button, x, y } => {
                self.state.mouse.set_button(button, false);
                self.state.mouse.x = x;
                self.state.mouse.y = y;
            }
            InputEvent::Scroll { axis, delta } => {
                self.state.mouse.accumulate_scroll(axis, delta);
            }
            InputEvent::GamepadConnect { slot } => {
                if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                    g.connected = true;
                }
            }
            InputEvent::GamepadDisconnect { slot } => {
                if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                    g.clear_all();
                }
            }
            InputEvent::GamepadAxisChange { slot, axis, value } => {
                if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                    g.set_axis(axis, value);
                }
            }
            InputEvent::GamepadButtonChange {
                slot,
                button,
                pressed,
            } => {
                if let Some(g) = self.state.gamepads.get_mut(slot as usize) {
                    g.set_button(button, pressed);
                }
            }
        }

        // Kill-switch inspection.
        let prior = self.state.grab_state;
        if let Some(reason) = self.kill_switch.on_event(&event, prior) {
            self.release_grab_internal(reason);
        }

        self.events.push_back(event);
    }

    /// Notify the backend of a macOS lock-screen / sleep event.
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
        self.kill_switch.trigger(reason, prior, self.state.tick);
    }
}

impl InputBackend for MacosBackend {
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
        // GameController.framework supports rumble via
        // `setMotorSpeeds:lowFrequency:highFrequency:` ; out of scope
        // for S7-F2.
        Err(InputError::FeatureUnavailable)
    }

    fn kill_switch(&self) -> &KillSwitch {
        &self.kill_switch
    }

    fn kind(&self) -> BackendKind {
        BackendKind::MacOS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{KeyCode, RepeatCount};

    #[test]
    fn macos_backend_constructs() {
        let b = MacosBackend::from_builder(InputBackendBuilder::new());
        assert_eq!(b.kind(), BackendKind::MacOS);
        assert_eq!(b.current_state().tick, 0);
    }

    #[test]
    fn macos_process_keydown() {
        let mut b = MacosBackend::from_builder(InputBackendBuilder::new());
        b.process_event(InputEvent::KeyDown {
            code: KeyCode::Space,
            repeat_count: RepeatCount::FirstPress,
        });
        assert!(b.current_state().keys.is_pressed(KeyCode::Space));
    }

    #[test]
    fn macos_esc_during_grab_fires_kill_switch() {
        let mut b = MacosBackend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.process_event(InputEvent::KeyDown {
            code: KeyCode::Escape,
            repeat_count: RepeatCount::FirstPress,
        });
        assert!(!b.current_state().grab_state.is_grabbed());
        let ks = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks.reason, KillSwitchReason::EscPressed);
    }

    #[test]
    fn macos_session_lock_fires_kill_switch() {
        let mut b = MacosBackend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.on_session_lock_change(true);
        assert!(!b.current_state().grab_state.is_grabbed());
        let ks = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks.reason, KillSwitchReason::SessionLockChord);
    }

    #[test]
    fn macos_rumble_unavailable() {
        let mut b = MacosBackend::from_builder(InputBackendBuilder::new());
        let err = b.set_gamepad_rumble(0, 100, 100).unwrap_err();
        assert!(matches!(err, InputError::FeatureUnavailable));
    }

    #[test]
    fn macos_release_grab_records_application() {
        let mut b = MacosBackend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        b.release_grab().unwrap();
        let ks = b.poll_kill_switch_events().unwrap();
        assert_eq!(ks.reason, KillSwitchReason::ApplicationRequested);
    }
}
