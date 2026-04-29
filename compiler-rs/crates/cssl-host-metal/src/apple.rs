//! Apple-platform real-FFI implementation.
//!
//! § This module compiles only on macOS / iOS / tvOS / visionOS where the
//!   `metal` crate is available. The cfg-gate is in `lib.rs`.
//!
//! § DESIGN
//!   - A process-global `AppleSessionPool` (behind a `Mutex<RefCell<Vec<...>>>`)
//!     holds real `metal::Device` / `metal::CommandQueue` / `metal::Library`
//!     instances. `MetalSession`'s public type carries only a `pool_idx: u32`
//!     which resolves to the real handle inside this module.
//!   - This indirection keeps the cssl-host-metal **public** types
//!     `Cargo.toml`-free of Apple-only types — non-Apple builds compile the
//!     same `MetalSession` / `BufferHandle` / `PipelineHandle` shapes without
//!     a single `metal::*` symbol leaking through the cfg fence.
//!   - The `metal` crate already wraps Cocoa retain/release ; the
//!     cssl-host-metal abstraction does not leak ARC semantics to user code.
//!
//! § APOCKY-HOST NOTE
//!   The S6-E3 slice is dispatched from a Windows host. Cargo on Windows will
//!   never compile this module ; the workspace's `cargo check --workspace`
//!   gate runs against the non-Apple cfg path. A future macOS CI runner will
//!   exercise the live FFI shape via `#[cfg(target_os = "macos")]` integration
//!   tests in this crate.

#![cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "visionos"
))]

use core::cell::RefCell;
use std::sync::Mutex;

use metal::{
    Buffer as MtlMetalBuffer, CommandQueue as MtlMetalQueue,
    ComputePipelineDescriptor as MtlComputeDesc, Device as MtlMetalDevice, Event as MtlMetalEvent,
    Fence as MtlMetalFence, Library as MtlMetalLibrary, MTLResourceOptions, MTLStorageMode,
    RenderPipelineDescriptor as MtlRenderDesc,
};

use crate::buffer::{BufferHandle, BufferInner, BufferUsage};
use crate::command::{CommandQueueHandle, CommandQueueInner};
use crate::device::{GpuFamily, MtlDevice};
use crate::error::{MetalError, MetalResult};
use crate::heap::MetalHeapType;
use crate::pipeline::{
    ComputePipelineDescriptor, PipelineHandle, PipelineInner, RenderPipelineDescriptor,
};
use crate::session::{MetalSession, SessionAllocations, SessionConfig, SessionInner};
use crate::sync::{EventHandle, EventInner, FenceHandle, FenceInner, SignalToken};

/// Apple-side session record — owns the `metal::Device` + `metal::CommandQueue`
/// + sub-pools (buffers / pipelines / events / fences).
pub(crate) struct AppleSession {
    pub(crate) device: MtlMetalDevice,
    pub(crate) buffers: Vec<MtlMetalBuffer>,
    pub(crate) compute_pipelines: Vec<metal::ComputePipelineState>,
    pub(crate) render_pipelines: Vec<metal::RenderPipelineState>,
    pub(crate) queues: Vec<MtlMetalQueue>,
    pub(crate) events: Vec<MtlMetalEvent>,
    pub(crate) fences: Vec<MtlMetalFence>,
}

/// Process-global pool of `AppleSession`s. The Mutex makes it safe to spin up
/// multiple `MetalSession`s concurrently (e.g., from multiple threads in a
/// future async runtime) ; the RefCell inside is for interior mutability.
fn pool() -> &'static Mutex<RefCell<Vec<AppleSession>>> {
    use std::sync::OnceLock;
    static POOL: OnceLock<Mutex<RefCell<Vec<AppleSession>>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(RefCell::new(Vec::new())))
}

/// Map a CSSLv3 `MetalHeapType` to a Metal `MTLStorageMode`.
fn map_storage_mode(mode: MetalHeapType) -> MTLStorageMode {
    match mode {
        MetalHeapType::Shared => MTLStorageMode::Shared,
        MetalHeapType::Private => MTLStorageMode::Private,
        MetalHeapType::Managed => MTLStorageMode::Managed,
        MetalHeapType::Memoryless => MTLStorageMode::Memoryless,
    }
}

/// Map a `MetalHeapType` to a `MTLResourceOptions` value carrying just the
/// storage-mode bits. Hazard-tracking + cache-mode bits are added by the
/// caller per the buffer's `BufferUsage`.
fn map_resource_options(mode: MetalHeapType) -> MTLResourceOptions {
    match mode {
        MetalHeapType::Shared => MTLResourceOptions::StorageModeShared,
        MetalHeapType::Private => MTLResourceOptions::StorageModePrivate,
        MetalHeapType::Managed => MTLResourceOptions::StorageModeManaged,
        MetalHeapType::Memoryless => MTLResourceOptions::StorageModeMemoryless,
    }
}

/// Inspect a `metal::Device` to derive the CSSLv3 [`MtlDevice`] record.
fn read_device_record(d: &MtlMetalDevice) -> MtlDevice {
    let name = d.name().to_string();
    let registry_id = d.registry_id();
    let supports_raytracing = d.supports_raytracing();
    let supports_function_pointers = d.supports_function_pointers();
    let supports_dynamic_libraries = d.supports_dynamic_libraries();
    let max_buffer_length = d.max_buffer_length();
    let has_unified_memory = d.has_unified_memory();
    // § GPU-family detection : `metal-rs` exposes `supports_family` for each
    //   `MTLGPUFamily::*`. We pick the highest family the device supports
    //   so downstream code matches the device's true tier.
    let gpu_family = derive_gpu_family(d);
    MtlDevice {
        name,
        registry_id,
        supports_raytracing,
        supports_function_pointers,
        supports_dynamic_libraries,
        max_buffer_length,
        has_unified_memory,
        gpu_family,
    }
}

/// Pick the highest `GpuFamily` the device supports.
fn derive_gpu_family(d: &MtlMetalDevice) -> GpuFamily {
    use metal::MTLGPUFamily;
    let candidates = [
        (MTLGPUFamily::Apple9, GpuFamily::Apple9),
        (MTLGPUFamily::Apple8, GpuFamily::Apple8),
        (MTLGPUFamily::Apple7, GpuFamily::Apple7),
        (MTLGPUFamily::Apple6, GpuFamily::Apple6),
        (MTLGPUFamily::Apple5, GpuFamily::Apple5),
        (MTLGPUFamily::Apple4, GpuFamily::Apple4),
        (MTLGPUFamily::Apple3, GpuFamily::Apple3),
        (MTLGPUFamily::Apple2, GpuFamily::Apple2),
        (MTLGPUFamily::Apple1, GpuFamily::Apple1),
        (MTLGPUFamily::Mac2, GpuFamily::Mac2),
        (MTLGPUFamily::Mac1, GpuFamily::Mac1),
        (MTLGPUFamily::Common3, GpuFamily::Common3),
        (MTLGPUFamily::Common2, GpuFamily::Common2),
        (MTLGPUFamily::Common1, GpuFamily::Common1),
    ];
    for (mtl, ours) in candidates {
        if d.supports_family(mtl) {
            return ours;
        }
    }
    GpuFamily::Common1
}

/// Apple-side public ops (called from `session.rs`).
pub(crate) mod session_ops {
    use super::{
        map_resource_options, map_storage_mode, pool, read_device_record, AppleSession,
        BufferHandle, BufferInner, BufferUsage, CommandQueueHandle, CommandQueueInner,
        ComputePipelineDescriptor, EventHandle, EventInner, FenceHandle, FenceInner, MetalError,
        MetalResult, MetalSession, MtlComputeDesc, MtlMetalDevice, MtlRenderDesc, PipelineHandle,
        PipelineInner, RenderPipelineDescriptor, SessionAllocations, SessionConfig, SessionInner,
        SignalToken,
    };

    use metal::{Device, MTLSize};

    /// Open a session against the system default device.
    pub fn open(config: SessionConfig) -> MetalResult<MetalSession> {
        let device = Device::system_default().ok_or(MetalError::NoDefaultDevice)?;
        let queue = device.new_command_queue();
        let device_record = read_device_record(&device);
        let session = AppleSession {
            device,
            buffers: Vec::new(),
            compute_pipelines: Vec::new(),
            render_pipelines: Vec::new(),
            queues: vec![queue],
            events: Vec::new(),
            fences: Vec::new(),
        };
        let pool = pool();
        let lock = pool.lock().expect("metal session pool poisoned");
        let mut sessions = lock.borrow_mut();
        let idx = sessions.len() as u32;
        sessions.push(session);
        Ok(MetalSession {
            config,
            device_record,
            allocations: SessionAllocations::default(),
            inner: SessionInner::Apple { pool_idx: idx },
        })
    }

    /// Allocate a buffer on the apple-side session.
    pub fn make_buffer(
        pool_idx: u32,
        byte_len: u64,
        mode: crate::heap::MetalHeapType,
        usage: BufferUsage,
    ) -> MetalResult<BufferHandle> {
        let pool = pool();
        let lock = pool.lock().expect("metal session pool poisoned");
        let mut sessions = lock.borrow_mut();
        let session =
            sessions
                .get_mut(pool_idx as usize)
                .ok_or_else(|| MetalError::CocoaError {
                    detail: format!("invalid session pool_idx {pool_idx}"),
                })?;
        let opts = map_resource_options(mode);
        let buf = session.device.new_buffer(byte_len, opts);
        let buf_idx = session.buffers.len() as u32;
        session.buffers.push(buf);
        let _ = map_storage_mode(mode); // verifies mapping unconditionally
        Ok(BufferHandle {
            byte_len,
            storage_mode: mode,
            usage,
            cap: "iso<gpu-buffer>",
            inner: BufferInner::Apple { pool_idx: buf_idx },
        })
    }

    /// Compile a compute pipeline.
    pub fn compile_compute_pipeline(
        pool_idx: u32,
        desc: ComputePipelineDescriptor,
    ) -> MetalResult<PipelineHandle> {
        let pool = pool();
        let lock = pool.lock().expect("metal session pool poisoned");
        let mut sessions = lock.borrow_mut();
        let session =
            sessions
                .get_mut(pool_idx as usize)
                .ok_or_else(|| MetalError::CocoaError {
                    detail: format!("invalid session pool_idx {pool_idx}"),
                })?;
        let library = session
            .device
            .new_library_with_source(&desc.msl_source, &metal::CompileOptions::new())
            .map_err(|e| MetalError::LibraryCompileFailed { detail: e })?;
        let function = library
            .get_function(&desc.entry_point, None)
            .map_err(|e| MetalError::ComputePipelineFailed { detail: e })?;
        let pipeline_desc = MtlComputeDesc::new();
        pipeline_desc.set_compute_function(Some(&function));
        pipeline_desc.set_label(&desc.label);
        let state = session
            .device
            .new_compute_pipeline_state(&pipeline_desc)
            .map_err(|e| MetalError::ComputePipelineFailed { detail: e })?;
        let pipe_idx = session.compute_pipelines.len() as u32;
        session.compute_pipelines.push(state);
        // Suppress unused-warning for the threadgroup hint until full encode-path lands.
        let _ = MTLSize::new(
            u64::from(desc.threadgroup_size.0),
            u64::from(desc.threadgroup_size.1),
            u64::from(desc.threadgroup_size.2),
        );
        Ok(PipelineHandle {
            label: desc.label,
            kind: crate::pipeline::PipelineKind::Compute,
            inner: PipelineInner::AppleCompute { pool_idx: pipe_idx },
        })
    }

    /// Compile a render pipeline.
    pub fn compile_render_pipeline(
        pool_idx: u32,
        desc: RenderPipelineDescriptor,
    ) -> MetalResult<PipelineHandle> {
        let pool = pool();
        let lock = pool.lock().expect("metal session pool poisoned");
        let mut sessions = lock.borrow_mut();
        let session =
            sessions
                .get_mut(pool_idx as usize)
                .ok_or_else(|| MetalError::CocoaError {
                    detail: format!("invalid session pool_idx {pool_idx}"),
                })?;
        let library = session
            .device
            .new_library_with_source(&desc.msl_source, &metal::CompileOptions::new())
            .map_err(|e| MetalError::LibraryCompileFailed { detail: e })?;
        let vfn = library
            .get_function(&desc.vertex_entry, None)
            .map_err(|e| MetalError::RenderPipelineFailed { detail: e })?;
        let ffn = library
            .get_function(&desc.fragment_entry, None)
            .map_err(|e| MetalError::RenderPipelineFailed { detail: e })?;
        let pipeline_desc = MtlRenderDesc::new();
        pipeline_desc.set_vertex_function(Some(&vfn));
        pipeline_desc.set_fragment_function(Some(&ffn));
        pipeline_desc.set_label(&desc.label);
        // § Color attachment 0 — pixel format from the descriptor (default
        // `bgra8Unorm`). Unknown formats fall back to `Invalid` and the
        // compile fails with a descriptive `RenderPipelineFailed`.
        let color = pipeline_desc
            .color_attachments()
            .object_at(0)
            .ok_or_else(|| MetalError::RenderPipelineFailed {
                detail: "color attachment 0 unavailable".into(),
            })?;
        color.set_pixel_format(map_pixel_format(&desc.color_pixel_format));
        let state = session
            .device
            .new_render_pipeline_state(&pipeline_desc)
            .map_err(|e| MetalError::RenderPipelineFailed { detail: e })?;
        let pipe_idx = session.render_pipelines.len() as u32;
        session.render_pipelines.push(state);
        Ok(PipelineHandle {
            label: desc.label,
            kind: crate::pipeline::PipelineKind::Render,
            inner: PipelineInner::AppleRender { pool_idx: pipe_idx },
        })
    }

    /// Map a CSSLv3 pixel-format string to `MTLPixelFormat`.
    fn map_pixel_format(s: &str) -> metal::MTLPixelFormat {
        use metal::MTLPixelFormat;
        match s {
            "bgra8Unorm" => MTLPixelFormat::BGRA8Unorm,
            "rgba8Unorm" => MTLPixelFormat::RGBA8Unorm,
            "rgba16Float" => MTLPixelFormat::RGBA16Float,
            "rgba32Float" => MTLPixelFormat::RGBA32Float,
            _ => MTLPixelFormat::Invalid,
        }
    }

    /// Create a command queue.
    pub fn make_command_queue(pool_idx: u32, label: String) -> MetalResult<CommandQueueHandle> {
        let pool = pool();
        let lock = pool.lock().expect("metal session pool poisoned");
        let mut sessions = lock.borrow_mut();
        let session =
            sessions
                .get_mut(pool_idx as usize)
                .ok_or_else(|| MetalError::CocoaError {
                    detail: format!("invalid session pool_idx {pool_idx}"),
                })?;
        let q = session.device.new_command_queue();
        q.set_label(&label);
        let q_idx = session.queues.len() as u32;
        session.queues.push(q);
        Ok(CommandQueueHandle {
            label,
            max_command_buffer_count: 64,
            inner: CommandQueueInner::Apple { pool_idx: q_idx },
        })
    }

    /// Create a shared event.
    pub fn make_event(pool_idx: u32, label: String) -> MetalResult<EventHandle> {
        let pool = pool();
        let lock = pool.lock().expect("metal session pool poisoned");
        let mut sessions = lock.borrow_mut();
        let session =
            sessions
                .get_mut(pool_idx as usize)
                .ok_or_else(|| MetalError::CocoaError {
                    detail: format!("invalid session pool_idx {pool_idx}"),
                })?;
        let event = session.device.new_event();
        let e_idx = session.events.len() as u32;
        session.events.push(event);
        Ok(EventHandle {
            label,
            last_signaled: SignalToken::ZERO,
            inner: EventInner::Apple { pool_idx: e_idx },
        })
    }

    /// Create a fence.
    pub fn make_fence(pool_idx: u32, label: String) -> MetalResult<FenceHandle> {
        let pool = pool();
        let lock = pool.lock().expect("metal session pool poisoned");
        let mut sessions = lock.borrow_mut();
        let session =
            sessions
                .get_mut(pool_idx as usize)
                .ok_or_else(|| MetalError::CocoaError {
                    detail: format!("invalid session pool_idx {pool_idx}"),
                })?;
        let fence = session.device.new_fence();
        fence.set_label(&label);
        let f_idx = session.fences.len() as u32;
        session.fences.push(fence);
        Ok(FenceHandle {
            label,
            updated: false,
            inner: FenceInner::Apple { pool_idx: f_idx },
        })
    }
}

#[cfg(test)]
mod tests {
    //! Apple-only integration tests — exercised on macOS / iOS / tvOS / visionOS
    //! CI runners. On Apocky's Windows host these are not compiled in (the
    //! parent module is cfg-gated out).

    use super::session_ops;
    use crate::buffer::BufferUsage;
    use crate::heap::MetalHeapType;
    use crate::msl_blob::MslShaderSet;
    use crate::pipeline::{ComputePipelineDescriptor, RenderPipelineDescriptor};
    use crate::session::SessionConfig;

    #[test]
    fn open_session_succeeds_when_default_device_exists() {
        let s = session_ops::open(SessionConfig::default());
        // § Apple host MUST have a default Metal device — assertion here is
        // strict (test will only run on Apple CI runners).
        assert!(s.is_ok());
    }

    #[test]
    fn make_buffer_with_shared_storage() {
        let mut s = session_ops::open(SessionConfig::default()).unwrap();
        let b = s.make_buffer(64, BufferUsage::Storage).unwrap();
        assert_eq!(b.byte_len, 64);
        assert_eq!(b.storage_mode, MetalHeapType::Shared);
        assert!(!b.is_stub());
    }

    #[test]
    fn compile_compute_placeholder_kernel() {
        let mut s = session_ops::open(SessionConfig::default()).unwrap();
        let set = MslShaderSet::placeholder();
        let d =
            ComputePipelineDescriptor::new("placeholder", set.compute_source, set.compute_entry);
        let p = s.compile_compute_pipeline(d).unwrap();
        assert!(!p.is_stub());
    }

    #[test]
    fn compile_render_placeholder_pair() {
        let mut s = session_ops::open(SessionConfig::default()).unwrap();
        let set = MslShaderSet::placeholder();
        // The render pipeline expects vertex + fragment in a single source.
        let combined = format!("{}\n{}", set.vertex_source, set.fragment_source);
        let d = RenderPipelineDescriptor::new(
            "placeholder",
            combined,
            set.vertex_entry,
            set.fragment_entry,
        );
        let p = s.compile_render_pipeline(d).unwrap();
        assert!(!p.is_stub());
    }
}
