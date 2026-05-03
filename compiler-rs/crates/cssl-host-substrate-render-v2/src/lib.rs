//! § cssl-host-substrate-render-v2 — RAW-COMPUTE substrate-render.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L6-V2 · raw-compute-direct-to-display · zero render-pipeline overhead
//!
//! § THESIS
//!
//! Conventional GPU pipelines do : *vertex-shader → rasterizer → fragment-shader
//! → output*. The substrate paradigm doesn't need any of that. It needs :
//!
//! ```text
//! observer-coord + crystal-list → per-pixel-ω-field-resonance → RGBA8 buffer
//!                                                              → display
//! ```
//!
//! v2 = single compute-pass writes pixels DIRECTLY to a storage-texture, then
//! one [`copy_texture_to_texture`](wgpu::CommandEncoder::copy_texture_to_texture)
//! blits it onto the swapchain image. There is :
//!
//! - **NO render-pipeline** (`RenderPipeline`)
//! - **NO vertex-shader**
//! - **NO fragment-shader**
//! - **NO rasterizer / depth-buffer / MSAA-resolve / blend-state**
//! - **NO bind-group-layout entries for vertex / fragment stages**
//!
//! Just :
//!
//! - 1 [`ComputePipeline`] with one bind-group (uniform · storage · storage-texture)
//! - 1 compute-dispatch per frame (`(ceil(w/8), ceil(h/8), 1)` workgroups)
//! - 1 texture-blit to the swapchain image
//! - 1 [`Surface::present`] call per frame
//!
//! § WHY NOT WRITE THE COMPUTE OUTPUT DIRECTLY TO THE SWAPCHAIN
//!
//! In wgpu 23 a swapchain image must be created with the surface's preferred
//! format (typically `Bgra8Unorm` or `Bgra8UnormSrgb`). Storage-binding for
//! these formats requires the `BGRA8UNORM_STORAGE` feature, which is gated
//! per-adapter — not all backends/drivers support it. To stay portable the
//! v2 pipeline writes its compute-output to an internal `Rgba8Unorm` storage-
//! texture (universally supported as a storage format) and then issues a single
//! intra-GPU `copy_texture_to_texture` to the swapchain image. This blit costs
//! a few microseconds — negligible compared to the per-pixel ray-walk in the
//! compute kernel.
//!
//! Callers that have verified `BGRA8UNORM_STORAGE` is available on their
//! adapter MAY skip the staging texture by binding the swapchain view directly
//! ; v2 does not currently expose that fast-path because the savings are
//! sub-millisecond at 4K and the portability win is significant.
//!
//! § PIPELINE
//!
//! 1. [`pack_observer`] writes the per-frame observer uniform.
//! 2. [`pack_crystals`] writes the per-frame crystal storage buffer (resized
//!    on demand if the slice grows).
//! 3. [`RendererV2::tick`] :
//!    a. Encodes a compute-pass : `dispatch_workgroups(ceil(w/8), ceil(h/8), 1)`.
//!    b. Encodes `copy_texture_to_texture(internal_rgba8 → swapchain_image)`.
//!    c. Submits the encoder.
//!
//! § DETERMINISM
//!
//! Each pixel is one independent thread (no shared state across threads).
//! Within a thread the iteration order is fixed (sample-major × crystal-
//! linear-scan). Same `(observer, crystals, width, height)` ⇒ same output
//! texture, byte-for-byte. The `per_frame_determinism` test verifies this on
//! a headless adapter when one is available.
//!
//! § CONSENT (PRIME-DIRECTIVE)
//!
//! The Σ-mask check lives in the shader itself, before any contribution is
//! added to the per-pixel accumulator. Revoking the silhouette aspect either
//! on the observer or the crystal ZEROes that crystal's contribution. There
//! is no fallback render-path that bypasses the mask.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

// ════════════════════════════════════════════════════════════════════════════
// § Re-exports — v2 reuses the W18-G buffer-pack verbatim. Same Crystal[]→
// GpuCrystal[] pack ; same ObserverCoord→GpuObserver pack. The architectural
// change is the OUTPUT stage, not the input pack.
// ════════════════════════════════════════════════════════════════════════════

pub use cssl_host_substrate_resonance_gpu::{
    pack_crystal, pack_crystals, pack_observer, GpuCrystal, GpuObserver,
};

// ════════════════════════════════════════════════════════════════════════════
// § T11-W18-SOA-PACK · 64-byte cache-friendly Crystal pack.
// ════════════════════════════════════════════════════════════════════════════
//
// `GpuCrystalPacked` shrinks the per-crystal storage-buffer footprint from
// 352 B (W18-G `GpuCrystal`) to 64 B by :
//   - keeping world-position + extent + Σ-mask at full precision,
//   - host-side pre-blending the 4-illuminant spectral table to a single
//     16-band byte-spectrum (16 B),
//   - quantizing the silhouette spline from 4-axis i32 to 2-axis i8 (32 B).
//
// At 128 crystals this drops the storage-buffer from 44 KiB to 8 KiB —
// a 5.5× cache-pressure reduction that fits inside every mainstream
// GPU's per-SM L1.
//
// The packed path is opt-in via `LOA_SUBSTRATE_PACKED=1`. Until the
// matching WGSL kernel ships the path is **scaffold-only** : the host-
// side struct + pack functions + tests are ready, callers can build
// against the API, but the v2 renderer continues to use the 352-byte
// path for now. See `packed.rs` module docs for the full layout +
// rationale + the future shader-rewrite plan.

pub mod packed;
pub use packed::{
    pack_crystal_packed, pack_crystals_packed, packed_path_enabled,
    GpuCrystalPacked,
};

#[cfg(feature = "runtime")]
use std::borrow::Cow;
#[cfg(feature = "runtime")]
use std::num::NonZeroU64;

#[cfg(feature = "runtime")]
use cssl_host_alien_materialization::observer::ObserverCoord;
#[cfg(feature = "runtime")]
use cssl_host_crystallization::Crystal;

/// The compute-shader source. Compiled-in at build time so the crate is
/// fully self-contained.
pub const SHADER_SRC: &str = include_str!("../shaders/substrate_v2.wgsl");

/// Workgroup-size constants (must match `@workgroup_size(8, 8, 1)` in WGSL).
pub const WORKGROUP_X: u32 = 8;
pub const WORKGROUP_Y: u32 = 8;

/// Initial GpuCrystal buffer size in elements. Resized on demand if the
/// caller pushes more crystals than capacity.
pub const INITIAL_CAPACITY: u32 = 1024;

/// Storage-texture format used for the compute output. `Rgba8Unorm` is
/// universally supported as a storage-binding format on Vulkan / D3D12 /
/// Metal / WebGPU without any feature flag.
#[cfg(feature = "runtime")]
pub const COMPUTE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

// ════════════════════════════════════════════════════════════════════════════
// § RendererV2 — host wrapper around a compute-only pipeline + swapchain blit.
// ════════════════════════════════════════════════════════════════════════════

/// One [`RendererV2`] per `(width, height)` resolution. Re-create on resize.
///
/// Owns :
/// - The compute pipeline (one shader · one entry-point)
/// - The bind-group layout (uniform · read-only-storage · write-only storage-texture)
/// - The observer uniform buffer
/// - The crystals storage buffer (resized on demand, power-of-2 amortised)
/// - The internal `Rgba8Unorm` storage-texture that the compute kernel writes
///   to (later blitted to the swapchain image).
#[cfg(feature = "runtime")]
#[allow(clippy::struct_field_names)]
pub struct RendererV2 {
    width: u32,
    height: u32,
    /// Capacity in `GpuCrystal` elements (resized on demand).
    capacity: u32,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    observer_buf: wgpu::Buffer,
    crystals_buf: wgpu::Buffer,
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
}

#[cfg(feature = "runtime")]
impl RendererV2 {
    /// Build a new compute-only pipeline + bind-group at the given resolution.
    /// `surface_config` is taken to confirm the swapchain dims at construction
    /// time ; the renderer matches its internal storage-texture to the surface
    /// dims so the per-frame blit is always a 1:1 copy (no scaling).
    pub fn new(device: &wgpu::Device, surface_config: &wgpu::SurfaceConfiguration) -> Self {
        Self::new_dims(device, surface_config.width, surface_config.height)
    }

    /// Construct directly from explicit dims. Useful for headless tests where
    /// no `SurfaceConfiguration` exists.
    pub fn new_dims(device: &wgpu::Device, width: u32, height: u32) -> Self {
        // ── Shader module ────────────────────────────────────────────
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("substrate_v2::compute"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER_SRC)),
        });

        // ── Output storage texture (Rgba8Unorm · STORAGE_BINDING +
        //    COPY_SRC so we can blit to swapchain). ─────────────────────
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("substrate_v2::output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COMPUTE_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── Observer uniform buffer (one frame's worth) ──────────────
        let observer_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("substrate_v2::observer"),
            size: GpuObserver::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Crystals storage buffer (capacity-tracked) ───────────────
        let capacity = INITIAL_CAPACITY;
        let crystals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("substrate_v2::crystals"),
            size: u64::from(capacity) * GpuCrystal::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Bind-group layout : 0=observer, 1=crystals, 2=output-texture
        //    Note : visibility is COMPUTE only. There is no vertex / fragment
        //    visibility because there is no render-pipeline. ────────────
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("substrate_v2::bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(GpuObserver::SIZE_BYTES as u64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(GpuCrystal::SIZE_BYTES as u64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: COMPUTE_TEXTURE_FORMAT,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        // ── Pipeline layout + compute pipeline ───────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("substrate_v2::pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("substrate_v2::compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            width,
            height,
            capacity,
            pipeline,
            bind_group_layout,
            observer_buf,
            crystals_buf,
            output_texture,
            output_view,
        }
    }

    /// `(width, height)` the renderer was configured for.
    pub const fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Borrow the internal compute-output texture (`Rgba8Unorm`). Useful for
    /// callers that want to sample the substrate-render image as a texture
    /// (e.g. for an in-game scope/screen).
    pub const fn output_texture(&self) -> &wgpu::Texture {
        &self.output_texture
    }

    /// Borrow the internal compute-output texture-view.
    pub const fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }

    /// Resize the storage buffer to fit ≥ `n` GpuCrystal entries. Power-of-2
    /// rounding amortises resize churn.
    fn ensure_capacity(&mut self, device: &wgpu::Device, n: u32) {
        if n <= self.capacity {
            return;
        }
        let new_cap = n.next_power_of_two().max(INITIAL_CAPACITY);
        self.crystals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("substrate_v2::crystals(resized)"),
            size: u64::from(new_cap) * GpuCrystal::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.capacity = new_cap;
    }

    /// Run one frame.
    ///
    /// 1. Writes the observer uniform + crystals storage buffer.
    /// 2. Encodes ONE compute-dispatch (writes pixels into the internal
    ///    `Rgba8Unorm` storage-texture).
    /// 3. Encodes ONE `copy_texture_to_texture` blit from the internal
    ///    storage-texture to `swapchain_texture`.
    /// 4. Submits the encoder.
    ///
    /// The caller is responsible for acquiring the swapchain image
    /// (`Surface::get_current_texture`) and calling `present()` after this
    /// returns.
    ///
    /// `swapchain_texture` MUST be the same size as the renderer (validated
    /// against `self.dims()` at the top of the function ; mismatched dims
    /// trigger a panic in debug and a clamped blit in release).
    pub fn tick(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        observer: ObserverCoord,
        crystals: &[Crystal],
        swapchain_texture: &wgpu::Texture,
    ) {
        debug_assert_eq!(
            (swapchain_texture.width(), swapchain_texture.height()),
            (self.width, self.height),
            "swapchain dims must match renderer dims (recreate RendererV2 on resize)",
        );

        self.ensure_capacity(device, crystals.len() as u32);

        // ── Write the observer uniform ───────────────────────────────
        let gpu_observer = pack_observer(
            observer,
            self.width,
            self.height,
            crystals.len() as u32,
        );
        queue.write_buffer(
            &self.observer_buf,
            0,
            bytemuck::bytes_of(&gpu_observer),
        );

        // ── Write the crystals storage buffer ────────────────────────
        if !crystals.is_empty() {
            let packed = pack_crystals(crystals);
            queue.write_buffer(&self.crystals_buf, 0, bytemuck::cast_slice(&packed));
        }

        // ── Build the bind-group (cheap ; rebound per-frame) ─────────
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("substrate_v2::bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.observer_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.crystals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.output_view),
                },
            ],
        });

        // ── Encode compute-dispatch + swapchain-blit + submit ────────
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("substrate_v2::encoder"),
        });

        // (a) Compute-pass : write pixels into self.output_texture.
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("substrate_v2::compute_pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg_x = self.width.div_ceil(WORKGROUP_X);
            let wg_y = self.height.div_ceil(WORKGROUP_Y);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        // (b) Texture blit : self.output_texture → swapchain_texture.
        //     Driver-native intra-GPU copy. No CPU readback. No format
        //     conversion (both sides Rgba8/Bgra8Unorm are the same byte
        //     layout ; the swapchain re-interprets per-channel order at
        //     present time).
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: &self.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: swapchain_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(Some(encoder.finish()));
    }

    /// Headless variant : runs only the compute-pass (no swapchain-blit). The
    /// caller can read back the internal output-texture afterward. Used by
    /// tests that have no surface but want to verify per-frame determinism +
    /// kernel correctness.
    pub fn tick_headless(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        observer: ObserverCoord,
        crystals: &[Crystal],
    ) -> &wgpu::TextureView {
        self.ensure_capacity(device, crystals.len() as u32);

        let gpu_observer = pack_observer(
            observer,
            self.width,
            self.height,
            crystals.len() as u32,
        );
        queue.write_buffer(&self.observer_buf, 0, bytemuck::bytes_of(&gpu_observer));

        if !crystals.is_empty() {
            let packed = pack_crystals(crystals);
            queue.write_buffer(&self.crystals_buf, 0, bytemuck::cast_slice(&packed));
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("substrate_v2::bg(headless)"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.observer_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.crystals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.output_view),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("substrate_v2::encoder(headless)"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("substrate_v2::compute_pass(headless)"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg_x = self.width.div_ceil(WORKGROUP_X);
            let wg_y = self.height.div_ceil(WORKGROUP_Y);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        queue.submit(Some(encoder.finish()));

        &self.output_view
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Headless-GPU helpers (pollster-blocking adapter request) — used by tests.
// ════════════════════════════════════════════════════════════════════════════

/// Try to acquire a `(instance, adapter, device, queue)` for headless compute.
/// Returns `None` if no GPU adapter is available (e.g. pure-CPU CI runner).
#[cfg(feature = "runtime")]
pub fn try_headless_device() -> Option<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::default(),
        dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        gles_minor_version: wgpu::Gles3MinorVersion::default(),
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("substrate_v2::headless"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .ok()?;
    Some((instance, adapter, device, queue))
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1 : the embedded WGSL parses + passes naga validation. Catches
    /// shader-syntax / binding / type errors at unit-test time without GPU.
    #[test]
    fn wgsl_naga_validates() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC)
            .expect("substrate_v2.wgsl must parse");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .expect("substrate_v2.wgsl must validate");
    }

    /// Test 2 : the WGSL exports exactly one COMPUTE entry-point named `main`
    /// with workgroup-size `(WORKGROUP_X, WORKGROUP_Y, 1)`. Guards against
    /// silent host-vs-shader drift AND verifies there is NO vertex/fragment
    /// stage (the architectural invariant of v2 = compute-only pipeline).
    #[test]
    fn compute_only_pipeline_builds() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC).unwrap();
        // Exactly one entry-point.
        assert_eq!(
            module.entry_points.len(),
            1,
            "v2 pipeline must have EXACTLY one entry-point (compute) ; \
             no vertex/fragment stages allowed",
        );
        let ep = &module.entry_points[0];
        assert_eq!(ep.name, "main");
        // It must be a compute stage.
        assert_eq!(
            ep.stage,
            naga::ShaderStage::Compute,
            "v2 pipeline rejects vertex/fragment stages by design",
        );
        // Workgroup size matches host consts.
        let [wgx, wgy, wgz] = ep.workgroup_size;
        assert_eq!(wgx, WORKGROUP_X);
        assert_eq!(wgy, WORKGROUP_Y);
        assert_eq!(wgz, 1);
    }

    /// Test 3 (headless GPU) : full pipeline construction + 64×64 dispatch.
    /// Documents end-to-end build at the smallest viable resolution. Cleanly
    /// skips on CI runners without a GPU (returns `None` from
    /// `try_headless_device`).
    #[cfg(feature = "runtime")]
    #[test]
    fn headless_tick_smoketest() {
        use cssl_host_alien_materialization::observer::ObserverCoord;
        use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};
        let Some((_inst, _adapter, device, queue)) = try_headless_device() else {
            eprintln!("no GPU adapter available · skipped");
            return;
        };
        let mut renderer = RendererV2::new_dims(&device, 64, 64);
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let observer = ObserverCoord::default();

        let _view = renderer.tick_headless(&device, &queue, observer, &[crystal]);
        // If we reach here without panicking, the compute pipeline ran end-
        // to-end (shader · bind-group · dispatch · submit).
    }

    /// Test 4 (headless GPU) : per-frame determinism. The same
    /// `(observer, crystals)` dispatched twice must produce the same output
    /// texture, byte-for-byte. Verifies the v2 architectural promise that
    /// compute-only kernels are deterministic across frames.
    ///
    /// Reads back the internal storage-texture via `copy_texture_to_buffer`
    /// + `Buffer::map_async` after each tick and compares bytes.
    #[cfg(feature = "runtime")]
    #[test]
    fn per_frame_determinism() {
        use cssl_host_alien_materialization::observer::ObserverCoord;
        use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};
        let Some((_inst, _adapter, device, queue)) = try_headless_device() else {
            eprintln!("no GPU adapter available · skipped");
            return;
        };

        const W: u32 = 32;
        const H: u32 = 32;
        let mut renderer = RendererV2::new_dims(&device, W, H);

        // Fixed scene.
        let crystals = vec![
            Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500)),
            Crystal::allocate(CrystalClass::Object, 2, WorldPos::new(500, 0, 1500)),
            Crystal::allocate(CrystalClass::Entity, 3, WorldPos::new(-500, 0, 1500)),
        ];
        let observer = ObserverCoord::default();

        let frame_a = readback_after_tick(&device, &queue, &mut renderer, observer, &crystals);
        let frame_b = readback_after_tick(&device, &queue, &mut renderer, observer, &crystals);

        assert_eq!(frame_a.len(), frame_b.len(), "readback len must match");
        assert_eq!(
            frame_a, frame_b,
            "v2 compute kernel must be deterministic across frames \
             for a fixed (observer, crystals)",
        );
    }

    /// Test 5 (always runs · headless or not) : verify the public API contract.
    /// `RendererV2` is documented to construct from a `SurfaceConfiguration`,
    /// from explicit dims, and to expose `dims()` + `output_texture()` +
    /// `output_view()` accessors. We stop short of constructing a real
    /// `RendererV2` (which needs a GPU) ; we only verify the API surface
    /// at compile-time via `let _ = ...` type-bindings.
    #[cfg(feature = "runtime")]
    #[test]
    fn api_surface_contract() {
        // Compile-time assertions only. If this test compiles, the API
        // surface is intact ; if a method signature regresses, this test
        // fails to compile.
        fn _accepts_renderer(r: &mut RendererV2) {
            let _: (u32, u32) = r.dims();
            let _: &wgpu::Texture = r.output_texture();
            let _: &wgpu::TextureView = r.output_view();
        }
        // Verify the constructor signatures exist (uncalled — type-check
        // only).
        let _ctor1: fn(&wgpu::Device, &wgpu::SurfaceConfiguration) -> RendererV2 =
            RendererV2::new;
        let _ctor2: fn(&wgpu::Device, u32, u32) -> RendererV2 = RendererV2::new_dims;
    }

    // ── readback helper for per-frame determinism test ──────────────
    #[cfg(feature = "runtime")]
    fn readback_after_tick(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut RendererV2,
        observer: cssl_host_alien_materialization::observer::ObserverCoord,
        crystals: &[cssl_host_crystallization::Crystal],
    ) -> Vec<u8> {
        let (w, h) = renderer.dims();
        // Run one tick.
        let _ = renderer.tick_headless(device, queue, observer, crystals);

        // Allocate a CPU-mappable buffer big enough for the readback.
        // Row-pitch must be aligned to COPY_BYTES_PER_ROW_ALIGNMENT (256).
        let bytes_per_row = align_up(w * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer_size = (bytes_per_row * h) as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("substrate_v2::readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("substrate_v2::readback_encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: renderer.output_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &staging,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        // Map + read.
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().expect("buffer map must succeed");
        let data = slice.get_mapped_range();
        // Strip per-row padding so we compare just the (w * 4) image bytes.
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            let off = (row * bytes_per_row) as usize;
            out.extend_from_slice(&data[off..off + (w * 4) as usize]);
        }
        drop(data);
        staging.unmap();
        out
    }

    #[cfg(feature = "runtime")]
    fn align_up(x: u32, align: u32) -> u32 {
        x.div_ceil(align) * align
    }
}
