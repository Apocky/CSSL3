//! Win32 USER32 + Shcore backend.
//!
//! § SPEC : `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS (extended for
//!          host-window @ T11-D78) + `specs/10_HW.csl` § OS Windows.
//!
//! § STRATEGY (T11-D78, S7-F1)
//!   This is the canonical Apocky-host (Windows 11 + Arc A770) path. The
//!   impl uses `windows-rs 0.58` to wrap :
//!     - `RegisterClassExW` / `CreateWindowExW`           ← class + window
//!     - `SetProcessDpiAwarenessContext`                  ← per-monitor v2 DPI
//!     - `PeekMessageW` / `TranslateMessage` / `DispatchMessageW`  ← pump
//!     - `DestroyWindow`                                  ← teardown
//!     - `GetMessageW` window-proc dispatch via `WNDPROC` callback
//!
//!   The `WNDPROC` callback uses `SetWindowLongPtrW(GWLP_USERDATA, ...)`
//!   to thread the per-window state (event-queue pointer + close-state)
//!   through the callback ; the `unsafe` boundary is contained to that
//!   callback + the OS API call sites.
//!
//! § PRIME-DIRECTIVE — KILL-SWITCH ENFORCEMENT
//!   The window-proc handles `WM_CLOSE` by ALWAYS pushing a
//!   `WindowEventKind::Close` event onto the queue. The default-handler
//!   path that `DefWindowProcW` would otherwise take (immediate destroy)
//!   is INTERCEPTED — the close-state machine becomes `Pending` and only
//!   transitions to `Granted` when user-code calls `request_destroy`, OR
//!   when the auto-grant grace-window elapses (per
//!   `CloseDispositionPolicy::AutoGrantAfterGrace`). Silent-suppression is
//!   structurally impossible — there is NO code path that swallows a
//!   `WM_CLOSE` without surfacing an event.
//!
//! § UNSAFE
//!   `unsafe` is allowed at this module's boundary only ; every block
//!   carries a `// SAFETY :` comment.

#![allow(unsafe_code)]
#![allow(clippy::cast_possible_wrap)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    GetLastError, BOOL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
#[cfg(test)]
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetWindowLongPtrW, LoadCursorW, PeekMessageW, RegisterClassExW,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_OWNDC, CS_VREDRAW,
    CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG, PM_REMOVE, SHOW_WINDOW_CMD, SW_SHOW, WM_CLOSE,
    WM_DESTROY, WM_DPICHANGED, WM_KILLFOCUS, WM_NCCREATE, WM_NCDESTROY, WM_SETFOCUS, WM_SIZE,
    WNDCLASSEXW, WNDCLASS_STYLES, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
};

use crate::consent::{CloseDispositionPolicy, CloseRequestState};
use crate::error::{Result, WindowError};
use crate::event::{WindowEvent, WindowEventKind};
use crate::raw_handle::RawWindowHandle;
use crate::window::{WindowConfig, WindowFullscreen};

/// One-shot guard for `SetProcessDpiAwarenessContext`. Win32 forbids calling
/// it more than once per process — we record the call + treat subsequent
/// invocations as no-ops.
static DPI_AWARENESS_INSTALLED: AtomicU32 = AtomicU32::new(0);

/// Class-name registration counter. We append a per-process counter to the
/// class name so multiple windows in the same process don't collide on
/// `RegisterClassExW` (which fails ERROR_CLASS_ALREADY_EXISTS).
static CLASS_NAME_SEQ: AtomicU32 = AtomicU32::new(0);

/// Convert a Rust `&str` to a NUL-terminated wide string for Win32 W-APIs.
fn to_wide_z(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

/// Per-window state pointer that the WNDPROC threads through GWLP_USERDATA.
/// Stored as a `Box<RefCell<Win32WindowProcState>>` ; the leaked raw pointer
/// is parked in GWLP_USERDATA, then freed during `WM_NCDESTROY`.
struct Win32WindowProcState {
    /// Live FIFO of events the pump has not yet drained.
    pending_events: Vec<WindowEvent>,
    /// Close-state machine.
    close_state: CloseRequestState,
    /// Disposition policy (copied from the WindowConfig at spawn).
    disposition: CloseDispositionPolicy,
    /// Monotonic clock for timestamping events ; pinned at window creation.
    epoch: Instant,
    /// Whether the OS has actually destroyed the window yet.
    destroyed: bool,
}

impl Win32WindowProcState {
    fn timestamp_ms(&self) -> u64 {
        // Saturating cast : a 200-year-old window would round down ; that
        // is operationally fine for the F1 surface.
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Owning Win32 window. Drop tears down the OS window via `DestroyWindow`.
///
/// `pub(crate)` is intentional : the type is referenced by
/// `crate::window::WindowInner::Win32(...)`, which is itself `pub(crate)`.
/// We override clippy::redundant_pub_crate explicitly because the
/// alternative — making the type bare-`pub` inside a `pub(crate)` module —
/// trips clippy::unreachable_pub. The two lints disagree on this exact
/// shape ; the documentation here pins which we picked + why.
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct Win32Window {
    pub hwnd: HWND,
    pub hinstance: HINSTANCE,
    /// Backing store for the WNDPROC state. We never read this `Box`
    /// directly — the live access is through the leaked pointer parked
    /// in `GWLP_USERDATA`. Keeping the `Box` alive here ensures the
    /// allocation outlives the OS window.
    state_box: *mut RefCell<Win32WindowProcState>,
    /// Class atom name. Kept around for two reasons : (a) the
    /// `RegisterClassExW` call captured the pointer + Win32 may follow it
    /// on certain message paths ; releasing the buffer before
    /// DestroyWindow finishes would be UB. (b) future support for
    /// `UnregisterClassW` (when we add a per-process class cache to avoid
    /// per-window registration).
    #[allow(dead_code)]
    class_name: Vec<u16>,
    /// Surfaces the close-state for the user-facing `Window::close_request_state`.
    /// Synced from the WNDPROC state after every pump.
    pub close_state: CloseRequestState,
}

// SAFETY : Win32 windows are thread-affine to their creation thread ; we
// expose neither Send nor Sync to enforce single-thread access.

impl Win32Window {
    pub(crate) fn spawn(cfg: &WindowConfig) -> Result<Self> {
        // 1. DPI awareness (per-monitor v2). Installed once per process.
        if cfg.dpi_aware && DPI_AWARENESS_INSTALLED.swap(1, Ordering::SeqCst) == 0 {
            // SAFETY : called at most once per process by the swap-guard ;
            // setting per-monitor v2 is the recommended baseline on Win10+.
            let _ = unsafe {
                SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            };
            // Failure is non-fatal — continue with the per-process default.
        }

        // 2. Resolve HINSTANCE.
        // SAFETY : `GetModuleHandleW(None)` always returns the current
        // process module handle ; never null on success.
        let module =
            unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|_| WindowError::OsFailure {
                op: "GetModuleHandleW",
                code: get_last_error_code(),
            })?;
        let hinstance = HINSTANCE(module.0);

        // 3. Build a unique class name.
        let seq = CLASS_NAME_SEQ.fetch_add(1, Ordering::SeqCst);
        let class_name_str = format!("CSSLv3HostWindow_{seq}");
        let class_name = to_wide_z(&class_name_str);

        // 4. Register the window class.
        // SAFETY : pointers are kept alive by `class_name` (Vec stack-rooted)
        // for the duration of the call. The wndproc address is static.
        let cursor =
            unsafe { LoadCursorW(None, IDC_ARROW) }.map_err(|_| WindowError::OsFailure {
                op: "LoadCursorW",
                code: get_last_error_code(),
            })?;
        let wnd_class = WNDCLASSEXW {
            cbSize: u32::try_from(core::mem::size_of::<WNDCLASSEXW>()).unwrap_or(0),
            style: WNDCLASS_STYLES(CS_OWNDC.0 | CS_HREDRAW.0 | CS_VREDRAW.0),
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: Default::default(),
            hCursor: cursor,
            hbrBackground: HBRUSH(core::ptr::null_mut()),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hIconSm: Default::default(),
        };
        // SAFETY : wnd_class is local-stack ; the wndproc fn is static ;
        // class-name pointer is alive for the call.
        let atom = unsafe { RegisterClassExW(&wnd_class) };
        if atom == 0 {
            return Err(WindowError::OsFailure {
                op: "RegisterClassExW",
                code: get_last_error_code(),
            });
        }

        // 5. Compute the outer-window rect from the requested client area.
        let (window_style, ex_style) = match cfg.fullscreen {
            WindowFullscreen::Windowed => {
                let mut s = WS_OVERLAPPEDWINDOW;
                if !cfg.resizable {
                    // Strip resize / maximize affordances.
                    s.0 &= !(0x0004_0000 | 0x0001_0000); // WS_THICKFRAME | WS_MAXIMIZEBOX
                }
                (s, WS_EX_APPWINDOW)
            }
            WindowFullscreen::BorderlessOnPrimary | WindowFullscreen::ExclusiveOnPrimary => {
                // Stage-0 : both borderless + exclusive collapse to a
                // borderless popup — exclusive mode-change lands in a
                // later F-axis slice (per HANDOFF_SESSION_6 § PHASE-F deferred).
                (WS_POPUP, WS_EX_APPWINDOW)
            }
        };

        let mut adjusted = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(cfg.width).unwrap_or(i32::MAX),
            bottom: i32::try_from(cfg.height).unwrap_or(i32::MAX),
        };
        // SAFETY : pointer to local `adjusted` ; valid for the call.
        let _ = unsafe { AdjustWindowRectEx(&mut adjusted, window_style, BOOL(0), ex_style) };
        let outer_w = adjusted.right - adjusted.left;
        let outer_h = adjusted.bottom - adjusted.top;

        // 6. Allocate the proc-state box BEFORE CreateWindowExW so the
        // pointer is available in WM_NCCREATE (passed via lpCreateParams).
        let state = Box::new(RefCell::new(Win32WindowProcState {
            pending_events: Vec::new(),
            close_state: CloseRequestState::Idle,
            disposition: cfg.close_disposition,
            epoch: Instant::now(),
            destroyed: false,
        }));
        let state_ptr: *mut RefCell<Win32WindowProcState> = Box::into_raw(state);

        // 7. Title pcwstr.
        let title_w = to_wide_z(&cfg.title);

        // SAFETY : every pointer is alive for the call ; lpCreateParams
        // routes the proc-state pointer to the WNDPROC's WM_NCCREATE.
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title_w.as_ptr()),
                window_style | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                outer_w,
                outer_h,
                None,
                None,
                hinstance,
                Some(state_ptr.cast::<core::ffi::c_void>()),
            )
        };
        let Ok(hwnd) = hwnd else {
            // Roll back the leaked Box on failure.
            // SAFETY : we just created this Box and CreateWindowExW failed
            // → no callbacks fired → safe to reclaim.
            let _ = unsafe { Box::from_raw(state_ptr) };
            return Err(WindowError::OsFailure {
                op: "CreateWindowExW",
                code: get_last_error_code(),
            });
        };

        // 8. Show the window. SW_SHOW preserves whatever state CreateWindowExW
        // chose ; on Win11 we want it activated.
        // SAFETY : hwnd is a valid window we just created.
        let _ = unsafe { ShowWindow(hwnd, SHOW_WINDOW_CMD(SW_SHOW.0)) };

        Ok(Self {
            hwnd,
            hinstance,
            state_box: state_ptr,
            class_name,
            close_state: CloseRequestState::Idle,
        })
    }

    pub(crate) fn pump_events(&mut self) -> Result<Vec<WindowEvent>> {
        if self.is_destroyed() {
            return Err(WindowError::AlreadyDestroyed);
        }

        // 1. Pump OS messages → WNDPROC fires → events accumulate in the
        // per-window state queue.
        let mut msg = MSG::default();
        loop {
            // SAFETY : pointer to local `msg` ; always valid for the call.
            // PM_REMOVE drains the queue ; we exit on first FALSE return.
            let has_msg = unsafe { PeekMessageW(&mut msg, self.hwnd, 0, 0, PM_REMOVE).as_bool() };
            if !has_msg {
                break;
            }
            // SAFETY : msg is a valid Win32 MSG just filled by PeekMessageW.
            unsafe {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }

        // 2. Drain the per-window pending-events queue.
        // SAFETY : state_box is alive for the lifetime of `self` ; the
        // raw pointer points to a Box we still own.
        let state = unsafe { &*self.state_box };
        let drained = {
            let mut s = state.borrow_mut();
            // 3. Apply the auto-grant grace policy.
            if let CloseRequestState::Pending { requested_at_ms } = s.close_state {
                if let CloseDispositionPolicy::AutoGrantAfterGrace { grace } = s.disposition {
                    let now = s.timestamp_ms();
                    if now.saturating_sub(requested_at_ms) >= grace.ms {
                        // Grace exceeded → grant the close. Push a
                        // synthetic Close event so user-code that polls the
                        // queue gets a final notification before destroy.
                        s.close_state = CloseRequestState::Granted;
                    }
                }
            }
            std::mem::take(&mut s.pending_events)
        };

        // 4. Sync close-state to the user-facing field.
        self.close_state = state.borrow().close_state;

        // 5. If the state-machine reached Granted, tear down the OS window.
        if self.close_state == CloseRequestState::Granted && !self.is_destroyed() {
            self.request_destroy()?;
        }

        Ok(drained)
    }

    pub(crate) fn raw_handle(&self) -> RawWindowHandle {
        let hwnd_usize = self.hwnd.0 as usize;
        let hinst_usize = self.hinstance.0 as usize;
        RawWindowHandle::win32(hwnd_usize, hinst_usize)
    }

    pub(crate) fn request_destroy(&mut self) -> Result<()> {
        if self.is_destroyed() {
            return Err(WindowError::AlreadyDestroyed);
        }
        // SAFETY : self.state_box is a valid pointer for the lifetime of self.
        let state = unsafe { &*self.state_box };
        state.borrow_mut().close_state = CloseRequestState::Granted;
        self.close_state = CloseRequestState::Granted;

        // SAFETY : self.hwnd is a valid window handle.
        unsafe {
            DestroyWindow(self.hwnd).map_err(|_| WindowError::OsFailure {
                op: "DestroyWindow",
                code: get_last_error_code(),
            })?;
        }
        // The actual `state.destroyed = true` flip happens in WM_NCDESTROY ;
        // forcing a final pump here would re-enter PeekMessageW, which is
        // safe but overkill. We let the next pump_events() drain the
        // remaining WM_DESTROY / WM_NCDESTROY messages.
        Ok(())
    }

    pub(crate) fn dismiss_close_request(&mut self) -> Result<()> {
        // SAFETY : state_box alive for self lifetime.
        let state = unsafe { &*self.state_box };
        let mut s = state.borrow_mut();
        match s.close_state {
            CloseRequestState::Pending { .. } => {
                s.close_state = CloseRequestState::Dismissed;
            }
            CloseRequestState::Idle | CloseRequestState::Dismissed => {
                // No-op : nothing pending. This is NOT a violation because
                // there's no in-flight Close to suppress.
            }
            CloseRequestState::Granted => {
                // Cannot dismiss a granted close — the OS is already tearing
                // the window down.
                return Err(WindowError::ConsentViolation);
            }
        }
        drop(s);
        self.close_state = state.borrow().close_state;
        Ok(())
    }

    pub(crate) fn is_destroyed(&self) -> bool {
        // SAFETY : state_box alive for self lifetime.
        let state = unsafe { &*self.state_box };
        state.borrow().destroyed
    }
}

impl Drop for Win32Window {
    fn drop(&mut self) {
        if !self.is_destroyed() {
            // Best-effort destroy ; ignore errors during drop. SAFETY : hwnd
            // valid until DestroyWindow finishes.
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
        // Reclaim the proc-state box. SAFETY : we created this Box in
        // `spawn` ; it is alive iff DestroyWindow has not yet completed
        // its WM_NCDESTROY callback. After WM_NCDESTROY runs, the pointer
        // parked in GWLP_USERDATA is the same Box we own here ; we re-
        // claim it exactly once.
        if !self.state_box.is_null() {
            // SAFETY : the box is owned by Self ; consuming it here.
            let _ = unsafe { Box::from_raw(self.state_box) };
            self.state_box = core::ptr::null_mut();
        }
        // We deliberately do NOT call UnregisterClassW : per-window unique
        // class names accumulate, but Win32 cleans them on process exit.
        // Keeping the registration alive avoids a TOCTOU between pump-end
        // and class-unregister on the next CreateWindowExW.
    }
}

/// WNDPROC callback. Called by Win32 for every message dispatched through
/// `DispatchMessageW`.
///
/// SAFETY : invoked by the OS ; we re-establish the per-window state
/// pointer from `GWLP_USERDATA` and route the message into the state.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Special : on WM_NCCREATE the GWLP_USERDATA isn't yet set ; the
    // CREATESTRUCT carries our state pointer in lpCreateParams.
    if msg == WM_NCCREATE {
        // SAFETY : Win32 contract — lpCreateParams from CREATESTRUCTW.
        let create_struct =
            lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
        if !create_struct.is_null() {
            let state_ptr = unsafe { (*create_struct).lpCreateParams };
            let state_isize = state_ptr as isize;
            // SAFETY : SetWindowLongPtrW with GWLP_USERDATA on a window
            // we just received in WM_NCCREATE ; OS guarantees validity.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_isize);
            }
        }
        // SAFETY : default handler always-safe.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    // Recover the per-window state pointer.
    // SAFETY : GetWindowLongPtrW with GWLP_USERDATA on a valid HWND.
    let state_isize = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    let state_ptr = state_isize as *mut RefCell<Win32WindowProcState>;

    if state_ptr.is_null() {
        // No state — we're either pre-NCCREATE or post-NCDESTROY ; default-handle.
        // SAFETY : default handler always-safe.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    // SAFETY : state_ptr was set by us in WM_NCCREATE ; alive until WM_NCDESTROY.
    let state_cell: &RefCell<Win32WindowProcState> = unsafe { &*state_ptr };

    match msg {
        WM_CLOSE => {
            // PRIME-DIRECTIVE : surface the Close event ; do NOT call
            // DefWindowProcW (which would auto-destroy). The state
            // machine + grace policy decide what happens next.
            let mut s = state_cell.borrow_mut();
            if matches!(s.close_state, CloseRequestState::Idle) {
                let now = s.timestamp_ms();
                s.close_state = CloseRequestState::Pending {
                    requested_at_ms: now,
                };
                s.pending_events.push(WindowEvent {
                    timestamp_ms: now,
                    kind: WindowEventKind::Close,
                });
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // OS-level destroy in progress ; drain any pending events into
            // a final synthetic Close if not already.
            // SAFETY : default handler always-safe.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_NCDESTROY => {
            // Final teardown ; flag the state as destroyed. The state Box
            // itself is reclaimed by `Win32Window::drop` ; we just zero
            // GWLP_USERDATA so subsequent messages take the early-out.
            state_cell.borrow_mut().destroyed = true;
            // SAFETY : SetWindowLongPtrW with GWLP_USERDATA = 0 is valid.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            // SAFETY : default handler always-safe.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SIZE => {
            // SAFETY : GetClientRect on a valid hwnd ; pointer to local rect.
            let mut rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut rect) };
            let width = u32::try_from(rect.right - rect.left).unwrap_or(0);
            let height = u32::try_from(rect.bottom - rect.top).unwrap_or(0);
            let mut s = state_cell.borrow_mut();
            let ts = s.timestamp_ms();
            s.pending_events.push(WindowEvent {
                timestamp_ms: ts,
                kind: WindowEventKind::Resize { width, height },
            });
            LRESULT(0)
        }
        WM_SETFOCUS => {
            let mut s = state_cell.borrow_mut();
            let ts = s.timestamp_ms();
            s.pending_events.push(WindowEvent {
                timestamp_ms: ts,
                kind: WindowEventKind::FocusGain,
            });
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            let mut s = state_cell.borrow_mut();
            let ts = s.timestamp_ms();
            s.pending_events.push(WindowEvent {
                timestamp_ms: ts,
                kind: WindowEventKind::FocusLoss,
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // wparam.LOWORD = new DPI X, HIWORD = new DPI Y. We map to the
            // X scale (Win11 keeps X+Y in lockstep on per-monitor v2). The
            // mask-then-narrow + integer→f32 lossless conversion sequence
            // suppresses the cast_precision_loss lint (a u16 fits in f32's
            // mantissa exactly).
            let dpi_x_u16: u16 = u16::try_from(wparam.0 & 0xFFFF).unwrap_or(96);
            let scale = f32::from(dpi_x_u16) / 96.0_f32;
            let mut s = state_cell.borrow_mut();
            let ts = s.timestamp_ms();
            s.pending_events.push(WindowEvent {
                timestamp_ms: ts,
                kind: WindowEventKind::DpiChanged { scale },
            });
            LRESULT(0)
        }
        _ => {
            // SAFETY : default handler always-safe.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

/// Synthesize an external close-request from the API side (e.g. tests +
/// future graceful-shutdown plumbing). The window-proc treats this exactly
/// like a user-initiated WM_CLOSE.
#[cfg(test)]
#[allow(dead_code, clippy::redundant_pub_crate)]
pub(crate) fn synthesize_close(window: &Win32Window) -> Result<()> {
    // SAFETY : PostMessageW on a valid HWND we own ; the OS routes the
    // message back through DispatchMessageW on the next pump.
    unsafe {
        PostMessageW(window.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)).map_err(|_| {
            WindowError::OsFailure {
                op: "PostMessageW",
                code: get_last_error_code(),
            }
        })?;
    }
    Ok(())
}

fn get_last_error_code() -> u32 {
    // SAFETY : GetLastError() is always-safe.
    unsafe { GetLastError().0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_wide_z_appends_nul() {
        let v = to_wide_z("ab");
        assert_eq!(v, vec![b'a' as u16, b'b' as u16, 0]);
    }

    #[test]
    fn to_wide_z_empty_is_just_nul() {
        let v = to_wide_z("");
        assert_eq!(v, vec![0]);
    }

    #[test]
    fn class_name_seq_is_strictly_increasing() {
        let a = CLASS_NAME_SEQ.fetch_add(1, Ordering::SeqCst);
        let b = CLASS_NAME_SEQ.fetch_add(1, Ordering::SeqCst);
        assert!(b > a);
    }
}
