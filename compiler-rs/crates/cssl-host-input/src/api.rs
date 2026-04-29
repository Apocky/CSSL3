//! § Cross-platform backend selection.
//!
//! § ROLE
//!   Per-OS modules (`win32`, `linux`, `macos`) implement the
//!   [`crate::backend::InputBackend`] trait. This module selects the
//!   active backend at compile time via `cfg` and re-exports it under
//!   stable cross-platform type-aliases ([`ActiveBackend`]).
//!
//!   Per the slice landmines : "Apocky's host = Win11 + Arc A770 ; Win32
//!   path is integration-tested. Linux/macOS structurally tested."

/// Identifier of the currently-active backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    /// Win32 raw-input + XInput backend.
    Win32,
    /// Linux evdev + libudev backend.
    Linux,
    /// macOS IOKit HID Manager backend.
    MacOS,
    /// Stub backend — present on every host as a fallback for hosts that
    /// don't have a real OS-input surface (tests, headless CI, etc.).
    Stub,
}

impl BackendKind {
    /// The backend that this build links against, determined at
    /// compile-time from the target triple.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Win32
        }
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Self::MacOS
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Self::Stub
        }
    }

    /// Returns `true` if this backend is the integration-tested primary
    /// (Win32 today on Apocky's host).
    #[must_use]
    pub const fn is_integration_tested(self) -> bool {
        matches!(self, Self::Win32)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ActiveBackend type-alias.
// ───────────────────────────────────────────────────────────────────────

/// The active per-OS backend type for this build.
///
/// Source-level CSSLv3 code uses this type alias to remain OS-agnostic
/// — the same `let backend : ActiveBackend = ...` line works on every
/// host.
#[cfg(target_os = "windows")]
pub type ActiveBackend = crate::win32::Win32Backend;

/// The active per-OS backend type for this build.
#[cfg(target_os = "linux")]
pub type ActiveBackend = crate::linux::LinuxBackend;

/// The active per-OS backend type for this build.
#[cfg(target_os = "macos")]
pub type ActiveBackend = crate::macos::MacosBackend;

/// The active per-OS backend type for this build (fallback).
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub type ActiveBackend = crate::stub::StubBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_backend_matches_target() {
        let kind = BackendKind::current();
        #[cfg(target_os = "windows")]
        assert_eq!(kind, BackendKind::Win32);
        #[cfg(target_os = "linux")]
        assert_eq!(kind, BackendKind::Linux);
        #[cfg(target_os = "macos")]
        assert_eq!(kind, BackendKind::MacOS);
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        assert_eq!(kind, BackendKind::Stub);
    }

    #[test]
    fn integration_tested_marker() {
        // Exactly one variant currently marked integration-tested.
        let count = [
            BackendKind::Win32,
            BackendKind::Linux,
            BackendKind::MacOS,
            BackendKind::Stub,
        ]
        .iter()
        .filter(|k| k.is_integration_tested())
        .count();
        assert_eq!(count, 1);
        assert!(BackendKind::Win32.is_integration_tested());
    }
}
