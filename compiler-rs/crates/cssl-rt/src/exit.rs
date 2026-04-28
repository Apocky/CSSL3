//! § cssl-rt exit + abort surface (T11-D52, S6-A1).
//!
//! § ROLE
//!   Stage-0 process-termination plumbing. CSSLv3-emitted code calls
//!   `__cssl_exit(code)` for clean shutdown with an exit code or
//!   `__cssl_abort()` for fatal-error termination.
//!
//! § DESIGN
//!   - The "real" implementations call [`std::process::exit`] /
//!     [`std::process::abort`] which never return.
//!   - For tests, we provide [`testable_exit`] / [`testable_abort`] that
//!     update internal counters and return `Result` rather than `!`.
//!     Tests verify the recorded code without terminating the runner.
//!   - The FFI symbols (in `crate::ffi`) call the real impls.
//!
//! § INVARIANTS
//!   - `cssl_exit_impl` flushes stdout/stderr before exit (best-effort).
//!   - `cssl_abort_impl` does NOT flush — abort means "drop everything".
//!   - Tracker counters are observable up to the exit call.

use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § exit + abort counters (test observation)
// ───────────────────────────────────────────────────────────────────────

static EXIT_COUNT: AtomicU64 = AtomicU64::new(0);
static ABORT_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_EXIT_CODE: AtomicI32 = AtomicI32::new(i32::MIN);

/// Number of `__cssl_exit` invocations observed since process start.
#[must_use]
pub fn exit_count() -> u64 {
    EXIT_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_abort` invocations observed since process start.
#[must_use]
pub fn abort_count() -> u64 {
    ABORT_COUNT.load(Ordering::Relaxed)
}

/// The most recent exit-code passed through [`testable_exit`] (or
/// [`cssl_exit_impl`] in non-`!` mode). `i32::MIN` ≡ never observed.
#[must_use]
pub fn last_exit_code() -> i32 {
    LAST_EXIT_CODE.load(Ordering::Relaxed)
}

/// Reset all exit + abort counters. Test-only.
pub fn reset_exit_state_for_tests() {
    EXIT_COUNT.store(0, Ordering::Relaxed);
    ABORT_COUNT.store(0, Ordering::Relaxed);
    LAST_EXIT_CODE.store(i32::MIN, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § record_* helpers : counter updates without termination
// ───────────────────────────────────────────────────────────────────────

/// Record an exit-code observation in counters. Does NOT terminate.
pub fn record_exit(code: i32) {
    LAST_EXIT_CODE.store(code, Ordering::Relaxed);
    EXIT_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Record an abort observation in counters. Does NOT terminate.
pub fn record_abort() {
    ABORT_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_exit_impl + cssl_abort_impl — real terminators (-> !)
// ───────────────────────────────────────────────────────────────────────

/// Stage-0 implementation of `__cssl_exit(code)`. Records the code, flushes
/// stdout/stderr (best-effort), then calls [`std::process::exit`].
pub fn cssl_exit_impl(code: i32) -> ! {
    use std::io::Write as _;
    record_exit(code);
    // best-effort flush ; ignore errors (terminating anyway)
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code);
}

/// Stage-0 implementation of `__cssl_abort()`. Records the abort, then
/// calls [`std::process::abort`].
pub fn cssl_abort_impl() -> ! {
    record_abort();
    std::process::abort();
}

// ───────────────────────────────────────────────────────────────────────
// § testable variants : record + return Result instead of `!`
// ───────────────────────────────────────────────────────────────────────

/// Test-only echo of `__cssl_exit` semantics : records the code, returns
/// `Ok(code)`. Does NOT terminate the process.
///
/// # Errors
/// Currently never returns `Err` ; the `Result` shape is reserved for
/// future cases (e.g., refusing to exit when in a test sandbox).
#[allow(clippy::unnecessary_wraps)]
pub fn testable_exit(code: i32) -> Result<i32, ExitError> {
    record_exit(code);
    Ok(code)
}

/// Test-only echo of `__cssl_abort` semantics : records the abort, returns
/// `Ok(())`. Does NOT terminate the process.
///
/// # Errors
/// Currently never returns `Err`.
#[allow(clippy::unnecessary_wraps)]
pub fn testable_abort() -> Result<(), ExitError> {
    record_abort();
    Ok(())
}

/// Reserved error type for [`testable_exit`] / [`testable_abort`].
#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct ExitError;

impl std::fmt::Display for ExitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cssl-rt exit-error")
    }
}

impl std::error::Error for ExitError {}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::lock_and_reset_all as lock_and_reset;

    #[test]
    fn counters_start_at_initial_state_after_reset() {
        let _g = lock_and_reset();
        assert_eq!(exit_count(), 0);
        assert_eq!(abort_count(), 0);
        assert_eq!(last_exit_code(), i32::MIN);
    }

    #[test]
    fn record_exit_updates_counter_and_code() {
        let _g = lock_and_reset();
        record_exit(0);
        assert_eq!(exit_count(), 1);
        assert_eq!(last_exit_code(), 0);
    }

    #[test]
    fn record_exit_overwrites_last_code() {
        let _g = lock_and_reset();
        record_exit(0);
        record_exit(42);
        assert_eq!(exit_count(), 2);
        assert_eq!(last_exit_code(), 42);
    }

    #[test]
    fn record_abort_updates_counter() {
        let _g = lock_and_reset();
        record_abort();
        assert_eq!(abort_count(), 1);
    }

    #[test]
    fn record_abort_does_not_touch_exit_state() {
        let _g = lock_and_reset();
        record_abort();
        record_abort();
        assert_eq!(abort_count(), 2);
        assert_eq!(exit_count(), 0);
        assert_eq!(last_exit_code(), i32::MIN);
    }

    #[test]
    fn testable_exit_returns_code() {
        let _g = lock_and_reset();
        let r = testable_exit(7).expect("exit ok");
        assert_eq!(r, 7);
        assert_eq!(last_exit_code(), 7);
        assert_eq!(exit_count(), 1);
    }

    #[test]
    fn testable_exit_negative_code() {
        let _g = lock_and_reset();
        let r = testable_exit(-1).expect("exit ok");
        assert_eq!(r, -1);
        assert_eq!(last_exit_code(), -1);
    }

    #[test]
    fn testable_abort_returns_unit() {
        let _g = lock_and_reset();
        let r = testable_abort();
        assert!(r.is_ok());
        assert_eq!(abort_count(), 1);
    }

    #[test]
    fn exit_then_abort_records_both() {
        let _g = lock_and_reset();
        let _ = testable_exit(99);
        let _ = testable_abort();
        assert_eq!(exit_count(), 1);
        assert_eq!(abort_count(), 1);
        assert_eq!(last_exit_code(), 99);
    }

    #[test]
    fn exit_error_implements_std_error() {
        let _g = lock_and_reset();
        let e = ExitError;
        let s = format!("{e}");
        assert!(s.contains("cssl-rt"));
        // explicit dyn coercion ← compiles iff Error is impl'd
        let _: &dyn std::error::Error = &e;
    }

    #[test]
    fn exit_code_extremes_round_trip() {
        let _g = lock_and_reset();
        let _ = testable_exit(i32::MAX);
        assert_eq!(last_exit_code(), i32::MAX);
        let _ = testable_exit(i32::MIN + 1); // MIN reserved as "never observed"
        assert_eq!(last_exit_code(), i32::MIN + 1);
    }

    #[test]
    fn many_exits_in_loop_increment_count() {
        let _g = lock_and_reset();
        for code in 0..20 {
            let _ = testable_exit(code);
        }
        assert_eq!(exit_count(), 20);
        assert_eq!(last_exit_code(), 19);
    }
}
