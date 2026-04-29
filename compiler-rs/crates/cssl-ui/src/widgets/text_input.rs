//! § TextInput — single-line text editor.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Retained text-input widget.
#[derive(Debug, Clone)]
pub struct TextInput {
    /// Placeholder shown when the buffer is empty AND the widget is unfocused.
    pub placeholder: String,
    pub buffer: String,
    pub disabled: bool,
    /// Maximum length in characters ; 0 = unlimited.
    pub max_length: usize,
    just_changed: bool,
    size: Size,
}

impl TextInput {
    /// Construct an empty text input with a placeholder.
    #[must_use]
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            buffer: String::new(),
            disabled: false,
            max_length: 0,
            just_changed: false,
            size: Size::ZERO,
        }
    }

    /// Replace the buffer with new content.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buffer = text.into();
    }

    /// `true` if the buffer changed during the last event-pass.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }

    /// `true` if the buffer is at or above the max-length cap.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.max_length > 0 && self.buffer.chars().count() >= self.max_length
    }
}

impl Widget for TextInput {
    fn type_tag(&self) -> &'static str {
        "TextInput"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        constraint.clamp(Size::new(180.0, 26.0))
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
            UiEvent::Char { ch, .. } => {
                if !ctx.focused {
                    return EventResult::Ignored;
                }
                if *ch == '\u{0008}' || *ch == '\u{007f}' {
                    if self.buffer.pop().is_some() {
                        self.just_changed = true;
                        return EventResult::Changed;
                    }
                    return EventResult::Consumed;
                }
                if ch.is_control() {
                    return EventResult::Ignored;
                }
                if self.is_full() {
                    return EventResult::Consumed;
                }
                self.buffer.push(*ch);
                self.just_changed = true;
                return EventResult::Changed;
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if !ctx.focused {
                    return EventResult::Ignored;
                }
                match key {
                    cssl_host_window::event::KeyCode::Backspace
                        if modifiers.is_empty() =>
                    {
                        if self.buffer.pop().is_some() {
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                        return EventResult::Consumed;
                    }
                    cssl_host_window::event::KeyCode::Delete if modifiers.is_empty() => {
                        // Stage-0 : Delete clears the whole buffer (no caret model yet).
                        if !self.buffer.is_empty() {
                            self.buffer.clear();
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                        return EventResult::Consumed;
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
        let rect = Rect::new(Point::ORIGIN, size);
        painter.fill_rect(rect, theme.color(ThemeSlot::ButtonFace), theme.corner_radius);
        let border_color = if ctx.focused {
            theme.color(ThemeSlot::Accent)
        } else {
            theme.color(ThemeSlot::Border)
        };
        let border_width = if ctx.focused { theme.focus_ring_width } else { 1.0 };
        painter.stroke_rect(rect, border_color, border_width, theme.corner_radius);
        let baseline_y = size.h * 0.5 + theme.font.size_px * 0.35;
        let text_origin = Point::new(theme.spacing.normal, baseline_y);
        if self.buffer.is_empty() && !ctx.focused {
            painter.text(
                text_origin,
                &self.placeholder,
                &theme.font,
                theme.color(ThemeSlot::ForegroundMuted),
            );
        } else {
            painter.text(
                text_origin,
                &self.buffer,
                &theme.font,
                theme.color(ThemeSlot::Foreground),
            );
            if ctx.focused {
                let caret_x = theme.spacing.normal
                    + (self.buffer.chars().count() as f32) * theme.font.size_px * 0.5;
                painter.stroke_line(
                    Point::new(caret_x, theme.spacing.normal),
                    Point::new(caret_x, size.h - theme.spacing.normal),
                    theme.color(ThemeSlot::Caret),
                    1.0,
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
    use cssl_host_window::event::{KeyCode, ModifierKeys};

    #[test]
    fn textinput_new_empty() {
        let t = TextInput::new("type here");
        assert_eq!(t.buffer, "");
        assert_eq!(t.placeholder, "type here");
    }

    #[test]
    fn textinput_set_text_overrides() {
        let mut t = TextInput::new("");
        t.set_text("hello");
        assert_eq!(t.buffer, "hello");
    }

    #[test]
    fn textinput_char_appends_when_focused() {
        let theme = Theme::default();
        let mut t = TextInput::new("");
        let r = t.event(
            &UiEvent::Char { ch: 'a', modifiers: ModifierKeys::empty() },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert_eq!(t.buffer, "a");
    }

    #[test]
    fn textinput_char_ignored_when_unfocused() {
        let theme = Theme::default();
        let mut t = TextInput::new("");
        let r = t.event(
            &UiEvent::Char { ch: 'a', modifiers: ModifierKeys::empty() },
            EventContext { theme: &theme, hovered: false, focused: false },
        );
        assert_eq!(r, EventResult::Ignored);
        assert!(t.buffer.is_empty());
    }

    #[test]
    fn textinput_backspace_pops_last() {
        let theme = Theme::default();
        let mut t = TextInput::new("");
        t.buffer = "hi".into();
        let r = t.event(
            &UiEvent::KeyDown {
                key: KeyCode::Backspace,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert_eq!(t.buffer, "h");
    }

    #[test]
    fn textinput_delete_clears() {
        let theme = Theme::default();
        let mut t = TextInput::new("");
        t.buffer = "hi".into();
        let r = t.event(
            &UiEvent::KeyDown {
                key: KeyCode::Delete,
                modifiers: ModifierKeys::empty(),
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(t.buffer.is_empty());
    }

    #[test]
    fn textinput_max_length_blocks_extra() {
        let theme = Theme::default();
        let mut t = TextInput::new("");
        t.max_length = 1;
        t.buffer = "x".into();
        let r = t.event(
            &UiEvent::Char { ch: 'a', modifiers: ModifierKeys::empty() },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Consumed);
        assert_eq!(t.buffer, "x");
    }

    #[test]
    fn textinput_disabled_ignores() {
        let theme = Theme::default();
        let mut t = TextInput::new("");
        t.disabled = true;
        let r = t.event(
            &UiEvent::Char { ch: 'a', modifiers: ModifierKeys::empty() },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Ignored);
    }

    #[test]
    fn textinput_focusable_unless_disabled() {
        let mut t = TextInput::new("");
        assert!(t.focusable());
        t.disabled = true;
        assert!(!t.focusable());
    }
}
