//! § cssl-render::backend — RenderBackend trait + null backend
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Backend abstraction layer. The renderer is generic over backends ;
//!   each per-host crate (`cssl-host-vulkan`, `cssl-host-d3d12`,
//!   `cssl-host-metal`, `cssl-host-webgpu`) implements [`RenderBackend`]
//!   to translate the renderer's backend-agnostic commands to its native
//!   API.
//!
//! § DESIGN PRINCIPLE — backend-agnostic public surface
//!   No Vulkan / D3D12 / Metal types appear in cssl-render's public API.
//!   Resources are referenced via opaque [`crate::asset::AssetHandle`]s ;
//!   commands flow through [`BackendCommand`] enum variants. The backend
//!   turns `BindPipeline { ... }` into `vkCmdBindPipeline` /
//!   `IASetVertexBuffers` / `setPipelineState` / `setPipeline` depending
//!   on its native API.
//!
//! § CALL PROTOCOL — submit() lifecycle
//!   1. Renderer caller invokes [`crate::submit::submit`] with scene + camera.
//!   2. Submit walks the scene producing a `RenderQueue` + executes the
//!      graph topo-order to drive the backend.
//!   3. For each pass, the backend is invoked via :
//!      - [`RenderBackend::begin_pass`] — bind attachments, clear, etc.
//!      - For each draw call: [`RenderBackend::draw`].
//!      - [`RenderBackend::end_pass`].
//!   4. Submit calls [`RenderBackend::present`] to deliver the swapchain image.
//!
//! § NullBackend
//!   A test-only backend that records commands without doing real GPU
//!   work. Used by the renderer's unit tests + integration tests to
//!   verify command emission without standing up a real Vulkan context.

use crate::asset::TextureHandle;
use crate::graph::{AttachmentId, PassKind};
use crate::math::Mat4;
use crate::queue::DrawCall;

// ════════════════════════════════════════════════════════════════════════════
// § RenderError — error taxonomy
// ════════════════════════════════════════════════════════════════════════════

/// Backend / submission error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RenderError {
    /// Backend reported a generic device-lost / out-of-memory failure.
    #[error("render: backend device error : {0}")]
    Backend(String),
    /// Render-graph topo-sort detected a cycle.
    #[error("render: graph error : {0}")]
    Graph(#[from] crate::graph::GraphError),
    /// Scene-graph operation failed.
    #[error("render: scene error : {0}")]
    Scene(#[from] crate::scene::SceneError),
    /// Attempted to submit with an unbound or zero-size swapchain.
    #[error("render: swapchain not bound or zero-size ({width}x{height})")]
    SwapchainNotReady { width: u32, height: u32 },
    /// Camera intrinsics failed validation (FOV / aspect / near / far).
    #[error("render: invalid camera : {0}")]
    InvalidCamera(String),
}

// ════════════════════════════════════════════════════════════════════════════
// § PassContext — passed to backend per-pass calls
// ════════════════════════════════════════════════════════════════════════════

/// Per-pass invocation context : pass kind + render-target attachments +
/// view-projection matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassContext {
    /// Discriminator from the render-graph.
    pub kind: PassKind,
    /// View matrix (world → view).
    pub view_matrix: Mat4,
    /// Projection matrix (view → clip, reverse-Z RH).
    pub projection_matrix: Mat4,
    /// World-space camera position (useful for view-dependent effects).
    pub camera_position: crate::math::Vec3,
    /// Render-target color attachment(s) ; up to 4 MRT slots.
    pub color_attachments: [AttachmentId; 4],
    /// Number of valid color attachments.
    pub color_count: u8,
    /// Optional depth attachment.
    pub depth_attachment: AttachmentId,
}

impl Default for PassContext {
    fn default() -> Self {
        Self {
            kind: PassKind::GeometryPass,
            view_matrix: Mat4::IDENTITY,
            projection_matrix: Mat4::IDENTITY,
            camera_position: crate::math::Vec3::ZERO,
            color_attachments: [AttachmentId::NONE; 4],
            color_count: 0,
            depth_attachment: AttachmentId::NONE,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § FrameStats — submit return value
// ════════════════════════════════════════════════════════════════════════════

/// Diagnostic counters from a single `submit()` call. Backend implementations
/// fill these in via the [`RenderBackend::frame_stats`] accessor (or update
/// them directly during pass execution).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    /// Number of passes the backend processed.
    pub passes_executed: u32,
    /// Total draw calls submitted.
    pub draw_calls: u32,
    /// Total primitives (triangles + lines + points) drawn.
    pub primitives: u64,
    /// Total vertices processed by the vertex shader.
    pub vertices: u64,
    /// Estimated GPU time in microseconds, if the backend can measure.
    /// `0` for backends without timing-query support (e.g. NullBackend).
    pub gpu_time_us: u32,
}

// ════════════════════════════════════════════════════════════════════════════
// § BackendCommand — recorded command for null / debug backends
// ════════════════════════════════════════════════════════════════════════════

/// Backend-recordable command. The NullBackend stores these for inspection
/// in tests ; real backends (cssl-host-vulkan etc) translate them to native
/// API calls without storing them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackendCommand {
    /// Begin a pass with the given context.
    BeginPass { kind: PassKind },
    /// Issue a draw call.
    Draw {
        /// Number of primitives this draw produces.
        primitives: u32,
        /// Pass kind under which the draw happened.
        pass_kind: PassKind,
    },
    /// End the current pass.
    EndPass { kind: PassKind },
    /// Present the swapchain image.
    Present,
}

// ════════════════════════════════════════════════════════════════════════════
// § RenderBackend trait
// ════════════════════════════════════════════════════════════════════════════

/// Backend-abstraction trait. Each per-host adapter implements this to
/// translate the renderer's backend-agnostic commands to its native API.
///
/// § THREADING
///   Calls in the lifecycle (`begin_pass` → `draw`* → `end_pass` → ... →
///   `present`) are sequential per-frame. Backends may internally
///   parallelize using their command-buffer / encoder model ; the
///   renderer's submit pipeline does not multi-thread the trait calls.
///
/// § STATE
///   The backend retains its own state between calls (active pipeline,
///   bound resources, command-encoder handle). The renderer treats the
///   backend as a stateful object — no attempts to inspect or reset
///   internal state from outside.
pub trait RenderBackend {
    /// Backend identification string for diagnostics. Examples : "vulkan-1.4
    /// / Intel Arc A770", "d3d12 / NVIDIA RTX 4090".
    fn description(&self) -> &str;

    /// Resize the swapchain / output target to `(width, height)`. Called by
    /// the host when the window is resized. Returns the actual size the
    /// backend committed to (may clamp to its own limits).
    fn resize_swapchain(&mut self, width: u32, height: u32) -> Result<(u32, u32), RenderError>;

    /// Begin a frame's worth of passes. Called once per `submit()` before
    /// any `begin_pass` call.
    fn begin_frame(&mut self) -> Result<(), RenderError>;

    /// Begin a render pass with the given context.
    fn begin_pass(&mut self, ctx: &PassContext) -> Result<(), RenderError>;

    /// Issue a draw call within the current pass.
    fn draw(&mut self, dc: &DrawCall) -> Result<(), RenderError>;

    /// End the current pass.
    fn end_pass(&mut self, kind: PassKind) -> Result<(), RenderError>;

    /// End the frame — finalize command buffers, signal fences, etc.
    fn end_frame(&mut self) -> Result<(), RenderError>;

    /// Present the rendered swapchain image to the display.
    fn present(&mut self) -> Result<(), RenderError>;

    /// Borrow the backend's accumulated frame stats. Called by the renderer
    /// after `present` to report counters back to the caller.
    fn frame_stats(&self) -> FrameStats;

    /// Read back the contents of the given attachment as raw RGBA8 bytes.
    /// Used by tests + screenshot tools. Backends may return
    /// [`RenderError::Backend`] if readback isn't supported.
    fn readback_attachment(
        &mut self,
        attachment: TextureHandle,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, RenderError>;
}

// ════════════════════════════════════════════════════════════════════════════
// § NullBackend — record-only backend for tests
// ════════════════════════════════════════════════════════════════════════════

/// Record-only backend. Captures `BackendCommand`s for inspection in
/// tests + drives the renderer's smoke paths without standing up a real
/// GPU context.
#[derive(Debug, Default)]
pub struct NullBackend {
    /// Recorded commands in invocation order.
    pub commands: Vec<BackendCommand>,
    /// Current pass kind (for asserting begin/end pairing).
    pub current_pass: Option<PassKind>,
    /// Configured swapchain dimensions.
    pub swapchain_width: u32,
    pub swapchain_height: u32,
    /// Accumulated stats.
    pub stats: FrameStats,
}

impl NullBackend {
    /// Construct a NullBackend with a default 1x1 swapchain. Tests
    /// typically call `resize_swapchain` to set a meaningful size.
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            current_pass: None,
            swapchain_width: 1,
            swapchain_height: 1,
            stats: FrameStats::default(),
        }
    }

    /// Reset all recorded state. Useful between test cases reusing the
    /// same backend instance.
    pub fn reset(&mut self) {
        self.commands.clear();
        self.current_pass = None;
        self.stats = FrameStats::default();
    }

    /// Count commands of a specific BeginPass kind.
    #[must_use]
    pub fn count_begin_pass(&self, kind: PassKind) -> usize {
        self.commands
            .iter()
            .filter(|c| matches!(c, BackendCommand::BeginPass { kind: k } if *k == kind))
            .count()
    }

    /// Total Draw commands recorded.
    #[must_use]
    pub fn count_draws(&self) -> usize {
        self.commands
            .iter()
            .filter(|c| matches!(c, BackendCommand::Draw { .. }))
            .count()
    }

    /// Total Present commands recorded.
    #[must_use]
    pub fn count_presents(&self) -> usize {
        self.commands
            .iter()
            .filter(|c| matches!(c, BackendCommand::Present))
            .count()
    }
}

impl RenderBackend for NullBackend {
    fn description(&self) -> &str {
        "null-backend (record-only ; no GPU)"
    }

    fn resize_swapchain(&mut self, width: u32, height: u32) -> Result<(u32, u32), RenderError> {
        self.swapchain_width = width;
        self.swapchain_height = height;
        Ok((width, height))
    }

    fn begin_frame(&mut self) -> Result<(), RenderError> {
        self.stats = FrameStats::default();
        Ok(())
    }

    fn begin_pass(&mut self, ctx: &PassContext) -> Result<(), RenderError> {
        if self.current_pass.is_some() {
            return Err(RenderError::Backend(
                "begin_pass called while another pass is active".into(),
            ));
        }
        self.current_pass = Some(ctx.kind);
        self.commands
            .push(BackendCommand::BeginPass { kind: ctx.kind });
        self.stats.passes_executed += 1;
        Ok(())
    }

    fn draw(&mut self, dc: &DrawCall) -> Result<(), RenderError> {
        let pass_kind = self
            .current_pass
            .ok_or_else(|| RenderError::Backend("draw called outside a pass".into()))?;
        let prims = dc.mesh.primitive_count();
        self.commands.push(BackendCommand::Draw {
            primitives: prims,
            pass_kind,
        });
        self.stats.draw_calls += 1;
        self.stats.primitives += u64::from(prims);
        self.stats.vertices += u64::from(dc.mesh.vertex_count);
        Ok(())
    }

    fn end_pass(&mut self, kind: PassKind) -> Result<(), RenderError> {
        match self.current_pass {
            Some(active) if active == kind => {
                self.commands.push(BackendCommand::EndPass { kind });
                self.current_pass = None;
                Ok(())
            }
            _ => Err(RenderError::Backend(format!(
                "end_pass kind mismatch : active={:?}, end={:?}",
                self.current_pass, kind
            ))),
        }
    }

    fn end_frame(&mut self) -> Result<(), RenderError> {
        if self.current_pass.is_some() {
            return Err(RenderError::Backend(
                "end_frame called while a pass is still active".into(),
            ));
        }
        Ok(())
    }

    fn present(&mut self) -> Result<(), RenderError> {
        if self.swapchain_width == 0 || self.swapchain_height == 0 {
            return Err(RenderError::SwapchainNotReady {
                width: self.swapchain_width,
                height: self.swapchain_height,
            });
        }
        self.commands.push(BackendCommand::Present);
        Ok(())
    }

    fn frame_stats(&self) -> FrameStats {
        self.stats
    }

    fn readback_attachment(
        &mut self,
        _attachment: TextureHandle,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, RenderError> {
        // NullBackend produces a deterministic gradient as a placeholder
        // readback so tests can verify the readback-path machinery.
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                out.push((x & 0xFF) as u8);
                out.push((y & 0xFF) as u8);
                out.push(((x ^ y) & 0xFF) as u8);
                out.push(0xFF);
            }
        }
        Ok(out)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetHandle;
    use crate::material::Material;
    use crate::mesh::{Mesh, VertexAttributeLayout};
    use crate::scene::NodeId;

    fn dummy_draw_call() -> DrawCall {
        DrawCall {
            world_matrix: Mat4::IDENTITY,
            mesh: Mesh {
                layout: VertexAttributeLayout::standard_pbr(),
                vertex_buffer: AssetHandle::new(0),
                vertex_count: 3,
                index_count: 3,
                ..Mesh::EMPTY
            },
            material: Material::DEFAULT_PBR,
            mesh_asset: AssetHandle::INVALID,
            source_node: NodeId(0),
            view_depth: 5.0,
        }
    }

    #[test]
    fn null_backend_default_swapchain_1x1() {
        let b = NullBackend::new();
        assert_eq!(b.swapchain_width, 1);
        assert_eq!(b.swapchain_height, 1);
    }

    #[test]
    fn null_backend_resize_swapchain() {
        let mut b = NullBackend::new();
        let (w, h) = b.resize_swapchain(1920, 1080).unwrap();
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
    }

    #[test]
    fn null_backend_begin_end_pass_pairing() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        let ctx = PassContext {
            kind: PassKind::GeometryPass,
            ..PassContext::default()
        };
        b.begin_pass(&ctx).unwrap();
        b.end_pass(PassKind::GeometryPass).unwrap();
        b.end_frame().unwrap();
        // Two commands : BeginPass, EndPass.
        assert_eq!(b.commands.len(), 2);
    }

    #[test]
    fn null_backend_double_begin_errors() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        let ctx = PassContext::default();
        b.begin_pass(&ctx).unwrap();
        // Second begin without end : error.
        let res = b.begin_pass(&ctx);
        assert!(matches!(res, Err(RenderError::Backend(_))));
    }

    #[test]
    fn null_backend_end_pass_kind_mismatch_errors() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        let ctx = PassContext {
            kind: PassKind::GeometryPass,
            ..PassContext::default()
        };
        b.begin_pass(&ctx).unwrap();
        let res = b.end_pass(PassKind::TonemapPass);
        assert!(matches!(res, Err(RenderError::Backend(_))));
    }

    #[test]
    fn null_backend_draw_outside_pass_errors() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        let dc = dummy_draw_call();
        let res = b.draw(&dc);
        assert!(matches!(res, Err(RenderError::Backend(_))));
    }

    #[test]
    fn null_backend_draw_records_stats() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        let ctx = PassContext::default();
        b.begin_pass(&ctx).unwrap();
        let dc = dummy_draw_call();
        b.draw(&dc).unwrap();
        b.draw(&dc).unwrap();
        b.end_pass(ctx.kind).unwrap();
        b.end_frame().unwrap();

        let stats = b.frame_stats();
        assert_eq!(stats.draw_calls, 2);
        assert_eq!(stats.passes_executed, 1);
        assert_eq!(stats.primitives, 2);
        assert_eq!(stats.vertices, 6);
    }

    #[test]
    fn null_backend_present_zero_swapchain_errors() {
        let mut b = NullBackend::new();
        b.swapchain_width = 0;
        b.swapchain_height = 0;
        let res = b.present();
        assert!(matches!(res, Err(RenderError::SwapchainNotReady { .. })));
    }

    #[test]
    fn null_backend_present_records_command() {
        let mut b = NullBackend::new();
        b.resize_swapchain(800, 600).unwrap();
        b.present().unwrap();
        assert_eq!(b.count_presents(), 1);
    }

    #[test]
    fn null_backend_count_begin_pass_by_kind() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        let geom = PassContext {
            kind: PassKind::GeometryPass,
            ..PassContext::default()
        };
        let tone = PassContext {
            kind: PassKind::TonemapPass,
            ..PassContext::default()
        };
        b.begin_pass(&geom).unwrap();
        b.end_pass(PassKind::GeometryPass).unwrap();
        b.begin_pass(&tone).unwrap();
        b.end_pass(PassKind::TonemapPass).unwrap();
        b.end_frame().unwrap();

        assert_eq!(b.count_begin_pass(PassKind::GeometryPass), 1);
        assert_eq!(b.count_begin_pass(PassKind::TonemapPass), 1);
        assert_eq!(b.count_begin_pass(PassKind::ShadowPass), 0);
    }

    #[test]
    fn null_backend_reset_clears_state() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        b.begin_pass(&PassContext::default()).unwrap();
        b.end_pass(PassKind::GeometryPass).unwrap();
        assert!(!b.commands.is_empty());
        b.reset();
        assert!(b.commands.is_empty());
        assert_eq!(b.stats.draw_calls, 0);
    }

    #[test]
    fn null_backend_readback_produces_rgba_bytes() {
        let mut b = NullBackend::new();
        let bytes = b.readback_attachment(TextureHandle::new(0), 4, 4).unwrap();
        // 4×4 pixels × 4 bytes = 64.
        assert_eq!(bytes.len(), 64);
        // Alpha column is always 0xFF.
        assert_eq!(bytes[3], 0xFF);
    }

    #[test]
    fn null_backend_description_nonempty() {
        let b = NullBackend::new();
        assert!(!b.description().is_empty());
    }

    #[test]
    fn null_backend_end_frame_with_active_pass_errors() {
        let mut b = NullBackend::new();
        b.begin_frame().unwrap();
        b.begin_pass(&PassContext::default()).unwrap();
        // Forgetting end_pass should fail end_frame.
        let res = b.end_frame();
        assert!(matches!(res, Err(RenderError::Backend(_))));
    }

    #[test]
    fn render_error_backend_display() {
        let e = RenderError::Backend("device lost".into());
        let s = format!("{e}");
        assert!(s.contains("device lost"));
    }

    #[test]
    fn frame_stats_default_zeroed() {
        let s = FrameStats::default();
        assert_eq!(s.passes_executed, 0);
        assert_eq!(s.draw_calls, 0);
        assert_eq!(s.primitives, 0);
        assert_eq!(s.vertices, 0);
    }
}
