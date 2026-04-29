//! § Image — bitmap reference widget.
//!
//! § ROLE
//!   `Image` references an external bitmap by handle. The actual pixel
//!   sourcing happens at the painter / render-graph level — this widget
//!   simply records the handle + intended draw rect.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Opaque handle identifying a bitmap registered with the painter.
///
/// Stage-0 keeps this as a `u64` — render backends carry their own table
/// keyed by this id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ImageHandle(pub u64);

/// Retained image widget.
#[derive(Debug, Clone)]
pub struct Image {
    pub handle: ImageHandle,
    pub fixed_size: Size,
    /// Border around the image (0 = none).
    pub border_width: f32,
    /// `true` to render a placeholder rect when handle = 0 (debug helper).
    pub placeholder_when_missing: bool,
}

impl Image {
    /// Construct an image.
    #[must_use]
    pub fn new(handle: ImageHandle, size: Size) -> Self {
        Self {
            handle,
            fixed_size: size,
            border_width: 0.0,
            placeholder_when_missing: true,
        }
    }
}

impl Widget for Image {
    fn type_tag(&self) -> &'static str {
        "Image"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        constraint.clamp(self.fixed_size)
    }

    fn event(&mut self, _event: &UiEvent, _ctx: EventContext<'_>) -> EventResult {
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let rect = Rect::new(Point::ORIGIN, size);
        if self.handle.0 == 0 && self.placeholder_when_missing {
            // Hatched placeholder : a filled rect + diagonal cross.
            painter.fill_rect(rect, theme.color(ThemeSlot::Disabled), 0.0);
            painter.stroke_line(
                Point::ORIGIN,
                Point::new(size.w, size.h),
                theme.color(ThemeSlot::Border),
                1.0,
            );
            painter.stroke_line(
                Point::new(0.0, size.h),
                Point::new(size.w, 0.0),
                theme.color(ThemeSlot::Border),
                1.0,
            );
        } else {
            // Real images would emit a textured-quad command via the painter.
            // Stage-0 records a fill so the rect is observable in PaintList.
            painter.fill_rect(rect, theme.color(ThemeSlot::Background), 0.0);
        }
        if self.border_width > 0.0 {
            painter.stroke_rect(rect, theme.color(ThemeSlot::Border), self.border_width, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::PaintList;
    use crate::theme::Theme;

    #[test]
    fn image_layout_returns_fixed_size() {
        let mut img = Image::new(ImageHandle(0), Size::new(64.0, 32.0));
        let s = img.layout(LayoutConstraint::loose(Size::new(200.0, 200.0)));
        assert_eq!(s, Size::new(64.0, 32.0));
    }

    #[test]
    fn image_event_always_ignored() {
        let theme = Theme::default();
        let mut img = Image::new(ImageHandle(0), Size::new(10.0, 10.0));
        let r = img.event(
            &UiEvent::WindowFocus,
            EventContext {
                theme: &theme,
                hovered: false,
                focused: false,
            },
        );
        assert_eq!(r, EventResult::Ignored);
    }

    #[test]
    fn image_placeholder_paints_diagonal_cross() {
        let theme = Theme::default();
        let img = Image::new(ImageHandle(0), Size::new(40.0, 40.0));
        let mut p = PaintList::new();
        img.paint(
            Size::new(40.0, 40.0),
            &mut p,
            PaintContext {
                theme: &theme,
                hovered: false,
                focused: false,
                active: false,
                disabled: false,
            },
        );
        // FillRect + 2 strokes for the diagonal cross.
        assert!(p
            .commands()
            .iter()
            .any(|c| matches!(c, crate::paint::PaintCommand::StrokeLine { .. })));
    }

    #[test]
    fn image_with_handle_no_placeholder() {
        let theme = Theme::default();
        let img = Image::new(ImageHandle(42), Size::new(40.0, 40.0));
        let mut p = PaintList::new();
        img.paint(
            Size::new(40.0, 40.0),
            &mut p,
            PaintContext {
                theme: &theme,
                hovered: false,
                focused: false,
                active: false,
                disabled: false,
            },
        );
        // Just the fill, no diagonals.
        assert!(!p
            .commands()
            .iter()
            .any(|c| matches!(c, crate::paint::PaintCommand::StrokeLine { .. })));
    }

    #[test]
    fn image_not_focusable() {
        let img = Image::new(ImageHandle(0), Size::ZERO);
        assert!(!img.focusable());
    }
}
