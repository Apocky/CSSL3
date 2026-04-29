//! § Configurable action → physical-input binding (declarative).
//!
//! § ROLE
//!   Source-level CSSLv3 code typically operates on *actions* ("jump",
//!   "fire", "move-forward") rather than *physical inputs* (keyboard W,
//!   gamepad button A, mouse left). The `mapping` module is the
//!   declarative bridge — an [`ActionMap`] is a list of bindings, each
//!   pairing an [`ActionName`] string with one or more
//!   [`ActionTrigger`]s (physical inputs). Multiple triggers per action
//!   = "any of these fires the action" (logical OR).
//!
//! § JSON-STYLE LOAD
//!   Per the slice brief : "Configurable input-mapping : action →
//!   physical-input binding (declarative, JSON-style)". The
//!   [`ActionMap::from_csl_table`] fn parses a tiny declarative-table
//!   format that source-level CSSLv3 emits ; it's intentionally NOT
//!   `serde_json` (we keep cssl-host-input's dependency surface
//!   minimal — only `thiserror` + `libloading`). The table format
//!   looks like :
//!
//!   ```text
//!   action "jump"
//!     trigger key W
//!     trigger gamepad-button A
//!   action "fire"
//!     trigger mouse-button Left
//!     trigger gamepad-button RightTrigger
//!   ```
//!
//!   Lines starting with `#` are comments ; whitespace is collapsed.
//!   Action names are quoted strings (max 64 bytes ASCII). Each trigger
//!   line is `trigger <kind> <name>` with kinds :
//!     - `key <KeyCode>`               (e.g., `key W`)
//!     - `mouse-button <MouseButton>`  (e.g., `mouse-button Left`)
//!     - `gamepad-button <GamepadButton>` (e.g., `gamepad-button A`)
//!     - `gamepad-axis <GamepadAxis> <threshold>`
//!         where `threshold` is `-32768..=32767` and the trigger fires
//!         when the axis value crosses the threshold (positive threshold
//!         → fire when axis ≥ threshold ; negative threshold → fire when
//!         axis ≤ threshold).
//!
//! § QUERY API
//!   - [`ActionMap::is_action_active`] — fast "is this action firing
//!     right now?" check using the current [`crate::state::InputState`].
//!   - [`ActionMap::triggers_for`] — enumerate the bindings for one
//!     action.
//!   - [`ActionMap::actions_active_in`] — iterate every action that's
//!     currently firing (one frame's worth ; suits per-frame action
//!     dispatch).

use crate::event::{GamepadAxis, GamepadButton, KeyCode, MouseButton};
use crate::state::{GamepadState, InputState};
use thiserror::Error;

// ───────────────────────────────────────────────────────────────────────
// § Action name — max 64 bytes ASCII.
// ───────────────────────────────────────────────────────────────────────

/// Action identifier.
///
/// Limited to 64 bytes ASCII so the binding table stays cache-friendly.
/// Source-level code can match on the inner `&str` for dispatch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionName(String);

impl ActionName {
    /// Maximum length in bytes.
    pub const MAX_LEN: usize = 64;

    /// Construct from a string. Returns `Err` if the name is empty,
    /// longer than [`MAX_LEN`], or contains non-ASCII bytes.
    ///
    /// [`MAX_LEN`]: ActionName::MAX_LEN
    pub fn new(name: impl Into<String>) -> Result<Self, MappingError> {
        let s = name.into();
        if s.is_empty() {
            return Err(MappingError::EmptyActionName);
        }
        if s.len() > Self::MAX_LEN {
            return Err(MappingError::ActionNameTooLong {
                len: s.len(),
                max: Self::MAX_LEN,
            });
        }
        if !s.is_ascii() {
            return Err(MappingError::ActionNameNotAscii);
        }
        Ok(Self(s))
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ActionTrigger — physical-input description.
// ───────────────────────────────────────────────────────────────────────

/// Physical-input description that fires an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionTrigger {
    /// Fire when keyboard key `code` is held down.
    Key(KeyCode),
    /// Fire when mouse button `btn` is held down.
    MouseButton(MouseButton),
    /// Fire when gamepad button `btn` is held down on slot `slot`.
    /// `slot` = 0xFF means "any connected gamepad" (logical OR).
    GamepadButton { slot: u8, button: GamepadButton },
    /// Fire when gamepad axis `axis` value crosses `threshold`. If
    /// `threshold` is positive, fires when the axis value is
    /// ≥ `threshold` ; if negative, fires when ≤ `threshold`.
    /// `slot = 0xFF` means "any connected gamepad".
    GamepadAxis {
        slot: u8,
        axis: GamepadAxis,
        threshold: i16,
    },
}

/// Sentinel slot value meaning "any connected gamepad" (logical OR).
pub const GAMEPAD_SLOT_ANY: u8 = 0xFF;

// ───────────────────────────────────────────────────────────────────────
// § ActionBinding — one action + its triggers.
// ───────────────────────────────────────────────────────────────────────

/// One action and the list of physical-input triggers that fire it.
#[derive(Clone, Debug)]
pub struct ActionBinding {
    /// Name of the action (unique within an [`ActionMap`]).
    pub action: ActionName,
    /// List of physical-input triggers ; ANY one firing fires the action
    /// (logical OR).
    pub triggers: Vec<ActionTrigger>,
}

impl ActionBinding {
    /// Construct a new binding with no triggers.
    #[must_use]
    pub fn new(action: ActionName) -> Self {
        Self {
            action,
            triggers: Vec::new(),
        }
    }

    /// Add a trigger.
    #[must_use]
    pub fn with_trigger(mut self, trigger: ActionTrigger) -> Self {
        self.triggers.push(trigger);
        self
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ActionMap — top-level binding table.
// ───────────────────────────────────────────────────────────────────────

/// Collection of [`ActionBinding`]s.
#[derive(Clone, Debug, Default)]
pub struct ActionMap {
    bindings: Vec<ActionBinding>,
}

impl ActionMap {
    /// Construct an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a binding. Returns `Err(MappingError::DuplicateAction)` if
    /// the action name is already bound.
    pub fn add_binding(&mut self, binding: ActionBinding) -> Result<(), MappingError> {
        if self.bindings.iter().any(|b| b.action == binding.action) {
            return Err(MappingError::DuplicateAction(binding.action.0));
        }
        self.bindings.push(binding);
        Ok(())
    }

    /// Number of bindings currently in the map.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` if the map has no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Lookup the bindings for an action by name. Returns `None` if no
    /// such action is bound.
    #[must_use]
    pub fn triggers_for(&self, action: &str) -> Option<&[ActionTrigger]> {
        self.bindings
            .iter()
            .find(|b| b.action.as_str() == action)
            .map(|b| b.triggers.as_slice())
    }

    /// Returns `true` if the named action is currently firing per the
    /// given input state.
    #[must_use]
    pub fn is_action_active(&self, action: &str, state: &InputState) -> bool {
        let Some(triggers) = self.triggers_for(action) else {
            return false;
        };
        triggers.iter().any(|t| Self::is_trigger_firing(*t, state))
    }

    /// Iterate over every action currently firing per the given input
    /// state. Suits per-frame action dispatch.
    pub fn actions_active_in<'a>(&'a self, state: &'a InputState) -> impl Iterator<Item = &'a str> {
        self.bindings.iter().filter_map(|b| {
            if b.triggers
                .iter()
                .any(|t| Self::is_trigger_firing(*t, state))
            {
                Some(b.action.as_str())
            } else {
                None
            }
        })
    }

    /// Test whether one trigger is firing.
    fn is_trigger_firing(trigger: ActionTrigger, state: &InputState) -> bool {
        match trigger {
            ActionTrigger::Key(code) => state.keys.is_pressed(code),
            ActionTrigger::MouseButton(btn) => state.mouse.is_button_pressed(btn),
            ActionTrigger::GamepadButton { slot, button } => {
                Self::for_each_slot(slot, state, |g| g.is_button_pressed(button))
            }
            ActionTrigger::GamepadAxis {
                slot,
                axis,
                threshold,
            } => Self::for_each_slot(slot, state, |g| {
                let v = g.axis(axis);
                if threshold >= 0 {
                    v >= threshold
                } else {
                    v <= threshold
                }
            }),
        }
    }

    /// Helper : evaluate `pred` against `state.gamepads[slot]` (one slot)
    /// or against every connected gamepad if `slot == GAMEPAD_SLOT_ANY`.
    fn for_each_slot(slot: u8, state: &InputState, pred: impl Fn(&GamepadState) -> bool) -> bool {
        if slot == GAMEPAD_SLOT_ANY {
            state.gamepads.iter().filter(|g| g.connected).any(pred)
        } else {
            state
                .gamepad(slot as usize)
                .is_some_and(|g| g.connected && pred(g))
        }
    }

    /// Parse a declarative-table source into an [`ActionMap`].
    ///
    /// See module docs for the format. Returns `Err(MappingError::*)`
    /// on parse error.
    pub fn from_csl_table(src: &str) -> Result<Self, MappingError> {
        let mut map = Self::new();
        let mut current: Option<ActionBinding> = None;

        for (lineno, raw_line) in src.lines().enumerate() {
            let line = raw_line.trim();
            // Skip comments + blank lines.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(rest) = line.strip_prefix("action ") {
                // Flush previous.
                if let Some(prev) = current.take() {
                    map.add_binding(prev)?;
                }
                let name = parse_quoted(rest, lineno + 1)?;
                current = Some(ActionBinding::new(ActionName::new(name)?));
            } else if let Some(rest) = line.strip_prefix("trigger ") {
                let Some(binding) = current.as_mut() else {
                    return Err(MappingError::TriggerWithoutAction { line: lineno + 1 });
                };
                let trigger = parse_trigger(rest, lineno + 1)?;
                binding.triggers.push(trigger);
            } else {
                return Err(MappingError::UnknownDirective {
                    line: lineno + 1,
                    text: line.to_string(),
                });
            }
        }

        if let Some(last) = current.take() {
            map.add_binding(last)?;
        }

        Ok(map)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Parsing helpers.
// ───────────────────────────────────────────────────────────────────────

fn parse_quoted(src: &str, line: usize) -> Result<String, MappingError> {
    let s = src.trim();
    if !s.starts_with('"') || !s.ends_with('"') || s.len() < 2 {
        return Err(MappingError::ExpectedQuotedString { line });
    }
    Ok(s[1..s.len() - 1].to_string())
}

fn parse_trigger(rest: &str, line: usize) -> Result<ActionTrigger, MappingError> {
    let mut parts = rest.split_whitespace();
    let kind = parts
        .next()
        .ok_or(MappingError::MissingTriggerKind { line })?;
    match kind {
        "key" => {
            let name = parts
                .next()
                .ok_or(MappingError::MissingTriggerName { line })?;
            let code = key_code_from_str(name).ok_or_else(|| MappingError::UnknownKeyCode {
                line,
                text: name.to_string(),
            })?;
            Ok(ActionTrigger::Key(code))
        }
        "mouse-button" => {
            let name = parts
                .next()
                .ok_or(MappingError::MissingTriggerName { line })?;
            let btn =
                mouse_button_from_str(name).ok_or_else(|| MappingError::UnknownMouseButton {
                    line,
                    text: name.to_string(),
                })?;
            Ok(ActionTrigger::MouseButton(btn))
        }
        "gamepad-button" => {
            let name = parts
                .next()
                .ok_or(MappingError::MissingTriggerName { line })?;
            let btn = gamepad_button_from_str(name).ok_or_else(|| {
                MappingError::UnknownGamepadButton {
                    line,
                    text: name.to_string(),
                }
            })?;
            Ok(ActionTrigger::GamepadButton {
                slot: GAMEPAD_SLOT_ANY,
                button: btn,
            })
        }
        "gamepad-axis" => {
            let name = parts
                .next()
                .ok_or(MappingError::MissingTriggerName { line })?;
            let axis =
                gamepad_axis_from_str(name).ok_or_else(|| MappingError::UnknownGamepadAxis {
                    line,
                    text: name.to_string(),
                })?;
            let thresh_str = parts
                .next()
                .ok_or(MappingError::MissingThreshold { line })?;
            let threshold: i16 =
                thresh_str
                    .parse()
                    .map_err(|_| MappingError::InvalidThreshold {
                        line,
                        text: thresh_str.to_string(),
                    })?;
            Ok(ActionTrigger::GamepadAxis {
                slot: GAMEPAD_SLOT_ANY,
                axis,
                threshold,
            })
        }
        other => Err(MappingError::UnknownTriggerKind {
            line,
            text: other.to_string(),
        }),
    }
}

fn key_code_from_str(s: &str) -> Option<KeyCode> {
    match s {
        "A" => Some(KeyCode::A),
        "B" => Some(KeyCode::B),
        "C" => Some(KeyCode::C),
        "D" => Some(KeyCode::D),
        "E" => Some(KeyCode::E),
        "F" => Some(KeyCode::F),
        "G" => Some(KeyCode::G),
        "H" => Some(KeyCode::H),
        "I" => Some(KeyCode::I),
        "J" => Some(KeyCode::J),
        "K" => Some(KeyCode::K),
        "L" => Some(KeyCode::L),
        "M" => Some(KeyCode::M),
        "N" => Some(KeyCode::N),
        "O" => Some(KeyCode::O),
        "P" => Some(KeyCode::P),
        "Q" => Some(KeyCode::Q),
        "R" => Some(KeyCode::R),
        "S" => Some(KeyCode::S),
        "T" => Some(KeyCode::T),
        "U" => Some(KeyCode::U),
        "V" => Some(KeyCode::V),
        "W" => Some(KeyCode::W),
        "X" => Some(KeyCode::X),
        "Y" => Some(KeyCode::Y),
        "Z" => Some(KeyCode::Z),
        "Space" => Some(KeyCode::Space),
        "Enter" => Some(KeyCode::Enter),
        "Tab" => Some(KeyCode::Tab),
        "Escape" => Some(KeyCode::Escape),
        "Backspace" => Some(KeyCode::Backspace),
        "ArrowLeft" => Some(KeyCode::ArrowLeft),
        "ArrowRight" => Some(KeyCode::ArrowRight),
        "ArrowUp" => Some(KeyCode::ArrowUp),
        "ArrowDown" => Some(KeyCode::ArrowDown),
        "LeftShift" => Some(KeyCode::LeftShift),
        "RightShift" => Some(KeyCode::RightShift),
        "LeftCtrl" => Some(KeyCode::LeftCtrl),
        "RightCtrl" => Some(KeyCode::RightCtrl),
        "LeftAlt" => Some(KeyCode::LeftAlt),
        "RightAlt" => Some(KeyCode::RightAlt),
        "F1" => Some(KeyCode::F1),
        "F2" => Some(KeyCode::F2),
        "F3" => Some(KeyCode::F3),
        "F4" => Some(KeyCode::F4),
        "F5" => Some(KeyCode::F5),
        "F6" => Some(KeyCode::F6),
        "F7" => Some(KeyCode::F7),
        "F8" => Some(KeyCode::F8),
        "F9" => Some(KeyCode::F9),
        "F10" => Some(KeyCode::F10),
        "F11" => Some(KeyCode::F11),
        "F12" => Some(KeyCode::F12),
        _ => None,
    }
}

fn mouse_button_from_str(s: &str) -> Option<MouseButton> {
    match s {
        "Left" => Some(MouseButton::Left),
        "Right" => Some(MouseButton::Right),
        "Middle" => Some(MouseButton::Middle),
        "X1" => Some(MouseButton::X1),
        "X2" => Some(MouseButton::X2),
        _ => None,
    }
}

fn gamepad_button_from_str(s: &str) -> Option<GamepadButton> {
    match s {
        "A" => Some(GamepadButton::A),
        "B" => Some(GamepadButton::B),
        "X" => Some(GamepadButton::X),
        "Y" => Some(GamepadButton::Y),
        "LeftBumper" => Some(GamepadButton::LeftBumper),
        "RightBumper" => Some(GamepadButton::RightBumper),
        "Back" => Some(GamepadButton::Back),
        "Start" => Some(GamepadButton::Start),
        "LeftStick" => Some(GamepadButton::LeftStick),
        "RightStick" => Some(GamepadButton::RightStick),
        "DPadUp" => Some(GamepadButton::DPadUp),
        "DPadDown" => Some(GamepadButton::DPadDown),
        "DPadLeft" => Some(GamepadButton::DPadLeft),
        "DPadRight" => Some(GamepadButton::DPadRight),
        "Guide" => Some(GamepadButton::Guide),
        _ => None,
    }
}

fn gamepad_axis_from_str(s: &str) -> Option<GamepadAxis> {
    match s {
        "LeftStickX" => Some(GamepadAxis::LeftStickX),
        "LeftStickY" => Some(GamepadAxis::LeftStickY),
        "RightStickX" => Some(GamepadAxis::RightStickX),
        "RightStickY" => Some(GamepadAxis::RightStickY),
        "LeftTrigger" => Some(GamepadAxis::LeftTrigger),
        "RightTrigger" => Some(GamepadAxis::RightTrigger),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Errors.
// ───────────────────────────────────────────────────────────────────────

/// Mapping-table parse / construction errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MappingError {
    #[error("action name is empty")]
    EmptyActionName,
    #[error("action name is too long ({len} bytes ; max {max})")]
    ActionNameTooLong { len: usize, max: usize },
    #[error("action name must be ASCII")]
    ActionNameNotAscii,
    #[error("duplicate action name : {0}")]
    DuplicateAction(String),
    #[error("line {line} : trigger declared without preceding action")]
    TriggerWithoutAction { line: usize },
    #[error("line {line} : unknown directive : {text}")]
    UnknownDirective { line: usize, text: String },
    #[error("line {line} : expected quoted string")]
    ExpectedQuotedString { line: usize },
    #[error("line {line} : trigger missing kind")]
    MissingTriggerKind { line: usize },
    #[error("line {line} : trigger missing name")]
    MissingTriggerName { line: usize },
    #[error("line {line} : unknown trigger kind : {text}")]
    UnknownTriggerKind { line: usize, text: String },
    #[error("line {line} : unknown key code : {text}")]
    UnknownKeyCode { line: usize, text: String },
    #[error("line {line} : unknown mouse button : {text}")]
    UnknownMouseButton { line: usize, text: String },
    #[error("line {line} : unknown gamepad button : {text}")]
    UnknownGamepadButton { line: usize, text: String },
    #[error("line {line} : unknown gamepad axis : {text}")]
    UnknownGamepadAxis { line: usize, text: String },
    #[error("line {line} : missing axis threshold")]
    MissingThreshold { line: usize },
    #[error("line {line} : invalid threshold : {text}")]
    InvalidThreshold { line: usize, text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> ActionName {
        ActionName::new(s).unwrap()
    }

    #[test]
    fn action_name_validation() {
        assert!(ActionName::new("").is_err());
        assert!(ActionName::new("ascii-ok").is_ok());
        assert!(ActionName::new("x".repeat(65)).is_err());
        assert!(ActionName::new("non-ascii-éèà").is_err());
    }

    #[test]
    fn action_map_add_binding() {
        let mut map = ActionMap::new();
        let b = ActionBinding::new(name("jump")).with_trigger(ActionTrigger::Key(KeyCode::Space));
        assert!(map.add_binding(b).is_ok());
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn action_map_duplicate_action() {
        let mut map = ActionMap::new();
        map.add_binding(ActionBinding::new(name("jump"))).unwrap();
        let err = map
            .add_binding(ActionBinding::new(name("jump")))
            .unwrap_err();
        assert_eq!(err, MappingError::DuplicateAction("jump".to_string()));
    }

    #[test]
    fn action_active_via_keyboard() {
        let mut map = ActionMap::new();
        map.add_binding(
            ActionBinding::new(name("jump")).with_trigger(ActionTrigger::Key(KeyCode::Space)),
        )
        .unwrap();

        let mut state = InputState::default();
        assert!(!map.is_action_active("jump", &state));
        state.keys.set(KeyCode::Space, true);
        assert!(map.is_action_active("jump", &state));
    }

    #[test]
    fn action_active_via_mouse() {
        let mut map = ActionMap::new();
        map.add_binding(
            ActionBinding::new(name("fire"))
                .with_trigger(ActionTrigger::MouseButton(MouseButton::Left)),
        )
        .unwrap();

        let mut state = InputState::default();
        assert!(!map.is_action_active("fire", &state));
        state.mouse.set_button(MouseButton::Left, true);
        assert!(map.is_action_active("fire", &state));
    }

    #[test]
    fn action_active_via_gamepad_button_any_slot() {
        let mut map = ActionMap::new();
        map.add_binding(ActionBinding::new(name("fire")).with_trigger(
            ActionTrigger::GamepadButton {
                slot: GAMEPAD_SLOT_ANY,
                button: GamepadButton::A,
            },
        ))
        .unwrap();

        let mut state = InputState::default();
        // Slot 0 not connected → no fire.
        state.gamepads[0].set_button(GamepadButton::A, true);
        assert!(!map.is_action_active("fire", &state));
        // Connect → fires.
        state.gamepads[0].connected = true;
        assert!(map.is_action_active("fire", &state));
    }

    #[test]
    fn action_active_via_gamepad_axis_positive_threshold() {
        let mut map = ActionMap::new();
        map.add_binding(ActionBinding::new(name("look-right")).with_trigger(
            ActionTrigger::GamepadAxis {
                slot: GAMEPAD_SLOT_ANY,
                axis: GamepadAxis::RightStickX,
                threshold: 8000,
            },
        ))
        .unwrap();

        let mut state = InputState::default();
        state.gamepads[0].connected = true;
        state.gamepads[0].set_axis(GamepadAxis::RightStickX, 5000);
        assert!(!map.is_action_active("look-right", &state));
        state.gamepads[0].set_axis(GamepadAxis::RightStickX, 16000);
        assert!(map.is_action_active("look-right", &state));
    }

    #[test]
    fn action_active_via_gamepad_axis_negative_threshold() {
        let mut map = ActionMap::new();
        map.add_binding(ActionBinding::new(name("look-left")).with_trigger(
            ActionTrigger::GamepadAxis {
                slot: GAMEPAD_SLOT_ANY,
                axis: GamepadAxis::RightStickX,
                threshold: -8000,
            },
        ))
        .unwrap();

        let mut state = InputState::default();
        state.gamepads[0].connected = true;
        state.gamepads[0].set_axis(GamepadAxis::RightStickX, -5000);
        assert!(!map.is_action_active("look-left", &state));
        state.gamepads[0].set_axis(GamepadAxis::RightStickX, -16000);
        assert!(map.is_action_active("look-left", &state));
    }

    #[test]
    fn actions_active_iter() {
        let mut map = ActionMap::new();
        map.add_binding(
            ActionBinding::new(name("jump")).with_trigger(ActionTrigger::Key(KeyCode::Space)),
        )
        .unwrap();
        map.add_binding(
            ActionBinding::new(name("crouch")).with_trigger(ActionTrigger::Key(KeyCode::LeftCtrl)),
        )
        .unwrap();

        let mut state = InputState::default();
        state.keys.set(KeyCode::Space, true);

        let active: Vec<&str> = map.actions_active_in(&state).collect();
        assert_eq!(active, vec!["jump"]);
    }

    #[test]
    fn parse_csl_table_simple() {
        let src = r#"
# comment line — ignored
action "jump"
  trigger key W
  trigger key Space
action "fire"
  trigger mouse-button Left
  trigger gamepad-button A
"#;
        let map = ActionMap::from_csl_table(src).unwrap();
        assert_eq!(map.len(), 2);
        let jump_triggers = map.triggers_for("jump").unwrap();
        assert_eq!(jump_triggers.len(), 2);
        let fire_triggers = map.triggers_for("fire").unwrap();
        assert_eq!(fire_triggers.len(), 2);
    }

    #[test]
    fn parse_csl_table_axis_with_threshold() {
        let src = r#"
action "look-right"
  trigger gamepad-axis RightStickX 8000
"#;
        let map = ActionMap::from_csl_table(src).unwrap();
        let triggers = map.triggers_for("look-right").unwrap();
        assert_eq!(triggers.len(), 1);
        match triggers[0] {
            ActionTrigger::GamepadAxis {
                threshold, axis, ..
            } => {
                assert_eq!(threshold, 8000);
                assert_eq!(axis, GamepadAxis::RightStickX);
            }
            _ => panic!("expected GamepadAxis trigger"),
        }
    }

    #[test]
    fn parse_csl_table_trigger_without_action() {
        let src = r"
trigger key W
";
        let err = ActionMap::from_csl_table(src).unwrap_err();
        match err {
            MappingError::TriggerWithoutAction { line } => assert_eq!(line, 2),
            _ => panic!("wrong error : {err:?}"),
        }
    }

    #[test]
    fn parse_csl_table_unknown_key() {
        let src = r#"
action "x"
  trigger key NotAKey
"#;
        let err = ActionMap::from_csl_table(src).unwrap_err();
        match err {
            MappingError::UnknownKeyCode { text, .. } => assert_eq!(text, "NotAKey"),
            _ => panic!("wrong error : {err:?}"),
        }
    }

    #[test]
    fn parse_csl_table_invalid_threshold() {
        let src = r#"
action "x"
  trigger gamepad-axis LeftStickX foo
"#;
        let err = ActionMap::from_csl_table(src).unwrap_err();
        match err {
            MappingError::InvalidThreshold { text, .. } => assert_eq!(text, "foo"),
            _ => panic!("wrong error : {err:?}"),
        }
    }

    #[test]
    fn parse_csl_table_duplicate_action_rejected() {
        let src = r#"
action "x"
  trigger key A
action "x"
  trigger key B
"#;
        let err = ActionMap::from_csl_table(src).unwrap_err();
        assert!(matches!(err, MappingError::DuplicateAction(_)));
    }
}
