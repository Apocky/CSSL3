//! § Dropdown — popup menu picker.
//!
//! § ROLE
//!   A `Dropdown` collapses to a single row showing the current selection.
//!   Clicking it expands the option list ; clicking an option commits it
//!   and collapses.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Retained dropdown widget.
#[derive(Debug, Clone, Default)]
pub struct Dropdown {
    pub options: Vec<String>,
    pub selected_index: usize,
    pub disabled: bool,
    /// `true` if the option list is currently visible.
    pub expanded: bool,
    just_changed: bool,
    size: Size,
    /// Pixel-rect of the trigger row.
    trigger_h: f32,
}

impl Dropdown {
    /// Construct a new dropdown.
    #[must_use]
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        let n = options.len();
        Self {
            options,
            selected_index: selected.min(n.saturating_sub(1)),
            disabled: false,
            expanded: false,
            just_changed: false,
            size: Size::ZERO,
            trigger_h: 26.0,
        }
    }

    /// `true` if the selection changed during the last event-pass.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }

    fn option_at(&self, y: f32) -> Option<usize> {
        if !self.expanded {
            return None;
        }
        let row_h = self.trigger_h;
        let local_y = y - row_h;
        if local_y < 0.0 {
            return None;
        }
        let i = (local_y / row_h) as usize;
        if i < self.options.len() {
            Some(i)
        } else {
            None
        }
    }
}

impl Widget for Dropdown {
    fn type_tag(&self) -> &'static str {
        "Dropdown"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let glyph_w = 8.0_f32;
        let max_label_len = self
            .options
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0);
        let w = max_label_len as f32 * glyph_w + 36.0; // glyphs + chevron + padding
        let h = if self.expanded {
            self.trigger_h * (1 + self.options.len()) as f32
        } else {
            self.trigger_h
        };
        constraint.clamp(Size::new(w, h))
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
                if !matches!(button, cssl_host_window::event::MouseButton::Left) {
                    return EventResult::Ignored;
                }
                if !ctx.hovered {
                    if self.expanded {
                        self.expanded = false;
                        return EventResult::Changed;
                    }
                    return EventResult::Ignored;
                }
                if position.y <= self.trigger_h {
                    self.expanded = !self.expanded;
                    return EventResult::Changed;
                }
                if let Some(i) = self.option_at(position.y) {
                    if i != self.selected_index {
                        self.selected_index = i;
                        self.just_changed = true;
                    }
                    self.expanded = false;
                    return EventResult::Changed;
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
                match key {
                    cssl_host_window::event::KeyCode::Enter
                    | cssl_host_window::event::KeyCode::Space => {
                        self.expanded = !self.expanded;
                        return EventResult::Changed;
                    }
                    cssl_host_window::event::KeyCode::Down => {
                        let i = (self.selected_index + 1) % n;
                        if i != self.selected_index {
                            self.selected_index = i;
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                    }
                    cssl_host_window::event::KeyCode::Up => {
                        let i = (self.selected_index + n - 1) % n;
                        if i != self.selected_index {
                            self.selected_index = i;
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                    }
                    cssl_host_window::event::KeyCode::Escape => {
                        if self.expanded {
                            self.expanded = false;
                            return EventResult::Changed;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let trigger = Rect::new(Point::ORIGIN, Size::new(size.w, self.trigger_h));
        painter.fill_rect(trigger, theme.color(ThemeSlot::ButtonFace), theme.corner_radius);
        painter.stroke_rect(
            trigger,
            theme.color(ThemeSlot::Border),
            1.0,
            theme.corner_radius,
        );
        if ctx.focused {
            painter.stroke_rect(
                trigger,
                theme.color(ThemeSlot::Accent),
                theme.focus_ring_width,
                theme.corner_radius,
            );
        }
        let baseline_y = self.trigger_h * 0.5 + theme.font.size_px * 0.35;
        if let Some(label) = self.options.get(self.selected_index) {
            painter.text(
                Point::new(theme.spacing.normal, baseline_y),
                label,
                &theme.font,
                theme.color(ThemeSlot::Foreground),
            );
        }
        // Chevron : a small triangle on the right.
        painter.fill_circle(
            Point::new(size.w - theme.spacing.normal - 4.0, self.trigger_h * 0.5),
            3.0,
            theme.color(ThemeSlot::Foreground),
        );
        if self.expanded {
            for (i, opt) in self.options.iter().enumerate() {
                let row_y = self.trigger_h * (i as f32 + 1.0);
                let row = Rect::new(Point::new(0.0, row_y), Size::new(size.w, self.trigger_h));
                let face = if i == self.selected_index {
                    theme.color(ThemeSlot::Selection)
                } else {
                    theme.color(ThemeSlot::Background)
                };
                painter.fill_rect(row, face, 0.0);
                painter.text(
                    Point::new(
                        theme.spacing.normal,
                        row_y + self.trigger_h * 0.5 + theme.font.size_px * 0.35,
                    ),
                    opt,
                    &theme.font,
                    theme.color(ThemeSlot::Foreground),
                );
            }
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
    fn dropdown_new_clamps_index() {
        let d = Dropdown::new(vec!["a".into(), "b".into()], 99);
        assert_eq!(d.selected_index, 1);
    }

    #[test]
    fn dropdown_enter_toggles_expansion() {
        let theme = Theme::default();
        let mut d = Dropdown::new(vec!["a".into(), "b".into()], 0);
        let r = d.event(
            &UiEvent::KeyDown {
                key: KeyCode::Enter,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(d.expanded);
    }

    #[test]
    fn dropdown_down_advances_selection() {
        let theme = Theme::default();
        let mut d = Dropdown::new(vec!["a".into(), "b".into(), "c".into()], 0);
        let _ = d.event(
            &UiEvent::KeyDown {
                key: KeyCode::Down,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(d.selected_index, 1);
    }

    #[test]
    fn dropdown_click_outside_collapses() {
        let theme = Theme::default();
        let mut d = Dropdown::new(vec!["a".into(), "b".into()], 0);
        d.expanded = true;
        let r = d.event(
            &UiEvent::PointerDown {
                position: Point::new(0.0, 0.0),
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: false, focused: false },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(!d.expanded);
    }

    #[test]
    fn dropdown_escape_collapses_when_expanded() {
        let theme = Theme::default();
        let mut d = Dropdown::new(vec!["a".into()], 0);
        d.expanded = true;
        let r = d.event(
            &UiEvent::KeyDown {
                key: KeyCode::Escape,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(!d.expanded);
    }

    #[test]
    fn dropdown_disabled_ignores() {
        let theme = Theme::default();
        let mut d = Dropdown::new(vec!["a".into()], 0);
        d.disabled = true;
        let r = d.event(
            &UiEvent::KeyDown {
                key: KeyCode::Enter,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Ignored);
    }
}
