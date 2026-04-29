//! `MTLComputePipelineState` + `MTLRenderPipelineState` descriptors.
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Metal` +
//!          `specs/14_BACKEND.csl § OWNED MSL EMITTER` (stage2 hand-off).
//!
//! § DESIGN
//!   - [`ComputePipelineDescriptor`] carries the compile-input for a compute
//!     pipeline — entry-point name + (eventually) MSL source bytes from the
//!     S6-D3 emitter ; at S6-E3 the source comes from `msl_blob`.
//!   - [`RenderPipelineDescriptor`] carries vertex + fragment entry-point
//!     names + bind-group layout for a basic graphics pipeline.
//!   - [`PipelineHandle`] is the post-compile capability marker. Apple-side
//!     it carries the real `MTLComputePipelineState` / `MTLRenderPipelineState`
//!     ; non-Apple side it carries the descriptor for test-shape inspection.
//!   - [`BindGroupLayout`] models tier-1 argument-table bindings — slot, kind
//!     (buffer / texture / sampler), stage-mask. Tier-2 bindless lands with
//!     the GPU body emitters (D-phase).

/// Bind-group binding kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindKind {
    /// Buffer (storage / uniform / argument).
    Buffer,
    /// Texture (1D / 2D / 3D / Cube).
    Texture,
    /// Sampler-state.
    Sampler,
}

impl BindKind {
    /// Short canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Buffer => "buffer",
            Self::Texture => "texture",
            Self::Sampler => "sampler",
        }
    }
}

/// Stage-visibility mask — which shader stages see this binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageMask {
    /// Visible in vertex stage.
    pub vertex: bool,
    /// Visible in fragment stage.
    pub fragment: bool,
    /// Visible in compute stage.
    pub compute: bool,
}

impl StageMask {
    /// Compute-only.
    #[must_use]
    pub const fn compute_only() -> Self {
        Self {
            vertex: false,
            fragment: false,
            compute: true,
        }
    }

    /// Vertex + fragment (graphics).
    #[must_use]
    pub const fn graphics() -> Self {
        Self {
            vertex: true,
            fragment: true,
            compute: false,
        }
    }

    /// All stages.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            vertex: true,
            fragment: true,
            compute: true,
        }
    }
}

/// One binding in a pipeline's argument-table layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutBinding {
    /// Argument-table slot index.
    pub slot: u32,
    /// What lives in this slot.
    pub kind: BindKind,
    /// Stage-visibility mask.
    pub stages: StageMask,
}

/// Bind-group layout for a pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindGroupLayout {
    /// Layout name (debug-info).
    pub label: String,
    /// Ordered slot bindings.
    pub bindings: Vec<LayoutBinding>,
}

impl BindGroupLayout {
    /// Empty layout (no bindings).
    #[must_use]
    pub fn empty(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            bindings: Vec::new(),
        }
    }

    /// Append a binding.
    #[must_use]
    pub fn with_binding(mut self, slot: u32, kind: BindKind, stages: StageMask) -> Self {
        self.bindings.push(LayoutBinding { slot, kind, stages });
        self
    }
}

/// Compute-pipeline descriptor — input to `MTLDevice newComputePipelineStateWithFunction`.
#[derive(Debug, Clone)]
pub struct ComputePipelineDescriptor {
    /// Pipeline label (debug-info).
    pub label: String,
    /// MSL source bytes (placeholder until S6-D3 emits real MSL).
    pub msl_source: String,
    /// Compute kernel entry-point name.
    pub entry_point: String,
    /// Threadgroup size hint `(x, y, z)`.
    pub threadgroup_size: (u32, u32, u32),
    /// Argument-table layout.
    pub layout: BindGroupLayout,
}

impl ComputePipelineDescriptor {
    /// Construct a compute descriptor with the supplied MSL + entry.
    #[must_use]
    pub fn new(
        label: impl Into<String>,
        msl_source: impl Into<String>,
        entry_point: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            msl_source: msl_source.into(),
            entry_point: entry_point.into(),
            threadgroup_size: (32, 1, 1),
            layout: BindGroupLayout::empty("compute_default"),
        }
    }

    /// Override the threadgroup size hint.
    #[must_use]
    pub fn with_threadgroup_size(mut self, x: u32, y: u32, z: u32) -> Self {
        self.threadgroup_size = (x, y, z);
        self
    }

    /// Override the bind-group layout.
    #[must_use]
    pub fn with_layout(mut self, layout: BindGroupLayout) -> Self {
        self.layout = layout;
        self
    }
}

/// Render-pipeline descriptor — input to `MTLDevice newRenderPipelineStateWithDescriptor`.
#[derive(Debug, Clone)]
pub struct RenderPipelineDescriptor {
    /// Pipeline label (debug-info).
    pub label: String,
    /// MSL source bytes (placeholder until S6-D3).
    pub msl_source: String,
    /// Vertex entry-point name.
    pub vertex_entry: String,
    /// Fragment entry-point name.
    pub fragment_entry: String,
    /// Pixel format string (`"bgra8Unorm"` / `"rgba16Float"` / etc.).
    pub color_pixel_format: String,
    /// Argument-table layout shared between vertex + fragment.
    pub layout: BindGroupLayout,
}

impl RenderPipelineDescriptor {
    /// Construct a render descriptor.
    #[must_use]
    pub fn new(
        label: impl Into<String>,
        msl_source: impl Into<String>,
        vertex_entry: impl Into<String>,
        fragment_entry: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            msl_source: msl_source.into(),
            vertex_entry: vertex_entry.into(),
            fragment_entry: fragment_entry.into(),
            color_pixel_format: "bgra8Unorm".into(),
            layout: BindGroupLayout::empty("render_default"),
        }
    }

    /// Override the color pixel format.
    #[must_use]
    pub fn with_color_pixel_format(mut self, fmt: impl Into<String>) -> Self {
        self.color_pixel_format = fmt.into();
        self
    }
}

/// Compiled pipeline-state handle.
#[derive(Debug)]
pub struct PipelineHandle {
    /// Pipeline label.
    pub label: String,
    /// Pipeline kind (compute vs render).
    pub kind: PipelineKind,
    /// Inner state — Apple-side carries the FFI handle ; stub side carries
    /// the descriptor for shape-inspection.
    pub(crate) inner: PipelineInner,
}

/// Pipeline kind (compute vs render).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineKind {
    /// Compute pipeline.
    Compute,
    /// Render pipeline (vertex + fragment).
    Render,
}

impl PipelineKind {
    /// Short canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compute => "compute",
            Self::Render => "render",
        }
    }
}

#[derive(Debug)]
pub(crate) enum PipelineInner {
    /// Stub state — descriptor preserved for inspection.
    StubCompute(ComputePipelineDescriptor),
    /// Stub state — descriptor preserved for inspection.
    StubRender(RenderPipelineDescriptor),
    /// Apple-side handle.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    AppleCompute {
        /// Index into the Apple-session pipeline pool.
        pool_idx: u32,
    },
    /// Apple-side handle.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    AppleRender {
        /// Index into the Apple-session pipeline pool.
        pool_idx: u32,
    },
}

impl PipelineHandle {
    /// Stub-construct a compute pipeline handle from a descriptor.
    #[must_use]
    pub fn stub_compute(desc: ComputePipelineDescriptor) -> Self {
        Self {
            label: desc.label.clone(),
            kind: PipelineKind::Compute,
            inner: PipelineInner::StubCompute(desc),
        }
    }

    /// Stub-construct a render pipeline handle from a descriptor.
    #[must_use]
    pub fn stub_render(desc: RenderPipelineDescriptor) -> Self {
        Self {
            label: desc.label.clone(),
            kind: PipelineKind::Render,
            inner: PipelineInner::StubRender(desc),
        }
    }

    /// Returns `true` when this handle was created via the stub path.
    #[must_use]
    pub fn is_stub(&self) -> bool {
        matches!(
            self.inner,
            PipelineInner::StubCompute(_) | PipelineInner::StubRender(_)
        )
    }

    /// Returns the compute-descriptor (if this is a stub-compute handle).
    #[must_use]
    pub fn stub_compute_desc(&self) -> Option<&ComputePipelineDescriptor> {
        match &self.inner {
            PipelineInner::StubCompute(d) => Some(d),
            PipelineInner::StubRender(_) => None,
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            PipelineInner::AppleCompute { .. } | PipelineInner::AppleRender { .. } => None,
        }
    }

    /// Returns the render-descriptor (if this is a stub-render handle).
    #[must_use]
    pub fn stub_render_desc(&self) -> Option<&RenderPipelineDescriptor> {
        match &self.inner {
            PipelineInner::StubRender(d) => Some(d),
            PipelineInner::StubCompute(_) => None,
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            PipelineInner::AppleCompute { .. } | PipelineInner::AppleRender { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BindGroupLayout, BindKind, ComputePipelineDescriptor, PipelineHandle, PipelineKind,
        RenderPipelineDescriptor, StageMask,
    };

    #[test]
    fn bind_kind_names() {
        assert_eq!(BindKind::Buffer.as_str(), "buffer");
        assert_eq!(BindKind::Texture.as_str(), "texture");
        assert_eq!(BindKind::Sampler.as_str(), "sampler");
    }

    #[test]
    fn stage_mask_variants() {
        let c = StageMask::compute_only();
        assert!(c.compute);
        assert!(!c.vertex);
        assert!(!c.fragment);
        let g = StageMask::graphics();
        assert!(g.vertex);
        assert!(g.fragment);
        assert!(!g.compute);
        let a = StageMask::all();
        assert!(a.vertex && a.fragment && a.compute);
    }

    #[test]
    fn empty_layout_has_no_bindings() {
        let l = BindGroupLayout::empty("test");
        assert_eq!(l.label, "test");
        assert!(l.bindings.is_empty());
    }

    #[test]
    fn with_binding_appends() {
        let l = BindGroupLayout::empty("l")
            .with_binding(0, BindKind::Buffer, StageMask::compute_only())
            .with_binding(1, BindKind::Texture, StageMask::graphics());
        assert_eq!(l.bindings.len(), 2);
        assert_eq!(l.bindings[0].slot, 0);
        assert_eq!(l.bindings[1].kind, BindKind::Texture);
    }

    #[test]
    fn compute_descriptor_defaults() {
        let d = ComputePipelineDescriptor::new("c", "kernel void f() {}", "f");
        assert_eq!(d.label, "c");
        assert_eq!(d.entry_point, "f");
        assert_eq!(d.threadgroup_size, (32, 1, 1));
        assert!(d.layout.bindings.is_empty());
    }

    #[test]
    fn compute_descriptor_with_threadgroup_size() {
        let d = ComputePipelineDescriptor::new("c", "...", "f").with_threadgroup_size(8, 8, 1);
        assert_eq!(d.threadgroup_size, (8, 8, 1));
    }

    #[test]
    fn render_descriptor_defaults() {
        let d = RenderPipelineDescriptor::new("r", "...", "vs_main", "fs_main");
        assert_eq!(d.label, "r");
        assert_eq!(d.vertex_entry, "vs_main");
        assert_eq!(d.fragment_entry, "fs_main");
        assert_eq!(d.color_pixel_format, "bgra8Unorm");
    }

    #[test]
    fn render_descriptor_with_color_pixel_format() {
        let d = RenderPipelineDescriptor::new("r", "...", "v", "f")
            .with_color_pixel_format("rgba16Float");
        assert_eq!(d.color_pixel_format, "rgba16Float");
    }

    #[test]
    fn stub_compute_handle_round_trip() {
        let d = ComputePipelineDescriptor::new("c", "src", "f");
        let h = PipelineHandle::stub_compute(d.clone());
        assert!(h.is_stub());
        assert_eq!(h.kind, PipelineKind::Compute);
        let got = h.stub_compute_desc().unwrap();
        assert_eq!(got.label, d.label);
        assert_eq!(got.entry_point, d.entry_point);
    }

    #[test]
    fn stub_render_handle_round_trip() {
        let d = RenderPipelineDescriptor::new("r", "src", "v", "f");
        let h = PipelineHandle::stub_render(d.clone());
        assert!(h.is_stub());
        assert_eq!(h.kind, PipelineKind::Render);
        let got = h.stub_render_desc().unwrap();
        assert_eq!(got.label, d.label);
    }

    #[test]
    fn pipeline_kind_names() {
        assert_eq!(PipelineKind::Compute.as_str(), "compute");
        assert_eq!(PipelineKind::Render.as_str(), "render");
    }
}
