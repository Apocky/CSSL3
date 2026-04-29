//! § UI events — translated from `cssl-host-window::WindowEvent`.
//!
//! § ROLE
//!   The cssl-ui framework consumes a stream of [`UiEvent`]s. Application
//!   code feeds the host-window events through [`UiEvent::from_window`] (the
//!   common case) or constructs them directly for headless testing.
//!
//! § DESIGN
//!   `UiEvent` is a slimmer enum than `WindowEventKind` — UI cares about
//!   pointer-position + button-state + key-codes + modifiers, but does not
//!   need (and ignores) DPI-change / focus-loss-of-window etc. Those still
//!   flow through the `Ui` context (which retains them for application
//!   handlers) but the widget tree only sees the slim variants.
//!
//! § EVENT CATEGORIES
//!   - **Pointer** : `PointerMove` / `PointerDown` / `PointerUp` / `Scroll` —
//!     unified across mouse + touch (touch routes through the same surface
//!     with an additional `pointer_id : u32` so multitouch is forward-
//!     compatible).
//!   - **Keyboard** : `KeyDown` / `KeyUp` / `Char` — `Char` carries a
//!     decoded codepoint for `TextInput`. Stage-0 derives `Char` from
//!     `KeyDown` for ASCII printable keys ; full IME composition lands in
//!     a follow-up slice.
//!   - **Lifecycle** : `WindowResize` (so `Ui` can re-layout), `Focus` /
//!     `Unfocus` (for caret blink + suppression).
//!
//! § PRIME-DIRECTIVE — surveillance prohibition
//!   Every event surfaces as a transparent value. There is no hidden
//!   accumulator, no covert log, no shadow ring-buffer. The `Ui` may keep a
//!   per-frame queue but it is bounded + observable by `Ui::pending_events`.

use cssl_host_window::event::{KeyCode, ModifierKeys, MouseButton, ScrollDelta, WindowEventKind};

use crate::geometry::Point;

/// One UI input event.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum UiEvent {
    /// Pointer (mouse / touch / pen) moved to a new position.
    PointerMove {
        position: Point,
        modifiers: ModifierKeys,
        pointer_id: u32,
    },
    /// Pointer button pressed.
    PointerDown {
        position: Point,
        button: MouseButton,
        modifiers: ModifierKeys,
        pointer_id: u32,
    },
    /// Pointer button released.
    PointerUp {
        position: Point,
        button: MouseButton,
        modifiers: ModifierKeys,
        pointer_id: u32,
    },
    /// Wheel / trackpad / touch scroll.
    Scroll {
        position: Point,
        delta: ScrollDelta,
        modifiers: ModifierKeys,
    },
    /// Keyboard key pressed.
    KeyDown {
        key: KeyCode,
        modifiers: ModifierKeys,
        repeat: bool,
    },
    /// Keyboard key released.
    KeyUp {
        key: KeyCode,
        modifiers: ModifierKeys,
    },
    /// Character input (post-IME). The codepoint is a decoded Unicode scalar
    /// value — `TextInput` widgets append this directly.
    Char { ch: char, modifiers: ModifierKeys },
    /// Window was resized — `Ui` should re-layout to the new client size.
    WindowResize { width: f32, height: f32 },
    /// Window gained focus — `Ui` resumes caret blink etc.
    WindowFocus,
    /// Window lost focus — `Ui` should clear hover, keep focus state but
    /// suspend caret blink.
    WindowUnfocus,
}

impl UiEvent {
    /// Translate a host-window event into a UI event. Returns `None` for
    /// events the UI ignores (e.g. DPI change, which the application
    /// handles directly via `Ui::set_dpi_scale`).
    #[must_use]
    pub fn from_window(window_event: &WindowEventKind) -> Option<Self> {
        match window_event {
            WindowEventKind::MouseMove { x, y } => Some(Self::PointerMove {
                position: Point::new(*x as f32, *y as f32),
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            }),
            WindowEventKind::MouseDown {
                button,
                x,
                y,
                modifiers,
            } => Some(Self::PointerDown {
                position: Point::new(*x as f32, *y as f32),
                button: *button,
                modifiers: *modifiers,
                pointer_id: 0,
            }),
            WindowEventKind::MouseUp {
                button,
                x,
                y,
                modifiers,
            } => Some(Self::PointerUp {
                position: Point::new(*x as f32, *y as f32),
                button: *button,
                modifiers: *modifiers,
                pointer_id: 0,
            }),
            WindowEventKind::Scroll {
                delta,
                x,
                y,
                modifiers,
            } => Some(Self::Scroll {
                position: Point::new(*x as f32, *y as f32),
                delta: *delta,
                modifiers: *modifiers,
            }),
            WindowEventKind::KeyDown {
                key,
                modifiers,
                repeat,
            } => Some(Self::KeyDown {
                key: *key,
                modifiers: *modifiers,
                repeat: *repeat,
            }),
            WindowEventKind::KeyUp { key, modifiers } => Some(Self::KeyUp {
                key: *key,
                modifiers: *modifiers,
            }),
            WindowEventKind::Resize { width, height } => Some(Self::WindowResize {
                width: *width as f32,
                height: *height as f32,
            }),
            WindowEventKind::FocusGain => Some(Self::WindowFocus),
            WindowEventKind::FocusLoss => Some(Self::WindowUnfocus),
            // Close + DpiChanged are application-level concerns not handled
            // by widgets ; the application tracks them on the Ui itself.
            // The non_exhaustive wildcard catches future additions to
            // WindowEventKind ; UI ignores them by default.
            WindowEventKind::Close | WindowEventKind::DpiChanged { .. } => None,
            _ => None,
        }
    }

    /// Helper : pointer position if this event has one.
    #[must_use]
    pub fn pointer_position(&self) -> Option<Point> {
        match self {
            Self::PointerMove { position, .. }
            | Self::PointerDown { position, .. }
            | Self::PointerUp { position, .. }
            | Self::Scroll { position, .. } => Some(*position),
            _ => None,
        }
    }

    /// Helper : `true` if this event is a pointer-down for the left button.
    #[must_use]
    pub fn is_primary_press(&self) -> bool {
        matches!(
            self,
            Self::PointerDown {
                button: MouseButton::Left,
                ..
            }
        )
    }

    /// Helper : `true` if this event is a pointer-up for the left button.
    #[must_use]
    pub fn is_primary_release(&self) -> bool {
        matches!(
            self,
            Self::PointerUp {
                button: MouseButton::Left,
                ..
            }
        )
    }
}

/// What a widget did with an event — the return value of
/// [`crate::widget::Widget::event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventResult {
    /// Widget did nothing observable. The event continues to propagate.
    #[default]
    Ignored,
    /// Widget consumed the event ; do not propagate further.
    Consumed,
    /// Widget consumed the event AND its retained-state changed. The host
    /// application should request a redraw.
    Changed,
}

impl EventResult {
    /// `true` if the event has been claimed (and should not propagate).
    #[must_use]
    pub fn is_consumed(self) -> bool {
        matches!(self, Self::Consumed | Self::Changed)
    }

    /// `true` if the widget's state changed.
    #[must_use]
    pub fn is_changed(self) -> bool {
        matches!(self, Self::Changed)
    }

    /// Combine two results : the stronger result wins
    /// (Changed > Consumed > Ignored).
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Changed, _) | (_, Self::Changed) => Self::Changed,
            (Self::Consumed, _) | (_, Self::Consumed) => Self::Consumed,
            _ => Self::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_event_from_window_mouse_move() {
        let we = WindowEventKind::MouseMove { x: 50, y: 60 };
        let ue = UiEvent::from_window(&we).unwrap();
        match ue {
            UiEvent::PointerMove { position, .. } => {
                assert!((position.x - 50.0).abs() < f32::EPSILON);
                assert!((position.y - 60.0).abs() < f32::EPSILON);
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
    }

    #[test]
    fn ui_event_from_window_mouse_down_carries_button() {
        let we = WindowEventKind::MouseDown {
            button: MouseButton::Right,
            x: 10,
            y: 20,
            modifiers: ModifierKeys::SHIFT,
        };
        let ue = UiEvent::from_window(&we).unwrap();
        match ue {
            UiEvent::PointerDown {
                button, modifiers, ..
            } => {
                assert_eq!(button, MouseButton::Right);
                assert!(modifiers.contains(ModifierKeys::SHIFT));
            }
            other => panic!("expected PointerDown, got {other:?}"),
        }
    }

    #[test]
    fn ui_event_from_window_close_is_none() {
        let we = WindowEventKind::Close;
        assert!(UiEvent::from_window(&we).is_none());
    }

    #[test]
    fn ui_event_from_window_dpi_is_none() {
        let we = WindowEventKind::DpiChanged { scale: 2.0 };
        assert!(UiEvent::from_window(&we).is_none());
    }

    #[test]
    fn ui_event_pointer_position_for_pointer_events() {
        let e = UiEvent::PointerMove {
            position: Point::new(7.0, 8.0),
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        };
        assert_eq!(e.pointer_position(), Some(Point::new(7.0, 8.0)));
    }

    #[test]
    fn ui_event_pointer_position_none_for_keyboard() {
        let e = UiEvent::KeyDown {
            key: KeyCode::A,
            modifiers: ModifierKeys::empty(),
            repeat: false,
        };
        assert_eq!(e.pointer_position(), None);
    }

    #[test]
    fn ui_event_is_primary_press_only_for_left() {
        let left = UiEvent::PointerDown {
            position: Point::ORIGIN,
            button: MouseButton::Left,
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        };
        let right = UiEvent::PointerDown {
            position: Point::ORIGIN,
            button: MouseButton::Right,
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        };
        assert!(left.is_primary_press());
        assert!(!right.is_primary_press());
    }

    #[test]
    fn event_result_combine_picks_strongest() {
        assert_eq!(
            EventResult::Ignored.combine(EventResult::Consumed),
            EventResult::Consumed
        );
        assert_eq!(
            EventResult::Consumed.combine(EventResult::Changed),
            EventResult::Changed
        );
        assert_eq!(
            EventResult::Ignored.combine(EventResult::Ignored),
            EventResult::Ignored
        );
    }

    #[test]
    fn event_result_is_consumed_inclusive_of_changed() {
        assert!(EventResult::Consumed.is_consumed());
        assert!(EventResult::Changed.is_consumed());
        assert!(!EventResult::Ignored.is_consumed());
    }

    #[test]
    fn event_result_is_changed_only_for_changed() {
        assert!(EventResult::Changed.is_changed());
        assert!(!EventResult::Consumed.is_changed());
        assert!(!EventResult::Ignored.is_changed());
    }

    #[test]
    fn event_result_default_is_ignored() {
        assert_eq!(EventResult::default(), EventResult::Ignored);
    }
}
