//! § render — wgpu render pipeline + per-frame draw call.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-1 (W-LOA-host-render) : owns the WGSL shader module +
//! render pipeline + uniform buffer + bind group + vertex/index buffers
//! for the test-room mesh. Each frame :
//!   1. Acquire swapchain frame
//!   2. Update uniform buffer with current view-proj matrix
//!   3. Encode a single render pass : clear color = sky-blue, draw mesh
//!   4. Submit + present
//!   5. Emit telemetry RENDER_FRAME
//!
//! § COLOR-SPACE
//!   Surface format prefers sRGB; we write LINEAR colors and let the surface
//!   convert. Clear color is written in linear-space, so 0x6cb4ee sky-blue
//!   becomes ~(0.171, 0.453, 0.798) after sRGB→linear approximation.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::unnested_or_patterns)] // wgpu::SurfaceError variants kept on separate arms for log readability
#![allow(clippy::float_cmp)] // POD bit-pattern tests use exact f32 equality

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use cssl_rt::loa_startup::log_event;

use crate::camera::Camera;
use crate::geometry::{RoomGeometry, Vertex};
use crate::gpu::GpuContext;
use crate::ui_overlay::{HudContext, MenuState, UiOverlay};

/// CPU-side mirror of the WGSL `Uniforms` struct. 144 bytes (16-byte aligned).
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    ambient: [f32; 4],
}

impl Uniforms {
    fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            // Direction TOWARD the sun (down-forward sun light per brief).
            sun_dir: Vec4::new(-0.4, 0.8, -0.45, 0.0).normalize().to_array(),
            // Ambient fill (cool indirect tone).
            ambient: [0.18, 0.20, 0.24, 0.0],
        }
    }
}

/// Render pipeline + GPU resources for the test-room scene.
pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    depth_view: wgpu::TextureView,
    depth_format: wgpu::TextureFormat,
    /// Frame counter for telemetry throttling (log every Nth frame).
    frame_n: u64,
    /// UI overlay (HUD + menu). Pass-2 after the scene draw.
    ui: UiOverlay,
}

impl Renderer {
    /// Embedded WGSL shader source.
    pub const SHADER_SRC: &'static str = include_str!("../shaders/scene.wgsl");

    /// Construct the renderer for the given GPU context. Loads the test-room
    /// geometry, uploads vertex/index buffers, and compiles the pipeline.
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
            "shader module created (scene.wgsl)",
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("loa-host/scene-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
                // § T11-LOA-FIX-CULL : disabled while we audit per-face winding
                //   against the post-strafe-fix axis convention (forward=-Z at
                //   yaw=0). Apocky reported "seeing through/into objects" which
                //   is the classic culling-flipped symptom. Disabled until
                //   the rich-render-system rewrite normalises winding.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Uniform buffer (144 bytes, mapped at creation for first-frame value).
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

        // Test-room geometry uploaded once.
        let geom = RoomGeometry::test_room();
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

        log_event(
            "INFO",
            "loa-host/render",
            &format!(
                "geometry uploaded : {} verts, {} indices, {} plinths",
                geom.vertices.len(),
                geom.indices.len(),
                geom.plinth_count
            ),
        );

        let depth_view =
            create_depth_view(device, gpu.config.width, gpu.config.height, depth_format);

        let ui = UiOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format);

        Self {
            pipeline,
            bind_group,
            uniform_buf,
            vertex_buf,
            index_buf,
            index_count,
            depth_view,
            depth_format,
            frame_n: 0,
            ui,
        }
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
    ) -> Result<(), wgpu::SurfaceError> {
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

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Update uniforms with current view-proj.
        let aspect = gpu.aspect();
        let uniforms = Uniforms {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            sun_dir: Vec4::new(-0.4, 0.8, -0.45, 0.0).normalize().to_array(),
            ambient: [0.18, 0.20, 0.24, 0.0],
        };
        gpu.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("loa-host/frame-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Sky-blue 0x6cb4ee in linear-space.
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
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        // § UI overlay : second render pass over the same surface texture.
        // Build the per-frame vertex stream, then encode an alpha-blended
        // pass that LOADs (preserves) the scene color attachment.
        self.ui.prepare_frame(
            &gpu.device,
            &gpu.queue,
            gpu.config.width,
            gpu.config.height,
            hud,
            menu,
        );
        self.ui.encode_pass(&mut encoder, &view);

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        // Telemetry : log first frame + every 600th frame after (roughly 10s @ 60Hz).
        if self.frame_n == 0 {
            log_event("INFO", "loa-host/render", "first-frame-rendered");
        } else if self.frame_n % 600 == 0 {
            log_event(
                "INFO",
                "loa-host/render",
                &format!("RENDER_FRAME · n={}", self.frame_n),
            );
        }
        self.frame_n += 1;
        Ok(())
    }

    /// Total number of frames presented. Used by tests + telemetry.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_n
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
    }

    #[test]
    fn shader_src_is_nonempty() {
        assert!(!Renderer::SHADER_SRC.is_empty());
        assert!(Renderer::SHADER_SRC.contains("vs_main"));
        assert!(Renderer::SHADER_SRC.contains("fs_main"));
    }

    /// The catalog-level test in `lib.rs` runs the same naga validate over
    /// `SCENE_WGSL` (which is the same `include_str!` source). This module
    /// only re-exposes the constant via `Renderer::SHADER_SRC` for test
    /// compatibility; we don't duplicate the naga validation here.
    #[test]
    fn renderer_shader_src_matches_crate_const() {
        assert_eq!(Renderer::SHADER_SRC, crate::SCENE_WGSL);
    }
}
