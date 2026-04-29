//! § TreeView — collapsible hierarchy.

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// Single node in a `TreeView`.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub label: String,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Construct a leaf.
    #[must_use]
    pub fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: false,
            children: Vec::new(),
        }
    }

    /// Construct a branch with children.
    #[must_use]
    pub fn branch(label: impl Into<String>, children: Vec<Self>) -> Self {
        Self {
            label: label.into(),
            expanded: false,
            children,
        }
    }

    /// Total visible row count when this subtree is rendered.
    #[must_use]
    pub fn visible_row_count(&self) -> usize {
        1 + if self.expanded {
            self.children.iter().map(Self::visible_row_count).sum()
        } else {
            0
        }
    }

    /// Recursive walk : assigns each visible row a depth + label.
    fn flatten(&self, depth: u8, into: &mut Vec<(u8, String, bool)>) {
        let has_children = !self.children.is_empty();
        into.push((depth, self.label.clone(), has_children && self.expanded));
        if self.expanded {
            for c in &self.children {
                c.flatten(depth + 1, into);
            }
        }
    }
}

/// Retained tree-view widget.
#[derive(Debug, Clone, Default)]
pub struct TreeView {
    pub roots: Vec<TreeNode>,
    pub selected: Option<usize>,
    pub disabled: bool,
    pub row_height: f32,
    just_changed: bool,
    size: Size,
    /// Cached flat row-list rebuilt on `layout`.
    flat: Vec<(u8, String, bool)>,
}

impl TreeView {
    /// Construct a tree.
    #[must_use]
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            roots,
            selected: None,
            disabled: false,
            row_height: 22.0,
            just_changed: false,
            size: Size::ZERO,
            flat: Vec::new(),
        }
    }

    /// `true` if the tree changed (selection or expansion).
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }

    fn rebuild_flat(&mut self) {
        self.flat.clear();
        for r in &self.roots {
            r.flatten(0, &mut self.flat);
        }
    }

    /// Toggle expansion at the visible-row index. Returns `true` if the
    /// expansion state changed.
    pub fn toggle_at(&mut self, idx: usize) -> bool {
        let mut path = self.path_to(idx);
        if path.is_empty() {
            return false;
        }
        // Walk roots / children using path, toggling the deepest.
        let last = path.pop().unwrap();
        let mut nodes = &mut self.roots;
        for &i in &path {
            nodes = &mut nodes[i].children;
        }
        if !nodes[last].children.is_empty() {
            nodes[last].expanded = !nodes[last].expanded;
            self.rebuild_flat();
            return true;
        }
        false
    }

    /// Convert a flat row index to the path-of-indices into `roots`.
    fn path_to(&self, mut target: usize) -> Vec<usize> {
        // Recursive walk.
        fn rec(nodes: &[TreeNode], target: &mut usize, path: &mut Vec<usize>) -> bool {
            for (i, n) in nodes.iter().enumerate() {
                if *target == 0 {
                    path.push(i);
                    return true;
                }
                *target -= 1;
                if n.expanded {
                    path.push(i);
                    if rec(&n.children, target, path) {
                        return true;
                    }
                    path.pop();
                }
            }
            false
        }
        let mut path = Vec::new();
        if rec(&self.roots, &mut target, &mut path) {
            path
        } else {
            Vec::new()
        }
    }
}

impl Widget for TreeView {
    fn type_tag(&self) -> &'static str {
        "TreeView"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        self.rebuild_flat();
        let glyph_w = 8.0_f32;
        let max_label_len = self
            .flat
            .iter()
            .map(|(d, l, _)| (*d as usize) * 2 + l.chars().count())
            .max()
            .unwrap_or(0);
        let w = max_label_len as f32 * glyph_w + 16.0;
        let h = (self.flat.len() as f32 * self.row_height).max(self.row_height);
        constraint.clamp(Size::new(w.max(160.0), h))
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
                if !matches!(button, cssl_host_window::event::MouseButton::Left) || !ctx.hovered {
                    return EventResult::Ignored;
                }
                let idx = (position.y / self.row_height) as usize;
                if idx >= self.flat.len() {
                    return EventResult::Ignored;
                }
                if self.selected != Some(idx) {
                    self.selected = Some(idx);
                    self.just_changed = true;
                }
                // Check chevron region (first 16 px of indent + chevron).
                let depth = self.flat[idx].0;
                let chevron_x = f32::from(depth).mul_add(12.0, 8.0);
                if position.x < chevron_x && self.toggle_at(idx) {
                    return EventResult::Changed;
                }
                if self.just_changed {
                    return EventResult::Changed;
                }
                return EventResult::Consumed;
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if !ctx.focused || !modifiers.is_empty() {
                    return EventResult::Ignored;
                }
                let n = self.flat.len();
                if n == 0 {
                    return EventResult::Ignored;
                }
                match key {
                    cssl_host_window::event::KeyCode::Down => {
                        let i = match self.selected {
                            None => 0,
                            Some(i) => (i + 1).min(n - 1),
                        };
                        if Some(i) != self.selected {
                            self.selected = Some(i);
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                    }
                    cssl_host_window::event::KeyCode::Up => {
                        let i = match self.selected {
                            None => n - 1,
                            Some(i) => i.saturating_sub(1),
                        };
                        if Some(i) != self.selected {
                            self.selected = Some(i);
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                    }
                    cssl_host_window::event::KeyCode::Right
                    | cssl_host_window::event::KeyCode::Left => {
                        if let Some(i) = self.selected {
                            if self.toggle_at(i) {
                                return EventResult::Changed;
                            }
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
        for (i, (depth, label, expanded)) in self.flat.iter().enumerate() {
            let y = i as f32 * self.row_height;
            if self.selected == Some(i) {
                let row = Rect::new(Point::new(0.0, y), Size::new(size.w, self.row_height));
                painter.fill_rect(row, theme.color(ThemeSlot::Selection), 0.0);
            }
            let x_indent = *depth as f32 * 12.0 + 4.0;
            // Chevron : a small dot.
            painter.fill_circle(
                Point::new(x_indent, y + self.row_height * 0.5),
                if *expanded { 3.5 } else { 2.5 },
                theme.color(ThemeSlot::Foreground),
            );
            painter.text(
                Point::new(
                    x_indent + 8.0,
                    y + self.row_height * 0.5 + theme.font.size_px * 0.35,
                ),
                label,
                &theme.font,
                theme.color(ThemeSlot::Foreground),
            );
        }
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
    use cssl_host_window::event::{KeyCode, ModifierKeys};

    fn sample_tree() -> Vec<TreeNode> {
        vec![
            TreeNode::branch(
                "root1",
                vec![TreeNode::leaf("child-a"), TreeNode::leaf("child-b")],
            ),
            TreeNode::leaf("root2"),
        ]
    }

    #[test]
    fn tree_visible_count_collapsed() {
        let t = TreeView::new(sample_tree());
        // Roots only (root1 not expanded, root2 leaf) → 2.
        let total: usize = t.roots.iter().map(TreeNode::visible_row_count).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn tree_layout_rebuilds_flat() {
        let mut t = TreeView::new(sample_tree());
        let _ = t.layout(LayoutConstraint::loose(Size::new(200.0, 200.0)));
        assert_eq!(t.flat.len(), 2);
    }

    #[test]
    fn tree_toggle_expands_branch() {
        let mut t = TreeView::new(sample_tree());
        let _ = t.layout(LayoutConstraint::loose(Size::new(200.0, 200.0)));
        assert!(t.toggle_at(0));
        // Now root1 is expanded → flat = root1, child-a, child-b, root2.
        assert_eq!(t.flat.len(), 4);
    }

    #[test]
    fn tree_arrow_down_advances() {
        let theme = Theme::default();
        let mut t = TreeView::new(sample_tree());
        let _ = t.layout(LayoutConstraint::loose(Size::new(200.0, 200.0)));
        let _ = t.event(
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
        assert_eq!(t.selected, Some(0));
    }

    #[test]
    fn tree_disabled_ignores() {
        let theme = Theme::default();
        let mut t = TreeView::new(sample_tree());
        t.disabled = true;
        let r = t.event(
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
}
