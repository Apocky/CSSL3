//! Error type + Result alias for the window host backend.
//!
//! § PATTERN — mirrors `cssl-host-d3d12::error::D3d12Error` (T11-D66) +
//! `cssl-host-vulkan::error::VulkanError` (T11-D65) : a single `WindowError`
//! enum routes all backend-FFI failures + cfg-gate misses through one
//! `thiserror`-derived shape.
//!
//! § PHILOSOPHY (per HANDOFF_SESSION_6 § LANDMINES) :
//!   - LoaderMissing on platforms without an active backend → caller-friendly
//!     diagnostic, never a panic.
//!   - Win32-specific failures preserve the underlying HRESULT / Win32 last-
//!     error code for forensics.
//!   - User-facing variants carry a `&'static str` reason string ; English
//!     prose only @ user-actionable messages (CLAUDE.md global directive).

use thiserror::Error;

/// Crate-wide alias.
pub type Result<T> = core::result::Result<T, WindowError>;

/// Categorized failure surface for the window host backend.
#[derive(Debug, Error)]
pub enum WindowError {
    /// No backend impl is available on the current target.
    ///
    /// Returned by `backend::spawn_window` when compiled for a non-Windows
    /// target. F-axis siblings (Linux X11/Wayland, macOS Cocoa, Web canvas)
    /// land in later slices and will replace this variant on those targets.
    #[error("Window backend missing : no platform impl is available for this target ({reason})")]
    LoaderMissing {
        /// Platform / target description for the caller.
        reason: &'static str,
    },

    /// The underlying OS rejected the request (Win32 last-error / HRESULT).
    ///
    /// `code` is the platform-native error code ; `op` names the failing
    /// API call so callers can route diagnostics. On Win32 `code` is the
    /// value of `GetLastError()` after the failing call.
    #[error("Window OS failure during {op} : platform-error-code = 0x{code:08x}")]
    OsFailure {
        /// Operation name, e.g. `"RegisterClassExW"` / `"CreateWindowExW"`.
        op: &'static str,
        /// Platform-native error code.
        code: u32,
    },

    /// Configuration was rejected before any FFI call (zero size, etc.).
    #[error("Window config invalid : {reason}")]
    InvalidConfig {
        /// Caller-facing reason ; English-prose because surfaced in
        /// diagnostics.
        reason: &'static str,
    },

    /// PRIME-DIRECTIVE consent-arch violation : forbidden close-suppression
    /// pattern attempted at runtime.
    ///
    /// This is fired when user-code calls a path that would silently swallow
    /// a [`crate::event::WindowEventKind::Close`] without explicit
    /// [`crate::Window::dismiss_close_request`] acknowledgement.
    /// Per `PRIME_DIRECTIVE.md § 1 § entrapment` this MUST be a hard error,
    /// never a warning.
    #[error(
        "Window consent-arch violation : close-suppression attempted without \
         explicit dismiss_close_request → see PRIME_DIRECTIVE.md § 1"
    )]
    ConsentViolation,

    /// The event-pump was driven against a window already torn down.
    #[error("Window already destroyed — cannot drive event pump")]
    AlreadyDestroyed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_missing_displays_reason() {
        let err = WindowError::LoaderMissing {
            reason: "linux deferred to F-axis follow-up",
        };
        let s = format!("{err}");
        assert!(s.contains("linux deferred"), "display = {s}");
    }

    #[test]
    fn os_failure_formats_hex_code() {
        let err = WindowError::OsFailure {
            op: "CreateWindowExW",
            code: 0x0000_0005, // ERROR_ACCESS_DENIED
        };
        let s = format!("{err}");
        assert!(s.contains("CreateWindowExW"), "display = {s}");
        assert!(s.contains("0x00000005"), "display = {s}");
    }

    #[test]
    fn invalid_config_carries_reason() {
        let err = WindowError::InvalidConfig {
            reason: "width=0 forbidden",
        };
        assert!(format!("{err}").contains("width=0"));
    }

    #[test]
    fn consent_violation_mentions_prime_directive() {
        let err = WindowError::ConsentViolation;
        assert!(format!("{err}").contains("PRIME_DIRECTIVE"));
    }

    #[test]
    fn already_destroyed_is_terminal() {
        let err = WindowError::AlreadyDestroyed;
        assert!(format!("{err}").contains("already destroyed"));
    }
}
