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

/// § T11-W18-PARADIGM-FOREST (Apocky-pivot 2026-05-02) · the SUBSTRATE-RESONANCE
/// § T11-W18-OVERLAY-DIM · Apocky 2026-05-03 directive : "It needs to render
/// the same test-room scene as the old engine, or else we go back to our
/// proprietary CSSL take on traditional rendering bleeding-edge approaches."
///
/// REVERTED 1.0 → 0.0. Traditional-pipeline (scene.wgsl + cfer-raymarch +
/// tonemap) is the PRIMARY path · substrate is now an OPT-IN overlay via
/// LOA_OVERLAY_STRENGTH env-var (0.0..=1.0). This restores the navigatable
/// test-room. Substrate paradigm work persists in cssl-host-substrate-*
/// crates and substrate_v2.wgsl ; user can flip env-var to surface it.
///
/// PRIOR-RATIONALE (preserved for context) : 1.0 was set during the
/// "make substrate-PRIMARY" pivot when Apocky asked "why does it still look
/// the same as before?" — but the substrate-output (concentric crystal-rings)
/// turned out to be visually impoverished compared to the test-room scene.
pub const DEFAULT_OVERLAY_STRENGTH: f32 = 0.0;

/// § T11-W18-PARADIGM-FOREST · sub-threshold alpha → pure (0,0,0,0) so AMOLED
/// preserves true-black void · BUT we want substrate-pixels to dominate ·
/// lowered 0.04 → 0.005 (≈ 1/255) so almost-every substrate-resonance
/// emission lights · only fully-zero alpha is suppressed. Crystal-emergent
/// fringes · interference patterns · spectral fringes all visible against
/// the pitch-black AMOLED void.
pub const DEFAULT_AMOLED_BLACK_THRESHOLD: f32 = 0.005;

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
    /// § T11-W18-L9-AMOLED-DEEP · re-tuned :
    ///   - Amoled : 0.003 · pitch-black-er · matches AMOLED snap-to-zero physics
    ///   - Oled : 0.008 · most OLED leaks BFI light at this floor
    ///   - IpsLcd : 0.020 · no point lifting blacks below the LCD-floor
    ///   - VaLcd : 0.012 · between OLED + IPS
    ///   - HdrExt : 0.0001 · PQ EOTF zeros out below this anyway
    /// Saturation-boost / snap-to-zero / peak-nits / is-HDR live on the
    /// `DisplayProfileDeep` trait in `display_profile`.
    pub fn defaults(self) -> (f32, f32) {
        match self {
            Self::Amoled => (0.003, 0.40),
            Self::Oled => (0.008, 0.30),
            Self::IpsLcd => (0.020, 0.15), // do NOT crush blacks · IPS already gray
            Self::VaLcd => (0.012, 0.25),
            Self::HdrExt => (0.0001, 0.45), // PQ EOTF preserves true-black
        }
    }
}

/// § T11-W18-L9-AMOLED-DEEP — Per-profile deep-attribute triple
/// `(snap_to_zero, saturation_boost, peak_nits)`. Held inline in
/// `substrate_compose` to avoid a back-reference into `display_profile`
/// (which depends on this module via `DisplayProfile`). The constants in
/// `display_profile::*` are the canonical names callers should use ; the
/// numbers here MUST stay in sync.
#[must_use]
pub fn deep_attributes_for(profile: DisplayProfile) -> (f32, f32, f32) {
    match profile {
        // (snap_to_zero, saturation_boost, peak_nits)
        DisplayProfile::Amoled => (0.003, 1.15, 800.0),
        DisplayProfile::Oled => (0.008, 1.08, 600.0),
        DisplayProfile::IpsLcd => (0.020, 1.00, 400.0),
        DisplayProfile::VaLcd => (0.012, 1.05, 500.0),
        DisplayProfile::HdrExt => (0.0001, 1.20, 1000.0),
    }
}

/// Embedded WGSL source for the compose shader.
pub const SUBSTRATE_COMPOSE_WGSL: &str = include_str!("../shaders/substrate_compose.wgsl");

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ComposeUniforms {
    /// .x = overlay strength
    /// .y = AMOLED black-threshold (alpha gate)
    /// .z = contrast S-curve strength
    /// .w = display-profile-id (Amoled=0..HdrExt=4)
    compose_ctl: [f32; 4],
    /// § T11-W18-L9-AMOLED-DEEP — extended per-profile attributes :
    /// .x = snap-to-zero luminance threshold (pixels below → pure (0,0,0))
    /// .y = saturation-boost (HSV-S × this · clamped 0..2)
    /// .z = peak-nits (HDR PQ-encode target · ignored for SDR)
    /// .w = is-hdr flag (1.0 = Rec.2020 + PQ encoding · 0.0 = SDR sRGB)
    display_ctl: [f32; 4],
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
    /// § T11-W18-L9 — snap-to-zero luminance (display_ctl.x in shader).
    snap_to_zero: f32,
    /// § T11-W18-L9 — saturation-boost (display_ctl.y in shader).
    saturation_boost: f32,
    /// § T11-W18-L9 — peak-nits (display_ctl.z · HDR-only · ignored for SDR).
    peak_nits: f32,
    /// Cached texture dimensions. Re-allocated by `ensure_size` if the
    /// host ever resizes the substrate pixel-field at runtime.
    width: u32,
    height: u32,
    /// Surface format remembered so `ensure_size` doesn't re-fetch it.
    target_format: wgpu::TextureFormat,
    /// § T11-W18-N · true iff the bind-group was rebuilt around an external
    /// `TextureView` (the GPU compute-shader output) rather than the local
    /// 256×256 CPU upload texture. While `true`, callers should NOT invoke
    /// `upload(queue, bytes)` — the bind-group's texture view points at the
    /// GPU compute-shader output instead, so CPU uploads would be a no-op
    /// at best and waste bandwidth. Reset to `false` by `ensure_size` (which
    /// reallocates the local texture + rebuilds the bind-group around it).
    use_gpu_view: bool,
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

        // § T11-W18-L9-AMOLED-DEEP · seed deep-attribute defaults from the
        // Amoled profile (matches the existing display_profile = Amoled
        // bootstrap). The host immediately overwrites these via
        // `set_display_profile` once the auto-detect completes.
        let initial_profile = DisplayProfile::Amoled;
        let initial_snap = 0.003_f32;
        let initial_sat = 1.15_f32;
        let initial_nits = 800.0_f32;
        let initial_hdr_flag = 0.0_f32; // Amoled is SDR
        // § T11-W18-LOA_OVERLAY_STRENGTH · env-var override 0.0..=1.0
        //   default = DEFAULT_OVERLAY_STRENGTH (currently 0.0 = traditional-PRIMARY)
        //   set LOA_OVERLAY_STRENGTH=1.0 to opt-in to substrate-PRIMARY overlay
        let initial_overlay = std::env::var("LOA_OVERLAY_STRENGTH")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| v.clamp(0.0, 1.0))
            .unwrap_or(DEFAULT_OVERLAY_STRENGTH);
        let uniforms = ComposeUniforms {
            compose_ctl: [
                initial_overlay,
                DEFAULT_AMOLED_BLACK_THRESHOLD,
                DEFAULT_AMOLED_CONTRAST,
                initial_profile as u32 as f32,
            ],
            display_ctl: [initial_snap, initial_sat, initial_nits, initial_hdr_flag],
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
                 · overlay={initial_overlay} (default {DEFAULT_OVERLAY_STRENGTH} · env LOA_OVERLAY_STRENGTH override)"
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
            overlay_strength: initial_overlay,
            black_threshold: DEFAULT_AMOLED_BLACK_THRESHOLD,
            contrast: DEFAULT_AMOLED_CONTRAST,
            display_profile: initial_profile,
            snap_to_zero: initial_snap,
            saturation_boost: initial_sat,
            peak_nits: initial_nits,
            width: COMPOSE_TEX_W,
            height: COMPOSE_TEX_H,
            target_format,
            use_gpu_view: false,
        }
    }

    /// § T11-W18-N · Rebuild the bind-group so it samples the supplied
    /// external `wgpu::TextureView` (e.g., the 1440p substrate-resonance
    /// compute-shader output) instead of the local 256×256 CPU-upload
    /// texture. Idempotent (cheap; rebuilds the bind-group once per call —
    /// callers should invoke this once after the GPU pipeline binds its
    /// output, not every frame).
    ///
    /// While the external view is bound, the local texture upload path
    /// (`upload(queue, bytes)`) is dormant — the bind-group's texture
    /// binding points at `external_view`. Call `ensure_size` (which
    /// reallocates the local texture and rebuilds the bind-group around it)
    /// to revert to the CPU-upload path.
    ///
    /// SAFETY : the caller must keep `external_view`'s underlying texture
    /// alive for as long as this pipeline issues render-passes ; typically
    /// this is the substrate-resonance GPU crate's `output_view` whose
    /// lifetime is bound to the long-lived `SubstrateRenderState::gpu`
    /// field on the host.
    pub fn bind_external_view(
        &mut self,
        device: &wgpu::Device,
        external_view: &wgpu::TextureView,
    ) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loa-host/substrate-compose-bg (external GPU view)"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(external_view),
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
        self.use_gpu_view = true;
        log_event(
            "INFO",
            "loa-host/substrate-compose",
            "bind_external_view · sampling GPU compute-shader output (CPU upload path dormant)",
        );
    }

    /// § T11-W18-N · True iff the bind-group is currently sampling an
    /// external GPU `TextureView` (set by `bind_external_view`). False when
    /// the local CPU-upload texture is bound (the default after `new` or
    /// after `ensure_size`).
    #[must_use]
    pub fn is_external_view_bound(&self) -> bool {
        self.use_gpu_view
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
        // § T11-W18-N · reverting bind-group to local CPU-upload texture
        // invalidates any prior external-view binding. CPU upload path is
        // active again until `bind_external_view` is called.
        self.use_gpu_view = false;
        log_event(
            "INFO",
            "loa-host/substrate-compose",
            &format!("texture-resized · {w}×{h}"),
        );
    }

    /// Upload the substrate pixel-field bytes to the GPU texture. `bytes`
    /// must be `width * height * 4` long (RGBA8). Short payloads are
    /// truncated to the texture extent ; over-long payloads are clipped.
    ///
    /// § T11-W18-N · No-op when the bind-group is sampling an external
    /// `TextureView` (set by `bind_external_view`) — the local CPU-upload
    /// texture is no longer the bind-group's source, so writing to it would
    /// waste bandwidth without affecting the rendered overlay. Caller can
    /// query `is_external_view_bound()` to decide whether to skip the CPU
    /// path altogether.
    pub fn upload(&self, queue: &wgpu::Queue, bytes: &[u8]) {
        if self.use_gpu_view {
            return;
        }
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

    /// Adopt a `DisplayProfile`. Updates `black_threshold` + `contrast` +
    /// the deep attributes (snap-to-zero · saturation-boost · peak-nits) to
    /// the profile's tuned defaults · re-uploads uniform on change. Call
    /// once at startup after detecting the panel + on monitor-change.
    pub fn set_display_profile(&mut self, queue: &wgpu::Queue, profile: DisplayProfile) {
        if self.display_profile == profile {
            return;
        }
        let (bt, ct) = profile.defaults();
        let (snap, sat, nits) = deep_attributes_for(profile);
        self.display_profile = profile;
        self.black_threshold = bt;
        self.contrast = ct;
        self.snap_to_zero = snap;
        self.saturation_boost = sat;
        self.peak_nits = nits;
        self.write_uniforms(queue);
        log_event(
            "INFO",
            "loa-host/substrate-compose",
            &format!(
                "display-profile · {profile:?} · black_thresh={bt:.3} · contrast={ct:.3} · \
                 snap={snap:.4} · sat={sat:.2} · peak-nits={nits}"
            ),
        );
    }

    /// § T11-W18-L9-AMOLED-DEEP — direct override of the deep-attribute
    /// triple. Useful when the operator wants to override one profile-default
    /// at runtime (e.g., bump saturation on a slightly-faded OLED). Re-uploads
    /// only when at least one component differs from the current setting.
    pub fn set_deep_attributes(
        &mut self,
        queue: &wgpu::Queue,
        snap_to_zero: f32,
        saturation_boost: f32,
        peak_nits: f32,
    ) {
        let s = snap_to_zero.clamp(0.0, 1.0);
        let b = saturation_boost.clamp(0.0, 2.0);
        let n = peak_nits.clamp(50.0, 10000.0);
        let unchanged = (s - self.snap_to_zero).abs() < f32::EPSILON
            && (b - self.saturation_boost).abs() < f32::EPSILON
            && (n - self.peak_nits).abs() < f32::EPSILON;
        if unchanged {
            return;
        }
        self.snap_to_zero = s;
        self.saturation_boost = b;
        self.peak_nits = n;
        self.write_uniforms(queue);
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
        let is_hdr_flag = if matches!(self.display_profile, DisplayProfile::HdrExt) {
            1.0
        } else {
            0.0
        };
        let uniforms = ComposeUniforms {
            compose_ctl: [
                self.overlay_strength,
                self.black_threshold,
                self.contrast,
                self.display_profile as u32 as f32,
            ],
            display_ctl: [
                self.snap_to_zero,
                self.saturation_boost,
                self.peak_nits,
                is_hdr_flag,
            ],
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));
    }

    /// § T11-W18-L9 — current snap-to-zero threshold (display_ctl.x).
    #[must_use]
    pub fn snap_to_zero(&self) -> f32 {
        self.snap_to_zero
    }

    /// § T11-W18-L9 — current saturation-boost (display_ctl.y).
    #[must_use]
    pub fn saturation_boost(&self) -> f32 {
        self.saturation_boost
    }

    /// § T11-W18-L9 — current peak-nits (display_ctl.z).
    #[must_use]
    pub fn peak_nits(&self) -> f32 {
        self.peak_nits
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
        // § T11-W18-L9-AMOLED-DEEP — verify the new shader-stages are present.
        assert!(
            SUBSTRATE_COMPOSE_WGSL.contains("display_ctl"),
            "extended display_ctl uniform must be present"
        );
        assert!(
            SUBSTRATE_COMPOSE_WGSL.contains("rgb_to_hsv"),
            "saturation-boost requires HSV conversion path"
        );
        assert!(
            SUBSTRATE_COMPOSE_WGSL.contains("hdr_pq_encode"),
            "HDR PQ encoding path must be present"
        );
        assert!(
            SUBSTRATE_COMPOSE_WGSL.contains("rec709_to_rec2020"),
            "Rec.2020 wide-gamut matrix must be present"
        );
        assert!(
            SUBSTRATE_COMPOSE_WGSL.contains("snap_thr"),
            "snap-to-zero gate must be present"
        );
    }

    #[test]
    fn deep_attributes_for_amoled_pitch_black() {
        let (snap, sat, nits) = deep_attributes_for(DisplayProfile::Amoled);
        assert!((snap - 0.003).abs() < 1e-6);
        assert!((sat - 1.15).abs() < 1e-6);
        assert!((nits - 800.0).abs() < 1e-3);
    }

    #[test]
    fn deep_attributes_for_oled_below_amoled() {
        let (a_snap, a_sat, _) = deep_attributes_for(DisplayProfile::Amoled);
        let (o_snap, o_sat, _) = deep_attributes_for(DisplayProfile::Oled);
        assert!(o_snap > a_snap, "OLED snap > AMOLED snap");
        assert!(o_sat < a_sat, "OLED sat < AMOLED sat");
    }

    #[test]
    fn deep_attributes_for_ips_no_crush_no_boost() {
        let (snap, sat, nits) = deep_attributes_for(DisplayProfile::IpsLcd);
        assert!((sat - 1.0).abs() < 1e-6, "IPS saturation = identity");
        assert!(snap > 0.015, "IPS snap floor ≥ LCD-floor");
        assert!((nits - 400.0).abs() < 1e-3);
    }

    #[test]
    fn deep_attributes_for_va_between_ips_and_oled() {
        let (snap, sat, _) = deep_attributes_for(DisplayProfile::VaLcd);
        let (i_snap, _, _) = deep_attributes_for(DisplayProfile::IpsLcd);
        let (o_snap, _, _) = deep_attributes_for(DisplayProfile::Oled);
        assert!(snap < i_snap && snap > o_snap);
        assert!((sat - 1.05).abs() < 1e-6);
    }

    #[test]
    fn deep_attributes_for_hdr_max_punch() {
        let (snap, sat, nits) = deep_attributes_for(DisplayProfile::HdrExt);
        assert!(snap < 0.001, "HDR snap below 1/1000");
        assert!(sat > 1.15, "HDR sat strongest");
        assert!((nits - 1000.0).abs() < 1e-3, "HDR peak = 1000 nits");
    }

    #[test]
    fn defaults_table_amoled_re_tuned_to_l9() {
        // § T11-W18-L9-AMOLED-DEEP · re-tuned defaults table.
        let (bt, ct) = DisplayProfile::Amoled.defaults();
        assert!((bt - 0.003).abs() < 1e-6, "AMOLED black-thr re-tuned to 0.003");
        assert!((ct - 0.40).abs() < 1e-6, "AMOLED contrast bumped to 0.40");
    }

    #[test]
    fn defaults_table_hdr_pq_aware() {
        let (bt, ct) = DisplayProfile::HdrExt.defaults();
        assert!((bt - 0.0001).abs() < 1e-7, "HDR black-thr re-tuned to 0.0001");
        assert!((ct - 0.45).abs() < 1e-6, "HDR contrast bumped to 0.45");
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

    /// § T11-W18-N · `bind_external_view` rebuilds the bind-group around a
    /// caller-supplied `TextureView` and flips `is_external_view_bound`.
    /// Marked `#[ignore]` because constructing a real `TextureView` requires
    /// a GPU device.
    #[test]
    #[ignore]
    fn bind_external_view_rebinds_bind_group_and_sets_flag() {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            },
        ))
        .expect("adapter required");
        let (device, _queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("loa-host/substrate-compose-bind-external-test"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .expect("device required");
        let mut p = SubstrateComposePipeline::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        assert!(!p.is_external_view_bound(), "freshly-constructed pipeline must default to CPU-upload path");

        // Allocate a 1440p Rgba8Unorm texture · stand-in for the GPU
        // compute-shader output. TEXTURE_BINDING usage is required so the
        // resulting view is bind-group-compatible.
        let ext_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("loa-host/substrate-compose-bind-external-test-tex"),
            size: wgpu::Extent3d {
                width: 2560,
                height: 1440,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let ext_view = ext_tex.create_view(&wgpu::TextureViewDescriptor::default());

        p.bind_external_view(&device, &ext_view);
        assert!(p.is_external_view_bound(), "after bind_external_view the GPU-view flag must be set");

        // ensure_size at the existing dimensions is a no-op (does not flip the flag).
        p.ensure_size(&device, COMPOSE_TEX_W, COMPOSE_TEX_H);
        assert!(p.is_external_view_bound(), "no-op ensure_size must NOT clear the GPU-view flag");

        // ensure_size to a different resolution reallocates the local
        // texture + rebuilds the bind-group around it · CPU path is back.
        p.ensure_size(&device, 512, 512);
        assert!(!p.is_external_view_bound(), "ensure_size with new dims must revert to CPU-upload path");
    }
}
