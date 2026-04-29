//! § Radio + RadioGroup — mutually-exclusive selection.
//!
//! § ROLE
//!   `Radio` is a single radio button ; `RadioGroup` owns a set of options
//!   and ensures only one is selected at a time.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Single radio button — usually owned by a `RadioGroup`.
#[derive(Debug, Clone)]
pub struct Radio {
    pub label: String,
    pub selected: bool,
    pub disabled: bool,
}

impl Radio {
    /// Construct a radio.
    #[must_use]
    pub fn new(label: impl Into<String>, selected: bool) -> Self {
        Self {
            label: label.into(),
            selected,
            disabled: false,
        }
    }
}

impl Widget for Radio {
    fn type_tag(&self) -> &'static str {
        "Radio"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let h = 22.0_f32;
        let glyph_w = 8.0_f32;
        let pad = 8.0_f32;
        constraint.clamp(Size::new(h + pad + self.label.chars().count() as f32 * glyph_w, h))
    }

    fn event(&mut self, _event: &UiEvent, _ctx: EventContext<'_>) -> EventResult {
        // Group manages selection ; the radio itself only paints.
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let h = size.h;
        let cx = h * 0.5;
        let cy = h * 0.5;
        let r_outer = h * 0.4;
        let face = if self.selected {
            theme.color(ThemeSlot::Accent)
        } else {
            theme.color(ThemeSlot::ButtonFace)
        };
        painter.fill_circle(Point::new(cx, cy), r_outer, face);
        if self.selected {
            painter.fill_circle(
                Point::new(cx, cy),
                r_outer * 0.5,
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

/// A group of mutually-exclusive radios.
#[derive(Debug, Clone, Default)]
pub struct RadioGroup {
    pub label: String,
    pub options: Vec<String>,
    pub selected_index: usize,
    pub disabled: bool,
    just_changed: bool,
    size: Size,
    /// Pixel-bounds for each radio row, computed during layout.
    row_heights: Vec<f32>,
}

impl RadioGroup {
    /// Construct a new group.
    #[must_use]
    pub fn new(label: impl Into<String>, options: Vec<String>, selected: usize) -> Self {
        let n = options.len();
        Self {
            label: label.into(),
            options,
            selected_index: selected.min(n.saturating_sub(1)),
            disabled: false,
            just_changed: false,
            size: Size::ZERO,
            row_heights: Vec::new(),
        }
    }

    /// `true` if the selection changed during the last event-pass.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }

    fn row_at(&self, y: f32) -> Option<usize> {
        let mut acc = 0.0_f32;
        for (i, h) in self.row_heights.iter().enumerate() {
            if y >= acc && y < acc + h {
                return Some(i);
            }
            acc += h;
        }
        None
    }
}

impl Widget for RadioGroup {
    fn type_tag(&self) -> &'static str {
        "RadioGroup"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let h_row = 22.0_f32;
        self.row_heights = vec![h_row; self.options.len()];
        let glyph_w = 8.0_f32;
        let max_label_len = self
            .options
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0);
        let w = h_row + 8.0 + max_label_len as f32 * glyph_w;
        let total_h = h_row * self.options.len() as f32;
        constraint.clamp(Size::new(w, total_h))
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
                    if let Some(i) = self.row_at(position.y) {
                        if i != self.selected_index {
                            self.selected_index = i;
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                        return EventResult::Consumed;
                    }
                }
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if !ctx.focused || !modifiers.is_empty() {
                    return EventResult::Ignored;
                }
                let n = self.options.len();
                if n == 0 {
                    return EventResult::Ignored;
                }
                let new_idx = match key {
                    cssl_host_window::event::KeyCode::Up
                    | cssl_host_window::event::KeyCode::Left => {
                        Some((self.selected_index + n - 1) % n)
                    }
                    cssl_host_window::event::KeyCode::Down
                    | cssl_host_window::event::KeyCode::Right => {
                        Some((self.selected_index + 1) % n)
                    }
                    cssl_host_window::event::KeyCode::Home => Some(0),
                    cssl_host_window::event::KeyCode::End => Some(n - 1),
                    _ => None,
                };
                if let Some(i) = new_idx {
                    if i != self.selected_index {
                        self.selected_index = i;
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
        let mut y = 0.0_f32;
        for (i, label) in self.options.iter().enumerate() {
            let h = self.row_heights.get(i).copied().unwrap_or(22.0);
            let cx = h * 0.5;
            let cy = y + h * 0.5;
            let r_outer = h * 0.4;
            let face = if i == self.selected_index {
                theme.color(ThemeSlot::Accent)
            } else {
                theme.color(ThemeSlot::ButtonFace)
            };
            painter.fill_circle(Point::new(cx, cy), r_outer, face);
            if i == self.selected_index {
                painter.fill_circle(
                    Point::new(cx, cy),
                    r_outer * 0.5,
                    theme.color(ThemeSlot::Foreground),
                );
            }
            painter.text(
                Point::new(h + theme.spacing.normal, cy + theme.font.size_px * 0.35),
                label,
                &theme.font,
                theme.color(ThemeSlot::Foreground),
            );
            y += h;
        }
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
    use cssl_host_window::event::{KeyCode, ModifierKeys};

    #[test]
    fn radio_group_clamps_initial_selection() {
        let g = RadioGroup::new("opts", vec!["a".into(), "b".into()], 99);
        assert_eq!(g.selected_index, 1);
    }

    #[test]
    fn radio_group_arrow_down_advances() {
        let theme = Theme::default();
        let mut g = RadioGroup::new("opts", vec!["a".into(), "b".into(), "c".into()], 0);
        let _ = g.layout(LayoutConstraint::loose(Size::new(100.0, 100.0)));
        let r = g.event(
            &UiEvent::KeyDown {
                key: KeyCode::Down,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert_eq!(g.selected_index, 1);
    }

    #[test]
    fn radio_group_arrow_wraps() {
        let theme = Theme::default();
        let mut g = RadioGroup::new("opts", vec!["a".into(), "b".into()], 0);
        let _ = g.layout(LayoutConstraint::loose(Size::new(100.0, 100.0)));
        let _ = g.event(
            &UiEvent::KeyDown {
                key: KeyCode::Up,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(g.selected_index, 1); // wrapped from 0 → 1 (n-1 mod n)
    }

    #[test]
    fn radio_group_pointer_selects_row() {
        let theme = Theme::default();
        let mut g = RadioGroup::new("opts", vec!["a".into(), "b".into(), "c".into()], 0);
        let _ = g.layout(LayoutConstraint::loose(Size::new(100.0, 100.0)));
        // Row 1 spans y in [22, 44).
        let r = g.event(
            &UiEvent::PointerDown {
                position: Point::new(5.0, 30.0),
                button: cssl_host_window::event::MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Changed);
        assert_eq!(g.selected_index, 1);
    }

    #[test]
    fn radio_group_disabled_ignores() {
        let theme = Theme::default();
        let mut g = RadioGroup::new("opts", vec!["a".into(), "b".into()], 0);
        g.disabled = true;
        let r = g.event(
            &UiEvent::KeyDown {
                key: KeyCode::Down,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Ignored);
    }

    #[test]
    fn radio_widget_paints_without_panic() {
        // Smoke test : painting succeeds for both selected + unselected.
        let theme = Theme::default();
        let r = Radio::new("x", true);
        let mut p = crate::paint::PaintList::new();
        r.paint(
            Size::new(22.0, 22.0),
            &mut p,
            PaintContext {
                theme: &theme,
                hovered: false,
                focused: false,
                active: false,
                disabled: false,
            },
        );
        assert!(!p.is_empty());
    }
}
