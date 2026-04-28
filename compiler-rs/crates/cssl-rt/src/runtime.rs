//! § cssl-rt entry shim — `__cssl_entry` (T11-D52, S6-A1).
//!
//! § ROLE
//!   Every CSSLv3 executable's process entry-point eventually delegates to
//!   `__cssl_entry`, which:
//!     1. Initializes the runtime (TLS slot, telemetry-ring, panic-hook),
//!     2. Invokes the user `main` function,
//!     3. Tears down (best-effort) and returns the user's exit-code.
//!
//! § DESIGN
//!   `__cssl_entry` accepts a function-pointer-style "user main" so the
//!   FFI surface is concrete and JIT-link-friendly. For a compiled-from-
//!   source CSSLv3 program, `csslc` (S6-A2) emits a thin C `main` that
//!   calls `__cssl_entry(__cssl_user_main)` ; `__cssl_user_main` is the
//!   mangled CSSLv3 `main()` symbol.
//!
//! § INVARIANTS
//!   - `cssl_entry_impl` initializes the runtime exactly once per process
//!     (subsequent calls re-enter the user fn but skip init).
//!   - On user-main panic (via `__cssl_panic` FFI), `__cssl_abort` runs
//!     before `__cssl_entry` returns. The `cssl_entry_impl` testable
//!     variant catches `Result` instead of unwinding.
//!   - The init phase records [`init_count`] increments observable from
//!     tests.
//!
//! § FUTURE (deferred to phase-B+)
//!   - Real TLS-key creation (`pthread_key_create` / `TlsAlloc`).
//!   - Telemetry-ring instantiation (R18 evidence).
//!   - User-installable panic-hook (currently the panic FFI is fixed).
//!   - Argc / argv plumbing — stage-0 user main has signature `() -> i32`.

// § T11-D52 (S6-A1) : the FFI-shape entry shim takes a raw fn-pointer ;
// file-level `unsafe_code` allow per cssl-cgen-cpu-cranelift convention.
#![allow(unsafe_code)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § init state
// ───────────────────────────────────────────────────────────────────────

static RUNTIME_INITIALIZED: AtomicBool = AtomicBool::new(false);
static INIT_COUNT: AtomicU64 = AtomicU64::new(0);
static ENTRY_INVOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

/// True iff [`cssl_entry_impl`] has run its init phase at least once
/// in this process.
#[must_use]
pub fn is_runtime_initialized() -> bool {
    RUNTIME_INITIALIZED.load(Ordering::Relaxed)
}

/// Number of init phases executed since process start. ≤ `entry_invocation_count`.
#[must_use]
pub fn init_count() -> u64 {
    INIT_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_entry` invocations observed since process start.
#[must_use]
pub fn entry_invocation_count() -> u64 {
    ENTRY_INVOCATION_COUNT.load(Ordering::Relaxed)
}

/// Reset all runtime state. Test-only.
pub fn reset_runtime_for_tests() {
    RUNTIME_INITIALIZED.store(false, Ordering::Relaxed);
    INIT_COUNT.store(0, Ordering::Relaxed);
    ENTRY_INVOCATION_COUNT.store(0, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § init / teardown
// ───────────────────────────────────────────────────────────────────────

/// Run runtime initialization. Idempotent (safe to call multiple times).
fn init_runtime() {
    if RUNTIME_INITIALIZED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        // first-time init phase ; stage-0 stubs the heavy work
        INIT_COUNT.fetch_add(1, Ordering::Relaxed);
        // future : telemetry-ring instantiation
        // future : TLS-key creation
        // future : panic-hook installation beyond the FFI default
    }
}

/// Best-effort teardown ; stage-0 has no resources to release.
fn teardown_runtime() {
    // future : telemetry-ring flush, TLS-key delete, profile-report emission
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_entry_impl — Rust-side, generic over user-main shape
// ───────────────────────────────────────────────────────────────────────

/// Stage-0 entry-shim — runs init, invokes `user_main`, runs teardown.
///
/// § GENERIC
///   `F: FnOnce() -> i32` so tests can pass a closure capturing test state.
///   The FFI surface ([`crate::ffi::__cssl_entry`]) accepts an
///   `extern "C" fn() -> i32` pointer ; the closure-form is a thin wrapper.
pub fn cssl_entry_impl<F: FnOnce() -> i32>(user_main: F) -> i32 {
    ENTRY_INVOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    init_runtime();
    let exit_code = user_main();
    teardown_runtime();
    exit_code
}

/// FFI-shape variant of [`cssl_entry_impl`] taking a raw fn-ptr.
///
/// # Safety
/// Caller must ensure `user_main` is a valid fn-pointer with `extern "C"`
/// ABI returning `i32`. Calling a stale or null pointer is UB.
pub unsafe fn cssl_entry_impl_extern(user_main: extern "C" fn() -> i32) -> i32 {
    // closure-wrap is required : `extern "C" fn() -> i32` does not auto-coerce
    // to `FnOnce() -> i32` (Rust ABI vs C ABI distinction). The closure
    // adapts the FFI fn-ptr to the generic `cssl_entry_impl` interface.
    #[allow(clippy::redundant_closure)]
    cssl_entry_impl(|| user_main())
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::lock_and_reset_all as lock_and_reset;

    extern "C" fn user_main_returns_7() -> i32 {
        7
    }

    #[test]
    fn entry_with_zero_returning_main() {
        let _g = lock_and_reset();
        let code = cssl_entry_impl(|| 0);
        assert_eq!(code, 0);
        assert_eq!(entry_invocation_count(), 1);
        assert!(is_runtime_initialized());
    }

    #[test]
    fn entry_with_42_returning_main() {
        let _g = lock_and_reset();
        let code = cssl_entry_impl(|| 42);
        assert_eq!(code, 42);
    }

    #[test]
    fn entry_with_negative_returning_main() {
        let _g = lock_and_reset();
        let code = cssl_entry_impl(|| -1);
        assert_eq!(code, -1);
    }

    #[test]
    fn entry_runs_init_exactly_once_across_calls() {
        let _g = lock_and_reset();
        let _ = cssl_entry_impl(|| 0);
        let _ = cssl_entry_impl(|| 0);
        let _ = cssl_entry_impl(|| 0);
        assert_eq!(init_count(), 1, "init should be idempotent");
        assert_eq!(entry_invocation_count(), 3);
    }

    #[test]
    fn init_count_increments_only_on_first_entry() {
        let _g = lock_and_reset();
        assert_eq!(init_count(), 0);
        let _ = cssl_entry_impl(|| 0);
        assert_eq!(init_count(), 1);
        let _ = cssl_entry_impl(|| 0);
        assert_eq!(init_count(), 1); // still 1
    }

    #[test]
    fn entry_invocation_count_grows_per_call() {
        let _g = lock_and_reset();
        for n in 1..=5 {
            let _ = cssl_entry_impl(|| 0);
            assert_eq!(entry_invocation_count(), n);
        }
    }

    #[test]
    fn entry_propagates_user_main_return_value() {
        let _g = lock_and_reset();
        for code in [0, 1, 42, -1, i32::MAX, i32::MIN] {
            reset_runtime_for_tests();
            assert_eq!(cssl_entry_impl(|| code), code);
        }
    }

    #[test]
    fn user_main_observes_initialized_runtime() {
        let _g = lock_and_reset();
        let observed = cssl_entry_impl(|| i32::from(is_runtime_initialized()));
        assert_eq!(observed, 1, "user_main should see init complete");
    }

    #[test]
    fn entry_with_extern_fn_form() {
        let _g = lock_and_reset();
        let code = unsafe { cssl_entry_impl_extern(user_main_returns_7) };
        assert_eq!(code, 7);
    }

    #[test]
    fn reset_runtime_clears_initialized_flag() {
        let _g = lock_and_reset();
        let _ = cssl_entry_impl(|| 0);
        assert!(is_runtime_initialized());
        reset_runtime_for_tests();
        assert!(!is_runtime_initialized());
        assert_eq!(init_count(), 0);
        assert_eq!(entry_invocation_count(), 0);
    }

    #[test]
    fn capturing_closure_from_user_main_works() {
        let _g = lock_and_reset();
        let mut sentinel = 0_i32;
        let code = cssl_entry_impl(|| {
            sentinel = 99;
            sentinel
        });
        assert_eq!(code, 99);
        assert_eq!(sentinel, 99);
    }
}
