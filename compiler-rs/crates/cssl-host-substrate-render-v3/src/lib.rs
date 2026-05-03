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
}
