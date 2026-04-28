//! § cssl-rt — CSSLv3 stage-0 runtime library
//! ═══════════════════════════════════════════
//!
//! Authoritative spec : `specs/01_BOOTSTRAP.csl § RUNTIME-LIB`
//!                    + `specs/22_TELEMETRY.csl § RING-INTEGRATION` (deferred).
//!
//! § ROLE
//!   Linked into every CSSLv3 artifact. Provides:
//!     - allocator + tracker  (`alloc` module)
//!     - panic + format       (`panic` module)
//!     - exit + abort         (`exit` module)
//!     - entry shim           (`runtime` module)
//!     - stable FFI surface   (`ffi` module)
//!
//! § FFI SURFACE  (ABI-stable from S6-A1 forward)
//!   ```text
//!   __cssl_entry(user_main: extern "C" fn() -> i32) -> i32
//!   __cssl_alloc(size, align) -> *mut u8
//!   __cssl_free(ptr, size, align)
//!   __cssl_realloc(ptr, old_size, new_size, align) -> *mut u8
//!   __cssl_panic(msg, msg_len, file, file_len, line) -> !
//!   __cssl_abort() -> !
//!   __cssl_exit(code: i32) -> !
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
pub mod panic;
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
pub use panic::{format_panic, panic_count, record_panic, reset_panic_count_for_tests};
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
    /// Panics on poisoned-lock (test failure earlier left state corrupt).
    /// In practice, each test follows lock-and-reset → run → drop pattern,
    /// so poisoning indicates a real bug in a prior test.
    pub fn lock_and_reset_all() -> MutexGuard<'static, ()> {
        let g = GLOBAL_TEST_LOCK
            .lock()
            .expect("crate-shared test lock poisoned ; prior test failed mid-update");
        crate::alloc::reset_for_tests();
        crate::panic::reset_panic_count_for_tests();
        crate::exit::reset_exit_state_for_tests();
        crate::runtime::reset_runtime_for_tests();
        g
    }
}

// ───────────────────────────────────────────────────────────────────────
// § crate-level tests — sanity checks on metadata + re-exports
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod crate_tests {
    use super::*;

    #[test]
    fn version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present_and_canonical() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }

    #[test]
    fn re_exports_resolve() {
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
        assert!(ALIGN_MAX.is_power_of_two());
        assert_eq!(ALIGN_MAX, 16);
    }

    #[test]
    fn bump_arena_constructible_via_top_level_re_export() {
        let arena = BumpArena::new(64);
        assert!(arena.is_some());
    }
}
