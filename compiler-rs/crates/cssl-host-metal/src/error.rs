//! Metal host error taxonomy.
//!
//! § The `MetalError` taxonomy is shared between the Apple-real and non-Apple
//!   stub paths so user code matches once. Stub paths return
//!   `MetalError::HostNotApple` from every fallible entry-point ; Apple paths
//!   surface real Cocoa / `MTLDevice::new` / pipeline-compile failures.

use thiserror::Error;

/// Convenient `Result` alias for Metal host operations.
pub type MetalResult<T> = Result<T, MetalError>;

/// Failure modes for the Metal host backend.
///
/// § Variant `HostNotApple` is returned by every fallible entry-point on
///   non-Apple hosts where the `metal` crate is not compiled in. Other
///   variants surface only on Apple hosts where the FFI layer is active.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MetalError {
    /// The current host is not Apple-platform ; the stub backend is active.
    #[error(
        "Metal host backend is Apple-only (current target: {target}); stub returned HostNotApple"
    )]
    HostNotApple {
        /// Resolved `target_os` string at compile-time.
        target: &'static str,
    },

    /// `MTLCreateSystemDefaultDevice()` returned `nil` — no GPU available.
    #[error("MTLCreateSystemDefaultDevice returned nil — no Metal-capable GPU on this host")]
    NoDefaultDevice,

    /// Requested storage mode unavailable on this Apple platform.
    ///
    /// `MTLStorageModeManaged` exists on macOS only ; iOS / tvOS / visionOS
    /// must use `Shared` for cross-platform Apple correctness.
    #[error("storage mode {mode} unavailable on target {target}")]
    ManagedUnavailable {
        /// Storage-mode short-name (e.g., `"managed"`).
        mode: &'static str,
        /// Target OS short-name (e.g., `"ios"`).
        target: &'static str,
    },

    /// MSL library compile failed with the supplied diagnostic.
    #[error("MTLLibrary newWithSource failed: {detail}")]
    LibraryCompileFailed {
        /// Raw NSError-derived diagnostic.
        detail: String,
    },

    /// Compute pipeline creation failed.
    #[error("MTLComputePipelineState creation failed: {detail}")]
    ComputePipelineFailed {
        /// Raw NSError-derived diagnostic.
        detail: String,
    },

    /// Render pipeline creation failed.
    #[error("MTLRenderPipelineState creation failed: {detail}")]
    RenderPipelineFailed {
        /// Raw NSError-derived diagnostic.
        detail: String,
    },

    /// Buffer allocation failed (out-of-memory or invalid options).
    #[error("MTLBuffer allocation failed: requested {bytes} bytes mode={mode}")]
    BufferAllocFailed {
        /// Bytes requested.
        bytes: u64,
        /// Storage-mode short-name.
        mode: &'static str,
    },

    /// Command-buffer commit failed (encoder still alive, queue invalid, etc.).
    #[error("MTLCommandBuffer commit failed: {detail}")]
    CommitFailed {
        /// Failure reason.
        detail: String,
    },

    /// Wait-for-completion timed out at the runtime level.
    #[error("MTLCommandBuffer wait timed out after {timeout_ms} ms")]
    WaitTimeout {
        /// Timeout in milliseconds.
        timeout_ms: u64,
    },

    /// Telemetry-ring push failed (full ring + overflow-saturated).
    #[error("Metal telemetry-ring push failed: {detail}")]
    TelemetryFull {
        /// Reason from the underlying ring.
        detail: String,
    },

    /// Generic Cocoa / Foundation runtime error wrapping an `NSError`.
    #[error("Cocoa runtime error: {detail}")]
    CocoaError {
        /// `[NSError localizedDescription]` capture.
        detail: String,
    },
}

impl MetalError {
    /// Construct the canonical not-Apple error using the compile-time `target_os`.
    #[must_use]
    pub const fn host_not_apple() -> Self {
        Self::HostNotApple {
            target: current_target_os(),
        }
    }
}

/// Compile-time `target_os` short-name used in error variants.
#[must_use]
pub const fn current_target_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "tvos") {
        "tvos"
    } else if cfg!(target_os = "visionos") {
        "visionos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_family = "wasm") {
        "wasm"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::{current_target_os, MetalError};

    #[test]
    fn host_not_apple_carries_compile_time_target() {
        let e = MetalError::host_not_apple();
        match e {
            MetalError::HostNotApple { target } => {
                assert_eq!(target, current_target_os());
            }
            _ => panic!("expected HostNotApple"),
        }
    }

    #[test]
    fn current_target_os_is_well_known() {
        let t = current_target_os();
        assert!(matches!(
            t,
            "macos"
                | "ios"
                | "tvos"
                | "visionos"
                | "windows"
                | "linux"
                | "android"
                | "wasm"
                | "unknown"
        ));
    }

    #[test]
    fn error_display_for_host_not_apple_contains_target() {
        let e = MetalError::host_not_apple();
        let s = format!("{e}");
        assert!(s.contains("Apple-only"));
        assert!(s.contains(current_target_os()));
    }

    #[test]
    fn error_display_for_managed_unavailable_includes_mode_and_target() {
        let e = MetalError::ManagedUnavailable {
            mode: "managed",
            target: "ios",
        };
        let s = format!("{e}");
        assert!(s.contains("managed"));
        assert!(s.contains("ios"));
    }

    #[test]
    fn error_display_for_library_compile_failed_includes_detail() {
        let e = MetalError::LibraryCompileFailed {
            detail: "syntax error at line 1".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("MTLLibrary"));
        assert!(s.contains("syntax error"));
    }

    #[test]
    fn error_eq_is_value_eq() {
        let a = MetalError::WaitTimeout { timeout_ms: 1000 };
        let b = MetalError::WaitTimeout { timeout_ms: 1000 };
        assert_eq!(a, b);
        let c = MetalError::WaitTimeout { timeout_ms: 2000 };
        assert_ne!(a, c);
    }

    #[test]
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    )))]
    fn current_target_os_on_windows_or_linux_is_not_apple() {
        let t = current_target_os();
        assert!(!matches!(t, "macos" | "ios" | "tvos" | "visionos"));
    }
}
