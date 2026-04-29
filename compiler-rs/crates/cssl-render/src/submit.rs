//! § cssl-render::submit — per-frame submission entry-point
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The single per-frame entry-point that drives the renderer end-to-end :
//!   `submit(scene, camera, graph, backend) -> Result<FrameStats, RenderError>`.
//!   Walks the render-graph in topo order, builds a `RenderQueue` for the
//!   camera, and dispatches passes through the [`RenderBackend`] trait.
//!
//! § {Render} EFFECT-ROW
//!   Substrate canonical : every render-side function carries the `{Render}`
//!   effect row. At stage-0 (Rust-host) we encode this as a ZST marker
//!   [`RenderEffect`] threaded through the [`RenderContext`] struct, so
//!   anywhere a render fn lives in the call graph the compiler can verify
//!   the effect is present. When CSSLv3 source-level effects emit through
//!   to LIR/MIR/HIR, this scaffolding lifts cleanly to spec-canonical
//!   `{Render}` effect-row.

use crate::backend::{FrameStats, PassContext, RenderBackend, RenderError};
use crate::graph::RenderGraph;
use crate::math::{Mat4, Vec3};
use crate::projections::Camera;
use crate::queue::{DrawCall, RenderQueue};
use crate::scene::SceneGraph;

// ════════════════════════════════════════════════════════════════════════════
// § RenderEffect — ZST marker for the {Render} effect row
// ════════════════════════════════════════════════════════════════════════════

/// Zero-sized marker representing the substrate's `{Render}` effect row at
/// stage-0. Functions taking `&RenderEffect` (or returning `RenderContext`
/// which contains it) participate in the render-effect surface ; functions
/// without a [`RenderEffect`] reference cannot reach the backend.
///
/// § FUTURE
///   When CSSLv3 source-level effects emit, this lifts to a real effect-row
///   token and the type-system gains the standard "function with effect E
///   can only be called from a context that has E" discipline. Stage-0
///   approximates this by making `RenderEffect` non-constructible outside
///   `submit`'s call path — the only mint-point lives in the private
///   `begin_render_effect` fn invoked by [`RenderContext::new`].
#[derive(Debug)]
pub struct RenderEffect {
    /// Private field : prevents external construction. Renderer entry-points
    /// are the only places that mint a `RenderEffect`.
    _phantom: core::marker::PhantomData<()>,
}

/// The single mint-point for [`RenderEffect`]. Called by the renderer's
/// public submit entry-point. External callers cannot mint `RenderEffect`
/// directly — they must reach it via a render-side fn that already has
/// access to one.
const fn begin_render_effect() -> RenderEffect {
    RenderEffect {
        _phantom: core::marker::PhantomData,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § RenderContext — per-frame state
// ════════════════════════════════════════════════════════════════════════════

/// Per-frame render context. Carries the [`RenderEffect`] marker + queue
/// + frame-counter for diagnostics.
#[derive(Debug)]
pub struct RenderContext {
    /// {Render} effect-row marker. Required by every render-side fn.
    pub effect: RenderEffect,
    /// Per-camera draw queue. Reused across frames via `clear()`.
    pub queue: RenderQueue,
    /// Monotonic frame counter — increments on every successful submit.
    pub frame_index: u64,
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderContext {
    /// Construct a fresh render context. Mints the [`RenderEffect`] —
    /// the only path to a real `RenderContext`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            effect: begin_render_effect(),
            queue: RenderQueue::new(),
            frame_index: 0,
        }
    }

    /// Number of frames submitted so far.
    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// Borrow the {Render} effect marker. Used by render-fns that need to
    /// witness the effect for type-checking.
    #[must_use]
    pub const fn effect(&self) -> &RenderEffect {
        &self.effect
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § submit() — the per-frame entry-point
// ════════════════════════════════════════════════════════════════════════════

/// Submit one frame of rendering : walk the render-graph in topo order,
/// build the per-camera draw queue, and dispatch passes through the backend.
///
/// Returns the accumulated [`FrameStats`] from the backend on success.
///
/// § PRECONDITIONS
///   - `scene.propagate_transforms()` must have been called this frame.
///   - `graph.topo_sort()` must have been called (or `topo_order` is empty
///     and the call sets it).
///   - `camera.validate()` must succeed — invalid intrinsics produce
///     `RenderError::InvalidCamera`.
///   - `backend.swapchain_*` must be non-zero.
///
/// § ALGORITHM
///   1. Validate camera. Return early on bad intrinsics.
///   2. Re-sort graph if topo order is stale.
///   3. Build `ctx.queue` for the camera (frustum culling + sort).
///   4. `backend.begin_frame()`.
///   5. For each pass in topo order :
///      a. `backend.begin_pass(ctx)`.
///      b. For each draw call in the queue (filtered by pass kind) :
///         `backend.draw(dc)`.
///      c. `backend.end_pass(kind)`.
///   6. `backend.end_frame()`.
///   7. `backend.present()`.
///   8. Return `backend.frame_stats()`.
pub fn submit<B: RenderBackend>(
    ctx: &mut RenderContext,
    scene: &SceneGraph,
    camera: &Camera,
    graph: &mut RenderGraph,
    backend: &mut B,
) -> Result<FrameStats, RenderError> {
    // 1. Validate camera.
    camera
        .validate()
        .map_err(|e| RenderError::InvalidCamera(format!("{e}")))?;

    // 2. Topo-sort if stale.
    if graph.topo_order.is_empty() && !graph.is_empty() {
        graph.topo_sort()?;
    }

    // 3. Build queue.
    ctx.queue.build_for_camera(scene, camera);

    // 4. begin_frame.
    backend.begin_frame()?;

    // Compute per-pass context once : view + projection are constant
    // across passes for a single camera (lighting passes might want their
    // own projection in the future, but at stage-0 we share).
    let view_matrix = Mat4::from_proj(camera.view_matrix());
    let projection_matrix = Mat4::from_proj(camera.view_projection_matrix());
    let camera_position = Vec3::from_proj(camera.position);

    // 5. For each pass in topo order...
    let order: Vec<crate::graph::PassId> = graph.topo_order.clone();
    for pass_id in order {
        let pass = match graph.get(pass_id) {
            Some(p) => *p,
            None => continue,
        };

        let pass_ctx = PassContext {
            kind: pass.kind,
            view_matrix,
            projection_matrix,
            camera_position,
            color_attachments: [crate::graph::AttachmentId::NONE; 4],
            color_count: 0,
            depth_attachment: crate::graph::AttachmentId::NONE,
        };
        let _ = pass_ctx.color_attachments;
        let _ = pass_ctx.color_count;
        let _ = pass_ctx.depth_attachment;

        backend.begin_pass(&pass_ctx)?;

        // 5b. Dispatch draws appropriate to the pass kind.
        dispatch_draws_for_pass(pass.kind, &ctx.queue, backend)?;

        backend.end_pass(pass.kind)?;
    }

    // 6. end_frame.
    backend.end_frame()?;

    // 7. present.
    backend.present()?;

    ctx.frame_index += 1;

    // 8. Return stats.
    Ok(backend.frame_stats())
}

/// Dispatch the appropriate draw subset to the backend for a given pass.
///
/// § PASS-TO-DRAWS MAPPING
///   - `GeometryPass` + `LightingPass` + `ShadowPass` → opaque queue
///   - `TranslucentPass` → translucent queue
///   - `TonemapPass` + `UiPass` + `Custom` → no scene-draws (the backend
///     emits its own fullscreen-triangle / UI primitives)
fn dispatch_draws_for_pass<B: RenderBackend>(
    kind: crate::graph::PassKind,
    queue: &RenderQueue,
    backend: &mut B,
) -> Result<(), RenderError> {
    use crate::graph::PassKind;
    let calls: &[DrawCall] = match kind {
        PassKind::ShadowPass | PassKind::GeometryPass | PassKind::LightingPass => &queue.opaque,
        PassKind::TranslucentPass => &queue.translucent,
        PassKind::TonemapPass | PassKind::UiPass | PassKind::Custom(_) => &[],
    };
    for dc in calls {
        backend.draw(dc)?;
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetHandle;
    use crate::backend::NullBackend;
    use crate::graph::{PassKind, RenderGraph, RenderPass};
    use crate::math::{Aabb, Transform, Vec3 as RVec3};
    use crate::mesh::{Mesh, VertexAttributeLayout};
    use crate::scene::SceneGraph;

    fn mk_drawable_scene() -> SceneGraph {
        let mut g = SceneGraph::new();
        let r = g.add_root(Transform::from_position(RVec3::new(0.0, 0.0, -5.0)));
        if let Some(n) = g.get_mut(r) {
            n.mesh = Mesh {
                layout: VertexAttributeLayout::standard_pbr(),
                vertex_buffer: AssetHandle::new(0),
                vertex_count: 3,
                local_aabb: Aabb::new(RVec3::splat(-0.5), RVec3::splat(0.5)),
                ..Mesh::EMPTY
            };
        }
        g.propagate_transforms();
        g
    }

    fn mk_minimal_graph() -> RenderGraph {
        let mut g = RenderGraph::new();
        g.add_pass(RenderPass::new(PassKind::GeometryPass));
        g
    }

    #[test]
    fn render_effect_constructible_via_context() {
        let ctx = RenderContext::new();
        // The marker is reachable through the context — that's the only path.
        let _ = ctx.effect();
    }

    #[test]
    fn render_context_starts_at_frame_zero() {
        let ctx = RenderContext::new();
        assert_eq!(ctx.frame_index(), 0);
    }

    #[test]
    fn submit_increments_frame_counter() {
        let mut ctx = RenderContext::new();
        let scene = mk_drawable_scene();
        let camera = Camera::default();
        let mut graph = mk_minimal_graph();
        let mut backend = NullBackend::new();
        backend.resize_swapchain(800, 600).unwrap();
        submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        assert_eq!(ctx.frame_index(), 1);
        submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        assert_eq!(ctx.frame_index(), 2);
    }

    #[test]
    fn submit_rejects_invalid_camera() {
        let mut ctx = RenderContext::new();
        let scene = SceneGraph::new();
        let mut camera = Camera::default();
        camera.fov_y_rad = -1.0; // invalid
        let mut graph = mk_minimal_graph();
        let mut backend = NullBackend::new();
        backend.resize_swapchain(100, 100).unwrap();
        let res = submit(&mut ctx, &scene, &camera, &mut graph, &mut backend);
        assert!(matches!(res, Err(RenderError::InvalidCamera(_))));
    }

    #[test]
    fn submit_topo_sorts_lazy_graph() {
        let mut ctx = RenderContext::new();
        let scene = SceneGraph::new();
        let camera = Camera::default();
        let mut graph = RenderGraph::default_forward_pipeline();
        // topo_order starts empty.
        assert!(graph.topo_order.is_empty());
        let mut backend = NullBackend::new();
        backend.resize_swapchain(100, 100).unwrap();
        submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        // Topo-sort happened during submit.
        assert_eq!(graph.topo_order.len(), 6);
    }

    #[test]
    fn submit_executes_all_passes() {
        let mut ctx = RenderContext::new();
        let scene = SceneGraph::new();
        let camera = Camera::default();
        let mut graph = RenderGraph::default_forward_pipeline();
        let mut backend = NullBackend::new();
        backend.resize_swapchain(100, 100).unwrap();
        let stats = submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        // 6 passes in default forward pipeline.
        assert_eq!(stats.passes_executed, 6);
    }

    #[test]
    fn submit_calls_present() {
        let mut ctx = RenderContext::new();
        let scene = SceneGraph::new();
        let camera = Camera::default();
        let mut graph = mk_minimal_graph();
        let mut backend = NullBackend::new();
        backend.resize_swapchain(100, 100).unwrap();
        submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        assert_eq!(backend.count_presents(), 1);
    }

    #[test]
    fn submit_drives_geometry_pass_draws_for_opaque() {
        let mut ctx = RenderContext::new();
        let scene = mk_drawable_scene();
        let camera = Camera::default();
        let mut graph = mk_minimal_graph();
        let mut backend = NullBackend::new();
        backend.resize_swapchain(800, 600).unwrap();
        let stats = submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        assert_eq!(stats.passes_executed, 1);
        // A drawable opaque node should produce at least one draw call.
        assert!(stats.draw_calls >= 1);
    }

    #[test]
    fn submit_zero_swapchain_present_fails() {
        let mut ctx = RenderContext::new();
        let scene = SceneGraph::new();
        let camera = Camera::default();
        let mut graph = mk_minimal_graph();
        let mut backend = NullBackend::new();
        backend.swapchain_width = 0;
        backend.swapchain_height = 0;
        let res = submit(&mut ctx, &scene, &camera, &mut graph, &mut backend);
        assert!(matches!(res, Err(RenderError::SwapchainNotReady { .. })));
    }

    #[test]
    fn submit_empty_graph_runs_clean() {
        let mut ctx = RenderContext::new();
        let scene = SceneGraph::new();
        let camera = Camera::default();
        let mut graph = RenderGraph::new();
        let mut backend = NullBackend::new();
        backend.resize_swapchain(100, 100).unwrap();
        let stats = submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        assert_eq!(stats.passes_executed, 0);
        assert_eq!(stats.draw_calls, 0);
    }

    #[test]
    fn dispatch_for_geometry_uses_opaque() {
        let mut ctx = RenderContext::new();
        let scene = mk_drawable_scene();
        let camera = Camera::default();
        let mut graph = mk_minimal_graph();
        let mut backend = NullBackend::new();
        backend.resize_swapchain(100, 100).unwrap();
        submit(&mut ctx, &scene, &camera, &mut graph, &mut backend).unwrap();
        // Records show all draws happened under GeometryPass.
        let geom_draws = backend
            .commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    crate::backend::BackendCommand::Draw {
                        pass_kind: PassKind::GeometryPass,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(geom_draws, backend.count_draws());
    }
}
