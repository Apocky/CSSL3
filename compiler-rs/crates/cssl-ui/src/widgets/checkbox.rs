//! § Checkbox — toggle box with label.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Retained checkbox.
#[derive(Debug, Clone)]
pub struct Checkbox {
    pub label: String,
    pub value: bool,
    pub disabled: bool,
    holding: bool,
    just_changed: bool,
    size: Size,
}

impl Checkbox {
    /// Construct a checkbox with the supplied label + initial value.
    #[must_use]
    pub fn new(label: impl Into<String>, value: bool) -> Self {
        Self {
            label: label.into(),
            value,
            disabled: false,
            holding: false,
            just_changed: false,
            size: Size::ZERO,
        }
    }

    /// `true` if the checkbox toggled during the last event-pass.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }
}

impl Widget for Checkbox {
    fn type_tag(&self) -> &'static str {
        "Checkbox"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let h = 22.0_f32;
        let glyph_w = 8.0_f32;
        let pad = 8.0_f32;
        let preferred = Size::new(h + pad + self.label.chars().count() as f32 * glyph_w, h);
        constraint.clamp(preferred)
    }

    fn assign_final_size(&mut self, final_size: Size) {
        self.size = final_size;
    }

    fn event(&mut self, event: &UiEvent, ctx: EventContext<'_>) -> EventResult {
        self.just_changed = false;
        if self.disabled {
            return EventResult::Ignored;
        }
        match event {
            UiEvent::PointerDown { button, .. } => {
                if matches!(button, cssl_host_window::event::MouseButton::Left) && ctx.hovered {
                    self.holding = true;
                    return EventResult::Consumed;
                }
            }
            UiEvent::PointerUp { button, .. } => {
                if matches!(button, cssl_host_window::event::MouseButton::Left) && self.holding {
                    self.holding = false;
                    if ctx.hovered {
                        self.value = !self.value;
                        self.just_changed = true;
                        return EventResult::Changed;
                    }
                    return EventResult::Consumed;
                }
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if matches!(
                    key,
                    cssl_host_window::event::KeyCode::Space
                        | cssl_host_window::event::KeyCode::Enter
                ) && ctx.focused
                    && modifiers.is_empty()
                {
                    self.value = !self.value;
                    self.just_changed = true;
                    return EventResult::Changed;
                }
            }
            _ => {}
        }
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let h = size.h;
        let box_rect = Rect::new(Point::ORIGIN, Size::new(h, h));
        let face = if self.value {
            theme.color(ThemeSlot::Accent)
        } else {
            theme.color(ThemeSlot::ButtonFace)
        };
        painter.fill_rect(box_rect, face, theme.corner_radius);
        painter.stroke_rect(
            box_rect,
            theme.color(ThemeSlot::Border),
            1.0,
            theme.corner_radius,
        );
        if ctx.focused {
            painter.stroke_rect(
                box_rect,
                theme.color(ThemeSlot::Accent),
                theme.focus_ring_width,
                theme.corner_radius,
            );
        }
        if self.value {
            // Centre dot for the "checked" affordance.
            let cx = h * 0.5;
            let cy = h * 0.5;
            painter.fill_circle(
                Point::new(cx, cy),
                h * 0.2,
                theme.color(ThemeSlot::Foreground),
            );
        }
        let baseline_y = h * 0.5 + theme.font.size_px * 0.35;
        painter.text(
            Point::new(h + theme.spacing.normal, baseline_y),
            &self.label,
            &theme.font,
            theme.color(ThemeSlot::Foreground),
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
    use cssl_host_window::event::{KeyCode, ModifierKeys, MouseButton};

    #[test]
    fn checkbox_new_default() {
        let c = Checkbox::new("Mute", false);
        assert_eq!(c.label, "Mute");
        assert!(!c.value);
        assert!(!c.disabled);
    }

    #[test]
    fn checkbox_layout_min_includes_box_plus_label() {
        let mut c = Checkbox::new("hi", false);
        let s = c.layout(LayoutConstraint::loose(Size::new(500.0, 500.0)));
        assert!(s.w > s.h); // wider than tall once label included
    }

    #[test]
    fn checkbox_click_toggles() {
        let theme = Theme::default();
        let mut c = Checkbox::new("Mute", false);
        let _ = c.event(
            &UiEvent::PointerDown {
                position: Point::ORIGIN,
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext {
                theme: &theme,
                hovered: true,
                focused: false,
            },
        );
        let r = c.event(
            &UiEvent::PointerUp {
                position: Point::ORIGIN,
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext {
                theme: &theme,
                hovered: true,
                focused: false,
            },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(c.value);
        assert!(c.just_changed());
    }

    #[test]
    fn checkbox_keyboard_space_toggles_when_focused() {
        let theme = Theme::default();
        let mut c = Checkbox::new("Mute", false);
        let r = c.event(
            &UiEvent::KeyDown {
                key: KeyCode::Space,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext {
                theme: &theme,
                hovered: false,
                focused: true,
            },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(c.value);
    }

    #[test]
    fn checkbox_disabled_ignores() {
        let theme = Theme::default();
        let mut c = Checkbox::new("Mute", false);
        c.disabled = true;
        let r = c.event(
            &UiEvent::PointerDown {
                position: Point::ORIGIN,
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext {
                theme: &theme,
                hovered: true,
                focused: false,
            },
        );
        assert_eq!(r, EventResult::Ignored);
        assert!(!c.value);
    }

    #[test]
    fn checkbox_focusable_unless_disabled() {
        let mut c = Checkbox::new("x", false);
        assert!(c.focusable());
        c.disabled = true;
        assert!(!c.focusable());
    }
}
