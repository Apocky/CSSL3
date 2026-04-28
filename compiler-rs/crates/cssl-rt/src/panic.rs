//! § cssl-rt panic-handling surface (T11-D52, S6-A1).
//!
//! § ROLE
//!   Stage-0 panic plumbing for CSSLv3-emitted code. Splits "format the
//!   message" from "emit + exit" so tests can verify formatting without
//!   actually terminating the test runner.
//!
//! § FFI bridge ←→ [`crate::ffi`]
//!   `__cssl_panic(msg_ptr, msg_len, file_ptr, file_len, line)` — the FFI
//!   surface — composes [`format_panic`] + emits to stderr + calls
//!   [`crate::exit::cssl_abort_impl`].
//!
//! § FORMAT
//!   `panic: <msg> at <file>:<line>\n`
//!   The trailing newline is intentional — easier downstream `grep`-ability.
//!
//! § INVARIANTS
//!   - `format_panic` never panics, even with malformed UTF-8 inputs.
//!   - Empty msg + empty file gracefully render as `panic:  at :<line>`.
//!   - The handler increments [`panic_count`] on every invocation.
//!
//! § FUTURE (deferred to phase-B)
//!   - Telemetry-ring emission (R18 evidence).
//!   - Backtrace capture.
//!   - Custom user panic-hook registration.

// § T11-D52 (S6-A1) : the FFI bridge consumes raw byte-pointers ; file-level
// `unsafe_code` allow per cssl-cgen-cpu-cranelift convention. Each unsafe
// block carries an inline SAFETY paragraph.
#![allow(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § panic counter
// ───────────────────────────────────────────────────────────────────────

static PANIC_COUNT: AtomicU64 = AtomicU64::new(0);

/// Number of panic handler invocations observed since process start.
#[must_use]
pub fn panic_count() -> u64 {
    PANIC_COUNT.load(Ordering::Relaxed)
}

/// Reset the panic counter. Test-only.
pub fn reset_panic_count_for_tests() {
    PANIC_COUNT.store(0, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § format_panic — message composition (no I/O, no exit)
// ───────────────────────────────────────────────────────────────────────

/// Compose the canonical stage-0 panic line.
///
/// § OUT  `panic: <msg> at <file>:<line>\n`
///
/// Inputs are byte-slices ; non-UTF-8 sequences render via
/// [`String::from_utf8_lossy`] so the format is total. This avoids the
/// classic bootstrap-bug of "panic-handler panicked while formatting".
#[must_use]
pub fn format_panic(msg: &[u8], file: &[u8], line: u32) -> String {
    let msg_s = String::from_utf8_lossy(msg);
    let file_s = String::from_utf8_lossy(file);
    format!("panic: {msg_s} at {file_s}:{line}\n")
}

/// Bridge from raw FFI byte-pointers to a formatted panic line.
///
/// # Safety
/// Caller must ensure:
/// - `msg_ptr` is valid for `msg_len` bytes (or `msg_len == 0`),
/// - `file_ptr` is valid for `file_len` bytes (or `file_len == 0`),
/// - the byte ranges do not overlap with concurrent writes.
#[allow(unsafe_code)]
pub unsafe fn format_panic_from_ptrs(
    msg_ptr: *const u8,
    msg_len: usize,
    file_ptr: *const u8,
    file_len: usize,
    line: u32,
) -> String {
    let msg = if msg_ptr.is_null() || msg_len == 0 {
        b"" as &[u8]
    } else {
        // SAFETY : caller-asserted validity for msg_len bytes ⇒ slice OK.
        unsafe { core::slice::from_raw_parts(msg_ptr, msg_len) }
    };
    let file = if file_ptr.is_null() || file_len == 0 {
        b"" as &[u8]
    } else {
        // SAFETY : caller-asserted validity for file_len bytes ⇒ slice OK.
        unsafe { core::slice::from_raw_parts(file_ptr, file_len) }
    };
    format_panic(msg, file, line)
}

// ───────────────────────────────────────────────────────────────────────
// § panic_handler — emits + counts ; for tests, suppresses the exit step
// ───────────────────────────────────────────────────────────────────────

/// Internal panic dispatch entry-point.
///
/// In production, the FFI layer calls this then [`crate::exit::cssl_abort_impl`].
/// In tests, callers invoke this directly to exercise format + counter
/// updates without aborting the test process.
pub fn record_panic(line: &str) {
    eprint!("{line}");
    PANIC_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::lock_and_reset_all as lock_and_reset;

    #[test]
    fn format_panic_basic() {
        let s = format_panic(b"oh no", b"foo.cssl", 42);
        assert_eq!(s, "panic: oh no at foo.cssl:42\n");
    }

    #[test]
    fn format_panic_empty_msg() {
        let s = format_panic(b"", b"foo.cssl", 7);
        assert_eq!(s, "panic:  at foo.cssl:7\n");
    }

    #[test]
    fn format_panic_empty_file() {
        let s = format_panic(b"err", b"", 99);
        assert_eq!(s, "panic: err at :99\n");
    }

    #[test]
    fn format_panic_empty_msg_and_file() {
        let s = format_panic(b"", b"", 0);
        assert_eq!(s, "panic:  at :0\n");
    }

    #[test]
    fn format_panic_lossy_for_invalid_utf8() {
        let bad = &[0xFF_u8, 0xFE, 0xFD];
        let s = format_panic(bad, b"foo", 1);
        assert!(s.starts_with("panic: "));
        assert!(s.contains(" at foo:1\n"));
        // U+FFFD replacement-char survives the round trip
        assert!(s.contains('\u{FFFD}'));
    }

    #[test]
    fn format_panic_long_msg() {
        let long = "x".repeat(2048);
        let s = format_panic(long.as_bytes(), b"big.cssl", 1);
        assert!(s.contains(&long));
        assert!(s.ends_with(" at big.cssl:1\n"));
    }

    #[test]
    fn format_panic_max_line_number() {
        let s = format_panic(b"top", b"top.cssl", u32::MAX);
        assert!(s.contains(&format!(":{}\n", u32::MAX)));
    }

    #[test]
    fn format_panic_from_ptrs_null_inputs_render_empty() {
        let s = unsafe { format_panic_from_ptrs(core::ptr::null(), 5, core::ptr::null(), 5, 17) };
        assert_eq!(s, "panic:  at :17\n");
    }

    #[test]
    fn format_panic_from_ptrs_zero_len_renders_empty() {
        let bytes = b"abcd";
        let s = unsafe { format_panic_from_ptrs(bytes.as_ptr(), 0, bytes.as_ptr(), 0, 1) };
        assert_eq!(s, "panic:  at :1\n");
    }

    #[test]
    fn format_panic_from_ptrs_round_trips() {
        let msg = b"oops";
        let file = b"x.cssl";
        let s = unsafe {
            format_panic_from_ptrs(msg.as_ptr(), msg.len(), file.as_ptr(), file.len(), 9)
        };
        assert_eq!(s, "panic: oops at x.cssl:9\n");
    }

    #[test]
    fn record_panic_increments_counter() {
        let _g = lock_and_reset();
        let line = format_panic(b"x", b"y", 1);
        record_panic(&line);
        assert_eq!(panic_count(), 1);
        record_panic(&line);
        record_panic(&line);
        assert_eq!(panic_count(), 3);
    }

    #[test]
    fn panic_count_starts_zero_after_reset() {
        let _g = lock_and_reset();
        assert_eq!(panic_count(), 0);
    }

    #[test]
    fn format_panic_handles_unicode_in_msg() {
        let s = format_panic("néphalim ✓".as_bytes(), b"x", 5);
        assert_eq!(s, "panic: néphalim ✓ at x:5\n");
    }

    #[test]
    fn format_panic_trailing_newline_present() {
        let s = format_panic(b"x", b"y", 1);
        assert!(s.ends_with('\n'));
    }
}
