//! § cssl-render::queue — RenderQueue + DrawCall + frustum culling
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per-camera draw list. Walks the scene graph, evaluates visibility +
//!   frustum culling against the camera's view-projection, and produces
//!   an ordered list of [`DrawCall`]s ready for backend submission.
//!
//! § PIPELINE
//!   1. Caller builds a [`SceneGraph`] + propagates transforms.
//!   2. Caller constructs an [`crate::projections::Camera`] with
//!      view-projection state.
//!   3. `RenderQueue::build_for_camera(scene, camera)` walks the scene,
//!      tests each renderable node's world AABB against the camera frustum,
//!      and emits `DrawCall`s for non-culled visible nodes.
//!   4. Backend reads the DrawCall list at submit time and translates each
//!      to backend-specific commands.
//!
//! § SORTING
//!   Stage-0 ships **front-to-back sorting** for opaque draws (depth-test
//!   early-Z benefits) + **back-to-front sorting** for translucent draws
//!   (alpha-blend correctness). Material-state sorting / pipeline-state
//!   bucketing for state-change minimization is a future slice.

use crate::asset::AssetHandle;
use crate::material::{AlphaMode, Material};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::scene::{NodeId, SceneGraph};

// ════════════════════════════════════════════════════════════════════════════
// § DrawCall — single backend-bound draw item
// ════════════════════════════════════════════════════════════════════════════

/// A single draw item : the mesh + material + world transform that the
/// backend will translate to a draw call. Backend-agnostic — it carries
/// asset handles, not Vulkan / D3D12 / Metal handles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawCall {
    /// World-space transform of this draw item. Backend uploads as a
    /// uniform / push-constant for the vertex shader.
    pub world_matrix: Mat4,
    /// Mesh data (or rather, its asset reference + draw metadata).
    pub mesh: Mesh,
    /// Material applied to this mesh.
    pub material: Material,
    /// Asset-database mesh handle, for backends that resolve mesh data
    /// via the asset crate rather than from the inline `Mesh` struct.
    pub mesh_asset: AssetHandle<Mesh>,
    /// The originating scene node — useful for debugging + per-draw
    /// telemetry. Not consumed by the backend.
    pub source_node: NodeId,
    /// Sort-key : depth in view space (positive = farther). Used to
    /// front-to-back / back-to-front sort within a pass.
    pub view_depth: f32,
}

// ════════════════════════════════════════════════════════════════════════════
// § RenderQueue — per-camera draw collection
// ════════════════════════════════════════════════════════════════════════════

/// Per-camera draw queue : separate buckets for opaque and translucent
/// draws, with sort order applied appropriately.
#[derive(Debug, Default, Clone)]
pub struct RenderQueue {
    /// Opaque draw calls. Sorted front-to-back (small `view_depth` first)
    /// for early-Z efficiency.
    pub opaque: Vec<DrawCall>,
    /// Translucent (alpha-blended) draw calls. Sorted back-to-front
    /// (large `view_depth` first) for alpha-blend correctness.
    pub translucent: Vec<DrawCall>,
    /// Diagnostic counters from the build pass.
    pub stats: QueueStats,
}

/// Diagnostic counters from a `build_for_camera` pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct QueueStats {
    /// Total renderable nodes considered.
    pub nodes_considered: u32,
    /// Nodes culled by the frustum test.
    pub nodes_frustum_culled: u32,
    /// Nodes culled by the visibility flag.
    pub nodes_visibility_culled: u32,
    /// Nodes that produced a draw call.
    pub nodes_drawn: u32,
}

impl RenderQueue {
    /// Construct an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total draw count across opaque + translucent buckets.
    #[must_use]
    pub fn total_draws(&self) -> usize {
        self.opaque.len() + self.translucent.len()
    }

    /// Reset all buckets + stats. Useful for queue reuse across frames.
    pub fn clear(&mut self) {
        self.opaque.clear();
        self.translucent.clear();
        self.stats = QueueStats::default();
    }

    /// Build the queue for a given camera by walking the scene graph,
    /// culling, sorting. Caller must have called
    /// `SceneGraph::propagate_transforms` before this.
    pub fn build_for_camera(&mut self, scene: &SceneGraph, camera: &crate::projections::Camera) {
        self.clear();
        let view_pos = Vec3::from_proj(camera.position);
        // Note : `frustum_cull` in cssl-substrate-projections takes a Camera
        // (not a Frustum) — it builds the frustum internally from the
        // camera's view-projection. We use that direct path rather than
        // pre-constructing a Frustum to keep the bridge thin.
        let vp = camera.view_projection_matrix();
        let _ = vp;

        // Walk all nodes, applying the visibility flag + frustum test.
        // Visibility is inherited : a hidden ancestor hides its subtree.
        // Stage-0 implements this via a per-node ancestor-visibility
        // recompute (linear pass).
        for (id, node) in scene.iter() {
            self.stats.nodes_considered += 1;

            // Inherited visibility check : walk parent chain.
            if !is_visible_inherited(scene, id, node) {
                self.stats.nodes_visibility_culled += 1;
                continue;
            }

            if !node.is_renderable() {
                continue;
            }

            // Frustum cull against world AABB. The substrate-projections
            // `frustum_cull(aabb, &Camera)` builds the frustum internally
            // from the camera's view-projection so callers don't have to.
            let world_aabb = node.world_aabb();
            if world_aabb.is_valid() {
                let proj_aabb = crate::projections::Aabb::new(
                    world_aabb.min.to_proj(),
                    world_aabb.max.to_proj(),
                );
                if crate::projections::frustum_cull(proj_aabb, camera) {
                    self.stats.nodes_frustum_culled += 1;
                    continue;
                }
            }

            // Compute view-space depth (z-distance from camera to node center).
            let center = if world_aabb.is_valid() {
                world_aabb.center()
            } else {
                node.world_matrix.mul_point(Vec3::ZERO)
            };
            let view_depth = (center - view_pos).length();

            let dc = DrawCall {
                world_matrix: node.world_matrix,
                mesh: node.mesh,
                material: node.material,
                mesh_asset: node.mesh_asset,
                source_node: id,
                view_depth,
            };

            if matches!(node.material.alpha_mode, AlphaMode::Blend) {
                self.translucent.push(dc);
            } else {
                self.opaque.push(dc);
            }
            self.stats.nodes_drawn += 1;
        }

        // Sort opaque front-to-back, translucent back-to-front.
        self.opaque.sort_by(|a, b| {
            a.view_depth
                .partial_cmp(&b.view_depth)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        self.translucent.sort_by(|a, b| {
            b.view_depth
                .partial_cmp(&a.view_depth)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
    }
}

/// True if the node is visible considering inherited visibility — every
/// ancestor must also have `visible == true`.
fn is_visible_inherited(scene: &SceneGraph, id: NodeId, node: &crate::scene::SceneNode) -> bool {
    if !node.visible {
        return false;
    }
    let mut cursor = node.parent;
    while cursor.is_valid() {
        match scene.get(cursor) {
            Some(p) => {
                if !p.visible {
                    return false;
                }
                cursor = p.parent;
            }
            None => break,
        }
    }
    let _ = id;
    true
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetHandle;
    use crate::math::{Aabb, Transform, Vec3};
    use crate::mesh::{Mesh, VertexAttributeLayout};
    use crate::scene::{SceneGraph, SceneNode};

    fn dummy_drawable_node() -> SceneNode {
        let mut n = SceneNode::default();
        n.mesh = Mesh {
            layout: VertexAttributeLayout::standard_pbr(),
            vertex_buffer: AssetHandle::new(0),
            vertex_count: 3,
            local_aabb: Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5)),
            ..Mesh::EMPTY
        };
        n
    }

    #[test]
    fn empty_queue_zero_draws() {
        let q = RenderQueue::new();
        assert_eq!(q.total_draws(), 0);
        assert_eq!(q.opaque.len(), 0);
        assert_eq!(q.translucent.len(), 0);
    }

    #[test]
    fn queue_clear_resets_all() {
        let mut q = RenderQueue::new();
        q.opaque.push(DrawCall {
            world_matrix: Mat4::IDENTITY,
            mesh: Mesh::EMPTY,
            material: Material::DEFAULT_PBR,
            mesh_asset: AssetHandle::INVALID,
            source_node: NodeId(0),
            view_depth: 0.0,
        });
        q.stats.nodes_considered = 5;
        q.clear();
        assert_eq!(q.opaque.len(), 0);
        assert_eq!(q.stats.nodes_considered, 0);
    }

    #[test]
    fn build_skips_invisible_node() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        // Make it drawable but invisible.
        if let Some(n) = g.get_mut(r) {
            *n = dummy_drawable_node();
            n.visible = false;
        }
        g.propagate_transforms();

        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);

        assert_eq!(q.total_draws(), 0);
        assert_eq!(q.stats.nodes_visibility_culled, 1);
    }

    #[test]
    fn build_skips_non_renderable_node() {
        let mut g = SceneGraph::new();
        let _r = g.add_root(Transform::IDENTITY);
        g.propagate_transforms();
        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);
        // Node was visible but not renderable (no mesh / light) -> 0 draws.
        assert_eq!(q.total_draws(), 0);
        assert_eq!(q.stats.nodes_drawn, 0);
    }

    #[test]
    fn build_includes_renderable_in_view() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::from_position(Vec3::new(0.0, 0.0, -5.0)));
        if let Some(n) = g.get_mut(r) {
            let dn = dummy_drawable_node();
            n.mesh = dn.mesh;
        }
        g.propagate_transforms();

        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);

        // Camera default is at origin looking down -Z ; node at z = -5 is
        // in front of the camera.
        assert!(q.total_draws() >= 1);
    }

    #[test]
    fn opaque_translucent_buckets_split_correctly() {
        let mut g = SceneGraph::new();
        let r1 = g.add_root(Transform::from_position(Vec3::new(0.0, 0.0, -3.0)));
        let r2 = g.add_root(Transform::from_position(Vec3::new(0.5, 0.0, -3.0)));
        if let Some(n) = g.get_mut(r1) {
            let dn = dummy_drawable_node();
            n.mesh = dn.mesh;
            n.material.alpha_mode = AlphaMode::Opaque;
        }
        if let Some(n) = g.get_mut(r2) {
            let dn = dummy_drawable_node();
            n.mesh = dn.mesh;
            n.material.alpha_mode = AlphaMode::Blend;
        }
        g.propagate_transforms();

        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);

        assert_eq!(q.opaque.len(), 1);
        assert_eq!(q.translucent.len(), 1);
    }

    #[test]
    fn opaque_sorted_front_to_back() {
        let mut g = SceneGraph::new();
        // Two nodes at different distances from camera.
        let _far = g.add_root(Transform::from_position(Vec3::new(0.0, 0.0, -10.0)));
        let _near = g.add_root(Transform::from_position(Vec3::new(0.0, 0.0, -3.0)));
        for id in [NodeId(0), NodeId(1)] {
            if let Some(n) = g.get_mut(id) {
                let dn = dummy_drawable_node();
                n.mesh = dn.mesh;
            }
        }
        g.propagate_transforms();
        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);

        if q.opaque.len() == 2 {
            // Front-to-back : near should come first.
            assert!(q.opaque[0].view_depth <= q.opaque[1].view_depth);
        }
    }

    #[test]
    fn translucent_sorted_back_to_front() {
        let mut g = SceneGraph::new();
        let _near = g.add_root(Transform::from_position(Vec3::new(0.0, 0.0, -3.0)));
        let _far = g.add_root(Transform::from_position(Vec3::new(0.0, 0.0, -10.0)));
        for id in [NodeId(0), NodeId(1)] {
            if let Some(n) = g.get_mut(id) {
                let dn = dummy_drawable_node();
                n.mesh = dn.mesh;
                n.material.alpha_mode = AlphaMode::Blend;
            }
        }
        g.propagate_transforms();
        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);

        if q.translucent.len() == 2 {
            // Back-to-front : far should come first (larger view_depth).
            assert!(q.translucent[0].view_depth >= q.translucent[1].view_depth);
        }
    }

    #[test]
    fn parent_invisible_hides_children() {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::IDENTITY);
        let c = g.add_child(r, Transform::IDENTITY).unwrap();
        if let Some(n) = g.get_mut(r) {
            n.visible = false;
        }
        if let Some(n) = g.get_mut(c) {
            let dn = dummy_drawable_node();
            n.mesh = dn.mesh;
            // c is visible but parent is not.
        }
        g.propagate_transforms();
        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);
        // Parent is invisible -> child also hidden in the queue.
        assert_eq!(q.total_draws(), 0);
    }

    #[test]
    fn stats_count_consistent() {
        let mut g = SceneGraph::new();
        let _ = g.add_root(Transform::IDENTITY);
        let _ = g.add_root(Transform::IDENTITY);
        let _ = g.add_root(Transform::IDENTITY);
        g.propagate_transforms();
        let cam = crate::projections::Camera::default();
        let mut q = RenderQueue::new();
        q.build_for_camera(&g, &cam);
        assert_eq!(q.stats.nodes_considered, 3);
    }
}
