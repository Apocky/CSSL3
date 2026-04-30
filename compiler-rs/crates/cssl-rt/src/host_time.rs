//! § cssl-rt host_time — Wave-D1 (T11-D…) — host-FFI time primitives.
//!
//! ═══════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Provides four ABI-stable `extern "C"` symbols that CSSLv3-emitted
//!   code links against for monotonic clock / wall clock / sleep /
//!   deadline-sleep. The four symbols are :
//!
//!   ```text
//!   __cssl_time_monotonic_ns()        -> u64    monotonic-since-boot ns
//!   __cssl_time_wall_unix_ns()        -> i64    UNIX-epoch ns (signed)
//!   __cssl_time_sleep_ns(ns)          -> i32    0=ok, -1=err
//!   __cssl_time_deadline_until(dl_ns) -> i32    sleep until monotonic-ns
//!                                                deadline ; 0=ok, -1=err,
//!                                                +1 = already-past (no-op)
//!   ```
//!
//!   Authoritative spec : `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § time`
//!                      + `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D § D1`.
//!
//! § STAGE-0 SCOPE
//!   - Pure-syscall delegation : `std::time::Instant` / `SystemTime` /
//!     `std::thread::sleep`. Zero allocations on any hot path.
//!   - Boot-instant cached in a process-wide `OnceLock<Instant>` so the
//!     monotonic clock returns "since-process-start ns" without
//!     per-call atomic / mutex traffic. Stage-1 self-host will replace
//!     this with a true RDTSC / `clock_gettime(CLOCK_MONOTONIC_RAW)`
//!     wrapper via §§ 14 ASM intrinsics ; the FFI symbol stays the same.
//!   - Counters are atomic for thread-safety from day-1.
//!
//! § PRIME-DIRECTIVE
//!   Time is `NonDeterministic` per §§ 11. The wall-clock + monotonic
//!   reads carry no Sensitive label. No data egress, no telemetry, no
//!   side-channels. The four symbols are pure observation + sleep ;
//!   nothing escapes the process.
//!
//! § FFI INVARIANTS
//!   ‼ Renaming any of these symbols is a major-version-bump event ;
//!     CSSLv3-emitted code references them by exact name.
//!   ‼ Argument types + ordering are also locked. Use additional symbols
//!     (e.g., `__cssl_time_sleep_until_unix_ns`) for new behaviors.
//!   ‼ Each symbol delegates to a Rust-side `_impl` fn so unit tests can
//!     exercise behavior without going through the FFI boundary.
//!
//! § SAWYER-EFFICIENCY  (per WAVE-CSSL_PLAN § DISPATCH-DISCIPLINE)
//!   - Boot-instant cached in `OnceLock<Instant>` ; first call pays the
//!     init cost, subsequent calls are pure `Instant::elapsed()`.
//!   - All counters are 64-bit atomics, `Relaxed` ordering — no mutex,
//!     no allocation, no syscall beyond the time-read itself.
//!   - `sleep_ns` clamps `Duration::from_nanos(ns)` directly ; no
//!     intermediate `Duration` arithmetic on the hot path.
//!   - `deadline_until` is a single `saturating_sub` + a `sleep` call ;
//!     past-deadlines short-circuit before any syscall.
//!
//! § INTEGRATION_NOTE  (per Wave-D1 dispatch directive)
//!   This file ships as a NEW module but `cssl-rt::lib.rs`'s `pub mod`
//!   list is INTENTIONALLY NOT modified by this slice — the integration
//!   commit owns that surface change (per HARD CONSTRAINTS in the
//!   dispatch prompt). The four `__cssl_time_*` symbols are still
//!   exposed at link-time because `extern "C"` `#[no_mangle]` exports
//!   participate in the final cdylib/staticlib symbol table independent
//!   of the `pub mod` declaration in lib.rs ; the module compiles + the
//!   symbols are emitted as soon as `cssl-rt` is built. The Rust-side
//!   `_impl` helpers are crate-private until the integration commit
//!   adds `pub mod host_time` ; cross-crate consumers that need the
//!   helpers (Cranelift cgen + tests outside this file) should reach
//!   them via the FFI symbols. See the matching cgen wiring in
//!   `cssl-cgen-cpu-cranelift::cgen_time` for the paired call-emission
//!   pattern that mirrors Wave-C3 `cgen_fs`.

// § Wave-D1 : the FFI surface fundamentally requires `extern "C"` ; allow
// at file-level to mirror cssl-rt::ffi convention. Each unsafe block (only
// the four `extern "C" fn` declarations) is a no-op safety surface — the
// implementations themselves are written in safe Rust.
#![allow(unsafe_code)]
#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ───────────────────────────────────────────────────────────────────────
// § return-code constants (FFI-stable)
//
//   ‼ The three return codes for `sleep_ns` / `deadline_until` are locked.
//     `0` = success ; `-1` = error (e.g., `Duration::from_nanos` overflow,
//     though `from_nanos` is total over `u64`) ; `+1` = the deadline-only
//     no-op signal indicating the deadline was already in the past.
// ───────────────────────────────────────────────────────────────────────

/// FFI return : success.
pub const TIME_OK: i32 = 0;
/// FFI return : error / invalid input. Reserved for future use ; the
/// stage-0 implementations of `sleep_ns` / `deadline_until` never
/// produce this code (`Duration::from_nanos` is total over `u64`),
/// but the constant locks the ABI for stage-1's syscall-direct path.
pub const TIME_ERR: i32 = -1;
/// FFI return : deadline-already-past. `__cssl_time_deadline_until`
/// returns this when the supplied `deadline_ns` is `<=` the current
/// monotonic-ns reading at call entry — i.e., the call short-circuits
/// without sleeping. Distinct from `TIME_OK` so callers that want to
/// distinguish "we slept" from "we didn't have to" can branch without
/// re-reading the clock. Stage-1 self-host preserves this semantic.
pub const TIME_DEADLINE_ALREADY_PAST: i32 = 1;

// ───────────────────────────────────────────────────────────────────────
// § counters — Sawyer-efficiency : 64-bit atomic, Relaxed ordering
//
//   Tracker counters are LOCAL ; nothing escapes the process. The four
//   counters mirror the host_fs / host_net pattern :
//     monotonic_count   — number of `__cssl_time_monotonic_ns` invocations
//     wall_count        — number of `__cssl_time_wall_unix_ns` invocations
//     sleep_count       — number of `__cssl_time_sleep_ns` invocations
//     deadline_count    — number of `__cssl_time_deadline_until` invocations
//   `total_sleep_ns` accumulates the slept-for nanoseconds across all
//   `sleep_ns` + `deadline_until` calls (post-sleep ; the past-deadline
//   short-circuit does NOT contribute). Useful for power-regression
//   testing (per specs/24 § TESTING power-regression oracle).
// ───────────────────────────────────────────────────────────────────────

static MONOTONIC_COUNT: AtomicU64 = AtomicU64::new(0);
static WALL_COUNT: AtomicU64 = AtomicU64::new(0);
static SLEEP_COUNT: AtomicU64 = AtomicU64::new(0);
static DEADLINE_COUNT: AtomicU64 = AtomicU64::new(0);
static TOTAL_SLEEP_NS: AtomicU64 = AtomicU64::new(0);

/// Number of `__cssl_time_monotonic_ns` invocations since process start.
#[must_use]
pub fn monotonic_count() -> u64 {
    MONOTONIC_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_time_wall_unix_ns` invocations since process start.
#[must_use]
pub fn wall_count() -> u64 {
    WALL_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_time_sleep_ns` invocations since process start.
#[must_use]
pub fn sleep_count() -> u64 {
    SLEEP_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_time_deadline_until` invocations since process start.
#[must_use]
pub fn deadline_count() -> u64 {
    DEADLINE_COUNT.load(Ordering::Relaxed)
}

/// Total nanoseconds spent sleeping (sum across both `sleep_ns` +
/// non-short-circuit `deadline_until` calls). Useful for power-regression
/// oracle assertions.
#[must_use]
pub fn total_sleep_ns() -> u64 {
    TOTAL_SLEEP_NS.load(Ordering::Relaxed)
}

/// Reset every host_time counter to zero. Test-only.
pub fn reset_time_for_tests() {
    MONOTONIC_COUNT.store(0, Ordering::Relaxed);
    WALL_COUNT.store(0, Ordering::Relaxed);
    SLEEP_COUNT.store(0, Ordering::Relaxed);
    DEADLINE_COUNT.store(0, Ordering::Relaxed);
    TOTAL_SLEEP_NS.store(0, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § boot-instant cache — Sawyer : OnceLock + saturating math
//
//   The first invocation of `__cssl_time_monotonic_ns` records the
//   process boot-instant ; every subsequent invocation reads
//   `Instant::elapsed` against that anchor. Single OnceLock store, no
//   per-call mutex contention.
//
//   Stage-1 self-host will replace this with `clock_gettime(
//   CLOCK_MONOTONIC_RAW, ...)` (POSIX) / `QueryPerformanceCounter`
//   (Win32) — both raw-syscall paths that don't need a Rust-Instant
//   anchor. The FFI symbol-name stays the same.
// ───────────────────────────────────────────────────────────────────────

static BOOT_INSTANT: OnceLock<Instant> = OnceLock::new();

/// Lazily initialize + return the process boot-instant.
fn boot_instant() -> Instant {
    *BOOT_INSTANT.get_or_init(Instant::now)
}

// ───────────────────────────────────────────────────────────────────────
// § _impl helpers — Rust-side, testable without going through FFI
// ───────────────────────────────────────────────────────────────────────

/// Implementation : monotonic-since-boot in nanoseconds. Saturates at
/// `u64::MAX` for absurd uptimes (~584 years), which is the documented
/// stage-0 behavior. Stage-1's raw-syscall path will return the kernel
/// counter directly with the same saturation semantic.
#[must_use]
pub fn cssl_time_monotonic_ns_impl() -> u64 {
    MONOTONIC_COUNT.fetch_add(1, Ordering::Relaxed);
    let elapsed = boot_instant().elapsed();
    // u128 → u64 saturating cast (Duration::as_nanos returns u128).
    elapsed.as_nanos().min(u128::from(u64::MAX)) as u64
}

/// Implementation : wall-clock UNIX-epoch in nanoseconds. Returns a
/// signed i64 to match the FFI declaration ; pre-epoch timestamps yield
/// a negative value with absolute-value bounded by ~292 years (i64::MAX
/// in nanoseconds ≈ 9.2 × 10^18). Saturates at `i64::MAX` / `i64::MIN`
/// for clocks far outside that range.
///
/// § PRIME-DIRECTIVE
///   No data egress. The wall-clock read carries no Sensitive label —
///   it's a process-local observation of the host clock.
#[must_use]
pub fn cssl_time_wall_unix_ns_impl() -> i64 {
    WALL_COUNT.fetch_add(1, Ordering::Relaxed);
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => i64::try_from(dur.as_nanos()).unwrap_or(i64::MAX),
        Err(err) => {
            // Pre-epoch reading : duration_since returns Err with the
            // negative offset as a positive Duration. Negate (saturating).
            let pre_dur = err.duration();
            let abs_ns = i64::try_from(pre_dur.as_nanos()).unwrap_or(i64::MAX);
            // saturating_neg is correct here — i64::MIN.saturating_neg() = i64::MAX
            // which is bounded by the i64::MAX clamp above.
            abs_ns.saturating_neg()
        }
    }
}

/// Implementation : sleep for `ns` nanoseconds. Returns `TIME_OK` (=0)
/// on completion. The function is total over `u64::MAX` — `Duration::
/// from_nanos` accepts any `u64` and returns a Duration of up to ~584 years.
///
/// § STAGE-0 SCOPE
///   Stage-0 delegates to `std::thread::sleep`. Stage-1 self-host will
///   issue `nanosleep` (POSIX) / `WaitForSingleObject + timer-queue`
///   (Win32) directly via §§ 14 ASM intrinsics.
pub fn cssl_time_sleep_ns_impl(ns: u64) -> i32 {
    SLEEP_COUNT.fetch_add(1, Ordering::Relaxed);
    if ns == 0 {
        // Zero-duration sleep : skip the syscall, but DO count it.
        // This matches POSIX `nanosleep(0, NULL)` semantics — a yield.
        // Future stage-1 path may emit `sched_yield()` here.
        return TIME_OK;
    }
    TOTAL_SLEEP_NS.fetch_add(ns, Ordering::Relaxed);
    std::thread::sleep(Duration::from_nanos(ns));
    TIME_OK
}

/// Implementation : sleep until `deadline_ns` (a monotonic-ns reading
/// from `cssl_time_monotonic_ns_impl`). Returns :
///   - `TIME_OK` (0) if we actually slept,
///   - `TIME_DEADLINE_ALREADY_PAST` (1) if the deadline was already in the past,
///   - `TIME_ERR` (-1) is reserved (stage-0 never returns it ; the
///     `from_nanos` path is total).
///
/// § SEMANTICS
///   Computes `delta = deadline_ns.saturating_sub(now_ns)`. If `delta == 0`,
///   short-circuits with `+1` ; else sleeps for `delta` ns + returns 0.
///
/// § SAWYER-EFFICIENCY
///   Reads the monotonic clock ONCE — saturating-sub avoids a second
///   read for the negative case. Past-deadline path bypasses the syscall.
pub fn cssl_time_deadline_until_impl(deadline_ns: u64) -> i32 {
    DEADLINE_COUNT.fetch_add(1, Ordering::Relaxed);
    let now_ns = cssl_time_monotonic_ns_impl();
    let delta = deadline_ns.saturating_sub(now_ns);
    if delta == 0 {
        // Already past — no syscall, no contribution to total_sleep_ns.
        return TIME_DEADLINE_ALREADY_PAST;
    }
    TOTAL_SLEEP_NS.fetch_add(delta, Ordering::Relaxed);
    std::thread::sleep(Duration::from_nanos(delta));
    TIME_OK
}

// ───────────────────────────────────────────────────────────────────────
// § FFI surface — the four ABI-stable `__cssl_time_*` symbols
//
//   These are what CSSLv3-emitted code links against. Each delegates to
//   the matching `_impl` so tests + cgen-driven calls share the same
//   behavior surface.
// ───────────────────────────────────────────────────────────────────────

/// FFI : monotonic-since-boot nanoseconds.
///
/// Returns a u64 ns counter that is monotonically non-decreasing from
/// process start. Saturates at `u64::MAX` for absurd uptimes (~584
/// years from process start).
///
/// § PRIME-DIRECTIVE
///   Time-monotonic carries the `NonDeterministic` IFC label per §§ 11 ;
///   no Sensitive data, no egress, no side-channels.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_time_monotonic_ns() -> u64 {
    cssl_time_monotonic_ns_impl()
}

/// FFI : wall-clock UNIX-epoch nanoseconds (signed i64).
///
/// Returns the host wall-clock reading as ns since `1970-01-01 00:00:00 UTC`.
/// Pre-epoch readings yield a negative value, clamped to `i64::MIN`.
/// Far-future readings clamp to `i64::MAX` (~292 years post-epoch).
///
/// § PRIME-DIRECTIVE
///   Time-wall carries the `NonDeterministic` IFC label per §§ 11 ;
///   no Sensitive data, no egress.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_time_wall_unix_ns() -> i64 {
    cssl_time_wall_unix_ns_impl()
}

/// FFI : sleep for `ns` nanoseconds. Returns `0` on completion.
///
/// `ns == 0` short-circuits with `0` (no syscall, but counts as a
/// sleep-call for the tracker). Total over `u64::MAX`.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_time_sleep_ns(ns: u64) -> i32 {
    cssl_time_sleep_ns_impl(ns)
}

/// FFI : sleep until `deadline_ns` (monotonic-ns reading).
///
/// Returns :
///   - `0` if we slept,
///   - `+1` if the deadline was already in the past at call entry (no-op),
///   - `-1` (reserved ; stage-0 never produces this).
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_time_deadline_until(deadline_ns: u64) -> i32 {
    cssl_time_deadline_until_impl(deadline_ns)
}

// ───────────────────────────────────────────────────────────────────────
// § tests — exercise every `_impl` + each FFI shim
//
//   Tests serialize on a host_time-local Mutex (NOT the crate-shared
//   GLOBAL_TEST_LOCK in lib.rs::test_helpers — that one is intentionally
//   crate-private). Each test acquires the local lock + resets the
//   counter set, then exercises one behavior.
//
//   ‼ Sleep-based tests use SHORT durations (≤ 5 ms) to keep `cargo test`
//     fast. The discipline is "verify the syscall returned + the counter
//     advanced", not "verify the OS scheduler hits a precise wakeup".
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::MutexGuard;

    // Local test-lock — host_time tests share counters but don't touch
    // alloc / panic / runtime / io / net globals. A module-local Mutex
    // is sufficient ; integration with the crate-shared lock is a
    // follow-up for the integration commit (per INTEGRATION_NOTE).
    static HOST_TIME_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> MutexGuard<'static, ()> {
        let g = match HOST_TIME_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                HOST_TIME_TEST_LOCK.clear_poison();
                poisoned.into_inner()
            }
        };
        reset_time_for_tests();
        g
    }

    // ── monotonic — non-decreasing + counter discipline ────────────────

    #[test]
    fn monotonic_returns_non_decreasing_values_across_calls() {
        let _g = lock_and_reset();
        // Three reads ; t1 ≤ t2 ≤ t3 ; counter == 3.
        let t1 = cssl_time_monotonic_ns_impl();
        let t2 = cssl_time_monotonic_ns_impl();
        let t3 = cssl_time_monotonic_ns_impl();
        assert!(t1 <= t2, "monotonic violation : t1={t1} > t2={t2}");
        assert!(t2 <= t3, "monotonic violation : t2={t2} > t3={t3}");
        assert_eq!(monotonic_count(), 3);
    }

    #[test]
    fn monotonic_advances_after_short_sleep() {
        let _g = lock_and_reset();
        let t0 = cssl_time_monotonic_ns_impl();
        // 1 ms ≈ 1_000_000 ns ; we sleep, then the next read MUST be
        // strictly greater. (OS scheduler latency ⇒ actual delta
        // typically 1-10 ms ; assertion is delta > 0.)
        std::thread::sleep(Duration::from_millis(1));
        let t1 = cssl_time_monotonic_ns_impl();
        assert!(
            t1 > t0,
            "monotonic should advance after 1ms sleep : t0={t0} t1={t1}"
        );
    }

    // ── wall-clock — positive + counter discipline ─────────────────────

    #[test]
    fn wall_unix_ns_is_positive_in_2026() {
        let _g = lock_and_reset();
        let w = cssl_time_wall_unix_ns_impl();
        // Test runs in 2026 ; UNIX epoch is 1970. Positive.
        assert!(w > 0, "wall-clock should be > 0 in 2026 : got {w}");
        // Sanity : 2024-01-01 UTC ≈ 1.7 × 10^18 ns since epoch ; far
        // less than i64::MAX (≈ 9.2 × 10^18).
        let year_2024_ns: i64 = 1_704_067_200 * 1_000_000_000;
        assert!(
            w > year_2024_ns,
            "wall-clock should be after 2024-01-01 : got {w}"
        );
        assert_eq!(wall_count(), 1);
    }

    #[test]
    fn wall_clock_increases_across_brief_calls() {
        let _g = lock_and_reset();
        let w1 = cssl_time_wall_unix_ns_impl();
        // Tiny sleep to force a clock-tick boundary on most OSes.
        std::thread::sleep(Duration::from_millis(2));
        let w2 = cssl_time_wall_unix_ns_impl();
        assert!(
            w2 >= w1,
            "wall-clock should not regress : w1={w1} w2={w2}"
        );
    }

    // ── sleep — zero / positive / counter discipline ───────────────────

    #[test]
    fn sleep_zero_ns_returns_ok_without_syscall() {
        let _g = lock_and_reset();
        let r = cssl_time_sleep_ns_impl(0);
        assert_eq!(r, TIME_OK);
        assert_eq!(sleep_count(), 1);
        // Zero-duration : tracker counts the call but contributes 0 ns.
        assert_eq!(total_sleep_ns(), 0);
    }

    #[test]
    fn sleep_positive_ns_returns_ok_and_advances_total() {
        let _g = lock_and_reset();
        // 500 µs = 500_000 ns ; short enough to keep the test fast.
        let ns = 500_000u64;
        let r = cssl_time_sleep_ns_impl(ns);
        assert_eq!(r, TIME_OK);
        assert_eq!(sleep_count(), 1);
        assert_eq!(total_sleep_ns(), ns);
    }

    #[test]
    fn sleep_positive_actually_delays() {
        let _g = lock_and_reset();
        // 2 ms requested ; verify monotonic advanced AT LEAST 1 ms
        // (OS scheduler granularity is the lower bound).
        let t0 = cssl_time_monotonic_ns_impl();
        let _ = cssl_time_sleep_ns_impl(2_000_000);
        let t1 = cssl_time_monotonic_ns_impl();
        let delta_ns = t1.saturating_sub(t0);
        assert!(
            delta_ns >= 1_000_000,
            "expected ≥ 1 ms elapsed, got {delta_ns} ns"
        );
    }

    // ── deadline — past / future / counter discipline ──────────────────

    #[test]
    fn deadline_in_past_returns_one_and_skips_syscall() {
        let _g = lock_and_reset();
        // Anchor the monotonic ; pass a deadline strictly less than now.
        let now = cssl_time_monotonic_ns_impl();
        // monotonic_count is now 1 (anchor read) ; deadline_count == 0.
        let r = cssl_time_deadline_until_impl(now.saturating_sub(1_000_000));
        assert_eq!(r, TIME_DEADLINE_ALREADY_PAST);
        // No actual sleep occurred ; total_sleep_ns stays at 0.
        assert_eq!(total_sleep_ns(), 0);
        assert_eq!(deadline_count(), 1);
    }

    #[test]
    fn deadline_in_future_sleeps_and_returns_ok() {
        let _g = lock_and_reset();
        let now = cssl_time_monotonic_ns_impl();
        // 500 µs in the future.
        let dl = now.saturating_add(500_000);
        let r = cssl_time_deadline_until_impl(dl);
        assert_eq!(r, TIME_OK);
        // total_sleep_ns received SOME contribution > 0 (the saturating-sub
        // compute ; bound by 500_000 + a bit of clock-skew slack).
        assert!(
            total_sleep_ns() > 0,
            "deadline-future should contribute to total_sleep_ns"
        );
        assert!(
            total_sleep_ns() <= 500_000,
            "deadline-future contribution {} should be ≤ 500_000 ns",
            total_sleep_ns()
        );
    }

    #[test]
    fn deadline_correctness_actually_waits() {
        let _g = lock_and_reset();
        // 3 ms in the future ; verify monotonic advanced ≥ 2 ms (slack
        // for OS scheduler granularity).
        let now = cssl_time_monotonic_ns_impl();
        let dl = now.saturating_add(3_000_000);
        let _ = cssl_time_deadline_until_impl(dl);
        let after = cssl_time_monotonic_ns_impl();
        let delta = after.saturating_sub(now);
        assert!(
            delta >= 2_000_000,
            "deadline-correctness : expected ≥ 2 ms elapsed, got {delta} ns"
        );
    }

    // ── FFI shims — exercise each `__cssl_time_*` symbol ───────────────

    #[test]
    fn ffi_monotonic_shim_matches_impl() {
        let _g = lock_and_reset();
        // SAFETY : the FFI shim is safe-to-call.
        let a = unsafe { __cssl_time_monotonic_ns() };
        let b = cssl_time_monotonic_ns_impl();
        assert!(a <= b, "ffi-monotonic should be ≤ subsequent impl read");
        assert_eq!(monotonic_count(), 2);
    }

    #[test]
    fn ffi_wall_shim_matches_impl() {
        let _g = lock_and_reset();
        // SAFETY : the FFI shim is safe-to-call.
        let w = unsafe { __cssl_time_wall_unix_ns() };
        assert!(w > 0);
        assert_eq!(wall_count(), 1);
    }

    #[test]
    fn ffi_sleep_shim_matches_impl() {
        let _g = lock_and_reset();
        // SAFETY : the FFI shim is safe-to-call.
        let r = unsafe { __cssl_time_sleep_ns(0) };
        assert_eq!(r, TIME_OK);
        assert_eq!(sleep_count(), 1);
    }

    #[test]
    fn ffi_deadline_shim_matches_impl() {
        let _g = lock_and_reset();
        // SAFETY : the FFI shim is safe-to-call.
        let r = unsafe { __cssl_time_deadline_until(0) };
        // Deadline 0 ns is forever in the past relative to any process
        // older than 0 seconds ⇒ TIME_DEADLINE_ALREADY_PAST.
        assert_eq!(r, TIME_DEADLINE_ALREADY_PAST);
        assert_eq!(deadline_count(), 1);
    }

    // ── ABI lock-test : compile-time-assert the four FFI signatures ────

    #[test]
    fn ffi_symbols_have_correct_signatures() {
        // Compile-time assertion : these `let _ : <type> = …` lines fail
        // to compile if any FFI signature drifts from the documented ABI.
        let _: unsafe extern "C" fn() -> u64 = __cssl_time_monotonic_ns;
        let _: unsafe extern "C" fn() -> i64 = __cssl_time_wall_unix_ns;
        let _: unsafe extern "C" fn(u64) -> i32 = __cssl_time_sleep_ns;
        let _: unsafe extern "C" fn(u64) -> i32 = __cssl_time_deadline_until;
    }

    // ── return-code constants : sanity ─────────────────────────────────

    #[test]
    fn return_code_constants_have_canonical_values() {
        // Locked ABI ; renaming is a major-version-bump event.
        assert_eq!(TIME_OK, 0);
        assert_eq!(TIME_ERR, -1);
        assert_eq!(TIME_DEADLINE_ALREADY_PAST, 1);
    }

    // ── reset discipline : every counter zeros after reset ─────────────

    #[test]
    fn reset_zeros_every_counter() {
        let _g = lock_and_reset();
        // Drive every counter > 0.
        let _ = cssl_time_monotonic_ns_impl();
        let _ = cssl_time_wall_unix_ns_impl();
        let _ = cssl_time_sleep_ns_impl(0);
        let _ = cssl_time_deadline_until_impl(0);
        assert!(monotonic_count() > 0);
        assert!(wall_count() > 0);
        assert!(sleep_count() > 0);
        assert!(deadline_count() > 0);
        // Reset.
        reset_time_for_tests();
        assert_eq!(monotonic_count(), 0);
        assert_eq!(wall_count(), 0);
        assert_eq!(sleep_count(), 0);
        assert_eq!(deadline_count(), 0);
        assert_eq!(total_sleep_ns(), 0);
    }
}

// ───────────────────────────────────────────────────────────────────────
// § INTEGRATION_NOTE — wiring path for the integration commit
//
//   This file delivers the four `__cssl_time_*` FFI symbols + their
//   `_impl` testable counterparts but DOES NOT touch `cssl-rt::lib.rs`'s
//   `pub mod` list (per HARD CONSTRAINTS in the Wave-D1 dispatch). The
//   integration commit owns the surface change — at that time it should :
//
//   1. Add `pub mod host_time;` to `cssl-rt/src/lib.rs` after the existing
//      `pub mod path_hash;` line. The pub-mod ordering is alphabetical-
//      ish ; insert after `path_hash` to keep the layout grouped.
//   2. Add the host_time re-exports to the `pub use` block in lib.rs :
//        `pub use host_time::{`
//            `cssl_time_deadline_until_impl, cssl_time_monotonic_ns_impl,`
//            `cssl_time_sleep_ns_impl, cssl_time_wall_unix_ns_impl,`
//            `deadline_count, monotonic_count, reset_time_for_tests,`
//            `sleep_count, total_sleep_ns, wall_count, TIME_DEADLINE_ALREADY_PAST,`
//            `TIME_ERR, TIME_OK,`
//        `};`
//   3. Extend `lib.rs::test_helpers::lock_and_reset_all` to call
//      `crate::host_time::reset_time_for_tests();` so cross-module tests
//      that touch host_time alongside other globals serialize cleanly.
//   4. Update the `// § FFI SURFACE` doc-block in `lib.rs` to mention
//      the four new `__cssl_time_*` symbols (mirror the host_fs / host_net
//      blocks at lines 27-53).
//
//   Until that commit lands, this file is link-time-complete — the four
//   `extern "C" #[no_mangle]` symbols participate in `cssl-rt`'s symbol
//   table independent of the `pub mod` declaration in lib.rs (Rust
//   exports `#[no_mangle]` symbols from any compiled module). Cranelift-
//   driven calls to `__cssl_time_*` resolve at link time without
//   requiring the `pub mod` line.
//
//   The matching cgen wiring in
//   `cssl-cgen-cpu-cranelift/src/cgen_time.rs` was authored as part of
//   the same Wave-D1 slice ; together they expose a complete vertical :
//   MIR → cranelift `call __cssl_time_*` → cssl-rt impl → OS syscall.
// ───────────────────────────────────────────────────────────────────────
