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
    SubstrateKernelSpec,
};

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
                let q_props =
                    unsafe { instance.get_physical_device_queue_family_properties(pd) };
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
            let device_create_info =
                vk::DeviceCreateInfo::default().queue_create_infos(&queue_create_infos);
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
            let dsl_create_info =
                vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
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
                    .create_compute_pipelines(
                        vk::PipelineCache::null(),
                        &[cp_create_info],
                        None,
                    )
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
            self.build_pipeline()?;
            let pipeline = self
                .compute_pipeline
                .expect("build_pipeline guarantees Some");
            let _pipeline_layout = self
                .pipeline_layout
                .expect("build_pipeline guarantees Some");
            // Allocate a command pool + buffer.
            let cp_create_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(self.compute_queue_family);
            let cmd_pool = unsafe {
                self.device
                    .create_command_pool(&cp_create_info, None)
                    .map_err(AshError::ComputePipelineCreate)?
            };
            let cb_alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(cmd_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd_buffers = unsafe {
                self.device
                    .allocate_command_buffers(&cb_alloc_info)
                    .map_err(AshError::ComputePipelineCreate)?
            };
            let cb = cmd_buffers[0];
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            unsafe {
                self.device
                    .begin_command_buffer(cb, &begin_info)
                    .map_err(AshError::ComputePipelineCreate)?;
                self.device
                    .cmd_bind_pipeline(cb, vk::PipelineBindPoint::COMPUTE, pipeline);
                // 1×1×1 dispatch — verifies the dispatch path without
                // requiring real descriptor-set bindings.
                self.device.cmd_dispatch(cb, 1, 1, 1);
                self.device
                    .end_command_buffer(cb)
                    .map_err(AshError::ComputePipelineCreate)?;
                let submits = [vk::SubmitInfo::default().command_buffers(cmd_buffers.as_slice())];
                self.device
                    .queue_submit(self.compute_queue, &submits, vk::Fence::null())
                    .map_err(AshError::ComputePipelineCreate)?;
                self.device
                    .queue_wait_idle(self.compute_queue)
                    .map_err(AshError::ComputePipelineCreate)?;
                self.device.destroy_command_pool(cmd_pool, None);
            }
            // Cast the pipeline handle to u64 (vk handles are u64-equivalent
            // in vulkan ABI).
            Ok(pipeline.as_raw())
        }
    }

    impl Drop for AshSubstrateRenderer {
        fn drop(&mut self) {
            unsafe {
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
pub use ash_runtime::{try_headless_ash_renderer, AshError, AshSubstrateRenderer};

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

    /// § Number of frames-in-flight kept in the present-ring. Double-buffered
    /// = 2 ; tuned to match a typical desktop swapchain image-count and avoid
    /// stalls without growing memory unbounded.
    pub const FRAMES_IN_FLIGHT: usize = 2;

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
        swapchain_extent: vk::Extent2D,
        swapchain_images: Vec<vk::Image>,
        swapchain_image_views: Vec<vk::ImageView>,
        // § Per-frame sync (ring of FRAMES_IN_FLIGHT).
        image_available: [vk::Semaphore; FRAMES_IN_FLIGHT],
        render_finished: [vk::Semaphore; FRAMES_IN_FLIGHT],
        in_flight_fence: [vk::Fence; FRAMES_IN_FLIGHT],
        // § Command pool + per-frame command-buffers.
        command_pool: vk::CommandPool,
        command_buffers: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
        // § Descriptors : one pool sized for FRAMES_IN_FLIGHT × 3 bindings.
        descriptor_pool: vk::DescriptorPool,
        descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
        // § Observer + crystal stub buffers (16 bytes uniform · 256 bytes storage).
        // Kernel currently has empty body so contents are inert ; we still
        // need real buffer handles so the descriptor-set writes type-check.
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
    /// Kept type-erased here so callers don't need to mirror the .csl
    /// observer/crystal layouts ; the kernel currently has an empty body so
    /// the bytes are accepted blindly.
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
            let win32_surface_loader =
                ash::khr::win32_surface::Instance::new(&entry, &instance);
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
                let qprops =
                    unsafe { instance.get_physical_device_queue_family_properties(pd) };
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
                .or_else(|| formats.iter().find(|f| f.format == vk::Format::R8G8B8A8_UNORM))
                .copied()
                .ok_or(PresentError::NoSuitableFormat)?;
            let present_modes = unsafe {
                surface_loader
                    .get_physical_device_surface_present_modes(physical_device, surface)
                    .map_err(PresentError::SurfaceCreate)?
            };
            let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
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
            //    swapchain image. This is the headline architectural change
            //    vs L7 — the kernel writes pixels straight to the present
            //    image, no vkCmdCopyImage hop.
            let sc_ci = vk::SwapchainCreateInfoKHR::default()
                .surface(surface)
                .min_image_count(image_count)
                .image_format(surface_fmt.format)
                .image_color_space(surface_fmt.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::STORAGE,
                )
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
                let fence_ci = vk::FenceCreateInfo::default()
                    .flags(vk::FenceCreateFlags::SIGNALED);
                in_flight_fence[i] = unsafe {
                    device
                        .create_fence(&fence_ci, None)
                        .map_err(PresentError::FenceCreate)?
                };
            }

            // 13. Observer + crystal stub buffers (host-visible · 64 + 4096 bytes).
            //     Kernel currently has empty body so the contents are inert ;
            //     we still need real buffer handles so descriptor-set writes
            //     pass validation in the host.
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
                swapchain_extent: extent,
                swapchain_images,
                swapchain_image_views,
                image_available,
                render_finished,
                in_flight_fence,
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

        /// § Per-frame compute-dispatch-with-present.
        ///
        /// Acquires the next swapchain image, records a single
        /// `vkCmdDispatch(⌈width/8⌉ × ⌈height/8⌉ × 1)` that writes the
        /// substrate-kernel output directly into the swapchain image as a
        /// storage-image, then presents.
        ///
        /// `_observer` and `_crystals` are accepted for shape-symmetry with
        /// the headless dispatch API ; the canonical kernel currently has an
        /// empty body so the bytes are inert, but the buffer-bindings are
        /// real (so when the kernel body lands the present-path needs no
        /// further plumbing).
        ///
        /// § ERRORS
        ///   - `WaitForFences` / `ResetFences` : fence sync error.
        ///   - `AcquireImage` : surface lost ; caller should rebuild via
        ///     [`Self::recreate_swapchain`].
        ///   - `QueueSubmit` / `QueuePresent` : driver/queue error.
        pub fn dispatch_with_present(
            &mut self,
            _observer: ObserverCoord,
            _crystals: &[Crystal],
        ) -> Result<(), PresentError> {
            let frame = self.current_frame;
            let in_flight = self.in_flight_fence[frame];
            let image_avail = self.image_available[frame];
            let render_done = self.render_finished[frame];
            let cb = self.command_buffers[frame];
            let dset = self.descriptor_sets[frame];

            // 1. Wait for previous use of this slot to finish.
            unsafe {
                self.device
                    .wait_for_fences(&[in_flight], true, u64::MAX)
                    .map_err(PresentError::WaitForFences)?;
                self.device
                    .reset_fences(&[in_flight])
                    .map_err(PresentError::ResetFences)?;
            }

            // 2. Acquire next image.
            let (image_index, _suboptimal) = unsafe {
                self.swapchain_loader
                    .acquire_next_image(
                        self.swapchain,
                        u64::MAX,
                        image_avail,
                        vk::Fence::null(),
                    )
                    .map_err(PresentError::AcquireImage)?
            };
            let image = self.swapchain_images[image_index as usize];
            let image_view = self.swapchain_image_views[image_index as usize];

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

                // Bind + dispatch.
                self.device
                    .cmd_bind_pipeline(cb, vk::PipelineBindPoint::COMPUTE, self.compute_pipeline);
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

                // Barrier 2 : GENERAL → PRESENT_SRC_KHR.
                let to_present = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::empty())
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
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
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_present],
                );

                self.device
                    .end_command_buffer(cb)
                    .map_err(PresentError::EndCommandBuffer)?;
            }

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
            unsafe {
                self.device
                    .queue_submit(self.compute_present_queue, &submit, in_flight)
                    .map_err(PresentError::QueueSubmit)?;
            }

            // 6. Present.
            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&sig_sems)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let _ = unsafe {
                self.swapchain_loader
                    .queue_present(self.compute_present_queue, &present_info)
                    .map_err(PresentError::QueuePresent)?
            };

            self.current_frame = (frame + 1) % FRAMES_IN_FLIGHT;
            self.frame_count = self.frame_count.wrapping_add(1);
            Ok(())
        }

        /// § Recreate swapchain on resize / out-of-date. Tears down per-image
        /// state (image-views) + the swapchain itself ; rebuilds at the new
        /// surface-extent. Sync objects + command-buffers + descriptor-sets
        /// are reused (they're per-frame-in-flight, not per-image).
        ///
        /// § ERRORS
        ///   Same as `try_new_with_swapchain` for the swapchain-rebuild half.
        pub fn recreate_swapchain(
            &mut self,
            new_extent: (u32, u32),
        ) -> Result<(), PresentError> {
            // Wait idle so we don't tear down resources mid-flight.
            unsafe {
                let _ = self.device.device_wait_idle();
                for v in &self.swapchain_image_views {
                    self.device.destroy_image_view(*v, None);
                }
                self.swapchain_image_views.clear();
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
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
                .image_usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::STORAGE,
                )
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(vk::PresentModeKHR::FIFO)
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

    impl Drop for AshSwapchainPresenter {
        fn drop(&mut self) {
            unsafe {
                let _ = self.device.device_wait_idle();
                // Per-frame sync.
                for i in 0..FRAMES_IN_FLIGHT {
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
                    self.device.destroy_descriptor_pool(self.descriptor_pool, None);
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
                    self.swapchain_loader.destroy_swapchain(self.swapchain, None);
                }
                if !self.compute_pipeline.is_null() {
                    self.device.destroy_pipeline(self.compute_pipeline, None);
                }
                if !self.pipeline_layout.is_null() {
                    self.device.destroy_pipeline_layout(self.pipeline_layout, None);
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
    AshSwapchainPresenter, Crystal, ObserverCoord, PresentError, FRAMES_IN_FLIGHT,
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
    /// Verifies the `FRAMES_IN_FLIGHT` constant has the expected double-
    /// buffering value (2). This is load-bearing because the per-frame
    /// arrays in `AshSwapchainPresenter` are sized at compile-time off this
    /// constant ; if it changed silently the array indexing would compile
    /// but the synchronization invariant could regress.
    #[cfg(feature = "present")]
    #[test]
    fn present_frames_in_flight_constant() {
        assert_eq!(
            FRAMES_IN_FLIGHT, 2,
            "double-buffered ring assumed throughout the present-path",
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

        // (c) 256 Crystals fit in the 4096-byte storage binding.
        assert!(
            std::mem::size_of::<Crystal>() * 256 <= 4096,
            "256-Crystal payload must fit in the 4096-byte storage binding",
        );
    }
}
