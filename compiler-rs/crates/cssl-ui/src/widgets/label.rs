//! § Label — read-only text widget.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// A read-only text label.
#[derive(Debug, Clone)]
pub struct Label {
    pub text: String,
    pub muted: bool,
}

impl Label {
    /// Construct a label with the supplied text.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            muted: false,
        }
    }

    /// Mark the label muted (rendered in the foreground-muted colour).
    #[must_use]
    pub fn muted(mut self, m: bool) -> Self {
        self.muted = m;
        self
    }
}

impl Widget for Label {
    fn type_tag(&self) -> &'static str {
        "Label"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let glyph_w = 8.0_f32;
        let s = Size::new(self.text.chars().count() as f32 * glyph_w, 18.0_f32);
        constraint.clamp(s)
    }

    fn event(&mut self, _event: &UiEvent, _ctx: EventContext<'_>) -> EventResult {
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let color = if self.muted {
            theme.color(ThemeSlot::ForegroundMuted)
        } else {
            theme.color(ThemeSlot::Foreground)
        };
        let baseline_y = size.h * 0.5 + theme.font.size_px * 0.35;
        let _ = Rect::new(Point::ORIGIN, size); // explicit rect-construction kept for symmetry.
        painter.text(Point::new(0.0, baseline_y), &self.text, &theme.font, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn label_new_carries_text() {
        let l = Label::new("hello");
        assert_eq!(l.text, "hello");
        assert!(!l.muted);
    }

    #[test]
    fn label_muted_builder() {
        let l = Label::new("x").muted(true);
        assert!(l.muted);
    }

    #[test]
    fn label_layout_returns_text_sized() {
        let mut l = Label::new("hello");
        let s = l.layout(LayoutConstraint::loose(Size::new(500.0, 500.0)));
        assert!(s.w > 0.0);
    }

    #[test]
    fn label_event_always_ignored() {
        let theme = Theme::default();
        let mut l = Label::new("x");
        let r = l.event(
            &UiEvent::WindowFocus,
            EventContext { theme: &theme, hovered: false, focused: false },
        );
        assert_eq!(r, EventResult::Ignored);
    }

    #[test]
    fn label_not_focusable_default() {
        let l = Label::new("x");
        assert!(!l.focusable());
    }
}
