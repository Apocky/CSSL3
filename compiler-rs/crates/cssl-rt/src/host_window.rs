//! § cssl-rt host_window — `__cssl_window_*` ABI-stable extern surface (Wave-D3).
//! ════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § window`.
//! Plan reference     : `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D ↳ D3`.
//!
//! § ROLE
//!   Exposes the six `__cssl_window_*` `extern "C"` symbols that
//!   CSSLv3-emitted code links against to drive an OS-window. Each
//!   symbol is delivered as a stable ABI per the `specs/24_HOST_FFI.csl
//!   § ABI-STABLE-SYMBOLS` lock — renaming any of them is a major-
//!   version-bump event (matches the `__cssl_alloc / __cssl_fs_* /
//!   __cssl_net_*` FFI lock-step discipline in `crate::ffi`).
//!
//! § ABI-STABLE SYMBOLS  (extern "C" ; locked-forever from Wave-D3 forward)
//!   ```text
//!   __cssl_window_spawn(title_ptr, title_len, w, h, flags) -> u64
//!       returns window-handle ; 0 = error
//!   __cssl_window_pump(handle, events_out, max_events) -> i64
//!       returns event-count or negative-errno
//!   __cssl_window_request_close(handle) -> i32
//!       0 = ok ; -1 = bad-handle
//!   __cssl_window_destroy(handle) -> i32
//!       0 = ok ; -1 = bad-handle
//!   __cssl_window_raw_handle(handle, out, max_len) -> i32
//!       returns bytes-written or -1 on bad-handle / -2 on buf-too-small
//!   __cssl_window_get_dims(handle, w_out, h_out) -> i32
//!       0 = ok ; -1 = bad-handle / null-out-ptr
//!   ```
//!
//! § INVARIANTS  (carried-forward landmine ; matches `crate::ffi` discipline)
//!   ‼ Renaming any of these symbols is a major-version-bump event ;
//!     CSSLv3-emitted code references them by exact name + cssl-cgen-cpu-
//!     cranelift::cgen_window pins the symbol-strings on its side. Any
//!     drift between the two sides = link-time symbol mismatch ⇒ UB.
//!   ‼ Argument types + ordering are also locked. Use additional symbols
//!     (e.g. `__cssl_window_spawn_v2`) for new behaviors, never mutate
//!     existing ones.
//!   ‼ Each symbol delegates to a Rust-side `_impl` fn so unit tests can
//!     exercise behavior without going through the FFI boundary.
//!
//! § WINDOW-HANDLE DESIGN  (Sawyer-mindset)
//!   Source-level CSSLv3 sees an opaque `u64` window-handle. The runtime
//!   implements this as a **registry index** rather than exposing an OS
//!   pointer through the FFI :
//!
//!   ```text
//!     SLOT_REGISTRY : OnceLock<Mutex<HashMap<u64, RegistryEntry>>>
//!     NEXT_HANDLE   : AtomicU64  (monotonic counter ; never re-issues 0)
//!   ```
//!
//!   Why a registry rather than `Box<Window>` raw-pointers :
//!     - safety : invalid u64 handles return error rather than UB. Source
//!       code can pass an arbitrary u64 across the FFI without dereferencing
//!       a stale pointer.
//!     - opacity : the OS-specific `cssl_host_window::Window` value never
//!       crosses the FFI ; we lookup-by-id on the runtime side.
//!     - de-dup : double-`destroy` is detectable + idempotent (entry
//!       removed on first destroy ; second destroy returns -1).
//!     - thread-safe : the Mutex serializes registry mutations ; the
//!       per-window operations (`pump_events` etc.) take a brief lock to
//!       look up + then release before issuing OS calls.
//!
//!   Handle 0 is reserved as the "invalid" sentinel — `spawn` never returns
//!   0 ; `request_close(0)` / `destroy(0)` / etc. all return -1.
//!
//! § EVENT-PACKING ABI  (32-byte fixed-size record)
//!   `__cssl_window_pump` writes events into the caller's buffer as an
//!   array of 32-byte fixed-size records. This avoids per-call scratch
//!   allocation on the runtime side + lets source-level CSSLv3 declare
//!   a stack-buffer of [u8; N*32] without pulling in alloc.
//!
//!   Record layout (LE byte-order on every supported target) :
//!   ```text
//!     offset  size  field
//!        0    u16   kind         (see EVENT_KIND_*)
//!        2    u16   reserved     (always 0 ; future fields)
//!        4    u32   payload_a    (kind-specific)
//!        8    u32   payload_b    (kind-specific)
//!       12    u32   payload_c    (kind-specific)
//!       16    u32   payload_d    (kind-specific)
//!       20    u64   timestamp_ms (millis since window construction)
//!       28    u32   reserved2    (always 0)
//!   ```
//!
//!   The `EVENT_RECORD_SIZE = 32` constant + `EVENT_KIND_*` discriminants
//!   are **ABI-locked** ; cssl-cgen-cpu-cranelift::cgen_window pins them
//!   on its side via the same constants.
//!
//!   Per-kind payload layout :
//!   ```text
//!     EVENT_KIND_NONE      : (record never emitted ; empty-pump sentinel)
//!     EVENT_KIND_CLOSE     : payload_a..d = 0
//!     EVENT_KIND_RESIZE    : payload_a = new_w, payload_b = new_h, c=d=0
//!     EVENT_KIND_FOCUS_GAIN: payload_a..d = 0
//!     EVENT_KIND_FOCUS_LOSS: payload_a..d = 0
//!     EVENT_KIND_KEY_DOWN  : payload_a = key, payload_b = mods,
//!                            payload_c = repeat-flag, d = 0
//!     EVENT_KIND_KEY_UP    : payload_a = key, payload_b = mods, c=d=0
//!     EVENT_KIND_MOUSE_MOVE: payload_a = x_i32 (cast), b = y_i32, c=d=0
//!     EVENT_KIND_MOUSE_DOWN: a=button, b=x_i32, c=y_i32, d=mods
//!     EVENT_KIND_MOUSE_UP  : a=button, b=x_i32, c=y_i32, d=mods
//!     EVENT_KIND_SCROLL    : a=delta_x_bits, b=delta_y_bits, c=x, d=y_packed
//!     EVENT_KIND_DPI_CHANGE: payload_a = scale_bits (f32 to_bits), b..d=0
//!   ```
//!
//! § INTEGRATION_NOTE  (per Wave-D3 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-rt/Cargo.toml` is
//!   intentionally NOT modified per task constraint "Touch ONLY :
//!   cssl-rt/src/host_window.rs (NEW), cssl-cgen-cpu-cranelift/src/
//!   cgen_window.rs (NEW)". `crate::lib.rs` is also NOT modified.
//!
//!   ⌈ Main-thread integration follow-up ⌋ :
//!     1. Add `cssl-host-window = { path = "../cssl-host-window" }` to
//!        `compiler-rs/crates/cssl-rt/Cargo.toml [dependencies]` (and the
//!        matching workspace.dependencies entry in `compiler-rs/Cargo.toml`
//!        if absent).
//!     2. Add `pub mod host_window;` to `crate::lib.rs`.
//!     3. Re-export the public surface : `pub use host_window::{
//!        cssl_window_spawn_impl, cssl_window_pump_impl, ... ,
//!        EVENT_RECORD_SIZE, EVENT_KIND_* };`.
//!     4. Replace the `RegistryEntry::Stub` variant inline (see §
//!        STUB-VS-REAL below) with a real
//!        `cssl_host_window::Window`-holding variant, then thread the
//!        existing `_impl` bodies through it.
//!
//!   Until the integration follow-up lands, the `__cssl_window_*` extern
//!   symbols use the `RegistryEntry::Stub` variant which simulates a
//!   live OS window (records (w, h) + close-state) so source-level code
//!   + cgen + tests + downstream waves can compile + exercise the surface
//!   end-to-end. The handle-allocation + event-packing + close-state
//!   machinery is identical between stub + real ; only the actual
//!   `pump_events` / `raw_handle` / `get_dims` data-fetch differs.
//!
//! § STUB-VS-REAL TRANSITION
//!   The `delegate_to_host_window` constant — currently `false` — is the
//!   single switch that future work flips. When the Cargo.toml dep +
//!   `pub mod host_window` line both land, the stub branches in this file
//!   are replaced by `cssl_host_window::Window` calls (one branch per
//!   `_impl` fn). The TESTS in this module already exercise the stub
//!   path ; they continue to pass after the swap because the public ABI
//!   shape (handle-as-u64 + 32-byte records + return-codes) is locked
//!   PRE-stub via the const + assertion lattice in this file.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone /
//!    anything / anybody."
//!   This module is a windowing surface — visual + interactive presence
//!   for source-level CSSLv3 programs. It does not surveil ; it does not
//!   intercept user input outside the window's own client area ; it does
//!   not track keystrokes outside what the user-code observes via pump.
//!   Every event is emitted in response to direct user-action on the
//!   window. The close-button is structurally always observable via
//!   `EVENT_KIND_CLOSE` (consent-arch from `cssl_host_window::consent`
//!   carries through to the runtime — silent default-suppress is FORBIDDEN
//!   per `PRIME_DIRECTIVE.md § 1 § entrapment`).
//!
//! § SAWYER-EFFICIENCY
//!   - `NEXT_HANDLE` : single `AtomicU64::fetch_add(1, Relaxed)` per spawn ;
//!     no contention except on the registry-lock acquisition.
//!   - `SLOT_REGISTRY` : `HashMap<u64, RegistryEntry>` rather than `Vec` :
//!     spawn / destroy / lookup are all O(1) ; spawn-rate is bounded
//!     (≤ ~100/s typical) so HashMap overhead is dwarfed by OS-window
//!     creation cost.
//!   - **No per-pump scratch allocation** : `pump_events_impl` writes
//!     directly into the caller's buffer via `pack_event` ; events that
//!     overflow `max_events` are dropped + a count returned.
//!   - **Event-record packing** : 32 bytes fixed-size ; cache-line-friendly
//!     (one record fits in 1/2 of a typical 64-byte line, two per line).
//!   - **Bit-pack scroll-delta** : f32 `to_bits` so the record stays
//!     fixed-width (no enum-tag overhead per event).

#![allow(unsafe_code)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::missing_errors_doc)]
// § REGISTRY-LOCK NOTE — registry-lock guards intentionally hold across
// post-mutation work (counter increments, lookup-then-write) for clarity ;
// matches the cssl-log + cssl-metrics + host_gpu sibling pattern. The
// pre-existing tightening in `cssl_window_pump_impl` shows the guard CAN
// be tightened where it matters (lookup-only path).
#![allow(clippy::significant_drop_tightening)]

use core::cell::RefCell;
use core::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// § T11-W19-β-RT-DELEG-WINDOW : real-Win32 delegation.
//
// The historical Stub variant remains as a fallback (used in unit tests
// + on platforms where cssl-host-window returns LoaderMissing). The new
// Real variant holds a `cssl_host_window::Window` directly + delegates
// pump / raw-handle / dims to the backend's real Win32 calls.
//
// § Win32 thread-affinity : Win32 windows are thread-affine to their
// creation thread (the message-pump can only be drained by that thread).
// We therefore park the Real-variant entries in a `thread_local!` registry
// so source-level CSSLv3 code that happens to spawn + pump on different
// threads gets a deterministic bad-handle error rather than the silent
// race the upstream Win32 API would surface. The cross-thread Stub
// registry continues to satisfy the existing unit tests.
use cssl_host_window::{
    spawn_window, RawWindowHandleKind, Window as HostWindow, WindowConfig as HostWindowConfig,
    WindowEventKind,
};

// ───────────────────────────────────────────────────────────────────────
// § ABI-locked constants — pinned by cssl-cgen-cpu-cranelift::cgen_window
//
// ‼ Drift between the two sides = silent ABI mismatch. The constants
//   are intentionally `pub` so cgen_window can reference them via a
//   doc-link without re-declaring.
// ───────────────────────────────────────────────────────────────────────

/// Size of one packed event record in bytes. ABI-locked from Wave-D3
/// forward — cssl-cgen-cpu-cranelift::cgen_window pins this value too.
pub const EVENT_RECORD_SIZE: usize = 32;

/// Maximum length of the raw-handle blob written by
/// `__cssl_window_raw_handle` on Win32 : `(HWND, HINSTANCE)` = 2 × usize.
/// On a 64-bit host this is 16 bytes ; on 32-bit hosts it is 8.
pub const RAW_HANDLE_MAX_BYTES_WIN32: usize = 2 * core::mem::size_of::<usize>();

/// Sentinel : an invalid/uninitialized handle is `0`. Returned by
/// `__cssl_window_spawn` on failure ; rejected by every other entrypoint.
pub const INVALID_WINDOW_HANDLE: u64 = 0;

// ── window-spawn flag bitset (Sawyer-mindset : single u32 ; no enum-tag)

/// Allow user to resize the window via window-edges.
pub const SPAWN_FLAG_RESIZABLE: u32 = 1 << 0;
/// Spawn fullscreen on the primary monitor.
pub const SPAWN_FLAG_FULLSCREEN: u32 = 1 << 1;
/// Opt into per-monitor v2 DPI awareness on Win32.
pub const SPAWN_FLAG_DPI_AWARE: u32 = 1 << 2;
/// Borderless window (no title-bar / window-edges).
pub const SPAWN_FLAG_BORDERLESS: u32 = 1 << 3;

/// Mask of the recognized spawn-flag bits ; any other bit is rejected.
pub const SPAWN_FLAG_MASK: u32 = SPAWN_FLAG_RESIZABLE
    | SPAWN_FLAG_FULLSCREEN
    | SPAWN_FLAG_DPI_AWARE
    | SPAWN_FLAG_BORDERLESS;

// ── event-kind discriminants (ABI-locked u16 ; cgen_window mirrors)

/// Sentinel — not emitted ; reserved for "no event".
pub const EVENT_KIND_NONE: u16 = 0;
/// User requested window close (X button / Alt-F4 / system-menu).
pub const EVENT_KIND_CLOSE: u16 = 1;
/// Window resized — payload_a = new_w, payload_b = new_h.
pub const EVENT_KIND_RESIZE: u16 = 2;
/// Window gained input focus.
pub const EVENT_KIND_FOCUS_GAIN: u16 = 3;
/// Window lost input focus.
pub const EVENT_KIND_FOCUS_LOSS: u16 = 4;
/// Keyboard key pressed.
pub const EVENT_KIND_KEY_DOWN: u16 = 5;
/// Keyboard key released.
pub const EVENT_KIND_KEY_UP: u16 = 6;
/// Mouse cursor moved within client area.
pub const EVENT_KIND_MOUSE_MOVE: u16 = 7;
/// Mouse button pressed.
pub const EVENT_KIND_MOUSE_DOWN: u16 = 8;
/// Mouse button released.
pub const EVENT_KIND_MOUSE_UP: u16 = 9;
/// Mouse wheel scrolled.
pub const EVENT_KIND_SCROLL: u16 = 10;
/// DPI change (window dragged to different-DPI monitor).
pub const EVENT_KIND_DPI_CHANGE: u16 = 11;

// ── pump return-code domain (negative = errno ; non-negative = event-count)

/// Pump error : the supplied window handle is not registered.
pub const PUMP_ERR_BAD_HANDLE: i64 = -1;
/// Pump error : `events_out` pointer is null + `max_events > 0`.
pub const PUMP_ERR_NULL_BUF: i64 = -2;
/// Pump error : window has been destroyed (post-`destroy`).
pub const PUMP_ERR_DESTROYED: i64 = -3;

// ───────────────────────────────────────────────────────────────────────
// § registry-state holder
//
// `OnceLock<Mutex<HashMap>>` : lazy-init on first access ; per-spawn
// branches arbitrate via the AtomicU64 monotonic counter ; the Mutex
// serializes registry mutations.
// ───────────────────────────────────────────────────────────────────────

/// Per-window state held in the registry.
///
/// § STUB-VS-REAL : the variants represent the two transition states
/// of the Wave-D3 → main-thread integration follow-up. Until the
/// `cssl-host-window` Cargo.toml dep + `pub mod host_window;` line both
/// land, only `RegistryEntry::Stub` is constructible. After the
/// integration : the same struct holds the `cssl_host_window::Window`
/// directly + the `_impl` bodies dispatch on it.
#[derive(Debug)]
enum RegistryEntry {
    /// Stub variant — simulates a live OS window for ABI-shape testing.
    /// All `_impl` fns route through this variant pre-integration. The
    /// `#[allow(dead_code)]` on `flags` + `title` reflects that pre-
    /// integration the runtime only inspects (width, height, close_state)
    /// for ABI-shape testing ; post-integration the fields are consumed
    /// by `cssl_host_window::WindowConfig::{title, fullscreen, ...}`.
    Stub {
        /// Initial width in physical pixels.
        width: u32,
        /// Initial height in physical pixels.
        height: u32,
        /// Spawn-flag bitset (resizable / fullscreen / dpi-aware / …).
        #[allow(dead_code)]
        flags: u32,
        /// User-supplied title (validated UTF-8 ; non-empty).
        #[allow(dead_code)]
        title: String,
        /// Has `request_close` been called (semantic : "close request
        /// sent ; pending user-grant via `destroy`").
        close_requested: bool,
        /// Has `destroy` been called (post-`destroy` ops return -1).
        destroyed: bool,
        /// Window-construction timestamp ; events stamp millis-since.
        created_at: std::time::Instant,
    },
}

/// Real-backend per-window state. Held in the thread-local registry
/// because `cssl_host_window::Window` (Win32) is `!Send` + `!Sync`.
struct RealEntry {
    /// Owned `cssl_host_window::Window` ; drop = `DestroyWindow`.
    win: HostWindow,
    /// Initial width in physical pixels (cached for `get_dims` until
    /// real-backend `pump_events` updates via `Resize` events).
    width: u32,
    /// Initial height in physical pixels (cached, see `width`).
    height: u32,
    /// Window-construction timestamp ; events stamp millis-since.
    created_at: std::time::Instant,
    /// Marks `request_close` for the next `pump` call (the real backend's
    /// `pump_events` already surfaces user-driven `Close` events ; this
    /// flag handles the explicit `__cssl_window_request_close` path).
    explicit_close_requested: bool,
}

impl core::fmt::Debug for RealEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RealEntry")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("explicit_close_requested", &self.explicit_close_requested)
            .finish()
    }
}

/// Process-wide stub-window registry. Lazy-initialized on first `_impl`
/// call. Used for non-Win32 platforms + for unit tests (cross-thread
/// safe). When the real-Win32 path returns `Ok` the entry lands in the
/// thread-local `REAL_REGISTRY` instead.
fn registry() -> &'static Mutex<HashMap<u64, RegistryEntry>> {
    static REG: OnceLock<Mutex<HashMap<u64, RegistryEntry>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    /// Per-thread real-backend window registry. Win32 windows are
    /// thread-affine to their creation thread ; a global Mutex<HashMap>
    /// would force `Send` on the Window type which the upstream crate
    /// deliberately does NOT impl. Instead each construction thread keeps
    /// its own `RefCell<HashMap>` ; cross-thread handle lookups land
    /// `PUMP_ERR_BAD_HANDLE` rather than the upstream Win32 silent-race.
    static REAL_REGISTRY: RefCell<HashMap<u64, RealEntry>> =
        RefCell::new(HashMap::new());
}

/// Translate cssl-rt spawn-flags to a `cssl_host_window::WindowConfig`.
fn build_host_config(title: String, width: u32, height: u32, flags: u32) -> HostWindowConfig {
    let mut cfg = HostWindowConfig::new(title, width, height);
    cfg.resizable = (flags & SPAWN_FLAG_RESIZABLE) != 0;
    cfg.dpi_aware = (flags & SPAWN_FLAG_DPI_AWARE) != 0;
    if (flags & SPAWN_FLAG_FULLSCREEN) != 0 {
        cfg.fullscreen = cssl_host_window::WindowFullscreen::ExclusiveOnPrimary;
    } else if (flags & SPAWN_FLAG_BORDERLESS) != 0 {
        cfg.fullscreen = cssl_host_window::WindowFullscreen::BorderlessOnPrimary;
    }
    cfg
}

/// Monotonic counter for handle issuance. Starts at 1 (handle 0 reserved).
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Atomic counter : total successful `spawn` calls (audit visibility).
static SPAWN_COUNT: AtomicU64 = AtomicU64::new(0);
/// Atomic counter : total successful `destroy` calls.
static DESTROY_COUNT: AtomicU64 = AtomicU64::new(0);
/// Atomic counter : total `pump` calls (regardless of return-code).
static PUMP_COUNT: AtomicU64 = AtomicU64::new(0);

// ── public counter accessors (audit-visibility ; matches `crate::io` discipline)

/// Total successful `__cssl_window_spawn` calls since process start.
#[must_use]
pub fn spawn_count() -> u64 {
    SPAWN_COUNT.load(Ordering::Relaxed)
}

/// Total successful `__cssl_window_destroy` calls since process start.
#[must_use]
pub fn destroy_count() -> u64 {
    DESTROY_COUNT.load(Ordering::Relaxed)
}

/// Total `__cssl_window_pump` calls since process start (success + error).
#[must_use]
pub fn pump_count() -> u64 {
    PUMP_COUNT.load(Ordering::Relaxed)
}

/// Reset all counters + clear the registry. Test-only.
#[cfg(test)]
pub(crate) fn reset_for_tests() {
    SPAWN_COUNT.store(0, Ordering::Relaxed);
    DESTROY_COUNT.store(0, Ordering::Relaxed);
    PUMP_COUNT.store(0, Ordering::Relaxed);
    NEXT_HANDLE.store(1, Ordering::Relaxed);
    let mut g = registry().lock().expect("registry mutex");
    g.clear();
}

// ───────────────────────────────────────────────────────────────────────
// § _impl entry-points — Rust-side fns that the FFI symbols delegate to
// ───────────────────────────────────────────────────────────────────────

/// Validate the spawn-flag bitset.
///
/// Returns `true` when only recognized bits are set + no contradictory
/// combinations (FULLSCREEN+BORDERLESS together is contradictory ; the
/// host-window crate would reject the combination via WindowError).
#[must_use]
pub fn validate_spawn_flags(flags: u32) -> bool {
    if (flags & !SPAWN_FLAG_MASK) != 0 {
        return false;
    }
    // FULLSCREEN + BORDERLESS = contradictory (fullscreen IS already
    // borderless ; the host-window crate's Win32 backend rejects this
    // combination). Catch on this side before delegation.
    if (flags & SPAWN_FLAG_FULLSCREEN) != 0 && (flags & SPAWN_FLAG_BORDERLESS) != 0 {
        return false;
    }
    true
}

/// Read a UTF-8 title slice from `(ptr, len)` returning the validated
/// `String` or `None` on bad-UTF-8 / null-with-nonzero-len / empty-title.
///
/// # Safety
/// `ptr` must be valid for `len` bytes if `len > 0` ; reads only the
/// region `[ptr, ptr+len)`.
unsafe fn read_title(title_ptr: *const u8, title_len: usize) -> Option<String> {
    if title_len == 0 {
        return None;
    }
    if title_ptr.is_null() {
        return None;
    }
    // SAFETY : caller's contract per the FFI doc-block guarantees
    // `title_ptr` is valid for `title_len` bytes when `title_len > 0`.
    let bytes = unsafe { core::slice::from_raw_parts(title_ptr, title_len) };
    core::str::from_utf8(bytes).ok().map(str::to_owned)
}

/// Implementation : spawn a new window.
///
/// Returns the issued handle on success or [`INVALID_WINDOW_HANDLE`] (0)
/// on validation failure.
///
/// # Safety
/// Caller must ensure `title_ptr` is valid for `title_len` bytes when
/// `title_len > 0`.
pub unsafe fn cssl_window_spawn_impl(
    title_ptr: *const u8,
    title_len: usize,
    width: u32,
    height: u32,
    flags: u32,
) -> u64 {
    // § T11-W19-β-LIVE-TRACE : write directly to stderr handle to surface
    // entry path during bring-up. Avoid eprintln! buffering issues by
    // writing the bytes directly through a flush-on-drop handle.
    {
        use std::io::Write as _;
        let mut e = std::io::stderr().lock();
        let _ = writeln!(
            e,
            "[trace] cssl_window_spawn_impl called : tp={:p} tl={} w={} h={} flags={}",
            title_ptr, title_len, width, height, flags
        );
        let _ = e.flush();
    }
    // Validation gate-1 : dimensions.
    if width == 0 || height == 0 {
        eprintln!("[trace] rejected : zero dim");
        return INVALID_WINDOW_HANDLE;
    }
    // Validation gate-2 : flag bitset.
    if !validate_spawn_flags(flags) {
        return INVALID_WINDOW_HANDLE;
    }
    // Validation gate-3 : title.
    //
    // § T11-W19-β-LIVE-PUMP  (2026-05-03)
    //   Accept (null-ptr OR zero-len) by substituting a default title.
    //   This unblocks CSSL-source-level callers that cannot easily extract
    //   the (ptr, len) pair from a `&str` via the stage-0 stdlib surface.
    //   The 2-segment `window::spawn(t_ptr, t_len, w, h, flags)` recognizer
    //   in cssl_mir::body_lower expects 5 i64-shaped args ; the .cssl
    //   wrapper-fn surface in stdlib/window.cssl can simply pass `(0i64,
    //   0i64, w, h, flags)` and rely on this fallback.
    //
    //   ‼ The default title is a static ASCII string ("CSSL-Window") ;
    //   no user-supplied bytes are read in this branch, so the
    //   PRIME-DIRECTIVE no-surveillance posture is preserved.
    // SAFETY : caller's contract on (title_ptr, title_len).
    let title = match unsafe { read_title(title_ptr, title_len) } {
        Some(t) if !t.is_empty() => t,
        _ => "CSSL-Window".to_string(),
    };

    // Issue handle. Counter starts at 1 ; never re-issues 0. The Relaxed
    // ordering is sufficient because the registry-mutex also synchronizes.
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    // Defensive : if the counter ever wraps to 0 (4 billion+ spawns) skip
    // the slot. In practice this is never reached at stage-0 + the wrap is
    // benign : we'd just consume one slot.
    if handle == INVALID_WINDOW_HANDLE {
        return INVALID_WINDOW_HANDLE;
    }

    // § T11-W19-β-RT-DELEG-WINDOW : try real-Win32 first. Fall back to
    // the Stub variant on LoaderMissing (non-Windows + missing-backend)
    // so the existing unit tests + non-host platforms keep working. The
    // production path on Apocky-host (Windows 11 + Arc A770) goes Real.
    let cfg = build_host_config(title.clone(), width, height, flags);
    eprintln!("[trace] calling spawn_window cfg=({}, {}x{}, flags={:?})", title, width, height, flags);
    match spawn_window(&cfg) {
        Ok(real_win) => {
            eprintln!("[trace] spawn_window OK · handle={}", handle);
            REAL_REGISTRY.with(|reg| {
                reg.borrow_mut().insert(
                    handle,
                    RealEntry {
                        win: real_win,
                        width,
                        height,
                        created_at: std::time::Instant::now(),
                        explicit_close_requested: false,
                    },
                );
            });
            SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
            return handle;
        }
        Err(e) => {
            eprintln!("[trace] spawn_window FAILED · err={:?}", e);
            // Fall through to stub on real-backend failure (LoaderMissing
            // on non-Windows ; OsFailure on hosts where Win32 rejected the
            // spawn). Stub keeps the FFI-shape testable.
        }
    }

    let entry = RegistryEntry::Stub {
        width,
        height,
        flags,
        title,
        close_requested: false,
        destroyed: false,
        created_at: std::time::Instant::now(),
    };

    {
        let mut g = registry().lock().expect("registry mutex");
        g.insert(handle, entry);
    }
    SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
    handle
}

/// Implementation : pump pending events into the caller's buffer.
///
/// Returns the number of events written (0..=max_events) on success or
/// a negative error code (`PUMP_ERR_*`) on failure.
///
/// # Safety
/// Caller must ensure `events_out` is valid for `max_events *
/// EVENT_RECORD_SIZE` bytes when `max_events > 0` (checked-tolerant on
/// nullptr + zero-length).
pub unsafe fn cssl_window_pump_impl(
    handle: u64,
    events_out: *mut u8,
    max_events: usize,
) -> i64 {
    PUMP_COUNT.fetch_add(1, Ordering::Relaxed);

    // Buffer-validity gate.
    if max_events > 0 && events_out.is_null() {
        return PUMP_ERR_NULL_BUF;
    }

    // § T11-W19-β-RT-DELEG-WINDOW : real-backend first.
    let real_result = REAL_REGISTRY.with(|reg| -> Option<i64> {
        let mut g = reg.borrow_mut();
        let entry = g.get_mut(&handle)?;
        // Drain OS messages.
        let pumped = entry.win.pump_events();
        let events = match pumped {
            Ok(v) => v,
            Err(_) => return Some(PUMP_ERR_BAD_HANDLE),
        };
        let mut written: usize = 0;
        // First, surface explicit_close_requested as a CLOSE event.
        if entry.explicit_close_requested && written < max_events && max_events > 0 {
            let ts = entry.created_at.elapsed().as_millis() as u64;
            // SAFETY : caller's contract on (events_out, max_events) +
            // we ensured `written < max_events > 0` so the offset is
            // within the buffer.
            unsafe {
                let slot = events_out.add(written * EVENT_RECORD_SIZE);
                write_close_event(slot, ts);
            }
            entry.explicit_close_requested = false;
            written += 1;
        }
        // Translate pumped events into the 32-byte ABI records.
        for ev in events {
            if written >= max_events {
                break;
            }
            // SAFETY : (events_out, max_events) caller-contract + bounded
            // `written < max_events` makes each slot offset valid.
            unsafe {
                let slot = events_out.add(written * EVENT_RECORD_SIZE);
                pack_window_event(slot, &ev);
            }
            // Cache resize dims for get_dims fallback path.
            if let WindowEventKind::Resize { width, height } = ev.kind {
                entry.width = width;
                entry.height = height;
            }
            written += 1;
        }
        Some(written as i64)
    });
    if let Some(rc) = real_result {
        return rc;
    }

    // STUB-VS-REAL § : real-backend miss falls through to the stub. The
    // stub returns either 0 events (the typical idle path) or a
    // synthesized Close event after `request_close` has fired.
    //
    // The match below extracts the data needed (timestamp + close_requested
    // flag) under the registry lock, then RELEASES the lock before issuing
    // the buffer write. This keeps the critical section tight per
    // clippy::significant_drop_tightening.
    let (synth_close, ts_ms) = {
        let mut g = registry().lock().expect("registry mutex");
        let Some(entry) = g.get_mut(&handle) else {
            return PUMP_ERR_BAD_HANDLE;
        };
        match entry {
            RegistryEntry::Stub {
                destroyed,
                close_requested,
                created_at,
                ..
            } => {
                if *destroyed {
                    return PUMP_ERR_DESTROYED;
                }
                if *close_requested && max_events > 0 {
                    let ts = created_at.elapsed().as_millis() as u64;
                    *close_requested = false;
                    (true, ts)
                } else {
                    (false, 0)
                }
            }
        }
    };
    if synth_close {
        // SAFETY : events_out is valid for at least 1 event-record by the
        // caller's contract + the max_events > 0 + non-null buffer gate.
        unsafe { write_close_event(events_out, ts_ms) };
        return 1;
    }
    0
}

/// Implementation : send a close-request to the OS window.
///
/// Returns `0` on success ; `-1` on bad-handle / already-destroyed.
pub fn cssl_window_request_close_impl(handle: u64) -> i32 {
    // § T11-W19-β-RT-DELEG-WINDOW : real-backend first.
    let real_hit = REAL_REGISTRY.with(|reg| {
        let mut g = reg.borrow_mut();
        if let Some(entry) = g.get_mut(&handle) {
            entry.explicit_close_requested = true;
            true
        } else {
            false
        }
    });
    if real_hit {
        return 0;
    }
    let mut g = registry().lock().expect("registry mutex");
    let Some(entry) = g.get_mut(&handle) else {
        return -1;
    };
    let RegistryEntry::Stub {
        destroyed,
        close_requested,
        ..
    } = entry;
    if *destroyed {
        return -1;
    }
    // Idempotent — repeat calls just keep the flag set.
    *close_requested = true;
    0
}

/// Implementation : tear down the OS window + remove from the registry.
///
/// Returns `0` on success ; `-1` on bad-handle (idempotent : second
/// destroy of the same handle returns -1 since the entry has been removed).
pub fn cssl_window_destroy_impl(handle: u64) -> i32 {
    // § T11-W19-β-RT-DELEG-WINDOW : real-backend first. Drop of the
    // RealEntry triggers `cssl_host_window::Window::Drop` →
    // `DestroyWindow(hwnd)` → message-loop wind-down.
    let real_removed = REAL_REGISTRY.with(|reg| reg.borrow_mut().remove(&handle).is_some());
    if real_removed {
        DESTROY_COUNT.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    let removed = {
        let mut g = registry().lock().expect("registry mutex");
        g.remove(&handle).is_some()
    };
    if removed {
        DESTROY_COUNT.fetch_add(1, Ordering::Relaxed);
        0
    } else {
        -1
    }
}

/// Implementation : write the OS-native raw-handle blob into `out`.
///
/// On Win32 the blob is `(HWND, HINSTANCE)` packed as two `usize` words.
/// Returns the number of bytes written on success ; `-1` on bad-handle ;
/// `-2` on buffer-too-small. The function NEVER writes more than
/// `max_len` bytes — the fixed-width platform-handle (16 on 64-bit Win32)
/// is reported via the return code so callers know how big to size.
///
/// # Safety
/// Caller must ensure `out` is valid for `max_len` writable bytes when
/// `max_len > 0`. `max_len == 0` is allowed + the function returns
/// `RAW_HANDLE_MAX_BYTES_WIN32` so the caller learns the required size.
pub unsafe fn cssl_window_raw_handle_impl(handle: u64, out: *mut u8, max_len: usize) -> i32 {
    // § T11-W19-β-RT-DELEG-WINDOW : real-backend first.
    let real_pair = REAL_REGISTRY.with(|reg| -> Option<(usize, usize)> {
        let g = reg.borrow();
        let entry = g.get(&handle)?;
        let raw = entry.win.raw_handle().ok()?;
        match raw.kind {
            RawWindowHandleKind::Win32 { hwnd, hinstance } => Some((hwnd, hinstance)),
            // Future X11/Wayland/Cocoa variants on `#[non_exhaustive]` enum.
            // Stage-0 Win32-only path : drop the entry on unknown shape.
            #[allow(unreachable_patterns)]
            _ => None,
        }
    });
    if let Some((hwnd, hinstance)) = real_pair {
        let needed = RAW_HANDLE_MAX_BYTES_WIN32;
        if max_len < needed {
            return -2;
        }
        if out.is_null() {
            return -2;
        }
        let hwnd_bytes = hwnd.to_le_bytes();
        let hinst_bytes = hinstance.to_le_bytes();
        let word_size = core::mem::size_of::<usize>();
        // SAFETY : caller's contract on (out, max_len) + max_len >= needed.
        unsafe {
            core::ptr::copy_nonoverlapping(hwnd_bytes.as_ptr(), out, word_size);
            core::ptr::copy_nonoverlapping(hinst_bytes.as_ptr(), out.add(word_size), word_size);
        }
        return needed as i32;
    }
    // Tight critical-section : peek the handle's destroyed-flag under the
    // lock + drop before issuing the OS-style write.
    let exists_alive = {
        let g = registry().lock().expect("registry mutex");
        match g.get(&handle) {
            Some(RegistryEntry::Stub { destroyed, .. }) => !*destroyed,
            None => false,
        }
    };
    if !exists_alive {
        return -1;
    }
    // STUB-VS-REAL § : real-backend miss → Stub fallback writes a
    // deterministic (hwnd, hinstance) pair derived from the handle.
    let needed = RAW_HANDLE_MAX_BYTES_WIN32;
    if max_len < needed {
        return -2;
    }
    if out.is_null() {
        return -2;
    }
    // Synthesize a stub blob : (hwnd = handle * 0x10, hinstance = 1).
    // Real impl substitutes the actual HWND + HINSTANCE.
    let stub_hwnd: usize = (handle as usize).wrapping_mul(0x10);
    let stub_hinstance: usize = 1;
    // Write each usize as raw bytes via copy_nonoverlapping ; this avoids
    // the *mut u8 → *mut usize alignment cast (clippy::cast_ptr_alignment)
    // since the caller's `out` is u8-aligned ; the FFI wire-format is
    // host-LE byte-stream regardless of source alignment.
    let hwnd_bytes = stub_hwnd.to_le_bytes();
    let hinst_bytes = stub_hinstance.to_le_bytes();
    let word_size = core::mem::size_of::<usize>();
    // SAFETY : caller's contract on (out, max_len) + we just checked
    // max_len >= needed (= 2 * sizeof(usize)). The two writes target
    // disjoint regions [0, word_size) + [word_size, 2*word_size).
    unsafe {
        core::ptr::copy_nonoverlapping(hwnd_bytes.as_ptr(), out, word_size);
        core::ptr::copy_nonoverlapping(hinst_bytes.as_ptr(), out.add(word_size), word_size);
    }
    needed as i32
}

/// Implementation : read the window's current dimensions.
///
/// Returns `0` on success + writes `(width, height)` to `(w_out, h_out)` ;
/// `-1` on bad-handle ; `-2` on null `w_out` or `h_out`.
///
/// # Safety
/// `w_out` + `h_out` must be valid for one writable u32 each when non-null.
pub unsafe fn cssl_window_get_dims_impl(handle: u64, w_out: *mut u32, h_out: *mut u32) -> i32 {
    if w_out.is_null() || h_out.is_null() {
        return -2;
    }
    // § T11-W19-β-RT-DELEG-WINDOW : real-backend first.
    let real_dims = REAL_REGISTRY.with(|reg| -> Option<(u32, u32)> {
        let g = reg.borrow();
        let entry = g.get(&handle)?;
        Some((entry.width, entry.height))
    });
    if let Some((w, h)) = real_dims {
        // SAFETY : null-check above guarantees writable u32 slot per ptr.
        unsafe {
            w_out.write(w);
            h_out.write(h);
        }
        return 0;
    }
    // Tight critical-section : extract (width, height) under the lock,
    // drop, then write the caller's u32 slots.
    let dims = {
        let g = registry().lock().expect("registry mutex");
        match g.get(&handle) {
            Some(RegistryEntry::Stub {
                width,
                height,
                destroyed,
                ..
            }) if !*destroyed => Some((*width, *height)),
            _ => None,
        }
    };
    let Some((w, h)) = dims else { return -1 };
    // SAFETY : null-check above guarantees writable u32 slot per ptr.
    unsafe {
        w_out.write(w);
        h_out.write(h);
    }
    0
}

// ───────────────────────────────────────────────────────────────────────
// § event-record packing helpers
// ───────────────────────────────────────────────────────────────────────

/// Translate a `cssl_host_window::WindowEvent` into the 32-byte ABI
/// record at `out`.
///
/// # Safety
/// `out` must be valid for `EVENT_RECORD_SIZE` writable bytes.
unsafe fn pack_window_event(out: *mut u8, ev: &cssl_host_window::WindowEvent) {
    // Zero the full record first ; cheap (32 bytes) + future-proof.
    // SAFETY : caller's contract on (out, EVENT_RECORD_SIZE writable).
    unsafe { core::ptr::write_bytes(out, 0, EVENT_RECORD_SIZE) };
    let ts = ev.timestamp_ms;
    let (kind, a, b, c, d): (u16, u32, u32, u32, u32) = match &ev.kind {
        WindowEventKind::Close => (EVENT_KIND_CLOSE, 0, 0, 0, 0),
        WindowEventKind::Resize { width, height } => (EVENT_KIND_RESIZE, *width, *height, 0, 0),
        WindowEventKind::FocusGain => (EVENT_KIND_FOCUS_GAIN, 0, 0, 0, 0),
        WindowEventKind::FocusLoss => (EVENT_KIND_FOCUS_LOSS, 0, 0, 0, 0),
        WindowEventKind::KeyDown { repeat, .. } => {
            (EVENT_KIND_KEY_DOWN, 0, 0, u32::from(*repeat), 0)
        }
        WindowEventKind::KeyUp { .. } => (EVENT_KIND_KEY_UP, 0, 0, 0, 0),
        WindowEventKind::MouseMove { x, y } => {
            (EVENT_KIND_MOUSE_MOVE, *x as u32, *y as u32, 0, 0)
        }
        WindowEventKind::MouseDown { x, y, .. } => {
            (EVENT_KIND_MOUSE_DOWN, 0, *x as u32, *y as u32, 0)
        }
        WindowEventKind::MouseUp { x, y, .. } => (EVENT_KIND_MOUSE_UP, 0, *x as u32, *y as u32, 0),
        WindowEventKind::Scroll { x, y, .. } => (EVENT_KIND_SCROLL, 0, 0, *x as u32, *y as u32),
        WindowEventKind::DpiChanged { scale } => {
            (EVENT_KIND_DPI_CHANGE, scale.to_bits(), 0, 0, 0)
        }
        // Future event kinds on `#[non_exhaustive]` enum drop to NONE
        // record (count-bumped but no payload until cgen learns the new
        // variant + ABI is widened in lock-step).
        #[allow(unreachable_patterns)]
        _ => (EVENT_KIND_NONE, 0, 0, 0, 0),
    };
    let kind_b = kind.to_le_bytes();
    let a_b = a.to_le_bytes();
    let b_b = b.to_le_bytes();
    let c_b = c.to_le_bytes();
    let d_b = d.to_le_bytes();
    let ts_b = ts.to_le_bytes();
    // SAFETY : caller's contract on (out, EVENT_RECORD_SIZE writable).
    unsafe {
        core::ptr::copy_nonoverlapping(kind_b.as_ptr(), out, kind_b.len());
        // payload at offsets 4 / 8 / 12 / 16 (each u32).
        core::ptr::copy_nonoverlapping(a_b.as_ptr(), out.add(4), a_b.len());
        core::ptr::copy_nonoverlapping(b_b.as_ptr(), out.add(8), b_b.len());
        core::ptr::copy_nonoverlapping(c_b.as_ptr(), out.add(12), c_b.len());
        core::ptr::copy_nonoverlapping(d_b.as_ptr(), out.add(16), d_b.len());
        // timestamp at offset 20 (LE u64).
        core::ptr::copy_nonoverlapping(ts_b.as_ptr(), out.add(20), ts_b.len());
    }
}

/// Write a Close event into the slot at `out`.
///
/// # Safety
/// `out` must be valid for `EVENT_RECORD_SIZE` writable bytes.
unsafe fn write_close_event(out: *mut u8, timestamp_ms: u64) {
    // Layout : kind:u16=CLOSE, reserved:u16=0, payload_a..d:u32=0,
    //          timestamp:u64, reserved2:u32=0.
    //
    // Write LE byte-stream via copy_nonoverlapping to avoid the
    // *mut u8 → *mut u16/u64 alignment cast (clippy::cast_ptr_alignment) ;
    // the FFI wire-format is host-LE byte-stream so byte-by-byte deserialize
    // is the canonical reader-side path too.
    let kind_bytes = EVENT_KIND_CLOSE.to_le_bytes();
    let ts_bytes = timestamp_ms.to_le_bytes();
    // SAFETY : caller's contract on (out, EVENT_RECORD_SIZE writable). The
    // three writes target disjoint regions [0,32) zero / [0,2) kind /
    // [20,28) timestamp, all within the 32-byte record.
    unsafe {
        // Zero the full record first ; cheap (32 bytes) + future-proof.
        core::ptr::write_bytes(out, 0, EVENT_RECORD_SIZE);
        // kind at offset 0 (LE u16).
        core::ptr::copy_nonoverlapping(kind_bytes.as_ptr(), out, kind_bytes.len());
        // timestamp at offset 20 (LE u64).
        core::ptr::copy_nonoverlapping(ts_bytes.as_ptr(), out.add(20), ts_bytes.len());
    }
}

// ───────────────────────────────────────────────────────────────────────
// § FFI surface — `__cssl_window_*` `extern "C"` symbols
//
// ABI-stable from Wave-D3 forward. Each symbol delegates to the matching
// `_impl` fn so unit-tests can exercise behavior without going through the
// FFI boundary.
// ───────────────────────────────────────────────────────────────────────

/// FFI : spawn a new window with `(title, width, height, flags)`.
///
/// Returns the issued window-handle on success ; [`INVALID_WINDOW_HANDLE`]
/// (`0`) on failure (invalid dimensions / invalid flags / bad title).
///
/// # Safety
/// Caller must ensure :
/// - `title_ptr` is valid for `title_len` bytes (or `title_len == 0`),
/// - `title_len <= isize::MAX`,
/// - the title bytes are valid UTF-8 (non-UTF-8 yields error).
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_spawn(
    title_ptr: *const u8,
    title_len: usize,
    width: u32,
    height: u32,
    flags: u32,
) -> u64 {
    // SAFETY : caller's contract on (title_ptr, title_len) inherited.
    unsafe { cssl_window_spawn_impl(title_ptr, title_len, width, height, flags) }
}

/// FFI : pump pending events into `events_out`.
///
/// Returns the number of events written (0..=max_events) on success ; a
/// negative error-code on failure.
///
/// # Safety
/// Caller must ensure `events_out` is valid for `max_events *
/// EVENT_RECORD_SIZE` bytes when `max_events > 0`.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_pump(
    handle: u64,
    events_out: *mut u8,
    max_events: usize,
) -> i64 {
    // SAFETY : caller's contract on (events_out, max_events).
    unsafe { cssl_window_pump_impl(handle, events_out, max_events) }
}

/// FFI : send a close-request to the OS window.
///
/// Returns `0` on success ; `-1` on bad-handle / already-destroyed.
///
/// # Safety
/// Always safe ; the `unsafe` qualifier is only for `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_request_close(handle: u64) -> i32 {
    cssl_window_request_close_impl(handle)
}

/// FFI : destroy the OS window + remove from the registry.
///
/// Returns `0` on success ; `-1` on bad-handle. Idempotent : second call
/// on the same handle returns `-1` because the entry has been removed.
///
/// # Safety
/// Always safe ; the `unsafe` qualifier is only for `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_destroy(handle: u64) -> i32 {
    cssl_window_destroy_impl(handle)
}

/// FFI : write the OS-native raw-handle into `out`.
///
/// Returns bytes-written on success ; `-1` on bad-handle ; `-2` on
/// buffer-too-small / null-out.
///
/// # Safety
/// Caller must ensure `out` is valid for `max_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_raw_handle(
    handle: u64,
    out: *mut u8,
    max_len: usize,
) -> i32 {
    // SAFETY : caller's contract on (out, max_len).
    unsafe { cssl_window_raw_handle_impl(handle, out, max_len) }
}

/// FFI : read the current window dimensions into `(w_out, h_out)`.
///
/// Returns `0` on success ; `-1` on bad-handle ; `-2` on null out-ptrs.
///
/// # Safety
/// Caller must ensure `w_out` + `h_out` are valid for one `u32` write
/// each when non-null.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_get_dims(
    handle: u64,
    w_out: *mut u32,
    h_out: *mut u32,
) -> i32 {
    // SAFETY : caller's contract on (w_out, h_out).
    unsafe { cssl_window_get_dims_impl(handle, w_out, h_out) }
}

// ───────────────────────────────────────────────────────────────────────
// § tests — ABI-shape locks + stub-registry round-trips
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // § Test serialization : registry is a process-wide singleton. The
    // tests reset state between cases to keep them order-independent.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_for_tests();
        g
    }

    // ── ABI-shape locks ─────────────────────────────────────────────────

    #[test]
    fn event_record_size_is_locked_at_32() {
        // ‼ ABI-locked : drift here breaks cssl-cgen-cpu-cranelift::cgen_window.
        assert_eq!(EVENT_RECORD_SIZE, 32);
    }

    #[test]
    fn event_kind_discriminants_are_distinct_and_locked() {
        // ‼ ABI-locked : the integer values are pinned by cgen_window.
        let kinds = [
            EVENT_KIND_NONE,
            EVENT_KIND_CLOSE,
            EVENT_KIND_RESIZE,
            EVENT_KIND_FOCUS_GAIN,
            EVENT_KIND_FOCUS_LOSS,
            EVENT_KIND_KEY_DOWN,
            EVENT_KIND_KEY_UP,
            EVENT_KIND_MOUSE_MOVE,
            EVENT_KIND_MOUSE_DOWN,
            EVENT_KIND_MOUSE_UP,
            EVENT_KIND_SCROLL,
            EVENT_KIND_DPI_CHANGE,
        ];
        // All distinct.
        for (i, &a) in kinds.iter().enumerate() {
            for (j, &b) in kinds.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "event kinds at idx {i}+{j} collide");
                }
            }
        }
        // Pinned ordinals (locked by cgen_window).
        assert_eq!(EVENT_KIND_NONE, 0);
        assert_eq!(EVENT_KIND_CLOSE, 1);
        assert_eq!(EVENT_KIND_RESIZE, 2);
        assert_eq!(EVENT_KIND_DPI_CHANGE, 11);
    }

    #[test]
    fn spawn_flag_bits_are_distinct_and_subset_of_mask() {
        let bits = [
            SPAWN_FLAG_RESIZABLE,
            SPAWN_FLAG_FULLSCREEN,
            SPAWN_FLAG_DPI_AWARE,
            SPAWN_FLAG_BORDERLESS,
        ];
        for (i, &b) in bits.iter().enumerate() {
            assert!(b.is_power_of_two(), "spawn flag at {i} = {b:#x} non-power-of-two");
            assert!(b & !SPAWN_FLAG_MASK == 0, "spawn flag at {i} not in mask");
        }
        // Pairwise disjoint.
        for (i, &a) in bits.iter().enumerate() {
            for (j, &c) in bits.iter().enumerate() {
                if i != j {
                    assert_eq!(a & c, 0);
                }
            }
        }
    }

    #[test]
    fn validate_spawn_flags_accepts_known_combinations() {
        assert!(validate_spawn_flags(0));
        assert!(validate_spawn_flags(SPAWN_FLAG_RESIZABLE));
        assert!(validate_spawn_flags(SPAWN_FLAG_RESIZABLE | SPAWN_FLAG_DPI_AWARE));
        assert!(validate_spawn_flags(SPAWN_FLAG_FULLSCREEN));
    }

    #[test]
    fn validate_spawn_flags_rejects_unknown_bits() {
        let bad = 1u32 << 31;
        assert!(!validate_spawn_flags(bad));
    }

    #[test]
    fn validate_spawn_flags_rejects_fullscreen_and_borderless_together() {
        // Contradictory : fullscreen IS already borderless. Reject.
        let bad = SPAWN_FLAG_FULLSCREEN | SPAWN_FLAG_BORDERLESS;
        assert!(!validate_spawn_flags(bad));
    }

    // ── spawn round-trips ───────────────────────────────────────────────

    #[test]
    fn spawn_returns_nonzero_handle_on_valid_args() {
        let _g = lock_and_reset();
        let title = b"hello";
        // SAFETY : title slice is valid for title.len() bytes.
        let handle = unsafe {
            cssl_window_spawn_impl(title.as_ptr(), title.len(), 800, 600, SPAWN_FLAG_RESIZABLE)
        };
        assert_ne!(handle, INVALID_WINDOW_HANDLE);
        assert_eq!(spawn_count(), 1);
    }

    #[test]
    fn spawn_rejects_zero_width() {
        let _g = lock_and_reset();
        let title = b"x";
        let handle = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 0, 600, 0) };
        assert_eq!(handle, INVALID_WINDOW_HANDLE);
    }

    #[test]
    fn spawn_rejects_zero_height() {
        let _g = lock_and_reset();
        let title = b"x";
        let handle = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 800, 0, 0) };
        assert_eq!(handle, INVALID_WINDOW_HANDLE);
    }

    #[test]
    fn spawn_rejects_empty_title() {
        let _g = lock_and_reset();
        let handle = unsafe { cssl_window_spawn_impl(core::ptr::null(), 0, 800, 600, 0) };
        assert_eq!(handle, INVALID_WINDOW_HANDLE);
    }

    #[test]
    fn spawn_rejects_bad_utf8_title() {
        let _g = lock_and_reset();
        // Invalid UTF-8 : 0xFF is not a valid leading byte.
        let bad = [0xFFu8, 0xFE, 0xFD];
        let handle = unsafe { cssl_window_spawn_impl(bad.as_ptr(), bad.len(), 800, 600, 0) };
        assert_eq!(handle, INVALID_WINDOW_HANDLE);
    }

    #[test]
    fn spawn_rejects_bad_flags() {
        let _g = lock_and_reset();
        let title = b"x";
        let bad_flags = 1u32 << 31;
        let handle = unsafe {
            cssl_window_spawn_impl(title.as_ptr(), title.len(), 800, 600, bad_flags)
        };
        assert_eq!(handle, INVALID_WINDOW_HANDLE);
    }

    #[test]
    fn spawn_handles_are_monotonic_and_distinct() {
        let _g = lock_and_reset();
        let title = b"x";
        let h1 = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let h2 = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let h3 = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
        assert!(h1 < h2);
        assert!(h2 < h3);
        assert_eq!(spawn_count(), 3);
    }

    // ── pump_events ─────────────────────────────────────────────────────

    #[test]
    fn pump_returns_bad_handle_for_unknown_id() {
        let _g = lock_and_reset();
        let mut buf = [0u8; EVENT_RECORD_SIZE];
        let n = unsafe { cssl_window_pump_impl(0xDEAD_BEEF, buf.as_mut_ptr(), 1) };
        assert_eq!(n, PUMP_ERR_BAD_HANDLE);
    }

    #[test]
    fn pump_returns_zero_on_idle_window() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let mut buf = [0u8; EVENT_RECORD_SIZE * 4];
        let n = unsafe { cssl_window_pump_impl(h, buf.as_mut_ptr(), 4) };
        // Post-T11-W19-β-RT-DELEG-WINDOW : on Windows hosts the spawn
        // returns a Real-Win32 entry. The first pump after CreateWindowExW
        // legitimately drains startup messages (focus-gain, paint, sizing)
        // — the count is non-deterministic across CI vs developer hosts.
        // Stub path remains exact ; real path bounds upper-limit.
        assert!(
            (0..=4).contains(&n),
            "pump events count must be in [0, 4] (stub: 0 ; real: 0-4) ; got {n}"
        );
    }

    #[test]
    fn pump_emits_close_event_after_request_close() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        assert_eq!(cssl_window_request_close_impl(h), 0);
        let mut buf = [0u8; EVENT_RECORD_SIZE];
        let n = unsafe { cssl_window_pump_impl(h, buf.as_mut_ptr(), 1) };
        assert_eq!(n, 1, "expected 1 close event after request_close");
        // Decode kind from offset 0 (LE u16).
        let kind = u16::from_le_bytes([buf[0], buf[1]]);
        assert_eq!(kind, EVENT_KIND_CLOSE);
        // Subsequent pump returns 0 (request consumed).
        let n2 = unsafe { cssl_window_pump_impl(h, buf.as_mut_ptr(), 1) };
        assert_eq!(n2, 0);
    }

    #[test]
    fn pump_with_null_buf_and_max_zero_is_ok() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let n = unsafe { cssl_window_pump_impl(h, core::ptr::null_mut(), 0) };
        assert_eq!(n, 0);
    }

    #[test]
    fn pump_with_null_buf_and_nonzero_max_returns_null_buf_err() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let n = unsafe { cssl_window_pump_impl(h, core::ptr::null_mut(), 4) };
        assert_eq!(n, PUMP_ERR_NULL_BUF);
    }

    #[test]
    fn pump_count_increments_on_every_call() {
        let _g = lock_and_reset();
        let before = pump_count();
        let mut buf = [0u8; EVENT_RECORD_SIZE];
        let _ = unsafe { cssl_window_pump_impl(0xDEAD, buf.as_mut_ptr(), 1) };
        let _ = unsafe { cssl_window_pump_impl(0xBEEF, buf.as_mut_ptr(), 1) };
        assert_eq!(pump_count(), before + 2);
    }

    // ── request_close ───────────────────────────────────────────────────

    #[test]
    fn request_close_is_idempotent() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        assert_eq!(cssl_window_request_close_impl(h), 0);
        // Repeat call still returns 0 (idempotent).
        assert_eq!(cssl_window_request_close_impl(h), 0);
        assert_eq!(cssl_window_request_close_impl(h), 0);
    }

    #[test]
    fn request_close_on_bad_handle_returns_minus_one() {
        let _g = lock_and_reset();
        assert_eq!(cssl_window_request_close_impl(0), -1);
        assert_eq!(cssl_window_request_close_impl(0xDEAD_BEEF), -1);
    }

    // ── destroy ─────────────────────────────────────────────────────────

    #[test]
    fn destroy_removes_handle_and_increments_count() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        assert_eq!(cssl_window_destroy_impl(h), 0);
        assert_eq!(destroy_count(), 1);
    }

    #[test]
    fn destroy_second_call_on_same_handle_returns_minus_one() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        assert_eq!(cssl_window_destroy_impl(h), 0);
        assert_eq!(cssl_window_destroy_impl(h), -1, "second destroy = -1");
    }

    #[test]
    fn destroy_on_bad_handle_returns_minus_one() {
        let _g = lock_and_reset();
        assert_eq!(cssl_window_destroy_impl(0), -1);
        assert_eq!(cssl_window_destroy_impl(0xDEAD_BEEF), -1);
    }

    // ── raw_handle ──────────────────────────────────────────────────────

    #[test]
    fn raw_handle_writes_two_usize_words_on_64bit_host() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let mut buf = [0u8; RAW_HANDLE_MAX_BYTES_WIN32];
        let n = unsafe { cssl_window_raw_handle_impl(h, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(n as usize, RAW_HANDLE_MAX_BYTES_WIN32);
        // Decode the (hwnd, hinstance) pair via byte-level deserialize.
        // Post-T11-W19-β-RT-DELEG-WINDOW : on Windows hosts the spawn
        // returns a Real entry whose hwnd is an actual `HWND` (kernel-
        // assigned ; non-zero ; non-deterministic across runs). The
        // Stub-only assertion `hwnd == handle * 0x10` no longer applies
        // when the real backend wins. We assert : (a) at least one of
        // hwnd / hinstance is non-zero, (b) bytes round-trip cleanly.
        let word_size = core::mem::size_of::<usize>();
        let mut hwnd_bytes = [0u8; core::mem::size_of::<usize>()];
        let mut hinst_bytes = [0u8; core::mem::size_of::<usize>()];
        hwnd_bytes.copy_from_slice(&buf[..word_size]);
        hinst_bytes.copy_from_slice(&buf[word_size..2 * word_size]);
        let hwnd = usize::from_le_bytes(hwnd_bytes);
        let hinstance = usize::from_le_bytes(hinst_bytes);
        assert_ne!(hwnd, 0, "hwnd must be non-zero (real or stub)");
        // Stub path : (hwnd = handle * 0x10, hinstance = 1).
        // Real path : real (HWND, HINSTANCE) ; both non-zero on success.
        let stub_hwnd_expected = (h as usize).wrapping_mul(0x10);
        if hwnd == stub_hwnd_expected {
            // Stub path verified.
            assert_eq!(hinstance, 1, "stub hinstance sentinel = 1");
        } else {
            // Real-Win32 path : both should be non-zero kernel-handles.
            assert_ne!(hinstance, 0, "real-Win32 hinstance must be non-zero");
        }
    }

    #[test]
    fn raw_handle_buffer_too_small_returns_minus_two() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let mut buf = [0u8; 4]; // way too small (need at least 16 on 64-bit).
        let n = unsafe { cssl_window_raw_handle_impl(h, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(n, -2);
    }

    #[test]
    fn raw_handle_bad_handle_returns_minus_one() {
        let _g = lock_and_reset();
        let mut buf = [0u8; RAW_HANDLE_MAX_BYTES_WIN32];
        let n = unsafe { cssl_window_raw_handle_impl(0xDEAD, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(n, -1);
    }

    // ── get_dims ────────────────────────────────────────────────────────

    #[test]
    fn get_dims_returns_initial_dimensions() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 1280, 720, 0) };
        let mut w = 0u32;
        let mut height_out = 0u32;
        let r = unsafe { cssl_window_get_dims_impl(h, &mut w, &mut height_out) };
        assert_eq!(r, 0);
        assert_eq!(w, 1280);
        assert_eq!(height_out, 720);
    }

    #[test]
    fn get_dims_null_out_ptr_returns_minus_two() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        let mut h_out = 0u32;
        let r = unsafe { cssl_window_get_dims_impl(h, core::ptr::null_mut(), &mut h_out) };
        assert_eq!(r, -2);
        let mut w_out = 0u32;
        let r2 = unsafe { cssl_window_get_dims_impl(h, &mut w_out, core::ptr::null_mut()) };
        assert_eq!(r2, -2);
    }

    #[test]
    fn get_dims_bad_handle_returns_minus_one() {
        let _g = lock_and_reset();
        let mut w = 0u32;
        let mut h = 0u32;
        let r = unsafe { cssl_window_get_dims_impl(0xDEAD, &mut w, &mut h) };
        assert_eq!(r, -1);
    }

    // ── lifecycle / cross-fn invariants ─────────────────────────────────

    #[test]
    fn pump_after_destroy_returns_bad_handle() {
        let _g = lock_and_reset();
        let title = b"x";
        let h = unsafe { cssl_window_spawn_impl(title.as_ptr(), title.len(), 100, 100, 0) };
        assert_eq!(cssl_window_destroy_impl(h), 0);
        let mut buf = [0u8; EVENT_RECORD_SIZE];
        // Post-destroy : registry entry removed ⇒ BAD_HANDLE.
        let n = unsafe { cssl_window_pump_impl(h, buf.as_mut_ptr(), 1) };
        assert_eq!(n, PUMP_ERR_BAD_HANDLE);
    }

    #[test]
    fn ffi_extern_symbols_round_trip_through_impl() {
        // ‼ Compile-time check : the extern "C" symbols MUST exist with the
        // expected signatures. Renaming any of them is a major-version bump.
        let _: unsafe extern "C" fn(*const u8, usize, u32, u32, u32) -> u64 = __cssl_window_spawn;
        let _: unsafe extern "C" fn(u64, *mut u8, usize) -> i64 = __cssl_window_pump;
        let _: unsafe extern "C" fn(u64) -> i32 = __cssl_window_request_close;
        let _: unsafe extern "C" fn(u64) -> i32 = __cssl_window_destroy;
        let _: unsafe extern "C" fn(u64, *mut u8, usize) -> i32 = __cssl_window_raw_handle;
        let _: unsafe extern "C" fn(u64, *mut u32, *mut u32) -> i32 = __cssl_window_get_dims;
    }

    #[test]
    fn handle_zero_is_invalid_sentinel() {
        // ‼ Locked : handle 0 = INVALID_WINDOW_HANDLE ; never issued by spawn.
        assert_eq!(INVALID_WINDOW_HANDLE, 0);
        // Operations on handle 0 always fail.
        assert_eq!(cssl_window_request_close_impl(0), -1);
        assert_eq!(cssl_window_destroy_impl(0), -1);
    }
}

// ── INTEGRATION_NOTE ────────────────────────────────────────────────────
//
// § Wave-D3 dispatch : "Touch ONLY cssl-rt/src/host_window.rs (NEW)
//   + cssl-cgen-cpu-cranelift/src/cgen_window.rs (NEW)".
//
// This module is delivered as a NEW file ; `crate::lib.rs` + `Cargo.toml`
// are intentionally NOT modified per task constraint. The `__cssl_window_*`
// extern symbols are reachable at link-time once the file is added to the
// build (the `crates/*` glob in `compiler-rs/Cargo.toml [workspace]` is
// sufficient + the `pub mod host_window;` line is the only follow-up
// touch needed in `lib.rs` for the public Rust-side surface).
//
// § Main-thread integration follow-up (small ; ≤ 20 lines across 2 files) :
//   1. Add to `compiler-rs/crates/cssl-rt/Cargo.toml [dependencies]` :
//        `cssl-host-window = { path = "../cssl-host-window" }`
//      (and the workspace.dependencies entry in `compiler-rs/Cargo.toml`
//      if absent).
//   2. Add to `compiler-rs/crates/cssl-rt/src/lib.rs` :
//        `pub mod host_window;`
//      and the matching `pub use host_window::{ ... };` re-exports.
//   3. Replace the `RegistryEntry::Stub` variant body at the spawn /
//      pump / raw_handle / get_dims call-sites with a
//      `cssl_host_window::Window`-holding `RegistryEntry::Real` variant.
//      The public ABI shape (handle-as-u64 + 32-byte event records +
//      return-codes) is locked PRE-stub via the const + assertion lattice
//      in this file ; the swap is invisible to source-level CSSLv3 code +
//      to cssl-cgen-cpu-cranelift::cgen_window.
//   4. Wire the corresponding cssl-mir::CsslOp::Window* variants once
//      Wave-D5 (host_input) lands its MIR-op surface ; cgen_window will
//      then add a `lower_window_op_to_symbol` dispatcher mirroring
//      `cgen_net::lower_net_op_to_symbol`.
