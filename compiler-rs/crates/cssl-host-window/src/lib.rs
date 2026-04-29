//! CSSLv3 stage0 — Window host backend (Phase F foundation).
//!
//! § SPEC : `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS (extended at F1) +
//!          `specs/10_HW.csl` § OS (Windows + Linux primary).
//!
//! § STRATEGY (T11-D78, S7-F1)
//!   On Windows targets the impl uses `windows-rs 0.58` to wrap real Win32
//!   USER32 + KERNEL32 + Shcore : window-class registration, CreateWindowExW,
//!   message-pump (PeekMessageW + TranslateMessage + DispatchMessageW),
//!   per-monitor-v2 DPI awareness, and graceful DestroyWindow shutdown.
//!
//!   On non-Windows targets every constructor returns
//!   [`error::WindowError::LoaderMissing`] so the workspace `cargo check`
//!   stays green on Linux + macOS. X11 / Wayland / Cocoa backends are
//!   deferred to a future F-axis slice (this matches cssl-host-d3d12
//!   (T11-D66) + cssl-host-vulkan (T11-D65) precedent).
//!
//! § UNSAFE
//!   FFI work is opt-in : `unsafe` is allowed only at specific call-sites
//!   wrapping `windows-rs` interfaces inside `crate::backend::win32`, never
//!   spread crate-wide. Each unsafe block carries a `// SAFETY :` comment.
//!
//! § PRIME-DIRECTIVE — KILL-SWITCH (consent-arch)
//!   The window's close-button MUST always emit
//!   [`event::WindowEventKind::Close`]. User code that intercepts Close
//!   (to confirm-quit, save state, etc.) MUST explicitly call
//!   [`Window::dismiss_close_request`] to consume the request — silent
//!   default-suppress is FORBIDDEN per
//!   `PRIME_DIRECTIVE.md § 1 PROHIBITIONS § entrapment`.
//!
//!   See `consent::CloseRequestState` for the full state-machine + the
//!   anti-trap grace-window enforcement.
//!
//! § HANDLE INTEROP
//!   The Window exposes its raw OS handle via [`Window::raw_handle`] →
//!   [`raw_handle::RawWindowHandle`]. On Win32 this is `(HWND, HINSTANCE)`,
//!   suitable for direct consumption by `cssl-host-vulkan` swapchain creation
//!   (`VkWin32SurfaceCreateInfoKHR`) + `cssl-host-d3d12` swapchain creation
//!   (`IDXGIFactory::CreateSwapChainForHwnd`).
//!
//!   Cross-platform handle expansion (Wayland / X11 / Cocoa / Web) lands in
//!   later F-axis slices ; the enum is pre-shaped so additions are
//!   backwards-compatible.
//!
//! § F-AXIS SCOPE
//!   F1 (this slice)  : window foundation                         ← here
//!   F2               : input (KB / mouse / gamepad / XInput)     ← deferred
//!   F3               : audio (WASAPI / ALSA / PulseAudio / CoreAudio)
//!   F4               : networking (Win32 Sockets + BSD sockets)
//!   F5 (optional)    : clipboard + file-dialog
//!
//!   Input event-shapes are scoped HERE at the API level so F2 has a stable
//!   target ; full input dispatch lands in F2.

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod backend;
pub mod consent;
pub mod error;
pub mod event;
pub mod raw_handle;
pub mod window;

pub use backend::{spawn_window, BackendKind};
pub use consent::{CloseDispositionPolicy, CloseRequestState, GraceWindowConfig};
pub use error::{Result, WindowError};
pub use event::{KeyCode, ModifierKeys, MouseButton, ScrollDelta, WindowEvent, WindowEventKind};
pub use raw_handle::{RawWindowHandle, RawWindowHandleKind};
pub use window::{Window, WindowConfig, WindowFullscreen, WindowVsyncHint};
