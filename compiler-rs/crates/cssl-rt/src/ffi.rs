//! § cssl-rt FFI surface — stable `#[no_mangle]` symbols (T11-D52, S6-A1).
//!
//! § ROLE
//!   The single source-of-truth for the ABI-stable surface that
//!   CSSLv3-emitted code links against.
//!
//! § SYMBOLS  (all `extern "C"` ; ABI-stable from S6-A1 forward)
//!   - `__cssl_entry(user_main: extern "C" fn() -> i32) -> i32`
//!   - `__cssl_alloc(size: usize, align: usize) -> *mut u8`
//!   - `__cssl_free(ptr: *mut u8, size: usize, align: usize) -> ()`
//!   - `__cssl_realloc(ptr, old_size, new_size, align) -> *mut u8`
//!   - `__cssl_panic(msg_ptr, msg_len, file_ptr, file_len, line) -> !`
//!   - `__cssl_abort() -> !`
//!   - `__cssl_exit(code: i32) -> !`
//!
//! § INVARIANTS  (carried-forward landmine — see HANDOFF_SESSION_6.csl)
//!   ‼ Renaming any of these symbols is a major-version-bump event ;
//!     CSSLv3-emitted code references them by exact name.
//!   ‼ Argument types + ordering are also locked. Use additional symbols
//!     (e.g., `__cssl_alloc_zeroed`) for new behaviors.
//!   ‼ Each symbol delegates to a Rust-side `_impl` fn so unit tests can
//!     exercise behavior without going through the FFI boundary.
//!
//! § SAFETY
//!   These are `unsafe extern "C" fn` ; the SAFETY comment under each
//!   describes caller obligations. The unsafe is scoped narrowly inside
//!   the function body when calling internal `unsafe fn` helpers.

#![allow(unsafe_code)]

// ───────────────────────────────────────────────────────────────────────
// § __cssl_alloc / __cssl_free / __cssl_realloc
// ───────────────────────────────────────────────────────────────────────

/// FFI : allocate `size` bytes with `align` alignment.
///
/// Returns null on rejection (size 0, align not power-of-two, OOM).
///
/// # Safety
/// Caller must:
/// - eventually pair with [`__cssl_free`] using the same `size`+`align`,
/// - never write or read outside `[ret, ret+size)`,
/// - treat the bytes as uninitialized.
#[no_mangle]
pub unsafe extern "C" fn __cssl_alloc(size: usize, align: usize) -> *mut u8 {
    // SAFETY : raw_alloc preconditions documented at its definition ;
    // caller of __cssl_alloc inherits the contract.
    unsafe { crate::alloc::raw_alloc(size, align) }
}

/// FFI : free a `(ptr, size, align)` allocation produced by [`__cssl_alloc`].
///
/// Null `ptr` is a no-op.
///
/// # Safety
/// Caller must:
/// - have obtained `ptr` from `__cssl_alloc` with the SAME `size`+`align`,
/// - not double-free or use after free.
#[no_mangle]
pub unsafe extern "C" fn __cssl_free(ptr: *mut u8, size: usize, align: usize) {
    // SAFETY : raw_free preconditions match __cssl_free per ABI contract.
    unsafe { crate::alloc::raw_free(ptr, size, align) };
}

/// FFI : reallocate `(ptr, old_size)` to `new_size` keeping `align`.
///
/// Returns null on failure ; on success the old pointer is invalidated.
///
/// # Safety
/// Caller must:
/// - have obtained `ptr` from `__cssl_alloc` with `align`+`old_size`,
/// - not use `ptr` after a successful (non-null) return.
#[no_mangle]
pub unsafe extern "C" fn __cssl_realloc(
    ptr: *mut u8,
    old_size: usize,
    new_size: usize,
    align: usize,
) -> *mut u8 {
    // SAFETY : raw_realloc preconditions match __cssl_realloc ABI contract.
    unsafe { crate::alloc::raw_realloc(ptr, old_size, new_size, align) }
}

// ───────────────────────────────────────────────────────────────────────
// § __cssl_panic — formatted message + abort
// ───────────────────────────────────────────────────────────────────────

/// FFI : emit a panic message and terminate.
///
/// Format : `panic: <msg> at <file>:<line>\n` to stderr ; then aborts.
///
/// # Safety
/// Caller must ensure:
/// - `msg_ptr` valid for `msg_len` bytes (or `msg_len == 0`),
/// - `file_ptr` valid for `file_len` bytes (or `file_len == 0`).
#[no_mangle]
pub unsafe extern "C" fn __cssl_panic(
    msg_ptr: *const u8,
    msg_len: usize,
    file_ptr: *const u8,
    file_len: usize,
    line: u32,
) -> ! {
    // SAFETY : pointers + lengths inherited from caller ABI contract.
    let line_str =
        unsafe { crate::panic::format_panic_from_ptrs(msg_ptr, msg_len, file_ptr, file_len, line) };
    crate::panic::record_panic(&line_str);
    crate::exit::cssl_abort_impl()
}

// ───────────────────────────────────────────────────────────────────────
// § __cssl_abort + __cssl_exit
// ───────────────────────────────────────────────────────────────────────

/// FFI : terminate the process via abort. Never returns.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_abort() -> ! {
    crate::exit::cssl_abort_impl()
}

/// FFI : terminate the process with exit-code `code`. Never returns.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_exit(code: i32) -> ! {
    crate::exit::cssl_exit_impl(code);
}

// ───────────────────────────────────────────────────────────────────────
// § __cssl_entry — process entry shim
// ───────────────────────────────────────────────────────────────────────

/// FFI : process entry shim. Initializes the runtime, calls `user_main`,
/// returns its exit-code.
///
/// # Safety
/// `user_main` must be a valid `extern "C" fn() -> i32` pointer.
#[no_mangle]
pub unsafe extern "C" fn __cssl_entry(user_main: extern "C" fn() -> i32) -> i32 {
    // SAFETY : user_main validity is the caller's contract.
    unsafe { crate::runtime::cssl_entry_impl_extern(user_main) }
}

// ───────────────────────────────────────────────────────────────────────
// § __cssl_fs_open / __cssl_fs_read / __cssl_fs_write / __cssl_fs_close —
// file-system I/O surface (T11-D76, S6-B5).
//
// Each shim delegates to the platform `cssl_fs_*_impl` selected via cfg
// in [`crate::io`]. The shims preserve the i64-handle ABI so a single
// CSSLv3-source-level interface can drive both Win32 HANDLEs and POSIX
// fds without per-OS source-code changes.
// ───────────────────────────────────────────────────────────────────────

/// FFI : open a file for I/O. See [`crate::io`] doc-block for the flag
/// bitset + error semantics.
///
/// Returns the platform handle cast to `i64` on success, or
/// [`crate::io::INVALID_HANDLE`] (`-1`) on failure with the canonical
/// error in the per-thread last-error slot.
///
/// # Safety
/// Caller must ensure :
/// - `path_ptr` valid for `path_len` bytes (or `path_len == 0` and
///   `path_ptr` is null — yields `InvalidInput`).
/// - `path_len <= isize::MAX`.
/// - `flags` is a valid combination from
///   [`crate::io::OPEN_FLAG_MASK`] (validation happens inside the impl ;
///   bad flags produce `InvalidInput`).
#[no_mangle]
pub unsafe extern "C" fn __cssl_fs_open(path_ptr: *const u8, path_len: usize, flags: i32) -> i64 {
    // SAFETY : path_ptr / path_len contract inherited from caller.
    unsafe { crate::io::cssl_fs_open_impl(path_ptr, path_len, flags) }
}

/// FFI : read up to `buf_len` bytes from `handle` into `buf_ptr`.
///
/// Returns bytes-read (0 at EOF) ; `-1` on failure with the canonical
/// error in the per-thread last-error slot.
///
/// # Safety
/// Caller must ensure :
/// - `handle` is a valid handle returned from [`__cssl_fs_open`] with
///   read access and not yet closed.
/// - `buf_ptr` valid for `buf_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn __cssl_fs_read(handle: i64, buf_ptr: *mut u8, buf_len: usize) -> i64 {
    // SAFETY : handle + buffer contract inherited from caller.
    unsafe { crate::io::cssl_fs_read_impl(handle, buf_ptr, buf_len) }
}

/// FFI : write `buf_len` bytes from `buf_ptr` to `handle`.
///
/// Returns bytes-written ; `-1` on failure. Short writes are possible
/// per-syscall ; callers loop until all bytes have been written.
///
/// # Safety
/// Caller must ensure :
/// - `handle` is a valid handle returned from [`__cssl_fs_open`] with
///   write or append access and not yet closed.
/// - `buf_ptr` valid for `buf_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn __cssl_fs_write(handle: i64, buf_ptr: *const u8, buf_len: usize) -> i64 {
    // SAFETY : handle + buffer contract inherited from caller.
    unsafe { crate::io::cssl_fs_write_impl(handle, buf_ptr, buf_len) }
}

/// FFI : close `handle`. Returns `0` on success, `-1` on failure with the
/// canonical error in the per-thread last-error slot.
///
/// # Safety
/// Caller must ensure `handle` is a valid handle returned from
/// [`__cssl_fs_open`] and not yet closed.
#[no_mangle]
pub unsafe extern "C" fn __cssl_fs_close(handle: i64) -> i64 {
    // SAFETY : handle contract inherited from caller.
    unsafe { crate::io::cssl_fs_close_impl(handle) }
}

/// FFI : read the canonical error kind from the last fs op.
///
/// Returns the discriminant from [`crate::io::io_error_code`]
/// (`SUCCESS = 0`, `NOT_FOUND = 1`, ...). Source-level CSSLv3 maps the
/// returned i32 onto the `IoError` sum-type via the recognizer in
/// `cssl_mir::body_lower`.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_fs_last_error_kind() -> i32 {
    crate::io::last_io_error_kind()
}

/// FFI : read the raw OS code from the last fs op (Win32
/// `GetLastError` / POSIX `errno`).
///
/// Useful for diagnostic logging when the canonical kind is `OTHER`.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_fs_last_error_os() -> i32 {
    crate::io::last_io_error_os()
}

// ───────────────────────────────────────────────────────────────────────
// § tests — exercise FFI boundary
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
// fs FFI tests cast `payload.len() as i64` for assert comparison ;
// scope the cast-lint suppression to this test mod (production paths
// already carry per-fn `#[allow]` annotations).
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::test_helpers::lock_and_reset_all;

    // § Module-level test fns referenced by `__cssl_entry` integration tests.
    // Defined here (not inside #[test] bodies) to avoid clippy
    // `items_after_statements`.

    extern "C" fn entry_returns_42() -> i32 {
        42
    }

    extern "C" fn entry_returns_0() -> i32 {
        0
    }

    extern "C" fn entry_alloc_and_free() -> i32 {
        let p = unsafe { __cssl_alloc(256, 16) };
        if p.is_null() {
            return 1;
        }
        unsafe { __cssl_free(p, 256, 16) };
        0
    }

    #[test]
    fn ffi_alloc_then_free_roundtrips() {
        let _g = lock_and_reset_all();
        let p = unsafe { __cssl_alloc(128, 8) };
        assert!(!p.is_null());
        assert_eq!(crate::alloc::alloc_count(), 1);
        unsafe { __cssl_free(p, 128, 8) };
        assert_eq!(crate::alloc::free_count(), 1);
        assert_eq!(crate::alloc::bytes_in_use(), 0);
    }

    #[test]
    fn ffi_alloc_zero_size_returns_null() {
        let _g = lock_and_reset_all();
        let p = unsafe { __cssl_alloc(0, 8) };
        assert!(p.is_null());
        assert_eq!(crate::alloc::alloc_count(), 0);
    }

    #[test]
    fn ffi_alloc_grow_via_realloc() {
        let _g = lock_and_reset_all();
        let p = unsafe { __cssl_alloc(8, 8) };
        for i in 0..8 {
            unsafe { p.add(i).write(i as u8) };
        }
        let p2 = unsafe { __cssl_realloc(p, 8, 64, 8) };
        assert!(!p2.is_null());
        for i in 0..8 {
            assert_eq!(unsafe { p2.add(i).read() }, i as u8);
        }
        unsafe { __cssl_free(p2, 64, 8) };
    }

    #[test]
    fn ffi_realloc_to_zero_frees() {
        let _g = lock_and_reset_all();
        let p = unsafe { __cssl_alloc(32, 8) };
        let p2 = unsafe { __cssl_realloc(p, 32, 0, 8) };
        assert!(p2.is_null());
        assert_eq!(crate::alloc::free_count(), 1);
    }

    #[test]
    fn ffi_entry_with_extern_main() {
        let _g = lock_and_reset_all();
        let code = unsafe { __cssl_entry(entry_returns_42) };
        assert_eq!(code, 42);
        assert_eq!(crate::runtime::entry_invocation_count(), 1);
        assert!(crate::runtime::is_runtime_initialized());
    }

    #[test]
    fn ffi_entry_propagates_zero_exit_code() {
        let _g = lock_and_reset_all();
        assert_eq!(unsafe { __cssl_entry(entry_returns_0) }, 0);
    }

    #[test]
    fn ffi_alloc_via_free_null_is_safe_noop() {
        let _g = lock_and_reset_all();
        unsafe { __cssl_free(core::ptr::null_mut(), 0, 8) };
        assert_eq!(crate::alloc::free_count(), 0);
    }

    #[test]
    fn ffi_alloc_align_validation() {
        let _g = lock_and_reset_all();
        // align=0 rejected
        assert!(unsafe { __cssl_alloc(64, 0) }.is_null());
        // align=3 (not power of two) rejected
        assert!(unsafe { __cssl_alloc(64, 3) }.is_null());
        // align=8 succeeds
        let p = unsafe { __cssl_alloc(64, 8) };
        assert!(!p.is_null());
        unsafe { __cssl_free(p, 64, 8) };
    }

    #[test]
    fn ffi_symbols_have_correct_signatures() {
        // Compile-time assertion : these `let _ : <type> = …` lines fail to
        // compile if any FFI signature drifts from the documented ABI.
        let _: unsafe extern "C" fn(usize, usize) -> *mut u8 = __cssl_alloc;
        let _: unsafe extern "C" fn(*mut u8, usize, usize) = __cssl_free;
        let _: unsafe extern "C" fn(*mut u8, usize, usize, usize) -> *mut u8 = __cssl_realloc;
        let _: unsafe extern "C" fn(*const u8, usize, *const u8, usize, u32) -> ! = __cssl_panic;
        let _: unsafe extern "C" fn() -> ! = __cssl_abort;
        let _: unsafe extern "C" fn(i32) -> ! = __cssl_exit;
        let _: unsafe extern "C" fn(extern "C" fn() -> i32) -> i32 = __cssl_entry;
        // S6-B5 (T11-D76) — fs surface ABI lock.
        let _: unsafe extern "C" fn(*const u8, usize, i32) -> i64 = __cssl_fs_open;
        let _: unsafe extern "C" fn(i64, *mut u8, usize) -> i64 = __cssl_fs_read;
        let _: unsafe extern "C" fn(i64, *const u8, usize) -> i64 = __cssl_fs_write;
        let _: unsafe extern "C" fn(i64) -> i64 = __cssl_fs_close;
        let _: unsafe extern "C" fn() -> i32 = __cssl_fs_last_error_kind;
        let _: unsafe extern "C" fn() -> i32 = __cssl_fs_last_error_os;
    }

    // S6-B5 (T11-D76) — fs FFI roundtrip via the FFI shims. Mirrors the
    // platform-specific `open_write_create_close_roundtrip` test but
    // deliberately exercises the `__cssl_fs_*` symbols (= the surface
    // CSSLv3-emitted code calls into) rather than the platform `_impl`
    // fns directly. Confirms the FFI boundary preserves the i64-handle
    // ABI without re-tagging.
    #[test]
    fn ffi_fs_open_write_read_close_roundtrip() {
        let _g = lock_and_reset_all();
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("cssl_b5_ffi_roundtrip.txt");
        let path_str = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);

        let payload = b"hello cssl-b5 ffi roundtrip\n";

        // Write phase via __cssl_fs_*.
        let h = unsafe {
            __cssl_fs_open(
                path_str.as_ptr(),
                path_str.len(),
                crate::io::OPEN_WRITE | crate::io::OPEN_CREATE | crate::io::OPEN_TRUNCATE,
            )
        };
        assert_ne!(h, crate::io::INVALID_HANDLE);
        let n = unsafe { __cssl_fs_write(h, payload.as_ptr(), payload.len()) };
        assert_eq!(n, payload.len() as i64);
        let cr = unsafe { __cssl_fs_close(h) };
        assert_eq!(cr, 0);

        // Read phase via __cssl_fs_*.
        let h2 = unsafe { __cssl_fs_open(path_str.as_ptr(), path_str.len(), crate::io::OPEN_READ) };
        assert_ne!(h2, crate::io::INVALID_HANDLE);
        let mut buf = vec![0u8; payload.len() + 4];
        let nr = unsafe { __cssl_fs_read(h2, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(nr, payload.len() as i64);
        let cr2 = unsafe { __cssl_fs_close(h2) };
        assert_eq!(cr2, 0);

        assert_eq!(&buf[..payload.len()], payload);

        // Counter discipline : 2 opens + ≥ 1 read + 1 write + 2 closes.
        assert_eq!(crate::io::open_count(), 2);
        assert_eq!(crate::io::write_count(), 1);
        // read_count may be > 1 if the syscall returns short ; one read is
        // the minimum.
        assert!(crate::io::read_count() >= 1);
        assert_eq!(crate::io::close_count(), 2);
        assert_eq!(crate::io::bytes_written_total(), payload.len() as u64);
        assert_eq!(crate::io::bytes_read_total(), payload.len() as u64);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ffi_fs_open_invalid_path_returns_minus_one() {
        let _g = lock_and_reset_all();
        let r = unsafe { __cssl_fs_open(core::ptr::null(), 0, crate::io::OPEN_READ) };
        assert_eq!(r, crate::io::INVALID_HANDLE);
        let kind = unsafe { __cssl_fs_last_error_kind() };
        assert_eq!(kind, crate::io::io_error_code::INVALID_INPUT);
    }

    #[test]
    fn ffi_fs_close_invalid_handle_returns_minus_one() {
        let _g = lock_and_reset_all();
        let r = unsafe { __cssl_fs_close(crate::io::INVALID_HANDLE) };
        assert_eq!(r, -1);
        let kind = unsafe { __cssl_fs_last_error_kind() };
        assert_eq!(kind, crate::io::io_error_code::INVALID_INPUT);
    }

    #[test]
    fn ffi_entry_runs_alloc_and_free_inside_user_main() {
        let _g = lock_and_reset_all();
        let code = unsafe { __cssl_entry(entry_alloc_and_free) };
        assert_eq!(code, 0);
        assert_eq!(crate::alloc::alloc_count(), 1);
        assert_eq!(crate::alloc::free_count(), 1);
    }

    #[test]
    fn ffi_panic_format_records_via_internal_helper() {
        // We CANNOT call __cssl_panic directly in a test (it aborts the
        // process). Instead, exercise the underlying format + record path.
        let _g = lock_and_reset_all();
        let line = unsafe {
            crate::panic::format_panic_from_ptrs(b"err".as_ptr(), 3, b"f.cssl".as_ptr(), 6, 17)
        };
        crate::panic::record_panic(&line);
        assert_eq!(crate::panic::panic_count(), 1);
    }
}
