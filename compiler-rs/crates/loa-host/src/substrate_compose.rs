//! § substrate_compose — wgpu compositing pass for the Substrate-Resonance
//!                       Pixel Field.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-A-COMPOSITE · make-pixels-VISIBLE-on-screen (W-T11-W18-A-REDUX)
//!
//! § APOCKY-DIRECTIVE
//!   "Substrate-Resonance Pixel Field runs per-frame in LoA.exe (W17-WIRED)
//!    but the PixelField bytes are NOT YET uploaded to a wgpu texture or
//!    composited onto screen. Make it VISIBLE."
//!
//! § ROLE
//!   Owns the wgpu resources for the substrate-compose pass :
//!     - 256×256 RGBA8 texture + linear-clamp sampler
//!     - small uniform buffer (overlay-strength control)
//!     - bind-group + pipeline targeting the surface format with
//!       SrcAlpha/OneMinusSrcAlpha blend (standard premultiplied-aware
//!       compositing) so the substrate pixels appear as a translucent
//!       overlay over the conventional 3D scene.
//!
//! § FRAME LIFECYCLE
//!   1. host calls `upload(queue, bytes)` once per frame with the current
//!      `PixelField::as_bytes_owned()` payload (256 × 256 × 4 = 256 KB).
//!   2. host calls `record_pass(encoder, view)` AFTER the conventional
//!      scene + CFER + tonemap passes and BEFORE the UI overlay. The
//!      substrate pixels alpha-blend over the scene buffer.
//!
//! § PRIME-DIRECTIVE attestation (PD)
//!   No hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::module_name_repetitions)]

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use cssl_rt::loa_startup::log_event;

/// Default substrate-pixel-field width (matches `substrate_render::DEFAULT_SUBSTRATE_W`).
pub const COMPOSE_TEX_W: u32 = 256;

/// Default substrate-pixel-field height (matches `substrate_render::DEFAULT_SUBSTRATE_H`).
pub const COMPOSE_TEX_H: u32 = 256;

/// Default overlay strength : 50 %. The shader multiplies sampled alpha
/// by this scalar so the conventional scene shows through equally.
pub const DEFAULT_OVERLAY_STRENGTH: f32 = 0.50;

/// Default AMOLED black-threshold (≈ 10/255 = 0.039). Sub-threshold alpha
/// emits pure (0,0,0,0) so AMOLED/OLED/HDR-pitch-black panels keep pixels
/// fully off · maximum contrast + power savings + zero black-leakage.
pub const DEFAULT_AMOLED_BLACK_THRESHOLD: f32 = 0.04;

/// Default contrast S-curve strength (0 = linear · 1 = strong S-curve).
/// 0.35 tuned for AMOLED-pop : substrate-pixels stand out against pure
/// black void without crushing legitimate mid-tones.
pub const DEFAULT_AMOLED_CONTRAST: f32 = 0.35;

/// Display-profile discriminant (substrate-canon · matches `compose_ctl.w`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayProfile {
    /// AMOLED · pitch-black emit-nothing · highest contrast (DEFAULT)
    Amoled = 0,
    /// OLED · near-pitch-black · slight gray-lift OK
    Oled = 1,
    /// IPS LCD · backlit · blacks are gray · lift-blacks-allowed
    IpsLcd = 2,
    /// VA LCD · good contrast · between IPS + OLED
    VaLcd = 3,
    /// HDR external · 1000+ nit peak · wider gamut
    HdrExt = 4,
}

impl DisplayProfile {
    /// Per-profile (black_threshold, contrast) tuned defaults.
    pub fn defaults(self) -> (f32, f32) {
        match self {
            Self::Amoled => (DEFAULT_AMOLED_BLACK_THRESHOLD, DEFAULT_AMOLED_CONTRAST),
            Self::Oled => (0.03, 0.30),
            Self::IpsLcd => (0.0, 0.15), // do NOT crush blacks · IPS already gray
            Self::VaLcd => (0.02, 0.25),
            Self::HdrExt => (0.05, 0.40), // can afford strong S-curve
        }
    }
}

/// Embedded WGSL source for the compose shader.
pub const SUBSTRATE_COMPOSE_WGSL: &str = include_str!("../shaders/substrate_compose.wgsl");

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ComposeUniforms {
    /// .x = overlay strength · .yzw = reserved.
    compose_ctl: [f32; 4],
}

/// Owns the wgpu resources for the substrate-compose pass.
pub struct SubstrateComposePipeline {
    /// 256×256 RGBA8 texture re-uploaded each frame from the host
    /// substrate `PixelField::as_bytes_owned()` payload.
    pub texture: wgpu::Texture,
    /// View over `texture` referenced by the bind-group.
    pub view: wgpu::TextureView,
    /// Linear-filter clamp-to-edge sampler so the 256×256 substrate field
    /// appears smoothly upsampled across any window resolution.
    pub sampler: wgpu::Sampler,
    /// Uniform buffer : 16-byte ComposeUniforms.
    pub uniform_buf: wgpu::Buffer,
    /// Bind-group layout cached so resize() can rebuild the bind-group.
    pub bgl: wgpu::BindGroupLayout,
    /// Bind-group used by the compose pass (texture + sampler + uniform).
    pub bind_group: wgpu::BindGroup,
    /// Render pipeline (fullscreen triangle · alpha-blend over surface).
    pub pipeline: wgpu::RenderPipeline,
    /// Cached overlay strength so `set_overlay_strength` only re-uploads
    /// the uniform when it actually changes.
    overlay_strength: f32,
    /// AMOLED-aware black-threshold · sub-threshold alpha emits true (0,0,0,0).
    black_threshold: f32,
    /// Contrast S-curve strength (0 = linear · 1 = strong).
    contrast: f32,
    /// Cached display-profile-id (matches compose_ctl.w in shader).
    display_profile: DisplayProfile,
    /// Cached texture dimensions. Re-allocated by `ensure_size` if the
    /// host ever resizes the substrate pixel-field at runtime.
    width: u32,
    height: u32,
    /// Surface format remembered so `ensure_size` doesn't re-fetch it.
    target_format: wgpu::TextureFormat,
}

impl SubstrateComposePipeline {
    /// Construct the compose pipeline against the supplied surface format.
    /// Allocates a 256×256 RGBA8 texture pre-sized for the default substrate
    /// pixel-field. The texture is uploaded each frame via `upload`.
    #[must_use]
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("loa-host/substrate_compose.wgsl"),
            source: wgpu::ShaderSource::Wgsl(SUBSTRATE_COMPOSE_WGSL.into()),
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("loa-host/substrate-compose-texture"),
            size: wgpu::Extent3d {
                width: COMPOSE_TEX_W,
                height: COMPOSE_TEX_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("loa-host/substrate-compose-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let uniforms = ComposeUniforms {
            compose_ctl: [
                DEFAULT_OVERLAY_STRENGTH,
                DEFAULT_AMOLED_BLACK_THRESHOLD,
                DEFAULT_AMOLED_CONTRAST,
                DisplayProfile::Amoled as u32 as f32,
            ],
        };
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loa-host/substrate-compose-uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loa-host/substrate-compose-bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loa-host/substrate-compose-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buf.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("loa-host/substrate-compose-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("loa-host/substrate-compose-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    // Standard alpha-blend (src.rgb * src.a + dst.rgb * (1-src.a)).
                    // The shader pre-scales alpha by overlay-strength so the
                    // result is "scene faint-with-substrate-overlay" by default.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
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

        log_event(
            "INFO",
            "loa-host/substrate-compose",
            &format!(
                "init · {COMPOSE_TEX_W}×{COMPOSE_TEX_H} RGBA8 texture · target={target_format:?} \
                 · overlay={DEFAULT_OVERLAY_STRENGTH}"
            ),
        );

        Self {
            texture,
            view,
            sampler,
            uniform_buf,
            bgl,
            bind_group,
            pipeline,
            overlay_strength: DEFAULT_OVERLAY_STRENGTH,
            black_threshold: DEFAULT_AMOLED_BLACK_THRESHOLD,
            contrast: DEFAULT_AMOLED_CONTRAST,
            display_profile: DisplayProfile::Amoled,
            width: COMPOSE_TEX_W,
            height: COMPOSE_TEX_H,
            target_format,
        }
    }

    /// Re-allocate the texture if the host changed the substrate pixel-field
    /// resolution. Idempotent when dimensions match.
    pub fn ensure_size(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if w == self.width && h == self.height {
            return;
        }
        self.texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("loa-host/substrate-compose-texture (resized)"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loa-host/substrate-compose-bg (resized)"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform_buf.as_entire_binding(),
                },
            ],
        });
        self.width = w;
        self.height = h;
        log_event(
            "INFO",
            "loa-host/substrate-compose",
            &format!("texture-resized · {w}×{h}"),
        );
    }

    /// Upload the substrate pixel-field bytes to the GPU texture. `bytes`
    /// must be `width * height * 4` long (RGBA8). Short payloads are
    /// truncated to the texture extent ; over-long payloads are clipped.
    pub fn upload(&self, queue: &wgpu::Queue, bytes: &[u8]) {
        let needed = (self.width as usize) * (self.height as usize) * 4;
        let n = bytes.len().min(needed);
        if n == 0 {
            return;
        }
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes[..n],
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Tweak overlay strength at runtime (0.0 = invisible, 1.0 = scene-
    /// suppressing). Re-uploads only on actual change.
    pub fn set_overlay_strength(&mut self, queue: &wgpu::Queue, strength: f32) {
        let s = strength.clamp(0.0, 1.0);
        if (s - self.overlay_strength).abs() < f32::EPSILON {
            return;
        }
        self.overlay_strength = s;
        self.write_uniforms(queue);
        log_event(
            "DEBUG",
            "loa-host/substrate-compose",
            &format!("overlay-strength · {s:.3}"),
        );
    }

    /// Adopt a `DisplayProfile`. Updates `black_threshold` + `contrast` to
    /// the profile's tuned defaults · re-uploads uniform on change. Call
    /// once at startup after detecting the panel + on monitor-change.
    pub fn set_display_profile(&mut self, queue: &wgpu::Queue, profile: DisplayProfile) {
        if self.display_profile == profile {
            return;
        }
        let (bt, ct) = profile.defaults();
        self.display_profile = profile;
        self.black_threshold = bt;
        self.contrast = ct;
        self.write_uniforms(queue);
        log_event(
            "INFO",
            "loa-host/substrate-compose",
            &format!(
                "display-profile · {profile:?} · black_thresh={bt:.3} · contrast={ct:.3}"
            ),
        );
    }

    /// Tweak black-threshold (AMOLED-aware true-black gate).
    pub fn set_black_threshold(&mut self, queue: &wgpu::Queue, threshold: f32) {
        let t = threshold.clamp(0.0, 1.0);
        if (t - self.black_threshold).abs() < f32::EPSILON {
            return;
        }
        self.black_threshold = t;
        self.write_uniforms(queue);
    }

    /// Tweak contrast S-curve strength.
    pub fn set_contrast(&mut self, queue: &wgpu::Queue, contrast: f32) {
        let c = contrast.clamp(0.0, 1.0);
        if (c - self.contrast).abs() < f32::EPSILON {
            return;
        }
        self.contrast = c;
        self.write_uniforms(queue);
    }

    fn write_uniforms(&self, queue: &wgpu::Queue) {
        let uniforms = ComposeUniforms {
            compose_ctl: [
                self.overlay_strength,
                self.black_threshold,
                self.contrast,
                self.display_profile as u32 as f32,
            ],
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Current display-profile.
    #[must_use]
    pub fn display_profile(&self) -> DisplayProfile {
        self.display_profile
    }

    /// Current overlay strength.
    #[must_use]
    pub fn overlay_strength(&self) -> f32 {
        self.overlay_strength
    }

    /// Texture format the pipeline targets.
    #[must_use]
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    /// Encode a render-pass that draws the fullscreen-triangle compose
    /// onto `view` (the surface texture). Loads existing color (so the
    /// scene shows through where substrate alpha is < 1).
    pub fn record_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("loa-host/substrate-compose-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        // Big-triangle fullscreen — 3 vertices, no VBO.
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wgsl_parses_via_naga() {
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(SUBSTRATE_COMPOSE_WGSL)
            .expect("substrate_compose.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .expect("substrate_compose.wgsl must validate via naga");
        assert!(SUBSTRATE_COMPOSE_WGSL.contains("vs_main"));
        assert!(SUBSTRATE_COMPOSE_WGSL.contains("fs_main"));
        assert!(SUBSTRATE_COMPOSE_WGSL.contains("substrate_tex"));
    }

    /// Construct a SubstrateComposePipeline against a real wgpu device.
    /// Marked `#[ignore]` so CI on machines without a GPU adapter skip ;
    /// run locally with `cargo test -p loa-host --features runtime
    /// substrate_compose -- --ignored` to exercise the GPU path.
    #[test]
    #[ignore]
    fn pipeline_construction_with_gpu_device() {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            },
        ))
        .expect("adapter required");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("loa-host/substrate-compose-test-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .expect("device required");
        let mut p = SubstrateComposePipeline::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(p.width, COMPOSE_TEX_W);
        assert_eq!(p.height, COMPOSE_TEX_H);
        assert!((p.overlay_strength() - DEFAULT_OVERLAY_STRENGTH).abs() < 1e-6);
        // Upload a deterministic gradient.
        let mut bytes = Vec::with_capacity(256 * 256 * 4);
        for y in 0..COMPOSE_TEX_H {
            for x in 0..COMPOSE_TEX_W {
                bytes.extend_from_slice(&[(x & 0xFF) as u8, (y & 0xFF) as u8, 0x80, 0xFF]);
            }
        }
        p.upload(&queue, &bytes);
        // Tweak overlay strength + record a no-op pass into a throwaway view.
        p.set_overlay_strength(&queue, 0.25);
        assert!((p.overlay_strength() - 0.25).abs() < 1e-6);
    }
}
