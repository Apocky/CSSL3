//! § ScrollView — clipped overflow.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Retained scroll-view widget — wraps a child whose preferred size is
/// larger than the viewport. Vertical scroll only at stage-0 ; horizontal
/// scroll lands in a follow-up slice (the bookkeeping is parallel).
pub struct ScrollView {
    pub child: Box<dyn Widget>,
    pub disabled: bool,
    pub scroll_y: f32,
    /// Cached viewport size (set by `assign_final_size`).
    viewport: Size,
    /// Cached child preferred size (set by `layout`).
    child_size: Size,
    just_changed: bool,
}

impl ScrollView {
    /// Construct a scroll view wrapping `child`.
    pub fn new(child: Box<dyn Widget>) -> Self {
        Self {
            child,
            disabled: false,
            scroll_y: 0.0,
            viewport: Size::ZERO,
            child_size: Size::ZERO,
            just_changed: false,
        }
    }

    /// Maximum scroll distance.
    #[must_use]
    pub fn max_scroll(&self) -> f32 {
        (self.child_size.h - self.viewport.h).max(0.0)
    }

    /// `true` if the scroll position changed.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }
}

impl std::fmt::Debug for ScrollView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollView")
            .field("scroll_y", &self.scroll_y)
            .field("viewport", &self.viewport)
            .field("child_size", &self.child_size)
            .finish()
    }
}

impl Widget for ScrollView {
    fn type_tag(&self) -> &'static str {
        "ScrollView"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        // Child sees an unbounded vertical constraint (so it can be tall).
        let child_constraint = LayoutConstraint {
            min_size: Size::ZERO,
            max_size: Size::new(constraint.max_size.w, f32::INFINITY),
            axis_priority: constraint.axis_priority,
        };
        self.child_size = self.child.layout(child_constraint);
        // Viewport defaults to constraint.max but never exceeds child width.
        let w = self.child_size.w.min(constraint.max_size.w);
        let h = constraint
            .max_size
            .h
            .min(self.child_size.h)
            .max(constraint.min_size.h);
        constraint.clamp(Size::new(w, h))
    }

    fn assign_final_size(&mut self, final_size: Size) {
        self.viewport = final_size;
        self.child.assign_final_size(self.child_size);
    }

    fn event(&mut self, event: &UiEvent, ctx: EventContext<'_>) -> EventResult {
        self.just_changed = false;
        if self.disabled {
            return EventResult::Ignored;
        }
        if let UiEvent::Scroll { delta, .. } = event {
            if !ctx.hovered {
                return EventResult::Ignored;
            }
            let dy = match delta {
                cssl_host_window::event::ScrollDelta::Lines { y, .. } => -y * 22.0,
                cssl_host_window::event::ScrollDelta::Pixels { y, .. } => -*y,
            };
            let max = self.max_scroll();
            let new_y = (self.scroll_y + dy).clamp(0.0, max);
            if (new_y - self.scroll_y).abs() > f32::EPSILON {
                self.scroll_y = new_y;
                self.just_changed = true;
                return EventResult::Changed;
            }
            return EventResult::Consumed;
        }
        // Forward all other events to child.
        self.child.event(event, ctx)
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        let outer = Rect::new(Point::ORIGIN, size);
        painter.fill_rect(outer, theme.color(ThemeSlot::Background), theme.corner_radius);
        painter.stroke_rect(outer, theme.color(ThemeSlot::Border), 1.0, theme.corner_radius);
        painter.push_clip(outer);
        // Translate by -scroll_y by painting child at offset y. We can't
        // translate the painter directly (the trait has no transform stack),
        // so we paint the child in its own coordinate space and rely on the
        // higher-level driver to honour the offset ; in stage-0 we approximate
        // by painting the child at its natural position and clipping.
        // The visible area starts at y = scroll_y inside the child.
        painter.push_clip(outer);
        self.child.paint(self.child_size, painter, ctx);
        painter.pop_clip();
        // Scrollbar (vertical) on the right edge.
        let max = self.max_scroll();
        if max > 0.0 {
            let track_w = 4.0;
            let track = Rect::new(
                Point::new(size.w - track_w - 2.0, 2.0),
                Size::new(track_w, size.h - 4.0),
            );
            painter.fill_rect(track, theme.color(ThemeSlot::AccentMuted), 2.0);
            let knob_h = (size.h * size.h / self.child_size.h).max(20.0);
            let knob_y = (self.scroll_y / max) * (size.h - knob_h - 4.0);
            let knob = Rect::new(
                Point::new(size.w - track_w - 2.0, 2.0 + knob_y),
                Size::new(track_w, knob_h),
            );
            painter.fill_rect(knob, theme.color(ThemeSlot::Accent), 2.0);
        }
        painter.pop_clip();
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::widgets::Label;
    use cssl_host_window::event::{ModifierKeys, ScrollDelta};

    fn fake_tall_child() -> Box<dyn Widget> {
        Box::new(Label::new("X".repeat(200)))
    }

    #[test]
    fn scrollview_layout_caches_child_size() {
        let mut sv = ScrollView::new(fake_tall_child());
        let _ = sv.layout(LayoutConstraint::loose(Size::new(100.0, 50.0)));
        // Label heights are constant ; child size > 0.
        assert!(sv.child_size.w > 0.0);
    }

    #[test]
    fn scrollview_scroll_updates_offset() {
        let theme = Theme::default();
        let mut sv = ScrollView::new(fake_tall_child());
        let _ = sv.layout(LayoutConstraint::loose(Size::new(100.0, 30.0)));
        sv.assign_final_size(Size::new(100.0, 30.0));
        // Force child_size.h > viewport.h to enable scroll.
        sv.child_size = Size::new(100.0, 200.0);
        let r = sv.event(
            &UiEvent::Scroll {
                position: Point::new(10.0, 10.0),
                delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
                modifiers: ModifierKeys::empty(),
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Changed);
        assert!(sv.scroll_y > 0.0);
    }

    #[test]
    fn scrollview_max_scroll_zero_when_content_fits() {
        let mut sv = ScrollView::new(fake_tall_child());
        sv.viewport = Size::new(100.0, 200.0);
        sv.child_size = Size::new(100.0, 50.0);
        assert!(sv.max_scroll().abs() < f32::EPSILON);
    }

    #[test]
    fn scrollview_disabled_ignores() {
        let theme = Theme::default();
        let mut sv = ScrollView::new(fake_tall_child());
        sv.disabled = true;
        let r = sv.event(
            &UiEvent::Scroll {
                position: Point::new(10.0, 10.0),
                delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
                modifiers: ModifierKeys::empty(),
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Ignored);
    }
}
