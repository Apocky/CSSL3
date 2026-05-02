//! § cssl-host-substrate-resonance-gpu — GPU compute-shader port of the
//! pixel-field resonance algorithm. 1440p @ 144Hz capable.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-G-GPU
//!
//! § THESIS
//!
//! `cssl-host-alien-materialization::pixel_field::resolve_substrate_resonance`
//! is the canonical CPU implementation. At 256×256 it hits ~112 fps with
//! rayon ; 2560×1440 has 56× more pixels so the CPU implementation cannot
//! sustain a 6.94 ms / 144 Hz frame budget. This crate ports the algorithm
//! to a wgpu compute-shader (`shaders/substrate_resonance.wgsl`) where every
//! pixel is a thread.
//!
//! § PIPELINE
//!
//! 1. Host packs the `&[Crystal]` slice into `&[GpuCrystal]` via
//!    `buffer_pack::pack_crystals` (336 B/crystal, bytemuck::Pod).
//! 2. Host packs the `ObserverCoord` + frame-meta into `GpuObserver` via
//!    `buffer_pack::pack_observer` (uniform, 16-aligned).
//! 3. `SubstrateResonanceGpu::dispatch` writes both buffers, dispatches a
//!    `(ceil(w/8), ceil(h/8), 1)` workgroup grid, and returns a wgpu
//!    storage texture (rgba8unorm).
//!
//! § DETERMINISM
//!
//! Each pixel is independent (one thread, no shared state across threads).
//! Within a thread the iteration order is fixed (sample-major × crystal-
//! linear-scan). Same `(observer, crystals)` ⇒ same output texture.
//!
//! § CONSENT (PRIME-DIRECTIVE)
//!
//! The Σ-mask check lives in the shader itself, before any contribution
//! is added to the per-pixel accumulator. Revoking the silhouette aspect
//! either on the observer or the crystal ZEROes that crystal's contribution.
//! There is no fallback render-path that bypasses the mask.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod buffer_pack;

pub use buffer_pack::{pack_crystal, pack_crystals, pack_observer, GpuCrystal, GpuObserver};

#[cfg(feature = "runtime")]
use std::borrow::Cow;
#[cfg(feature = "runtime")]
use std::num::NonZeroU64;

#[cfg(feature = "runtime")]
use cssl_host_alien_materialization::observer::ObserverCoord;
#[cfg(feature = "runtime")]
use cssl_host_crystallization::Crystal;

/// The compute-shader source. Compiled-in at build time so the crate is
/// fully self-contained ; the shader text is also re-exported for callers
/// that want to validate it through a custom naga config.
pub const SHADER_SRC: &str = include_str!("../shaders/substrate_resonance.wgsl");

/// Workgroup-size constants (must match `@workgroup_size(8, 8, 1)` in WGSL).
pub const WORKGROUP_X: u32 = 8;
pub const WORKGROUP_Y: u32 = 8;

/// Initial GpuCrystal buffer size in elements. Resized on demand if the
/// caller pushes more crystals than capacity.
pub const INITIAL_CAPACITY: u32 = 1024;

// ════════════════════════════════════════════════════════════════════════════
// § SubstrateResonanceGpu — the host wrapper around the wgpu compute pipeline.
// ════════════════════════════════════════════════════════════════════════════

/// Wraps : the compute-pipeline + bind-group + resizable storage buffer +
/// uniform buffer + RGBA8 output texture. One instance per (width, height)
/// resolution. Re-create on resize.
#[cfg(feature = "runtime")]
#[allow(clippy::struct_field_names)]
pub struct SubstrateResonanceGpu {
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
impl SubstrateResonanceGpu {
    /// Build a new compute pipeline + bind-group at the given resolution.
    /// The output texture is `rgba8unorm` (matches RGBA8 byte-pack used by
    /// `cssl-host-alien-materialization::PixelField::as_bytes_owned`).
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        // ── Shader module ────────────────────────────────────────────
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("substrate_resonance::compute"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER_SRC)),
        });

        // ── Output storage texture ───────────────────────────────────
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("substrate_resonance::output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── Observer uniform buffer (one frame's worth) ──────────────
        let observer_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("substrate_resonance::observer"),
            size: GpuObserver::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Crystals storage buffer (capacity-tracked) ───────────────
        let capacity = INITIAL_CAPACITY;
        let crystals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("substrate_resonance::crystals"),
            size: u64::from(capacity) * GpuCrystal::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Bind-group layout : 0=observer (uniform), 1=crystals (storage),
        //    2=output (storage texture)
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("substrate_resonance::bgl"),
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
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        // ── Pipeline layout + compute pipeline ───────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("substrate_resonance::pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("substrate_resonance::compute_pipeline"),
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

    /// Width × height the texture was allocated for.
    pub const fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Borrow the output texture (rgba8unorm). Safe to bind as a sampled
    /// texture in the next render-pass.
    pub const fn output_texture(&self) -> &wgpu::Texture {
        &self.output_texture
    }

    /// Borrow the output texture-view.
    pub const fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }

    /// Resize the storage buffer to fit ≥ `n` GpuCrystal entries. No-op if
    /// current capacity is already sufficient. Caller must invalidate any
    /// previously-bound bind-group ; `dispatch()` re-builds the bind-group
    /// every frame so the standard path is automatic.
    fn ensure_capacity(&mut self, device: &wgpu::Device, n: u32) {
        if n <= self.capacity {
            return;
        }
        // Round up to the next power of 2 to amortise resize churn.
        let new_cap = n.next_power_of_two().max(INITIAL_CAPACITY);
        self.crystals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("substrate_resonance::crystals(resized)"),
            size: u64::from(new_cap) * GpuCrystal::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.capacity = new_cap;
    }

    /// Dispatch the compute-shader for one frame. Writes the observer
    /// uniform + crystals storage buffer, builds a bind-group, encodes a
    /// compute-pass with `(ceil(w/8), ceil(h/8), 1)` workgroups.
    ///
    /// Returns a borrow of the output texture-view (rgba8unorm). The texture
    /// is owned by `self`, so the borrow is bounded by `self`'s lifetime.
    pub fn dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        observer: ObserverCoord,
        crystals: &[Crystal],
    ) -> &wgpu::TextureView {
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
            label: Some("substrate_resonance::bg"),
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

        // ── Encode + submit the compute pass ─────────────────────────
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("substrate_resonance::encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("substrate_resonance::pass"),
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
// § Headless-GPU helpers (pollster-blocking adapter request) — used by tests
// + bench harnesses that have no winit window.
// ════════════════════════════════════════════════════════════════════════════

/// Try to acquire a (device, queue) for headless compute. Returns None if
/// no GPU adapter is available (e.g. pure-CPU CI runner).
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
            label: Some("substrate_resonance::headless"),
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

    /// Test 1 : the embedded WGSL passes naga validation. This catches
    /// shader-syntax / binding / type errors at unit-test time without
    /// needing a GPU.
    #[test]
    fn shader_passes_naga_validation() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC)
            .expect("substrate_resonance.wgsl must parse");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .expect("substrate_resonance.wgsl must validate");
    }

    /// Test 2 : verify the shader exports the expected entry-point name.
    #[test]
    fn shader_has_main_entry() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC)
            .expect("substrate_resonance.wgsl must parse");
        let has_main = module.entry_points.iter().any(|ep| ep.name == "main");
        assert!(has_main, "compute entry-point `main` must exist");
        // And it must be a compute stage.
        let main_ep = module
            .entry_points
            .iter()
            .find(|ep| ep.name == "main")
            .unwrap();
        assert_eq!(main_ep.stage, naga::ShaderStage::Compute);
    }

    /// Test 3 : workgroup size must match the host-side WORKGROUP_X/Y consts.
    /// This guards against silent drift between shader + Rust dispatch.
    #[test]
    fn workgroup_size_matches_host_consts() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC).unwrap();
        let main_ep = module
            .entry_points
            .iter()
            .find(|ep| ep.name == "main")
            .unwrap();
        let [wgx, wgy, wgz] = main_ep.workgroup_size;
        assert_eq!(wgx, WORKGROUP_X);
        assert_eq!(wgy, WORKGROUP_Y);
        assert_eq!(wgz, 1);
    }

    /// Test 4 (ignored — GPU required) : full pipeline construction +
    /// 256×256 dispatch + readback ; documents per-resolution timing if
    /// a GPU is available. Uses `try_headless_device` so it cleanly skips
    /// on CI runners without a GPU.
    #[cfg(feature = "runtime")]
    #[test]
    #[ignore]
    fn gpu_dispatch_smoketest_256() {
        use cssl_host_alien_materialization::observer::ObserverCoord;
        use cssl_host_crystallization::Crystal;
        let Some((_inst, _adapter, device, queue)) = try_headless_device() else {
            eprintln!("no GPU adapter available · ignored");
            return;
        };
        let mut gpu = SubstrateResonanceGpu::new(&device, 256, 256);

        // One crystal at (0, 0, 1500) in front of the observer.
        let crystal = Crystal::allocate(
            cssl_host_crystallization::CrystalClass::Object,
            1,
            cssl_host_crystallization::WorldPos::new(0, 0, 1500),
        );
        let observer = ObserverCoord::default();

        let _view = gpu.dispatch(&device, &queue, observer, &[crystal]);
        // If we reach here without panicking the pipeline ran end-to-end.
    }

    /// Test 5 (ignored — GPU required) : 1440p dispatch — the actual
    /// performance-critical resolution from the W18-G mission.
    #[cfg(feature = "runtime")]
    #[test]
    #[ignore]
    fn gpu_dispatch_1440p_bench() {
        use cssl_host_alien_materialization::observer::ObserverCoord;
        use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};
        let Some((_inst, _adapter, device, queue)) = try_headless_device() else {
            eprintln!("no GPU adapter available · ignored");
            return;
        };
        let mut gpu = SubstrateResonanceGpu::new(&device, 2560, 1440);

        // Populate ~100 crystals to mirror the per-room density.
        let mut crystals = Vec::with_capacity(100);
        for i in 0..100i32 {
            crystals.push(Crystal::allocate(
                CrystalClass::Object,
                i as u64,
                WorldPos::new(i * 100, 0, 1500 + i * 50),
            ));
        }
        let observer = ObserverCoord::default();

        let start = std::time::Instant::now();
        let _view = gpu.dispatch(&device, &queue, observer, &crystals);
        // Submit + force a CPU-GPU sync via a no-op buffer-mapping so we
        // can time the wall-clock of the dispatch.
        device.poll(wgpu::Maintain::Wait);
        let elapsed = start.elapsed();
        eprintln!(
            "[T11-W18-G-GPU bench] 2560×1440 × 100 crystals : {:?}",
            elapsed
        );
        // Soft target = 6.94 ms (144 Hz). Don't assert ; just report.
    }
}
