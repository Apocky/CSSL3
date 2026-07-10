//! § cssl-host-substrate-render-v3 — ash-direct vulkan-1.3 substrate-render.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L7-SPIRV-DIRECT · the L6-V2 stack got rid of the render-pipeline
//! (vertex+fragment+rasterizer) ; this V3 stack goes one level deeper and
//! gets rid of *wgpu itself* — including naga, WGSL, the pipeline-builder,
//! and every wgpu-specific abstraction layer. The chain is now :
//!
//! ```text
//! Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl
//!         │  (csslc — proprietary compiler)
//!         ▼
//! cssl-cgen-spirv : Vec<u32>  — canonical SPIR-V 1.5 binary words
//!         │  (no rspirv on this path · no naga · no WGSL)
//!         ▼
//! cssl-cgen-gpu-spirv::emit_substrate_kernel_spirv  — orchestrator
//!         │  (runtime emit · or compile-time bake via build.rs in callers)
//!         ▼
//! ash::vk::Device::create_shader_module(&pCode = words)
//!         │
//!         ▼
//! ash-direct vk-pipeline + descriptor-set + command-buffer
//!         │
//!         ▼
//! one vkCmdDispatch per frame + vkCmdCopyImage to swapchain
//! ```
//!
//! § PROPRIETARY-EVERYTHING (§ I> spec/14_BACKEND § OWNED SPIR-V EMITTER)
//!   - Source-of-truth : `Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl`
//!   - Compiler : `cssl-cgen-spirv` (from-scratch SPIR-V binary · zero ext-dep)
//!   - GPU API : `ash` 0.38 (Vulkan-1.3 raw bindings · single dep · loader-only)
//!   - NO wgpu · NO naga · NO WGSL · NO pipeline-builder vendor-abstractions
//!
//! § HEADLESS-FIRST DESIGN
//!   The v3 crate exposes :
//!   - [`SubstrateKernelArtifact`] — the SPIR-V binary words emitted from
//!     the substrate-kernel `.csl` source. Available WITHOUT the `runtime`
//!     feature ; Tests 1+2 verify the emit path on any CI runner.
//!   - [`AshSubstrateRenderer`] (gated behind `runtime` feature) — the
//!     ash-direct vulkan-1.3 host wrapper. Constructs Instance · PhysicalDevice
//!     · Device · ShaderModule · DescriptorSetLayout · PipelineLayout ·
//!     ComputePipeline · CommandPool. Tests 3+4+5 exercise it WHEN a vulkan
//!     loader is present ; cleanly skip otherwise (returning `None` from
//!     [`try_headless_ash_renderer`]).
//!
//! § DETERMINISM (§ Apocky-directive)
//!   Same `(SubstrateKernelSpec)` ⇒ byte-identical SPIR-V (verified by
//!   `cssl-cgen-gpu-spirv::substrate_kernel::tests::emit_is_deterministic`).
//!   Same dispatch on the same device ⇒ byte-identical output image
//!   (Test #5 = `per_frame_determinism`, gated behind `runtime`).
//!
//! § PRIME-DIRECTIVE
//!   Σ-mask consent gating is encoded structurally in the substrate-kernel
//!   `.csl` source (§ ω-FIELD § Σ-mask-check W! consent-gate). The host
//!   never bypasses the kernel — there is exactly one compute path, exactly
//!   one shader module, exactly one entry-point.

// § Crate-level safety policy — the default-build path holds
// `forbid(unsafe_code)`. The optional `runtime` feature opts a single inner
// module into `unsafe_code` for the direct vulkan FFI calls that ash exposes.
// Without `runtime`, this crate is fully unsafe-free.
#![cfg_attr(not(feature = "runtime"), forbid(unsafe_code))]
#![cfg_attr(feature = "runtime", deny(unsafe_code))]
#![allow(clippy::module_name_repetitions)]

use cssl_cgen_gpu_spirv::{
    emit_substrate_kernel_spirv, emit_substrate_kernel_spirv_bytes, SubstrateKernelEmitError,
    SubstrateKernelSpec, MAX_SCENE_CRYSTALS as CSSL_MAX_SCENE_CRYSTALS,
};

/// Maximum active crystal slots accepted by the canonical CSSL substrate
/// kernel storage buffer.
pub const MAX_SCENE_CRYSTALS: usize = CSSL_MAX_SCENE_CRYSTALS;

// ════════════════════════════════════════════════════════════════════════════
// § SubstrateKernelArtifact — the compiled SPIR-V binary, available without
// any GPU dep. Carries enough metadata to drive vkCreateShaderModule but no
// vulkan handles itself.
// ════════════════════════════════════════════════════════════════════════════

/// § The emitted SPIR-V artifact for the substrate-kernel.
///
/// Construct via [`SubstrateKernelArtifact::compile`]. Carries the raw u32
/// word stream, the original spec, and convenience accessors for
/// `vkCreateShaderModule` consumption.
#[derive(Debug, Clone)]
pub struct SubstrateKernelArtifact {
    /// The spec the artifact was compiled from. Carried so callers can
    /// inspect entry-name / workgroup at runtime.
    spec: SubstrateKernelSpec,
    /// Canonical SPIR-V 1.5 binary words.
    words: Vec<u32>,
}

impl SubstrateKernelArtifact {
    /// § Compile the canonical substrate-kernel `.csl` source to SPIR-V.
    ///
    /// `Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl` is the
    /// source-of-truth ; the canonical spec it declares is available via
    /// [`SubstrateKernelSpec::canonical`].
    ///
    /// § ERRORS
    ///   Forwards [`SubstrateKernelEmitError`] from the SPIR-V backend.
    pub fn compile(spec: SubstrateKernelSpec) -> Result<Self, SubstrateKernelEmitError> {
        let words = emit_substrate_kernel_spirv(&spec)?;
        Ok(Self { spec, words })
    }

    /// § Convenience : compile the canonical spec from
    /// `substrate_v2_kernel.csl`.
    pub fn compile_canonical() -> Result<Self, SubstrateKernelEmitError> {
        Self::compile(SubstrateKernelSpec::canonical())
    }

    /// Borrow the spec.
    #[must_use]
    pub const fn spec(&self) -> &SubstrateKernelSpec {
        &self.spec
    }

    /// Borrow the SPIR-V word stream (1 word = 4 bytes ; little-endian).
    #[must_use]
    pub fn words(&self) -> &[u32] {
        &self.words
    }

    /// Total byte length of the SPIR-V binary (= `words.len() * 4`).
    /// This is what `VkShaderModuleCreateInfo::code_size` expects.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.words.len() * 4
    }

    /// SPIR-V magic `0x07230203` from word 0. Verifies the artifact is a
    /// well-formed SPIR-V binary (cheap structural check before passing to
    /// `vkCreateShaderModule`).
    #[must_use]
    pub fn magic(&self) -> u32 {
        self.words.first().copied().unwrap_or(0)
    }

    /// SPIR-V version word from word 1.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.words.get(1).copied().unwrap_or(0)
    }

    /// SPIR-V `bound` (max-id + 1) word from word 3.
    #[must_use]
    pub fn id_bound(&self) -> u32 {
        self.words.get(3).copied().unwrap_or(0)
    }

    /// Expected RGBA8 v13 probe emitted by the current substrate kernel.
    #[must_use]
    pub fn expected_v13_probe_rgba8(&self) -> [u8; 4] {
        self.spec.v13_probe_rgba8()
    }

    /// Expected RGBA8 v13 probe when observer.width is bound in b0.
    #[must_use]
    pub fn expected_v13_probe_rgba8_for_observer_width(&self, observer_width: u32) -> [u8; 4] {
        self.spec.v13_probe_rgba8_for_observer_width(observer_width)
    }

    /// Expected RGBA8 v13 probe when observer + crystal descriptors are bound.
    #[must_use]
    pub fn expected_v13_probe_rgba8_for_descriptors(
        &self,
        observer_width: u32,
        crystal_salt: u32,
    ) -> [u8; 4] {
        self.spec
            .v13_probe_rgba8_for_descriptors(observer_width, crystal_salt)
    }

    /// Expected first two RGBA8 pixels for the gid-driven probe row.
    #[must_use]
    pub fn expected_v13_probe_row_prefix_rgba8(&self) -> [[u8; 4]; 2] {
        self.spec.v13_probe_row_prefix_rgba8()
    }

    /// Expected first two RGBA8 pixels when observer.width is bound in b0.
    #[must_use]
    pub fn expected_v13_probe_row_prefix_rgba8_for_observer_width(
        &self,
        observer_width: u32,
    ) -> [[u8; 4]; 2] {
        self.spec
            .v13_probe_row_prefix_rgba8_for_observer_width(observer_width)
    }

    /// Expected first two RGBA8 pixels when observer + crystal descriptors are bound.
    #[must_use]
    pub fn expected_v13_probe_row_prefix_rgba8_for_descriptors(
        &self,
        observer_width: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 2] {
        self.spec
            .v13_probe_row_prefix_rgba8_for_descriptors(observer_width, crystal_salt)
    }

    /// Expected 8-pixel camera row when observer + crystal descriptors are bound.
    #[must_use]
    pub fn expected_v13_camera_row8_rgba8_for_descriptors(
        &self,
        observer_width: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 8] {
        self.spec
            .v13_camera_row8_rgba8_for_descriptors(observer_width, crystal_salt)
    }

    /// Expected 8-pixel camera row for observer yaw and first-crystal strength.
    #[must_use]
    pub fn expected_v13_camera_row8_rgba8_for_inputs(
        &self,
        observer_width: u32,
        yaw_milli: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 8] {
        self.spec
            .v13_camera_row8_rgba8_for_inputs(observer_width, yaw_milli, crystal_salt)
    }

    /// Expected 8x8 camera tile for observer size/yaw and first-crystal strength.
    #[must_use]
    pub fn expected_v13_camera_tile8_rgba8_for_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 64] {
        self.spec
            .v13_camera_tile8_rgba8_for_inputs(observer_size, yaw_milli, crystal_salt)
    }

    /// Expected 8x8 camera tile for observer pose and first-crystal words.
    #[must_use]
    pub fn expected_v13_camera_tile8_rgba8_for_scene_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_words: [u32; 4],
    ) -> [[u8; 4]; 64] {
        self.spec
            .v13_camera_tile8_rgba8_for_scene_inputs(observer_size, yaw_milli, crystal_words)
    }

    /// Expected 8x8 camera tile for observer pose and two crystal slots.
    #[must_use]
    pub fn expected_v13_camera_tile8_rgba8_for_scene2_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_words: [[u32; 4]; 2],
    ) -> [[u8; 4]; 64] {
        self.spec
            .v13_camera_tile8_rgba8_for_scene2_inputs(observer_size, yaw_milli, crystal_words)
    }

    /// Expected 8x8 camera tile for observer pose and bounded scene crystals.
    #[must_use]
    pub fn expected_v13_camera_tile8_rgba8_for_scene_slots_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_count: u32,
        crystal_words: [[u32; 4]; CSSL_MAX_SCENE_CRYSTALS],
    ) -> [[u8; 4]; 64] {
        self.spec.v13_camera_tile8_rgba8_for_scene_slots_inputs(
            observer_size,
            yaw_milli,
            crystal_count,
            crystal_words,
        )
    }

    /// Serialize to little-endian byte buffer. Useful for on-disk caching
    /// or vendor SPIR-V tooling round-trips.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SubstrateKernelEmitError> {
        emit_substrate_kernel_spirv_bytes(&self.spec)
    }
}

/// § Magic SPIR-V word 0 from Khronos § 2.3. Re-exported here so tests +
/// downstream callers can structurally validate without pulling
/// `cssl-cgen-spirv` directly.
pub const SPIRV_MAGIC: u32 = 0x0723_0203;

// ════════════════════════════════════════════════════════════════════════════
// § AshSubstrateRenderer — ash-direct vulkan-1.3 host wrapper.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "runtime")]
mod ash_runtime {
    //! § The ash-direct vulkan-1.3 path.
    //!
    //! All vulkan-loader interaction is gated behind the `runtime` feature so
    //! the default crate build doesn't pull `ash` (and the implicit dynamic-
    //! library link to `vulkan-1.dll` / `libvulkan.so` / `libMoltenVK.dylib`).
    //!
    //! § SAFETY
    //! The ash bindings expose `unsafe` for vulkan calls. `cssl-host-substrate-
    //! render-v3` holds `#![forbid(unsafe_code)]` at the crate root ; the
    //! single `mod` below opts into local `#[allow(unsafe_code)]` for the
    //! direct vulkan calls. The opt-in is bounded to this module.
    #![allow(unsafe_code)]
    #![allow(clippy::missing_safety_doc)]

    use super::SubstrateKernelArtifact;
    use ash::vk;
    use ash::vk::Handle;

    const HEADLESS_PROBE_WIDTH_USIZE: usize = 8;
    const HEADLESS_PROBE_HEIGHT_USIZE: usize = 8;
    const HEADLESS_PROBE_PIXELS_USIZE: usize =
        HEADLESS_PROBE_WIDTH_USIZE * HEADLESS_PROBE_HEIGHT_USIZE;
    const MAX_SCENE_CRYSTALS: usize = super::CSSL_MAX_SCENE_CRYSTALS;
    pub(crate) const HEADLESS_PROBE_WIDTH: u32 = HEADLESS_PROBE_WIDTH_USIZE as u32;
    pub(crate) const HEADLESS_PROBE_HEIGHT: u32 = HEADLESS_PROBE_HEIGHT_USIZE as u32;
    pub(crate) const HEADLESS_OBSERVER_YAW_MILLI: u32 = 500;
    pub(crate) const HEADLESS_CRYSTAL_COUNT: u32 = 17;
    pub(crate) const HEADLESS_CRYSTAL_SALT: u32 = 5;
    pub(crate) fn headless_crystal_words() -> [[u32; 4]; MAX_SCENE_CRYSTALS] {
        let mut words = [[0, 128, 128, 0]; MAX_SCENE_CRYSTALS];
        words[..HEADLESS_CRYSTAL_COUNT as usize].copy_from_slice(&[
            [HEADLESS_CRYSTAL_SALT, 160, 96, 0],
            [4, 96, 160, 0],
            [3, 128, 128, 0],
            [2, 192, 64, 0],
            [1, 144, 112, 0],
            [5, 112, 144, 0],
            [4, 80, 176, 0],
            [3, 176, 80, 0],
            [2, 152, 104, 0],
            [1, 104, 152, 0],
            [5, 136, 120, 0],
            [4, 120, 136, 0],
            [3, 168, 88, 0],
            [2, 88, 168, 0],
            [1, 156, 100, 0],
            [5, 100, 156, 0],
            [4, 132, 124, 0],
        ]);
        words
    }

    /// § Headless probe output plus instrumentation.
    #[derive(Debug, Clone)]
    pub struct HeadlessProbeFrame {
        pixels: Vec<[u8; 4]>,
        gpu_elapsed_ns: Option<u64>,
    }

    impl HeadlessProbeFrame {
        #[must_use]
        pub fn pixels(&self) -> &[[u8; 4]] {
            &self.pixels
        }

        #[must_use]
        pub fn into_pixels(self) -> Vec<[u8; 4]> {
            self.pixels
        }

        #[must_use]
        pub const fn gpu_elapsed_ns(&self) -> Option<u64> {
            self.gpu_elapsed_ns
        }
    }

    struct HeadlessProbeSession {
        width: u32,
        height: u32,
        pixel_count: usize,
        readback_bytes: usize,
        readback_bytes_u64: u64,
        observer_buf: vk::Buffer,
        observer_mem: vk::DeviceMemory,
        crystal_buf: vk::Buffer,
        crystal_mem: vk::DeviceMemory,
        readback_buf: vk::Buffer,
        readback_mem: vk::DeviceMemory,
        image: vk::Image,
        image_mem: vk::DeviceMemory,
        image_view: vk::ImageView,
        image_layout: vk::ImageLayout,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set: vk::DescriptorSet,
        command_pool: vk::CommandPool,
        command_buffer: vk::CommandBuffer,
        query_pool: Option<vk::QueryPool>,
        timestamp_period_ns: f32,
    }

    /// § One ash-direct vulkan-1.3 substrate-renderer.
    ///
    /// Owns the Instance · PhysicalDevice · Device · ShaderModule built from
    /// the substrate-kernel SPIR-V ; pipeline construction is performed
    /// lazily on first frame or eagerly via [`AshSubstrateRenderer::build_pipeline`].
    pub struct AshSubstrateRenderer {
        /// The ash entry-point (loaded from the system Vulkan loader).
        entry: ash::Entry,
        /// Vulkan instance.
        instance: ash::Instance,
        /// Physical device chosen at construction.
        physical_device: vk::PhysicalDevice,
        /// Logical device.
        device: ash::Device,
        /// Compute queue family index.
        compute_queue_family: u32,
        /// Compute queue handle.
        compute_queue: vk::Queue,
        /// Shader module created from the substrate-kernel SPIR-V.
        shader_module: vk::ShaderModule,
        /// The original artifact ; carried for re-use + introspection.
        artifact: SubstrateKernelArtifact,
        /// Optional descriptor-set-layout (built lazily on first pipeline build).
        descriptor_set_layout: Option<vk::DescriptorSetLayout>,
        /// Optional pipeline-layout (built lazily on first pipeline build).
        pipeline_layout: Option<vk::PipelineLayout>,
        /// Optional compute pipeline.
        compute_pipeline: Option<vk::Pipeline>,
        /// Optional extent-keyed headless resources reused across telemetry frames.
        headless_probe_session: Option<HeadlessProbeSession>,
    }

    /// § Errors from the ash-direct path.
    #[derive(Debug, thiserror::Error)]
    pub enum AshError {
        #[error("vulkan loader not available : {0}")]
        Loader(#[from] ash::LoadingError),
        #[error("vulkan instance creation failed : {0}")]
        InstanceCreate(vk::Result),
        #[error("no vulkan physical device with compute queue available")]
        NoComputeDevice,
        #[error("vulkan device creation failed : {0}")]
        DeviceCreate(vk::Result),
        #[error("vulkan shader-module creation failed : {0}")]
        ShaderModuleCreate(vk::Result),
        #[error("vulkan descriptor-set-layout creation failed : {0}")]
        DescriptorSetLayoutCreate(vk::Result),
        #[error("vulkan pipeline-layout creation failed : {0}")]
        PipelineLayoutCreate(vk::Result),
        #[error("vulkan compute-pipeline creation failed : {0}")]
        ComputePipelineCreate(vk::Result),
        #[error("vulkan headless resource operation failed : {0}")]
        HeadlessResource(vk::Result),
        #[error("invalid headless extent {0}x{1}; must be nonzero and divisible by workgroup")]
        InvalidHeadlessExtent(u32, u32),
        #[error("no compatible vulkan memory type found for headless resource")]
        NoMemoryType,
    }

    impl AshSubstrateRenderer {
        /// § Try to construct an ash-direct vulkan renderer for the
        /// substrate-kernel.
        ///
        /// Loads the system Vulkan loader, creates an Instance, picks the
        /// first physical device with a compute queue, creates a Device,
        /// and uploads the substrate-kernel SPIR-V via vkCreateShaderModule.
        /// Pipeline creation is lazy ; call [`build_pipeline`] explicitly
        /// or call [`headless_dispatch`] which builds-on-first-use.
        ///
        /// § ERRORS
        ///   Returns [`AshError::Loader`] if vulkan-1.dll / libvulkan.so is
        ///   not installed ; this is the single test-skip point on
        ///   GPU-less CI runners.
        ///
        /// [`build_pipeline`]: AshSubstrateRenderer::build_pipeline
        /// [`headless_dispatch`]: AshSubstrateRenderer::headless_dispatch
        pub fn try_new(artifact: SubstrateKernelArtifact) -> Result<Self, AshError> {
            // 1. Load the vulkan loader.
            let entry = unsafe { ash::Entry::load()? };

            // 2. Create the instance.
            let app_name = c"cssl-host-substrate-render-v3";
            let app_info = vk::ApplicationInfo::default()
                .application_name(app_name)
                .application_version(0)
                .engine_name(app_name)
                .engine_version(0)
                .api_version(vk::make_api_version(0, 1, 3, 0));
            let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);
            let instance = unsafe {
                entry
                    .create_instance(&create_info, None)
                    .map_err(AshError::InstanceCreate)?
            };

            // 3. Pick a physical device with a compute queue.
            let physical_devices = unsafe {
                instance
                    .enumerate_physical_devices()
                    .map_err(|_| AshError::NoComputeDevice)?
            };
            let mut chosen: Option<(vk::PhysicalDevice, u32)> = None;
            for pd in physical_devices {
                let q_props = unsafe { instance.get_physical_device_queue_family_properties(pd) };
                for (i, q) in q_props.iter().enumerate() {
                    if q.queue_flags.contains(vk::QueueFlags::COMPUTE) {
                        chosen = Some((pd, i as u32));
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }
            let (physical_device, compute_queue_family) =
                chosen.ok_or(AshError::NoComputeDevice)?;

            // 4. Create the logical device + grab the compute queue.
            let queue_priorities = [1.0_f32];
            let queue_create_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(compute_queue_family)
                .queue_priorities(&queue_priorities)];
            let supported_features =
                unsafe { instance.get_physical_device_features(physical_device) };
            let mut enabled_features = vk::PhysicalDeviceFeatures::default();
            if supported_features.shader_storage_image_extended_formats == vk::TRUE {
                enabled_features.shader_storage_image_extended_formats = vk::TRUE;
            }
            let device_create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_create_infos)
                .enabled_features(&enabled_features);
            let device = unsafe {
                instance
                    .create_device(physical_device, &device_create_info, None)
                    .map_err(AshError::DeviceCreate)?
            };
            let compute_queue = unsafe { device.get_device_queue(compute_queue_family, 0) };

            // 5. Create the shader-module from the substrate-kernel SPIR-V.
            //
            //    `code_size` is in BYTES per the vulkan spec ; words.len()*4.
            let words = artifact.words();
            let shader_module_create_info = vk::ShaderModuleCreateInfo::default().code(words);
            let shader_module = unsafe {
                device
                    .create_shader_module(&shader_module_create_info, None)
                    .map_err(AshError::ShaderModuleCreate)?
            };

            Ok(Self {
                entry,
                instance,
                physical_device,
                device,
                compute_queue_family,
                compute_queue,
                shader_module,
                artifact,
                descriptor_set_layout: None,
                pipeline_layout: None,
                compute_pipeline: None,
                headless_probe_session: None,
            })
        }

        /// Borrow the underlying SPIR-V artifact.
        #[must_use]
        pub fn artifact(&self) -> &SubstrateKernelArtifact {
            &self.artifact
        }

        /// Compute queue-family index that the device + queue were created
        /// from.
        #[must_use]
        pub const fn compute_queue_family(&self) -> u32 {
            self.compute_queue_family
        }

        /// Whether the lazy pipeline has been built.
        #[must_use]
        pub const fn pipeline_built(&self) -> bool {
            self.compute_pipeline.is_some()
        }

        /// Current reusable headless probe extent, if a probe session exists.
        #[must_use]
        pub fn headless_probe_session_extent(&self) -> Option<(u32, u32)> {
            self.headless_probe_session
                .as_ref()
                .map(|session| (session.width, session.height))
        }

        /// Whether the active headless probe session can report GPU timestamps.
        #[must_use]
        pub fn headless_probe_gpu_timestamps_available(&self) -> bool {
            self.headless_probe_session
                .as_ref()
                .is_some_and(|session| session.query_pool.is_some())
        }

        /// § Build the descriptor-set-layout · pipeline-layout · compute-
        /// pipeline. Idempotent : safe to call more than once ; subsequent
        /// calls return Ok with no work.
        ///
        /// The descriptor-set-layout matches `substrate_v2_kernel.csl`
        /// § INPUTS exactly :
        /// - binding 0 : uniform buffer (observer)  · stage = COMPUTE
        /// - binding 1 : storage buffer (crystals)  · stage = COMPUTE
        /// - binding 2 : storage image (output)     · stage = COMPUTE
        pub fn build_pipeline(&mut self) -> Result<(), AshError> {
            if self.compute_pipeline.is_some() {
                return Ok(());
            }
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            ];
            let dsl_create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
            let dsl = unsafe {
                self.device
                    .create_descriptor_set_layout(&dsl_create_info, None)
                    .map_err(AshError::DescriptorSetLayoutCreate)?
            };
            let dsls = [dsl];
            let pl_create_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&dsls);
            let pl = unsafe {
                self.device
                    .create_pipeline_layout(&pl_create_info, None)
                    .map_err(AshError::PipelineLayoutCreate)?
            };
            // Compute pipeline.
            let entry_name = std::ffi::CString::new(self.artifact.spec().entry_name.clone())
                .expect("entry-name must be valid C-str");
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(self.shader_module)
                .name(&entry_name);
            let cp_create_info = vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(pl);
            let pipelines = unsafe {
                self.device
                    .create_compute_pipelines(vk::PipelineCache::null(), &[cp_create_info], None)
                    .map_err(|(_, r)| AshError::ComputePipelineCreate(r))?
            };
            let pipeline = pipelines[0];
            self.descriptor_set_layout = Some(dsl);
            self.pipeline_layout = Some(pl);
            self.compute_pipeline = Some(pipeline);
            Ok(())
        }

        /// § Headless dispatch — builds the pipeline if needed, runs an
        /// empty command-buffer that binds the pipeline + records one
        /// `vkCmdDispatch(1, 1, 1)`. Verifies the end-to-end ash path :
        /// shader-module → pipeline → command-buffer → submit → wait.
        ///
        /// Returns the pipeline handle as `u64` so tests can confirm a
        /// non-null pipeline was produced without exposing the raw vk
        /// type to safe callers.
        pub fn headless_dispatch(&mut self) -> Result<u64, AshError> {
            let _pixel = self.headless_first_pixel_rgba8()?;
            let pipeline = self
                .compute_pipeline
                .expect("headless_first_pixel_rgba8 guarantees Some");
            Ok(pipeline.as_raw())
        }

        /// § Headless v13 probe dispatch — writes an 8×8 private storage
        /// image, copies it into a host-visible readback buffer, and returns
        /// the first RGBA8 pixel.
        ///
        /// This is the first real GPU-output gate for the substrate path:
        /// `.csl` source declaration → SPIR-V storage-image shader →
        /// descriptor-bound Vulkan dispatch → readback bytes.
        pub fn headless_first_pixel_rgba8(&mut self) -> Result<[u8; 4], AshError> {
            Ok(self.headless_probe_row_rgba8()?[0])
        }

        /// § Headless camera row dispatch — writes an 8×8 private storage
        /// image, copies it into a host-visible readback buffer, and returns
        /// first-row RGBA8 pixels. `gid.x` maps through observer.width into a
        /// normalized camera ray.
        pub fn headless_probe_row_rgba8(
            &mut self,
        ) -> Result<[[u8; 4]; HEADLESS_PROBE_WIDTH_USIZE], AshError> {
            let tile = self.headless_probe_tile_rgba8()?;
            let mut row = [[0_u8; 4]; HEADLESS_PROBE_WIDTH_USIZE];
            row.copy_from_slice(&tile[..HEADLESS_PROBE_WIDTH_USIZE]);
            Ok(row)
        }

        /// § Headless camera tile dispatch — writes an 8×8 private storage
        /// image, copies it into a host-visible readback buffer, and returns
        /// all RGBA8 pixels in row-major order.
        pub fn headless_probe_tile_rgba8(
            &mut self,
        ) -> Result<[[u8; 4]; HEADLESS_PROBE_PIXELS_USIZE], AshError> {
            let tile = self
                .headless_probe_tile_rgba8_for_size(HEADLESS_PROBE_WIDTH, HEADLESS_PROBE_HEIGHT)?;
            let mut fixed = [[0_u8; 4]; HEADLESS_PROBE_PIXELS_USIZE];
            fixed.copy_from_slice(&tile);
            Ok(fixed)
        }

        /// § Headless camera dispatch for an arbitrary workgroup-aligned
        /// storage image. Returns row-major RGBA8 pixels.
        pub fn headless_probe_tile_rgba8_for_size(
            &mut self,
            width: u32,
            height: u32,
        ) -> Result<Vec<[u8; 4]>, AshError> {
            Ok(self
                .headless_probe_frame_rgba8_for_size(width, height)?
                .into_pixels())
        }

        /// § Headless camera dispatch with resource reuse + GPU timestamp
        /// instrumentation. End-to-end callers still get the full pixel
        /// readback, while telemetry can separate host overhead from GPU work.
        pub fn headless_probe_frame_rgba8_for_size(
            &mut self,
            width: u32,
            height: u32,
        ) -> Result<HeadlessProbeFrame, AshError> {
            let (wg_x, wg_y, _) = self.artifact.spec().workgroup;
            if width == 0
                || height == 0
                || wg_x == 0
                || wg_y == 0
                || width % wg_x != 0
                || height % wg_y != 0
            {
                return Err(AshError::InvalidHeadlessExtent(width, height));
            }
            let pixel_count_u64 = u64::from(width) * u64::from(height);
            let pixel_count = usize::try_from(pixel_count_u64)
                .map_err(|_| AshError::InvalidHeadlessExtent(width, height))?;
            let readback_bytes = pixel_count
                .checked_mul(4)
                .ok_or(AshError::InvalidHeadlessExtent(width, height))?;
            let readback_bytes_u64 = u64::try_from(readback_bytes)
                .map_err(|_| AshError::InvalidHeadlessExtent(width, height))?;

            self.build_pipeline()?;
            let pipeline = self
                .compute_pipeline
                .expect("build_pipeline guarantees Some");
            let pipeline_layout = self
                .pipeline_layout
                .expect("build_pipeline guarantees Some");

            let needs_rebuild = match self.headless_probe_session.as_ref() {
                Some(session) => session.width != width || session.height != height,
                None => true,
            };
            if needs_rebuild {
                if let Some(mut session) = self.headless_probe_session.take() {
                    unsafe { session.destroy(&self.device) };
                }
                self.headless_probe_session = Some(self.build_headless_probe_session(
                    width,
                    height,
                    pixel_count,
                    readback_bytes,
                    readback_bytes_u64,
                )?);
            }

            let device = &self.device;
            let compute_queue = self.compute_queue;
            let session = self
                .headless_probe_session
                .as_mut()
                .expect("headless_probe_session created above");
            session.dispatch_and_readback(
                device,
                compute_queue,
                pipeline,
                pipeline_layout,
                wg_x,
                wg_y,
            )
        }

        fn build_headless_probe_session(
            &self,
            width: u32,
            height: u32,
            pixel_count: usize,
            readback_bytes: usize,
            readback_bytes_u64: u64,
        ) -> Result<HeadlessProbeSession, AshError> {
            let descriptor_set_layout = self
                .descriptor_set_layout
                .expect("build_pipeline guarantees Some");
            let mem_props = unsafe {
                self.instance
                    .get_physical_device_memory_properties(self.physical_device)
            };

            let (observer_buf, observer_mem) = create_headless_buffer(
                &self.device,
                &mem_props,
                64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            write_headless_observer_uniform(
                &self.device,
                observer_mem,
                [
                    width,
                    height,
                    HEADLESS_OBSERVER_YAW_MILLI,
                    self.artifact.id_bound(),
                ],
            )?;
            let (crystal_buf, crystal_mem) = create_headless_buffer(
                &self.device,
                &mem_props,
                4096,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            write_headless_scene_crystals(
                &self.device,
                crystal_mem,
                HEADLESS_CRYSTAL_COUNT,
                &headless_crystal_words(),
            )?;
            let (readback_buf, readback_mem) = create_headless_buffer(
                &self.device,
                &mem_props,
                readback_bytes_u64,
                vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;

            let image_ci = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = unsafe {
                self.device
                    .create_image(&image_ci, None)
                    .map_err(AshError::HeadlessResource)?
            };
            let image_req = unsafe { self.device.get_image_memory_requirements(image) };
            let image_mem_type = find_headless_memory_type(
                &mem_props,
                image_req.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .or_else(|| {
                find_headless_memory_type(
                    &mem_props,
                    image_req.memory_type_bits,
                    vk::MemoryPropertyFlags::empty(),
                )
            })
            .ok_or(AshError::NoMemoryType)?;
            let image_alloc = vk::MemoryAllocateInfo::default()
                .allocation_size(image_req.size)
                .memory_type_index(image_mem_type);
            let image_mem = unsafe {
                self.device
                    .allocate_memory(&image_alloc, None)
                    .map_err(AshError::HeadlessResource)?
            };
            unsafe {
                self.device
                    .bind_image_memory(image, image_mem, 0)
                    .map_err(AshError::HeadlessResource)?;
            }
            let image_view_ci = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let image_view = unsafe {
                self.device
                    .create_image_view(&image_view_ci, None)
                    .map_err(AshError::HeadlessResource)?
            };

            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 1,
                },
            ];
            let pool_ci = vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&pool_sizes)
                .max_sets(1);
            let descriptor_pool = unsafe {
                self.device
                    .create_descriptor_pool(&pool_ci, None)
                    .map_err(AshError::HeadlessResource)?
            };
            let layouts = [descriptor_set_layout];
            let ds_alloc = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts);
            let descriptor_set = unsafe {
                self.device
                    .allocate_descriptor_sets(&ds_alloc)
                    .map_err(AshError::HeadlessResource)?[0]
            };
            let observer_info = [vk::DescriptorBufferInfo::default()
                .buffer(observer_buf)
                .offset(0)
                .range(64)];
            let crystal_info = [vk::DescriptorBufferInfo::default()
                .buffer(crystal_buf)
                .offset(0)
                .range(4096)];
            let image_info = [vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(image_view)];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&observer_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&crystal_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&image_info),
            ];
            unsafe { self.device.update_descriptor_sets(&writes, &[]) };

            // Allocate a command pool + buffer.
            let cp_create_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(self.compute_queue_family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            let cmd_pool = unsafe {
                self.device
                    .create_command_pool(&cp_create_info, None)
                    .map_err(AshError::HeadlessResource)?
            };
            let cb_alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(cmd_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd_buffers = unsafe {
                self.device
                    .allocate_command_buffers(&cb_alloc_info)
                    .map_err(AshError::HeadlessResource)?
            };
            let cb = cmd_buffers[0];

            let props = unsafe {
                self.instance
                    .get_physical_device_properties(self.physical_device)
            };
            let timestamp_period_ns = props.limits.timestamp_period;
            let query_pool = if props.limits.timestamp_compute_and_graphics == vk::TRUE
                && timestamp_period_ns > 0.0
            {
                let query_ci = vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(2);
                unsafe { self.device.create_query_pool(&query_ci, None).ok() }
            } else {
                None
            };

            Ok(HeadlessProbeSession {
                width,
                height,
                pixel_count,
                readback_bytes,
                readback_bytes_u64,
                observer_buf,
                observer_mem,
                crystal_buf,
                crystal_mem,
                readback_buf,
                readback_mem,
                image,
                image_mem,
                image_view,
                image_layout: vk::ImageLayout::UNDEFINED,
                descriptor_pool,
                descriptor_set,
                command_pool: cmd_pool,
                command_buffer: cb,
                query_pool,
                timestamp_period_ns,
            })
        }
    }

    impl HeadlessProbeSession {
        fn dispatch_and_readback(
            &mut self,
            device: &ash::Device,
            compute_queue: vk::Queue,
            pipeline: vk::Pipeline,
            pipeline_layout: vk::PipelineLayout,
            wg_x: u32,
            wg_y: u32,
        ) -> Result<HeadlessProbeFrame, AshError> {
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            unsafe {
                device
                    .reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())
                    .map_err(AshError::HeadlessResource)?;
                device
                    .begin_command_buffer(self.command_buffer, &begin_info)
                    .map_err(AshError::HeadlessResource)?;
                if let Some(query_pool) = self.query_pool {
                    device.cmd_reset_query_pool(self.command_buffer, query_pool, 0, 2);
                }
                let (pre_dispatch_src_stage, pre_dispatch_src_access) =
                    if self.image_layout == vk::ImageLayout::UNDEFINED {
                        (
                            vk::PipelineStageFlags::TOP_OF_PIPE,
                            vk::AccessFlags::empty(),
                        )
                    } else {
                        (
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_READ,
                        )
                    };
                let to_general = vk::ImageMemoryBarrier::default()
                    .src_access_mask(pre_dispatch_src_access)
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .old_layout(self.image_layout)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .image(self.image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                device.cmd_pipeline_barrier(
                    self.command_buffer,
                    pre_dispatch_src_stage,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_general],
                );
                if let Some(query_pool) = self.query_pool {
                    device.cmd_write_timestamp(
                        self.command_buffer,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        query_pool,
                        0,
                    );
                }
                device.cmd_bind_pipeline(
                    self.command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline,
                );
                device.cmd_bind_descriptor_sets(
                    self.command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline_layout,
                    0,
                    &[self.descriptor_set],
                    &[],
                );
                device.cmd_dispatch(
                    self.command_buffer,
                    self.width / wg_x,
                    self.height / wg_y,
                    1,
                );
                if let Some(query_pool) = self.query_pool {
                    device.cmd_write_timestamp(
                        self.command_buffer,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        query_pool,
                        1,
                    );
                }
                let to_transfer = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .image(self.image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                device.cmd_pipeline_barrier(
                    self.command_buffer,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_transfer],
                );
                let copy = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(0)
                            .base_array_layer(0)
                            .layer_count(1),
                    )
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width: self.width,
                        height: self.height,
                        depth: 1,
                    });
                device.cmd_copy_image_to_buffer(
                    self.command_buffer,
                    self.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    self.readback_buf,
                    &[copy],
                );
                device
                    .end_command_buffer(self.command_buffer)
                    .map_err(AshError::HeadlessResource)?;
                let command_buffers = [self.command_buffer];
                let submits = [vk::SubmitInfo::default().command_buffers(&command_buffers)];
                device
                    .queue_submit(compute_queue, &submits, vk::Fence::null())
                    .map_err(AshError::HeadlessResource)?;
                device
                    .queue_wait_idle(compute_queue)
                    .map_err(AshError::HeadlessResource)?;
                self.image_layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
            }

            let mut tile = vec![[0_u8; 4]; self.pixel_count];
            unsafe {
                let ptr = device
                    .map_memory(
                        self.readback_mem,
                        0,
                        self.readback_bytes_u64,
                        vk::MemoryMapFlags::empty(),
                    )
                    .map_err(AshError::HeadlessResource)?;
                let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), self.readback_bytes);
                for (dst, src) in tile.iter_mut().zip(bytes.chunks_exact(4)) {
                    dst.copy_from_slice(src);
                }
                device.unmap_memory(self.readback_mem);
            }

            Ok(HeadlessProbeFrame {
                pixels: tile,
                gpu_elapsed_ns: self.read_gpu_elapsed_ns(device),
            })
        }

        fn read_gpu_elapsed_ns(&self, device: &ash::Device) -> Option<u64> {
            let query_pool = self.query_pool?;
            let mut ticks = [0_u64; 2];
            unsafe {
                device
                    .get_query_pool_results(
                        query_pool,
                        0,
                        &mut ticks,
                        vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                    )
                    .ok()?;
            }
            let elapsed_ticks = ticks[1].checked_sub(ticks[0])?;
            let elapsed_ns = (elapsed_ticks as f64) * f64::from(self.timestamp_period_ns);
            if elapsed_ns.is_finite() && elapsed_ns >= 0.0 {
                Some(elapsed_ns.round() as u64)
            } else {
                None
            }
        }

        unsafe fn destroy(&mut self, device: &ash::Device) {
            if let Some(query_pool) = self.query_pool.take() {
                device.destroy_query_pool(query_pool, None);
            }
            device.destroy_command_pool(self.command_pool, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_image_view(self.image_view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.image_mem, None);
            device.destroy_buffer(self.readback_buf, None);
            device.free_memory(self.readback_mem, None);
            device.destroy_buffer(self.crystal_buf, None);
            device.free_memory(self.crystal_mem, None);
            device.destroy_buffer(self.observer_buf, None);
            device.free_memory(self.observer_mem, None);
        }
    }

    fn create_headless_buffer(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        usage: vk::BufferUsageFlags,
        flags: vk::MemoryPropertyFlags,
    ) -> Result<(vk::Buffer, vk::DeviceMemory), AshError> {
        let bi = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buf = unsafe {
            device
                .create_buffer(&bi, None)
                .map_err(AshError::HeadlessResource)?
        };
        let req = unsafe { device.get_buffer_memory_requirements(buf) };
        let mt_idx = find_headless_memory_type(mem_props, req.memory_type_bits, flags)
            .ok_or(AshError::NoMemoryType)?;
        let ai = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mt_idx);
        let mem = unsafe {
            device
                .allocate_memory(&ai, None)
                .map_err(AshError::HeadlessResource)?
        };
        unsafe {
            device
                .bind_buffer_memory(buf, mem, 0)
                .map_err(AshError::HeadlessResource)?;
        }
        Ok((buf, mem))
    }

    fn find_headless_memory_type(
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        type_bits: u32,
        flags: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        (0..mem_props.memory_type_count).find(|i| {
            (type_bits & (1 << i)) != 0
                && mem_props.memory_types[*i as usize]
                    .property_flags
                    .contains(flags)
        })
    }

    fn write_headless_observer_uniform(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        words: [u32; 4],
    ) -> Result<(), AshError> {
        write_headless_descriptor_words(device, mem, words)
    }

    fn write_headless_descriptor_words(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        words: [u32; 4],
    ) -> Result<(), AshError> {
        write_headless_descriptor_word_sets(device, mem, &[words])
    }

    fn write_headless_descriptor_word_sets(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        word_sets: &[[u32; 4]],
    ) -> Result<(), AshError> {
        let byte_len = word_sets.len() * core::mem::size_of::<[u32; 4]>();
        if byte_len == 0 {
            return Ok(());
        }
        unsafe {
            let ptr = device
                .map_memory(mem, 0, byte_len as u64, vk::MemoryMapFlags::empty())
                .map_err(AshError::HeadlessResource)?;
            std::ptr::copy_nonoverlapping(
                word_sets.as_ptr().cast::<u8>(),
                ptr.cast::<u8>(),
                byte_len,
            );
            device.unmap_memory(mem);
        }
        Ok(())
    }

    fn write_headless_scene_crystals(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        crystal_count: u32,
        crystal_words: &[[u32; 4]; MAX_SCENE_CRYSTALS],
    ) -> Result<(), AshError> {
        let mut scene_words = [[0_u32; 4]; MAX_SCENE_CRYSTALS + 1];
        scene_words[0] = [crystal_count.min(MAX_SCENE_CRYSTALS as u32), 0, 0, 0];
        scene_words[1..].copy_from_slice(crystal_words);
        write_headless_descriptor_word_sets(device, mem, &scene_words)
    }

    impl Drop for AshSubstrateRenderer {
        fn drop(&mut self) {
            unsafe {
                if let Some(mut session) = self.headless_probe_session.take() {
                    session.destroy(&self.device);
                }
                if let Some(p) = self.compute_pipeline.take() {
                    self.device.destroy_pipeline(p, None);
                }
                if let Some(pl) = self.pipeline_layout.take() {
                    self.device.destroy_pipeline_layout(pl, None);
                }
                if let Some(dsl) = self.descriptor_set_layout.take() {
                    self.device.destroy_descriptor_set_layout(dsl, None);
                }
                self.device.destroy_shader_module(self.shader_module, None);
                self.device.destroy_device(None);
                self.instance.destroy_instance(None);
                // Drop entry last (no destructor — just dropping the dynamic
                // library handle).
                let _ = (&self.entry, &self.physical_device);
            }
        }
    }

    /// § Convenience : try to build a renderer for the canonical substrate-
    /// kernel. Returns `None` if (a) SPIR-V emit failed, OR (b) no vulkan
    /// loader / no compute device. Tests call this and skip cleanly when
    /// vulkan is unavailable.
    pub fn try_headless_ash_renderer() -> Option<AshSubstrateRenderer> {
        let artifact = SubstrateKernelArtifact::compile_canonical().ok()?;
        AshSubstrateRenderer::try_new(artifact).ok()
    }
}

#[cfg(feature = "runtime")]
pub use ash_runtime::{
    try_headless_ash_renderer, AshError, AshSubstrateRenderer, HeadlessProbeFrame,
};

// ════════════════════════════════════════════════════════════════════════════
// § AshSwapchainPresenter — VkSwapchainKHR-backed present path.
//
// § T11-W18-L7-PRESENT · The headless renderer above proved the .csl-source →
// SPIR-V → vkCmdDispatch chain on a private GPU image. This module extends
// the same stack with a Win32 VkSurfaceKHR + VkSwapchainKHR so the compute
// kernel writes DIRECTLY into the swapchain image (skipping vkCmdCopyImage)
// and we present per frame. The output format is fixed to R8G8B8A8_UNORM
// to match the `.csl` substrate-kernel `StorageImage⟨RGBA8Unorm⟩` declaration.
//
// § ARCHITECTURE
//   - Surface-creation : VK_KHR_surface + VK_KHR_win32_surface (Win32 only ;
//     the host is Apocky's Windows 11 box per `Take words LITERALLY`).
//   - Queue : we pick a queue-family that supports BOTH compute AND present
//     to keep the path single-queue, single-submit.
//   - Format : R8G8B8A8_UNORM, COLOR_SPACE_SRGB_NONLINEAR_KHR. Swapchain image
//     usage = COLOR_ATTACHMENT | STORAGE so the compute shader can write
//     directly via `vkImage` storage-image bindings.
//   - Per-frame ring : 2 frames-in-flight × {ImageAvailable Sem · RenderFinished
//     Sem · InFlight Fence · CommandBuffer · DescriptorSet}. Acquire-image
//     waits on ImageAvailable, dispatch waits on InFlight fence, present
//     waits on RenderFinished.
//   - Per-image : we create a VkImageView with FORMAT_R8G8B8A8_UNORM. The
//     descriptor binding-2 (storage image) gets bound to the acquired image's
//     view per frame.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "present")]
mod ash_present {
    //! § The ash-direct Win32 swapchain present-path.
    //!
    //! All of the unsafe vulkan FFI is bounded to this single module ; the
    //! crate root holds `forbid(unsafe_code)` for the default build and
    //! `deny(unsafe_code)` for `runtime` (where this module's
    //! `#[allow(unsafe_code)]` opt-in is the bounded escape hatch).
    #![allow(unsafe_code)]
    #![allow(clippy::missing_safety_doc)]
    #![allow(clippy::too_many_lines)]

    use super::SubstrateKernelArtifact;
    use ash::vk;
    use ash::vk::Handle;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const MAX_SCENE_CRYSTALS: usize = super::CSSL_MAX_SCENE_CRYSTALS;

    /// § Errors from the ash-direct swapchain-present path.
    #[derive(Debug, thiserror::Error)]
    pub enum PresentError {
        #[error("vulkan loader not available : {0}")]
        Loader(#[from] ash::LoadingError),
        #[error("vulkan instance creation failed : {0}")]
        InstanceCreate(vk::Result),
        #[error("no vulkan physical device with compute+present queue")]
        NoComputePresentDevice,
        #[error("vulkan device creation failed : {0}")]
        DeviceCreate(vk::Result),
        #[error("vulkan shader-module creation failed : {0}")]
        ShaderModuleCreate(vk::Result),
        #[error("vulkan descriptor-set-layout creation failed : {0}")]
        DescriptorSetLayoutCreate(vk::Result),
        #[error("vulkan pipeline-layout creation failed : {0}")]
        PipelineLayoutCreate(vk::Result),
        #[error("vulkan compute-pipeline creation failed : {0}")]
        ComputePipelineCreate(vk::Result),
        #[error("vulkan timestamp-query-pool creation failed : {0}")]
        QueryPoolCreate(vk::Result),
        #[error("vulkan win32-surface creation failed : {0}")]
        SurfaceCreate(vk::Result),
        #[error("vulkan swapchain creation failed : {0}")]
        SwapchainCreate(vk::Result),
        #[error("vulkan swapchain image-view creation failed : {0}")]
        ImageViewCreate(vk::Result),
        #[error("vulkan command-pool creation failed : {0}")]
        CommandPoolCreate(vk::Result),
        #[error("vulkan command-buffer alloc failed : {0}")]
        CommandBufferAlloc(vk::Result),
        #[error("vulkan descriptor-pool creation failed : {0}")]
        DescriptorPoolCreate(vk::Result),
        #[error("vulkan descriptor-set alloc failed : {0}")]
        DescriptorSetAlloc(vk::Result),
        #[error("vulkan semaphore creation failed : {0}")]
        SemaphoreCreate(vk::Result),
        #[error("vulkan fence creation failed : {0}")]
        FenceCreate(vk::Result),
        #[error("vulkan buffer creation failed : {0}")]
        BufferCreate(vk::Result),
        #[error("vulkan memory allocation failed : {0}")]
        MemoryAlloc(vk::Result),
        #[error("swapchain surface lacks required image usage flags : {0:?}")]
        UnsupportedSwapchainUsage(vk::ImageUsageFlags),
        #[error("vulkan observer uniform write failed : {0}")]
        UniformWrite(vk::Result),
        #[error("vulkan present-capture readback failed : {0}")]
        CaptureRead(vk::Result),
        #[error("vulkan present-capture readback too large")]
        CaptureTooLarge,
        #[error("vulkan present-capture artifact write failed : {0}")]
        CaptureArtifact(#[from] std::io::Error),
        #[error("vulkan acquire-next-image failed : {0}")]
        AcquireImage(vk::Result),
        #[error("vulkan queue-submit failed : {0}")]
        QueueSubmit(vk::Result),
        #[error("vulkan queue-present failed : {0}")]
        QueuePresent(vk::Result),
        #[error("vulkan begin-command-buffer failed : {0}")]
        BeginCommandBuffer(vk::Result),
        #[error("vulkan end-command-buffer failed : {0}")]
        EndCommandBuffer(vk::Result),
        #[error("vulkan reset-command-buffer failed : {0}")]
        ResetCommandBuffer(vk::Result),
        #[error("vulkan reset-fences failed : {0}")]
        ResetFences(vk::Result),
        #[error("vulkan wait-for-fences failed : {0}")]
        WaitForFences(vk::Result),
        #[error("only Win32 window handles are supported on this host")]
        UnsupportedWindowHandle,
        #[error("no surface format with R8G8B8A8_UNORM available")]
        NoSuitableFormat,
        #[error("no suitable memory-type-index for the requested allocation")]
        NoMemoryType,
    }

    /// § Number of frames-in-flight kept in the present-ring. Triple-buffered
    /// = 3 ; matches typical desktop swapchain image-count (2-3) and gives
    /// CPU one frame of head-room over GPU so `wait_for_fences` at frame-start
    /// is effectively a no-op when GPU is keeping up.
    /// § T11-W18-FPS-CAP-FIX : 2→3 per fps-cap-hunt diagnosis (frame-start
    ///   fence-wait was stalling pipelining at double-buffer depth)
    pub const FRAMES_IN_FLIGHT: usize = 3;

    /// § One ash-direct vulkan-1.3 substrate-renderer with Win32 swapchain
    /// present.
    ///
    /// Owns the Instance · PhysicalDevice · Device · ShaderModule · Compute-
    /// pipeline · Surface · Swapchain · per-frame sync + command-buffer
    /// resources. The compute shader writes DIRECTLY into the acquired
    /// swapchain image as a storage-image — no vkCmdCopyImage round-trip.
    pub struct AshSwapchainPresenter {
        // § Loaders + instance-level (kept for Drop ordering).
        _entry: ash::Entry,
        instance: ash::Instance,
        // § Extension loaders.
        surface_loader: ash::khr::surface::Instance,
        swapchain_loader: ash::khr::swapchain::Device,
        // § Logical state.
        physical_device: vk::PhysicalDevice,
        device: ash::Device,
        compute_present_queue_family: u32,
        compute_present_queue: vk::Queue,
        shader_module: vk::ShaderModule,
        descriptor_set_layout: vk::DescriptorSetLayout,
        pipeline_layout: vk::PipelineLayout,
        compute_pipeline: vk::Pipeline,
        // § Surface + swapchain.
        surface: vk::SurfaceKHR,
        swapchain: vk::SwapchainKHR,
        swapchain_format: vk::Format,
        present_mode: vk::PresentModeKHR,
        swapchain_extent: vk::Extent2D,
        swapchain_images: Vec<vk::Image>,
        swapchain_image_views: Vec<vk::ImageView>,
        // § Per-frame sync (ring of FRAMES_IN_FLIGHT).
        image_available: [vk::Semaphore; FRAMES_IN_FLIGHT],
        render_finished: [vk::Semaphore; FRAMES_IN_FLIGHT],
        in_flight_fence: [vk::Fence; FRAMES_IN_FLIGHT],
        // § Optional per-frame timestamp query-pools. A frame slot's GPU
        // timestamp becomes readable after its fence is observed on the next
        // reuse of that slot.
        timestamp_query_pools: [vk::QueryPool; FRAMES_IN_FLIGHT],
        timestamp_query_valid: [bool; FRAMES_IN_FLIGHT],
        timestamp_period_ns: f32,
        // § Command pool + per-frame command-buffers.
        command_pool: vk::CommandPool,
        command_buffers: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
        // § Descriptors : one pool sized for FRAMES_IN_FLIGHT × 3 bindings.
        descriptor_pool: vk::DescriptorPool,
        descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
        // § Observer + crystal buffers. Observer is written per frame with
        // dimensions/camera words ; crystal remains a fixed-size storage
        // slice until the kernel consumes scene data.
        observer_buf: vk::Buffer,
        observer_mem: vk::DeviceMemory,
        crystal_buf: vk::Buffer,
        crystal_mem: vk::DeviceMemory,
        // § Per-frame counters.
        current_frame: usize,
        frame_count: u64,
        // § Carry artifact for re-introspection (entry-name, etc).
        artifact: SubstrateKernelArtifact,
    }

    /// § Per-frame observer + crystal data passed to dispatch_with_present.
    /// Kept compact here so callers don't need to mirror the eventual full
    /// `.csl` observer layout. The present path packs swapchain dimensions
    /// plus these camera words into the binding=0 uniform.
    #[derive(Debug, Clone, Copy)]
    pub struct ObserverCoord {
        pub world_x: i32,
        pub world_y: i32,
        pub world_z: i32,
        pub yaw_milli: u32,
    }

    /// § Per-frame crystal sample. Layout is opaque to the host ; the kernel
    /// reads it as a storage-buffer.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Crystal {
        pub x: i32,
        pub y: i32,
        pub z: i32,
        pub strength_milli: u32,
        pub material_code: u32,
    }

    fn present_mode_label(mode: vk::PresentModeKHR) -> &'static str {
        if mode == vk::PresentModeKHR::IMMEDIATE {
            "IMMEDIATE"
        } else if mode == vk::PresentModeKHR::MAILBOX {
            "MAILBOX"
        } else if mode == vk::PresentModeKHR::FIFO {
            "FIFO"
        } else if mode == vk::PresentModeKHR::FIFO_RELAXED {
            "FIFO_RELAXED"
        } else {
            "OTHER"
        }
    }

    /// § Per-frame present telemetry. CPU timings are for the current frame.
    /// GPU dispatch timing is delayed by the frame ring and reports the last
    /// completed submission for the same slot once its fence has signaled.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PresentFrameTelemetry {
        pub frame_slot: usize,
        pub frame_count_before: u64,
        pub image_index: u32,
        pub width: u32,
        pub height: u32,
        pub present_mode: vk::PresentModeKHR,
        pub cpu_wait_us: u128,
        pub cpu_acquire_us: u128,
        pub cpu_upload_us: u128,
        pub cpu_record_us: u128,
        pub cpu_submit_us: u128,
        pub cpu_present_us: u128,
        pub previous_gpu_dispatch_ns: Option<u64>,
        pub gpu_timestamps_available: bool,
    }

    impl PresentFrameTelemetry {
        #[must_use]
        pub fn telemetry_line(&self) -> String {
            let gpu_dispatch_us = self
                .previous_gpu_dispatch_ns
                .map(|ns| ns.saturating_add(999) / 1_000)
                .map_or_else(|| "unavailable".to_owned(), |us| us.to_string());
            format!(
                "telemetry.present_frame use_case=present_loop_probe realworld_gate=false p99_us=unmeasured p99_9_us=unmeasured frame_slot={} frame_count_before={} image_index={} width={} height={} present_mode={} cpu_wait_us={} cpu_acquire_us={} cpu_upload_us={} cpu_record_us={} cpu_submit_us={} cpu_present_us={} gpu_timestamps_available={} previous_gpu_dispatch_us={gpu_dispatch_us}",
                self.frame_slot,
                self.frame_count_before,
                self.image_index,
                self.width,
                self.height,
                present_mode_label(self.present_mode),
                self.cpu_wait_us,
                self.cpu_acquire_us,
                self.cpu_upload_us,
                self.cpu_record_us,
                self.cpu_submit_us,
                self.cpu_present_us,
                self.gpu_timestamps_available,
            )
        }
    }

    /// § Pixel coordinate requested by the explicit present-capture probe.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PresentPixelSample {
        pub x: u32,
        pub y: u32,
    }

    /// § Pixel value captured from the swapchain image before present.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PresentCapturedPixel {
        pub x: u32,
        pub y: u32,
        pub rgba: [u8; 4],
    }

    /// § Non-production pixel evidence for live-present validation.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PresentCaptureTelemetry {
        pub width: u32,
        pub height: u32,
        pub sample_count: usize,
        pub nonzero_samples: usize,
        pub alpha255_samples: usize,
        pub checksum: u64,
        pub cpu_capture_wait_us: u128,
        pub artifact_path: Option<String>,
        pub pixels: Vec<PresentCapturedPixel>,
    }

    impl PresentCaptureTelemetry {
        #[must_use]
        pub fn telemetry_line(&self) -> String {
            let samples = self
                .pixels
                .iter()
                .map(|p| {
                    format!(
                        "{},{}:{:02x}{:02x}{:02x}{:02x}",
                        p.x, p.y, p.rgba[0], p.rgba[1], p.rgba[2], p.rgba[3]
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "telemetry.present_capture use_case=live_pixel_probe realworld_gate=false width={} height={} sample_count={} nonzero_samples={} alpha255_samples={} checksum={} cpu_capture_wait_us={} artifact_path={} samples={}",
                self.width,
                self.height,
                self.sample_count,
                self.nonzero_samples,
                self.alpha255_samples,
                self.checksum,
                self.cpu_capture_wait_us,
                self.artifact_path.as_deref().unwrap_or("none"),
                samples,
            )
        }
    }

    impl AshSwapchainPresenter {
        /// § Try to construct a present-capable ash renderer for the given
        /// winit window. Win32-only on this host (the `.csl` source path
        /// targets Apocky's Windows 11 desktop).
        ///
        /// § ERRORS
        ///   - `Loader` : vulkan-1.dll not installed.
        ///   - `UnsupportedWindowHandle` : non-Win32 window-handle (web/wayland
        ///     /xlib paths are out-of-scope for L7-PRESENT ; the host crate
        ///     is desktop-Windows-only per spec/14_BACKEND).
        ///   - `NoComputePresentDevice` : no GPU exposes a queue-family that
        ///     supports BOTH compute AND present-to-our-surface.
        ///   - `NoSuitableFormat` : the surface advertises no R8G8B8A8_UNORM
        ///     candidate (extremely rare on Windows ; would fall back to
        ///     headless V3 in window.rs).
        pub fn try_new_with_swapchain<W: HasWindowHandle>(
            window: &W,
            artifact: SubstrateKernelArtifact,
            initial_extent: (u32, u32),
        ) -> Result<Self, PresentError> {
            // 1. Load the loader.
            let entry = unsafe { ash::Entry::load()? };

            // 2. Pull the win32 raw-window-handle. We only support Win32 here
            //    per spec/14 + Take-words-LITERALLY ; other platforms are
            //    intentionally rejected so any silent-fallback bug surfaces
            //    immediately rather than producing a black window.
            let wh = window
                .window_handle()
                .map_err(|_| PresentError::UnsupportedWindowHandle)?;
            let RawWindowHandle::Win32(win32) = wh.as_raw() else {
                return Err(PresentError::UnsupportedWindowHandle);
            };

            // 3. Build instance with surface + win32-surface extensions.
            let app_name = c"cssl-host-substrate-render-v3";
            let app_info = vk::ApplicationInfo::default()
                .application_name(app_name)
                .application_version(0)
                .engine_name(app_name)
                .engine_version(0)
                .api_version(vk::make_api_version(0, 1, 3, 0));
            let instance_extensions = [
                ash::khr::surface::NAME.as_ptr(),
                ash::khr::win32_surface::NAME.as_ptr(),
            ];
            let inst_ci = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&instance_extensions);
            let instance = unsafe {
                entry
                    .create_instance(&inst_ci, None)
                    .map_err(PresentError::InstanceCreate)?
            };

            // 4. Surface (win32).
            let win32_surface_loader = ash::khr::win32_surface::Instance::new(&entry, &instance);
            let surface_ci = vk::Win32SurfaceCreateInfoKHR::default()
                .hwnd(win32.hwnd.get())
                .hinstance(win32.hinstance.map_or(0, |h| h.get()));
            let surface = unsafe {
                win32_surface_loader
                    .create_win32_surface(&surface_ci, None)
                    .map_err(PresentError::SurfaceCreate)?
            };
            let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

            // 5. Pick physical-device + queue-family that supports BOTH
            //    compute AND present-to-this-surface. We keep a single-
            //    queue path to make the per-frame dispatch path minimal.
            let physical_devices = unsafe {
                instance
                    .enumerate_physical_devices()
                    .map_err(|_| PresentError::NoComputePresentDevice)?
            };
            let mut chosen: Option<(vk::PhysicalDevice, u32)> = None;
            for pd in physical_devices {
                let qprops = unsafe { instance.get_physical_device_queue_family_properties(pd) };
                for (i, q) in qprops.iter().enumerate() {
                    if !q.queue_flags.contains(vk::QueueFlags::COMPUTE) {
                        continue;
                    }
                    let supports_present = unsafe {
                        surface_loader
                            .get_physical_device_surface_support(pd, i as u32, surface)
                            .unwrap_or(false)
                    };
                    if supports_present {
                        chosen = Some((pd, i as u32));
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }
            let (physical_device, qf) = match chosen {
                Some(c) => c,
                None => {
                    // Cleanup partial state before returning err.
                    unsafe {
                        surface_loader.destroy_surface(surface, None);
                        instance.destroy_instance(None);
                    }
                    return Err(PresentError::NoComputePresentDevice);
                }
            };

            // 6. Create logical device with VK_KHR_swapchain.
            let priorities = [1.0_f32];
            let q_cis = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(qf)
                .queue_priorities(&priorities)];
            let device_extensions = [ash::khr::swapchain::NAME.as_ptr()];
            let dev_ci = vk::DeviceCreateInfo::default()
                .queue_create_infos(&q_cis)
                .enabled_extension_names(&device_extensions);
            let device = unsafe {
                instance
                    .create_device(physical_device, &dev_ci, None)
                    .map_err(PresentError::DeviceCreate)?
            };
            let queue = unsafe { device.get_device_queue(qf, 0) };
            let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

            // 7. Choose surface format : prefer R8G8B8A8_UNORM (matches the
            //    `.csl` `StorageImage⟨RGBA8Unorm⟩` storage-target). Present
            //    mode : MAILBOX if available else FIFO.
            let formats = unsafe {
                surface_loader
                    .get_physical_device_surface_formats(physical_device, surface)
                    .map_err(PresentError::SurfaceCreate)?
            };
            let surface_fmt = formats
                .iter()
                .find(|f| {
                    f.format == vk::Format::R8G8B8A8_UNORM
                        && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                })
                .or_else(|| {
                    formats
                        .iter()
                        .find(|f| f.format == vk::Format::R8G8B8A8_UNORM)
                })
                .copied()
                .ok_or(PresentError::NoSuitableFormat)?;
            let present_modes = unsafe {
                surface_loader
                    .get_physical_device_surface_present_modes(physical_device, surface)
                    .map_err(PresentError::SurfaceCreate)?
            };
            // § T11-W18-V3-IMMEDIATE · prefer IMMEDIATE (no-vsync · 1440p144
            //   path · pair w/ fullscreen-exclusive for tear-free) · MAILBOX
            //   (triple-buffer · low-latency) · FIFO (vsync · ensures present)
            //   · LOA_VK_PRESENT_MODE env-override allowed.
            let env = std::env::var("LOA_VK_PRESENT_MODE").ok();
            let prefer_immediate = matches!(env.as_deref(), Some("immediate") | None);
            let present_mode =
                if prefer_immediate && present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
                    vk::PresentModeKHR::IMMEDIATE
                } else if env.as_deref() == Some("fifo") {
                    vk::PresentModeKHR::FIFO
                } else if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
                    vk::PresentModeKHR::MAILBOX
                } else {
                    vk::PresentModeKHR::FIFO
                };
            let caps = unsafe {
                surface_loader
                    .get_physical_device_surface_capabilities(physical_device, surface)
                    .map_err(PresentError::SurfaceCreate)?
            };
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: initial_extent.0.clamp(
                        caps.min_image_extent.width,
                        caps.max_image_extent.width.max(1),
                    ),
                    height: initial_extent.1.clamp(
                        caps.min_image_extent.height,
                        caps.max_image_extent.height.max(1),
                    ),
                }
            };
            let image_count = (caps.min_image_count + 1).min(if caps.max_image_count == 0 {
                u32::MAX
            } else {
                caps.max_image_count
            });

            // 8. Build the SwapchainCreateInfo with COLOR_ATTACHMENT | STORAGE
            //    usage so the compute shader can write directly into the
            //    swapchain image. TRANSFER_SRC is enabled only for explicit
            //    pixel-capture probes; normal frames remain no-copy.
            let image_usage = present_swapchain_image_usage(&caps)?;
            let sc_ci = vk::SwapchainCreateInfoKHR::default()
                .surface(surface)
                .min_image_count(image_count)
                .image_format(surface_fmt.format)
                .image_color_space(surface_fmt.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(image_usage)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(present_mode)
                .clipped(true);
            let swapchain = unsafe {
                swapchain_loader
                    .create_swapchain(&sc_ci, None)
                    .map_err(PresentError::SwapchainCreate)?
            };
            let swapchain_images = unsafe {
                swapchain_loader
                    .get_swapchain_images(swapchain)
                    .map_err(PresentError::SwapchainCreate)?
            };
            let swapchain_image_views: Vec<vk::ImageView> = swapchain_images
                .iter()
                .map(|img| {
                    let ci = vk::ImageViewCreateInfo::default()
                        .image(*img)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(surface_fmt.format)
                        .components(vk::ComponentMapping::default())
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(1)
                                .base_array_layer(0)
                                .layer_count(1),
                        );
                    unsafe { device.create_image_view(&ci, None) }
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(PresentError::ImageViewCreate)?;

            // 9. Shader module + pipeline.
            let words = artifact.words();
            let sm_ci = vk::ShaderModuleCreateInfo::default().code(words);
            let shader_module = unsafe {
                device
                    .create_shader_module(&sm_ci, None)
                    .map_err(PresentError::ShaderModuleCreate)?
            };
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            ];
            let dsl_ci = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
            let descriptor_set_layout = unsafe {
                device
                    .create_descriptor_set_layout(&dsl_ci, None)
                    .map_err(PresentError::DescriptorSetLayoutCreate)?
            };
            let dsls = [descriptor_set_layout];
            let pl_ci = vk::PipelineLayoutCreateInfo::default().set_layouts(&dsls);
            let pipeline_layout = unsafe {
                device
                    .create_pipeline_layout(&pl_ci, None)
                    .map_err(PresentError::PipelineLayoutCreate)?
            };
            let entry_name = std::ffi::CString::new(artifact.spec().entry_name.clone())
                .expect("entry-name must be valid C-str");
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(shader_module)
                .name(&entry_name);
            let cp_ci = vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(pipeline_layout);
            let pipelines = unsafe {
                device
                    .create_compute_pipelines(vk::PipelineCache::null(), &[cp_ci], None)
                    .map_err(|(_, r)| PresentError::ComputePipelineCreate(r))?
            };
            let compute_pipeline = pipelines[0];

            // 10. Command pool + per-frame command-buffers.
            let cp_ci = vk::CommandPoolCreateInfo::default()
                .queue_family_index(qf)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            let command_pool = unsafe {
                device
                    .create_command_pool(&cp_ci, None)
                    .map_err(PresentError::CommandPoolCreate)?
            };
            let cb_alloc = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(FRAMES_IN_FLIGHT as u32);
            let cbs = unsafe {
                device
                    .allocate_command_buffers(&cb_alloc)
                    .map_err(PresentError::CommandBufferAlloc)?
            };
            let mut command_buffers = [vk::CommandBuffer::null(); FRAMES_IN_FLIGHT];
            for (i, cb) in cbs.iter().enumerate() {
                command_buffers[i] = *cb;
            }

            // 11. Descriptor pool + sets.
            let pool_sizes = [
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(FRAMES_IN_FLIGHT as u32),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(FRAMES_IN_FLIGHT as u32),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(FRAMES_IN_FLIGHT as u32),
            ];
            let dp_ci = vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&pool_sizes)
                .max_sets(FRAMES_IN_FLIGHT as u32);
            let descriptor_pool = unsafe {
                device
                    .create_descriptor_pool(&dp_ci, None)
                    .map_err(PresentError::DescriptorPoolCreate)?
            };
            let dsl_arr = [descriptor_set_layout; FRAMES_IN_FLIGHT];
            let ds_alloc = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&dsl_arr);
            let dsets = unsafe {
                device
                    .allocate_descriptor_sets(&ds_alloc)
                    .map_err(PresentError::DescriptorSetAlloc)?
            };
            let mut descriptor_sets = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
            for (i, ds) in dsets.iter().enumerate() {
                descriptor_sets[i] = *ds;
            }

            // 12. Sync objects (image-available · render-finished · in-flight fence).
            let mut image_available = [vk::Semaphore::null(); FRAMES_IN_FLIGHT];
            let mut render_finished = [vk::Semaphore::null(); FRAMES_IN_FLIGHT];
            let mut in_flight_fence = [vk::Fence::null(); FRAMES_IN_FLIGHT];
            for i in 0..FRAMES_IN_FLIGHT {
                let sem_ci = vk::SemaphoreCreateInfo::default();
                image_available[i] = unsafe {
                    device
                        .create_semaphore(&sem_ci, None)
                        .map_err(PresentError::SemaphoreCreate)?
                };
                render_finished[i] = unsafe {
                    device
                        .create_semaphore(&sem_ci, None)
                        .map_err(PresentError::SemaphoreCreate)?
                };
                let fence_ci = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
                in_flight_fence[i] = unsafe {
                    device
                        .create_fence(&fence_ci, None)
                        .map_err(PresentError::FenceCreate)?
                };
            }

            let physical_props =
                unsafe { instance.get_physical_device_properties(physical_device) };
            let timestamp_period_ns = physical_props.limits.timestamp_period;
            let timestamps_available = physical_props.limits.timestamp_compute_and_graphics
                == vk::TRUE
                && timestamp_period_ns > 0.0;
            let mut timestamp_query_pools = [vk::QueryPool::null(); FRAMES_IN_FLIGHT];
            if timestamps_available {
                let query_ci = vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(2);
                for pool in &mut timestamp_query_pools {
                    *pool = unsafe {
                        device
                            .create_query_pool(&query_ci, None)
                            .map_err(PresentError::QueryPoolCreate)?
                    };
                }
            }
            let timestamp_query_valid = [false; FRAMES_IN_FLIGHT];

            // 13. Observer + crystal buffers (host-visible · 64 + 4096 bytes).
            //     Dispatch writes compact descriptor payloads each frame so
            //     the kernel can fold them into its current probe canary.
            let mem_props =
                unsafe { instance.get_physical_device_memory_properties(physical_device) };
            let (observer_buf, observer_mem) = create_buffer(
                &device,
                &mem_props,
                64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let (crystal_buf, crystal_mem) = create_buffer(
                &device,
                &mem_props,
                4096,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;

            // 14. Note : descriptor-sets are written PER-FRAME inside
            //     dispatch_with_present so we can rotate the storage-image
            //     binding across the swapchain images. observer + crystal
            //     bindings are static so we could write them once here, but
            //     keeping the write per-frame simplifies the resize path.
            Ok(Self {
                _entry: entry,
                instance,
                surface_loader,
                swapchain_loader,
                physical_device,
                device,
                compute_present_queue_family: qf,
                compute_present_queue: queue,
                shader_module,
                descriptor_set_layout,
                pipeline_layout,
                compute_pipeline,
                surface,
                swapchain,
                swapchain_format: surface_fmt.format,
                present_mode,
                swapchain_extent: extent,
                swapchain_images,
                swapchain_image_views,
                image_available,
                render_finished,
                in_flight_fence,
                timestamp_query_pools,
                timestamp_query_valid,
                timestamp_period_ns,
                command_pool,
                command_buffers,
                descriptor_pool,
                descriptor_sets,
                observer_buf,
                observer_mem,
                crystal_buf,
                crystal_mem,
                current_frame: 0,
                frame_count: 0,
                artifact,
            })
        }

        /// Borrow the underlying SPIR-V artifact.
        #[must_use]
        pub fn artifact(&self) -> &SubstrateKernelArtifact {
            &self.artifact
        }

        /// Width × height of the live swapchain.
        #[must_use]
        pub const fn extent(&self) -> (u32, u32) {
            (self.swapchain_extent.width, self.swapchain_extent.height)
        }

        /// VkFormat of the swapchain (always R8G8B8A8_UNORM by construction).
        #[must_use]
        pub const fn format(&self) -> vk::Format {
            self.swapchain_format
        }

        /// Total number of frames presented since construction.
        #[must_use]
        pub const fn frame_count(&self) -> u64 {
            self.frame_count
        }

        /// Compute+present queue-family index that the device + queue were created from.
        #[must_use]
        pub const fn queue_family(&self) -> u32 {
            self.compute_present_queue_family
        }

        /// Number of swapchain images created (typically 2 or 3 on Windows).
        #[must_use]
        pub fn image_count(&self) -> usize {
            self.swapchain_images.len()
        }

        /// Vulkan present mode selected for this swapchain.
        #[must_use]
        pub fn present_mode_label(&self) -> &'static str {
            present_mode_label(self.present_mode)
        }

        /// Whether the present path can report GPU dispatch timestamps.
        #[must_use]
        pub fn present_gpu_timestamps_available(&self) -> bool {
            self.timestamp_query_pools
                .iter()
                .any(|pool| !pool.is_null())
        }

        /// § Per-frame compute-dispatch-with-present.
        ///
        /// Acquires the next swapchain image, records a single
        /// `vkCmdDispatch(⌈width/8⌉ × ⌈height/8⌉ × 1)` that writes the
        /// substrate-kernel output directly into the swapchain image as a
        /// storage-image, then presents.
        ///
        /// `observer` and `crystals` are accepted for shape-symmetry with
        /// the headless dispatch API. The canonical kernel consumes the first
        /// two crystal slots as fixed spatial emissive samples while the
        /// variable scene array loop is still maturing.
        ///
        /// § ERRORS
        ///   - `WaitForFences` / `ResetFences` : fence sync error.
        ///   - `AcquireImage` : surface lost ; caller should rebuild via
        ///     [`Self::recreate_swapchain`].
        ///   - `QueueSubmit` / `QueuePresent` : driver/queue error.
        pub fn dispatch_with_present(
            &mut self,
            observer: ObserverCoord,
            crystals: &[Crystal],
        ) -> Result<(), PresentError> {
            self.dispatch_with_present_profiled(observer, crystals)
                .map(|_| ())
        }

        /// § Profiled per-frame compute-dispatch-with-present. Same path as
        /// `dispatch_with_present`, but returns CPU segment timings plus the
        /// previous completed GPU dispatch timestamp for this frame slot.
        ///
        /// § ERRORS
        ///   Same as [`Self::dispatch_with_present`].
        pub fn dispatch_with_present_profiled(
            &mut self,
            observer: ObserverCoord,
            crystals: &[Crystal],
        ) -> Result<PresentFrameTelemetry, PresentError> {
            self.dispatch_with_present_profiled_inner(observer, crystals, None)
                .map(|(telemetry, _capture)| telemetry)
        }

        /// § Profiled present path with explicit pixel readback.
        ///
        /// This is a validation probe, not a production frame path: it inserts
        /// a compute→transfer copy and waits for the submitted command-buffer
        /// before present so sampled pixels can be inspected.
        ///
        /// § ERRORS
        ///   Same as [`Self::dispatch_with_present_profiled`] plus capture
        ///   buffer/readback errors.
        pub fn dispatch_with_present_profiled_capture(
            &mut self,
            observer: ObserverCoord,
            crystals: &[Crystal],
            samples: &[PresentPixelSample],
        ) -> Result<(PresentFrameTelemetry, PresentCaptureTelemetry), PresentError> {
            self.dispatch_with_present_profiled_capture_artifact(observer, crystals, samples, None)
        }

        /// § Profiled present path with explicit pixel readback and optional
        /// full-frame PPM artifact write.
        ///
        /// Artifact output is validation-only. The file is written from the
        /// captured swapchain image after compute completion and before present.
        ///
        /// § ERRORS
        ///   Same as [`Self::dispatch_with_present_profiled_capture`] plus
        ///   artifact IO errors.
        pub fn dispatch_with_present_profiled_capture_artifact(
            &mut self,
            observer: ObserverCoord,
            crystals: &[Crystal],
            samples: &[PresentPixelSample],
            artifact_path: Option<&std::path::Path>,
        ) -> Result<(PresentFrameTelemetry, PresentCaptureTelemetry), PresentError> {
            let (telemetry, capture) = self.dispatch_with_present_profiled_inner(
                observer,
                crystals,
                Some((samples, artifact_path)),
            )?;
            let capture = capture.unwrap_or_else(|| PresentCaptureTelemetry {
                width: telemetry.width,
                height: telemetry.height,
                sample_count: 0,
                nonzero_samples: 0,
                alpha255_samples: 0,
                checksum: 0,
                cpu_capture_wait_us: 0,
                artifact_path: None,
                pixels: Vec::new(),
            });
            Ok((telemetry, capture))
        }

        fn dispatch_with_present_profiled_inner(
            &mut self,
            observer: ObserverCoord,
            crystals: &[Crystal],
            capture_request: Option<(&[PresentPixelSample], Option<&std::path::Path>)>,
        ) -> Result<(PresentFrameTelemetry, Option<PresentCaptureTelemetry>), PresentError>
        {
            let frame = self.current_frame;
            let frame_count_before = self.frame_count;
            let in_flight = self.in_flight_fence[frame];
            let image_avail = self.image_available[frame];
            let render_done = self.render_finished[frame];
            let cb = self.command_buffers[frame];
            let dset = self.descriptor_sets[frame];
            let query_pool = self.timestamp_query_pools[frame];
            let gpu_timestamps_available = !query_pool.is_null();

            // 1. Wait for previous use of this slot to finish.
            let wait_started = std::time::Instant::now();
            unsafe {
                self.device
                    .wait_for_fences(&[in_flight], true, u64::MAX)
                    .map_err(PresentError::WaitForFences)?;
                self.device
                    .reset_fences(&[in_flight])
                    .map_err(PresentError::ResetFences)?;
            }
            let cpu_wait_us = wait_started.elapsed().as_micros();
            let previous_gpu_dispatch_ns = self.read_present_gpu_dispatch_ns(frame);
            self.timestamp_query_valid[frame] = false;

            // 2. Acquire next image.
            let acquire_started = std::time::Instant::now();
            let (image_index, _suboptimal) = unsafe {
                self.swapchain_loader
                    .acquire_next_image(self.swapchain, u64::MAX, image_avail, vk::Fence::null())
                    .map_err(PresentError::AcquireImage)?
            };
            let cpu_acquire_us = acquire_started.elapsed().as_micros();
            let image = self.swapchain_images[image_index as usize];
            let image_view = self.swapchain_image_views[image_index as usize];

            let upload_started = std::time::Instant::now();
            write_present_observer_uniform(
                &self.device,
                self.observer_mem,
                [
                    self.swapchain_extent.width,
                    self.swapchain_extent.height,
                    observer.yaw_milli,
                    self.frame_count.min(u64::from(u32::MAX)) as u32,
                ],
            )?;
            let crystal_words = pack_present_scene_crystals(crystals);
            write_present_scene_crystals(
                &self.device,
                self.crystal_mem,
                crystals.len().min(MAX_SCENE_CRYSTALS) as u32,
                &crystal_words,
            )?;
            let cpu_upload_us = upload_started.elapsed().as_micros();

            let capture_requested = capture_request.is_some_and(|(samples, _)| !samples.is_empty());
            let capture_byte_len = u64::from(self.swapchain_extent.width)
                .saturating_mul(u64::from(self.swapchain_extent.height))
                .saturating_mul(4);
            let mem_props = unsafe {
                self.instance
                    .get_physical_device_memory_properties(self.physical_device)
            };
            let capture_scratch = if capture_requested {
                let (buffer, memory) = create_buffer(
                    &self.device,
                    &mem_props,
                    capture_byte_len,
                    vk::BufferUsageFlags::TRANSFER_DST,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )?;
                Some(CaptureScratch {
                    device: &self.device,
                    buffer,
                    memory,
                    byte_len: capture_byte_len,
                })
            } else {
                None
            };

            // 3. Update descriptor-set : observer (binding 0) · crystals
            //    (binding 1) · swapchain image-view (binding 2 · GENERAL layout).
            let observer_info = [vk::DescriptorBufferInfo::default()
                .buffer(self.observer_buf)
                .offset(0)
                .range(64)];
            let crystal_info = [vk::DescriptorBufferInfo::default()
                .buffer(self.crystal_buf)
                .offset(0)
                .range(4096)];
            let image_info = [vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(image_view)];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(dset)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&observer_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(dset)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&crystal_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(dset)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&image_info),
            ];
            unsafe { self.device.update_descriptor_sets(&writes, &[]) };

            // 4. Reset + record command-buffer.
            let record_started = std::time::Instant::now();
            unsafe {
                self.device
                    .reset_command_buffer(cb, vk::CommandBufferResetFlags::empty())
                    .map_err(PresentError::ResetCommandBuffer)?;
                let bi = vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
                self.device
                    .begin_command_buffer(cb, &bi)
                    .map_err(PresentError::BeginCommandBuffer)?;

                // Barrier 1 : UNDEFINED → GENERAL (compute-shader storage-write).
                let to_general = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .image(image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                self.device.cmd_pipeline_barrier(
                    cb,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_general],
                );
                if gpu_timestamps_available {
                    self.device.cmd_reset_query_pool(cb, query_pool, 0, 2);
                    self.device.cmd_write_timestamp(
                        cb,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        query_pool,
                        0,
                    );
                }

                // Bind + dispatch.
                self.device.cmd_bind_pipeline(
                    cb,
                    vk::PipelineBindPoint::COMPUTE,
                    self.compute_pipeline,
                );
                self.device.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::COMPUTE,
                    self.pipeline_layout,
                    0,
                    &[dset],
                    &[],
                );
                let gx = self.swapchain_extent.width.div_ceil(8);
                let gy = self.swapchain_extent.height.div_ceil(8);
                self.device.cmd_dispatch(cb, gx, gy, 1);
                if gpu_timestamps_available {
                    self.device.cmd_write_timestamp(
                        cb,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        query_pool,
                        1,
                    );
                }

                let color_range = vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1);
                if let Some(scratch) = capture_scratch.as_ref() {
                    let to_transfer = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .old_layout(vk::ImageLayout::GENERAL)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .image(image)
                        .subresource_range(color_range);
                    self.device.cmd_pipeline_barrier(
                        cb,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[to_transfer],
                    );
                    let copy_region = vk::BufferImageCopy::default()
                        .buffer_offset(0)
                        .buffer_row_length(0)
                        .buffer_image_height(0)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .mip_level(0)
                                .base_array_layer(0)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: self.swapchain_extent.width,
                            height: self.swapchain_extent.height,
                            depth: 1,
                        });
                    self.device.cmd_copy_image_to_buffer(
                        cb,
                        image,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        scratch.buffer,
                        &[copy_region],
                    );
                    let to_present = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .dst_access_mask(vk::AccessFlags::empty())
                        .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                        .image(image)
                        .subresource_range(color_range);
                    self.device.cmd_pipeline_barrier(
                        cb,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[to_present],
                    );
                } else {
                    // Barrier 2 : GENERAL → PRESENT_SRC_KHR.
                    let to_present = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::empty())
                        .old_layout(vk::ImageLayout::GENERAL)
                        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                        .image(image)
                        .subresource_range(color_range);
                    self.device.cmd_pipeline_barrier(
                        cb,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[to_present],
                    );
                }

                self.device
                    .end_command_buffer(cb)
                    .map_err(PresentError::EndCommandBuffer)?;
            }
            let cpu_record_us = record_started.elapsed().as_micros();

            // 5. Submit (wait on image-available · signal render-finished).
            let wait_sems = [image_avail];
            let wait_stages = [vk::PipelineStageFlags::COMPUTE_SHADER];
            let sig_sems = [render_done];
            let cb_arr = [cb];
            let submit = [vk::SubmitInfo::default()
                .wait_semaphores(&wait_sems)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&cb_arr)
                .signal_semaphores(&sig_sems)];
            let submit_started = std::time::Instant::now();
            unsafe {
                self.device
                    .queue_submit(self.compute_present_queue, &submit, in_flight)
                    .map_err(PresentError::QueueSubmit)?;
            }
            let cpu_submit_us = submit_started.elapsed().as_micros();
            if gpu_timestamps_available {
                self.timestamp_query_valid[frame] = true;
            }

            let capture_telemetry = if let (Some(scratch), Some((samples, artifact_path))) =
                (capture_scratch.as_ref(), capture_request)
            {
                let capture_wait_started = std::time::Instant::now();
                unsafe {
                    self.device
                        .wait_for_fences(&[in_flight], true, u64::MAX)
                        .map_err(PresentError::WaitForFences)?;
                }
                let cpu_capture_wait_us = capture_wait_started.elapsed().as_micros();
                Some(read_present_capture_samples(
                    &self.device,
                    scratch.memory,
                    scratch.byte_len,
                    self.swapchain_extent.width,
                    self.swapchain_extent.height,
                    samples,
                    cpu_capture_wait_us,
                    artifact_path,
                )?)
            } else {
                None
            };
            drop(capture_scratch);

            // 6. Present.
            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&sig_sems)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let present_started = std::time::Instant::now();
            let _ = unsafe {
                self.swapchain_loader
                    .queue_present(self.compute_present_queue, &present_info)
                    .map_err(PresentError::QueuePresent)?
            };
            let cpu_present_us = present_started.elapsed().as_micros();

            self.current_frame = (frame + 1) % FRAMES_IN_FLIGHT;
            self.frame_count = self.frame_count.wrapping_add(1);
            let telemetry = PresentFrameTelemetry {
                frame_slot: frame,
                frame_count_before,
                image_index,
                width: self.swapchain_extent.width,
                height: self.swapchain_extent.height,
                present_mode: self.present_mode,
                cpu_wait_us,
                cpu_acquire_us,
                cpu_upload_us,
                cpu_record_us,
                cpu_submit_us,
                cpu_present_us,
                previous_gpu_dispatch_ns,
                gpu_timestamps_available,
            };
            Ok((telemetry, capture_telemetry))
        }

        fn read_present_gpu_dispatch_ns(&self, frame: usize) -> Option<u64> {
            if !self.timestamp_query_valid[frame] {
                return None;
            }
            let query_pool = self.timestamp_query_pools[frame];
            if query_pool.is_null() {
                return None;
            }
            let mut ticks = [0_u64; 2];
            unsafe {
                self.device
                    .get_query_pool_results(
                        query_pool,
                        0,
                        &mut ticks,
                        vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                    )
                    .ok()?;
            }
            let elapsed_ticks = ticks[1].checked_sub(ticks[0])?;
            let elapsed_ns = (elapsed_ticks as f64) * f64::from(self.timestamp_period_ns);
            if elapsed_ns.is_finite() && elapsed_ns >= 0.0 {
                Some(elapsed_ns.round() as u64)
            } else {
                None
            }
        }

        /// § Recreate swapchain on resize / out-of-date. Tears down per-image
        /// state (image-views) + the swapchain itself ; rebuilds at the new
        /// surface-extent. Sync objects + command-buffers + descriptor-sets
        /// are reused (they're per-frame-in-flight, not per-image).
        ///
        /// § ERRORS
        ///   Same as `try_new_with_swapchain` for the swapchain-rebuild half.
        pub fn recreate_swapchain(&mut self, new_extent: (u32, u32)) -> Result<(), PresentError> {
            // Wait idle so we don't tear down resources mid-flight.
            unsafe {
                let _ = self.device.device_wait_idle();
                for v in &self.swapchain_image_views {
                    self.device.destroy_image_view(*v, None);
                }
                self.swapchain_image_views.clear();
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
            }

            let caps = unsafe {
                self.surface_loader
                    .get_physical_device_surface_capabilities(self.physical_device, self.surface)
                    .map_err(PresentError::SurfaceCreate)?
            };
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: new_extent.0.clamp(
                        caps.min_image_extent.width,
                        caps.max_image_extent.width.max(1),
                    ),
                    height: new_extent.1.clamp(
                        caps.min_image_extent.height,
                        caps.max_image_extent.height.max(1),
                    ),
                }
            };
            let image_count = (caps.min_image_count + 1).min(if caps.max_image_count == 0 {
                u32::MAX
            } else {
                caps.max_image_count
            });
            let sc_ci = vk::SwapchainCreateInfoKHR::default()
                .surface(self.surface)
                .min_image_count(image_count)
                .image_format(self.swapchain_format)
                .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(present_swapchain_image_usage(&caps)?)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(self.present_mode)
                .clipped(true);
            self.swapchain = unsafe {
                self.swapchain_loader
                    .create_swapchain(&sc_ci, None)
                    .map_err(PresentError::SwapchainCreate)?
            };
            self.swapchain_extent = extent;
            self.swapchain_images = unsafe {
                self.swapchain_loader
                    .get_swapchain_images(self.swapchain)
                    .map_err(PresentError::SwapchainCreate)?
            };
            self.swapchain_image_views = self
                .swapchain_images
                .iter()
                .map(|img| {
                    let ci = vk::ImageViewCreateInfo::default()
                        .image(*img)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(self.swapchain_format)
                        .components(vk::ComponentMapping::default())
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(1)
                                .base_array_layer(0)
                                .layer_count(1),
                        );
                    unsafe { self.device.create_image_view(&ci, None) }
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(PresentError::ImageViewCreate)?;
            Ok(())
        }

        /// Pipeline handle as `u64` for tests / introspection.
        #[must_use]
        pub fn pipeline_raw(&self) -> u64 {
            self.compute_pipeline.as_raw()
        }

        /// Swapchain handle as `u64` for tests / introspection.
        #[must_use]
        pub fn swapchain_raw(&self) -> u64 {
            self.swapchain.as_raw()
        }
    }

    fn present_swapchain_image_usage(
        caps: &vk::SurfaceCapabilitiesKHR,
    ) -> Result<vk::ImageUsageFlags, PresentError> {
        let required = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC;
        if caps.supported_usage_flags.contains(required) {
            Ok(required)
        } else {
            Err(PresentError::UnsupportedSwapchainUsage(
                required & !caps.supported_usage_flags,
            ))
        }
    }

    struct CaptureScratch<'a> {
        device: &'a ash::Device,
        buffer: vk::Buffer,
        memory: vk::DeviceMemory,
        byte_len: u64,
    }

    impl Drop for CaptureScratch<'_> {
        fn drop(&mut self) {
            unsafe {
                self.device.destroy_buffer(self.buffer, None);
                self.device.free_memory(self.memory, None);
            }
        }
    }

    /// § Helper : create a host-visible buffer + memory backing.
    fn create_buffer(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        usage: vk::BufferUsageFlags,
        flags: vk::MemoryPropertyFlags,
    ) -> Result<(vk::Buffer, vk::DeviceMemory), PresentError> {
        let bi = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buf = unsafe {
            device
                .create_buffer(&bi, None)
                .map_err(PresentError::BufferCreate)?
        };
        let req = unsafe { device.get_buffer_memory_requirements(buf) };
        let mt_idx = (0..mem_props.memory_type_count)
            .find(|i| {
                (req.memory_type_bits & (1 << i)) != 0
                    && mem_props.memory_types[*i as usize]
                        .property_flags
                        .contains(flags)
            })
            .ok_or(PresentError::NoMemoryType)?;
        let ai = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mt_idx);
        let mem = unsafe {
            device
                .allocate_memory(&ai, None)
                .map_err(PresentError::MemoryAlloc)?
        };
        unsafe {
            device
                .bind_buffer_memory(buf, mem, 0)
                .map_err(PresentError::MemoryAlloc)?;
        }
        Ok((buf, mem))
    }

    fn read_present_capture_samples(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        byte_len: u64,
        width: u32,
        height: u32,
        samples: &[PresentPixelSample],
        cpu_capture_wait_us: u128,
        artifact_path: Option<&std::path::Path>,
    ) -> Result<PresentCaptureTelemetry, PresentError> {
        let byte_len_usize =
            usize::try_from(byte_len).map_err(|_| PresentError::CaptureTooLarge)?;
        let ptr = unsafe {
            device
                .map_memory(mem, 0, byte_len, vk::MemoryMapFlags::empty())
                .map_err(PresentError::CaptureRead)?
        };
        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), byte_len_usize) };
        let mut pixels = Vec::with_capacity(samples.len());
        let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
        let mut nonzero_samples = 0usize;
        let mut alpha255_samples = 0usize;
        for sample in samples {
            let x = sample.x.min(width.saturating_sub(1));
            let y = sample.y.min(height.saturating_sub(1));
            let offset = (u64::from(y)
                .saturating_mul(u64::from(width))
                .saturating_add(u64::from(x)))
            .saturating_mul(4);
            let offset = offset as usize;
            let rgba = if offset + 4 <= bytes.len() {
                [
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]
            } else {
                [0, 0, 0, 0]
            };
            if rgba.iter().any(|v| *v != 0) {
                nonzero_samples += 1;
            }
            if rgba[3] == 255 {
                alpha255_samples += 1;
            }
            for b in rgba {
                checksum ^= u64::from(b);
                checksum = checksum.wrapping_mul(0x100_0000_01b3);
            }
            pixels.push(PresentCapturedPixel { x, y, rgba });
        }
        let artifact_result = artifact_path
            .map(|path| write_present_capture_ppm(path, width, height, bytes))
            .transpose();
        unsafe {
            device.unmap_memory(mem);
        }
        artifact_result?;
        Ok(PresentCaptureTelemetry {
            width,
            height,
            sample_count: pixels.len(),
            nonzero_samples,
            alpha255_samples,
            checksum,
            cpu_capture_wait_us,
            artifact_path: artifact_path.map(|path| path.display().to_string()),
            pixels,
        })
    }

    fn write_present_capture_ppm(
        path: &std::path::Path,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> std::io::Result<()> {
        use std::io::Write;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        write!(file, "P6\n{} {}\n255\n", width, height)?;
        for px in rgba.chunks_exact(4) {
            file.write_all(&px[..3])?;
        }
        file.flush()
    }

    fn write_present_observer_uniform(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        words: [u32; 4],
    ) -> Result<(), PresentError> {
        write_present_descriptor_words(device, mem, words)
    }

    fn write_present_descriptor_words(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        words: [u32; 4],
    ) -> Result<(), PresentError> {
        write_present_descriptor_word_sets(device, mem, &[words])
    }

    fn write_present_descriptor_word_sets(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        word_sets: &[[u32; 4]],
    ) -> Result<(), PresentError> {
        let byte_len = word_sets.len() * core::mem::size_of::<[u32; 4]>();
        if byte_len == 0 {
            return Ok(());
        }
        unsafe {
            let ptr = device
                .map_memory(mem, 0, byte_len as u64, vk::MemoryMapFlags::empty())
                .map_err(PresentError::UniformWrite)?;
            std::ptr::copy_nonoverlapping(
                word_sets.as_ptr().cast::<u8>(),
                ptr.cast::<u8>(),
                byte_len,
            );
            device.unmap_memory(mem);
        }
        Ok(())
    }

    fn pack_present_crystal_words(crystal: Option<&Crystal>) -> [u32; 4] {
        crystal.map_or([0, 128, 128, 0], |crystal| {
            [
                crystal.strength_milli.min(255),
                encode_crystal_axis(crystal.x),
                encode_crystal_axis(crystal.y),
                crystal.material_code.min(255),
            ]
        })
    }

    fn pack_present_scene_crystals(crystals: &[Crystal]) -> [[u32; 4]; MAX_SCENE_CRYSTALS] {
        core::array::from_fn(|i| pack_present_crystal_words(crystals.get(i)))
    }

    fn write_present_scene_crystals(
        device: &ash::Device,
        mem: vk::DeviceMemory,
        crystal_count: u32,
        crystal_words: &[[u32; 4]; MAX_SCENE_CRYSTALS],
    ) -> Result<(), PresentError> {
        let mut scene_words = [[0_u32; 4]; MAX_SCENE_CRYSTALS + 1];
        scene_words[0] = [crystal_count.min(MAX_SCENE_CRYSTALS as u32), 0, 0, 0];
        scene_words[1..].copy_from_slice(crystal_words);
        write_present_descriptor_word_sets(device, mem, &scene_words)
    }

    fn encode_crystal_axis(value: i32) -> u32 {
        (value.clamp(-128, 127) + 128) as u32
    }

    impl Drop for AshSwapchainPresenter {
        fn drop(&mut self) {
            unsafe {
                let _ = self.device.device_wait_idle();
                // Per-frame sync.
                for i in 0..FRAMES_IN_FLIGHT {
                    if !self.timestamp_query_pools[i].is_null() {
                        self.device
                            .destroy_query_pool(self.timestamp_query_pools[i], None);
                    }
                    if !self.image_available[i].is_null() {
                        self.device.destroy_semaphore(self.image_available[i], None);
                    }
                    if !self.render_finished[i].is_null() {
                        self.device.destroy_semaphore(self.render_finished[i], None);
                    }
                    if !self.in_flight_fence[i].is_null() {
                        self.device.destroy_fence(self.in_flight_fence[i], None);
                    }
                }
                if !self.descriptor_pool.is_null() {
                    self.device
                        .destroy_descriptor_pool(self.descriptor_pool, None);
                }
                if !self.command_pool.is_null() {
                    self.device.destroy_command_pool(self.command_pool, None);
                }
                if !self.observer_buf.is_null() {
                    self.device.destroy_buffer(self.observer_buf, None);
                }
                if !self.observer_mem.is_null() {
                    self.device.free_memory(self.observer_mem, None);
                }
                if !self.crystal_buf.is_null() {
                    self.device.destroy_buffer(self.crystal_buf, None);
                }
                if !self.crystal_mem.is_null() {
                    self.device.free_memory(self.crystal_mem, None);
                }
                for v in &self.swapchain_image_views {
                    self.device.destroy_image_view(*v, None);
                }
                if !self.swapchain.is_null() {
                    self.swapchain_loader
                        .destroy_swapchain(self.swapchain, None);
                }
                if !self.compute_pipeline.is_null() {
                    self.device.destroy_pipeline(self.compute_pipeline, None);
                }
                if !self.pipeline_layout.is_null() {
                    self.device
                        .destroy_pipeline_layout(self.pipeline_layout, None);
                }
                if !self.descriptor_set_layout.is_null() {
                    self.device
                        .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
                }
                if !self.shader_module.is_null() {
                    self.device.destroy_shader_module(self.shader_module, None);
                }
                self.device.destroy_device(None);
                if !self.surface.is_null() {
                    self.surface_loader.destroy_surface(self.surface, None);
                }
                self.instance.destroy_instance(None);
                let _ = &self.physical_device;
            }
        }
    }
}

#[cfg(feature = "present")]
pub use ash_present::{
    AshSwapchainPresenter, Crystal, ObserverCoord, PresentCaptureTelemetry, PresentCapturedPixel,
    PresentError, PresentFrameTelemetry, PresentPixelSample, FRAMES_IN_FLIGHT,
};

// ════════════════════════════════════════════════════════════════════════════
// § Tests — five-test gate per § Definition-of-done.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// § Test #1 : csl→spirv compiles.
    /// The substrate-kernel `.csl` source's canonical spec produces a
    /// non-empty SPIR-V word stream with the correct magic. Verifies the
    /// end-to-end CSSL-source → cssl-cgen-gpu-spirv → cssl-cgen-spirv chain.
    #[test]
    fn csl_to_spirv_compiles() {
        let art = SubstrateKernelArtifact::compile_canonical()
            .expect("canonical .csl substrate-kernel must compile to SPIR-V");
        assert_eq!(
            art.magic(),
            SPIRV_MAGIC,
            "first word must be SPIR-V magic 0x07230203 (Khronos § 2.3)",
        );
        // SPIR-V 1.5 = 0x00010500.
        assert_eq!(art.version(), 0x0001_0500, "must emit SPIR-V version 1.5");
        // bound > 1 ⇒ at least one id was allocated.
        assert!(art.id_bound() > 1, "id-bound must be > 1");
    }

    /// § Test #2 : spirv-binary-size > 0 (and reasonable lower bound).
    /// Sanity-check the byte length + 4-alignment — the host's
    /// vkCreateShaderModule consumes `code_size = byte_len()` so this
    /// invariant is load-bearing for the ash-direct path.
    #[test]
    fn spirv_binary_size_reasonable() {
        let art = SubstrateKernelArtifact::compile_canonical().unwrap();
        let bytes = art.byte_len();
        // Header alone = 5 × 4 = 20 bytes. A real compute entry-point + 3
        // bindings adds well over 100 bytes ; assert > 100 to catch any
        // regression that strips the body.
        assert!(
            bytes > 100,
            "SPIR-V binary must be > 100 bytes (got {bytes})",
        );
        assert_eq!(bytes % 4, 0, "SPIR-V binary must be 4-byte aligned");
        // Round-trip via to_bytes() must agree with byte_len().
        let bs = art.to_bytes().unwrap();
        assert_eq!(bs.len(), bytes);
    }

    /// § Test #3 : vk-loader smoke-test (gated on `runtime` + present loader).
    /// Verifies that `ash::Entry::load` finds the system vulkan loader AND
    /// that `vkCreateInstance` + `vkEnumeratePhysicalDevices` succeed. Skips
    /// cleanly on CI runners with no vulkan.
    #[cfg(feature = "runtime")]
    #[test]
    fn vk_loader_smoketest() {
        let Some(renderer) = try_headless_ash_renderer() else {
            eprintln!("no vulkan loader / no compute device · skipped");
            return;
        };
        // Renderer constructed = entry · instance · device · shader_module
        // are all live. Just borrow the artifact to confirm.
        assert_eq!(renderer.artifact().magic(), SPIRV_MAGIC);
        assert!(renderer.compute_queue_family() != u32::MAX);
    }

    /// § Test #4 : headless-compute end-to-end. Build the pipeline + record
    /// + submit + wait for one `vkCmdDispatch(1,1,1)`. Skips cleanly if no
    /// vulkan loader.
    #[cfg(feature = "runtime")]
    #[test]
    fn headless_compute_dispatch() {
        let Some(mut renderer) = try_headless_ash_renderer() else {
            eprintln!("no vulkan · skipped");
            return;
        };
        let pipe = renderer
            .headless_dispatch()
            .expect("headless dispatch must succeed on a present vulkan loader");
        assert!(pipe != 0, "compute pipeline handle must be non-null");
        assert!(renderer.pipeline_built());
    }

    /// § Test #4b : headless camera-tile parity. Reads back the full 8x8
    /// storage-image probe and compares gid-derived camera rays to CPU oracles.
    #[cfg(feature = "runtime")]
    #[test]
    fn headless_camera_tile_matches_v13_oracles() {
        let expected = SubstrateKernelArtifact::compile_canonical()
            .unwrap()
            .expected_v13_camera_tile8_rgba8_for_scene_slots_inputs(
                (
                    ash_runtime::HEADLESS_PROBE_WIDTH,
                    ash_runtime::HEADLESS_PROBE_HEIGHT,
                ),
                ash_runtime::HEADLESS_OBSERVER_YAW_MILLI,
                ash_runtime::HEADLESS_CRYSTAL_COUNT,
                ash_runtime::headless_crystal_words(),
            );
        let Some(mut renderer) = try_headless_ash_renderer() else {
            eprintln!("no vulkan · skipped");
            return;
        };
        let tile = renderer
            .headless_probe_tile_rgba8()
            .expect("headless probe-tile readback must succeed");
        assert_eq!(
            tile, expected,
            "GPU RGBA8 camera tile must match observer+gid CPU oracle"
        );
    }

    /// § Runtime telemetry smoke-test. Samples end-to-end headless dispatch
    /// + storage-image readback timing while preserving byte-exact oracle
    /// validation. GPU timestamp samples are emitted separately when the
    /// physical device exposes compute timestamps.
    #[cfg(feature = "runtime")]
    #[test]
    fn headless_dispatch_timing_telemetry() {
        let expected = SubstrateKernelArtifact::compile_canonical()
            .unwrap()
            .expected_v13_camera_tile8_rgba8_for_scene_slots_inputs(
                (
                    ash_runtime::HEADLESS_PROBE_WIDTH,
                    ash_runtime::HEADLESS_PROBE_HEIGHT,
                ),
                ash_runtime::HEADLESS_OBSERVER_YAW_MILLI,
                ash_runtime::HEADLESS_CRYSTAL_COUNT,
                ash_runtime::headless_crystal_words(),
            );
        let Some(mut renderer) = try_headless_ash_renderer() else {
            eprintln!("no vulkan · skipping dispatch telemetry");
            return;
        };

        const SAMPLES: usize = 8;
        let mut samples_us = Vec::with_capacity(SAMPLES);
        let mut gpu_samples_ns = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = std::time::Instant::now();
            let frame = renderer
                .headless_probe_frame_rgba8_for_size(
                    ash_runtime::HEADLESS_PROBE_WIDTH,
                    ash_runtime::HEADLESS_PROBE_HEIGHT,
                )
                .expect("headless telemetry dispatch must succeed");
            let elapsed_us = started.elapsed().as_micros();
            assert_eq!(
                frame.pixels(),
                expected.as_slice(),
                "telemetry dispatch must preserve byte-exact GPU oracle"
            );
            if let Some(gpu_elapsed_ns) = frame.gpu_elapsed_ns() {
                gpu_samples_ns.push(gpu_elapsed_ns);
            }
            samples_us.push(elapsed_us);
        }

        samples_us.sort_unstable();
        emit_timing_telemetry(
            "headless_dispatch",
            "correctness_smoke",
            ash_runtime::HEADLESS_PROBE_WIDTH,
            ash_runtime::HEADLESS_PROBE_HEIGHT,
            &samples_us,
            &gpu_samples_ns,
        );
        let p95_us = samples_us[SAMPLES - 1];
        assert!(p95_us > 0, "telemetry samples must record nonzero time");
    }

    /// § Multi-workgroup telemetry. Dispatches a 256x256 storage image
    /// (8x8 workgroups) and verifies representative pixels against the CPU
    /// oracle so the timing surface cannot silently drift from correctness.
    #[cfg(feature = "runtime")]
    #[test]
    fn headless_scaled_dispatch_timing_telemetry() {
        const WIDTH: u32 = 256;
        const HEIGHT: u32 = 256;
        const SAMPLES: usize = 3;

        let artifact = SubstrateKernelArtifact::compile_canonical().unwrap();
        let Some(mut renderer) = try_headless_ash_renderer() else {
            eprintln!("no vulkan · skipping scaled dispatch telemetry");
            return;
        };

        let mut samples_us = Vec::with_capacity(SAMPLES);
        let mut gpu_samples_ns = Vec::with_capacity(SAMPLES);
        let mut last_tile = Vec::new();
        for _ in 0..SAMPLES {
            let started = std::time::Instant::now();
            let frame = renderer
                .headless_probe_frame_rgba8_for_size(WIDTH, HEIGHT)
                .expect("scaled headless telemetry dispatch must succeed");
            let elapsed_us = started.elapsed().as_micros();
            if let Some(gpu_elapsed_ns) = frame.gpu_elapsed_ns() {
                gpu_samples_ns.push(gpu_elapsed_ns);
            }
            let tile = frame.into_pixels();
            assert_eq!(tile.len(), (WIDTH * HEIGHT) as usize);
            samples_us.push(elapsed_us);
            last_tile = tile;
        }

        for pixel in [(0, 0), (WIDTH / 2, HEIGHT / 2), (WIDTH - 1, HEIGHT - 1)] {
            let idx = (pixel.1 * WIDTH + pixel.0) as usize;
            let expected = expected_headless_pixel(artifact.spec(), (WIDTH, HEIGHT), pixel);
            assert_eq!(
                last_tile[idx], expected,
                "scaled dispatch pixel {pixel:?} must match CPU oracle"
            );
        }

        samples_us.sort_unstable();
        emit_timing_telemetry(
            "headless_scaled_dispatch",
            "scaled_probe_not_production",
            WIDTH,
            HEIGHT,
            &samples_us,
            &gpu_samples_ns,
        );
        let p95_us = samples_us[SAMPLES - 1];
        assert!(
            p95_us > 0,
            "scaled telemetry samples must record nonzero time"
        );
    }

    /// § Persistent probe telemetry. This is still not a production gate:
    /// 256x256 is not 1440p and readback remains in the loop. It does,
    /// however, use enough frames to report p99 honestly for the persistent
    /// resource path.
    #[cfg(feature = "runtime")]
    #[test]
    fn headless_persistent_probe_120_frame_telemetry() {
        const WIDTH: u32 = 256;
        const HEIGHT: u32 = 256;
        const SAMPLES: usize = 120;

        let artifact = SubstrateKernelArtifact::compile_canonical().unwrap();
        let Some(mut renderer) = try_headless_ash_renderer() else {
            eprintln!("no vulkan · skipping persistent probe telemetry");
            return;
        };

        let mut samples_us = Vec::with_capacity(SAMPLES);
        let mut gpu_samples_ns = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = std::time::Instant::now();
            let frame = renderer
                .headless_probe_frame_rgba8_for_size(WIDTH, HEIGHT)
                .expect("persistent headless telemetry dispatch must succeed");
            let elapsed_us = started.elapsed().as_micros();
            for pixel in [(0, 0), (WIDTH / 2, HEIGHT / 2), (WIDTH - 1, HEIGHT - 1)] {
                let idx = (pixel.1 * WIDTH + pixel.0) as usize;
                let expected = expected_headless_pixel(artifact.spec(), (WIDTH, HEIGHT), pixel);
                assert_eq!(
                    frame.pixels()[idx],
                    expected,
                    "persistent dispatch pixel {pixel:?} must match CPU oracle"
                );
            }
            if let Some(gpu_elapsed_ns) = frame.gpu_elapsed_ns() {
                gpu_samples_ns.push(gpu_elapsed_ns);
            }
            samples_us.push(elapsed_us);
        }

        samples_us.sort_unstable();
        assert_eq!(samples_us.len(), SAMPLES);
        assert_eq!(
            renderer.headless_probe_session_extent(),
            Some((WIDTH, HEIGHT))
        );
        emit_timing_telemetry(
            "headless_persistent_probe_120",
            "persistent_probe_not_production",
            WIDTH,
            HEIGHT,
            &samples_us,
            &gpu_samples_ns,
        );
        assert!(
            percentile_us(&samples_us, 99, 100).is_some_and(|p99| p99 > 0),
            "persistent telemetry must record nonzero p99"
        );
    }

    /// § Profiled frame path must reuse extent-keyed headless resources while
    /// preserving byte-exact output. This guards against timing evidence
    /// sliding back into allocate-every-sample behavior.
    #[cfg(feature = "runtime")]
    #[test]
    fn headless_profiled_dispatch_reuses_session_and_preserves_oracle() {
        let expected = SubstrateKernelArtifact::compile_canonical()
            .unwrap()
            .expected_v13_camera_tile8_rgba8_for_scene_slots_inputs(
                (
                    ash_runtime::HEADLESS_PROBE_WIDTH,
                    ash_runtime::HEADLESS_PROBE_HEIGHT,
                ),
                ash_runtime::HEADLESS_OBSERVER_YAW_MILLI,
                ash_runtime::HEADLESS_CRYSTAL_COUNT,
                ash_runtime::headless_crystal_words(),
            );
        let Some(mut renderer) = try_headless_ash_renderer() else {
            eprintln!("no vulkan · skipping profiled session reuse test");
            return;
        };

        let first = renderer
            .headless_probe_frame_rgba8_for_size(
                ash_runtime::HEADLESS_PROBE_WIDTH,
                ash_runtime::HEADLESS_PROBE_HEIGHT,
            )
            .expect("first profiled dispatch must succeed");
        assert_eq!(
            renderer.headless_probe_session_extent(),
            Some((
                ash_runtime::HEADLESS_PROBE_WIDTH,
                ash_runtime::HEADLESS_PROBE_HEIGHT
            ))
        );
        let second = renderer
            .headless_probe_frame_rgba8_for_size(
                ash_runtime::HEADLESS_PROBE_WIDTH,
                ash_runtime::HEADLESS_PROBE_HEIGHT,
            )
            .expect("second profiled dispatch must reuse session and succeed");

        assert_eq!(first.pixels(), expected.as_slice());
        assert_eq!(second.pixels(), expected.as_slice());
        assert_eq!(first.pixels(), second.pixels());
        if renderer.headless_probe_gpu_timestamps_available() {
            assert!(
                first.gpu_elapsed_ns().is_some() || second.gpu_elapsed_ns().is_some(),
                "timestamp-capable session must return at least one GPU elapsed sample"
            );
        }
    }

    #[test]
    fn validation_profiles_make_real_world_gap_explicit() {
        const TARGET_PIXELS: u64 = REALTIME_TARGET_WIDTH as u64 * REALTIME_TARGET_HEIGHT as u64;
        const SCALED_PROBE_PIXELS: u64 = 256 * 256;
        assert!(
            SCALED_PROBE_PIXELS < TARGET_PIXELS / 50,
            "256x256 probe is intentionally <2% of 1440p pixels and cannot be treated as production validation"
        );
        assert!(
            COMPETITIVE_240HZ_P99_US < REALTIME_144HZ_P99_US,
            "competitive refresh budget must remain stricter than 144Hz budget"
        );
    }

    #[cfg(feature = "runtime")]
    fn expected_headless_pixel(
        spec: &SubstrateKernelSpec,
        observer_size: (u32, u32),
        pixel: (u32, u32),
    ) -> [u8; 4] {
        let (red, material_mix, object_mask) = spec
            .v13_camera_ray_visual_components_2d_for_crystal_count(
                observer_size,
                pixel,
                ash_runtime::HEADLESS_OBSERVER_YAW_MILLI,
                ash_runtime::HEADLESS_CRYSTAL_COUNT,
                ash_runtime::headless_crystal_words(),
            );
        let x_norm = camera_norm(observer_size.0, pixel.0);
        let y_norm = camera_norm(observer_size.1, pixel.1);
        let canary = f32::from(spec.descriptor_canary_value_for_crystal_count(
            observer_size.0,
            ash_runtime::HEADLESS_CRYSTAL_COUNT,
            ash_runtime::headless_crystal_words(),
        )) / 255.0;
        let edge = (object_mask * (1.0 - object_mask) * 4.0).clamp(0.0, 1.0);
        let depth_shadow = 1.0 - object_mask * 0.18;
        let red_raw = red * (0.82 + object_mask * 0.18) + edge * 0.18;
        let green_base = red * 0.48 + x_norm * 0.20 + material_mix * 0.16;
        let green_raw = green_base * depth_shadow + object_mask * material_mix * 0.25 + edge * 0.10;
        let blue_base = red * 0.30 + y_norm * 0.20 + canary * 0.12 + (1.0 - material_mix) * 0.20;
        let blue_raw = blue_base * (1.0 - object_mask * 0.22)
            + object_mask * (1.0 - material_mix) * 0.25
            + edge * 0.06;
        [
            unorm8(red_raw),
            unorm8(green_raw),
            unorm8(blue_raw),
            unorm8(y_norm),
        ]
    }

    #[cfg(feature = "runtime")]
    fn camera_norm(length: u32, position: u32) -> f32 {
        let denom = ((length as f32) - 1.0).max(1.0);
        (position as f32 / denom).clamp(0.0, 1.0)
    }

    #[cfg(feature = "runtime")]
    fn unorm8(value: f32) -> u8 {
        (value.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8
    }

    const REALTIME_TARGET_WIDTH: u32 = 2560;
    const REALTIME_TARGET_HEIGHT: u32 = 1440;
    const REALTIME_144HZ_P99_US: u128 = 6_944;
    const COMPETITIVE_240HZ_P99_US: u128 = 4_166;

    #[cfg(feature = "runtime")]
    fn emit_timing_telemetry(
        name: &str,
        use_case: &str,
        width: u32,
        height: u32,
        samples: &[u128],
        gpu_samples_ns: &[u64],
    ) {
        let min_us = samples[0];
        let p50_us = samples[samples.len() / 2];
        let p95_us = percentile_us(samples, 95, 100).expect("non-empty samples have p95");
        let pixels = u128::from(width) * u128::from(height);
        let target_pixels = u128::from(REALTIME_TARGET_WIDTH) * u128::from(REALTIME_TARGET_HEIGHT);
        let target_pixel_coverage_ppm = pixels * 1_000_000 / target_pixels;
        let p99_summary = if samples.len() >= 100 {
            format!(
                "p99_us={}",
                percentile_us(samples, 99, 100).expect("len>=100 has p99")
            )
        } else {
            "p99_us=unmeasured".to_owned()
        };
        let p999_summary = if samples.len() >= 1_000 {
            format!(
                "p99_9_us={}",
                percentile_us(samples, 999, 1_000).expect("len>=1000 has p99.9")
            )
        } else {
            "p99_9_us=unmeasured".to_owned()
        };
        let gpu_summary = gpu_timing_summary(gpu_samples_ns);
        println!(
            "telemetry.{name} use_case={use_case} realworld_gate=false {p99_summary} {p999_summary} sample_count={} width={width} height={height} pixels={pixels} target_width={} target_height={} target_144hz_p99_us={} target_240hz_p99_us={} target_pixel_coverage_ppm={target_pixel_coverage_ppm} scene_slots={} active_crystals={} min_us={min_us} p50_us={p50_us} p95_us={p95_us} {gpu_summary}",
            samples.len(),
            REALTIME_TARGET_WIDTH,
            REALTIME_TARGET_HEIGHT,
            REALTIME_144HZ_P99_US,
            COMPETITIVE_240HZ_P99_US,
            CSSL_MAX_SCENE_CRYSTALS,
            ash_runtime::HEADLESS_CRYSTAL_COUNT
        );
    }

    #[cfg(feature = "runtime")]
    fn percentile_us(
        sorted_samples: &[u128],
        numerator: usize,
        denominator: usize,
    ) -> Option<u128> {
        if sorted_samples.is_empty() || denominator == 0 {
            return None;
        }
        let rank = sorted_samples.len().saturating_mul(numerator);
        let idx = rank
            .saturating_add(denominator - 1)
            .checked_div(denominator)?
            .saturating_sub(1)
            .min(sorted_samples.len() - 1);
        sorted_samples.get(idx).copied()
    }

    #[cfg(feature = "runtime")]
    fn gpu_timing_summary(gpu_samples_ns: &[u64]) -> String {
        if gpu_samples_ns.is_empty() {
            return "gpu_timestamp_samples=0 gpu_min_us=unavailable gpu_p50_us=unavailable gpu_p95_us=unavailable".to_owned();
        }
        let mut gpu_us = gpu_samples_ns
            .iter()
            .map(|ns| ns.saturating_add(999) / 1_000)
            .collect::<Vec<_>>();
        gpu_us.sort_unstable();
        let gpu_p95_us = percentile_u64(&gpu_us, 95, 100).expect("non-empty samples have p95");
        format!(
            "gpu_timestamp_samples={} gpu_min_us={} gpu_p50_us={} gpu_p95_us={}",
            gpu_us.len(),
            gpu_us[0],
            gpu_us[gpu_us.len() / 2],
            gpu_p95_us
        )
    }

    #[cfg(feature = "runtime")]
    fn percentile_u64(sorted_samples: &[u64], numerator: usize, denominator: usize) -> Option<u64> {
        if sorted_samples.is_empty() || denominator == 0 {
            return None;
        }
        let rank = sorted_samples.len().saturating_mul(numerator);
        let idx = rank
            .saturating_add(denominator - 1)
            .checked_div(denominator)?
            .saturating_sub(1)
            .min(sorted_samples.len() - 1);
        sorted_samples.get(idx).copied()
    }

    /// § Test #5 : per-frame determinism. The same SPIR-V artifact built
    /// twice must match byte-for-byte ; the same renderer dispatched twice
    /// must yield the same pipeline-build result. Verifies the v3
    /// architectural promise that the `.csl`-source → SPIR-V chain is
    /// deterministic AND that the ash-direct dispatch path is reproducible.
    /// Skips cleanly on CI runners with no vulkan.
    #[cfg(feature = "runtime")]
    #[test]
    fn per_frame_determinism() {
        // (a) deterministic emit (no GPU needed for this half).
        let art_a = SubstrateKernelArtifact::compile_canonical().unwrap();
        let art_b = SubstrateKernelArtifact::compile_canonical().unwrap();
        assert_eq!(
            art_a.words(),
            art_b.words(),
            "SPIR-V emit must be byte-for-byte deterministic across calls",
        );
        // (b) deterministic dispatch path.
        let Some(mut r1) = try_headless_ash_renderer() else {
            eprintln!("no vulkan · skipping dispatch half of determinism test");
            return;
        };
        let p1 = r1.headless_dispatch().unwrap();
        let mut r2 = try_headless_ash_renderer().unwrap();
        let p2 = r2.headless_dispatch().unwrap();
        // The pipeline handles ARE different (per-vk-context handles), but
        // both must be non-null + both renderers must report the pipeline-
        // built state consistently.
        assert!(p1 != 0 && p2 != 0);
        assert!(r1.pipeline_built() && r2.pipeline_built());
    }

    /// § Determinism (no-runtime) variant — runs on every CI runner so the
    /// emit-path determinism is ALWAYS verified, vulkan or not.
    #[test]
    fn emit_path_deterministic_no_runtime() {
        let a = SubstrateKernelArtifact::compile_canonical().unwrap();
        let b = SubstrateKernelArtifact::compile_canonical().unwrap();
        assert_eq!(
            a.words(),
            b.words(),
            "SPIR-V emit must be byte-for-byte deterministic across calls (no GPU)",
        );
        assert_eq!(a.byte_len(), b.byte_len());
    }

    // ════════════════════════════════════════════════════════════════════════
    // § T11-W18-L7-PRESENT — four-test gate for the swapchain present-path.
    // The tests below exercise the module surface that is reachable without
    // a live winit::Window (constructing a real Window requires an event-loop
    // which can't be created from unit-test threads on Win32). The full
    // end-to-end live-window test is `loa-host` + `LOA_RENDER_V3=1` per the
    // T11-W18-L7-PRESENT integration commit ; that path needs a logged-in
    // desktop session and is verified at user-runtime, not unit-test time.
    // ════════════════════════════════════════════════════════════════════════

    /// § Test #6 : present-path constants are stable.
    /// Verifies the `FRAMES_IN_FLIGHT` constant has the expected triple-
    /// buffering value (3). This is load-bearing because the per-frame
    /// arrays in `AshSwapchainPresenter` are sized at compile-time off this
    /// constant ; if it changed silently the array indexing would compile
    /// but the synchronization invariant could regress.
    /// § T11-W18-FPS-CAP-FIX : updated 2→3 per fps-cap-hunt diagnosis
    #[cfg(feature = "present")]
    #[test]
    fn present_frames_in_flight_constant() {
        assert_eq!(
            FRAMES_IN_FLIGHT, 3,
            "triple-buffered ring · CPU-GPU pipelining requires depth ≥ 3",
        );
    }

    /// § Test #7 : present-path rejects non-Win32 window handles.
    /// Constructs a fake `HasWindowHandle` carrying a Web-flavoured raw
    /// handle and verifies `try_new_with_swapchain` returns
    /// `PresentError::UnsupportedWindowHandle`. The .csl-source path
    /// targets desktop-Windows-only per spec/14_BACKEND ; silent fallback
    /// to a black window is exactly the kind of bug this test catches.
    #[cfg(feature = "present")]
    #[test]
    #[allow(unsafe_code)]
    fn present_rejects_non_win32_handle() {
        use raw_window_handle::{
            HandleError, HasWindowHandle, RawWindowHandle, WebWindowHandle, WindowHandle,
        };

        struct FakeWebWindow {
            web: WebWindowHandle,
        }
        impl HasWindowHandle for FakeWebWindow {
            fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
                let raw = RawWindowHandle::Web(self.web);
                // SAFETY: WindowHandle::borrow_raw demands the raw handle is
                // valid for the borrow ; here our WebWindowHandle is a
                // synthetic struct used only to drive the err-path before any
                // platform call is made. The raw is read by our code, then
                // we error before any FFI dispatch.
                Ok(unsafe { WindowHandle::borrow_raw(raw) })
            }
        }

        let fake = FakeWebWindow {
            web: WebWindowHandle::new(0xDEAD_BEEF),
        };
        let artifact = SubstrateKernelArtifact::compile_canonical().unwrap();
        let result = AshSwapchainPresenter::try_new_with_swapchain(&fake, artifact, (640, 480));
        match result {
            Err(PresentError::UnsupportedWindowHandle) => {} // expected.
            Err(PresentError::Loader(_)) => {
                // CI runners w/o a vulkan loader will land here BEFORE the
                // window-handle inspection. Treat as benign skip.
                eprintln!("no vulkan loader · test skipped");
            }
            Err(other) => panic!("expected UnsupportedWindowHandle err ; got Err({other:?})"),
            Ok(_) => panic!(
                "expected UnsupportedWindowHandle err ; got Ok(AshSwapchainPresenter) — Web window-handle should never produce a live presenter"
            ),
        }
    }

    /// § Test #8 : artifact survives the present-error path.
    /// When `try_new_with_swapchain` errors out with
    /// `UnsupportedWindowHandle`, the input `SubstrateKernelArtifact` was
    /// moved into the function — the error itself is consumable. This test
    /// makes sure we can compile a fresh artifact afterwards (i.e. the
    /// error path doesn't poison any global SPIR-V emit state). Determinism
    /// equivalent for the present-feature build.
    #[cfg(feature = "present")]
    #[test]
    #[allow(unsafe_code)]
    fn present_artifact_recompile_after_err() {
        use raw_window_handle::{
            HandleError, HasWindowHandle, RawWindowHandle, WebWindowHandle, WindowHandle,
        };
        struct FakeWebWindow {
            web: WebWindowHandle,
        }
        impl HasWindowHandle for FakeWebWindow {
            fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
                let raw = RawWindowHandle::Web(self.web);
                Ok(unsafe { WindowHandle::borrow_raw(raw) })
            }
        }

        let fake = FakeWebWindow {
            web: WebWindowHandle::new(7),
        };
        let a1 = SubstrateKernelArtifact::compile_canonical().unwrap();
        let words_before = a1.words().to_vec();
        // We deliberately discard whatever the call returns ; the test is
        // about post-call determinism, not about the call's return value.
        // Drop the Result without inspecting the Ok variant (which doesn't
        // implement Debug).
        drop(AshSwapchainPresenter::try_new_with_swapchain(
            &fake,
            a1,
            (320, 240),
        ));
        let a2 = SubstrateKernelArtifact::compile_canonical().unwrap();
        assert_eq!(
            a2.words(),
            words_before.as_slice(),
            "post-err recompile must yield identical SPIR-V words (no global poison)",
        );
    }

    /// § Test #9 : per-frame determinism — pre-flight half.
    /// `dispatch_with_present` is what the per-frame loop runs ; we cannot
    /// invoke it without a real Win32 surface so this test asserts the
    /// pre-flight invariants the per-frame path relies on : the canonical
    /// kernel artifact's SPIR-V is stable across recompiles AND the
    /// `ObserverCoord` + `Crystal` types serialize to the buffer sizes
    /// the descriptor-set is built around (64-byte uniform · 4096-byte
    /// storage). Catches the failure-mode where someone widens
    /// `ObserverCoord` past 64 bytes without updating the buffer alloc.
    #[cfg(feature = "present")]
    #[test]
    fn present_per_frame_invariants() {
        // (a) artifact recompile is deterministic.
        let a = SubstrateKernelArtifact::compile_canonical().unwrap();
        let b = SubstrateKernelArtifact::compile_canonical().unwrap();
        assert_eq!(a.words(), b.words());

        // (b) ObserverCoord fits in the 64-byte uniform binding.
        assert!(
            std::mem::size_of::<ObserverCoord>() <= 64,
            "ObserverCoord must fit in the 64-byte uniform binding ; \
             grow the buffer alloc in `try_new_with_swapchain` if this fails",
        );

        // (c) CSSL-owned bounded scene fits in the 4096-byte storage binding.
        assert!(
            std::mem::size_of::<Crystal>() * CSSL_MAX_SCENE_CRYSTALS <= 4096,
            "CSSL_MAX_SCENE_CRYSTALS payload must fit in the 4096-byte storage binding",
        );
    }

    #[cfg(feature = "present")]
    #[test]
    fn present_frame_telemetry_line_is_explicitly_non_production() {
        let telemetry = PresentFrameTelemetry {
            frame_slot: 2,
            frame_count_before: 17,
            image_index: 1,
            width: 2560,
            height: 1440,
            present_mode: ash::vk::PresentModeKHR::IMMEDIATE,
            cpu_wait_us: 3,
            cpu_acquire_us: 4,
            cpu_upload_us: 5,
            cpu_record_us: 6,
            cpu_submit_us: 7,
            cpu_present_us: 8,
            previous_gpu_dispatch_ns: Some(14_500),
            gpu_timestamps_available: true,
        };
        let line = telemetry.telemetry_line();
        assert!(line.contains("telemetry.present_frame"));
        assert!(line.contains("use_case=present_loop_probe"));
        assert!(line.contains("realworld_gate=false"));
        assert!(line.contains("p99_us=unmeasured"));
        assert!(line.contains("p99_9_us=unmeasured"));
        assert!(line.contains("width=2560 height=1440"));
        assert!(line.contains("present_mode=IMMEDIATE"));
        assert!(line.contains("previous_gpu_dispatch_us=15"));
    }

    #[cfg(feature = "present")]
    #[test]
    fn present_capture_telemetry_line_reports_pixel_evidence() {
        let capture = PresentCaptureTelemetry {
            width: 2560,
            height: 1440,
            sample_count: 2,
            nonzero_samples: 2,
            alpha255_samples: 1,
            checksum: 0x1234,
            cpu_capture_wait_us: 77,
            artifact_path: Some("target/present_capture.ppm".to_string()),
            pixels: vec![
                PresentCapturedPixel {
                    x: 0,
                    y: 0,
                    rgba: [1, 2, 3, 255],
                },
                PresentCapturedPixel {
                    x: 1280,
                    y: 720,
                    rgba: [9, 8, 7, 6],
                },
            ],
        };
        let line = capture.telemetry_line();
        assert!(line.contains("telemetry.present_capture"));
        assert!(line.contains("use_case=live_pixel_probe"));
        assert!(line.contains("realworld_gate=false"));
        assert!(line.contains("sample_count=2"));
        assert!(line.contains("nonzero_samples=2"));
        assert!(line.contains("alpha255_samples=1"));
        assert!(line.contains("checksum=4660"));
        assert!(line.contains("artifact_path=target/present_capture.ppm"));
        assert!(line.contains("samples=0,0:010203ff,1280,720:09080706"));
    }
}
