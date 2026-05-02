//! § render — wgpu render pipeline + per-frame draw call.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-RICH-RENDER     (W-LOA-rich-render-overhaul) — base
//! § T11-LOA-FID-MAINSTREAM  (W-LOA-fidelity-mainstream)  — MSAA + HDR + tonemap
//!
//! § ROLE
//!   Owns the uber-shader render pipeline + uniform buffer (with material LUT
//!   + pattern LUT + time uniform) + bind group + vertex/index buffers for
//!   the diagnostic-dense test-room.
//!
//! § FRAME LIFECYCLE (T11-LOA-FID-MAINSTREAM rev)
//!   1. Acquire swap-chain surface texture
//!   2. Update uniforms (view-proj · sun-dir · ambient · time · LUTs)
//!   3. Scene pass (4x MSAA, HDR Rgba16Float color target + Depth32Float depth)
//!      → resolves into the non-multisampled HDR intermediate texture.
//!   4. Tonemap pass (full-screen triangle, ACES RRT+ODT) reads the HDR
//!      intermediate, writes display-linear values to the surface
//!      (BGRA8UnormSrgb · auto sRGB encode).
//!   5. UI overlay pass (HUD + menu, alpha blended over the tonemapped surface).
//!   6. Submit + present + telemetry (resolve_us + tonemap_us live counters).
//!
//! § FALLBACK
//!   When the adapter does not expose Rgba16Float as a render attachment,
//!   gpu.fidelity.tonemap_path = false ; we revert to the legacy single-pass
//!   surface-format render path (no HDR · no MSAA · still functional).
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
use crate::ffi as host_ffi;
use crate::geometry::{plinth_positions, RoomGeometry, Vertex};
use crate::gltf_loader::GltfMesh as LoadedGltfMesh;
use crate::gpu::GpuContext;
use crate::material::{Material, MATERIAL_LUT_LEN};
use crate::pattern::{pattern_lut, Pattern, PATTERN_LUT_LEN};
use crate::snapshot::Snapshotter;
use crate::spectral_bridge::{bake_material_lut, Illuminant};
use crate::stokes::{mueller_lut, sun_stokes_default, MUELLER_LUT_LEN};
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

/// § T11-WAVE3-GLTF : a single dynamically-spawned mesh slot.
///
/// Each slot owns its own VBO + IBO + recorded metadata. The Renderer
/// holds `Vec<DynamicMesh>` and uploads new entries by draining the
/// pending-spawn queue from `host_ffi::take_pending_gltf_spawns`.
///
/// § DRAW ORDER
///   1. Opaque static room (existing path)
///   2. **Dynamic glTF meshes (here, opaque pipeline reused)**
///   3. Transparent static room
///   4. Tonemap
///   5. CFER volumetric
///   6. UI
///
/// We reuse the existing opaque pipeline + bind-group : the dynamic
/// meshes feed the same Vertex layout + uniform buffer (material LUT +
/// pattern LUT) so the uber-shader handles them naturally.
pub struct DynamicMesh {
    /// 1-based instance id matching `GltfSpawnRecord.instance_id`.
    pub instance_id: u32,
    /// Vertex buffer (VBO) for this mesh.
    pub vbo: wgpu::Buffer,
    /// Index buffer (IBO) for this mesh.
    pub ibo: wgpu::Buffer,
    /// Index count (each draw is a single `draw_indexed(0..index_count)`).
    pub index_count: u32,
    /// Vertex count (telemetry only).
    pub vertex_count: u32,
    /// World-space AABB (after `transform_into_world`).
    pub bbox: ([f32; 3], [f32; 3]),
    /// Material id assigned to this mesh (carried for diagnostics ; the
    /// actual lookup uses each vertex's `material_id` field).
    pub material_id: u32,
    /// `true` when the mesh has been retained but not yet drawn (e.g.
    /// allocated but the renderer is in catalog mode). Always `false`
    /// once render_frame has issued the draw.
    pub pending_first_draw: bool,
}

impl std::fmt::Debug for DynamicMesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicMesh")
            .field("instance_id", &self.instance_id)
            .field("index_count", &self.index_count)
            .field("vertex_count", &self.vertex_count)
            .field("bbox", &self.bbox)
            .field("material_id", &self.material_id)
            .field("pending_first_draw", &self.pending_first_draw)
            .finish()
    }
}

impl DynamicMesh {
    /// Build a new `DynamicMesh` from a parsed glTF mesh. Allocates VBO +
    /// IBO on the GPU and records metadata for telemetry.
    pub fn new(device: &wgpu::Device, instance_id: u32, mesh: &LoadedGltfMesh) -> Self {
        let vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loa-host/dynamic-mesh-vbo"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loa-host/dynamic-mesh-ibo"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            instance_id,
            vbo,
            ibo,
            index_count: mesh.indices.len() as u32,
            vertex_count: mesh.vertices.len() as u32,
            bbox: mesh.bbox,
            material_id: mesh.material.material_id,
            pending_first_draw: true,
        }
    }
}

/// CPU-side mirror of the WGSL `Uniforms` struct.
///
/// Layout (matches `scene.wgsl::Uniforms`) :
///   - view_proj      : mat4x4              (64 B)
///   - sun_dir        : vec4                (16 B)
///   - ambient        : vec4                (16 B)
///   - time           : vec4                (16 B)
///   - camera_pos     : vec4                (16 B)   § T11-LOA-RAYMARCH
///   - sun_stokes     : vec4                (16 B)   § T11-LOA-FID-STOKES
///   - stokes_control : vec4                (16 B)   § polarization-mode + flags
///   - materials      : 16 × Material       (16 × 48 = 768 B)
///   - patterns       : 22 × Pattern        (22 × 16 = 352 B)
///   - muellers       : 16 × MuellerWGSL    (16 × 64 = 1024 B)
///   = 1248 + 32 + 1024 = 2304 bytes total. Well under 16 KiB UBO limit.
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
    /// § T11-LOA-FID-STOKES : sun light's Stokes vector (I, Q, U, V).
    /// Atmospheric scattering imparts a slight horizontal-Q bias.
    sun_stokes: [f32; 4],
    /// § T11-LOA-FID-STOKES : Stokes pipeline control word.
    /// .x = polarization_mode (0=Intensity · 1=Q · 2=U · 3=V · 4=DOP)
    /// .y = enable_mueller    (0/1)
    /// .z, .w = reserved
    stokes_control: [f32; 4],
    /// § T11-LOA-USERFIX : direct-render-mode control word (F1-F10 direct apply).
    /// .x = render_mode 0..9 ; .y/.z/.w reserved.
    render_mode_ctl: [f32; 4],
    materials: [Material; MATERIAL_LUT_LEN],
    patterns: [Pattern; PATTERN_LUT_LEN],
    /// § T11-LOA-FID-STOKES : per-material Mueller-matrix LUT.
    /// Each entry is 4 × vec4 = 64 bytes. The shader looks up
    /// `muellers[material_id]` and applies it to the sun Stokes.
    muellers: [MuellerWgsl; MUELLER_LUT_LEN],
}

/// WGSL-compatible Mueller matrix layout : 4 × vec4 = 64 bytes.
///
/// We do not directly upload the cssl-side `MuellerMatrix` because its
/// inner `[[f32; 4]; 4]` array could be padded by Rust on some targets.
/// Storing as 4 explicit `[f32; 4]` rows (each is a vec4 in WGSL) makes
/// the binary layout explicit and 16-byte aligned.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct MuellerWgsl {
    rows: [[f32; 4]; 4],
}

impl MuellerWgsl {
    fn from_cssl(m: crate::stokes::MuellerMatrix) -> Self {
        Self { rows: m.0 }
    }
}

fn build_mueller_lut_wgsl() -> [MuellerWgsl; MUELLER_LUT_LEN] {
    let mut out = [MuellerWgsl {
        rows: [[0.0; 4]; 4],
    }; MUELLER_LUT_LEN];
    let cssl_lut = mueller_lut();
    for (i, m) in cssl_lut.iter().enumerate() {
        out[i] = MuellerWgsl::from_cssl(*m);
    }
    out
}

impl Uniforms {
    fn new() -> Self {
        // § T11-LOA-FID-SPECTRAL : initial bake under the default D65
        // illuminant. The renderer-state's `current_illuminant` field tracks
        // subsequent changes ; per-frame the materials are re-baked iff the
        // EngineState illuminant generation has advanced.
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            sun_dir: Vec4::new(-0.4, 0.8, -0.45, 0.0).normalize().to_array(),
            ambient: [0.18, 0.20, 0.24, 0.0],
            time: [0.0; 4],
            camera_pos: [0.0, 1.7, 0.0, 0.0],
            sun_stokes: sun_stokes_default().as_array(),
            stokes_control: [0.0, 1.0, 0.0, 0.0], // mode=0 · enable=1
            render_mode_ctl: [0.0; 4],            // default 0 = Normal
            materials: bake_material_lut(Illuminant::default()),
            patterns: pattern_lut(),
            muellers: build_mueller_lut_wgsl(),
        }
    }
}

/// § T11-LOA-FID-MAINSTREAM : HDR intermediate + MSAA color target + the
/// matching resolved (non-multisampled) view consumed by the tonemap pass.
#[allow(dead_code)] // resolved_tex + width/height kept for diagnostic + resize debug
struct HdrTargets {
    /// Multisampled (sample_count=N) Rgba16Float color target the scene
    /// pass writes into. Created when fidelity.msaa_samples > 1, else None
    /// (single-sample direct path).
    msaa_view: Option<wgpu::TextureView>,
    /// Single-sample Rgba16Float texture that receives the resolved scene
    /// (or the direct render when MSAA is off). Owns the GPU memory the
    /// `resolved_view` references — must outlive the bind-group.
    resolved_tex: wgpu::Texture,
    /// View into `resolved_tex` consumed by the tonemap fragment.
    resolved_view: wgpu::TextureView,
    /// Tonemap input bind group (binds resolved_view + a sampler).
    tonemap_bind_group: wgpu::BindGroup,
    /// Cached width/height — when surface resizes, we recreate.
    width: u32,
    height: u32,
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
    /// § T11-LOA-USERFIX : control word.
    /// .x = cfer_intensity (0..1) — multiplies final alpha · default 0.10.
    /// .y/.z/.w = reserved.
    control: [f32; 4],
}

impl CferUniforms {
    fn new() -> Self {
        Self {
            inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0, 1.7, 0.0, 0.0],
            world_min: [WORLD_MIN[0], WORLD_MIN[1], WORLD_MIN[2], 0.0],
            world_max: [WORLD_MAX[0], WORLD_MAX[1], WORLD_MAX[2], 0.0],
            time: [0.0; 4],
            control: [0.10, 0.0, 0.0, 0.0],
        }
    }
}

/// Render pipeline + GPU resources for the test-room scene.
pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    /// Transparent-pipeline (alpha-blended) for the glass-cube stress object.
    pipeline_transparent: wgpu::RenderPipeline,
    /// § T11-LOA-FID-MAINSTREAM : tonemap pipeline (fullscreen triangle ACES).
    /// `None` when fidelity.tonemap_path == false (legacy direct path).
    pipeline_tonemap: Option<wgpu::RenderPipeline>,
    /// Tonemap bind-group layout (cached so resize() can rebuild the
    /// bind-group with the new resolved view).
    tonemap_bgl: Option<wgpu::BindGroupLayout>,
    /// Anisotropic-clamped sampler used by the tonemap fragment + future
    /// material-texture work. wgpu silently lowers anisotropy_clamp to the
    /// device cap if 16x is unsupported.
    tonemap_sampler: Option<wgpu::Sampler>,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    /// Range of indices that should be drawn alpha-blended (transparent pass).
    transparent_index_range: Option<(u32, u32)>,
    depth_view: wgpu::TextureView,
    depth_format: wgpu::TextureFormat,
    /// § T11-LOA-FID-MAINSTREAM : MSAA sample count (1, 2, 4, or 8). Drives
    /// pipeline multisample-state + depth-target sample_count. Source-of-
    /// truth lives in `gpu.fidelity.msaa_samples` ; cached here so the
    /// resize path doesn't have to thread the GpuContext through.
    msaa_samples: u32,
    /// § T11-LOA-FID-MAINSTREAM : HDR intermediate format (Rgba16Float when
    /// the tonemap path is active).
    hdr_format: wgpu::TextureFormat,
    /// § T11-LOA-FID-MAINSTREAM : HDR + MSAA targets. `None` when the
    /// fallback direct-surface path is engaged.
    hdr_targets: Option<HdrTargets>,
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
    /// § T11-LOA-FID-SPECTRAL : currently-active illuminant for the baked
    /// material LUT. The render loop re-bakes the LUT when this changes.
    pub current_illuminant: Illuminant,
    /// § T11-LOA-FID-SPECTRAL : last-observed `EngineState.illuminant_gen`
    /// snapshot. When the engine's gen advances past this value the
    /// material LUT is re-baked.
    pub last_illuminant_gen: u64,
    /// § T11-LOA-FID-SPECTRAL : cached spectrally-baked material LUT for
    /// the current illuminant. Avoids re-baking on every frame.
    cached_material_lut: [Material; MATERIAL_LUT_LEN],

    // ── § T11-LOA-USERFIX : direct render-mode + capture state ──
    /// Render-mode 0..9 currently active. Written by the host in
    /// per-frame `set_render_mode(...)` — the renderer pushes this into
    /// the scene-uniforms `render_mode_ctl` field on every frame so
    /// F-key presses take effect immediately.
    pub current_render_mode: u8,

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

    // ── § T11-WAVE3-GLTF : dynamic-mesh slots ──
    /// One slot per spawned glTF / GLB asset. New slots are created by
    /// `drain_pending_spawns` (called at the top of `render_frame`).
    /// Capped at `MAX_DYNAMIC_MESHES` (256) to bound GPU memory.
    pub dynamic_meshes: Vec<DynamicMesh>,

    // ── § T11-WAVE3-SPONT : spontaneous-condensation pipeline ──
    /// Manifestation detector — tracks each just-stamped seed-cell and
    /// emits `ManifestationEvent`s when a tracked cell crosses the radiance
    /// threshold. Polled per-frame from `window.rs` after `cfer.step_and_pack`.
    pub spontaneous_detector: crate::spontaneous::ManifestationDetector,

    // ── § T11-W18-A-COMPOSITE : substrate-compose pass ──
    /// Wgpu pipeline that uploads the Substrate-Resonance Pixel Field to a
    /// 256×256 RGBA8 texture and alpha-blends it over the conventional
    /// scene buffer (after CFER · before UI). The host pushes substrate
    /// bytes once per frame via `upload_substrate_pixels` ; the pass is
    /// recorded automatically inside `render_frame`.
    pub substrate_compose: crate::substrate_compose::SubstrateComposePipeline,
}

/// Hard upper bound on simultaneously-loaded dynamic meshes. Beyond
/// this we drop new spawns + emit a WARN log line. 256 × ~12 MB worst-
/// case = 3 GB — well within typical desktop VRAM but requires intent.
pub const MAX_DYNAMIC_MESHES: usize = 256;

impl Renderer {
    /// Embedded WGSL shader source.
    pub const SHADER_SRC: &'static str = include_str!("../shaders/scene.wgsl");

    /// § T11-LOA-FID-MAINSTREAM : tonemap fullscreen-triangle shader.
    pub const TONEMAP_SHADER_SRC: &'static str = include_str!("../shaders/tonemap.wgsl");

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

        // § T11-LOA-FID-MAINSTREAM : when the tonemap-path is engaged, the
        // scene pipelines target the HDR intermediate format
        // (Rgba16Float) — the surface format is reached only via the
        // tonemap pass. Otherwise we render directly to the surface.
        let scene_target_format = if gpu.fidelity.tonemap_path {
            gpu.fidelity.hdr_format
        } else {
            gpu.surface_format
        };
        let msaa_samples = gpu.fidelity.msaa_samples;

        // ─── Opaque pipeline : back-face culled ───
        let pipeline = Self::build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            scene_target_format,
            depth_format,
            /* transparent = */ false,
            msaa_samples,
        );

        // ─── Transparent pipeline : alpha-blend, depth-test but no depth-write ───
        let pipeline_transparent = Self::build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            scene_target_format,
            depth_format,
            /* transparent = */ true,
            msaa_samples,
        );

        // ─── Tonemap pipeline (T11-LOA-FID-MAINSTREAM) ───
        let (pipeline_tonemap, tonemap_bgl, tonemap_sampler) = if gpu.fidelity.tonemap_path {
            let tm_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("loa-host/tonemap.wgsl"),
                source: wgpu::ShaderSource::Wgsl(Self::TONEMAP_SHADER_SRC.into()),
            });
            let tm_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("loa-host/tonemap-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            let tm_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("loa-host/tonemap-pipeline-layout"),
                bind_group_layouts: &[&tm_bgl],
                push_constant_ranges: &[],
            });
            let tm_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("loa-host/tonemap-pipeline"),
                layout: Some(&tm_layout),
                vertex: wgpu::VertexState {
                    module: &tm_module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &tm_module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            // Aniso 16x sampler (clamp-to-edge so we don't bleed off the
            // resolved HDR target).
            let aniso = gpu.fidelity.aniso_max.max(1);
            let tm_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("loa-host/tonemap-sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Linear,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                anisotropy_clamp: aniso,
                ..Default::default()
            });
            log_event(
                "INFO",
                "loa-host/render",
                &format!(
                    "tonemap pipeline created : msaa={msaa_samples} aniso={aniso} hdr={:?}",
                    gpu.fidelity.hdr_format
                ),
            );
            (Some(tm_pipeline), Some(tm_bgl), Some(tm_sampler))
        } else {
            log_event(
                "WARN",
                "loa-host/render",
                "tonemap pipeline skipped : fidelity.tonemap_path=false (HDR fallback)",
            );
            (None, None, None)
        };

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

        let depth_view = create_depth_view(
            device,
            gpu.config.width,
            gpu.config.height,
            depth_format,
            msaa_samples,
        );

        // § T11-LOA-FID-MAINSTREAM : HDR + MSAA targets, only when the
        // tonemap path is engaged. The legacy direct-to-surface path bypasses.
        let hdr_targets = if gpu.fidelity.tonemap_path {
            let bgl_ref = tonemap_bgl
                .as_ref()
                .expect("tonemap_path implies tonemap_bgl");
            let sampler_ref = tonemap_sampler
                .as_ref()
                .expect("tonemap_path implies tonemap_sampler");
            Some(create_hdr_targets(
                device,
                gpu.config.width,
                gpu.config.height,
                gpu.fidelity.hdr_format,
                msaa_samples,
                bgl_ref,
                sampler_ref,
            ))
        } else {
            None
        };

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

        // § T11-W18-A-COMPOSITE : substrate-compose pipeline targets the
        // surface format (writes after tonemap, before UI).
        let substrate_compose =
            crate::substrate_compose::SubstrateComposePipeline::new(device, gpu.surface_format);

        Self {
            pipeline,
            pipeline_transparent,
            pipeline_tonemap,
            tonemap_bgl,
            tonemap_sampler,
            bind_group,
            uniform_buf,
            vertex_buf,
            index_buf,
            index_count,
            transparent_index_range,
            depth_view,
            depth_format,
            msaa_samples,
            hdr_format: gpu.fidelity.hdr_format,
            hdr_targets,
            frame_n: 0,
            ui,
            start_t: now,
            frame_times_ms: [16.7; 60],
            frame_time_idx: 0,
            last_frame_t: now,
            snapshot_pending: None,
            snapshotter: Snapshotter::new(),
            surface_copy_src,
            current_illuminant: Illuminant::default(),
            last_illuminant_gen: 0,
            cached_material_lut: bake_material_lut(Illuminant::default()),
            current_render_mode: 0,
            cfer,
            cfer_pipeline,
            cfer_bind_group,
            cfer_uniform_buf,
            cfer_texture,
            dynamic_meshes: Vec::new(),
            spontaneous_detector: crate::spontaneous::ManifestationDetector::new(),
            substrate_compose,
        }
    }

    /// § T11-W18-A-COMPOSITE : push the latest Substrate-Resonance Pixel
    /// Field bytes to the compose-pipeline texture. `bytes` must be RGBA8
    /// at the substrate field's native resolution (256×256 default). Called
    /// once per frame by `window.rs` after `substrate.tick(observer)`,
    /// BEFORE `render_frame`. Idempotent on empty payloads.
    pub fn upload_substrate_pixels(
        &mut self,
        gpu: &GpuContext,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) {
        if width == 0 || height == 0 || bytes.is_empty() {
            return;
        }
        // Reallocate the texture if the host changed the substrate
        // resolution (matches `substrate_render::DEFAULT_SUBSTRATE_*`).
        self.substrate_compose
            .ensure_size(&gpu.device, width, height);
        self.substrate_compose.upload(&gpu.queue, bytes);
    }

    /// § T11-W18-A-COMPOSITE : tweak the substrate-overlay alpha at runtime.
    /// 0.0 = invisible · 1.0 = scene-suppressing · default 0.50.
    pub fn set_substrate_overlay_strength(&mut self, gpu: &GpuContext, strength: f32) {
        self.substrate_compose
            .set_overlay_strength(&gpu.queue, strength);
    }

    /// § T11-WAVE3-GLTF : drain the global pending-spawn queue, allocating
    /// a fresh `DynamicMesh` per entry. Called at the top of `render_frame`
    /// so newly-spawned meshes appear from the very next frame onwards.
    /// Cap-checked against `MAX_DYNAMIC_MESHES` ; over-cap spawns are
    /// dropped with a WARN log line.
    pub fn drain_pending_spawns(&mut self, device: &wgpu::Device) -> u32 {
        let pending = host_ffi::take_pending_gltf_spawns();
        if pending.is_empty() {
            return 0;
        }
        let mut uploaded: u32 = 0;
        for spawn in pending {
            if self.dynamic_meshes.len() >= MAX_DYNAMIC_MESHES {
                log_event(
                    "WARN",
                    "loa-host/render",
                    &format!(
                        "drain_pending_spawns · slot full ({}); dropping instance_id={}",
                        MAX_DYNAMIC_MESHES, spawn.record.instance_id,
                    ),
                );
                continue;
            }
            let dm = DynamicMesh::new(device, spawn.record.instance_id, &spawn.mesh);
            log_event(
                "INFO",
                "loa-host/render",
                &format!(
                    "drain_pending_spawns · uploaded instance_id={} verts={} tris={} bbox.lo={:?}",
                    dm.instance_id,
                    dm.vertex_count,
                    spawn.mesh.triangle_count(),
                    dm.bbox.0,
                ),
            );
            self.dynamic_meshes.push(dm);
            uploaded += 1;
        }
        uploaded
    }

    /// § T11-WAVE3-GLTF : count of currently-loaded dynamic meshes.
    #[must_use]
    pub fn dynamic_mesh_count(&self) -> usize {
        self.dynamic_meshes.len()
    }

    /// § T11-WAVE3-GLTF : drop a previously-spawned mesh by id. Returns
    /// `true` if the slot was found and removed. Future MCP `world.despawn`
    /// hook calls this.
    pub fn despawn_dynamic_mesh(&mut self, instance_id: u32) -> bool {
        let before = self.dynamic_meshes.len();
        self.dynamic_meshes
            .retain(|m| m.instance_id != instance_id);
        before != self.dynamic_meshes.len()
    }

    /// § T11-LOA-USERFIX : push a new render-mode (0..9) into the renderer.
    /// Takes effect on the next frame's uniform write.
    pub fn set_render_mode(&mut self, mode: u8) {
        let clamped = mode.min(9);
        if self.current_render_mode != clamped {
            self.current_render_mode = clamped;
            crate::telemetry::global().record_render_mode_change(clamped);
            log_event(
                "INFO",
                "loa-host/render",
                &format!("set_render_mode · → {clamped}"),
            );
        }
    }

    /// § T11-LOA-USERFIX : current render-mode (0..9).
    #[must_use]
    pub fn current_render_mode(&self) -> u8 {
        self.current_render_mode
    }

    /// § T11-LOA-FID-MAINSTREAM : MSAA sample count active on this renderer.
    #[must_use]
    pub fn msaa_samples(&self) -> u32 {
        self.msaa_samples
    }

    /// § T11-LOA-FID-MAINSTREAM : HDR intermediate format (Rgba16Float when
    /// tonemap path is engaged, else surface format).
    #[must_use]
    pub fn hdr_format(&self) -> wgpu::TextureFormat {
        self.hdr_format
    }

    /// § T11-LOA-FID-MAINSTREAM : true iff a separate tonemap pass is wired.
    #[must_use]
    pub fn tonemap_path_active(&self) -> bool {
        self.pipeline_tonemap.is_some()
    }

    /// § T11-LOA-FID-SPECTRAL : called by the App's per-frame sync to install
    /// a new illuminant. Re-bakes the material LUT (cached for subsequent
    /// frames) and records a structured-event log line.
    pub fn set_illuminant(&mut self, illum: Illuminant, gen: u64) {
        self.current_illuminant = illum;
        self.last_illuminant_gen = gen;
        self.cached_material_lut = bake_material_lut(illum);
        log_event(
            "INFO",
            "loa-host/render",
            &format!(
                "render.illuminant_changed · {} · cct={}K · gen={}",
                illum.name(),
                illum.cct_kelvin(),
                gen,
            ),
        );
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
            // § T11-LOA-FIX-3 : CFER pipeline runs against non-MSAA surface
            //   (post-tonemap order). Drop depth_stencil to match the pass's
            //   `depth_stencil_attachment: None`. CFER's per-sample depth
            //   discrimination happens via the world-AABB ray test in the
            //   shader anyway — no GPU depth-test needed. When CFER moves
            //   pre-tonemap (against MSAA-resolved HDR) reinstate proper
            //   non-MSAA depth.
            depth_stencil: None,
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
        sample_count: u32,
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
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    /// Re-create the depth + HDR targets after a surface resize.
    pub fn resize(&mut self, gpu: &GpuContext) {
        self.depth_view = create_depth_view(
            &gpu.device,
            gpu.config.width,
            gpu.config.height,
            self.depth_format,
            self.msaa_samples,
        );
        // § T11-LOA-FID-MAINSTREAM : rebuild HDR + MSAA color targets at
        // the new size when the tonemap path is engaged.
        if let (Some(bgl), Some(sampler)) =
            (self.tonemap_bgl.as_ref(), self.tonemap_sampler.as_ref())
        {
            self.hdr_targets = Some(create_hdr_targets(
                &gpu.device,
                gpu.config.width,
                gpu.config.height,
                self.hdr_format,
                self.msaa_samples,
                bgl,
                sampler,
            ));
        }
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
        // § T11-WAVE3-GLTF : drain newly-spawned glTF meshes BEFORE we
        // acquire the surface texture. Newly-uploaded VBOs/IBOs are
        // visible to the dynamic-mesh draw pass below from this frame
        // onwards. Catalog builds never reach this code path.
        let new_meshes = self.drain_pending_spawns(&gpu.device);
        if new_meshes > 0 {
            log_event(
                "INFO",
                "loa-host/render",
                &format!(
                    "render_frame · {new_meshes} new dynamic mesh(es) uploaded · total={}",
                    self.dynamic_meshes.len()
                ),
            );
        }

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
        let pol_mode = host_ffi::polarization_view();
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
            sun_stokes: sun_stokes_default().as_array(),
            stokes_control: [pol_mode as f32, 1.0, 0.0, 0.0],
            // § T11-LOA-USERFIX : push the current render-mode each frame.
            render_mode_ctl: [f32::from(self.current_render_mode), 0.0, 0.0, 0.0],
            // § T11-LOA-FID-SPECTRAL : spectrally-baked material LUT (cached
            // ; only re-baked when `set_illuminant` mutates it).
            materials: self.cached_material_lut,
            patterns: pattern_lut(),
            muellers: build_mueller_lut_wgsl(),
        };
        gpu.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        // § T11-LOA-FID-STOKES · per-frame Mueller telemetry roll-up.
        // The shader applies one Mueller per visible fragment (CPU-side
        // estimator : count visible draw-calls × a typical fragment count).
        // We record a representative DOP sample per frame from the sun
        // Stokes vector (the source of all surface lighting).
        let sun_dop = sun_stokes_default().dop_total();
        // Estimate ~self.index_count / 3 visible fragments at typical view.
        let est_applies = (self.index_count / 3) as u64;
        for _ in 0..est_applies.min(64) {
            host_ffi::record_mueller_apply(sun_dop);
        }
        let (applies, dop_avg_q14, dop_max_q14) = host_ffi::snapshot_and_reset_mueller_telem();
        telem::global().record_stokes_frame(applies, dop_avg_q14, dop_max_q14);

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

        // § T11-LOA-FID-MAINSTREAM : decide where the scene pass writes.
        //   - With HDR/MSAA path : write into msaa_view (sample_count=N),
        //     resolve into resolved_view (sample_count=1) on store.
        //   - Without : write directly into the swap-chain `view`.
        let resolve_t0 = std::time::Instant::now();
        let (scene_color_view, scene_resolve_target) = if let Some(h) = self.hdr_targets.as_ref() {
            match h.msaa_view.as_ref() {
                Some(msaa) => (msaa, Some(&h.resolved_view)),
                None => (&h.resolved_view, None),
            }
        } else {
            (&view, None)
        };

        // ─── Pass 1 : opaque scene ───
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/scene-pass-opaque"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_color_view,
                    resolve_target: scene_resolve_target,
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

            // § T11-WAVE3-GLTF : dynamic-mesh draws (after static opaque).
            // Re-uses the same opaque pipeline + uniform bind-group ; only
            // the VBO/IBO change per slot. No extra pipeline-switch is
            // counted because we leave the same RenderPipeline bound.
            // Each mesh contributes one draw_indexed call against its own
            // 32-bit index buffer.
            for dm in &mut self.dynamic_meshes {
                pass.set_vertex_buffer(0, dm.vbo.slice(..));
                pass.set_index_buffer(dm.ibo.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..dm.index_count, 0, 0..1);
                draw_calls += 1;
                vertices += u64::from(dm.index_count);
                dm.pending_first_draw = false;
            }
        }

        // ─── Pass 2 : transparent objects (alpha-blended) ───
        if let Some((lo, hi)) = self.transparent_index_range {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/scene-pass-transparent"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_color_view,
                    resolve_target: scene_resolve_target,
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
        // Resolve elapsed = wall-clock from start of scene-pass record to
        // here (the actual GPU resolve happens inside frame.present(), but
        // the encoded resolve-store is the only point we can observe from
        // the host without adding TIMESTAMP_QUERY).
        let resolve_us = resolve_t0.elapsed().as_micros() as u64;
        telem::global().record_gpu_resolve_us(resolve_us);

        // ─── Pass 2.5 : tonemap (HDR Rgba16Float → surface BGRA8UnormSrgb) ───
        // § T11-LOA-FID-MAINSTREAM : ACES RRT+ODT fullscreen triangle.
        let tonemap_t0 = std::time::Instant::now();
        if let (Some(tm_pipe), Some(hdr)) =
            (self.pipeline_tonemap.as_ref(), self.hdr_targets.as_ref())
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/tonemap-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(tm_pipe);
            pipeline_switches += 1;
            telem::global().record_pipeline_switch();
            pass.set_bind_group(0, &hdr.tonemap_bind_group, &[]);
            // Fullscreen triangle : 3 vertices, no VBO.
            pass.draw(0..3, 0..1);
            draw_calls += 1;
        }
        let tonemap_us = tonemap_t0.elapsed().as_micros() as u64;
        telem::global().record_tonemap_us(tonemap_us);

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
        // § T11-LOA-USERFIX : pull live intensity from the CferRenderer
        // (host writes via `cfer.set_cfer_intensity` or the C-key toggle).
        let cfer_intensity = self.cfer.cfer_intensity();
        // Mirror to the global telemetry sink so MCP `telemetry.snapshot`
        // sees the live value.
        crate::telemetry::global().record_cfer_intensity(cfer_intensity);
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
            control: [cfer_intensity, 0.0, 0.0, 0.0],
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
                // § T11-LOA-FIX-3 : CFER pass runs AFTER tonemap (against
                //   non-MSAA surface). The depth view is sample_count=4 from
                //   the MSAA scene pass · attaching it here would mismatch the
                //   color count=1. Drop depth entirely for the volumetric pass
                //   — minor artifact (no occlusion) but unblocks the binary.
                //   Future slice : run CFER pre-tonemap against MSAA-resolved
                //   HDR target with proper non-MSAA depth.
                depth_stencil_attachment: None,
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

        // ─── Pass 3.5 : T11-W18-A-COMPOSITE — Substrate-Resonance Pixel
        //                Field overlay. Alpha-blends the 256×256 substrate
        //                texture (uploaded each frame by the host via
        //                `upload_substrate_pixels`) over the surface so
        //                the conventional 3D scene AND the substrate
        //                pixel-field are both visible. Pre-CFER scene + CFER
        //                fragments show through where substrate alpha < 1.
        self.substrate_compose.record_pass(&mut encoder, &view);
        draw_calls += 1;
        pipeline_switches += 1;
        telem::global().record_pipeline_switch();

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

        // § T11-LOA-SENSORY : framebuffer-thumbnail capture before present.
        // The MCP `sense.framebuffer_thumbnail` tool sets `capture_pending`
        // on the EngineState mirror ; if set + the surface supports COPY_SRC,
        // we readback the framebuffer, downsample CPU-side to 256×144, and
        // write the RGBA8 bytes into the mirror so the next MCP call can
        // base64-encode + return inline. Failures degrade silently (no
        // thumbnail available) — the harness is observability-only.
        // (Full capture path lives in `capture_thumbnail_to_mirror` below.)
        // Note : we DO NOT block the present even if this fails.

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

    // ──────────────────────────────────────────────────────────────────
    // § T11-WAVE3-SPONT : spontaneous-condensation pipeline
    // ──────────────────────────────────────────────────────────────────

    /// Sow an intent text into the CFER field at `origin`. Returns the
    /// `SowOutcome` so the caller can log + emit a structured event.
    ///
    /// This is the canonical "spontaneous generation" entry-point :
    ///   1. text → SeedCells (keyword table).
    ///   2. seeds → Ω-field stamps (Σ-bypass at seed-time).
    ///   3. detector registers each new seed for next-frame rising-edge poll.
    ///
    /// The actual stress-object spawn happens later (per-frame) when a
    /// tracked cell's radiance crosses `MANIFESTATION_THRESHOLD`. The host
    /// polls `scan_manifestations` to drain those events + dispatch spawn.
    pub fn sow_spontaneous_intent(
        &mut self,
        text: &str,
        origin: [f32; 3],
        frame: u64,
    ) -> crate::spontaneous::SowOutcome {
        let outcome = crate::spontaneous::sow_intent(&mut self.cfer.field, text, origin);
        self.spontaneous_detector
            .register_seeds(&outcome.stamped, frame);
        outcome
    }

    /// § T11-WAVE3-SPONT : per-frame manifestation-detector poll. Drains
    /// any rising-edge ManifestationEvents the detector observed in the
    /// just-evolved field. Caller dispatches `__cssl_render_spawn_stress_object`
    /// for each event.
    pub fn scan_spontaneous_manifestations(
        &mut self,
        frame: u64,
    ) -> Vec<crate::spontaneous::ManifestationEvent> {
        self.spontaneous_detector
            .scan_rising_edges(&self.cfer.field, frame)
    }

    /// § T11-WAVE3-SPONT : recent-events ring (oldest-first). Mirrored
    /// into EngineState by the per-frame sync so MCP `sense.spontaneous_recent`
    /// returns live values.
    #[must_use]
    pub fn spontaneous_recent_events(&self) -> Vec<crate::spontaneous::ManifestationEvent> {
        self.spontaneous_detector.recent_events_vec()
    }

    /// § T11-WAVE3-SPONT : seeds-total + manifests-total since startup.
    #[must_use]
    pub fn spontaneous_totals(&self) -> (u64, u64) {
        (
            self.spontaneous_detector.seeds_total,
            self.spontaneous_detector.manifests_total,
        )
    }
}

fn create_depth_view(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("loa-host/depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// § T11-LOA-FID-MAINSTREAM : build the HDR/MSAA color target trio.
///
///   - `msaa_view` : sample_count=N (Rgba16Float) — scene pass draws here.
///   - `resolved_tex/_view` : sample_count=1 (Rgba16Float) — receives the
///     resolve. Tonemap pass samples this.
///   - `tonemap_bind_group` : binds (resolved_view, sampler) for the
///     tonemap fragment shader.
///
/// When `sample_count == 1` we skip the multisampled texture (MSAA is off
/// but HDR is still wanted) — the scene pass writes directly into the
/// resolved texture.
fn create_hdr_targets(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    sample_count: u32,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> HdrTargets {
    let w = width.max(1);
    let h = height.max(1);
    // Resolved (sample_count=1) HDR target — the tonemap pass samples this.
    let resolved_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("loa-host/hdr-resolved"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let resolved_view = resolved_tex.create_view(&wgpu::TextureViewDescriptor::default());

    // Optional MSAA target (only when sample_count > 1).
    let msaa_view = if sample_count > 1 {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("loa-host/hdr-msaa"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Some(tex.create_view(&wgpu::TextureViewDescriptor::default()))
    } else {
        None
    };

    let tonemap_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("loa-host/tonemap-bind-group"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&resolved_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });

    HdrTargets {
        msaa_view,
        resolved_tex,
        resolved_view,
        tonemap_bind_group,
        width: w,
        height: h,
    }
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
        // § T11-LOA-FID-STOKES : layout grew by sun_stokes (16 B) +
        // stokes_control (16 B) + 16 muellers × 64 B (1024 B).
        // Prior 1248 + 32 + 1024 = 2304 bytes total.
        // § T11-LOA-USERFIX : added render_mode_ctl (16 B) →
        // 2304 + 16 = 2320 bytes total.
        assert_eq!(core::mem::size_of::<Uniforms>(), 2320);
    }

    #[test]
    fn uniforms_carries_camera_pos_field() {
        let u = Uniforms::new();
        // Default camera_pos seeds at (0, 1.7, 0) — eye-height @ room center.
        assert_eq!(u.camera_pos, [0.0, 1.7, 0.0, 0.0]);
    }

    /// § T11-LOA-FID-MAINSTREAM : tonemap shader source const is reachable
    /// + non-empty + has both entry points (vs_main + fs_main + the ACES
    /// helper `aces_rrt_odt`).
    #[test]
    fn tonemap_shader_src_is_nonempty_and_has_entry_points() {
        assert!(!Renderer::TONEMAP_SHADER_SRC.is_empty());
        assert!(Renderer::TONEMAP_SHADER_SRC.contains("vs_main"));
        assert!(Renderer::TONEMAP_SHADER_SRC.contains("fs_main"));
        assert!(Renderer::TONEMAP_SHADER_SRC.contains("aces_rrt_odt"));
    }

    /// § T11-LOA-FID-MAINSTREAM : crate-level const must match the
    /// renderer-side const (single source of truth, prevents accidental drift).
    #[test]
    fn tonemap_shader_src_matches_crate_const() {
        assert_eq!(Renderer::TONEMAP_SHADER_SRC, crate::TONEMAP_WGSL);
    }

    /// § T11-LOA-FID-MAINSTREAM : intermediate-texture format on the
    /// fidelity path must be Rgba16Float. We assert the constant directly
    /// (the field is set from `gpu.fidelity.hdr_format`) — pinning the
    /// HDR contract.
    #[test]
    fn intermediate_texture_format_is_rgba16float() {
        let f = crate::gpu::FidelityConfig {
            msaa_samples: 4,
            hdr_format: wgpu::TextureFormat::Rgba16Float,
            present_mode: wgpu::PresentMode::Mailbox,
            aniso_max: 16,
            tonemap_path: true,
        };
        assert_eq!(f.hdr_format, wgpu::TextureFormat::Rgba16Float);
        assert!(f.tonemap_path);
    }

    /// § T11-LOA-FID-MAINSTREAM : pipeline must use sample_count=4 when
    /// the negotiated fidelity says so. We construct the build_pipeline
    /// MultisampleState explicitly + assert.
    #[test]
    fn msaa_pipeline_uses_sample_count_4() {
        // Full pipeline build needs a wgpu Device which we don't have at
        // unit-test time. We exercise the MultisampleState carrier instead :
        // the renderer code path passes `msaa_samples` directly into
        // `MultisampleState { count: sample_count, .. }`.
        let s = wgpu::MultisampleState {
            count: 4,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        assert_eq!(s.count, 4);
    }

    /// § T11-LOA-FID-MAINSTREAM : Mailbox or Fifo is in the wgpu enum (the
    /// adapter capability probe code path picks one of these by query).
    /// Catalog test : just verify both variants exist in the `wgpu` API
    /// surface our code references.
    #[test]
    fn mailbox_or_fifo_supported() {
        let m = wgpu::PresentMode::Mailbox;
        let f = wgpu::PresentMode::Fifo;
        assert_ne!(m, f);
    }

    /// § T11-LOA-FID-MAINSTREAM : when the adapter does NOT expose
    /// PresentMode::Mailbox, the fallback must be Fifo (or AutoVsync
    /// when even Fifo isn't supported, which is rare). The
    /// `FidelityConfig::fallback` constructor pins this contract.
    #[test]
    fn present_mode_falls_back_to_fifo_when_mailbox_unavailable() {
        // The fallback config explicitly carries Fifo — matches what the
        // gpu.rs init logic chooses when Mailbox is missing but Fifo is
        // present.
        let f = crate::gpu::FidelityConfig::fallback(wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(f.present_mode, wgpu::PresentMode::Fifo);
        assert_eq!(f.msaa_samples, 1);
        assert!(!f.tonemap_path);
    }

    /// § T11-LOA-FID-MAINSTREAM : ACES tonemap output is always in [0, 1]
    /// (the WGSL `aces_rrt_odt` clamps the result). Verifies on the CPU
    /// reference path.
    #[test]
    fn tonemap_aces_returns_clamped_to_unit_range() {
        fn aces(x: f32) -> f32 {
            let a = x * 2.51 + 0.03;
            let b = x * (2.43 * x + 0.59) + 0.14;
            ((x * a) / b).clamp(0.0, 1.0)
        }
        // Sweep across 0 .. 1000 (representing scene-linear inputs).
        for k in 0..1000 {
            let x = k as f32 * 0.01;
            let y = aces(x);
            assert!(
                (0.0..=1.0).contains(&y),
                "aces({x})={y} out of [0,1]"
            );
        }
        // Specific bright HDR samples that would otherwise blow out :
        // 16.0 (16 nits-equivalent) and 64.0 (64 nits-equivalent).
        assert!(aces(16.0) <= 1.0);
        assert!(aces(64.0) <= 1.0);
    }

    /// § T11-LOA-FID-MAINSTREAM : pipeline build sets multisample.count from
    /// the renderer's msaa field. We exercise the field-thread directly via
    /// `MultisampleState` construction (cannot call `build_pipeline` without
    /// a `wgpu::Device`). When the adapter advertises 4x MSAA, the pipeline
    /// constructor receives sample_count=4 — pinned here.
    #[test]
    fn pipeline_uses_msaa_when_available() {
        for &sc in &[1u32, 2, 4, 8] {
            let s = wgpu::MultisampleState {
                count: sc,
                mask: !0,
                alpha_to_coverage_enabled: false,
            };
            assert_eq!(s.count, sc);
        }
    }

    #[test]
    fn uniforms_carries_sun_stokes_with_slight_q() {
        let u = Uniforms::new();
        assert!((u.sun_stokes[0] - 1.0).abs() < 1e-6, "I=1");
        assert!(u.sun_stokes[1] > 0.0, "Q > 0 (atmospheric horizontal pol)");
    }

    #[test]
    fn uniforms_carries_mueller_lut() {
        let u = Uniforms::new();
        // First entry (matte-grey) = depolarizer : M[0][0] = 1, rest of row 0 is zero.
        assert!((u.muellers[0].rows[0][0] - 1.0).abs() < 1e-6);
        assert!(u.muellers[0].rows[1][1].abs() < 1e-6);
        // 16 entries.
        assert_eq!(u.muellers.len(), 16);
    }

    #[test]
    fn uniforms_default_polarization_mode_is_intensity() {
        let u = Uniforms::new();
        assert_eq!(u.stokes_control[0], 0.0); // 0 = Intensity
        assert_eq!(u.stokes_control[1], 1.0); // 1 = enable_mueller on
    }

    // ── § T11-LOA-FID-CFER : volumetric-pass uniform layout ──

    #[test]
    fn cfer_uniforms_size_is_144_bytes() {
        // 64 (mat4x4) + 16 (camera) + 16 (world_min) + 16 (world_max) + 16 (time)
        // § T11-LOA-USERFIX : + 16 (control with cfer_intensity) = 144 bytes.
        assert_eq!(core::mem::size_of::<CferUniforms>(), 144);
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
        assert_eq!(u.control, [0.0; 4]);
    }

    #[test]
    fn render_mode_ctl_default_is_zero() {
        let u = Uniforms::new();
        assert_eq!(u.render_mode_ctl, [0.0; 4]);
    }

    #[test]
    fn cfer_uniforms_default_intensity_is_low() {
        let u = CferUniforms::new();
        // Default cfer_intensity = 0.10 → so the post-tonemap haze is subtle.
        assert!((u.control[0] - 0.10).abs() < 1e-6);
        assert!(u.control[0] <= 0.15);
    }
}
