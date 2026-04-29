//! `Window` + `WindowConfig` — the user-facing API surface.
//!
//! § DESIGN
//!   `Window` is the owning handle ; dropping it tears down the OS window.
//!   `WindowConfig` is the pre-construction blob ; user-code builds one,
//!   passes to [`crate::backend::spawn_window`], gets back a `Window`.
//!
//! § THREADING
//!   The Win32 backend creates the OS window from the calling thread (Win32
//!   message-pump thread-affinity is well-known) ; this is enforced by the
//!   pump being a `&mut self` method that cannot be called from another
//!   thread without coordination. F2 / F3 may extend with a separate audio
//!   thread but the message-pump itself stays on the construction thread.
//!
//! § CSSL-RT INTEGRATION
//!   The pump is poll-style (`pump_events` returns immediately after
//!   draining the OS queue), matching the cssl-rt async-readiness deferred
//!   status. Future F-axis slices may add an opt-in `wait_event` blocking
//!   variant once cssl-rt async support lands.

use crate::consent::{CloseDispositionPolicy, CloseRequestState};
use crate::error::{Result, WindowError};
use crate::event::WindowEvent;
use crate::raw_handle::RawWindowHandle;

/// Pre-construction window configuration.
///
/// Use the [`Default`] impl + setters for ergonomic construction. All
/// fields except `title` round-trip via `Copy`.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    /// Window title — visible in the title-bar + taskbar.
    pub title: String,
    /// Initial client-area width in physical pixels.
    pub width: u32,
    /// Initial client-area height in physical pixels.
    pub height: u32,
    /// User can resize the window via window edges.
    pub resizable: bool,
    /// Vsync hint — actual presentation cadence is controlled by the GPU
    /// host backend (E1 Vulkan / E2 D3D12) ; this is an advisory.
    pub vsync_hint: WindowVsyncHint,
    /// Fullscreen state at creation.
    pub fullscreen: WindowFullscreen,
    /// PRIME-DIRECTIVE close-request disposition policy. Defaults to
    /// `AutoGrantAfterGrace { 5000 }` — the only universally-safe default.
    pub close_disposition: CloseDispositionPolicy,
    /// Opt into per-monitor v2 DPI awareness on Win32. Defaults to `true`
    /// (recommended on Windows 10+ ; required for crisp rendering on
    /// HiDPI displays).
    pub dpi_aware: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "CSSLv3 window".into(),
            width: 1280,
            height: 720,
            resizable: true,
            vsync_hint: WindowVsyncHint::Vsync,
            fullscreen: WindowFullscreen::Windowed,
            close_disposition: CloseDispositionPolicy::default(),
            dpi_aware: true,
        }
    }
}

impl WindowConfig {
    /// Convenience constructor with title + dimensions ; the rest defaults.
    #[must_use]
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            ..Self::default()
        }
    }

    /// Validate the config without trying to spawn ; returns
    /// `WindowError::InvalidConfig` on illegal values.
    pub fn validate(&self) -> Result<()> {
        if self.width == 0 {
            return Err(WindowError::InvalidConfig {
                reason: "width = 0 is illegal",
            });
        }
        if self.height == 0 {
            return Err(WindowError::InvalidConfig {
                reason: "height = 0 is illegal",
            });
        }
        if self.title.is_empty() {
            return Err(WindowError::InvalidConfig {
                reason: "title must be non-empty",
            });
        }
        // PRIME-DIRECTIVE check : disposition policy MUST be present.
        // Default impl supplies a safe policy ; this assertion catches
        // future mistakes if someone tries to manually construct a
        // forbidden state.
        match self.close_disposition {
            CloseDispositionPolicy::AutoGrantAfterGrace { grace } => {
                // grace.ms == 0 with AutoGrantAfterGrace = nonsensical.
                // Per consent::CloseDispositionPolicy docstring, grace=0
                // requires policy=RequireExplicit. Catch + reject here.
                if grace.ms == 0 {
                    return Err(WindowError::InvalidConfig {
                        reason: "AutoGrantAfterGrace with grace=0 is illegal — \
                                 use RequireExplicit instead",
                    });
                }
            }
            CloseDispositionPolicy::RequireExplicit { .. } => { /* always OK */ }
        }
        Ok(())
    }
}

/// Vsync hint — advisory to the GPU swapchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowVsyncHint {
    /// Match the display refresh rate (default).
    Vsync,
    /// Present immediately, tear-allowed (for benchmarks / latency-bound
    /// scenarios).
    Immediate,
    /// Adaptive sync (Mailbox / FIFO_RELAXED on Vulkan, AllowTearing on
    /// DXGI). Requires platform support — host backend may downgrade to
    /// `Vsync`.
    Adaptive,
}

/// Fullscreen disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFullscreen {
    /// Windowed (default).
    Windowed,
    /// Borderless full-screen on the primary monitor.
    BorderlessOnPrimary,
    /// Exclusive full-screen on the primary monitor (mode-change). Stage-0
    /// only ; multi-monitor selection lands in a later F-axis slice.
    ExclusiveOnPrimary,
}

/// Owning handle for a live OS window.
///
/// Drop tears down the OS window via the platform backend's destructor
/// (Win32: `DestroyWindow`). The Debug impl elides FFI-handle internals
/// since they are platform-specific opaque pointers.
pub struct Window {
    pub(crate) inner: WindowInner,
}

impl core::fmt::Debug for Window {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Window")
            .field("backend", &self.inner.backend_kind_str())
            .field("destroyed", &self.is_destroyed())
            .finish()
    }
}

/// Type-erased platform-backed implementation. Not part of the public API.
pub(crate) enum WindowInner {
    #[cfg(target_os = "windows")]
    Win32(crate::backend::win32::Win32Window),
    /// Stub for non-Windows targets — unused at runtime since
    /// `spawn_window` returns `LoaderMissing` before this variant is
    /// reachable. Kept so the enum is never empty (Rust requires at least
    /// one variant) on non-Windows builds.
    #[allow(dead_code)]
    Stub,
}

impl WindowInner {
    fn backend_kind_str(&self) -> &'static str {
        match self {
            #[cfg(target_os = "windows")]
            Self::Win32(_) => "Win32",
            Self::Stub => "Stub",
        }
    }
}

impl Window {
    /// Drain pending OS messages + return the resulting [`WindowEvent`]s.
    /// Always returns immediately ; use a frame-pump loop pattern.
    pub fn pump_events(&mut self) -> Result<Vec<WindowEvent>> {
        match &mut self.inner {
            #[cfg(target_os = "windows")]
            WindowInner::Win32(w) => w.pump_events(),
            WindowInner::Stub => Err(WindowError::LoaderMissing {
                reason: "no platform backend on this target",
            }),
        }
    }

    /// Return the window's raw OS handle for swapchain interop with
    /// `cssl-host-vulkan` (E1) / `cssl-host-d3d12` (E2).
    ///
    /// On Win32 this is `(HWND, HINSTANCE)` packed as `usize` pair (see
    /// [`RawWindowHandle::win32`]).
    pub fn raw_handle(&self) -> Result<RawWindowHandle> {
        match &self.inner {
            #[cfg(target_os = "windows")]
            WindowInner::Win32(w) => Ok(w.raw_handle()),
            WindowInner::Stub => Err(WindowError::LoaderMissing {
                reason: "no platform backend on this target",
            }),
        }
    }

    /// Caller-visible state of the in-flight close request.
    pub fn close_request_state(&self) -> CloseRequestState {
        match &self.inner {
            #[cfg(target_os = "windows")]
            WindowInner::Win32(w) => w.close_state,
            WindowInner::Stub => CloseRequestState::Idle,
        }
    }

    /// Request the OS-level destroy ; window is gone after the next pump.
    /// This is the GRANT-side of the consent-arch — user-code calls it
    /// after observing a `Close` event + deciding to honor it.
    pub fn request_destroy(&mut self) -> Result<()> {
        match &mut self.inner {
            #[cfg(target_os = "windows")]
            WindowInner::Win32(w) => w.request_destroy(),
            WindowInner::Stub => Err(WindowError::LoaderMissing {
                reason: "no platform backend on this target",
            }),
        }
    }

    /// Dismiss an in-flight close-request. The `Pending` state transitions
    /// to `Dismissed` ; the window stays open.
    ///
    /// This is the explicit acknowledgement required by the consent-arch
    /// per `PRIME_DIRECTIVE.md § 1 § entrapment`. Silent default-dismiss
    /// is forbidden — that path returns `WindowError::ConsentViolation`.
    pub fn dismiss_close_request(&mut self) -> Result<()> {
        match &mut self.inner {
            #[cfg(target_os = "windows")]
            WindowInner::Win32(w) => w.dismiss_close_request(),
            WindowInner::Stub => Err(WindowError::LoaderMissing {
                reason: "no platform backend on this target",
            }),
        }
    }

    /// `true` if the OS has destroyed the window (post-`request_destroy` or
    /// post-`Close` grant). Subsequent `pump_events` calls return
    /// `WindowError::AlreadyDestroyed`.
    pub fn is_destroyed(&self) -> bool {
        match &self.inner {
            #[cfg(target_os = "windows")]
            WindowInner::Win32(w) => w.is_destroyed(),
            WindowInner::Stub => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::GraceWindowConfig;

    #[test]
    fn config_default_validates() {
        let cfg = WindowConfig::default();
        assert!(cfg.validate().is_ok(), "default cfg must validate");
    }

    #[test]
    fn config_zero_width_rejected() {
        let cfg = WindowConfig {
            width: 0,
            ..WindowConfig::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(matches!(err, WindowError::InvalidConfig { .. }));
    }

    #[test]
    fn config_zero_height_rejected() {
        let cfg = WindowConfig {
            height: 0,
            ..WindowConfig::default()
        };
        assert!(matches!(
            cfg.validate(),
            Err(WindowError::InvalidConfig { .. })
        ));
    }

    #[test]
    fn config_empty_title_rejected() {
        let cfg = WindowConfig {
            title: String::new(),
            ..WindowConfig::default()
        };
        assert!(matches!(
            cfg.validate(),
            Err(WindowError::InvalidConfig { .. })
        ));
    }

    #[test]
    fn config_auto_grant_zero_grace_rejected() {
        let cfg = WindowConfig {
            close_disposition: CloseDispositionPolicy::AutoGrantAfterGrace {
                grace: GraceWindowConfig { ms: 0 },
            },
            ..WindowConfig::default()
        };
        let err = cfg.validate().unwrap_err();
        let s = format!("{err}");
        assert!(s.contains("RequireExplicit"), "diag = {s}");
    }

    #[test]
    fn config_require_explicit_validates() {
        let cfg = WindowConfig {
            close_disposition: CloseDispositionPolicy::RequireExplicit {
                consent_arch_audit_window_ms: 60_000,
            },
            ..WindowConfig::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn config_new_sets_title_and_dims() {
        let cfg = WindowConfig::new("hello", 800, 600);
        assert_eq!(cfg.title, "hello");
        assert_eq!(cfg.width, 800);
        assert_eq!(cfg.height, 600);
    }

    #[test]
    fn vsync_variants_distinct() {
        assert_ne!(WindowVsyncHint::Vsync, WindowVsyncHint::Immediate);
        assert_ne!(WindowVsyncHint::Immediate, WindowVsyncHint::Adaptive);
    }

    #[test]
    fn fullscreen_variants_distinct() {
        assert_ne!(
            WindowFullscreen::Windowed,
            WindowFullscreen::BorderlessOnPrimary
        );
        assert_ne!(
            WindowFullscreen::BorderlessOnPrimary,
            WindowFullscreen::ExclusiveOnPrimary
        );
    }
}
