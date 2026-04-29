//! § Button widget.
//!
//! § ROLE
//!   Clickable rectangle with a text label. Reports a click event by
//!   transitioning its internal `pressed` flag to `true` for one frame
//!   after a primary-button release inside its bounds.
//!
//! § STATES
//!   - default       — unhovered, unpressed.
//!   - hovered       — cursor inside.
//!   - active        — primary-down on this widget, not yet released.
//!   - focused       — keyboard focus owner.
//!   - disabled      — non-interactive.
//!
//! § PRIME-DIRECTIVE attestation
//!   Local pure compute. No surveillance.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Retained-mode button widget.
#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
    pub disabled: bool,
    /// Internal flag set when the widget was clicked this frame.
    pressed: bool,
    /// Internal flag set when primary is currently held over this widget.
    holding: bool,
    /// Last-known assigned size (after pass-2).
    size: Size,
}

impl Button {
    /// Construct a new button with the supplied label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            disabled: false,
            pressed: false,
            holding: false,
            size: Size::ZERO,
        }
    }

    /// `true` if the button was clicked during the previous event-pass.
    /// The flag clears on each `event` call entry.
    #[must_use]
    pub fn was_pressed(&self) -> bool {
        self.pressed
    }

    /// Mark the button disabled / enabled.
    #[must_use]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Widget for Button {
    fn type_tag(&self) -> &'static str {
        "Button"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        // Button's preferred width = label-width + padding ; height = font + pad.
        let glyph_w = 8.0_f32; // Approx average glyph width at default font ; theme-aware fonts override.
        let pad = 12.0_f32;
        let preferred = Size::new(
            self.label.chars().count() as f32 * glyph_w + pad,
            22.0_f32,
        );
        constraint.clamp(preferred)
    }

    fn assign_final_size(&mut self, final_size: Size) {
        self.size = final_size;
    }

    fn event(&mut self, event: &UiEvent, ctx: EventContext<'_>) -> EventResult {
        // Reset one-shot flag.
        self.pressed = false;
        if self.disabled {
            return EventResult::Ignored;
        }
        match event {
            UiEvent::PointerDown { button, .. } => {
                if matches!(button, cssl_host_window::event::MouseButton::Left)
                    && ctx.hovered
                {
                    self.holding = true;
                    return EventResult::Consumed;
                }
            }
            UiEvent::PointerUp { button, .. } => {
                if matches!(button, cssl_host_window::event::MouseButton::Left)
                    && self.holding
                {
                    if ctx.hovered {
                        self.pressed = true;
                        self.holding = false;
                        return EventResult::Changed;
                    }
                    self.holding = false;
                    return EventResult::Consumed;
                }
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if matches!(
                    key,
                    cssl_host_window::event::KeyCode::Enter
                        | cssl_host_window::event::KeyCode::Space
                ) && ctx.focused
                    && modifiers.is_empty()
                {
                    self.pressed = true;
                    return EventResult::Changed;
                }
            }
            _ => {}
        }
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let face = if ctx.disabled || self.disabled {
            theme.color(ThemeSlot::Disabled)
        } else if ctx.active || self.holding {
            theme.color(ThemeSlot::ButtonActive)
        } else if ctx.hovered {
            theme.color(ThemeSlot::ButtonHover)
        } else {
            theme.color(ThemeSlot::ButtonFace)
        };
        let rect = crate::geometry::Rect::new(Point::ORIGIN, size);
        painter.fill_rect(rect, face, theme.corner_radius);
        painter.stroke_rect(rect, theme.color(ThemeSlot::Border), 1.0, theme.corner_radius);
        if ctx.focused {
            painter.stroke_rect(
                rect,
                theme.color(ThemeSlot::Accent),
                theme.focus_ring_width,
                theme.corner_radius,
            );
        }
        let text_color = if self.disabled {
            theme.color(ThemeSlot::ForegroundMuted)
        } else {
            theme.color(ThemeSlot::Foreground)
        };
        let baseline_y = size.h * 0.5 + theme.font.size_px * 0.35;
        painter.text(
            Point::new(theme.spacing.normal, baseline_y),
            &self.label,
            &theme.font,
            text_color,
        );
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn button_new_default_state() {
        let b = Button::new("OK");
        assert_eq!(b.label, "OK");
        assert!(!b.disabled);
        assert!(!b.was_pressed());
    }

    #[test]
    fn button_layout_within_constraint() {
        let mut b = Button::new("Save");
        let s = b.layout(LayoutConstraint::loose(Size::new(500.0, 500.0)));
        assert!(s.w > 0.0 && s.h > 0.0);
        assert!(s.w < 500.0);
    }

    #[test]
    fn button_assign_final_size_stores() {
        let mut b = Button::new("OK");
        b.assign_final_size(Size::new(80.0, 30.0));
        assert_eq!(b.size, Size::new(80.0, 30.0));
    }

    #[test]
    fn button_with_disabled_marks_disabled() {
        let b = Button::new("OK").with_disabled(true);
        assert!(b.disabled);
    }

    #[test]
    fn button_focusable_unless_disabled() {
        let b = Button::new("OK");
        assert!(b.focusable());
        let d = Button::new("OK").with_disabled(true);
        assert!(!d.focusable());
    }

    #[test]
    fn button_event_press_sets_pressed_on_release() {
        let theme = Theme::default();
        let mut b = Button::new("OK");
        // Down inside.
        let _ = b.event(
            &UiEvent::PointerDown {
                position: Point::ORIGIN,
                button: cssl_host_window::event::MouseButton::Left,
                modifiers: cssl_host_window::event::ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        // Up still hovered → click.
        let r = b.event(
            &UiEvent::PointerUp {
                position: Point::ORIGIN,
                button: cssl_host_window::event::MouseButton::Left,
                modifiers: cssl_host_window::event::ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(b.was_pressed());
    }

    #[test]
    fn button_event_release_outside_does_not_press() {
        let theme = Theme::default();
        let mut b = Button::new("OK");
        let _ = b.event(
            &UiEvent::PointerDown {
                position: Point::ORIGIN,
                button: cssl_host_window::event::MouseButton::Left,
                modifiers: cssl_host_window::event::ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        let r = b.event(
            &UiEvent::PointerUp {
                position: Point::ORIGIN,
                button: cssl_host_window::event::MouseButton::Left,
                modifiers: cssl_host_window::event::ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: false, focused: false },
        );
        assert_eq!(r, EventResult::Consumed);
        assert!(!b.was_pressed());
    }

    #[test]
    fn button_event_keyboard_enter_when_focused() {
        let theme = Theme::default();
        let mut b = Button::new("OK");
        let r = b.event(
            &UiEvent::KeyDown {
                key: cssl_host_window::event::KeyCode::Enter,
                modifiers: cssl_host_window::event::ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(b.was_pressed());
    }

    #[test]
    fn button_event_disabled_ignores_input() {
        let theme = Theme::default();
        let mut b = Button::new("OK").with_disabled(true);
        let r = b.event(
            &UiEvent::PointerDown {
                position: Point::ORIGIN,
                button: cssl_host_window::event::MouseButton::Left,
                modifiers: cssl_host_window::event::ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Ignored);
    }
}
