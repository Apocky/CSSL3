//! § List — vertical scrollable item list.
//!
//! § ROLE
//!   `List` displays a scrollable vertical column of text items with single-
//!   selection. Pointer-click selects a row ; Up/Down keys move the
//!   selection ; Home/End jump to bounds.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Retained list widget.
#[derive(Debug, Clone, Default)]
pub struct List {
    pub items: Vec<String>,
    pub selected: Option<usize>,
    pub disabled: bool,
    /// Per-row pixel height (default 22).
    pub row_height: f32,
    just_changed: bool,
    size: Size,
    scroll_offset: f32,
}

impl List {
    /// Construct a new list.
    #[must_use]
    pub fn new(items: Vec<String>) -> Self {
        Self {
            items,
            selected: None,
            disabled: false,
            row_height: 22.0,
            just_changed: false,
            size: Size::ZERO,
            scroll_offset: 0.0,
        }
    }

    /// `true` if the selection changed during the last event-pass.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }

    /// Pixel height of all items combined.
    #[must_use]
    pub fn content_height(&self) -> f32 {
        self.row_height * self.items.len() as f32
    }
}

impl Widget for List {
    fn type_tag(&self) -> &'static str {
        "List"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let glyph_w = 8.0_f32;
        let max_label_len = self
            .items
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0);
        let w = (max_label_len as f32 * glyph_w + 16.0).max(160.0);
        let h = self.content_height().max(self.row_height);
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
            UiEvent::PointerDown {
                button, position, ..
            } => {
                if matches!(button, cssl_host_window::event::MouseButton::Left) && ctx.hovered {
                    let local_y = position.y + self.scroll_offset;
                    let idx = (local_y / self.row_height) as usize;
                    if idx < self.items.len() {
                        if self.selected != Some(idx) {
                            self.selected = Some(idx);
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                        return EventResult::Consumed;
                    }
                }
            }
            UiEvent::Scroll { delta, .. } => {
                if !ctx.hovered {
                    return EventResult::Ignored;
                }
                let dy = match delta {
                    cssl_host_window::event::ScrollDelta::Lines { y, .. } => -y * self.row_height,
                    cssl_host_window::event::ScrollDelta::Pixels { y, .. } => -*y,
                };
                let max_scroll = (self.content_height() - self.size.h).max(0.0);
                self.scroll_offset = (self.scroll_offset + dy).clamp(0.0, max_scroll);
                return EventResult::Consumed;
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if !ctx.focused || !modifiers.is_empty() {
                    return EventResult::Ignored;
                }
                let n = self.items.len();
                if n == 0 {
                    return EventResult::Ignored;
                }
                let new_idx = match key {
                    cssl_host_window::event::KeyCode::Down => Some(match self.selected {
                        None => 0,
                        Some(i) => (i + 1).min(n - 1),
                    }),
                    cssl_host_window::event::KeyCode::Up => Some(match self.selected {
                        None => n - 1,
                        Some(i) => i.saturating_sub(1),
                    }),
                    cssl_host_window::event::KeyCode::Home => Some(0),
                    cssl_host_window::event::KeyCode::End => Some(n - 1),
                    _ => None,
                };
                if let Some(i) = new_idx {
                    if Some(i) != self.selected {
                        self.selected = Some(i);
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
        let outer = Rect::new(Point::ORIGIN, size);
        painter.fill_rect(
            outer,
            theme.color(ThemeSlot::Background),
            theme.corner_radius,
        );
        painter.stroke_rect(
            outer,
            theme.color(ThemeSlot::Border),
            1.0,
            theme.corner_radius,
        );
        painter.push_clip(outer);
        for (i, item) in self.items.iter().enumerate() {
            let y = i as f32 * self.row_height - self.scroll_offset;
            if y + self.row_height < 0.0 || y > size.h {
                continue;
            }
            let row = Rect::new(Point::new(0.0, y), Size::new(size.w, self.row_height));
            if self.selected == Some(i) {
                painter.fill_rect(row, theme.color(ThemeSlot::Selection), 0.0);
            }
            painter.text(
                Point::new(
                    theme.spacing.normal,
                    y + self.row_height * 0.5 + theme.font.size_px * 0.35,
                ),
                item,
                &theme.font,
                theme.color(ThemeSlot::Foreground),
            );
        }
        painter.pop_clip();
        if ctx.focused {
            painter.stroke_rect(
                outer,
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
    fn list_new_no_selection() {
        let l = List::new(vec!["a".into(), "b".into()]);
        assert!(l.selected.is_none());
    }

    #[test]
    fn list_pointer_selects_row() {
        let theme = Theme::default();
        let mut l = List::new(vec!["a".into(), "b".into(), "c".into()]);
        let _ = l.layout(LayoutConstraint::loose(Size::new(200.0, 200.0)));
        l.assign_final_size(Size::new(200.0, 100.0));
        // Row 1 spans y in [22, 44).
        let r = l.event(
            &UiEvent::PointerDown {
                position: Point::new(20.0, 30.0),
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
        assert_eq!(l.selected, Some(1));
    }

    #[test]
    fn list_arrow_down_advances() {
        let theme = Theme::default();
        let mut l = List::new(vec!["a".into(), "b".into()]);
        let r = l.event(
            &UiEvent::KeyDown {
                key: KeyCode::Down,
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
        assert_eq!(l.selected, Some(0));
    }

    #[test]
    fn list_home_end() {
        let theme = Theme::default();
        let mut l = List::new(vec!["a".into(), "b".into(), "c".into()]);
        let _ = l.event(
            &UiEvent::KeyDown {
                key: KeyCode::End,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext {
                theme: &theme,
                hovered: false,
                focused: true,
            },
        );
        assert_eq!(l.selected, Some(2));
        let _ = l.event(
            &UiEvent::KeyDown {
                key: KeyCode::Home,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext {
                theme: &theme,
                hovered: false,
                focused: true,
            },
        );
        assert_eq!(l.selected, Some(0));
    }

    #[test]
    fn list_disabled_ignores() {
        let theme = Theme::default();
        let mut l = List::new(vec!["a".into()]);
        l.disabled = true;
        let r = l.event(
            &UiEvent::KeyDown {
                key: KeyCode::Down,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext {
                theme: &theme,
                hovered: false,
                focused: true,
            },
        );
        assert_eq!(r, EventResult::Ignored);
    }

    #[test]
    fn list_content_height_matches_items() {
        let l = List::new(vec!["a".into(), "b".into(), "c".into()]);
        assert!((l.content_height() - 66.0).abs() < f32::EPSILON);
    }

    #[test]
    fn list_scroll_clamps_to_content() {
        let theme = Theme::default();
        let mut l = List::new(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
        ]);
        l.assign_final_size(Size::new(200.0, 50.0));
        // Content height = 110 ; visible 50 → max scroll 60.
        let r = l.event(
            &UiEvent::Scroll {
                position: Point::new(10.0, 10.0),
                delta: cssl_host_window::event::ScrollDelta::Lines { x: 0.0, y: -100.0 },
                modifiers: ModifierKeys::empty(),
            },
            EventContext {
                theme: &theme,
                hovered: true,
                focused: false,
            },
        );
        assert_eq!(r, EventResult::Consumed);
        assert!((l.scroll_offset - 60.0).abs() < f32::EPSILON);
    }
}
