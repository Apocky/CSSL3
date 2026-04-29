//! § F1 (host-window) ↔ F2 (host-input) integration contract.
//!
//! § STATUS
//!   F1 (`cssl-host-window`) has not landed on `parallel-fanout` as of
//!   S7-F2's worktree base (`df1daf5`). This module documents the API
//!   contract that F1 MUST conform to so the F1 ↔ F2 wire-up is a
//!   one-line ceremony once F1 lands.
//!
//! § CONTRACT
//!
//! ## 1. Window-handle vending
//!
//! F1 must expose a stable `WindowHandle` newtype wrapping the raw
//! OS window pointer :
//!   - Win32  : `HWND` (`*mut c_void`)
//!   - Linux  : `Display* + Window` pair (or `wl_surface*` for Wayland)
//!   - macOS  : `NSWindow*` (or its `CAMetalLayer` for the rendering
//!              surface) plus the `NSView*` for input attachment
//!
//! The handle MUST be `Copy` and `Send`-safe with the documented
//! lifetime contract that the handle is invalidated when the window is
//! destroyed (the consumer is responsible for not using the handle
//! after destruction — F1 emits a window-destroy event ; the input
//! backend MUST detach in response).
//!
//! ```text
//! // F1 pseudocode (proposed)
//! pub struct WindowHandle { ... }
//! impl WindowHandle {
//!     pub fn raw_hwnd(&self) -> *mut c_void;
//!     pub fn raw_xlib_window(&self) -> (*mut c_void, u64);
//!     pub fn raw_nswindow(&self) -> *mut c_void;
//! }
//! ```
//!
//! ## 2. Event-loop ownership
//!
//! Two strategies are documented (the slice brief allows either) :
//!
//!   **A. Shared event loop** — F1 owns the OS message-pump (Win32
//!      `GetMessage` / `PeekMessage`, Linux `XNextEvent` /
//!      `wl_display_dispatch`, macOS `NSApp.run`). F1 calls F2's
//!      `InputBackend::process_*` entry-points for every input-related
//!      event. Application-level code calls
//!      [`crate::backend::InputBackend::poll_events`] / `current_state`
//!      between frames.
//!
//!   **B. Poll-driven (no shared loop)** — F2 runs its own message
//!      pump (Windows : `MsgWaitForMultipleObjects` ;
//!      Linux : independent `epoll` on `/dev/input/event*` fds ;
//!      macOS : `CFRunLoopRun` on a worker thread). F1 doesn't need
//!      to know about input events. Suits headless or tool-style
//!      hosts.
//!
//!   The contract recommendation is **A (shared event loop)** because
//!   the slice landmines mandate raw-input WM_INPUT processing on the
//!   same thread that owns the window message-pump (or via a
//!   dedicated raw-input thread + lock-free queue) — option (A) is the
//!   minimum-friction path. The same-thread invariant is:
//!     - Win32 raw-input : `WM_INPUT` always delivered to the WndProc
//!       on the thread that registered the device. Crossing threads
//!       triggers per-PostMessage races.
//!     - Linux  : evdev fds are owned by the epoll set ; a dedicated
//!       input thread is fine here (no shared state with the X /
//!       Wayland connection).
//!     - macOS  : IOKit HID delivers callbacks on the run-loop thread
//!       that registered them.
//!
//! ## 3. Event types
//!
//! F1 forwards the following Win32 messages to F2 (or their
//! Linux / macOS equivalents) :
//!
//!   ```text
//!   WM_INPUT                    → process_raw_input_keyboard / mouse
//!   WM_WTSSESSION_CHANGE         → on_session_lock_change(locked)
//!   WM_QUERYENDSESSION /         → on_session_end()
//!   WM_ENDSESSION
//!   WM_DESTROY                   → detach()  (F1 owns the lifecycle)
//!   ```
//!
//!   Linux equivalents :
//!   ```text
//!   epoll on /dev/input/event*  → process_event(slot, raw)
//!   logind Session.Locked DBus  → on_session_lock_change(locked)
//!   ```
//!
//!   macOS equivalents :
//!   ```text
//!   IOHIDValueCallback            → process_event(InputEvent)
//!   NSWorkspaceWillSleepNoti      → on_session_lock_change(true)
//!   ```
//!
//! ## 4. Grab acquisition
//!
//! When the application calls
//! [`crate::backend::InputBackend::acquire_grab`], F2 needs F1's
//! window handle to call the OS-level grab functions :
//!   - Win32  : `SetCapture(hwnd)` + `ClipCursor(window-rect)` +
//!              `ShowCursor(false)`
//!   - Linux  : `XGrabPointer(display, window, ...)` /
//!              `wl_pointer_lock`
//!   - macOS  : `CGAssociateMouseAndMouseCursorPosition(false)` +
//!              `[NSCursor hide]`
//!
//! The contract is that F1 hands its `InputAttachment` (the F1-side
//! window-pump handle) to F2 once at startup ; F2 holds the attachment
//! for the lifetime of the grab. PRIME-DIRECTIVE-honouring kill-switch
//! fires through the same attachment to release the OS-level grab
//! BEFORE any application code observes the kill-switch event.
//!
//! ## 5. Stub mode
//!
//! When F1 is not present (headless / test mode), F2 falls through
//! to the [`crate::stub::StubBackend`] which never grabs and never
//! generates real OS input — only synthetic events from
//! [`crate::stub::StubBackend::inject_event`]. This keeps unit tests
//! independent of any window system.
//!
//! § DOCUMENTED API SHAPE (placeholder until F1 lands)
//!
//! The traits below describe the F1 ↔ F2 surface. F1 will impl them
//! when it lands ; today they're documented for discoverability.

use crate::api::BackendKind;

/// The contract F1 (`cssl-host-window`) MUST implement so F2 can
/// attach to its message pump.
///
/// **F1 has not yet landed.** This trait documents the shape ; once F1
/// lands it will be moved to `cssl-host-window::input_attachment`
/// and a one-line `cssl-host-input::ActiveBackend::attach_window`
/// wrapper will accept any `&mut dyn WindowMessagePump`.
pub trait WindowMessagePump {
    /// Returns the OS-specific window handle as a raw pointer that the
    /// OS-level grab APIs need.
    ///
    /// The returned pointer is valid for the lifetime of the F1 window.
    /// Per §1 PRIME-DIRECTIVE COGNITIVE-INTEGRITY rule that the system
    /// must not lie about its state, F1's `WindowHandle` MUST be
    /// non-NULL while the window is alive.
    fn window_handle_raw(&self) -> *mut std::ffi::c_void;

    /// Returns the [`BackendKind`] this pump's window system is meant
    /// to drive (so F2 can pick the matching backend).
    fn target_backend_kind(&self) -> BackendKind;

    /// Returns `true` if the window has been destroyed. F2 MUST drop
    /// any grab and stop calling other methods after this returns
    /// `true`.
    fn is_destroyed(&self) -> bool;
}

/// The contract F2 (`cssl-host-input`) implements for F1 to forward
/// OS messages into.
///
/// F1's WndProc / event-handler loop calls these methods as
/// equivalent OS messages arrive. The methods are no-args because
/// the input backend already holds the per-OS state ; F1 just hands
/// off the platform-specific raw-event bytes via the dedicated
/// per-backend entry-points (`process_raw_input_keyboard` on Win32,
/// `process_event` on Linux + macOS).
///
/// This trait is the F2 ↔ F1 ABI : F1 holds an `&mut dyn
/// WindowEventSink` and calls the appropriate method per OS event.
pub trait WindowEventSink {
    /// Notifies F2 that the OS reported a session-lock state change.
    /// `locked = true` => session newly locked (Win+L equivalent).
    fn on_session_lock_change(&mut self, locked: bool);

    /// Notifies F2 that the OS is ending the session (sign-out / shutdown).
    fn on_session_end(&mut self);

    /// Notifies F2 that the window is being destroyed. F2 MUST drop any
    /// active grab and clear all input state.
    fn on_window_destroy(&mut self);
}

// ───────────────────────────────────────────────────────────────────────
// § Default impl wrappers around the per-OS backends.
// ───────────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
impl WindowEventSink for crate::win32::Win32Backend {
    fn on_session_lock_change(&mut self, locked: bool) {
        Self::on_session_lock_change(self, locked);
    }
    fn on_session_end(&mut self) {
        Self::on_session_end(self);
    }
    fn on_window_destroy(&mut self) {
        // Best-effort grab release ; ignore "no grab" errors.
        let _ = crate::backend::InputBackend::release_grab(self);
    }
}

#[cfg(target_os = "linux")]
impl WindowEventSink for crate::linux::LinuxBackend {
    fn on_session_lock_change(&mut self, locked: bool) {
        Self::on_session_lock_change(self, locked);
    }
    fn on_session_end(&mut self) {
        // Linux : there's no single canonical "session-end" message ;
        // we synthesize via on_session_lock_change(true) since the
        // semantics for grab-release are identical.
        Self::on_session_lock_change(self, true);
    }
    fn on_window_destroy(&mut self) {
        let _ = crate::backend::InputBackend::release_grab(self);
    }
}

#[cfg(target_os = "macos")]
impl WindowEventSink for crate::macos::MacosBackend {
    fn on_session_lock_change(&mut self, locked: bool) {
        Self::on_session_lock_change(self, locked);
    }
    fn on_session_end(&mut self) {
        Self::on_session_lock_change(self, true);
    }
    fn on_window_destroy(&mut self) {
        let _ = crate::backend::InputBackend::release_grab(self);
    }
}

impl WindowEventSink for crate::stub::StubBackend {
    fn on_session_lock_change(&mut self, _locked: bool) {}
    fn on_session_end(&mut self) {}
    fn on_window_destroy(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{GrabModes, InputBackend, InputBackendBuilder};
    use crate::stub::StubBackend;

    /// Mock pump implementing the contract trait — used to exercise the
    /// shape of the F1 ↔ F2 ABI. Once F1 lands, F1's real pump replaces
    /// this mock in integration tests.
    struct MockPump {
        destroyed: bool,
    }

    impl WindowMessagePump for MockPump {
        fn window_handle_raw(&self) -> *mut std::ffi::c_void {
            std::ptr::null_mut() // mock has no real handle
        }
        fn target_backend_kind(&self) -> BackendKind {
            BackendKind::current()
        }
        fn is_destroyed(&self) -> bool {
            self.destroyed
        }
    }

    #[test]
    fn mock_pump_window_handle_null() {
        let p = MockPump { destroyed: false };
        assert!(p.window_handle_raw().is_null());
    }

    #[test]
    fn mock_pump_destroyed_flag() {
        let p = MockPump { destroyed: true };
        assert!(p.is_destroyed());
    }

    #[test]
    fn stub_window_event_sink_no_op() {
        // Verify the WindowEventSink surface is wired up on Stub — the
        // F1 integration can call these methods on a stub backend
        // without panicking.
        let mut b = StubBackend::default();
        b.on_session_lock_change(true);
        b.on_session_end();
        b.on_window_destroy();
        // No state mutation expected on the stub.
        assert_eq!(b.current_state().tick, 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn win32_backend_implements_window_event_sink() {
        let mut b = crate::win32::Win32Backend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        // Forwarded WM_WTSSESSION_CHANGE.
        WindowEventSink::on_session_lock_change(&mut b, true);
        assert!(!b.current_state().grab_state.is_grabbed());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_backend_implements_window_event_sink() {
        let mut b = crate::linux::LinuxBackend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        WindowEventSink::on_session_lock_change(&mut b, true);
        assert!(!b.current_state().grab_state.is_grabbed());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_backend_implements_window_event_sink() {
        let mut b = crate::macos::MacosBackend::from_builder(InputBackendBuilder::new());
        b.acquire_grab(GrabModes::all()).unwrap();
        WindowEventSink::on_session_lock_change(&mut b, true);
        assert!(!b.current_state().grab_state.is_grabbed());
    }

    #[test]
    fn target_backend_kind_matches_active() {
        let p = MockPump { destroyed: false };
        assert_eq!(p.target_backend_kind(), BackendKind::current());
    }
}
