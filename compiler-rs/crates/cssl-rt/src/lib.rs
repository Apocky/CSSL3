//! § cssl-rt — CSSLv3 stage-0 runtime library
//! ═══════════════════════════════════════════
//!
//! Authoritative spec : `specs/01_BOOTSTRAP.csl § RUNTIME-LIB`
//!                    + `specs/22_TELEMETRY.csl § RING-INTEGRATION` (deferred).
//!
//! § ROLE
//!   Linked into every CSSLv3 artifact. Provides:
//!     - allocator + tracker     (`alloc` module)
//!     - panic + format          (`panic` module)
//!     - exit + abort            (`exit` module)
//!     - entry shim              (`runtime` module)
//!     - file-system I/O         (`io` + `io_win32` / `io_unix`)
//!     - stable FFI surface      (`ffi` module)
//!
//! § FFI SURFACE  (ABI-stable)
//!   S6-A1 (T11-D52) — runtime baseline :
//!   ```text
//!   __cssl_entry(user_main: extern "C" fn() -> i32) -> i32
//!   __cssl_alloc(size, align) -> *mut u8
//!   __cssl_free(ptr, size, align)
//!   __cssl_realloc(ptr, old_size, new_size, align) -> *mut u8
//!   __cssl_panic(msg, msg_len, file, file_len, line) -> !
//!   __cssl_abort() -> !
//!   __cssl_exit(code: i32) -> !
//!   ```
//!   S6-B5 (T11-D76) — fs surface :
//!   ```text
//!   __cssl_fs_open(path_ptr, path_len, flags) -> i64
//!   __cssl_fs_read(handle, buf_ptr, buf_len) -> i64
//!   __cssl_fs_write(handle, buf_ptr, buf_len) -> i64
//!   __cssl_fs_close(handle) -> i64
//!   __cssl_fs_last_error_kind() -> i32
//!   __cssl_fs_last_error_os() -> i32
//!   ```
//!   S7-F4 (T11-D82) — net surface :
//!   ```text
//!   __cssl_net_socket(flags) -> i64
//!   __cssl_net_listen(sock, addr, port, backlog) -> i64
//!   __cssl_net_accept(sock) -> i64
//!   __cssl_net_connect(sock, addr, port) -> i64
//!   __cssl_net_send(sock, buf_ptr, buf_len) -> i64
//!   __cssl_net_recv(sock, buf_ptr, buf_len) -> i64
//!   __cssl_net_sendto(sock, buf_ptr, buf_len, addr, port) -> i64
//!   __cssl_net_recvfrom(sock, buf_ptr, buf_len, *addr_out, *port_out) -> i64
//!   __cssl_net_close(sock) -> i64
//!   __cssl_net_local_addr(sock, *addr_out, *port_out) -> i64
//!   __cssl_net_last_error_kind() -> i32
//!   __cssl_net_last_error_os() -> i32
//!   __cssl_net_caps_grant(bits) -> i32
//!   __cssl_net_caps_revoke(bits) -> i32
//!   __cssl_net_caps_current() -> i32
//!   ```
//!   See [`ffi`] for full documentation. Renaming any of these symbols
//!   is a major-version-bump event.
//!
//! § STAGE-0 SCOPE
//!   - Hosted (`cargo build`) only ; freestanding-target = phase-B work.
//!   - `std::alloc::System` backs the allocator ; no `mmap`/`VirtualAlloc`
//!     direct calls yet.
//!   - Telemetry-ring integration is stubbed.
//!   - TLS slot creation is stubbed (no per-thread state yet).
//!
//! § INVARIANTS
//!   - All counters are atomic ⇒ thread-safe from day-1.
//!   - All FFI shims have `_impl` Rust-side counterparts so unit tests
//!     can exercise behavior without going through the FFI boundary.
//!   - On panic, the handler emits the formatted line then aborts ;
//!     it does NOT unwind across the FFI boundary.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone /
//!    anything / anybody."
//!   This crate's stage-0 surface is intentionally minimal + auditable.
//!   No surveillance, no telemetry-without-consent, no hidden side-channels.
//!   Tracker counters are LOCAL ; nothing escapes the process.

// § T11-D52 (S6-A1) : `unsafe_code` downgraded from `forbid` to `deny` ;
// the FFI surface fundamentally requires `extern "C"` + raw-pointer ops.
// Each unsafe block carries an inline SAFETY paragraph.
#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_safety_doc)]

pub mod alloc;
// § T11-D286 (W-E5-3) — runtime cap-verify defense-in-depth helper. Pairs
// with `cssl-mir::cap_runtime_check` which emits `cssl.cap.verify` ops at
// every cap-boundary, and with `cssl-cgen-cpu-cranelift::jit` which
// lowers each emitted op to a `call __cssl_cap_verify(handle, op_kind)`
// against this module's FFI symbol. The static HIR `cap_check` pass
// remains authoritative ; this is a defense-in-depth runtime check.
pub mod cap_verify;
pub mod exit;
pub mod ffi;
// § Wave-D host-FFI surface (T11-D250 integration commit) — modules
// authored by W-D1..D8 fanout were committed dormant per race-discipline.
// This integration commit activates the seven cssl-rt host-* modules ;
// cssl-cgen-cpu-cranelift's matching cgen_* modules are activated in
// parallel. Real-backend swaps (cssl-host-vulkan / -d3d12 / -openxr / etc.)
// are deferred to post-Wave-E backend-binding work — current modules are
// stub or stdlib-only impls. Each module's `INTEGRATION_NOTE` block
// documents the swap-points + future cargo-dep additions.
pub mod host_audio;
pub mod host_gpu;
pub mod host_input;
pub mod host_thread;
pub mod host_time;
pub mod host_window;
pub mod host_xr;
pub mod io;
#[cfg(not(target_os = "windows"))]
pub mod io_unix;
// § T11-D317 (W-CSSL-rt-LoA-stubs) — host-symbol stubs for the LoA
// `__cssl_*` FFI surface (window / GPU / ω-field / time / telemetry /
// audio / MCP / cap / player / scene / utility). Stage-1 backend-binding
// work incrementally replaces each stub with its real implementation ;
// the ABI signatures locked here are the stable contract LoA scenes
// link against. See `loa_stubs.rs` § STAGE-1 PATH per-symbol.
pub mod loa_stubs;
// § T11-LOA-LOG-1 — auto-init logger : .CRT$XCU / .init_array / __mod_init_func
// ctor that fires before main() so any binary linked with cssl-rt produces
// logs/loa_runtime.log + a stderr banner from frame zero. Activated only
// once cssl-rt is in the binary's link surface (force via cssl.heap.alloc
// or future csslc-side default-link).
pub mod loa_startup;
#[cfg(target_os = "windows")]
pub mod io_win32;
pub mod net;
#[cfg(not(target_os = "windows"))]
pub mod net_unix;
#[cfg(target_os = "windows")]
pub mod net_win32;
pub mod panic;
pub mod path_hash;
pub mod runtime;

// ───────────────────────────────────────────────────────────────────────
// § public re-exports — the top-level surface tests + downstream code use
// ───────────────────────────────────────────────────────────────────────

pub use alloc::{
    alloc_count, bytes_allocated_total, bytes_freed_total, bytes_in_use, free_count,
    reset_for_tests, AllocTracker, BumpArena, ALIGN_MAX,
};
// § T11-D286 (W-E5-3) — re-export the cap-verify surface so cgen-side
// import-helpers and unit tests can reach it via `cssl_rt::*`.
pub use cap_verify::{
    cap_verify_impl, reset_cap_verify_for_tests, verify_call_count, verify_deny_count,
    CAP_INDEX_BOX, CAP_INDEX_ISO, CAP_INDEX_REF, CAP_INDEX_TAG, CAP_INDEX_TRN, CAP_INDEX_VAL,
    CAP_KIND_COUNT, OP_CALL_PASS_PARAM, OP_FIELD_ACCESS, OP_FN_ENTRY, OP_KIND_MAX, OP_RETURN,
};
pub use exit::{
    abort_count, cssl_abort_impl, cssl_exit_impl, exit_count, last_exit_code, record_abort,
    record_exit, reset_exit_state_for_tests, testable_abort, testable_exit, ExitError,
};
// § Wave-D host-FFI re-exports (T11-D250).
//
// Avoids name-collisions with `alloc::reset_for_tests` by accessing
// per-module reset-fns through their module path (see test_helpers
// below). The re-exported surface here is the ABI-constants + the
// audit-counter accessors + selected `_impl` helpers callers need
// at the top-level for ergonomic test-writing.
pub use host_audio::{
    audio_caps_current, audio_caps_grant, audio_caps_revoke, audio_error_code, is_valid_format,
    last_audio_error_kind, reset_audio_for_tests, AUDIO_CAP_DEFAULT, AUDIO_CAP_INPUT,
    AUDIO_CAP_MASK, AUDIO_CAP_OUTPUT, FMT_F32, FMT_F64, FMT_I16, FMT_I24, FMT_I32, FMT_MAX,
    FMT_MIN, INVALID_STREAM, MAX_STREAMS, STREAM_EXCLUSIVE, STREAM_FLAG_MASK, STREAM_INPUT,
    STREAM_NONBLOCK, STREAM_OUTPUT, STREAM_SHARED,
};
pub use host_gpu::{
    pipeline_kind_from_u32, pipeline_kind_to_u32, GpuPipelineKind, GPU_HANDLE_ERROR_SENTINEL,
    GPU_I32_ERROR_SENTINEL, GPU_I32_OK_SENTINEL, GPU_PIPELINE_IR_LEN_MAX,
    GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL,
};
pub use host_input::{
    caps_satisfied as input_caps_satisfied, gamepad_read_count, input_error_count,
    keyboard_read_count, last_input_error, mouse_delta_read_count, mouse_read_count,
    GAMEPAD_SLOT_COUNT, GAMEPAD_STATE_BYTES, INPUT_CAP_GAMEPAD, INPUT_CAP_KEYBOARD,
    INPUT_CAP_MASK, INPUT_CAP_MOUSE, INPUT_CAP_MOUSE_DELTA, INPUT_CAP_NONE,
    INPUT_ERR_BUFFER_TOO_SMALL, INPUT_ERR_CAP_DENIED, INPUT_ERR_DISCONNECTED,
    INPUT_ERR_INVALID_HANDLE, INPUT_ERR_INVALID_INDEX, INPUT_ERR_NULL_OUT, INPUT_HANDLE_SLOT_COUNT,
    INPUT_OK, KEYBOARD_STATE_BYTES, MOUSE_STATE_BYTES,
};
pub use host_thread::{
    map_load_ordering, map_ordering, map_store_ordering, ATOMIC_CAS_U64_SYMBOL,
    ATOMIC_LOAD_U64_SYMBOL, ATOMIC_STORE_U64_SYMBOL, MUTEX_CREATE_SYMBOL, MUTEX_DESTROY_SYMBOL,
    MUTEX_LOCK_SYMBOL, MUTEX_UNLOCK_SYMBOL, ORDERING_ACQUIRE, ORDERING_ACQ_REL, ORDERING_RELAXED,
    ORDERING_RELEASE, ORDERING_SEQ_CST, RC_ERR, RC_OK, THREAD_JOIN_SYMBOL, THREAD_SPAWN_SYMBOL,
};
pub use host_time::{
    cssl_time_deadline_until_impl, cssl_time_monotonic_ns_impl, cssl_time_sleep_ns_impl,
    cssl_time_wall_unix_ns_impl, deadline_count, monotonic_count, reset_time_for_tests,
    sleep_count, total_sleep_ns, wall_count, TIME_DEADLINE_ALREADY_PAST, TIME_ERR, TIME_OK,
};
pub use host_window::{
    cssl_window_destroy_impl, cssl_window_request_close_impl, destroy_count, pump_count,
    spawn_count, validate_spawn_flags, EVENT_KIND_CLOSE, EVENT_KIND_DPI_CHANGE,
    EVENT_KIND_FOCUS_GAIN, EVENT_KIND_FOCUS_LOSS, EVENT_KIND_KEY_DOWN, EVENT_KIND_KEY_UP,
    EVENT_KIND_MOUSE_DOWN, EVENT_KIND_MOUSE_MOVE, EVENT_KIND_MOUSE_UP, EVENT_KIND_NONE,
    EVENT_KIND_RESIZE, EVENT_KIND_SCROLL, EVENT_RECORD_SIZE, INVALID_WINDOW_HANDLE,
    PUMP_ERR_BAD_HANDLE, PUMP_ERR_DESTROYED, PUMP_ERR_NULL_BUF, RAW_HANDLE_MAX_BYTES_WIN32,
    SPAWN_FLAG_BORDERLESS, SPAWN_FLAG_DPI_AWARE, SPAWN_FLAG_FULLSCREEN, SPAWN_FLAG_MASK,
    SPAWN_FLAG_RESIZABLE,
};
pub use host_xr::{
    eye_name, input_state_count, last_xr_error_kind, last_xr_error_os, pose_stream_count,
    record_xr_error, reset_xr_for_tests, session_create_count, session_destroy_count,
    swapchain_acquire_count, swapchain_release_count, xr_caps_current, xr_caps_grant,
    xr_caps_revoke, xr_error_code, xr_input_state_impl, xr_pose_stream_impl,
    xr_session_create_impl, xr_session_destroy_impl, xr_swapchain_stereo_acquire_impl,
    xr_swapchain_stereo_release_impl, INVALID_XR_HANDLE, XR_CAP_INPUT, XR_CAP_MASK, XR_CAP_NONE,
    XR_CAP_POSE_CONTROLLER, XR_CAP_POSE_HEAD, XR_CAP_SESSION, XR_CAP_SWAPCHAIN, XR_EYE_COUNT,
    XR_EYE_LEFT, XR_EYE_RIGHT, XR_INPUT_STATE_BYTES, XR_MAX_SESSIONS, XR_POSE_BYTES,
    XR_POSE_STREAM_BYTES,
};
// § T11-D317 (W-CSSL-rt-LoA-stubs) — re-export the LoA-stub surface so
// downstream tests + cgen-side helpers can reach the constants/handles
// without going through the FFI boundary. Each symbol's `extern "C"`
// `#[no_mangle]` export is what LoA scenes link against ; these re-exports
// are convenience for Rust-side consumers.
pub use loa_stubs::{
    loa_monotonic_count, FIELD_CELL_BYTES, LOA_ERR, LOA_INVALID_HANDLE, LOA_OK, LOA_TRUE,
    MCP_TOOL_REGISTRY_CAP, PLAYER_STATE_BYTES, TELEMETRY_PAYLOAD_MAX, TELEMETRY_RING_CAP,
};
pub use io::{
    bytes_read_total, bytes_written_total, close_count, io_error_code, last_io_error_kind,
    last_io_error_os, open_count, path_hash_events_drain, path_hash_overflow_count, read_count,
    record_io_error, record_path_hash_event, reset_io_for_tests, reset_last_io_error_for_tests,
    validate_buffer, validate_open_flags, write_count, PathHashEvent, PathHashOpKind,
    INVALID_HANDLE, OPEN_APPEND, OPEN_CREATE, OPEN_CREATE_NEW, OPEN_FLAG_MASK, OPEN_READ,
    OPEN_READ_WRITE, OPEN_TRUNCATE, OPEN_WRITE, PATH_HASH_QUEUE_CAPACITY,
};
pub use net::{
    accept_count, addr_is_loopback, bytes_recv_total, bytes_sent_total, caps_current, caps_grant,
    caps_revoke, check_caps_for_addr, connect_count, last_net_error_kind, last_net_error_os,
    listen_count, net_close_count, net_error_code, record_net_error, recv_count,
    reset_net_for_tests, send_count, socket_count, validate_sock_flags, ANY_V4, INVALID_SOCKET,
    LOOPBACK_V4, NET_CAP_DEFAULT, NET_CAP_INBOUND, NET_CAP_LOOPBACK, NET_CAP_MASK,
    NET_CAP_OUTBOUND, SOCK_FLAG_MASK, SOCK_NODELAY, SOCK_NONBLOCK, SOCK_REUSEADDR, SOCK_TCP,
    SOCK_UDP,
};
pub use panic::{format_panic, panic_count, record_panic, reset_panic_count_for_tests};
pub use path_hash::{hash_path_bytes, install_test_salt};
pub use runtime::{
    cssl_entry_impl, entry_invocation_count, init_count, is_runtime_initialized,
    reset_runtime_for_tests,
};

// ───────────────────────────────────────────────────────────────────────
// § crate-level metadata
// ───────────────────────────────────────────────────────────────────────

/// Crate version string (from `Cargo.toml`).
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

// ───────────────────────────────────────────────────────────────────────
// § test helpers : single shared lock for cross-module test serialization
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(unreachable_pub)]
pub(crate) mod test_helpers {
    //! Crate-shared test lock + reset.
    //!
    //! § WHY  Every test in this crate touches global counters
    //! (`TRACKER` / `PANIC_COUNT` / `LAST_EXIT_CODE` / `RUNTIME_INITIALIZED`).
    //! Per-module Mutex's would let tests in *different* modules race on
    //! shared globals (e.g., an `alloc::tests::*` test and a
    //! `ffi::tests::*` test both reset `TRACKER`). One crate-shared lock
    //! eliminates the cross-module race at the cost of forcing all
    //! global-state tests to serialize.
    //!
    //! Tests that don't touch any global state may skip the lock.

    use std::sync::Mutex;
    use std::sync::MutexGuard;

    pub static GLOBAL_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Acquire the shared test lock + reset every global counter / flag.
    ///
    /// § T11-D153 — poison resilience.  Originally this routine panicked on a
    /// poisoned mutex with the message `"crate-shared test lock poisoned ;
    /// prior test failed mid-update"`.  The intent was "fail loudly so the
    /// real bug is visible".  In practice the result was the OPPOSITE :
    /// **one** flake (e.g. a cold-cache scheduler-jitter that interleaved an
    /// unlocked arena_* test inside a locked test's critical section)
    /// poisoned the mutex, and **every subsequent locked test** in the same
    /// `cargo test` invocation panicked with PoisonError, manifesting as
    /// the 118/198-failure cascade documented in T11-D56.  The original
    /// failure was buried inside ~117 cascade panics.
    ///
    /// The fix : on a poisoned mutex we (a) clear the poison + (b) take the
    /// guard via `into_inner()` so the test-suite can continue.  The reset
    /// calls below restore all globals to a clean state regardless of how
    /// the prior test exited, so it's safe to proceed.  The original
    /// failure is still visible — it shows up as a single failed test
    /// rather than 118 cascade panics.  This works *with* the per-test
    /// `lock_and_reset_all()` discipline (now applied uniformly to all
    /// arena_* tests in `alloc.rs` , see T11-D153 commentary there) ; the
    /// poison-clearing is the **safety net** for an unlikely future
    /// regression, not the primary fix.
    pub fn lock_and_reset_all() -> MutexGuard<'static, ()> {
        // § Poison-tolerant acquisition (T11-D153).
        // `lock()` returns `Err(PoisonError)` if a previous holder panicked
        // while holding the guard.  `clear_poison()` resets the flag so
        // future acquisitions return `Ok` again ; `into_inner()` extracts
        // the guard from the PoisonError.  The reset calls below restore
        // every global counter to a clean state regardless of why the
        // prior test exited.
        let g = match GLOBAL_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                GLOBAL_TEST_LOCK.clear_poison();
                poisoned.into_inner()
            }
        };
        crate::alloc::reset_for_tests();
        crate::panic::reset_panic_count_for_tests();
        crate::exit::reset_exit_state_for_tests();
        crate::runtime::reset_runtime_for_tests();
        crate::io::reset_io_for_tests();
        crate::net::reset_net_for_tests();
        // § Wave-D host-FFI reset propagation INTENTIONALLY OMITTED here
        // (T11-D250 integration). Each W-D `host_*` module ships its own
        // MODULE-LOCAL test Mutex (e.g. `host_time::tests::
        // HOST_TIME_TEST_LOCK`) and `reset_*_for_tests` routine that
        // serializes its OWN counters. Adding a
        // `crate::host_*::reset_*_for_tests()` call here would nuke those
        // counters concurrently with tests holding the module-local lock
        // (a flake source confirmed empirically during D250 integration —
        // `host_time::tests::deadline_in_future_sleeps_and_returns_ok`
        // observed `total_sleep_ns == 0` mid-sleep when GLOBAL_TEST_LOCK
        // reset fired in another module's test). Wave-D modules don't
        // touch alloc/panic/exit/runtime/io/net globals, so the two-lock
        // regime is correct as-is. If a future cross-module reset becomes
        // necessary, redesign by having each module expose a single
        // `lock_and_reset_with_global` that acquires BOTH locks.
        g
    }
}

// ───────────────────────────────────────────────────────────────────────
// § crate-level tests — sanity checks on metadata + re-exports
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod crate_tests {
    use super::*;
    use crate::test_helpers::lock_and_reset_all;

    #[test]
    fn version_present() {
        // Pure : reads a const string ; no global state touched.
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present_and_canonical() {
        // Pure : const equality ; no global state touched.
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }

    #[test]
    fn re_exports_resolve() {
        // § T11-D153 : reads global counters ; while individual `load(Relaxed)`
        // is atomic, taking the lock makes this test deterministic w.r.t.
        // concurrent locked tests so the type-witness check is the only
        // observed effect rather than incidental state.
        let _g = lock_and_reset_all();
        // Compile-time check : these names must be reachable via `cssl_rt::`.
        let _: u64 = alloc_count();
        let _: u64 = free_count();
        let _: u64 = bytes_in_use();
        let _: u64 = panic_count();
        let _: u64 = exit_count();
        let _: u64 = abort_count();
        let _: i32 = last_exit_code();
        let _: u64 = init_count();
        let _: u64 = entry_invocation_count();
        let _: bool = is_runtime_initialized();
    }

    #[test]
    fn align_max_is_power_of_two() {
        // Pure : const arithmetic ; no global state touched.
        assert!(ALIGN_MAX.is_power_of_two());
        assert_eq!(ALIGN_MAX, 16);
    }

    #[test]
    fn bump_arena_constructible_via_top_level_re_export() {
        // § T11-D153 : `BumpArena::new(64)` calls `raw_alloc` →
        // `TRACKER.record_alloc` ; without the lock this incremented TRACKER
        // concurrently with locked alloc-count assertions in `alloc::tests`,
        // contributing to the cold-cache cascade documented in T11-D56.
        let _g = lock_and_reset_all();
        let arena = BumpArena::new(64);
        assert!(arena.is_some());
    }
}

