//! Raw OS-window-handle exposure for swapchain interop with E1+E2.
//!
//! § PURPOSE
//!   `cssl-host-vulkan` (E1, T11-D65) requires an `HWND` to call
//!   `vkCreateWin32SurfaceKHR` ; `cssl-host-d3d12` (E2, T11-D66) requires the
//!   same `HWND` to call `IDXGIFactory::CreateSwapChainForHwnd`. This module
//!   exposes the raw handle in a backend-agnostic shape so the GPU host
//!   crates can stay generic over the windowing system.
//!
//! § PRIVACY
//!   The handle is OPAQUE to user-code at the API level — `HWND` is exposed
//!   as `usize` so callers cannot accidentally call USER32 APIs through the
//!   crate boundary. The GPU host crates that need real Win32 types convert
//!   via `windows::Win32::Foundation::HWND(handle as *mut _)` at their own
//!   FFI boundary, where `unsafe` is already opted-in.
//!
//! § DESIGN
//!   The enum is `#[non_exhaustive]` so X11Display+X11Window /
//!   WaylandSurface+Display / NSWindow / WebCanvas variants land cleanly in
//!   later F-axis slices. We deliberately do NOT pull in `raw-window-handle
//!   0.6` ; the upstream crate's API churn is not yet pinned per CSSLv3 R16
//!   reproducibility-anchor. This crate's type is FFI-equivalent and
//!   trivially-convertible at the swapchain-creation site.

/// Backend-tagged raw window handle, suitable for swapchain creation.
///
/// `Copy` because the underlying values are bag-of-bits (POD pointers); the
/// owning [`crate::Window`] retains ownership and validity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawWindowHandle {
    pub kind: RawWindowHandleKind,
}

/// Backend-tagged variants. `#[non_exhaustive]` — callers MUST handle a
/// fall-through arm so future additions are non-breaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RawWindowHandleKind {
    /// Win32 (HWND + HINSTANCE).
    ///
    /// `hwnd` is the window handle ; `hinstance` is the module instance
    /// passed to `RegisterClassExW`. Both are stored as `usize` to keep the
    /// crate `cfg`-portable ; users that need real Win32 types convert at
    /// the FFI boundary in `cssl-host-d3d12` / `cssl-host-vulkan`.
    Win32 { hwnd: usize, hinstance: usize },
    // X11 / Wayland / Cocoa / Web variants — placeholder slots filled by
    // later F-axis slices. We intentionally leave them off the enum until
    // the corresponding backend lands so the enum is never "lying" about
    // what shapes it supports.
}

impl RawWindowHandle {
    /// Construct a Win32 handle pair.
    #[must_use]
    pub fn win32(hwnd: usize, hinstance: usize) -> Self {
        Self {
            kind: RawWindowHandleKind::Win32 { hwnd, hinstance },
        }
    }

    /// `true` when this handle targets a Win32 window.
    #[must_use]
    pub fn is_win32(&self) -> bool {
        matches!(self.kind, RawWindowHandleKind::Win32 { .. })
    }

    /// Extract the Win32 (HWND, HINSTANCE) pair if applicable.
    #[must_use]
    pub fn as_win32(&self) -> Option<(usize, usize)> {
        match self.kind {
            RawWindowHandleKind::Win32 { hwnd, hinstance } => Some((hwnd, hinstance)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_round_trips_handles() {
        let h = RawWindowHandle::win32(0x1234, 0x5678);
        assert!(h.is_win32());
        assert_eq!(h.as_win32(), Some((0x1234, 0x5678)));
    }

    #[test]
    fn raw_handle_is_copy_and_eq() {
        let h1 = RawWindowHandle::win32(0xAA, 0xBB);
        let h2 = h1; // Copy
        assert_eq!(h1, h2);
        assert_eq!(h1.as_win32(), h2.as_win32());
    }
}
