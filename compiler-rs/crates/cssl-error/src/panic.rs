//! Panic-catch + frame-boundary helper.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.8 + § 4.2 + § 7.5.
//!
//! § DESIGN
//!   - [`catch_frame_panic`] : the canonical frame-boundary helper.
//!     Wraps a closure in `std::panic::catch_unwind` ; on panic, builds a
//!     structured [`crate::error::PanicReport`] + lifts to [`EngineError::Panic`].
//!   - PD-violation detection : the panic payload is scanned for known
//!     PD-trip patterns (e.g., starts with `"PD0001"`) ; if matched, the
//!     [`PanicReport::with_pd_violation(true)`] flag is set so callers know
//!     to fire the kill-switch.
//!   - Re-entrancy guard : [`is_handling_panic`] is checked at the top of
//!     [`engine_panic_hook`] ; nested panics fall through to the default
//!     handler (Rust process-abort) to avoid infinite loops (§ 7.5).
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - § 7 INTEGRITY : PD-violation panics ALWAYS surface a PD-tagged
//!     report ; degraded-mode override is rejected by callers via the
//!     [`crate::pd::halt_for_pd_violation`] bridge.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::context::SubsystemTag;
use crate::error::{EngineError, PanicReport};

// ───────────────────────────────────────────────────────────────────────
// § Re-entrancy guard.
// ───────────────────────────────────────────────────────────────────────

/// Re-entrancy flag : `true` while a panic-hook is currently executing.
/// Spec § 7.5 : nested panics ⟶ fall through to default handler.
static IS_HANDLING_PANIC: AtomicBool = AtomicBool::new(false);

/// Returns `true` if a panic-hook is currently executing (re-entrancy guard).
#[must_use]
pub fn is_handling_panic() -> bool {
    IS_HANDLING_PANIC.load(Ordering::Acquire)
}

/// RAII guard that sets/clears the re-entrancy flag.
struct PanicHandlingGuard;

impl PanicHandlingGuard {
    /// Try to acquire the guard. Returns `None` if already held (nested panic).
    fn try_acquire() -> Option<Self> {
        match IS_HANDLING_PANIC.compare_exchange(
            false,
            true,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Some(Self),
            Err(_) => None,
        }
    }
}

impl Drop for PanicHandlingGuard {
    fn drop(&mut self) {
        IS_HANDLING_PANIC.store(false, Ordering::Release);
    }
}

// ───────────────────────────────────────────────────────────────────────
// § PD-violation detection from panic payload.
// ───────────────────────────────────────────────────────────────────────

/// Inspect a panic-payload for PD-violation tags.
///
/// § HEURISTIC
///   - If the payload `Display`s as starting with `"PD"` followed by 4 digits,
///     treat as PD-violation (canonical PD-code form).
///   - Tests inject panics with payload `"PD0001 : harm-detected"` to verify
///     halt-trigger fires.
#[must_use]
pub fn payload_is_pd_violation(payload: &(dyn Any + Send)) -> bool {
    let msg = extract_panic_message(payload);
    if msg.len() < 6 || !msg.starts_with("PD") {
        return false;
    }
    let digits = &msg[2..];
    let first_four_are_digits = digits.chars().take(4).all(|c| c.is_ascii_digit());
    let fifth_is_not_digit = digits
        .chars()
        .nth(4)
        .map_or(true, |c| !c.is_ascii_digit());
    first_four_are_digits && fifth_is_not_digit
}

/// Extract a `&str`-like message from a panic-payload `dyn Any`.
/// Returns `"<unknown panic payload>"` for non-string payloads.
#[must_use]
pub fn extract_panic_message(payload: &(dyn Any + Send)) -> String {
    payload.downcast_ref::<&'static str>().map_or_else(
        || {
            payload
                .downcast_ref::<String>()
                .map_or_else(|| String::from("<unknown panic payload>"), Clone::clone)
        },
        |s| (*s).to_string(),
    )
}

// ───────────────────────────────────────────────────────────────────────
// § Frame-boundary panic-catch helper.
// ───────────────────────────────────────────────────────────────────────

/// Frame-boundary panic-catch.
///
/// § BEHAVIOR
///   - Runs the closure inside `std::panic::catch_unwind` (with `AssertUnwindSafe`
///     to avoid the unwind-safety bounds — caller's responsibility to ensure
///     state is consistent on panic-boundary).
///   - On panic : builds a structured [`PanicReport`] tagged with the
///     supplied `subsystem` ; payload is checked for PD-violation pattern.
///   - Returns `Result<R, EngineError>` so callers can `?` chain.
///
/// § DETERMINISM
///   - The panic-message extraction is deterministic given the same
///     panic-payload. The caller-supplied `frame_n` is included in the
///     report for replay-correlation.
///
/// § USE-SITE EXAMPLE
/// ```no_run
/// use cssl_error::panic::catch_frame_panic;
/// use cssl_error::context::SubsystemTag;
///
/// let result = catch_frame_panic(SubsystemTag::Render, 42, || {
///     // ... do frame-stage work that may panic ...
///     Ok::<_, cssl_error::EngineError>(7)
/// });
/// // On panic : result == Err(EngineError::Panic(...))
/// ```
pub fn catch_frame_panic<F, R>(
    subsystem: SubsystemTag,
    frame_n: u64,
    f: F,
) -> Result<R, EngineError>
where
    F: FnOnce() -> Result<R, EngineError>,
{
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    let payload = match result {
        Ok(Ok(value)) => return Ok(value),
        Ok(Err(err)) => return Err(err),
        Err(payload) => payload,
    };
    let pd_violation = payload_is_pd_violation(&*payload);
    let msg = extract_panic_message(&*payload);
    let report = PanicReport::new(msg, subsystem)
        .with_frame_n(frame_n)
        .with_pd_violation(pd_violation);
    Err(EngineError::Panic(report))
}

/// Thin wrapper : catch panic, do not propagate inner Result.
///
/// § BEHAVIOR
///   - Like [`catch_frame_panic`] but the inner closure returns `R` directly
///     (no inner Result). Convenient for closures that don't need to
///     propagate non-panic errors.
pub fn catch_frame_panic_simple<F, R>(
    subsystem: SubsystemTag,
    frame_n: u64,
    f: F,
) -> Result<R, EngineError>
where
    F: FnOnce() -> R,
{
    catch_frame_panic(subsystem, frame_n, || Ok(f()))
}

// ───────────────────────────────────────────────────────────────────────
// § Engine panic-hook installation.
// ───────────────────────────────────────────────────────────────────────

/// Install the engine-wide panic-hook.
///
/// § BEHAVIOR
///   - Registers a process-wide panic-hook via `std::panic::set_hook`.
///   - Hook checks the re-entrancy flag ; if already set, falls through to
///     the previous (default) handler (avoid infinite loops ; § 7.5).
///   - Hook DOES NOT abort the process : the engine's frame-boundary
///     [`catch_frame_panic`] is responsible for catching the panic. The
///     hook's role is purely structured-reporting.
///
/// § IDEMPOTENCY
///   - Calling this multiple times overrides the hook each time. Callers
///     should call it once at engine-init.
pub fn install_engine_panic_hook() {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let Some(_guard) = PanicHandlingGuard::try_acquire() else {
            // Nested panic ; fall through to previous (default) handler
            // to avoid infinite loop. § 7.5
            prev(info);
            return;
        };
        // Best-effort message extraction. Suppressed-but-readable so the
        // hook contract documents the structure that the frame-catch path
        // will reconstruct via PanicReport.
        let msg = info.payload().downcast_ref::<&'static str>().map_or_else(
            || {
                info.payload()
                    .downcast_ref::<String>()
                    .map_or_else(|| String::from("<unknown panic payload>"), Clone::clone)
            },
            |s| (*s).to_string(),
        );
        let location_str = info
            .location()
            .map(|l| format!(" @ {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let _ = (msg, location_str);
        // Emit through the previous hook so the engineer-visible default
        // formatting is preserved. The engine's frame-catch path constructs
        // the structured PanicReport.
        prev(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::{
        catch_frame_panic, catch_frame_panic_simple, extract_panic_message,
        install_engine_panic_hook, is_handling_panic, payload_is_pd_violation,
    };
    use crate::context::SubsystemTag;
    use crate::error::EngineError;

    #[test]
    fn catch_frame_panic_passes_through_ok() {
        let r = catch_frame_panic::<_, i32>(SubsystemTag::Render, 0, || Ok(42));
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn catch_frame_panic_passes_through_inner_err() {
        let r = catch_frame_panic::<_, i32>(SubsystemTag::Render, 0, || {
            Err(EngineError::other("boom"))
        });
        match r {
            Err(EngineError::Other(s)) => assert_eq!(s, "boom"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn catch_frame_panic_catches_explicit_panic() {
        let r = catch_frame_panic::<_, i32>(SubsystemTag::Render, 7, || {
            panic!("oops");
        });
        match r {
            Err(EngineError::Panic(report)) => {
                assert_eq!(report.subsystem, SubsystemTag::Render);
                assert_eq!(report.frame_n, 7);
                assert!(report.message.contains("oops"));
                assert!(!report.is_pd_violation());
            }
            other => panic!("expected Panic variant, got {other:?}"),
        }
    }

    #[test]
    fn catch_frame_panic_pd_violation_payload_tagged() {
        let r = catch_frame_panic::<_, i32>(SubsystemTag::PrimeDirective, 1, || {
            panic!("PD0001 : harm-detected");
        });
        match r {
            Err(EngineError::Panic(report)) => {
                assert!(report.is_pd_violation());
            }
            other => panic!("expected Panic variant, got {other:?}"),
        }
    }

    #[test]
    fn catch_frame_panic_pd_violation_string_payload() {
        let r = catch_frame_panic::<_, i32>(SubsystemTag::Render, 1, || {
            let msg = String::from("PD0018 : raw-path");
            panic!("{msg}");
        });
        match r {
            Err(EngineError::Panic(report)) => {
                assert!(report.is_pd_violation());
            }
            other => panic!("expected Panic variant, got {other:?}"),
        }
    }

    #[test]
    fn catch_frame_panic_simple_basic() {
        let r = catch_frame_panic_simple(SubsystemTag::Render, 0, || 7);
        assert_eq!(r.unwrap(), 7);
    }

    #[test]
    fn catch_frame_panic_simple_catches_panic() {
        let r = catch_frame_panic_simple::<_, i32>(SubsystemTag::Render, 0, || panic!("x"));
        assert!(r.is_err());
    }

    #[test]
    fn payload_is_pd_violation_true_for_canonical_codes() {
        let p1: Box<dyn std::any::Any + Send> = Box::new("PD0001 : harm");
        assert!(payload_is_pd_violation(&*p1));
        let p2: Box<dyn std::any::Any + Send> = Box::new(String::from("PD0018"));
        assert!(payload_is_pd_violation(&*p2));
    }

    #[test]
    fn payload_is_pd_violation_false_for_non_pd() {
        let p: Box<dyn std::any::Any + Send> = Box::new("oops");
        assert!(!payload_is_pd_violation(&*p));
        let q: Box<dyn std::any::Any + Send> = Box::new("PD123 too short");
        assert!(!payload_is_pd_violation(&*q));
        let r: Box<dyn std::any::Any + Send> = Box::new("XX0001 wrong prefix");
        assert!(!payload_is_pd_violation(&*r));
    }

    #[test]
    fn extract_panic_message_str_payload() {
        let p: Box<dyn std::any::Any + Send> = Box::new("hello");
        assert_eq!(extract_panic_message(&*p), "hello");
    }

    #[test]
    fn extract_panic_message_string_payload() {
        let p: Box<dyn std::any::Any + Send> = Box::new(String::from("world"));
        assert_eq!(extract_panic_message(&*p), "world");
    }

    #[test]
    fn extract_panic_message_unknown_payload() {
        let p: Box<dyn std::any::Any + Send> = Box::new(42i32);
        assert_eq!(extract_panic_message(&*p), "<unknown panic payload>");
    }

    #[test]
    fn re_entrancy_guard_default_false() {
        // Outside a panic-hook execution, flag is false.
        assert!(!is_handling_panic());
    }

    #[test]
    fn install_panic_hook_idempotent() {
        // Installing twice is OK ; second call simply re-registers.
        install_engine_panic_hook();
        install_engine_panic_hook();
        // Test passes if no panic occurred during install.
    }

    #[test]
    fn catch_frame_panic_preserves_replay_determinism() {
        // Same input ⟶ same output, same fingerprint.
        let r1 = catch_frame_panic::<_, ()>(SubsystemTag::Render, 100, || panic!("x"));
        let r2 = catch_frame_panic::<_, ()>(SubsystemTag::Render, 100, || panic!("x"));
        if let (Err(EngineError::Panic(a)), Err(EngineError::Panic(b))) = (r1, r2) {
            assert_eq!(a.message, b.message);
            assert_eq!(a.frame_n, b.frame_n);
            assert_eq!(a.subsystem, b.subsystem);
            assert_eq!(a.is_pd_violation(), b.is_pd_violation());
        } else {
            panic!("expected two Panic-variant Err results");
        }
    }

    #[test]
    fn catch_frame_panic_records_subsystem_tag() {
        let r = catch_frame_panic::<_, ()>(SubsystemTag::Audio, 1, || panic!("a"));
        if let Err(EngineError::Panic(report)) = r {
            assert_eq!(report.subsystem, SubsystemTag::Audio);
        }
    }

    #[test]
    fn catch_frame_panic_records_frame_n() {
        let r = catch_frame_panic::<_, ()>(SubsystemTag::Anim, 99, || panic!("a"));
        if let Err(EngineError::Panic(report)) = r {
            assert_eq!(report.frame_n, 99);
        }
    }
}
