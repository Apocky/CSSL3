//! § cssl-render::scene — SceneNode + SceneGraph + transform-propagation
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Scene-graph data structure : a tree of [`SceneNode`]s with hierarchical
//!   transform composition. Each node carries a local TRS transform, an
//!   optional mesh + material reference, an optional light, and a list of
//!   children. Transform propagation walks the tree depth-first composing
//!   `parent_world * child_local → child_world`.
//!
//! § STORAGE — flat arena via NodeId
//!   The scene graph is stored as a flat `Vec<SceneNode>` indexed by
//!   [`NodeId`] (newtype wrapping `u32`). This keeps the structure
//!   cache-friendly, avoids `Box<dyn ...>` indirection, and dovetails with
//!   ECS-style iteration. Hierarchy is encoded by `parent` + `first_child`
//!   + `next_sibling` linked-list pointers.
//!
//! § TRANSFORM PROPAGATION
//!   [`SceneGraph::propagate_transforms`] does a single linear pass
//!   computing world matrices for every reachable node. Caller invalidates
//!   the cache by mutating `local` ; recompute is O(N) where N = node count.
//!   Future slice : per-frame dirty-tracking + incremental propagation.
//!
//! § PRIME-DIRECTIVE — observer-perspective decoupling
//!   The scene graph stores world data ; nothing observer-specific. Per-
//!   camera frustum culling lives in `crate::queue` so the same scene
//!   can serve multiple observers with different visibility decisions.
//!   This matches the substrate-projections H3 design where multiple
//!   ObserverFrames coexist for split-screen / debug / mini-map.

use crate::asset::AssetHandle;
use crate::light::Light;
use crate::material::Material;
use crate::math::{Aabb, Mat4, Transform};
use crate::mesh::Mesh;

// ════════════════════════════════════════════════════════════════════════════
// § NodeId — opaque scene-node handle
// ════════════════════════════════════════════════════════════════════════════

/// Opaque scene-node handle. Newtype around `u32` to keep node references
/// distinct from raw indices + asset handles at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    /// Sentinel for "no node" — used by `parent` of root, `first_child`
    /// of leaves, and `next_sibling` of last children.
    pub const NONE: Self = Self(u32::MAX);

    /// True if this is a real node ID (not the NONE sentinel).
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::NONE
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § SceneNode — single node in the scene graph
// ════════════════════════════════════════════════════════════════════════════

/// A single node in the scene graph. Carries a local transform + optional
/// renderable payload (mesh / material / light) + tree-structure pointers.
///
/// § FIELD GROUPS
///   - **Local transform** : `local` is the per-node TRS in parent-space.
///   - **Cached world transform** : `world_matrix` is the propagated
///     world-space matrix, recomputed by `SceneGraph::propagate_transforms`.
///   - **Renderables** : `mesh` + `material` (optional) form a draw item.
///     `light` (optional) registers the node as a light source.
///   - **Tree structure** : `parent` + `first_child` + `next_sibling`
///     encode the hierarchy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneNode {
    /// Local-space TRS transform relative to parent.
    pub local: Transform,
    /// Cached world-space matrix. Recomputed by `propagate_transforms`.
    /// Stale until propagation runs ; consumers should not read this
    /// before calling propagate.
    pub world_matrix: Mat4,
    /// Optional mesh reference. `Mesh::EMPTY` (vertex_buffer == INVALID)
    /// means "no mesh on this node" ; the renderer skips this node's draw
    /// emission but still descends into children.
    pub mesh: Mesh,
    /// Material applied to `mesh`. Default-constructed PBR material if
    /// the consumer doesn't override.
    pub material: Material,
    /// Asset-database mesh handle (alternative path : if `mesh.is_drawable()`
    /// is false but this handle is valid, the asset crate provides the
    /// actual `Mesh` data at draw time).
    pub mesh_asset: AssetHandle<Mesh>,
    /// Optional light source. `None` = not a light. Propagated through
    /// the world transform so a light attached to a moving node moves
    /// with the node.
    pub light: Option<Light>,
    /// Whether this node + its subtree should be considered for rendering.
    /// `false` skips both this node's draws AND its descendants.
    pub visible: bool,

    // ─ Tree structure ─
    /// Parent node, or `NodeId::NONE` for root nodes.
    pub parent: NodeId,
    /// First child, or `NodeId::NONE` for leaves.
    pub first_child: NodeId,
    /// Next sibling in the parent's child-list, or `NodeId::NONE` for the
    /// last child.
    pub next_sibling: NodeId,
}

impl Default for SceneNode {
    fn default() -> Self {
        Self {
            local: Transform::IDENTITY,
            world_matrix: Mat4::IDENTITY,
            mesh: Mesh::EMPTY,
            material: Material::DEFAULT_PBR,
            mesh_asset: AssetHandle::INVALID,
            light: None,
            visible: true,
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            next_sibling: NodeId::NONE,
        }
    }
}

impl SceneNode {
    /// True if this node has any renderable content (mesh OR light).
    #[must_use]
    pub fn is_renderable(&self) -> bool {
        self.mesh.is_drawable() || self.mesh_asset.is_valid() || self.light.is_some()
    }

    /// World-space AABB derived from the local-space mesh AABB transformed
    /// by `world_matrix`. Empty AABB if no mesh or invalid.
    #[must_use]
    pub fn world_aabb(&self) -> Aabb {
        if self.mesh.is_drawable() {
            self.mesh.local_aabb.transform(self.world_matrix)
        } else {
            Aabb::EMPTY
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § SceneGraph — flat-arena tree
// ════════════════════════════════════════════════════════════════════════════

/// The scene graph itself : a flat arena of `SceneNode`s + a list of root
/// node IDs. Roots can be multiple — the graph is a forest, not a tree,
/// to support disjoint world segments (e.g. a level + a debug-overlay
/// hierarchy that shouldn't transform-couple).
#[derive(Debug, Default, Clone)]
pub struct SceneGraph {
    /// Node arena. NodeId(i) indexes `nodes[i]`.
    pub nodes: Vec<SceneNode>,
    /// Root node IDs. World transform of a root = its local transform.
    pub roots: Vec<NodeId>,
}

/// Errors that can occur during scene-graph operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SceneError {
    /// Node ID does not refer to a valid node in this graph.
    #[error("scene-graph: invalid node id {0:?}")]
    InvalidNode(NodeId),
    /// Attempted to attach a node that would create a cycle.
    #[error("scene-graph: cyclic parent assignment ({child:?} -> {parent:?})")]
    CyclicParent { parent: NodeId, child: NodeId },
}

impl SceneGraph {
    /// Construct an empty scene graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes in the graph.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the graph has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Borrow a node by ID.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&SceneNode> {
        self.nodes.get(id.0 as usize)
    }

    /// Mutably borrow a node by ID.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut SceneNode> {
        self.nodes.get_mut(id.0 as usize)
    }

    /// Add a new root node with the given transform + optional mesh/material.
    /// Returns the new node's ID.
    pub fn add_root(&mut self, local: Transform) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        let mut node = SceneNode {
            local,
            ..SceneNode::default()
        };
        node.world_matrix = local.to_matrix();
        self.nodes.push(node);
        self.roots.push(id);
        id
    }

    /// Add a child of an existing node. Returns the new child's ID.
    pub fn add_child(&mut self, parent: NodeId, local: Transform) -> Result<NodeId, SceneError> {
        if !parent.is_valid() || parent.0 as usize >= self.nodes.len() {
            return Err(SceneError::InvalidNode(parent));
        }
        let child_id = NodeId(self.nodes.len() as u32);
        let mut child = SceneNode {
            local,
            parent,
            ..SceneNode::default()
        };
        child.world_matrix = local.to_matrix();
        self.nodes.push(child);

        // Splice into parent's child-list at the head — O(1) insertion.
        let old_first = self.nodes[parent.0 as usize].first_child;
        self.nodes[child_id.0 as usize].next_sibling = old_first;
        self.nodes[parent.0 as usize].first_child = child_id;

        Ok(child_id)
    }

    /// Walk the tree depth-first computing world matrices for every node.
    /// Caller calls this once per frame after mutating any local transforms.
    pub fn propagate_transforms(&mut self) {
        // Iterative DFS via an explicit stack to avoid recursion depth
        // pathologies on deep scene graphs.
        // Stack holds (node_id, parent_world_matrix).
        let roots = self.roots.clone();
        let mut stack: Vec<(NodeId, Mat4)> = Vec::with_capacity(self.nodes.len());
        for r in roots {
            stack.push((r, Mat4::IDENTITY));
        }

        while let Some((id, parent_world)) = stack.pop() {
            if !id.is_valid() {
                continue;
            }
            let idx = id.0 as usize;
            if idx >= self.nodes.len() {
                continue;
            }
            let local_mat = self.nodes[idx].local.to_matrix();
            let world = parent_world.mul_mat(local_mat);
            self.nodes[idx].world_matrix = world;

            // Push children with this node's world matrix as their parent.
            let mut child = self.nodes[idx].first_child;
            while child.is_valid() {
                let cidx = child.0 as usize;
                stack.push((child, world));
                if cidx >= self.nodes.len() {
                    break;
                }
                child = self.nodes[cidx].next_sibling;
            }
        }
    }

    /// Iterate over all root nodes.
    pub fn roots(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.roots.iter().copied()
    }

    /// Iterate over all nodes in arena order.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &SceneNode)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (NodeId(i as u32), n))
    }

    /// Iterate over the children of a node (in linked-list order).
    pub fn children<'a>(&'a self, parent: NodeId) -> SceneChildIter<'a> {
        let next = self
            .nodes
            .get(parent.0 as usize)
            .map_or(NodeId::NONE, |n| n.first_child);
        SceneChildIter { graph: self, next }
    }

    /// True if `descendant` is a (transitive) descendant of `ancestor`.
    /// Used to detect cycles before reparenting.
    #[must_use]
    pub fn is_descendant_of(&self, descendant: NodeId, ancestor: NodeId) -> bool {
        let mut cursor = descendant;
        while cursor.is_valid() {
            if cursor == ancestor {
                return true;
            }
            match self.nodes.get(cursor.0 as usize) {
                Some(n) => cursor = n.parent,
                None => return false,
            }
        }
        false
    }
}

/// Iterator over a node's direct children. Walks the `next_sibling` chain.
pub struct SceneChildIter<'a> {
    graph: &'a SceneGraph,
    next: NodeId,
}

impl<'a> Iterator for SceneChildIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.next.is_valid() {
            return None;
        }
        let id = self.next;
        self.next = self
            .graph
            .nodes
            .get(id.0 as usize)
            .map_or(NodeId::NONE, |n| n.next_sibling);
        Some(id)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    #[test]
    fn node_id_default_invalid() {
        assert!(!NodeId::default().is_valid());
        assert_eq!(NodeId::default(), NodeId::NONE);
    }

    #[test]
    fn empty_scene_has_no_nodes_no_roots() {
        let g = SceneGraph::new();
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
        assert_eq!(g.roots().count(), 0);
    }

    #[test]
    fn add_root_appends_node_and_root() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        assert_eq!(g.len(), 1);
        assert_eq!(r, NodeId(0));
        assert_eq!(g.roots().count(), 1);
        assert!(g.get(r).is_some());
    }

    #[test]
    fn add_child_attaches_to_parent() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        let c = g.add_child(r, Transform::IDENTITY).unwrap();
        assert_eq!(g.get(c).unwrap().parent, r);
        assert_eq!(g.get(r).unwrap().first_child, c);
    }

    #[test]
    fn add_child_invalid_parent_errors() {
        let mut g = SceneGraph::new();
        let bad = NodeId(99);
        let res = g.add_child(bad, Transform::IDENTITY);
        assert_eq!(res, Err(SceneError::InvalidNode(bad)));
    }

    #[test]
    fn add_child_links_siblings() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        let c1 = g.add_child(r, Transform::IDENTITY).unwrap();
        let c2 = g.add_child(r, Transform::IDENTITY).unwrap();
        // Latest add_child becomes the new head ; c2.next == c1.
        assert_eq!(g.get(r).unwrap().first_child, c2);
        assert_eq!(g.get(c2).unwrap().next_sibling, c1);
        assert_eq!(g.get(c1).unwrap().next_sibling, NodeId::NONE);
    }

    #[test]
    fn children_iter_yields_all_children() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        let c1 = g.add_child(r, Transform::IDENTITY).unwrap();
        let c2 = g.add_child(r, Transform::IDENTITY).unwrap();
        let c3 = g.add_child(r, Transform::IDENTITY).unwrap();
        let kids: Vec<_> = g.children(r).collect();
        // Insertion-at-head order : c3, c2, c1.
        assert_eq!(kids, vec![c3, c2, c1]);
    }

    #[test]
    fn propagate_translation_composes() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::from_position(Vec3::new(10.0, 0.0, 0.0)));
        let c = g
            .add_child(r, Transform::from_position(Vec3::new(0.0, 5.0, 0.0)))
            .unwrap();
        g.propagate_transforms();
        // Child world matrix should translate origin to (10, 5, 0).
        let world = g.get(c).unwrap().world_matrix;
        assert_eq!(world.mul_point(Vec3::ZERO), Vec3::new(10.0, 5.0, 0.0));
    }

    #[test]
    fn propagate_three_levels() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::from_position(Vec3::new(1.0, 0.0, 0.0)));
        let m = g
            .add_child(r, Transform::from_position(Vec3::new(0.0, 2.0, 0.0)))
            .unwrap();
        let l = g
            .add_child(m, Transform::from_position(Vec3::new(0.0, 0.0, 3.0)))
            .unwrap();
        g.propagate_transforms();
        let world = g.get(l).unwrap().world_matrix;
        assert_eq!(world.mul_point(Vec3::ZERO), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn propagate_idempotent_when_no_changes() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::from_position(Vec3::new(5.0, 0.0, 0.0)));
        g.propagate_transforms();
        let w1 = g.get(r).unwrap().world_matrix;
        g.propagate_transforms();
        let w2 = g.get(r).unwrap().world_matrix;
        assert_eq!(w1, w2);
    }

    #[test]
    fn descendant_check() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        let c = g.add_child(r, Transform::IDENTITY).unwrap();
        let gc = g.add_child(c, Transform::IDENTITY).unwrap();
        assert!(g.is_descendant_of(c, r));
        assert!(g.is_descendant_of(gc, r));
        assert!(g.is_descendant_of(gc, c));
        assert!(g.is_descendant_of(r, r)); // reflexive
        assert!(!g.is_descendant_of(r, c)); // r not under c
    }

    #[test]
    fn iter_yields_all_in_arena_order() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        let c = g.add_child(r, Transform::IDENTITY).unwrap();
        let gc = g.add_child(c, Transform::IDENTITY).unwrap();
        let ids: Vec<_> = g.iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![r, c, gc]);
    }

    #[test]
    fn node_default_is_not_renderable() {
        let n = SceneNode::default();
        assert!(!n.is_renderable());
    }

    #[test]
    fn node_with_light_is_renderable() {
        let mut n = SceneNode::default();
        n.light = Some(Light::default());
        assert!(n.is_renderable());
    }

    #[test]
    fn node_visible_default_true() {
        assert!(SceneNode::default().visible);
    }

    #[test]
    fn multiple_roots_propagate_independently() {
        let mut g = SceneGraph::new();
        let r1 = g.add_root(Transform::from_position(Vec3::new(1.0, 0.0, 0.0)));
        let r2 = g.add_root(Transform::from_position(Vec3::new(0.0, 1.0, 0.0)));
        g.propagate_transforms();
        assert_eq!(
            g.get(r1).unwrap().world_matrix.mul_point(Vec3::ZERO),
            Vec3::new(1.0, 0.0, 0.0)
        );
        assert_eq!(
            g.get(r2).unwrap().world_matrix.mul_point(Vec3::ZERO),
            Vec3::new(0.0, 1.0, 0.0)
        );
    }
}
