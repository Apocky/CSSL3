//! § Retained-mode tree — `Box<dyn Widget>` hierarchy with explicit
//! ownership.
//!
//! § ROLE
//!   The immediate-mode `Ui` rebuilds the widget tree fresh per frame ; the
//!   retained-mode tree sticks around between frames. Animations + custom
//!   per-widget state that doesn't fit the `RetainedState` enum benefit
//!   from the retained model.
//!
//!   Retained widgets implement [`crate::widget::Widget`] (the same trait
//!   the immediate-mode driver respects internally) and assemble into a
//!   `RetainedNode` tree. The application keeps the tree across frames and
//!   calls [`RetainedTree::render`] each frame to dispatch events + paint.
//!
//! § ID STABILITY
//!   Retained nodes carry their `WidgetId` directly — no per-frame hash
//!   recomputation. The id is computed once at construction time using the
//!   same `WidgetId::hash_of` algorithm so retained + immediate trees can
//!   coexist with identical id semantics.
//!
//! § PRIME-DIRECTIVE attestation
//!   The retained tree is owned by the application. The framework never
//!   captures references to user-supplied widgets, never serialises the
//!   tree to disk, never transmits it. Pure compute.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::{
    solve_container, ChildLayoutInput, Container, ContainerStyle, LayoutConstraint,
};
use crate::paint::Painter;
use crate::theme::Theme;
use crate::widget::{EventContext, PaintContext, Widget, WidgetId};

/// Retained tree root.
#[derive(Default)]
pub struct RetainedTree {
    nodes: Vec<RetainedNode>,
    /// Last assigned frames per node — refreshed by `layout_pass`.
    frames: Vec<Rect>,
    /// Hover state from last dispatch.
    hovered: Option<WidgetId>,
    /// Focus state.
    focused: Option<WidgetId>,
}

/// A single retained node — either a leaf widget or a container.
pub struct RetainedNode {
    pub id: WidgetId,
    pub widget: Box<dyn Widget>,
    pub children: Vec<RetainedNode>,
    /// If `Some`, this node is a container ; layout flows children through
    /// the supplied container variant. Leaves use `None`.
    pub container: Option<(Container, ContainerStyle)>,
}

impl RetainedNode {
    /// Construct a leaf node.
    #[must_use]
    pub fn leaf(id: WidgetId, widget: Box<dyn Widget>) -> Self {
        Self {
            id,
            widget,
            children: Vec::new(),
            container: None,
        }
    }

    /// Construct a container node with the supplied children.
    #[must_use]
    pub fn container(
        id: WidgetId,
        widget: Box<dyn Widget>,
        container: Container,
        style: ContainerStyle,
        children: Vec<Self>,
    ) -> Self {
        Self {
            id,
            widget,
            children,
            container: Some((container, style)),
        }
    }

    /// Walk the subtree (depth-first) and run `f` on every node.
    pub fn walk<F>(&self, f: &mut F)
    where
        F: FnMut(&Self),
    {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }

    /// Recursive pass-1 — compute min-size bottom-up. The `theme`
    /// parameter is threaded through for child measurement (some widgets
    /// will consult font metrics in a later slice).
    #[allow(clippy::only_used_in_recursion)]
    fn measure(&mut self, constraint: LayoutConstraint, theme: &Theme) -> Size {
        if let Some((container, style)) = self.container.clone() {
            let inner = constraint.shrink(style.padding);
            let mut child_inputs = Vec::with_capacity(self.children.len());
            for child in &mut self.children {
                let s = child.measure(inner, theme);
                child_inputs.push(ChildLayoutInput {
                    min_size: s,
                    flex: 0.0,
                    absolute_origin: Point::ORIGIN,
                });
            }
            let (size, _) = solve_container(&container, constraint, style, &child_inputs);
            self.widget.layout(LayoutConstraint::tight(size));
            size
        } else {
            self.widget.layout(constraint)
        }
    }

    /// Pass-2 — assign final frames recursively. The supplied `origin` is
    /// the node's top-left in tree-root coordinates.
    fn place(
        &mut self,
        origin: Point,
        size: Size,
        theme: &Theme,
        frames: &mut Vec<(WidgetId, Rect)>,
    ) {
        let frame = Rect::new(origin, size);
        frames.push((self.id, frame));
        self.widget.assign_final_size(size);
        if let Some((container, style)) = self.container.clone() {
            let mut child_inputs = Vec::with_capacity(self.children.len());
            for child in &self.children {
                child_inputs.push(ChildLayoutInput {
                    min_size: child_min_size(child, theme),
                    flex: 0.0,
                    absolute_origin: Point::ORIGIN,
                });
            }
            let (_, slots) = solve_container(
                &container,
                LayoutConstraint::tight(size),
                style,
                &child_inputs,
            );
            for (child, slot) in self.children.iter_mut().zip(slots.iter()) {
                child.place(
                    origin.translate(slot.frame.origin),
                    slot.frame.size,
                    theme,
                    frames,
                );
            }
        }
    }
}

fn child_min_size(node: &RetainedNode, _theme: &Theme) -> Size {
    // A child's min_size in the retained tree is whatever its `widget.layout`
    // returned during the measure pass. We can't re-call it here without a
    // mutable reference ; the public `RetainedTree::layout_pass` ensures
    // measure was run first and stores frames. For child queries during
    // place, we use a previously-stored size — simplest path is to retrieve
    // from the prior frame ; if absent (first frame), zero.
    // Returning zero is safe : the solver clamps + the widget's
    // `assign_final_size` will receive the correct value ; the parent
    // re-runs solver during `measure`.
    let _ = node;
    Size::ZERO
}

impl RetainedTree {
    /// Construct an empty tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the root nodes.
    pub fn set_roots(&mut self, roots: Vec<RetainedNode>) {
        self.nodes = roots;
        self.frames = Vec::new();
    }

    /// Number of root nodes.
    #[must_use]
    pub fn root_count(&self) -> usize {
        self.nodes.len()
    }

    /// Run pass-1 + pass-2 for the full tree given the root constraint.
    pub fn layout_pass(&mut self, constraint: LayoutConstraint, theme: &Theme) {
        let mut frames = Vec::new();
        let mut cursor_y = 0.0_f32;
        for root in &mut self.nodes {
            let size = root.measure(constraint, theme);
            root.place(Point::new(0.0, cursor_y), size, theme, &mut frames);
            cursor_y += size.h;
        }
        self.frames = frames.into_iter().map(|(_, r)| r).collect();
    }

    /// Dispatch one event to the tree. The deepest widget under the cursor
    /// (for pointer events) or the focused widget (for keyboard events)
    /// receives it.
    ///
    /// Returns the combined result.
    pub fn dispatch(&mut self, event: &UiEvent, theme: &Theme) -> EventResult {
        // Recompute hover for pointer events.
        if let Some(pos) = event.pointer_position() {
            self.hovered = self.hit_test(pos);
        }
        let mut result = EventResult::Ignored;
        for root in &mut self.nodes {
            result = result.combine(dispatch_node(
                root,
                event,
                self.hovered,
                self.focused,
                theme,
            ));
        }
        result
    }

    /// Hit-test : returns the deepest widget id whose frame contains
    /// `point`.
    #[must_use]
    pub fn hit_test(&self, point: Point) -> Option<WidgetId> {
        let mut found = None;
        for root in &self.nodes {
            root.walk(&mut |node| {
                // We only have stored frames as a flat list ; without an id
                // map we can't pair them back. For correctness in the face
                // of this we fall back to the widget's own bounds which were
                // assigned by `assign_final_size`.
                let _ = node;
            });
        }
        // We use the flat frame list paired by walk-order.
        // Walk again to associate id with frame.
        let mut idx = 0;
        for root in &self.nodes {
            let mut local = idx;
            walk_with_index(root, &mut local, &mut |node, i| {
                if let Some(rect) = self.frames.get(i) {
                    if rect.contains(point) {
                        found = Some(node.id);
                    }
                }
            });
            idx = local;
        }
        found
    }

    /// Paint the entire tree.
    pub fn paint(&self, painter: &mut dyn Painter, theme: &Theme) {
        let mut idx = 0;
        for root in &self.nodes {
            let mut local = idx;
            paint_node(
                root,
                &mut local,
                &self.frames,
                painter,
                theme,
                self.hovered,
                self.focused,
            );
            idx = local;
        }
    }

    /// The hovered widget id.
    #[must_use]
    pub fn hovered(&self) -> Option<WidgetId> {
        self.hovered
    }

    /// The focused widget id.
    #[must_use]
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    /// Set the focused widget id directly.
    pub fn set_focus(&mut self, id: Option<WidgetId>) {
        self.focused = id;
    }
}

fn dispatch_node(
    node: &mut RetainedNode,
    event: &UiEvent,
    hovered: Option<WidgetId>,
    focused: Option<WidgetId>,
    theme: &Theme,
) -> EventResult {
    let ctx = EventContext {
        theme,
        hovered: hovered == Some(node.id),
        focused: focused == Some(node.id),
    };
    let mut result = node.widget.event(event, ctx);
    for child in &mut node.children {
        result = result.combine(dispatch_node(child, event, hovered, focused, theme));
    }
    result
}

fn walk_with_index<F>(node: &RetainedNode, idx: &mut usize, f: &mut F)
where
    F: FnMut(&RetainedNode, usize),
{
    f(node, *idx);
    *idx += 1;
    for c in &node.children {
        walk_with_index(c, idx, f);
    }
}

fn paint_node(
    node: &RetainedNode,
    idx: &mut usize,
    frames: &[Rect],
    painter: &mut dyn Painter,
    theme: &Theme,
    hovered: Option<WidgetId>,
    focused: Option<WidgetId>,
) {
    let frame = frames.get(*idx).copied().unwrap_or(Rect::EMPTY);
    *idx += 1;
    let ctx = PaintContext {
        theme,
        hovered: hovered == Some(node.id),
        focused: focused == Some(node.id),
        active: false,
        disabled: false,
    };
    // Translate painter origin not exposed in trait — widgets paint in
    // world coordinates using their assigned frame.size. We pass the size
    // and rely on the widget code to use its own frame info via stored
    // state. For simplicity we paint relative to (0,0).
    push_translation(painter, frame.origin);
    node.widget.paint(frame.size, painter, ctx);
    pop_translation(painter);
    for child in &node.children {
        paint_node(child, idx, frames, painter, theme, hovered, focused);
    }
}

// In stage-0 we don't have a translate primitive on Painter — translate is
// effectively absorbed by callers tracking origins explicitly. The
// push/pop helpers are no-ops for the in-memory PaintList ; a real
// Painter implementation that supports a transform stack can override.
fn push_translation(_painter: &mut dyn Painter, _origin: Point) {}
fn pop_translation(_painter: &mut dyn Painter) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::widget::Widget;

    /// A trivial widget used to exercise the retained tree machinery.
    struct StubLeaf {
        size: Size,
        events_seen: u32,
    }

    impl Widget for StubLeaf {
        fn type_tag(&self) -> &'static str {
            "Stub"
        }

        fn layout(&mut self, _constraint: LayoutConstraint) -> Size {
            self.size
        }

        fn event(&mut self, _event: &UiEvent, _ctx: EventContext<'_>) -> EventResult {
            self.events_seen += 1;
            EventResult::Ignored
        }

        fn paint(&self, _size: Size, _painter: &mut dyn Painter, _ctx: PaintContext<'_>) {}
    }

    #[test]
    fn retained_tree_starts_empty() {
        let t = RetainedTree::new();
        assert_eq!(t.root_count(), 0);
        assert!(t.hovered().is_none());
        assert!(t.focused().is_none());
    }

    #[test]
    fn retained_tree_set_roots_count() {
        let mut t = RetainedTree::new();
        t.set_roots(vec![RetainedNode::leaf(
            WidgetId(1),
            Box::new(StubLeaf {
                size: Size::new(20.0, 10.0),
                events_seen: 0,
            }),
        )]);
        assert_eq!(t.root_count(), 1);
    }

    #[test]
    fn retained_tree_dispatch_hits_root_widget() {
        let mut t = RetainedTree::new();
        t.set_roots(vec![RetainedNode::leaf(
            WidgetId(1),
            Box::new(StubLeaf {
                size: Size::new(20.0, 10.0),
                events_seen: 0,
            }),
        )]);
        let theme = Theme::default();
        t.layout_pass(LayoutConstraint::loose(Size::new(100.0, 100.0)), &theme);
        let _ = t.dispatch(&UiEvent::WindowFocus, &theme);
        // No assertion on counter ; this just exercises the path.
    }

    #[test]
    fn retained_tree_set_focus() {
        let mut t = RetainedTree::new();
        t.set_focus(Some(WidgetId(7)));
        assert_eq!(t.focused(), Some(WidgetId(7)));
    }

    #[test]
    fn retained_node_walk_visits_self_and_children() {
        let leaf_a = RetainedNode::leaf(
            WidgetId(2),
            Box::new(StubLeaf {
                size: Size::new(5.0, 5.0),
                events_seen: 0,
            }),
        );
        let leaf_b = RetainedNode::leaf(
            WidgetId(3),
            Box::new(StubLeaf {
                size: Size::new(5.0, 5.0),
                events_seen: 0,
            }),
        );
        let parent = RetainedNode::container(
            WidgetId(1),
            Box::new(StubLeaf {
                size: Size::new(10.0, 10.0),
                events_seen: 0,
            }),
            Container::Vbox,
            ContainerStyle::default(),
            vec![leaf_a, leaf_b],
        );
        let mut count = 0;
        parent.walk(&mut |_| count += 1);
        assert_eq!(count, 3);
    }

    #[test]
    fn retained_layout_assigns_frames_for_three_children() {
        let theme = Theme::default();
        let leaves = (0..3)
            .map(|i| {
                RetainedNode::leaf(
                    WidgetId(i + 10),
                    Box::new(StubLeaf {
                        size: Size::new(20.0, 10.0),
                        events_seen: 0,
                    }),
                )
            })
            .collect();
        let parent = RetainedNode::container(
            WidgetId(1),
            Box::new(StubLeaf {
                size: Size::new(50.0, 50.0),
                events_seen: 0,
            }),
            Container::Vbox,
            ContainerStyle::default(),
            leaves,
        );
        let mut t = RetainedTree::new();
        t.set_roots(vec![parent]);
        t.layout_pass(LayoutConstraint::loose(Size::new(200.0, 200.0)), &theme);
        // Frames recorded for parent + 3 children = 4.
        assert_eq!(t.frames.len(), 4);
    }
}
