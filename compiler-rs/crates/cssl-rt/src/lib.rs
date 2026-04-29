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
pub mod exit;
pub mod ffi;
pub mod io;
#[cfg(not(target_os = "windows"))]
pub mod io_unix;
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
pub use exit::{
    abort_count, cssl_abort_impl, cssl_exit_impl, exit_count, last_exit_code, record_abort,
    record_exit, reset_exit_state_for_tests, testable_abort, testable_exit, ExitError,
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
