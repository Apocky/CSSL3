//! § Widget trait + stable widget identifiers.
//!
//! § ROLE
//!   The `Widget` trait is the engine-side abstraction every retained-mode
//!   element implements (Button / Label / Slider / etc.). The trait has
//!   three concerns :
//!     - `event(&mut self, ev) -> EventResult`     — input handling.
//!     - `layout(&mut self, constraint) -> Size`   — size negotiation.
//!     - `paint(&self, painter, ctx)`              — rendering.
//!   Widgets keep their own retained state (toggle on/off, slider value,
//!   text-input buffer) ; the framework supplies layout + theming + input.
//!
//! § STABLE IDs (landmine)
//!   The brief calls for stable widget IDs across frames so the retained-
//!   state store survives re-render. The id hash combines :
//!     hash(widget-type-tag, label-bytes, parent-id, sibling-index)
//!   The widget-type-tag is a `&'static str` chosen by the widget impl ;
//!   the label-bytes are the user-supplied label (button caption, slider
//!   name) ; parent-id is the enclosing container's id ; sibling-index is
//!   the position among siblings of the same type. Same-label same-position
//!   widgets across frames hash to the same id even if the widget tree is
//!   rebuilt fresh per frame (the immediate-mode case).
//!
//!   The hash is `u64` FNV-1a — fast, no allocation, deterministic across
//!   compiles. We do NOT use `std::collections::hash_map::DefaultHasher`
//!   because its randomization would defeat cross-frame stability.
//!
//! § PRIME-DIRECTIVE attestation
//!   Widget IDs are not user-tracking. They are scoped to the running
//!   process, never persisted, never transmitted. They exist solely to map
//!   "this rendered widget" → "this retained-state slot" within one Ui.

use crate::event::{EventResult, UiEvent};
use crate::geometry::Size;
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::Theme;

/// Stable widget id — survives across frames as long as the widget's
/// type-tag, label, parent-id, and sibling-index are unchanged.
///
/// The id is opaque ; comparing for equality is the only supported op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WidgetId(pub u64);

impl WidgetId {
    /// The "no parent" sentinel — used for the root container.
    pub const ROOT: Self = Self(0);

    /// Compute a stable id from the four-tuple
    /// `(type_tag, label, parent, sibling_index)`.
    ///
    /// The hash is FNV-1a 64-bit ; deterministic + non-allocating.
    #[must_use]
    pub fn hash_of(type_tag: &str, label: &str, parent: Self, sibling_index: u32) -> Self {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

        let mut h = FNV_OFFSET;
        for byte in type_tag.bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
        // Separator so hash("ab","c") ≠ hash("a","bc").
        h ^= u64::from(b'\x1f');
        h = h.wrapping_mul(FNV_PRIME);
        for byte in label.bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
        h ^= u64::from(b'\x1f');
        h = h.wrapping_mul(FNV_PRIME);
        for byte in parent.0.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
        for byte in sibling_index.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
        Self(h)
    }

    /// Numeric value — exposed for hashmap-key debugging.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Read-only context every widget paint sees.
#[derive(Debug, Clone, Copy)]
pub struct PaintContext<'a> {
    /// The active theme.
    pub theme: &'a Theme,
    /// `true` if the cursor currently hovers this widget.
    pub hovered: bool,
    /// `true` if this widget owns keyboard focus.
    pub focused: bool,
    /// `true` if a press is currently held over this widget.
    pub active: bool,
    /// `true` if the widget is rendered in a disabled state.
    pub disabled: bool,
}

/// Read-only context every widget event-handler sees.
#[derive(Debug, Clone, Copy)]
pub struct EventContext<'a> {
    /// The active theme — useful for hit-test sizes that depend on font.
    pub theme: &'a Theme,
    /// `true` if the cursor entered this widget's bounds during this frame.
    pub hovered: bool,
    /// `true` if this widget owns keyboard focus.
    pub focused: bool,
}

/// Engine-side widget abstraction.
///
/// Implementors can be any owned type ; the framework boxes them only when
/// constructing a retained-mode tree. The immediate-mode driver constructs
/// stack-allocated widgets per frame and never boxes.
pub trait Widget {
    /// A short type-tag distinguishing this widget kind from others. Used
    /// in [`WidgetId::hash_of`] so two `Button("Save")` widgets in
    /// different positions still get distinct IDs through their parent +
    /// sibling-index, while two `Button("Save")` widgets at the same
    /// position across frames retain the same ID.
    fn type_tag(&self) -> &'static str;

    /// Pass-1 of layout : compute the minimum self-size given the parent's
    /// constraint. Pass-1 is bottom-up — children first, then containers
    /// add padding + sum / max. The default impl returns the constraint's
    /// `min_size` ; widgets override for content-driven sizing.
    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        constraint.min_size
    }

    /// Pass-2 hook : the widget has been assigned `final_size` by its
    /// container. Default no-op ; containers override to lay out children.
    fn assign_final_size(&mut self, _final_size: Size) {}

    /// Handle one UI event. Return [`EventResult::Ignored`] to let the
    /// event continue propagating ; [`EventResult::Consumed`] to claim it ;
    /// [`EventResult::Changed`] when the widget's retained state changed.
    fn event(&mut self, event: &UiEvent, ctx: EventContext<'_>) -> EventResult;

    /// Paint the widget. The widget's frame is `0,0..size` in painter-local
    /// coordinates ; the Ui has translated the painter's origin to the
    /// widget's top-left before calling.
    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>);

    /// `true` if this widget can take keyboard focus (Tab navigation
    /// reaches it). Default `false` ; override per-widget.
    fn focusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_id_root_is_zero() {
        assert_eq!(WidgetId::ROOT.raw(), 0);
    }

    #[test]
    fn widget_id_hash_deterministic() {
        let a = WidgetId::hash_of("Button", "Save", WidgetId::ROOT, 0);
        let b = WidgetId::hash_of("Button", "Save", WidgetId::ROOT, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn widget_id_hash_distinct_by_label() {
        let a = WidgetId::hash_of("Button", "Save", WidgetId::ROOT, 0);
        let b = WidgetId::hash_of("Button", "Cancel", WidgetId::ROOT, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn widget_id_hash_distinct_by_type_tag() {
        let a = WidgetId::hash_of("Button", "X", WidgetId::ROOT, 0);
        let b = WidgetId::hash_of("Label", "X", WidgetId::ROOT, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn widget_id_hash_distinct_by_parent() {
        let p1 = WidgetId(123);
        let p2 = WidgetId(456);
        let a = WidgetId::hash_of("Button", "X", p1, 0);
        let b = WidgetId::hash_of("Button", "X", p2, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn widget_id_hash_distinct_by_sibling_index() {
        let a = WidgetId::hash_of("Button", "X", WidgetId::ROOT, 0);
        let b = WidgetId::hash_of("Button", "X", WidgetId::ROOT, 1);
        assert_ne!(a, b);
    }

    #[test]
    fn widget_id_hash_separator_prevents_concat_collision() {
        // hash("ab","c") must differ from hash("a","bc").
        let a = WidgetId::hash_of("ab", "c", WidgetId::ROOT, 0);
        let b = WidgetId::hash_of("a", "bc", WidgetId::ROOT, 0);
        assert_ne!(a, b);
    }
}
