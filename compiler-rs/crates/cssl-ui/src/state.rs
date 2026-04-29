//! § Frame state — focus / hover / active widgets + retained-state store.
//!
//! § ROLE
//!   The `Ui` context (in `context.rs`) owns one `FrameState` per build.
//!   `FrameState` tracks :
//!     - the cursor position (mouse-tracking ; observable by `Ui`).
//!     - the currently hovered widget id (z-order tie-breaker).
//!     - the focused widget id (Tab navigation target).
//!     - the active widget id (currently-pressed button).
//!     - the retained-state store keyed by `WidgetId`.
//!
//!   The retained-state store survives across frames so a `Slider`'s value,
//!   a `Checkbox`'s on/off bit, and a `TextInput`'s buffer are remembered
//!   even though the immediate-mode caller rebuilds the widget tree fresh
//!   each frame.
//!
//! § FOCUS NAVIGATION (landmine)
//!   Tab order is depth-first traversal of focusable widgets. Shift-Tab is
//!   the reverse. The traversal order is captured by `Ui::register_focusable`
//!   (called by every focusable widget during its event-pass) ; the `Ui`
//!   maintains an ordered `Vec<WidgetId>` per frame. Hitting Tab cycles to
//!   the next id ; Shift-Tab to the previous.
//!
//! § HOVER STACKING (landmine)
//!   Multiple widgets may bound the cursor (e.g. a `Vbox` contains a
//!   `Button`). The deepest widget wins — which is the LAST registered
//!   during traversal because we visit children after the parent. The
//!   `FrameState::register_hover` records the most recent widget id whose
//!   bounds contain the cursor ; subsequent calls overwrite, so by the end
//!   of the frame `hover` holds the deepest hit.
//!
//! § PRIME-DIRECTIVE attestation
//!   `FrameState` is in-process state. The cursor position field is
//!   observable to the application via `Ui::cursor_position` — there is no
//!   silent surveillance of pointer movement. The retained store contains
//!   only widget-state values the application explicitly sets (slider
//!   floats, text-input strings).

use std::collections::HashMap;

use crate::geometry::Point;
use crate::widget::WidgetId;

/// Key codes for primary-axis keyboard navigation.
///
/// Mirrors the host-window `KeyCode` subset that drives Tab / Enter / Esc /
/// arrow-key navigation. A separate enum keeps `state.rs` decoupled from
/// the full host-window keyboard table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Tab,
    ShiftTab,
    Enter,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
}

/// Retained widget-state value — stored in the per-Ui hash map keyed by
/// `WidgetId`.
///
/// The variants cover the primitive shapes widgets need (bool / int /
/// float / string / pair). Adding a variant is a non-breaking change.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RetainedState {
    Bool(bool),
    Int(i64),
    Float(f32),
    Text(String),
    /// `(value, max)` pair — used by sliders + scrollbars.
    Range(f32, f32),
    /// Selected index in a multi-option widget.
    SelectedIndex(usize),
}

impl RetainedState {
    /// Try to read as `bool`.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to read as `i64`.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Try to read as `f32`.
    #[must_use]
    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Try to borrow as text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Try to read as a `(value, max)` pair.
    #[must_use]
    pub fn as_range(&self) -> Option<(f32, f32)> {
        match self {
            Self::Range(a, b) => Some((*a, *b)),
            _ => None,
        }
    }

    /// Try to read as a selected-index.
    #[must_use]
    pub fn as_selected_index(&self) -> Option<usize> {
        match self {
            Self::SelectedIndex(i) => Some(*i),
            _ => None,
        }
    }
}

/// Per-frame state the Ui owns.
#[derive(Debug, Clone, Default)]
pub struct FrameState {
    /// Cursor position in window-local coordinates (top-left origin).
    pub cursor: Point,
    /// Whether any pointer button is currently down (left primary).
    pub primary_down: bool,
    /// Currently hovered widget — None if cursor is outside any widget.
    pub hovered: Option<WidgetId>,
    /// Widget the keyboard focus belongs to — None if no focus.
    pub focused: Option<WidgetId>,
    /// Widget being actively pressed (mouse-down on this widget, mouse not
    /// yet released ; release-on-this triggers click).
    pub active: Option<WidgetId>,
    /// Frame number — increments on every `Ui::begin_frame`.
    pub frame_number: u64,
    /// Window client size in pixels (updated on `WindowResize`).
    pub window_size: (f32, f32),
    /// Retained state keyed by widget id. Survives across frames.
    pub retained: HashMap<WidgetId, RetainedState>,
    /// Ordered list of focusable widgets registered this frame ; Tab nav
    /// walks this list. Reset every `Ui::begin_frame`.
    pub focus_order: Vec<WidgetId>,
}

impl FrameState {
    /// Construct an empty frame state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get retained state for `id`, or `None` if not yet stored.
    #[must_use]
    pub fn get(&self, id: WidgetId) -> Option<&RetainedState> {
        self.retained.get(&id)
    }

    /// Get retained state for `id` ; if absent, insert the supplied default
    /// and return a mutable reference. Mirrors `entry().or_insert_with`.
    pub fn get_or_insert_with<F>(&mut self, id: WidgetId, default: F) -> &mut RetainedState
    where
        F: FnOnce() -> RetainedState,
    {
        self.retained.entry(id).or_insert_with(default)
    }

    /// Set retained state for `id`, overwriting any existing value.
    pub fn set(&mut self, id: WidgetId, value: RetainedState) {
        self.retained.insert(id, value);
    }

    /// Remove retained state for `id` ; returns the old value if any.
    pub fn remove(&mut self, id: WidgetId) -> Option<RetainedState> {
        self.retained.remove(&id)
    }

    /// Register a focusable widget — appended to the focus order. Called
    /// by every focusable widget during its event-pass.
    pub fn register_focusable(&mut self, id: WidgetId) {
        self.focus_order.push(id);
    }

    /// Move focus to the next focusable widget (`Tab` key).
    /// Returns the new focused id (None if no focusables registered).
    pub fn focus_next(&mut self) -> Option<WidgetId> {
        self.cycle_focus(true)
    }

    /// Move focus to the previous focusable widget (`Shift+Tab`).
    pub fn focus_prev(&mut self) -> Option<WidgetId> {
        self.cycle_focus(false)
    }

    fn cycle_focus(&mut self, forward: bool) -> Option<WidgetId> {
        if self.focus_order.is_empty() {
            return None;
        }
        let n = self.focus_order.len();
        let next = match self.focused {
            None => {
                if forward {
                    self.focus_order[0]
                } else {
                    self.focus_order[n - 1]
                }
            }
            Some(current) => {
                let pos = self.focus_order.iter().position(|i| *i == current);
                let i = pos.map_or(0, |p| {
                    if forward {
                        (p + 1) % n
                    } else {
                        (p + n - 1) % n
                    }
                });
                self.focus_order[i]
            }
        };
        self.focused = Some(next);
        Some(next)
    }

    /// Clear per-frame slots (focus order). Called by `Ui::begin_frame`.
    /// Retained state and focus selection persist.
    pub fn begin_frame(&mut self) {
        self.frame_number = self.frame_number.wrapping_add(1);
        self.focus_order.clear();
        // Hovered + active are reset because they're recomputed per-frame.
        self.hovered = None;
        // active persists between frames if a press is held.
    }

    /// Set the focused widget directly — used by `Ui::set_focus`.
    pub fn set_focus(&mut self, id: Option<WidgetId>) {
        self.focused = id;
    }

    /// Clear the active widget (called when primary-up fires anywhere).
    pub fn clear_active(&mut self) {
        self.active = None;
    }

    /// Set the active widget (called by widgets on primary-down inside
    /// their bounds).
    pub fn set_active(&mut self, id: WidgetId) {
        self.active = Some(id);
    }

    /// Update the hovered widget. Later calls overwrite earlier — by the
    /// end of frame the deepest widget under cursor wins.
    pub fn register_hover(&mut self, id: WidgetId) {
        self.hovered = Some(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_state_default_empty() {
        let s = FrameState::new();
        assert_eq!(s.frame_number, 0);
        assert!(s.focused.is_none());
        assert!(s.hovered.is_none());
        assert!(s.retained.is_empty());
    }

    #[test]
    fn frame_state_get_set_round_trip() {
        let mut s = FrameState::new();
        let id = WidgetId(42);
        s.set(id, RetainedState::Bool(true));
        assert_eq!(s.get(id).and_then(RetainedState::as_bool), Some(true));
    }

    #[test]
    fn frame_state_get_or_insert_inserts_default() {
        let mut s = FrameState::new();
        let id = WidgetId(7);
        let v = s.get_or_insert_with(id, || RetainedState::Int(99));
        assert_eq!(v.as_int(), Some(99));
    }

    #[test]
    fn frame_state_get_or_insert_keeps_existing() {
        let mut s = FrameState::new();
        let id = WidgetId(7);
        s.set(id, RetainedState::Int(11));
        let v = s.get_or_insert_with(id, || RetainedState::Int(99));
        assert_eq!(v.as_int(), Some(11)); // existing wins
    }

    #[test]
    fn retained_state_accessors() {
        assert_eq!(RetainedState::Bool(true).as_bool(), Some(true));
        assert_eq!(RetainedState::Bool(true).as_int(), None);
        assert_eq!(RetainedState::Int(7).as_int(), Some(7));
        assert_eq!(RetainedState::Float(1.5).as_float(), Some(1.5));
        assert_eq!(RetainedState::Text("hi".into()).as_text(), Some("hi"));
        assert_eq!(RetainedState::Range(0.5, 1.0).as_range(), Some((0.5, 1.0)));
        assert_eq!(RetainedState::SelectedIndex(2).as_selected_index(), Some(2));
    }

    #[test]
    fn focus_next_cycles_through_order() {
        let mut s = FrameState::new();
        s.register_focusable(WidgetId(1));
        s.register_focusable(WidgetId(2));
        s.register_focusable(WidgetId(3));
        assert_eq!(s.focus_next(), Some(WidgetId(1)));
        assert_eq!(s.focus_next(), Some(WidgetId(2)));
        assert_eq!(s.focus_next(), Some(WidgetId(3)));
        // Wraps.
        assert_eq!(s.focus_next(), Some(WidgetId(1)));
    }

    #[test]
    fn focus_prev_walks_backward() {
        let mut s = FrameState::new();
        s.register_focusable(WidgetId(1));
        s.register_focusable(WidgetId(2));
        s.register_focusable(WidgetId(3));
        // No initial focus → prev wraps to last.
        assert_eq!(s.focus_prev(), Some(WidgetId(3)));
        assert_eq!(s.focus_prev(), Some(WidgetId(2)));
        assert_eq!(s.focus_prev(), Some(WidgetId(1)));
        assert_eq!(s.focus_prev(), Some(WidgetId(3))); // wraps
    }

    #[test]
    fn focus_with_empty_order_returns_none() {
        let mut s = FrameState::new();
        assert_eq!(s.focus_next(), None);
        assert_eq!(s.focus_prev(), None);
    }

    #[test]
    fn begin_frame_resets_focus_order_and_hovered() {
        let mut s = FrameState::new();
        s.register_focusable(WidgetId(1));
        s.register_hover(WidgetId(2));
        s.begin_frame();
        assert!(s.focus_order.is_empty());
        assert!(s.hovered.is_none());
        assert_eq!(s.frame_number, 1);
    }

    #[test]
    fn begin_frame_keeps_retained_state() {
        let mut s = FrameState::new();
        s.set(WidgetId(1), RetainedState::Bool(true));
        s.begin_frame();
        assert_eq!(
            s.get(WidgetId(1)).and_then(RetainedState::as_bool),
            Some(true)
        );
    }

    #[test]
    fn register_hover_overwrites_previous_winner() {
        let mut s = FrameState::new();
        s.register_hover(WidgetId(1));
        s.register_hover(WidgetId(2));
        // Deepest hit (last registered) wins.
        assert_eq!(s.hovered, Some(WidgetId(2)));
    }

    #[test]
    fn remove_returns_old_value() {
        let mut s = FrameState::new();
        s.set(WidgetId(1), RetainedState::Int(5));
        let old = s.remove(WidgetId(1));
        assert_eq!(old.and_then(|v| v.as_int()), Some(5));
        assert!(s.get(WidgetId(1)).is_none());
    }
}
