//! High-level Metal session — RAII bundle of device + queue + library + pipelines.
//!
//! § DESIGN
//!   On Apple hosts the session opens a default `MTLDevice`, allocates a
//!   command queue, and exposes [`MetalSession::open`] / `compile_compute_pipeline`
//!   / `make_buffer` / `make_event` / `make_fence` entry-points that delegate
//!   to the `apple` module's FFI implementation.
//!
//!   On non-Apple hosts every entry-point returns
//!   [`MetalError::HostNotApple`] except [`MetalSession::open_stub`] which is
//!   the non-FFI test ctor used by both Apocky's Windows host smoke tests and
//!   non-Apple build verification.
//!
//! § CAP DISCIPLINE
//!   `MetalSession` itself is `iso<metal-session>` — exclusive ownership of
//!   the underlying `MTLDevice` + queue. Borrowing produces buffer / pipeline
//!   handles whose lifetimes are tied to the session's lifetime by Rust's
//!   borrow-checker (no explicit `'a` parameters needed at this level —
//!   handles carry their own ARC retain on Apple).

use crate::buffer::{validate_storage_mode, BufferHandle, BufferUsage};
use crate::command::CommandQueueHandle;
use crate::device::MtlDevice;
use crate::error::{MetalError, MetalResult};
use crate::heap::MetalHeapType;
use crate::pipeline::{ComputePipelineDescriptor, PipelineHandle, RenderPipelineDescriptor};
use crate::sync::{EventHandle, FenceHandle};

/// Session configuration.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Session label (debug-info ; surfaced via `[device setLabel:]` on Apple).
    pub label: String,
    /// Default storage mode for buffers without an explicit override.
    pub default_storage_mode: MetalHeapType,
    /// Number of command queues to spin up on session-open.
    pub num_queues: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            label: "cssl_metal_session".into(),
            // § Per the handoff landmine note : iOS / tvOS / visionOS lack
            // `Managed` storage mode. Default to `Shared` for cross-platform
            // Apple correctness. Callers on macOS who want CPU-side caching
            // can opt-in to `Managed` via `make_buffer_with_storage_mode`.
            default_storage_mode: MetalHeapType::Shared,
            num_queues: 1,
        }
    }
}

/// High-level Metal session.
#[derive(Debug)]
pub struct MetalSession {
    /// Effective configuration.
    pub config: SessionConfig,
    /// Stub-side device record (Apple-side has the real one inside `apple`).
    pub device_record: MtlDevice,
    /// Number of buffers / pipelines / events / fences allocated to date.
    pub allocations: SessionAllocations,
    /// Inner state (Apple vs stub).
    pub(crate) inner: SessionInner,
}

/// Per-session allocation counters (observability).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SessionAllocations {
    /// Buffers allocated.
    pub buffers: u64,
    /// Pipelines compiled.
    pub pipelines: u64,
    /// Events created.
    pub events: u64,
    /// Fences created.
    pub fences: u64,
    /// Command queues created.
    pub queues: u64,
}

#[derive(Debug)]
pub(crate) enum SessionInner {
    /// Stub state — no FFI, just records the construction.
    Stub,
    /// Apple state — opaque handle to the apple-module's session record.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    Apple {
        /// Index into the apple-module's session-pool ; resolves the real
        /// `metal::Device` + `metal::CommandQueue` + sub-pools.
        pool_idx: u32,
    },
}

impl MetalSession {
    /// Open a stub session — no FFI required. Always succeeds on every host.
    /// Used by tests + by the non-Apple production code paths.
    #[must_use]
    pub fn open_stub(config: SessionConfig) -> Self {
        Self {
            config,
            device_record: MtlDevice::stub_m3_max(),
            allocations: SessionAllocations::default(),
            inner: SessionInner::Stub,
        }
    }

    /// Open a session against the host's default Metal device.
    ///
    /// On Apple hosts this calls `MTLCreateSystemDefaultDevice()` ; on
    /// non-Apple hosts it returns `MetalError::HostNotApple`.
    pub fn open(config: SessionConfig) -> MetalResult<Self> {
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "visionos"
        ))]
        {
            crate::apple::session_ops::open(config)
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "visionos"
        )))]
        {
            let _ = config;
            Err(MetalError::host_not_apple())
        }
    }

    /// Allocate a buffer with the session's default storage mode.
    pub fn make_buffer(&mut self, byte_len: u64, usage: BufferUsage) -> MetalResult<BufferHandle> {
        self.make_buffer_with_storage_mode(byte_len, self.config.default_storage_mode, usage)
    }

    /// Allocate a buffer with an explicit storage mode.
    pub fn make_buffer_with_storage_mode(
        &mut self,
        byte_len: u64,
        mode: MetalHeapType,
        usage: BufferUsage,
    ) -> MetalResult<BufferHandle> {
        validate_storage_mode(mode)?;
        match &self.inner {
            SessionInner::Stub => {
                self.allocations.buffers += 1;
                Ok(BufferHandle::stub(byte_len, mode, usage))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            SessionInner::Apple { pool_idx } => {
                let h = crate::apple::session_ops::make_buffer(*pool_idx, byte_len, mode, usage)?;
                self.allocations.buffers += 1;
                Ok(h)
            }
        }
    }

    /// Compile a compute pipeline.
    pub fn compile_compute_pipeline(
        &mut self,
        desc: ComputePipelineDescriptor,
    ) -> MetalResult<PipelineHandle> {
        match &self.inner {
            SessionInner::Stub => {
                self.allocations.pipelines += 1;
                Ok(PipelineHandle::stub_compute(desc))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            SessionInner::Apple { pool_idx } => {
                let h = crate::apple::session_ops::compile_compute_pipeline(*pool_idx, desc)?;
                self.allocations.pipelines += 1;
                Ok(h)
            }
        }
    }

    /// Compile a render pipeline.
    pub fn compile_render_pipeline(
        &mut self,
        desc: RenderPipelineDescriptor,
    ) -> MetalResult<PipelineHandle> {
        match &self.inner {
            SessionInner::Stub => {
                self.allocations.pipelines += 1;
                Ok(PipelineHandle::stub_render(desc))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            SessionInner::Apple { pool_idx } => {
                let h = crate::apple::session_ops::compile_render_pipeline(*pool_idx, desc)?;
                self.allocations.pipelines += 1;
                Ok(h)
            }
        }
    }

    /// Create a command queue on this session.
    pub fn make_command_queue(
        &mut self,
        label: impl Into<String>,
    ) -> MetalResult<CommandQueueHandle> {
        let label = label.into();
        match &self.inner {
            SessionInner::Stub => {
                self.allocations.queues += 1;
                Ok(CommandQueueHandle::stub(label, 64))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            SessionInner::Apple { pool_idx } => {
                let q = crate::apple::session_ops::make_command_queue(*pool_idx, label)?;
                self.allocations.queues += 1;
                Ok(q)
            }
        }
    }

    /// Create a shared event for cross-queue synchronisation.
    pub fn make_event(&mut self, label: impl Into<String>) -> MetalResult<EventHandle> {
        let label = label.into();
        match &self.inner {
            SessionInner::Stub => {
                self.allocations.events += 1;
                Ok(EventHandle::stub(label))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            SessionInner::Apple { pool_idx } => {
                let e = crate::apple::session_ops::make_event(*pool_idx, label)?;
                self.allocations.events += 1;
                Ok(e)
            }
        }
    }

    /// Create a fence for intra-queue producer-consumer synchronisation.
    pub fn make_fence(&mut self, label: impl Into<String>) -> MetalResult<FenceHandle> {
        let label = label.into();
        match &self.inner {
            SessionInner::Stub => {
                self.allocations.fences += 1;
                Ok(FenceHandle::stub(label))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            SessionInner::Apple { pool_idx } => {
                let f = crate::apple::session_ops::make_fence(*pool_idx, label)?;
                self.allocations.fences += 1;
                Ok(f)
            }
        }
    }

    /// Returns `true` when this session was opened against the stub backend
    /// (non-Apple host or explicit `open_stub`).
    #[must_use]
    pub fn is_stub(&self) -> bool {
        matches!(self.inner, SessionInner::Stub)
    }
}

#[cfg(test)]
mod tests {
    use super::{MetalSession, SessionAllocations, SessionConfig};
    use crate::buffer::BufferUsage;
    use crate::heap::MetalHeapType;
    use crate::pipeline::{ComputePipelineDescriptor, RenderPipelineDescriptor};

    #[test]
    fn default_session_config_uses_shared_storage() {
        let c = SessionConfig::default();
        assert_eq!(c.default_storage_mode, MetalHeapType::Shared);
        assert_eq!(c.num_queues, 1);
    }

    #[test]
    fn open_stub_succeeds_with_zero_allocations() {
        let s = MetalSession::open_stub(SessionConfig::default());
        assert!(s.is_stub());
        assert_eq!(s.allocations, SessionAllocations::default());
    }

    #[test]
    fn make_buffer_default_uses_session_storage_mode() {
        let mut s = MetalSession::open_stub(SessionConfig::default());
        let b = s.make_buffer(1024, BufferUsage::Storage).unwrap();
        assert_eq!(b.byte_len, 1024);
        assert_eq!(b.storage_mode, MetalHeapType::Shared);
        assert_eq!(s.allocations.buffers, 1);
    }

    #[test]
    fn make_buffer_with_explicit_storage_mode() {
        let mut s = MetalSession::open_stub(SessionConfig::default());
        let b = s
            .make_buffer_with_storage_mode(2048, MetalHeapType::Private, BufferUsage::Vertex)
            .unwrap();
        assert_eq!(b.storage_mode, MetalHeapType::Private);
        assert_eq!(b.byte_len, 2048);
    }

    #[test]
    #[cfg(any(target_os = "ios", target_os = "tvos", target_os = "visionos"))]
    fn managed_storage_rejected_on_ios_tvos_visionos() {
        let mut s = MetalSession::open_stub(SessionConfig::default());
        let r = s.make_buffer_with_storage_mode(64, MetalHeapType::Managed, BufferUsage::Storage);
        assert!(matches!(
            r,
            Err(crate::error::MetalError::ManagedUnavailable { .. })
        ));
    }

    #[test]
    fn compile_compute_pipeline_stub_round_trip() {
        let mut s = MetalSession::open_stub(SessionConfig::default());
        let d = ComputePipelineDescriptor::new("c", "kernel ...", "main");
        let p = s.compile_compute_pipeline(d).unwrap();
        assert!(p.is_stub());
        assert_eq!(s.allocations.pipelines, 1);
    }

    #[test]
    fn compile_render_pipeline_stub_round_trip() {
        let mut s = MetalSession::open_stub(SessionConfig::default());
        let d = RenderPipelineDescriptor::new("r", "src", "vs", "fs");
        let p = s.compile_render_pipeline(d).unwrap();
        assert!(p.is_stub());
    }

    #[test]
    fn make_command_queue_increments_counter() {
        let mut s = MetalSession::open_stub(SessionConfig::default());
        let _ = s.make_command_queue("q1").unwrap();
        let _ = s.make_command_queue("q2").unwrap();
        assert_eq!(s.allocations.queues, 2);
    }

    #[test]
    fn make_event_and_fence_increment_counters() {
        let mut s = MetalSession::open_stub(SessionConfig::default());
        let _ = s.make_event("e1").unwrap();
        let _ = s.make_fence("f1").unwrap();
        assert_eq!(s.allocations.events, 1);
        assert_eq!(s.allocations.fences, 1);
    }

    #[test]
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    )))]
    fn open_real_session_returns_host_not_apple_on_windows() {
        let r = MetalSession::open(SessionConfig::default());
        assert!(matches!(
            r,
            Err(crate::error::MetalError::HostNotApple { .. })
        ));
    }
}
