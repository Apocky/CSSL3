//! § Slider — horizontal value picker.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Retained slider widget.
#[derive(Debug, Clone)]
pub struct Slider {
    pub label: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub disabled: bool,
    /// Step nudged on each arrow-key (default 5% of range).
    pub keyboard_step_fraction: f32,
    dragging: bool,
    just_changed: bool,
    size: Size,
}

impl Slider {
    /// Construct a new slider.
    #[must_use]
    pub fn new(label: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Self {
            label: label.into(),
            value: value.clamp(min, max),
            min,
            max,
            disabled: false,
            keyboard_step_fraction: 0.05,
            dragging: false,
            just_changed: false,
            size: Size::ZERO,
        }
    }

    /// `true` if the slider value changed during the last event-pass.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }

    fn snap_from_x(&self, x: f32) -> f32 {
        let w = self.size.w.max(1.0);
        let t = (x / w).clamp(0.0, 1.0);
        self.min + t * (self.max - self.min)
    }

    fn step(&self) -> f32 {
        (self.max - self.min) * self.keyboard_step_fraction
    }
}

impl Widget for Slider {
    fn type_tag(&self) -> &'static str {
        "Slider"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        constraint.clamp(Size::new(160.0, 22.0))
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
            UiEvent::PointerDown { button, position, .. } => {
                if matches!(button, cssl_host_window::event::MouseButton::Left)
                    && ctx.hovered
                {
                    self.dragging = true;
                    let new_value = self.snap_from_x(position.x);
                    if (new_value - self.value).abs() > f32::EPSILON {
                        self.value = new_value;
                        self.just_changed = true;
                        return EventResult::Changed;
                    }
                    return EventResult::Consumed;
                }
            }
            UiEvent::PointerMove { position, .. } => {
                if self.dragging {
                    let new_value = self.snap_from_x(position.x);
                    if (new_value - self.value).abs() > f32::EPSILON {
                        self.value = new_value;
                        self.just_changed = true;
                        return EventResult::Changed;
                    }
                }
            }
            UiEvent::PointerUp { button, .. } => {
                if matches!(button, cssl_host_window::event::MouseButton::Left)
                    && self.dragging
                {
                    self.dragging = false;
                    return EventResult::Consumed;
                }
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if !ctx.focused || !modifiers.is_empty() {
                    return EventResult::Ignored;
                }
                let step = self.step();
                let new_value = match key {
                    cssl_host_window::event::KeyCode::Right
                    | cssl_host_window::event::KeyCode::Up => Some(self.value + step),
                    cssl_host_window::event::KeyCode::Left
                    | cssl_host_window::event::KeyCode::Down => Some(self.value - step),
                    cssl_host_window::event::KeyCode::Home => Some(self.min),
                    cssl_host_window::event::KeyCode::End => Some(self.max),
                    _ => None,
                };
                if let Some(v) = new_value {
                    let clamped = v.clamp(self.min, self.max);
                    if (clamped - self.value).abs() > f32::EPSILON {
                        self.value = clamped;
                        self.just_changed = true;
                        return EventResult::Changed;
                    }
                    return EventResult::Consumed;
                }
            }
            _ => {}
        }
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        // Track.
        let track = Rect::new(
            Point::new(0.0, size.h * 0.5 - 2.0),
            Size::new(size.w, 4.0),
        );
        painter.fill_rect(track, theme.color(ThemeSlot::AccentMuted), 2.0);
        // Knob.
        let t = ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
        let knob_x = t * size.w;
        let knob_y = size.h * 0.5;
        painter.fill_circle(
            Point::new(knob_x, knob_y),
            size.h * 0.4,
            theme.color(ThemeSlot::Accent),
        );
        if ctx.focused {
            painter.stroke_rect(
                Rect::new(Point::ORIGIN, size),
                theme.color(ThemeSlot::Accent),
                theme.focus_ring_width,
                theme.corner_radius,
            );
        }
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
    fn slider_clamps_initial_value() {
        let s = Slider::new("v", 5.0, 0.0, 1.0);
        assert!((s.value - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn slider_arrow_right_increases() {
        let theme = Theme::default();
        let mut s = Slider::new("v", 0.5, 0.0, 1.0);
        s.assign_final_size(Size::new(160.0, 22.0));
        let r = s.event(
            &UiEvent::KeyDown {
                key: KeyCode::Right,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert!((s.value - 0.55).abs() < 0.001);
    }

    #[test]
    fn slider_home_jumps_to_min() {
        let theme = Theme::default();
        let mut s = Slider::new("v", 0.5, 0.0, 1.0);
        s.assign_final_size(Size::new(160.0, 22.0));
        let _ = s.event(
            &UiEvent::KeyDown {
                key: KeyCode::Home,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert!(s.value.abs() < f32::EPSILON);
    }

    #[test]
    fn slider_drag_updates_value() {
        let theme = Theme::default();
        let mut s = Slider::new("v", 0.0, 0.0, 1.0);
        s.assign_final_size(Size::new(160.0, 22.0));
        let _ = s.event(
            &UiEvent::PointerDown {
                position: Point::new(80.0, 11.0),
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert!((s.value - 0.5).abs() < 0.01);
    }

    #[test]
    fn slider_disabled_ignores_input() {
        let theme = Theme::default();
        let mut s = Slider::new("v", 0.5, 0.0, 1.0);
        s.disabled = true;
        s.assign_final_size(Size::new(160.0, 22.0));
        let r = s.event(
            &UiEvent::PointerDown {
                position: Point::new(80.0, 11.0),
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Ignored);
    }

    #[test]
    fn slider_focusable_unless_disabled() {
        let s = Slider::new("v", 0.0, 0.0, 1.0);
        assert!(s.focusable());
    }

    #[test]
    fn slider_keyboard_with_modifier_ignored() {
        let theme = Theme::default();
        let mut s = Slider::new("v", 0.5, 0.0, 1.0);
        s.assign_final_size(Size::new(160.0, 22.0));
        let r = s.event(
            &UiEvent::KeyDown {
                key: KeyCode::Right,
                modifiers: ModifierKeys::SHIFT,
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Ignored);
    }
}
