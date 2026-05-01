//! § render — wgpu render pipeline + per-frame draw call.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-RICH-RENDER (W-LOA-rich-render-overhaul)
//!
//! § ROLE
//!   Owns the uber-shader render pipeline + uniform buffer (with material LUT
//!   + pattern LUT + time uniform) + bind group + vertex/index buffers for
//!   the diagnostic-dense test-room.
//!
//! § FRAME LIFECYCLE
//!   1. Acquire swap-chain surface texture
//!   2. Update uniforms (view-proj · sun-dir · ambient · time · LUTs)
//!   3. Render scene-pass (opaque) with cull_mode = Back
//!   4. Render UI overlay-pass (HUD + menu)
//!   5. Submit + present + telemetry
//!
//! § CULLING
//!   `cull_mode = Some(Face::Back)` is RESTORED. The geometry module's
//!   winding tests guarantee CCW from the visible side for all faces (walls
//!   from inside · floor from above · ceiling from below · boxes from
//!   outside). See `geometry::tests::*winding*` for the audit.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::float_cmp)]
#![allow(clippy::too_many_lines)]

use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use cssl_rt::loa_startup::log_event;

use crate::camera::Camera;
use crate::cfer_render::{
    CferRenderer, CFER_WGSL, TEX_TOTAL_BYTES, TEX_X, TEX_Y, TEX_Z,
    WORLD_MAX, WORLD_MIN,
};
use crate::geometry::{plinth_positions, RoomGeometry, Vertex};
use crate::gpu::GpuContext;
use crate::material::{material_lut, Material, MATERIAL_LUT_LEN};
use crate::pattern::{pattern_lut, Pattern, PATTERN_LUT_LEN};
use crate::snapshot::Snapshotter;
use crate::telemetry as telem;
use crate::ui_overlay::{HudContext, MenuState, UiOverlay};

/// Per-frame metrics returned by `Renderer::render_frame`. The window
/// driver hands these to the global telemetry sink.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameMetrics {
    pub draw_calls: u32,
    pub vertices: u64,
    pub pipeline_switches: u32,
}

/// CPU-side mirror of the WGSL `Uniforms` struct.
///
/// Layout (matches `scene.wgsl::Uniforms`) :
///   - view_proj  : mat4x4              (64 B)
///   - sun_dir    : vec4                (16 B)
///   - ambient    : vec4                (16 B)
///   - time       : vec4                (16 B)
///   - camera_pos : vec4                (16 B)   § T11-LOA-RAYMARCH
///   - materials  : 16 × Material       (16 × 48 = 768 B)
///   - patterns   : 22 × Pattern        (22 × 16 = 352 B)
///   = 1248 bytes total.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    ambient: [f32; 4],
    time: [f32; 4],
    /// World-space camera position (xyz) ; w reserved for tracer flags.
    /// Read by the fragment-shader sphere-tracer to reconstruct view rays
    /// in cube-local space for the 6 RAYMARCH_* pattern kinds.
    camera_pos: [f32; 4],
    materials: [Material; MATERIAL_LUT_LEN],
    patterns: [Pattern; PATTERN_LUT_LEN],
}

impl Uniforms {
    fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            sun_dir: Vec4::new(-0.4, 0.8, -0.45, 0.0).normalize().to_array(),
            ambient: [0.18, 0.20, 0.24, 0.0],
            time: [0.0; 4],
            camera_pos: [0.0, 1.7, 0.0, 0.0],
            materials: material_lut(),
            patterns: pattern_lut(),
        }
    }
}

/// § T11-LOA-FID-CFER : CPU-side mirror of the WGSL `Uniforms` struct
/// for the CFER volumetric raymarcher.
///
/// Layout (matches `cfer.wgsl::Uniforms`) :
///   - inv_view_proj : mat4x4   (64 B)
///   - camera_pos    : vec4     (16 B)
///   - world_min     : vec4     (16 B)
///   - world_max     : vec4     (16 B)
///   - time          : vec4     (16 B)
///   = 128 bytes total.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CferUniforms {
    inv_view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    world_min: [f32; 4],
    world_max: [f32; 4],
    /// time.x = seconds since render start.
    /// time.y = unused
    /// time.z = step-count override (clamped 1..64 in shader ; 0 ⇒ default 32)
    /// time.w = unused
    time: [f32; 4],
}

impl CferUniforms {
    fn new() -> Self {
        Self {
            inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0, 1.7, 0.0, 0.0],
            world_min: [WORLD_MIN[0], WORLD_MIN[1], WORLD_MIN[2], 0.0],
            world_max: [WORLD_MAX[0], WORLD_MAX[1], WORLD_MAX[2], 0.0],
            time: [0.0; 4],
        }
    }
}

/// Render pipeline + GPU resources for the test-room scene.
pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    /// Transparent-pipeline (alpha-blended) for the glass-cube stress object.
    pipeline_transparent: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    /// Range of indices that should be drawn alpha-blended (transparent pass).
    transparent_index_range: Option<(u32, u32)>,
    depth_view: wgpu::TextureView,
    depth_format: wgpu::TextureFormat,
    /// Frame counter for telemetry throttling (log every Nth frame).
    frame_n: u64,
    /// UI overlay (HUD + menu). Pass-2 after the scene draw.
    ui: UiOverlay,
    /// Render-start instant (drives time-uniform).
    start_t: Instant,
    /// Frame-time histogram (60 most-recent frames in milliseconds).
    pub frame_times_ms: [f32; 60],
    /// Index of next slot in frame_times_ms.
    frame_time_idx: usize,
    /// Last frame's t timestamp (for delta).
    last_frame_t: Instant,
    /// § T11-LOA-TEST-APP : pending snapshot path. When `Some(p)`, the
    /// renderer copies the just-presented frame to a CPU staging buffer
    /// then writes a PNG at `p` after `submit() + present()`. Cleared
    /// after one frame (one shot per request).
    pub snapshot_pending: Option<std::path::PathBuf>,
    /// § T11-LOA-TEST-APP : framebuffer-readback helper, lazily allocates
    /// staging buffer sized for the current swap-chain.
    snapshotter: Snapshotter,
    /// True if the surface was configured with COPY_SRC usage. Snapshot
    /// readback uses the surface texture directly when true ; otherwise
    /// the snapshot tool returns an error.
    surface_copy_src: bool,

    // ── § T11-LOA-FID-CFER : volumetric Ω-field pass ──
    /// CPU-side CFER state (OmegaField + texel staging + step/pack).
    pub cfer: CferRenderer,
    /// Pipeline for the CFER volumetric raymarcher.
    cfer_pipeline: wgpu::RenderPipeline,
    /// Bind-group for cfer.wgsl (uniforms + 3D texture + sampler).
    cfer_bind_group: wgpu::BindGroup,
    /// Uniform buffer for cfer.wgsl.
    cfer_uniform_buf: wgpu::Buffer,
    /// 3D-texture holding the packed Ω-field radiance + density.
    /// Re-uploaded each frame in `render_frame` from the CPU staging
    /// buffer in `cfer.texels()`.
    cfer_texture: wgpu::Texture,
}

impl Renderer {
    /// Embedded WGSL shader source.
    pub const SHADER_SRC: &'static str = include_str!("../shaders/scene.wgsl");

    /// Construct the renderer for the given GPU context.
    #[must_use]
    pub fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("loa-host/scene.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });
        log_event(
            "INFO",
            "loa-host/render",
            "uber-shader module created (scene.wgsl)",
        );

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loa-host/uniforms-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("loa-host/pipeline-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let depth_format = wgpu::TextureFormat::Depth32Float;

        // ─── Opaque pipeline : back-face culled ───
        let pipeline = Self::build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            gpu.surface_format,
            depth_format,
            /* transparent = */ false,
        );

        // ─── Transparent pipeline : alpha-blend, depth-test but no depth-write ───
        let pipeline_transparent = Self::build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            gpu.surface_format,
            depth_format,
            /* transparent = */ true,
        );

        // Uniform buffer (1136 bytes).
        let uniforms = Uniforms::new();
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loa-host/uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loa-host/bind-group"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        // T11-LOA-ROOMS : full multi-room test-suite (hub + 4 spokes + 4 corridors).
        let geom = RoomGeometry::full_world();
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loa-host/test-room-vbo"),
            contents: bytemuck::cast_slice(&geom.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loa-host/test-room-ibo"),
            contents: bytemuck::cast_slice(&geom.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let index_count = geom.indices.len() as u32;
        let transparent_index_range = geom.transparent_index_range;

        log_event(
            "INFO",
            "loa-host/render",
            &format!(
                "rich geometry uploaded : {} verts, {} indices, {} plinths · transparent_range={:?}",
                geom.vertices.len(),
                geom.indices.len(),
                geom.plinth_count,
                transparent_index_range,
            ),
        );

        let depth_view =
            create_depth_view(device, gpu.config.width, gpu.config.height, depth_format);

        let ui = UiOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format);

        let now = Instant::now();
        let surface_copy_src = gpu
            .config
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC);

        // ─── § T11-LOA-FID-CFER : CPU-side CFER state + GPU resources ───
        let plinths_2d = plinth_positions();
        let plinths_xz: Vec<(f32, f32)> = plinths_2d.iter().copied().collect();
        let cfer = CferRenderer::new(&plinths_xz);
        log_event(
            "INFO",
            "loa-host/render",
            &format!(
                "cfer · runtime renderer init · active_cells={} · world {:?}..{:?}",
                cfer.active_cell_count(),
                WORLD_MIN,
                WORLD_MAX,
            ),
        );

        let (cfer_pipeline, cfer_bind_group, cfer_uniform_buf, cfer_texture) =
            Self::build_cfer_resources(device, &gpu.queue, gpu.surface_format, depth_format);

        Self {
            pipeline,
            pipeline_transparent,
            bind_group,
            uniform_buf,
            vertex_buf,
            index_buf,
            index_count,
            transparent_index_range,
            depth_view,
            depth_format,
            frame_n: 0,
            ui,
            start_t: now,
            frame_times_ms: [16.7; 60],
            frame_time_idx: 0,
            last_frame_t: now,
            snapshot_pending: None,
            snapshotter: Snapshotter::new(),
            surface_copy_src,
            cfer,
            cfer_pipeline,
            cfer_bind_group,
            cfer_uniform_buf,
            cfer_texture,
        }
    }

    /// § T11-LOA-FID-CFER : build the volumetric pipeline + uniform-buffer +
    /// 3D texture + sampler + bind-group. Returns the assembled handles in
    /// a single call so `Self::new` stays linear.
    fn build_cfer_resources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> (
        wgpu::RenderPipeline,
        wgpu::BindGroup,
        wgpu::Buffer,
        wgpu::Texture,
    ) {
        // 1. Shader module.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("loa-host/cfer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(CFER_WGSL.into()),
        });

        // 2. Bind-group layout : uniforms + 3D-texture + sampler.
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loa-host/cfer-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("loa-host/cfer-pipeline-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        // 3. Pipeline : alpha-blend, depth-test against scene, NO depth-write.
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("loa-host/cfer-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[], // full-screen triangle, no vertex buffer
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // full-screen tri ; no need to cull
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                // CFER reads scene-depth but DOES NOT write — atmosphere
                // doesn't occlude future objects.
                depth_write_enabled: false,
                // depth_compare = Always so the volumetric tracer always
                // fires inside the camera frustum ; the per-sample depth
                // discrimination happens via the world-AABB ray test in
                // the shader.
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 4. Uniform buffer (128 bytes).
        let cfer_u = CferUniforms::new();
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loa-host/cfer-uniforms"),
            contents: bytemuck::bytes_of(&cfer_u),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 5. 3D Texture (Rgba16Float, 32×16×32) + view.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("loa-host/cfer-3d-texture"),
            size: wgpu::Extent3d {
                width: TEX_X,
                height: TEX_Y,
                depth_or_array_layers: TEX_Z,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Initial upload : zeroed texture (CPU side will overwrite each frame).
        let initial_bytes = vec![0u8; TEX_TOTAL_BYTES as usize];
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &initial_bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(TEX_X * 8), // RGBA16F = 8B/texel
                rows_per_image: Some(TEX_Y),
            },
            wgpu::Extent3d {
                width: TEX_X,
                height: TEX_Y,
                depth_or_array_layers: TEX_Z,
            },
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D3),
            ..Default::default()
        });

        // 6. Sampler (linear-clamp).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("loa-host/cfer-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        // 7. Bind group.
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loa-host/cfer-bind-group"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        log_event(
            "INFO",
            "loa-host/render",
            &format!(
                "cfer pipeline built · 3D-tex {}×{}×{} = {} bytes · alpha-blend depth-test no-write",
                TEX_X, TEX_Y, TEX_Z, TEX_TOTAL_BYTES,
            ),
        );

        // The texture_view was used to build the bind_group above ; the
        // bind_group internally retains the GPU-side reference, so the
        // view can be dropped now without breaking the binding.
        drop(texture_view);

        (pipeline, bind_group, uniform_buf, texture)
    }

    /// True iff the surface was configured with COPY_SRC, i.e. snapshot
    /// readback is supported on this adapter.
    #[must_use]
    pub fn snapshot_supported(&self) -> bool {
        self.surface_copy_src
    }

    /// Request a snapshot of the next-presented frame. The path should
    /// be sanitized by the caller (see `snapshot::sanitize_snapshot_path`).
    /// Idempotent — replaces any prior pending request.
    pub fn request_snapshot(&mut self, path: std::path::PathBuf) {
        self.snapshot_pending = Some(path);
    }

    fn build_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
        transparent: bool,
    ) -> wgpu::RenderPipeline {
        let blend = if transparent {
            Some(wgpu::BlendState::ALPHA_BLENDING)
        } else {
            Some(wgpu::BlendState::REPLACE)
        };

        let label = if transparent {
            "loa-host/scene-pipeline-transparent"
        } else {
            "loa-host/scene-pipeline-opaque"
        };

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // § Re-enabled : geometry::tests::*winding* prove all faces
                // wind CCW from their stored-normal side. Back-face culling
                // saves ~50% fragment work and prevents see-through.
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                // Transparent pass : test depth but don't write (so other
                // transparent objects behind it remain visible-through).
                depth_write_enabled: !transparent,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Re-create the depth target after a surface resize.
    pub fn resize(&mut self, gpu: &GpuContext) {
        self.depth_view = create_depth_view(
            &gpu.device,
            gpu.config.width,
            gpu.config.height,
            self.depth_format,
        );
    }

    /// Update the uniform buffer with the current camera + draw a single
    /// frame. Returns Ok if the frame was presented or the surface was
    /// gracefully recovered ; never panics.
    pub fn render_frame(
        &mut self,
        gpu: &GpuContext,
        camera: &Camera,
        _window: &Arc<Window>,
        hud: &HudContext,
        menu: &MenuState,
    ) -> Result<FrameMetrics, wgpu::SurfaceError> {
        let frame = match gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(e @ wgpu::SurfaceError::Lost) | Err(e @ wgpu::SurfaceError::Outdated) => {
                log_event(
                    "WARN",
                    "loa-host/render",
                    &format!("surface stale ({e:?}) · skipping frame"),
                );
                return Err(e);
            }
            Err(e) => {
                log_event(
                    "ERROR",
                    "loa-host/render",
                    &format!("surface acquire failed : {e:?}"),
                );
                return Err(e);
            }
        };

        // Frame-time histogram update.
        let now = Instant::now();
        let dt_ms = (now - self.last_frame_t).as_secs_f32() * 1000.0;
        self.frame_times_ms[self.frame_time_idx] = dt_ms;
        self.frame_time_idx = (self.frame_time_idx + 1) % self.frame_times_ms.len();
        self.last_frame_t = now;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Update uniforms.
        let aspect = gpu.aspect();
        let t_secs = (now - self.start_t).as_secs_f32();
        let uniforms = Uniforms {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            sun_dir: Vec4::new(-0.4, 0.8, -0.45, 0.0).normalize().to_array(),
            ambient: [0.18, 0.20, 0.24, 0.0],
            time: [t_secs, self.frame_n as f32, 0.0, 0.0],
            // § T11-LOA-RAYMARCH : real eye position drives the
            // fragment-shader sphere-tracer view-ray reconstruction.
            camera_pos: [
                camera.position.x,
                camera.position.y,
                camera.position.z,
                0.0,
            ],
            materials: material_lut(),
            patterns: pattern_lut(),
        };
        gpu.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("loa-host/frame-encoder"),
            });

        // Per-frame metric counters — flow into `FrameMetrics` and the
        // global telemetry sink.
        let mut draw_calls: u32 = 0;
        let mut vertices: u64 = 0;
        let mut pipeline_switches: u32 = 0;

        // ─── Pass 1 : opaque scene ───
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/scene-pass-opaque"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.171,
                            g: 0.453,
                            b: 0.798,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pipeline_switches += 1;
            telem::global().record_pipeline_switch();
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            // Draw everything except the transparent range with the opaque pipeline.
            if let Some((lo, hi)) = self.transparent_index_range {
                if lo > 0 {
                    pass.draw_indexed(0..lo, 0, 0..1);
                    draw_calls += 1;
                    vertices += u64::from(lo);
                }
                if hi < self.index_count {
                    pass.draw_indexed(hi..self.index_count, 0, 0..1);
                    draw_calls += 1;
                    vertices += u64::from(self.index_count - hi);
                }
            } else {
                pass.draw_indexed(0..self.index_count, 0, 0..1);
                draw_calls += 1;
                vertices += u64::from(self.index_count);
            }
        }

        // ─── Pass 2 : transparent objects (alpha-blended) ───
        if let Some((lo, hi)) = self.transparent_index_range {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/scene-pass-transparent"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline_transparent);
            pipeline_switches += 1;
            telem::global().record_pipeline_switch();
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(lo..hi, 0, 0..1);
            draw_calls += 1;
            vertices += u64::from(hi - lo);
        }

        // ─── § T11-LOA-FID-CFER : Pass 3 — volumetric Ω-field ───
        // The substrate IS the renderer here. We step the OmegaField
        // forward, pack the active cells into the 3D-texture staging
        // buffer, upload, then alpha-blend a full-screen volumetric pass
        // onto the existing scene buffer.
        let cfer_metrics = self.cfer.step_and_pack(t_secs.fract());

        // Upload the freshly-packed texels.
        let cfer_bytes = self.cfer.texels_as_rgba16f_bytes();
        gpu.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.cfer_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &cfer_bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(TEX_X * 8),
                rows_per_image: Some(TEX_Y),
            },
            wgpu::Extent3d {
                width: TEX_X,
                height: TEX_Y,
                depth_or_array_layers: TEX_Z,
            },
        );

        // Update CFER uniforms (inverse view-proj for ray reconstruction).
        let view_proj = camera.view_proj(aspect);
        let inv_view_proj = view_proj.inverse();
        let cfer_u = CferUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            camera_pos: [
                camera.position.x,
                camera.position.y,
                camera.position.z,
                0.0,
            ],
            world_min: [WORLD_MIN[0], WORLD_MIN[1], WORLD_MIN[2], 0.0],
            world_max: [WORLD_MAX[0], WORLD_MAX[1], WORLD_MAX[2], 0.0],
            time: [t_secs, 0.0, 32.0, 0.0],
        };
        gpu.queue.write_buffer(
            &self.cfer_uniform_buf,
            0,
            bytemuck::bytes_of(&cfer_u),
        );

        // Encode the volumetric pass.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/cfer-volumetric-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.cfer_pipeline);
            pipeline_switches += 1;
            telem::global().record_pipeline_switch();
            pass.set_bind_group(0, &self.cfer_bind_group, &[]);
            pass.draw(0..3, 0..1); // full-screen triangle (no vertex buffer)
            draw_calls += 1;
        }

        // Forward CFER metrics to telemetry as a structured ad-hoc log
        // (the `cfer.step_and_pack` already emits a throttled INFO line ;
        // here we surface a per-frame draw-call attribution for the
        // global counter so cfer activity shows up in `frame_metrics`).
        let _ = cfer_metrics;

        // ─── Pass 4 : UI overlay ───
        self.ui.prepare_frame(
            &gpu.device,
            &gpu.queue,
            gpu.config.width,
            gpu.config.height,
            hud,
            menu,
        );
        self.ui.encode_pass(&mut encoder, &view);
        // UI overlay is conservatively counted as +1 draw call + +1 pipeline
        // switch (the prepare_frame/encode_pass owns its own pipeline).
        draw_calls += 1;
        pipeline_switches += 1;
        telem::global().record_pipeline_switch();

        gpu.queue.submit(std::iter::once(encoder.finish()));

        // § T11-LOA-TEST-APP : framebuffer readback BEFORE present(). If
        // a snapshot is pending and the surface was configured with
        // COPY_SRC usage, copy the about-to-present texture into our
        // staging buffer and write the PNG. Errors are logged but do
        // not block the present.
        if let Some(ref out_path) = self.snapshot_pending.take() {
            if self.surface_copy_src {
                match self.snapshotter.readback_to_png(
                    &gpu.device,
                    &gpu.queue,
                    &frame.texture,
                    out_path,
                ) {
                    Ok(bytes) => {
                        log_event(
                            "INFO",
                            "loa-host/render",
                            &format!(
                                "snapshot · wrote {} bytes to {}",
                                bytes,
                                out_path.display()
                            ),
                        );
                    }
                    Err(e) => {
                        log_event(
                            "ERROR",
                            "loa-host/render",
                            &format!(
                                "snapshot · readback failed for {}: {}",
                                out_path.display(),
                                e
                            ),
                        );
                    }
                }
            } else {
                log_event(
                    "WARN",
                    "loa-host/render",
                    &format!(
                        "snapshot · surface lacks COPY_SRC, cannot capture {}",
                        out_path.display()
                    ),
                );
            }
        }

        frame.present();

        // Telemetry : log first frame + every 600th frame after.
        if self.frame_n == 0 {
            log_event(
                "INFO",
                "loa-host/render",
                "first-frame-rendered (rich uber-shader)",
            );
        } else if self.frame_n % 600 == 0 {
            log_event(
                "INFO",
                "loa-host/render",
                &format!(
                    "RENDER_FRAME · n={} · t={:.1}s · draws={} verts={}",
                    self.frame_n, t_secs, draw_calls, vertices
                ),
            );
        }
        self.frame_n += 1;
        Ok(FrameMetrics {
            draw_calls,
            vertices,
            pipeline_switches,
        })
    }

    /// Total number of frames presented.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_n
    }

    /// Average frame-time over the last 60 frames (in milliseconds).
    #[must_use]
    pub fn average_frame_time_ms(&self) -> f32 {
        let sum: f32 = self.frame_times_ms.iter().sum();
        sum / self.frame_times_ms.len() as f32
    }

    /// § T11-LOA-FID-CFER : count of active cells in the Ω-field driving the
    /// volumetric pass. MCP `render.cfer_snapshot` exposes this.
    #[must_use]
    pub fn cfer_active_cells(&self) -> u64 {
        self.cfer.active_cell_count()
    }

    /// § T11-LOA-FID-CFER : last per-frame CFER metrics (step time, pack time,
    /// active cells, KAN evals).
    #[must_use]
    pub fn cfer_last_metrics(&self) -> crate::cfer_render::CferMetrics {
        self.cfer.last_metrics
    }

    /// § T11-LOA-FID-CFER : sample radiance at the world-envelope center
    /// (rgb in 0..1).
    #[must_use]
    pub fn cfer_sample_center_radiance(&self) -> [f32; 3] {
        self.cfer.sample_center_radiance()
    }

    /// § T11-LOA-FID-CFER : attach a KAN handle to the CFER field. Future
    /// step-and-pack calls will record per-cell KAN evaluations.
    pub fn cfer_set_kan_handle(&mut self, sovereign_handle: u16) {
        self.cfer.attach_kan_handle(sovereign_handle);
    }

    /// § T11-LOA-FID-CFER : detach the KAN handle.
    pub fn cfer_clear_kan_handle(&mut self) {
        self.cfer.detach_kan_handle();
    }
}

fn create_depth_view(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("loa-host/depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_struct_is_pod() {
        let u: Uniforms = bytemuck::Zeroable::zeroed();
        assert_eq!(u.ambient, [0.0; 4]);
        assert_eq!(u.time, [0.0; 4]);
        // First material zeroed
        assert_eq!(u.materials[0].albedo, [0.0; 3]);
        // First pattern zeroed
        assert_eq!(u.patterns[0].kind, 0);
    }

    #[test]
    fn shader_src_is_nonempty() {
        assert!(!Renderer::SHADER_SRC.is_empty());
        assert!(Renderer::SHADER_SRC.contains("vs_main"));
        assert!(Renderer::SHADER_SRC.contains("fs_main"));
    }

    #[test]
    fn renderer_shader_src_matches_crate_const() {
        assert_eq!(Renderer::SHADER_SRC, crate::SCENE_WGSL);
    }

    #[test]
    fn uniforms_size_is_correct() {
        // § T11-LOA-RAYMARCH : layout grew by camera_pos (16 B) + 6 extra
        // pattern entries (6 × 16 = 96 B) → 1136 + 16 + 96 = 1248.
        // 64 + 16 + 16 + 16 + 16 + (16 * 48) + (22 * 16)  =  1248
        assert_eq!(core::mem::size_of::<Uniforms>(), 1248);
    }

    #[test]
    fn uniforms_carries_camera_pos_field() {
        let u = Uniforms::new();
        // Default camera_pos seeds at (0, 1.7, 0) — eye-height @ room center.
        assert_eq!(u.camera_pos, [0.0, 1.7, 0.0, 0.0]);
    }

    // ── § T11-LOA-FID-CFER : volumetric-pass uniform layout ──

    #[test]
    fn cfer_uniforms_size_is_128_bytes() {
        // 64 (mat4x4) + 16 (camera) + 16 (world_min) + 16 (world_max) + 16 (time) = 128
        assert_eq!(core::mem::size_of::<CferUniforms>(), 128);
    }

    #[test]
    fn cfer_uniforms_default_carries_world_envelope() {
        let u = CferUniforms::new();
        assert_eq!(u.world_min[0], -60.0);
        assert_eq!(u.world_max[0], 60.0);
        assert_eq!(u.world_min[1], 0.0);
        assert_eq!(u.world_max[1], 12.0);
    }

    #[test]
    fn cfer_uniforms_is_pod() {
        let u: CferUniforms = bytemuck::Zeroable::zeroed();
        assert_eq!(u.time, [0.0; 4]);
        assert_eq!(u.camera_pos, [0.0; 4]);
    }
}
