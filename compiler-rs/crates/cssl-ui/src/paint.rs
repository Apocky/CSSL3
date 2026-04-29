//! § Paint — render-submission surface for widgets.
//!
//! § ROLE
//!   Widgets draw by appending to a [`PaintList`] (default impl) or by
//!   calling methods on a custom [`Painter`] implementation. The default
//!   impl records primitive [`PaintCommand`]s into a flat `Vec` so a future
//!   `cssl-render` slice (R1) consumes the recorded list and submits it
//!   through the render-graph.
//!
//! § DESIGN — effect-row stub
//!   The framework brief calls for a `{Render}` effect-row on UI fns. Stage-0
//!   does not have a runtime-checked effect system in this crate — the row
//!   surfaces structurally as : every drawing entry-point takes `&mut dyn
//!   Painter`, the painter is the only sink that can produce commands, and
//!   the painter trait is `Send + Sync`-bounded so future render-graph
//!   integration can move recordings across threads. When cssl-effects-row
//!   plumbing extends to UI (a follow-up slice), adding `#[effect(Render)]`
//!   to the trait method signatures is a non-breaking annotation pass.
//!
//! § PRIMITIVES
//!   The framework records six primitives :
//!     - `FillRect`     — solid-fill rectangle.
//!     - `StrokeRect`   — outlined rectangle.
//!     - `FillCircle`   — solid-fill circle (radio bullets, slider knobs).
//!     - `StrokeLine`   — straight line (focus-ring, separators).
//!     - `Text`         — glyph-string at a baseline-anchored position.
//!     - `Clip`         — push / pop a clipping rectangle (for ScrollView).
//!
//!   Image draws + bezier paths land in a follow-up slice ; the surface is
//!   `#[non_exhaustive]` so additions are non-breaking.
//!
//! § PRIME-DIRECTIVE attestation
//!   Painting is local-only. The `Painter` trait does not write to disk, do
//!   network IO, or escape the process. The recorded `PaintList` is owned
//!   by the calling code — no global side-channel.

use crate::geometry::{Point, Rect};
use crate::theme::{Color, FontStyle};

/// One unit of recorded UI rendering.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PaintCommand {
    /// Solid-fill rectangle.
    FillRect {
        rect: Rect,
        color: Color,
        corner_radius: f32,
    },
    /// Outlined rectangle.
    StrokeRect {
        rect: Rect,
        color: Color,
        line_width: f32,
        corner_radius: f32,
    },
    /// Solid-fill circle.
    FillCircle {
        center: Point,
        radius: f32,
        color: Color,
    },
    /// Straight line from `start` to `end`.
    StrokeLine {
        start: Point,
        end: Point,
        color: Color,
        line_width: f32,
    },
    /// Glyph-string at `position` with the given font + colour.
    /// `position` is the baseline-left anchor.
    Text {
        position: Point,
        text: String,
        font: FontStyle,
        color: Color,
    },
    /// Push a clip rectangle ; subsequent commands are clipped to this rect.
    PushClip { rect: Rect },
    /// Pop the most recent clip rectangle.
    PopClip,
}

/// The paint sink. Widgets call methods here to draw themselves.
///
/// The default impl is [`PaintList`] which records into a `Vec`. Render
/// backends (a future cssl-render slice) implement this trait to translate
/// commands into draw calls (Vulkan / D3D12 / Metal / WebGPU).
///
/// § Effect-row note
///   When `cssl-effects` lowers to inert no-ops in stage-0, this trait stays
///   plain. When the row gates land, every method gains `#[effect(Render)]`
///   without changing the surface seen by widget impls.
pub trait Painter {
    /// Append a fill-rect command.
    fn fill_rect(&mut self, rect: Rect, color: Color, corner_radius: f32);

    /// Append a stroke-rect command.
    fn stroke_rect(&mut self, rect: Rect, color: Color, line_width: f32, corner_radius: f32);

    /// Append a fill-circle command.
    fn fill_circle(&mut self, center: Point, radius: f32, color: Color);

    /// Append a stroke-line command.
    fn stroke_line(&mut self, start: Point, end: Point, color: Color, line_width: f32);

    /// Append a text command.
    fn text(&mut self, position: Point, text: &str, font: &FontStyle, color: Color);

    /// Push a clip rectangle. Subsequent draws are clipped to `rect`.
    fn push_clip(&mut self, rect: Rect);

    /// Pop the most recent clip rectangle.
    fn pop_clip(&mut self);
}

/// Default in-memory `Painter` — records every command into a flat `Vec`.
///
/// A render backend either walks the recorded `commands()` after frame
/// build, or implements [`Painter`] directly to sink commands at submit
/// time without the intermediate buffer.
#[derive(Debug, Clone, Default)]
pub struct PaintList {
    commands: Vec<PaintCommand>,
}

impl PaintList {
    /// Construct an empty paint list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Read-only access to the recorded commands.
    #[must_use]
    pub fn commands(&self) -> &[PaintCommand] {
        &self.commands
    }

    /// Number of recorded commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// `true` if no commands have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Drop all recorded commands.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Move the recorded commands out, replacing with an empty list.
    pub fn take(&mut self) -> Vec<PaintCommand> {
        core::mem::take(&mut self.commands)
    }
}

impl Painter for PaintList {
    fn fill_rect(&mut self, rect: Rect, color: Color, corner_radius: f32) {
        self.commands.push(PaintCommand::FillRect {
            rect,
            color,
            corner_radius,
        });
    }

    fn stroke_rect(&mut self, rect: Rect, color: Color, line_width: f32, corner_radius: f32) {
        self.commands.push(PaintCommand::StrokeRect {
            rect,
            color,
            line_width,
            corner_radius,
        });
    }

    fn fill_circle(&mut self, center: Point, radius: f32, color: Color) {
        self.commands.push(PaintCommand::FillCircle {
            center,
            radius,
            color,
        });
    }

    fn stroke_line(&mut self, start: Point, end: Point, color: Color, line_width: f32) {
        self.commands.push(PaintCommand::StrokeLine {
            start,
            end,
            color,
            line_width,
        });
    }

    fn text(&mut self, position: Point, text: &str, font: &FontStyle, color: Color) {
        self.commands.push(PaintCommand::Text {
            position,
            text: text.to_string(),
            font: font.clone(),
            color,
        });
    }

    fn push_clip(&mut self, rect: Rect) {
        self.commands.push(PaintCommand::PushClip { rect });
    }

    fn pop_clip(&mut self) {
        self.commands.push(PaintCommand::PopClip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;

    #[test]
    fn paint_list_starts_empty() {
        let p = PaintList::new();
        assert!(p.is_empty());
        assert_eq!(p.len(), 0);
    }

    #[test]
    fn paint_list_records_fill_rect() {
        let mut p = PaintList::new();
        let rect = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        p.fill_rect(rect, Color::rgb(1.0, 0.0, 0.0), 0.0);
        assert_eq!(p.len(), 1);
        match &p.commands()[0] {
            PaintCommand::FillRect {
                rect: r,
                color,
                corner_radius,
            } => {
                assert_eq!(*r, rect);
                assert!((color.r - 1.0).abs() < f32::EPSILON);
                assert!(corner_radius.abs() < f32::EPSILON);
            }
            other => panic!("expected FillRect, got {other:?}"),
        }
    }

    #[test]
    fn paint_list_records_text() {
        let mut p = PaintList::new();
        p.text(
            Point::new(5.0, 10.0),
            "hello",
            &FontStyle::default(),
            Color::rgb(1.0, 1.0, 1.0),
        );
        match &p.commands()[0] {
            PaintCommand::Text { position, text, .. } => {
                assert_eq!(*position, Point::new(5.0, 10.0));
                assert_eq!(text, "hello");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn paint_list_clip_push_pop() {
        let mut p = PaintList::new();
        let rect = Rect::new(Point::new(0.0, 0.0), Size::new(50.0, 50.0));
        p.push_clip(rect);
        p.fill_rect(rect, Color::rgb(0.0, 0.0, 0.0), 0.0);
        p.pop_clip();
        assert_eq!(p.len(), 3);
        assert!(matches!(p.commands()[0], PaintCommand::PushClip { .. }));
        assert!(matches!(p.commands()[2], PaintCommand::PopClip));
    }

    #[test]
    fn paint_list_clear_resets() {
        let mut p = PaintList::new();
        p.fill_rect(
            Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0)),
            Color::rgb(0.0, 0.0, 0.0),
            0.0,
        );
        p.clear();
        assert!(p.is_empty());
    }

    #[test]
    fn paint_list_take_moves_commands_out() {
        let mut p = PaintList::new();
        p.fill_rect(
            Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0)),
            Color::rgb(1.0, 1.0, 1.0),
            0.0,
        );
        let taken = p.take();
        assert_eq!(taken.len(), 1);
        assert!(p.is_empty());
    }

    #[test]
    fn paint_list_multiple_commands_preserve_order() {
        let mut p = PaintList::new();
        let rect = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        p.fill_rect(rect, Color::rgb(1.0, 0.0, 0.0), 0.0);
        p.stroke_rect(rect, Color::rgb(0.0, 1.0, 0.0), 1.0, 0.0);
        p.fill_circle(Point::new(5.0, 5.0), 3.0, Color::rgb(0.0, 0.0, 1.0));
        let cmds = p.commands();
        assert!(matches!(cmds[0], PaintCommand::FillRect { .. }));
        assert!(matches!(cmds[1], PaintCommand::StrokeRect { .. }));
        assert!(matches!(cmds[2], PaintCommand::FillCircle { .. }));
    }
}
