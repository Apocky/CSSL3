//! § Stub input backend — no-op fallback.
//!
//! § ROLE
//!
//!   Used by hosts where no real OS-input backend is appropriate :
//!   - non-Win/Linux/macOS targets (e.g., embedded, WASM)
//!   - tests that don't need real OS input
//!   - headless CI runs
//!
//!   Mirrors the [`InputBackend`] trait surface but returns
//!   [`InputError::FeatureUnavailable`] on every state-mutating call
//!   except `set_action_map` / `set_gamepad_deadzone` / `release_grab`
//!   (these silently succeed because they have no OS-side effect).
//!
//!   **Crucially** : the stub backend STILL honours the kill-switch.
//!   Per §6 PRIME-DIRECTIVE SCOPE, the stub cannot be a backdoor
//!   that bypasses kill-switch enforcement — even when no real grab is
//!   ever acquired. The stub's `acquire_grab` always returns
//!   `FeatureUnavailable`, so no grab can be active ; if a future
//!   stub variant grows real grab semantics, it inherits the
//!   non-overridable kill-switch via the [`KillSwitch`] field.

use crate::api::BackendKind;
use crate::backend::{GrabModes, InputBackend, InputBackendBuilder, InputError};
use crate::event::InputEvent;
use crate::kill_switch::{KillSwitch, KillSwitchEvent, KillSwitchReason};
use crate::mapping::ActionMap;
use crate::state::{InputState, GAMEPAD_AXIS_COUNT};
use std::collections::VecDeque;

/// Stub backend that does nothing.
#[derive(Debug)]
pub struct StubBackend {
    state: InputState,
    events: VecDeque<InputEvent>,
    action_map: ActionMap,
    deadzone: i16,
    per_axis_deadzone: [Option<i16>; GAMEPAD_AXIS_COUNT],
    kill_switch: KillSwitch,
}

impl Default for StubBackend {
    fn default() -> Self {
        Self::from_builder(InputBackendBuilder::new())
    }
}

impl StubBackend {
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
        }
    }

    /// Inject a synthetic event — used by tests + by the stub-mode F1
    /// integration to drive scripted input sequences. Available on the
    /// stub only ; real backends accept events from the OS.
    pub fn inject_event(&mut self, event: InputEvent) {
        // Apply state changes inline so the snapshot tracks injection.
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

        // Kill-switch inspection : if we're "grabbed" (no — stub never
        // grabs) we'd fire. Stub never grabs, so nothing happens here ;
        // real backends do this in `tick`.
        let prior_grab = self.state.grab_state;
        if let Some(reason) = self.kill_switch.on_event(&event, prior_grab) {
            // Release grab (stub has nothing to release).
            self.state.grab_state = Default::default();
            self.state.clear_all_inputs();
            self.kill_switch
                .trigger(reason, prior_grab, self.state.tick);
        }

        self.events.push_back(event);
    }

    /// Borrow the per-axis dead-zone overrides (test helper).
    #[must_use]
    pub fn per_axis_deadzone(&self) -> &[Option<i16>; GAMEPAD_AXIS_COUNT] {
        &self.per_axis_deadzone
    }
}

impl InputBackend for StubBackend {
    fn tick(&mut self) -> Result<usize, InputError> {
        // Stub doesn't poll anything ; just advance the tick counter.
        self.state.tick = self.state.tick.saturating_add(1);
        // Apply dead-zones.
        for g in &mut self.state.gamepads {
            if g.connected {
                crate::backend::apply_deadzones_to_gamepad(
                    g,
                    self.deadzone,
                    &self.per_axis_deadzone,
                );
            }
        }
        Ok(self.events.len())
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

    fn acquire_grab(&mut self, _modes: GrabModes) -> Result<(), InputError> {
        Err(InputError::FeatureUnavailable)
    }

    fn release_grab(&mut self) -> Result<(), InputError> {
        let prior = self.state.grab_state;
        if !prior.is_grabbed() {
            return Err(InputError::NoGrabActive);
        }
        self.state.grab_state = Default::default();
        self.state.clear_all_inputs();
        self.kill_switch.trigger(
            KillSwitchReason::ApplicationRequested,
            prior,
            self.state.tick,
        );
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
        Err(InputError::FeatureUnavailable)
    }

    fn kill_switch(&self) -> &KillSwitch {
        &self.kill_switch
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Stub
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{KeyCode, RepeatCount};

    #[test]
    fn stub_default_constructs() {
        let b = StubBackend::default();
        assert_eq!(b.kind(), BackendKind::Stub);
        assert_eq!(b.current_state().tick, 0);
    }

    #[test]
    fn stub_tick_advances() {
        let mut b = StubBackend::default();
        b.tick().unwrap();
        assert_eq!(b.current_state().tick, 1);
        b.tick().unwrap();
        assert_eq!(b.current_state().tick, 2);
    }

    #[test]
    fn stub_inject_keydown() {
        let mut b = StubBackend::default();
        let ev = InputEvent::KeyDown {
            code: KeyCode::Space,
            repeat_count: RepeatCount::FirstPress,
        };
        b.inject_event(ev);
        assert!(b.current_state().keys.is_pressed(KeyCode::Space));
        let polled = b.poll_events().unwrap();
        assert_eq!(polled, ev);
        assert!(b.poll_events().is_none());
    }

    #[test]
    fn stub_acquire_grab_returns_unavailable() {
        let mut b = StubBackend::default();
        let err = b.acquire_grab(GrabModes::all()).unwrap_err();
        assert!(matches!(err, InputError::FeatureUnavailable));
    }

    #[test]
    fn stub_release_grab_when_none_active() {
        let mut b = StubBackend::default();
        let err = b.release_grab().unwrap_err();
        assert!(matches!(err, InputError::NoGrabActive));
    }

    #[test]
    fn stub_rumble_returns_unavailable() {
        let mut b = StubBackend::default();
        let err = b.set_gamepad_rumble(0, 100, 100).unwrap_err();
        assert!(matches!(err, InputError::FeatureUnavailable));
    }

    #[test]
    fn stub_kill_switch_count_starts_zero() {
        let b = StubBackend::default();
        assert_eq!(b.kill_switch().fire_count(), 0);
    }

    #[test]
    fn stub_set_action_map() {
        use crate::mapping::{ActionBinding, ActionMap, ActionName, ActionTrigger};
        let mut map = ActionMap::new();
        map.add_binding(
            ActionBinding::new(ActionName::new("jump").unwrap())
                .with_trigger(ActionTrigger::Key(KeyCode::Space)),
        )
        .unwrap();

        let mut b = StubBackend::default();
        b.set_action_map(map);
        assert_eq!(b.action_map().len(), 1);
    }
}
