//! § cssl-host-substrate-volumetric — VOXEL-CLOUD paradigm shift.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L5-VOXEL · canonical : `Labyrinth of Apocalypse/systems/volumetric_voxel_cloud.csl`
//!
//! § APOCKY-DIRECTIVE (verbatim · 2026-05-02)
//!
//! "completely novel and proprietary visual representation"
//! "more alien than rendering entirely"
//! "L4 + L5 + L6 NOW · why wait for mediocrity"
//!
//! § THESIS — WHY VOXEL-CLOUD ≠ PIXEL-FIELD
//!
//! `cssl-host-substrate-resonance-gpu` (T11-W18-G) implements the PIXEL-FIELD
//! paradigm : per-screen-pixel HDC-resonance integral. Looks like a flat 2D
//! image — every pixel is a thread that walks-the-ray-through-the-field and
//! writes RGBA. That's a virtuoso version of "rendering" but it's still
//! rendering.
//!
//! L5 is the OPPOSITE PARADIGM. Instead of each pixel SAMPLING the field, the
//! field's CELLS DIRECTLY EMIT into the framebuffer as 3D points. The scene is
//! a sparse VOLUMETRIC VOXEL-CLOUD that the player WALKS THROUGH. Crystals are
//! visualized as dense-cell-clusters. The view is constructed by direct splat
//! of every active ω-field cell — no surface, no triangle, no mesh.
//!
//! § WHAT MAKES IT "MORE ALIEN THAN RENDERING"
//!
//!   - There is NO image. The output is a 3D POINT-CLOUD with per-cell
//!     spectral emission. The framebuffer is incidental — a future channel
//!     could project these directly to a volumetric display.
//!   - There is NO sampling. Each cell knows its own (x,y,z + spectrum) and
//!     contributes itself. The contribution is independent of the camera —
//!     the camera transform is just a final view-projection on the cloud.
//!   - There is NO scene-object. The cloud IS the scene. Crystals are
//!     emergent — they appear as denser regions of the cloud where many
//!     cells share an HDC-resonance.
//!   - Walking through the field literally moves the camera AMONG the
//!     cells. Cells parallax against each other in 3D. Phase-coherent
//!     crystals glow their spectrum into the volume.
//!
//! § PIPELINE
//!
//! 1. `build_voxel_cloud(crystals: &[Crystal]) -> VoxelCloudHandle`
//!    Walks each crystal's HDC + spectral + world-pos and EMITS a sparse
//!    cell-set : one VoxelPoint per density-sample inside the crystal's
//!    extent. Crystals contribute DENSE point-clusters. Empty regions
//!    contribute nothing (true sparsity).
//!
//! 2. `dispatch_volumetric(...)` (wgpu-runtime feature)
//!    Uploads the voxel-cloud as a `vec4<f32>` storage buffer, dispatches a
//!    splat compute-shader that writes per-cell RGBA into the output
//!    framebuffer with phase-coherent crystal contribution.
//!
//! § DETERMINISM
//!
//! - Crystal → voxel-cluster derivation is purely from `crystal.fingerprint`
//!   + `crystal.spectral` + `crystal.hdc`. No globals, no rng. Replay-safe.
//! - Per-frame splat order = cloud-buffer order = stable across runs.
//! - Σ-mask consent : if observer or crystal denies the silhouette aspect,
//!   the crystal contributes ZERO cells to the cloud. No fallback.
//!
//! § CONSENT (PRIME-DIRECTIVE)
//!
//! Σ-mask gating runs at `build_voxel_cloud` time : crystals with denied
//! silhouette are EXCLUDED from the cloud entirely. There is no rendering-
//! path that bypasses this gate. The cloud-handle itself records the gate
//! decisions for replay-attestation.
//!
//! § INTEGRATION
//!
//! The same `Crystal` type that drives `cssl-host-substrate-resonance-gpu`
//! drives this crate. Both can run simultaneously — the pixel-field path
//! produces a 2D framebuffer, the voxel-cloud path produces a 3D point-set
//! that can be projected with arbitrary camera transforms. They are
//! complementary visual representations of the SAME ω-field state.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::pub_underscore_fields)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_arguments)]

pub mod cloud;
pub mod splat;
pub mod voxel;

pub use cloud::{
    build_voxel_cloud, build_voxel_cloud_with_observer, CloudStats, VoxelCloudHandle,
    DEFAULT_CRYSTAL_DENSITY, DEFAULT_ENV_DENSITY, MAX_CLOUD_POINTS,
};
pub use splat::{pack_voxel_cloud, GpuVoxelCameraUniform, GpuVoxelPoint};
pub use voxel::{VoxelEmission, VoxelPoint, VOXEL_POINT_BYTES};

/// Crate-version stamp ; recorded in audit + telemetry.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

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
pub const SHADER_SRC: &str = include_str!("../shaders/volumetric_splat.wgsl");

/// Workgroup size constants (must match `@workgroup_size(64, 1, 1)` in WGSL).
/// Splatting is one-thread-per-voxel-point so a 1D dispatch fits naturally.
pub const WORKGROUP_X: u32 = 64;

/// Initial GpuVoxelPoint buffer size in elements. Resized on demand.
pub const INITIAL_CAPACITY: u32 = 16_384;

// ════════════════════════════════════════════════════════════════════════════
// § VolumetricVoxelCloud — the host wrapper around the wgpu splat pipeline.
// ════════════════════════════════════════════════════════════════════════════

/// Wraps : the splat compute-pipeline + bind-group + resizable storage buffer
/// for voxel-points + camera-uniform buffer + RGBA8 output texture. One
/// instance per (width, height) framebuffer. Re-create on resize.
#[cfg(feature = "runtime")]
#[allow(clippy::struct_field_names)]
pub struct VolumetricVoxelCloud {
    width: u32,
    height: u32,
    /// Capacity in `GpuVoxelPoint` elements (resized on demand).
    capacity: u32,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    points_buf: wgpu::Buffer,
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
}

#[cfg(feature = "runtime")]
impl VolumetricVoxelCloud {
    /// Build a new splat pipeline at the given framebuffer resolution.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("volumetric_voxel_cloud::splat"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER_SRC)),
        });

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("volumetric_voxel_cloud::output"),
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
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("volumetric_voxel_cloud::camera"),
            size: GpuVoxelCameraUniform::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let capacity = INITIAL_CAPACITY;
        let points_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("volumetric_voxel_cloud::points"),
            size: u64::from(capacity) * GpuVoxelPoint::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("volumetric_voxel_cloud::bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                GpuVoxelCameraUniform::SIZE_BYTES as u64,
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(GpuVoxelPoint::SIZE_BYTES as u64),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("volumetric_voxel_cloud::pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("volumetric_voxel_cloud::compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("splat_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            width,
            height,
            capacity,
            pipeline,
            bind_group_layout,
            camera_buf,
            points_buf,
            output_texture,
            output_view,
        }
    }

    pub const fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub const fn output_texture(&self) -> &wgpu::Texture {
        &self.output_texture
    }

    pub const fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, n: u32) {
        if n <= self.capacity {
            return;
        }
        let new_cap = n.next_power_of_two().max(INITIAL_CAPACITY);
        self.points_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("volumetric_voxel_cloud::points(resized)"),
            size: u64::from(new_cap) * GpuVoxelPoint::SIZE_BYTES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.capacity = new_cap;
    }

    /// Dispatch the splat compute-shader. Builds the cloud from the crystal
    /// slice (Σ-mask-gated), uploads it, and runs `(ceil(n/64), 1, 1)`
    /// workgroups to splat each voxel-point onto the output texture.
    ///
    /// Returns a borrow of the output texture-view (rgba8unorm).
    pub fn dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        observer: ObserverCoord,
        crystals: &[Crystal],
    ) -> &wgpu::TextureView {
        let cloud = build_voxel_cloud_with_observer(crystals, &observer);

        self.ensure_capacity(device, cloud.points.len() as u32);

        let camera_uniform = splat::pack_camera_uniform(
            &observer,
            self.width,
            self.height,
            cloud.points.len() as u32,
        );
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera_uniform));

        if !cloud.points.is_empty() {
            let packed = pack_voxel_cloud(&cloud);
            queue.write_buffer(&self.points_buf, 0, bytemuck::cast_slice(&packed));
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("volumetric_voxel_cloud::bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.points_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.output_view),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("volumetric_voxel_cloud::encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("volumetric_voxel_cloud::pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let n = cloud.points.len() as u32;
            let wg_x = n.div_ceil(WORKGROUP_X).max(1);
            cpass.dispatch_workgroups(wg_x, 1, 1);
        }
        queue.submit(Some(encoder.finish()));

        &self.output_view
    }
}

/// Headless GPU device acquisition for tests + bench harnesses.
#[cfg(feature = "runtime")]
pub fn try_headless_device() -> Option<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue)>
{
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
    let (device, queue) = pollster::block_on(
        adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("volumetric_voxel_cloud::headless"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ),
    )
    .ok()?;
    Some((instance, adapter, device, queue))
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};

    /// Test 1 : `build_voxel_cloud` returns a non-empty handle for visible
    /// crystals + an empty cloud for an empty input.
    #[test]
    fn build_voxel_cloud_basic() {
        let cloud_empty = build_voxel_cloud(&[]);
        assert_eq!(cloud_empty.points.len(), 0);
        assert_eq!(cloud_empty.stats.cells_emitted, 0);

        let crystals = vec![
            Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500)),
            Crystal::allocate(CrystalClass::Environment, 2, WorldPos::new(2000, 0, 3000)),
        ];
        let cloud = build_voxel_cloud(&crystals);
        assert!(
            !cloud.points.is_empty(),
            "two visible crystals must emit cells"
        );
        assert_eq!(cloud.stats.cells_emitted as usize, cloud.points.len());
        assert_eq!(cloud.stats.crystals_in, 2);
    }

    /// Test 2 : per-crystal cell emission is DETERMINISTIC. Same inputs ⇒
    /// identical voxel-cloud (same cell-count + same per-cell RGB+pos).
    #[test]
    fn cell_emission_deterministic() {
        let crystals = vec![
            Crystal::allocate(CrystalClass::Object, 7, WorldPos::new(100, 200, 300)),
            Crystal::allocate(CrystalClass::Aura, 11, WorldPos::new(-100, 50, 400)),
        ];
        let a = build_voxel_cloud(&crystals);
        let b = build_voxel_cloud(&crystals);
        assert_eq!(a.points.len(), b.points.len());
        for (pa, pb) in a.points.iter().zip(b.points.iter()) {
            assert_eq!(pa.world_x_mm, pb.world_x_mm);
            assert_eq!(pa.world_y_mm, pb.world_y_mm);
            assert_eq!(pa.world_z_mm, pb.world_z_mm);
            assert_eq!(pa.emission.rgb, pb.emission.rgb);
        }
        // Cloud fingerprints must match (replay-equality).
        assert_eq!(a.fingerprint, b.fingerprint);
    }

    /// Test 3 : Environment crystals (with the bigger extent) emit MORE
    /// cells than Object crystals. Density scales with extent.
    #[test]
    fn cloud_density_scales_with_extent() {
        let obj = vec![Crystal::allocate(
            CrystalClass::Object,
            42,
            WorldPos::new(0, 0, 1500),
        )];
        let env = vec![Crystal::allocate(
            CrystalClass::Environment,
            42,
            WorldPos::new(0, 0, 1500),
        )];
        let cloud_obj = build_voxel_cloud(&obj);
        let cloud_env = build_voxel_cloud(&env);
        assert!(
            cloud_env.points.len() > cloud_obj.points.len(),
            "environment crystals must emit more cells than objects (env={} obj={})",
            cloud_env.points.len(),
            cloud_obj.points.len()
        );
    }

    /// Test 4 : A Σ-mask-revoked crystal contributes ZERO cells. Empty
    /// observer mask similarly excludes all crystals.
    #[test]
    fn sigma_mask_revoke_excludes_crystal() {
        let mut c = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let baseline = build_voxel_cloud(std::slice::from_ref(&c));
        assert!(!baseline.points.is_empty());

        // Revoke the silhouette aspect : crystal should drop out entirely.
        c.revoke_aspect(0);
        let revoked = build_voxel_cloud(std::slice::from_ref(&c));
        assert_eq!(revoked.points.len(), 0);
        assert_eq!(revoked.stats.crystals_gated_out, 1);
    }

    /// Test 5 : Per-frame stability — re-running build_voxel_cloud on the
    /// same crystal slice yields a bit-identical cloud-fingerprint. This
    /// is the replay-determinism contract.
    #[test]
    fn per_frame_stable_fingerprint() {
        let crystals: Vec<Crystal> = (0..16)
            .map(|i| {
                Crystal::allocate(
                    if i % 2 == 0 {
                        CrystalClass::Object
                    } else {
                        CrystalClass::Aura
                    },
                    i as u64,
                    WorldPos::new((i * 200) - 1600, 0, 1500 + i * 50),
                )
            })
            .collect();

        let frame_a = build_voxel_cloud(&crystals);
        let frame_b = build_voxel_cloud(&crystals);
        let frame_c = build_voxel_cloud(&crystals);
        assert_eq!(frame_a.fingerprint, frame_b.fingerprint);
        assert_eq!(frame_b.fingerprint, frame_c.fingerprint);

        // Re-ordering crystals SHOULD change the fingerprint (per-frame
        // order is part of the replay contract).
        let mut shuffled = crystals.clone();
        shuffled.reverse();
        let frame_d = build_voxel_cloud(&shuffled);
        assert_ne!(frame_a.fingerprint, frame_d.fingerprint);
    }

    /// Test 6 : Empty-field invariant — the air case (no crystals) yields
    /// an empty but well-formed cloud-handle.
    #[test]
    fn empty_field_well_formed() {
        let cloud = build_voxel_cloud(&[]);
        assert_eq!(cloud.points.len(), 0);
        assert_eq!(cloud.stats.cells_emitted, 0);
        assert_eq!(cloud.stats.crystals_in, 0);
        assert_eq!(cloud.stats.crystals_gated_out, 0);
        // Empty cloud uses a sentinel fingerprint so callers can detect it.
        assert_eq!(cloud.fingerprint, cloud::EMPTY_CLOUD_FINGERPRINT);
    }

    /// Test 7 : Shader passes naga validation.
    #[test]
    fn shader_passes_naga_validation() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC)
            .expect("volumetric_splat.wgsl must parse");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .expect("volumetric_splat.wgsl must validate");
    }

    /// Test 8 : Shader exports the `splat_main` entry-point.
    #[test]
    fn shader_has_splat_main_entry() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC).unwrap();
        let has_splat = module
            .entry_points
            .iter()
            .any(|ep| ep.name == "splat_main");
        assert!(has_splat, "compute entry-point `splat_main` must exist");
        let main_ep = module
            .entry_points
            .iter()
            .find(|ep| ep.name == "splat_main")
            .unwrap();
        assert_eq!(main_ep.stage, naga::ShaderStage::Compute);
    }

    /// Test 9 : Workgroup size matches host-side WORKGROUP_X.
    #[test]
    fn workgroup_size_matches_host_consts() {
        let module = naga::front::wgsl::parse_str(SHADER_SRC).unwrap();
        let main_ep = module
            .entry_points
            .iter()
            .find(|ep| ep.name == "splat_main")
            .unwrap();
        let [wgx, wgy, wgz] = main_ep.workgroup_size;
        assert_eq!(wgx, WORKGROUP_X);
        assert_eq!(wgy, 1);
        assert_eq!(wgz, 1);
    }

    /// Test 10 : Voxel-point byte size + alignment match the GPU expectation.
    #[test]
    fn voxel_point_layout() {
        assert_eq!(GpuVoxelPoint::SIZE_BYTES, 32);
        assert_eq!(std::mem::size_of::<GpuVoxelPoint>(), 32);
        assert_eq!(std::mem::align_of::<GpuVoxelPoint>(), 4);
    }

    /// Test 11 : Crate-version stamp is populated.
    #[test]
    fn stage0_scaffold_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
