//! § flame-graph builder · hierarchical aggregation of [`MarkPair`] data
//!
//! Builds a tree where each [`FlameNode`] represents a label and aggregates
//! `count` · `total_us` · `self_us` across all occurrences. The flat-collapsed
//! render produces lines compatible with Brendan Gregg's `flamegraph.pl`.
//!
//! ## Aggregation rule
//!
//! Pairs nest by [`MarkPair::depth`]. We rebuild parent-child relationships by
//! walking pairs sorted by `start_ts` and tracking a depth-stack. Two pairs
//! with the same label at the same path are merged (count += 1, total_us
//! summed, etc).
//!
//! ## `self_us`
//!
//! `self_us = total_us − sum-of-direct-children-total_us`. This is the time
//! the function spent **not** in any nested marked region — useful for
//! identifying leaf hot-spots vs. fan-out aggregators.

use crate::pair::MarkPair;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § a node in the flame-graph tree.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FlameNode {
    /// Label string for this node. Root is `"<root>"`.
    pub label: String,
    /// Microseconds spent in this label *not* attributable to children.
    pub self_us: u64,
    /// Microseconds spent in this label *including* time spent in children.
    pub total_us: u64,
    /// Number of times this label occurred at this path in the input pairs.
    pub count: u64,
    /// Child labels keyed by aggregation path (sorted by label for
    /// determinism).
    pub children: Vec<FlameNode>,
}

impl FlameNode {
    fn new_root() -> Self {
        Self {
            label: "<root>".to_owned(),
            self_us: 0,
            total_us: 0,
            count: 0,
            children: Vec::new(),
        }
    }
}

/// § build a flame-graph tree from a slice of [`MarkPair`]s.
///
/// Pairs are sorted by `start_ts` ascending so that the depth-stack
/// reconstruction reflects original nesting order. Parents are detected by
/// the (depth, time-window-overlap) relationship.
#[must_use]
pub fn build_flame_graph(pairs: &[MarkPair]) -> FlameNode {
    let mut sorted: Vec<&MarkPair> = pairs.iter().collect();
    sorted.sort_by_key(|p| (p.start_ts, p.depth));

    let mut root = FlameNode::new_root();
    // Stack of (path-string, ptr-into-tree) for current ancestors.
    // We use a path-key string instead of pointers because Rust's borrow-
    // checker disallows `&mut FlameNode` ancestor-stacks in safe code.
    // The path-key is "label0;label1;..;labelK" — compatible with the
    // collapsed-format we emit later.
    let mut stack: Vec<String> = Vec::new();

    for pair in sorted {
        // Trim stack to the pair's depth.
        stack.truncate(pair.depth as usize);
        // Compute parent path then push the new label.
        let parent_path = stack.clone();
        stack.push(pair.label.clone());

        // Walk to the parent node, creating intermediate nodes if needed.
        // (Intermediate creation should never fire if input pairs are
        // well-formed ; defensive for robustness.)
        let mut cursor: &mut FlameNode = &mut root;
        for ancestor in &parent_path {
            let idx = match cursor.children.iter().position(|c| &c.label == ancestor) {
                Some(i) => i,
                None => {
                    cursor.children.push(FlameNode {
                        label: ancestor.clone(),
                        self_us: 0,
                        total_us: 0,
                        count: 0,
                        children: Vec::new(),
                    });
                    cursor.children.len() - 1
                }
            };
            cursor = &mut cursor.children[idx];
        }

        // Now add (or merge) the pair's label as a child of cursor.
        let idx = match cursor.children.iter().position(|c| c.label == pair.label) {
            Some(i) => i,
            None => {
                cursor.children.push(FlameNode {
                    label: pair.label.clone(),
                    self_us: 0,
                    total_us: 0,
                    count: 0,
                    children: Vec::new(),
                });
                cursor.children.len() - 1
            }
        };
        let child = &mut cursor.children[idx];
        child.count += 1;
        child.total_us += pair.duration_us;
        // self_us starts equal to total_us ; subtracted-down later.
        child.self_us += pair.duration_us;
    }

    // Now compute self_us correctly : for each node, subtract sum of direct
    // children's total_us from its own self_us. We do this in a post-order
    // traversal.
    fixup_self_us(&mut root);
    // Sort children alphabetically for deterministic output.
    sort_children_recursive(&mut root);
    // Aggregate total_us at root from its top-level children for ergonomics.
    root.total_us = root.children.iter().map(|c| c.total_us).sum();
    root
}

fn fixup_self_us(node: &mut FlameNode) {
    let children_total: u64 = node.children.iter().map(|c| c.total_us).sum();
    node.self_us = node.self_us.saturating_sub(children_total);
    for c in &mut node.children {
        fixup_self_us(c);
    }
}

fn sort_children_recursive(node: &mut FlameNode) {
    node.children.sort_by(|a, b| a.label.cmp(&b.label));
    for c in &mut node.children {
        sort_children_recursive(c);
    }
}

fn collapsed_walk(
    node: &FlameNode,
    path: &mut Vec<String>,
    buckets: &mut BTreeMap<String, u64>,
) {
    if node.label != "<root>" {
        path.push(node.label.clone());
    }
    if !path.is_empty() && node.self_us > 0 {
        let key = path.join(";");
        *buckets.entry(key).or_insert(0) += node.self_us;
    }
    for c in &node.children {
        collapsed_walk(c, path, buckets);
    }
    if node.label != "<root>" {
        path.pop();
    }
}

/// § render the flame-graph tree to flat-collapsed format.
///
/// Output : one line per leaf-and-internal-node-with-self-time, formatted as
/// `parent;child;grandchild count_in_us`. Suitable for piping to
/// Brendan Gregg's `flamegraph.pl`.
#[must_use]
pub fn render_flat_collapsed(root: &FlameNode) -> String {
    // Aggregate self_us per stack-path. Internal nodes contribute their
    // self_us under their own path ; leaves contribute total_us (which equals
    // self_us for leaves anyway).
    let mut buckets: BTreeMap<String, u64> = BTreeMap::new();
    let mut path: Vec<String> = Vec::new();
    collapsed_walk(root, &mut path, &mut buckets);

    let mut out = String::new();
    for (k, v) in buckets {
        out.push_str(&k);
        out.push(' ');
        out.push_str(&v.to_string());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(label: &str, depth: u8, start: u64, dur: u64) -> MarkPair {
        MarkPair {
            label: label.to_owned(),
            start_ts: start,
            end_ts: start + dur,
            duration_us: dur,
            depth,
        }
    }

    #[test]
    fn empty_input_yields_root_only() {
        let root = build_flame_graph(&[]);
        assert_eq!(root.label, "<root>");
        assert!(root.children.is_empty());
        assert_eq!(root.total_us, 0);
    }

    #[test]
    fn single_leaf_aggregates() {
        let pairs = vec![pair("frame", 0, 0, 100)];
        let root = build_flame_graph(&pairs);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].label, "frame");
        assert_eq!(root.children[0].count, 1);
        assert_eq!(root.children[0].total_us, 100);
        assert_eq!(root.children[0].self_us, 100, "leaf self_us == total_us");
    }

    #[test]
    fn two_level_tree_self_us_correct() {
        // outer:0..100 (total 100) ; inner:10..40 (total 30) nested under outer.
        let pairs = vec![pair("outer", 0, 0, 100), pair("inner", 1, 10, 30)];
        let root = build_flame_graph(&pairs);
        assert_eq!(root.children.len(), 1);
        let outer = &root.children[0];
        assert_eq!(outer.label, "outer");
        assert_eq!(outer.total_us, 100);
        // outer.self_us = 100 (total) − 30 (inner.total) = 70
        assert_eq!(outer.self_us, 70);
        assert_eq!(outer.children.len(), 1);
        let inner = &outer.children[0];
        assert_eq!(inner.label, "inner");
        assert_eq!(inner.total_us, 30);
        assert_eq!(inner.self_us, 30);
    }

    #[test]
    fn multi_children_aggregate_under_same_parent() {
        // outer wraps two distinct-label inners + one repeated inner.
        let pairs = vec![
            pair("outer", 0, 0, 1000),
            pair("a", 1, 10, 50),
            pair("b", 1, 100, 100),
            pair("a", 1, 300, 50), // repeat of "a" under same outer
        ];
        let root = build_flame_graph(&pairs);
        let outer = &root.children[0];
        assert_eq!(outer.children.len(), 2, "a and b deduped to 2 children");
        // outer.self_us = 1000 − (a-total + b-total) = 1000 − 100 − 100 = 800
        assert_eq!(outer.self_us, 800);
        let a = outer.children.iter().find(|c| c.label == "a").unwrap();
        assert_eq!(a.count, 2, "a aggregated 2 occurrences");
        assert_eq!(a.total_us, 100);
        let b = outer.children.iter().find(|c| c.label == "b").unwrap();
        assert_eq!(b.count, 1);
        assert_eq!(b.total_us, 100);
    }

    #[test]
    fn collapsed_format_emits_semicolon_paths() {
        let pairs = vec![pair("outer", 0, 0, 100), pair("inner", 1, 10, 30)];
        let root = build_flame_graph(&pairs);
        let out = render_flat_collapsed(&root);
        // outer self_us = 70 ; inner self_us = 30.
        assert!(out.contains("outer 70\n"), "outer self : {out}");
        assert!(
            out.contains("outer;inner 30\n"),
            "inner with semicolon-path : {out}"
        );
        // Lines must be sorted alphabetically (BTreeMap guarantee).
        // Verify "outer" comes before "outer;inner" — both start with 'outer'
        // but the longer key sorts after due to BTreeMap key-order.
        let outer_pos = out.find("outer 70").unwrap();
        let nested_pos = out.find("outer;inner 30").unwrap();
        assert!(outer_pos < nested_pos);
    }
}
