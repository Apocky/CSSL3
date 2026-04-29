//! `MTLBuffer` allocation + `iso<gpu-buffer>` capability discipline.
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Metal` +
//!          `specs/12_CAPABILITIES.csl § ISO-OWNERSHIP`.
//!
//! § DESIGN
//!   - [`BufferHandle`] is a CSSLv3-side capability-marker over a Metal buffer.
//!     On Apple hosts it carries an `apple::AppleBufferHandle` ; on non-Apple
//!     hosts it carries a `stub::StubBufferHandle` that records the params for
//!     test-shape inspection but never touches FFI.
//!   - The cap-discipline is `iso<gpu-buffer>` — a buffer is owned by exactly
//!     one CSSLv3 holder ; cloning produces a fresh `BufferHandle` with the
//!     refcount bumped (Apple side does the ARC retain ; stub side just
//!     duplicates the record).
//!   - `BufferUsage` annotates intended use (vertex / index / storage / uniform /
//!     argument) for downstream pipeline layout inference.
//!
//! § STORAGE-MODE PORTABILITY
//!   `MTLStorageModeManaged` is **macOS-only** ; iOS / tvOS / visionOS expose
//!   only `Shared` / `Private` / `Memoryless`. Constructors that take an
//!   explicit storage-mode return [`MetalError::ManagedUnavailable`] when the
//!   request is incompatible with the target. The default for cross-Apple
//!   builds is `Shared`.

use crate::error::{MetalError, MetalResult};
use crate::heap::MetalHeapType;

/// Intended use of a Metal buffer (drives root-signature / argument-buffer
/// inference once D-phase emitters land).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferUsage {
    /// Vertex-attribute input.
    Vertex,
    /// Index-buffer input.
    Index,
    /// Compute / fragment storage-buffer (read-write).
    Storage,
    /// Uniform / constant block (read-only on GPU).
    Uniform,
    /// Argument-buffer (tier-1 at stage-0).
    Argument,
}

impl BufferUsage {
    /// Short canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vertex => "vertex",
            Self::Index => "index",
            Self::Storage => "storage",
            Self::Uniform => "uniform",
            Self::Argument => "argument",
        }
    }

    /// All 5 usage modes.
    pub const ALL: [Self; 5] = [
        Self::Vertex,
        Self::Index,
        Self::Storage,
        Self::Uniform,
        Self::Argument,
    ];
}

/// `iso<gpu-buffer>` handle wrapping a Metal buffer (Apple) or a no-op
/// record (non-Apple).
///
/// § The `iso` discipline is enforced at the type-system level by treating
///   `BufferHandle` as a non-`Copy` move-only handle ; aliasing a buffer
///   requires going through the [`Self::clone_handle`] entry-point which
///   bumps the underlying ARC refcount on Apple and records the clone on stub.
#[derive(Debug)]
pub struct BufferHandle {
    /// Number of bytes the buffer was created with.
    pub byte_len: u64,
    /// Effective storage mode (after platform-portability check).
    pub storage_mode: MetalHeapType,
    /// Intended usage for downstream pipeline layout.
    pub usage: BufferUsage,
    /// Capability discipline marker — always `"iso<gpu-buffer>"` for now.
    pub cap: &'static str,
    /// Inner record (Apple) or stub-trail (non-Apple).
    pub(crate) inner: BufferInner,
}

/// Inner buffer state — Apple-side carries an FFI handle ; non-Apple side
/// carries a stub-trail for test inspection.
#[derive(Debug)]
pub(crate) enum BufferInner {
    /// Stub record used when the host is not Apple.
    Stub {
        /// Synthetic handle id for stub equality / clone tracking.
        stub_id: u64,
    },
    /// Apple-side handle — the real `metal::Buffer` is held inside the
    /// `apple` module to keep the FFI surface contained.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    Apple {
        /// Index into the per-session Apple buffer-pool ; resolves to the
        /// real `metal::Buffer` via `MetalSession::resolve_apple_buffer`.
        pool_idx: u32,
    },
}

impl BufferHandle {
    /// Stub constructor used by tests + non-Apple builds.
    #[must_use]
    pub fn stub(byte_len: u64, storage_mode: MetalHeapType, usage: BufferUsage) -> Self {
        Self {
            byte_len,
            storage_mode,
            usage,
            cap: "iso<gpu-buffer>",
            inner: BufferInner::Stub {
                stub_id: stub_id_for(byte_len, storage_mode, usage),
            },
        }
    }

    /// Clone the handle producing a fresh capability marker.
    ///
    /// § On Apple hosts this corresponds to `[buffer retain]` — the underlying
    ///   `MTLBuffer` reference-count is bumped. On non-Apple hosts the stub
    ///   record is duplicated.
    #[must_use]
    pub fn clone_handle(&self) -> Self {
        Self {
            byte_len: self.byte_len,
            storage_mode: self.storage_mode,
            usage: self.usage,
            cap: self.cap,
            inner: match &self.inner {
                BufferInner::Stub { stub_id } => BufferInner::Stub { stub_id: *stub_id },
                #[cfg(any(
                    target_os = "macos",
                    target_os = "ios",
                    target_os = "tvos",
                    target_os = "visionos"
                ))]
                BufferInner::Apple { pool_idx } => BufferInner::Apple {
                    pool_idx: *pool_idx,
                },
            },
        }
    }

    /// Returns `true` when this handle was created via the stub path
    /// (non-Apple host or explicit stub-test constructor).
    #[must_use]
    pub fn is_stub(&self) -> bool {
        matches!(self.inner, BufferInner::Stub { .. })
    }
}

/// Synchronisation hint for `MTLStorageModeManaged` buffers (macOS-only).
///
/// § On macOS, `Managed` buffers require explicit `didModifyRange` /
///   `synchronizeResource` calls when the CPU writes to the buffer and the
///   GPU needs to read those writes (or vice-versa). This enum models the
///   intent so the apple side can call the right Cocoa method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagedBufferSync {
    /// `[buffer didModifyRange:]` — CPU wrote, GPU reads next.
    DidModifyCpuToGpu,
    /// Blit encoder `synchronizeResource` — GPU wrote, CPU reads next.
    SynchronizeGpuToCpu,
}

/// Stub id derivation — deterministic but recognisable so test assertions
/// can match by shape.
const fn stub_id_for(byte_len: u64, mode: MetalHeapType, usage: BufferUsage) -> u64 {
    let mode_tag = match mode {
        MetalHeapType::Shared => 1,
        MetalHeapType::Private => 2,
        MetalHeapType::Managed => 3,
        MetalHeapType::Memoryless => 4,
    };
    let usage_tag = match usage {
        BufferUsage::Vertex => 1,
        BufferUsage::Index => 2,
        BufferUsage::Storage => 3,
        BufferUsage::Uniform => 4,
        BufferUsage::Argument => 5,
    };
    byte_len
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(mode_tag * 7919)
        .wrapping_add(usage_tag * 31)
}

/// Validate a storage-mode + target-OS combination.
///
/// § Returns `Err(ManagedUnavailable)` when `Managed` is requested on a
///   non-macOS Apple host. All other combinations succeed.
pub fn validate_storage_mode(mode: MetalHeapType) -> MetalResult<()> {
    match mode {
        MetalHeapType::Managed => {
            if cfg!(any(
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            )) {
                return Err(MetalError::ManagedUnavailable {
                    mode: "managed",
                    target: crate::error::current_target_os(),
                });
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_storage_mode, BufferHandle, BufferUsage, ManagedBufferSync, MetalHeapType,
    };

    #[test]
    fn buffer_usage_count_is_five() {
        assert_eq!(BufferUsage::ALL.len(), 5);
    }

    #[test]
    fn buffer_usage_names() {
        assert_eq!(BufferUsage::Vertex.as_str(), "vertex");
        assert_eq!(BufferUsage::Argument.as_str(), "argument");
    }

    #[test]
    fn stub_buffer_records_byte_len_mode_usage() {
        let b = BufferHandle::stub(4096, MetalHeapType::Shared, BufferUsage::Storage);
        assert_eq!(b.byte_len, 4096);
        assert_eq!(b.storage_mode, MetalHeapType::Shared);
        assert_eq!(b.usage, BufferUsage::Storage);
        assert_eq!(b.cap, "iso<gpu-buffer>");
        assert!(b.is_stub());
    }

    #[test]
    fn clone_handle_preserves_record_and_cap() {
        let a = BufferHandle::stub(256, MetalHeapType::Private, BufferUsage::Uniform);
        let b = a.clone_handle();
        assert_eq!(a.byte_len, b.byte_len);
        assert_eq!(a.storage_mode, b.storage_mode);
        assert_eq!(a.usage, b.usage);
        assert_eq!(a.cap, b.cap);
    }

    #[test]
    fn managed_sync_variants_are_distinct() {
        assert_ne!(
            ManagedBufferSync::DidModifyCpuToGpu,
            ManagedBufferSync::SynchronizeGpuToCpu
        );
    }

    #[test]
    fn validate_shared_succeeds_on_all_hosts() {
        assert!(validate_storage_mode(MetalHeapType::Shared).is_ok());
    }

    #[test]
    fn validate_private_succeeds_on_all_hosts() {
        assert!(validate_storage_mode(MetalHeapType::Private).is_ok());
    }

    #[test]
    fn validate_memoryless_succeeds_on_all_hosts() {
        assert!(validate_storage_mode(MetalHeapType::Memoryless).is_ok());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn validate_managed_succeeds_on_macos() {
        assert!(validate_storage_mode(MetalHeapType::Managed).is_ok());
    }

    #[test]
    #[cfg(any(target_os = "ios", target_os = "tvos", target_os = "visionos"))]
    fn validate_managed_fails_on_ios_tvos_visionos() {
        let r = validate_storage_mode(MetalHeapType::Managed);
        assert!(r.is_err());
    }

    #[test]
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    )))]
    fn validate_managed_succeeds_on_non_apple_host_path() {
        // § On non-Apple hosts the validator is the same fn ; the cfg-gate
        // at the top excludes the iOS/tvOS/visionOS check, so Managed
        // passes through the validator. Real construction still hits
        // HostNotApple via the session ctor.
        assert!(validate_storage_mode(MetalHeapType::Managed).is_ok());
    }
}
