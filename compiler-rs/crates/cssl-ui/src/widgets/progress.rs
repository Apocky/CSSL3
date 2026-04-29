//! § ProgressBar — read-only progress indicator.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Read-only progress bar widget.
#[derive(Debug, Clone)]
pub struct ProgressBar {
    pub value: f32,
    pub max: f32,
    pub indeterminate: bool,
}

impl ProgressBar {
    /// Construct a progress bar with the given current + max value.
    #[must_use]
    pub fn new(value: f32, max: f32) -> Self {
        Self {
            value: value.max(0.0),
            max: max.max(0.0001),
            indeterminate: false,
        }
    }

    /// Mark this bar as indeterminate (animated stripes).
    #[must_use]
    pub fn indeterminate(mut self, ind: bool) -> Self {
        self.indeterminate = ind;
        self
    }

    /// Fraction of max ; clamped to `0.0..=1.0`.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            return 0.0;
        }
        (self.value / self.max).clamp(0.0, 1.0)
    }
}

impl Widget for ProgressBar {
    fn type_tag(&self) -> &'static str {
        "ProgressBar"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        constraint.clamp(Size::new(160.0, 8.0))
    }

    fn event(&mut self, _event: &UiEvent, _ctx: EventContext<'_>) -> EventResult {
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let rect = Rect::new(Point::ORIGIN, size);
        painter.fill_rect(rect, theme.color(ThemeSlot::AccentMuted), theme.corner_radius);
        if self.indeterminate {
            // Stage-0 : draw a fixed-width slug at the centre.
            let slug_w = size.w * 0.3;
            let slug = Rect::new(
                Point::new((size.w - slug_w) * 0.5, 0.0),
                Size::new(slug_w, size.h),
            );
            painter.fill_rect(slug, theme.color(ThemeSlot::Accent), theme.corner_radius);
        } else {
            let fill = Rect::new(Point::ORIGIN, Size::new(size.w * self.fraction(), size.h));
            painter.fill_rect(fill, theme.color(ThemeSlot::Accent), theme.corner_radius);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fraction_full() {
        let p = ProgressBar::new(10.0, 10.0);
        assert!((p.fraction() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn progress_fraction_empty() {
        let p = ProgressBar::new(0.0, 10.0);
        assert!(p.fraction().abs() < f32::EPSILON);
    }

    #[test]
    fn progress_fraction_overflow_clamps() {
        let p = ProgressBar::new(50.0, 10.0);
        assert!((p.fraction() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn progress_indeterminate_builder() {
        let p = ProgressBar::new(0.0, 1.0).indeterminate(true);
        assert!(p.indeterminate);
    }

    #[test]
    fn progress_event_always_ignored() {
        let mut p = ProgressBar::new(0.5, 1.0);
        let theme = crate::theme::Theme::default();
        let r = p.event(
            &UiEvent::WindowFocus,
            EventContext { theme: &theme, hovered: false, focused: false },
        );
        assert_eq!(r, EventResult::Ignored);
    }
}
