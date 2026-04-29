//! § Accessibility — surface API for screen-reader hooks.
//!
//! § ROLE
//!   Every widget has a logical "role" + "name" + "value" + "state" the
//!   accessibility layer can expose to assistive technology (screen
//!   readers, switch-control, voice control). The actual platform binding
//!   (UIA on Windows / AX on macOS / AT-SPI on Linux) lands in a follow-up
//!   slice — this module exposes the surface so widget impls can build the
//!   description today.
//!
//! § DESIGN
//!   - `AccessibilityRole` enumerates ARIA-aligned roles the framework
//!     supports (`Button`, `Slider`, `Checkbox`, `TextInput`, `Label`,
//!     `Dialog`, `Tab`, `TabPanel`, `List`, `ListItem`, `Tree`, `TreeItem`,
//!     `Group`, `Image`, `ProgressBar`, `Separator`).
//!   - `AccessibilityNode` is a node-tree mirror of the widget tree, but
//!     populated from the accessibility-relevant fields only.
//!   - The `Ui` exposes `Ui::accessibility_snapshot` (impl in `context.rs`)
//!     to build a tree the caller can hand to a platform AT bridge.
//!
//! § PRIME-DIRECTIVE — surveillance prohibition
//!   Accessibility is OPT-IN observation. The application chooses whether
//!   to call `accessibility_snapshot`. The framework never ships labels to
//!   external endpoints, never logs them, never persists them. The
//!   snapshot is a value the application owns.
//!
//! § DEFERRED
//!   - Live-region announcement. Screen-reader-driven action invocation.
//!     Per-widget keyboard shortcuts are documented in `KeyCode` events.

use crate::widget::WidgetId;

/// ARIA-style accessibility role enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AccessibilityRole {
    Group,
    Label,
    Button,
    Checkbox,
    RadioGroup,
    Radio,
    Slider,
    Dropdown,
    TextInput,
    List,
    ListItem,
    Tree,
    TreeItem,
    Tab,
    TabPanel,
    ProgressBar,
    Image,
    Separator,
    Dialog,
}

/// Accessibility state flags — bitset of common attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccessibilityState {
    pub disabled: bool,
    pub focused: bool,
    pub selected: bool,
    pub expanded: bool,
    pub checked: Option<bool>,
}

/// One accessibility-relevant value (e.g. slider value, text-input contents).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AccessibilityValue {
    None,
    Text(String),
    /// `(value, min, max)` for sliders + progress bars.
    Numeric {
        value: f32,
        min: f32,
        max: f32,
    },
    /// Boolean for toggles (some screen-readers prefer this over the
    /// `checked` state flag).
    Toggle(bool),
}

impl Default for AccessibilityValue {
    fn default() -> Self {
        Self::None
    }
}

/// One node in the accessibility snapshot tree.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibilityNode {
    pub id: WidgetId,
    pub role: AccessibilityRole,
    /// User-visible name (button caption, field label).
    pub name: String,
    /// Optional longer description (tooltip text, hint).
    pub description: String,
    pub state: AccessibilityState,
    pub value: AccessibilityValue,
    pub children: Vec<AccessibilityNode>,
}

impl AccessibilityNode {
    /// Construct a leaf node.
    #[must_use]
    pub fn leaf(id: WidgetId, role: AccessibilityRole, name: impl Into<String>) -> Self {
        Self {
            id,
            role,
            name: name.into(),
            description: String::new(),
            state: AccessibilityState::default(),
            value: AccessibilityValue::None,
            children: Vec::new(),
        }
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }

    /// Set the state flags.
    #[must_use]
    pub fn with_state(mut self, s: AccessibilityState) -> Self {
        self.state = s;
        self
    }

    /// Set the value.
    #[must_use]
    pub fn with_value(mut self, v: AccessibilityValue) -> Self {
        self.value = v;
        self
    }

    /// Append a child node.
    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Walk the subtree depth-first.
    pub fn walk<F>(&self, f: &mut F)
    where
        F: FnMut(&Self),
    {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }

    /// `true` if this node OR any descendant has `id`.
    #[must_use]
    pub fn contains_id(&self, id: WidgetId) -> bool {
        if self.id == id {
            return true;
        }
        self.children.iter().any(|c| c.contains_id(id))
    }
}

/// Top-level accessibility snapshot — a forest of root nodes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccessibilitySnapshot {
    pub roots: Vec<AccessibilityNode>,
}

impl AccessibilitySnapshot {
    /// Empty snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a root node.
    pub fn add_root(&mut self, node: AccessibilityNode) {
        self.roots.push(node);
    }

    /// Number of root nodes.
    #[must_use]
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    /// Total node count across all roots (depth-first).
    #[must_use]
    pub fn total_count(&self) -> usize {
        let mut n = 0;
        for r in &self.roots {
            r.walk(&mut |_| n += 1);
        }
        n
    }

    /// Find a node by id, depth-first.
    #[must_use]
    pub fn find(&self, id: WidgetId) -> Option<&AccessibilityNode> {
        for r in &self.roots {
            if let Some(node) = find_in(r, id) {
                return Some(node);
            }
        }
        None
    }
}

fn find_in(node: &AccessibilityNode, id: WidgetId) -> Option<&AccessibilityNode> {
    if node.id == id {
        return Some(node);
    }
    for c in &node.children {
        if let Some(n) = find_in(c, id) {
            return Some(n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_leaf_constructs() {
        let n = AccessibilityNode::leaf(WidgetId(1), AccessibilityRole::Button, "Save");
        assert_eq!(n.role, AccessibilityRole::Button);
        assert_eq!(n.name, "Save");
        assert!(n.children.is_empty());
    }

    #[test]
    fn node_with_state_overrides() {
        let s = AccessibilityState {
            disabled: true,
            focused: false,
            selected: true,
            expanded: false,
            checked: Some(true),
        };
        let n =
            AccessibilityNode::leaf(WidgetId(1), AccessibilityRole::Checkbox, "Mute").with_state(s);
        assert!(n.state.disabled);
        assert!(n.state.selected);
        assert_eq!(n.state.checked, Some(true));
    }

    #[test]
    fn node_with_value_numeric() {
        let n = AccessibilityNode::leaf(WidgetId(1), AccessibilityRole::Slider, "vol").with_value(
            AccessibilityValue::Numeric {
                value: 0.5,
                min: 0.0,
                max: 1.0,
            },
        );
        match n.value {
            AccessibilityValue::Numeric { value, .. } => {
                assert!((value - 0.5).abs() < f32::EPSILON);
            }
            _ => panic!("expected Numeric"),
        }
    }

    #[test]
    fn node_walk_visits_three() {
        let mut parent = AccessibilityNode::leaf(WidgetId(1), AccessibilityRole::Group, "g");
        parent.add_child(AccessibilityNode::leaf(
            WidgetId(2),
            AccessibilityRole::Button,
            "a",
        ));
        parent.add_child(AccessibilityNode::leaf(
            WidgetId(3),
            AccessibilityRole::Button,
            "b",
        ));
        let mut n = 0;
        parent.walk(&mut |_| n += 1);
        assert_eq!(n, 3);
    }

    #[test]
    fn node_contains_id_recursive() {
        let mut parent = AccessibilityNode::leaf(WidgetId(1), AccessibilityRole::Group, "g");
        parent.add_child(AccessibilityNode::leaf(
            WidgetId(2),
            AccessibilityRole::Button,
            "a",
        ));
        assert!(parent.contains_id(WidgetId(2)));
        assert!(!parent.contains_id(WidgetId(99)));
    }

    #[test]
    fn snapshot_total_count_walks_forest() {
        let mut snap = AccessibilitySnapshot::new();
        let mut a = AccessibilityNode::leaf(WidgetId(1), AccessibilityRole::Group, "g");
        a.add_child(AccessibilityNode::leaf(
            WidgetId(2),
            AccessibilityRole::Button,
            "x",
        ));
        let b = AccessibilityNode::leaf(WidgetId(3), AccessibilityRole::Label, "y");
        snap.add_root(a);
        snap.add_root(b);
        assert_eq!(snap.total_count(), 3);
    }

    #[test]
    fn snapshot_find_locates_nested_node() {
        let mut snap = AccessibilitySnapshot::new();
        let mut a = AccessibilityNode::leaf(WidgetId(1), AccessibilityRole::Group, "g");
        a.add_child(AccessibilityNode::leaf(
            WidgetId(2),
            AccessibilityRole::Button,
            "x",
        ));
        snap.add_root(a);
        assert_eq!(snap.find(WidgetId(2)).map(|n| n.name.as_str()), Some("x"));
    }

    #[test]
    fn accessibility_value_default_is_none() {
        let v: AccessibilityValue = AccessibilityValue::default();
        assert_eq!(v, AccessibilityValue::None);
    }
}
