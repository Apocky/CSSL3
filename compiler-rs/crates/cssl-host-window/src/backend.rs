//! Platform-backend router.
//!
//! § STRATEGY
//!   The router picks the platform impl at compile-time via `cfg(target_os)`.
//!   On Windows targets the active impl is `crate::backend::win32` ; on
//!   every other target the router returns `WindowError::LoaderMissing` —
//!   matching the cssl-host-d3d12 (T11-D66) + cssl-host-vulkan (T11-D65)
//!   precedent.
//!
//! § ADDING A BACKEND (F-axis siblings)
//!   1. Add a new module gated by `cfg(target_os = "linux")` etc.
//!   2. Add a new variant to `WindowInner` (in `crate::window`, private).
//!   3. Add a new variant to [`BackendKind`] for runtime introspection.
//!   4. Wire the cfg-router below.
//!   5. Add a new variant to [`crate::raw_handle::RawWindowHandleKind`].
//!
//!   The window struct's `pump_events` / `raw_handle` / etc. methods will
//!   require new match arms — the explicit non-exhaustive `_` arms in
//!   `cfg(not(target_os = "windows"))` paths catch this at compile-time.

use crate::error::Result;
use crate::window::{Window, WindowConfig, WindowInner};

#[cfg(target_os = "windows")]
pub(crate) mod win32;

/// Categorical identifier of the active backend, for introspection +
/// telemetry routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BackendKind {
    /// Win32 USER32 + Shcore.
    Win32,
    /// No backend available — non-Windows target without an impl yet.
    None,
}

impl BackendKind {
    /// Backend that the current build targets.
    #[must_use]
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Win32
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::None
        }
    }
}

/// Spawn a window using the active platform backend.
///
/// On Windows this calls into the Win32 USER32 surface ; on every other
/// target this returns `WindowError::LoaderMissing` so the workspace builds
/// cleanly while X11 / Wayland / Cocoa land in later F-axis slices.
///
/// # Errors
/// - [`crate::WindowError::InvalidConfig`] if `cfg.validate()` fails.
/// - [`crate::WindowError::OsFailure`] if the OS rejects the request.
/// - [`crate::WindowError::LoaderMissing`] on non-Windows targets.
pub fn spawn_window(cfg: &WindowConfig) -> Result<Window> {
    cfg.validate()?;
    #[cfg(target_os = "windows")]
    {
        let inner = win32::Win32Window::spawn(cfg)?;
        Ok(Window {
            inner: WindowInner::Win32(inner),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(crate::error::WindowError::LoaderMissing {
            reason: "non-Windows backends deferred to later F-axis slices",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_kind_current_matches_cfg() {
        let bk = BackendKind::current();
        #[cfg(target_os = "windows")]
        assert_eq!(bk, BackendKind::Win32);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(bk, BackendKind::None);
    }

    #[test]
    fn backend_kind_distinct_variants() {
        assert_ne!(BackendKind::Win32, BackendKind::None);
    }

    #[test]
    fn spawn_with_invalid_config_returns_invalid_config() {
        let cfg = WindowConfig {
            width: 0,
            ..WindowConfig::default()
        };
        let err = spawn_window(&cfg).unwrap_err();
        assert!(matches!(err, crate::WindowError::InvalidConfig { .. }));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn spawn_on_non_windows_returns_loader_missing() {
        let cfg = WindowConfig::default();
        let err = spawn_window(&cfg).unwrap_err();
        assert!(matches!(err, crate::WindowError::LoaderMissing { .. }));
    }
}
