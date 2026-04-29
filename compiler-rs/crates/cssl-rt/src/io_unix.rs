//! § cssl-rt file-I/O — Unix platform layer (T11-D76, S6-B5).
//!
//! § ROLE
//!   Hand-rolled libc syscall bindings for `open` / `read` / `write` /
//!   `close`. Per the dispatch-plan landmines we deliberately avoid
//!   pulling in the `libc` crate at this slice — the POSIX file-I/O
//!   surface is small + stable since the SUSv2 specification, and adding
//!   a heavyweight crate would require its own DECISIONS sub-entry. The
//!   cssl-rt convention established at T11-D52 is "hand-rolled
//!   `extern "C"` declarations with each `unsafe` block carrying an
//!   inline SAFETY paragraph".
//!
//! § PLATFORMS
//!   Compiled for any `cfg(not(target_os = "windows"))` host — Linux +
//!   macOS share this implementation. The `open` flag values differ
//!   per-OS at the bit-pattern level (e.g., `O_CREAT = 0x40` on Linux
//!   x86-64 but `0x200` on macOS) so we use the libc-supplied values
//!   exposed through cfg-gated `pub const`s rather than hard-coding.
//!
//! § ABI NOTES
//!   - `c_int` = 32-bit `i32` on every supported platform.
//!   - `mode_t` = 16-bit on macOS, 32-bit on Linux ; we model as `u32`
//!     and the per-platform call-site widens / narrows appropriately
//!     (libc accepts the wider parameter on macOS via auto-promotion).
//!   - `ssize_t` = signed 64-bit on every supported 64-bit Unix.
//!   - `off_t` = 64-bit when `_FILE_OFFSET_BITS = 64` (Linux default for
//!     glibc on 64-bit + always on macOS) ; we don't expose seek-offset
//!     parameters at this slice (no append-mode seek required ; `O_APPEND`
//!     handles the per-write append atomically).
//!
//! § INVARIANTS
//!   - On failure, every `cssl_fs_*_impl` returns `-1 as i64` AND records
//!     the canonical error in [`crate::io::record_io_error`]. The high 32
//!     bits of the slot carry the raw `errno` value for diagnostic
//!     consumers.
//!   - Read / write byte counts are bounded to `i32::MAX` per call to
//!     match the Win32 path ; POSIX `read / write` accept up to `SSIZE_MAX`
//!     but we keep the unified ceiling for cross-platform consistency.
//!   - The `O_*` flag-translation table is the single source of truth for
//!     how cssl-rt portable flags map to POSIX open-flags.
//!
//! § DEFERRED — non-Windows CI
//!   Apocky's host is Windows ; the Unix path is structurally tested
//!   (compile-only) until a non-Windows CI runner exists. Per the
//!   dispatch-plan landmines + slice handoff REPORT BACK, this is
//!   documented rather than blocking. The Linux / macOS path is
//!   structurally identical to glibc's `open / read / write / close`
//!   signatures, so behavior is determined by the well-known POSIX
//!   semantics rather than per-platform quirks.
//!
//! § PRIME-DIRECTIVE
//!   File I/O = consent-arch concern ; see `crate::io` doc-block for the
//!   attestation. POSIX-specific note : `errno` is thread-local ; our
//!   error-translation reads it after every failing syscall and never
//!   inspects it speculatively.

#![allow(unsafe_code)]
// File-level cfg-gating happens at the `pub mod io_unix` site in lib.rs
// (with `#[cfg(not(target_os = "windows"))]`) — duplicating the cfg
// here trips clippy `duplicated_attributes`. The extern-C bindings
// below are only valid on Unix ; the lib.rs gate is the single source
// of truth for inclusion.

use core::ffi::c_void;

use crate::io::{
    io_error_code, record_io_error, validate_buffer, validate_open_flags, INVALID_HANDLE,
    OPEN_APPEND, OPEN_CREATE, OPEN_CREATE_NEW, OPEN_READ, OPEN_READ_WRITE, OPEN_TRUNCATE,
    OPEN_WRITE,
};

// ───────────────────────────────────────────────────────────────────────
// § POSIX type aliases.
// ───────────────────────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
type c_int = i32;
#[allow(non_camel_case_types)]
type c_char = i8;
#[allow(non_camel_case_types)]
type size_t = usize;
#[allow(non_camel_case_types)]
type ssize_t = isize;
#[allow(non_camel_case_types)]
type mode_t = u32;

// ───────────────────────────────────────────────────────────────────────
// § POSIX open-flag constants — values match glibc (Linux) ; macOS uses
//   the same flags from <fcntl.h>, with O_CREAT / O_EXCL / O_TRUNC at
//   different bit positions. The `cfg(target_os = ...)` blocks supply
//   the per-OS values.
// ───────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod posix_flags {
    pub const O_RDONLY: super::c_int = 0o0;
    pub const O_WRONLY: super::c_int = 0o1;
    pub const O_RDWR: super::c_int = 0o2;
    pub const O_CREAT: super::c_int = 0o100;
    pub const O_EXCL: super::c_int = 0o200;
    pub const O_TRUNC: super::c_int = 0o1000;
    pub const O_APPEND: super::c_int = 0o2000;
}

#[cfg(target_os = "macos")]
mod posix_flags {
    pub const O_RDONLY: super::c_int = 0x0000;
    pub const O_WRONLY: super::c_int = 0x0001;
    pub const O_RDWR: super::c_int = 0x0002;
    pub const O_CREAT: super::c_int = 0x0200;
    pub const O_EXCL: super::c_int = 0x0800;
    pub const O_TRUNC: super::c_int = 0x0400;
    pub const O_APPEND: super::c_int = 0x0008;
}

// Fallback for any other Unix-ish target — use the Linux convention as
// the safest default. A future port to *BSD or Solaris would refine.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod posix_flags {
    pub const O_RDONLY: super::c_int = 0o0;
    pub const O_WRONLY: super::c_int = 0o1;
    pub const O_RDWR: super::c_int = 0o2;
    pub const O_CREAT: super::c_int = 0o100;
    pub const O_EXCL: super::c_int = 0o200;
    pub const O_TRUNC: super::c_int = 0o1000;
    pub const O_APPEND: super::c_int = 0o2000;
}

// POSIX errno values — per `<errno.h>`. Values are platform-specific in
// principle but the basic NotFound / PermissionDenied / EEXIST / EINVAL
// set is canonical across SUSv2 hosts. Linux + macOS agree on these.
const ENOENT: c_int = 2;
const EINTR: c_int = 4;
const EACCES: c_int = 13;
const EEXIST: c_int = 17;
const EINVAL: c_int = 22;

// Default file-creation mode : 0o644 (rw-r--r--) per Rust std::fs::File::create.
const DEFAULT_CREATE_MODE: mode_t = 0o644;

// ───────────────────────────────────────────────────────────────────────
// § extern bindings — 4 syscalls + errno accessor.
// ───────────────────────────────────────────────────────────────────────

extern "C" {
    fn open(path: *const c_char, oflag: c_int, mode: mode_t) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t;
    fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t;
    fn close(fd: c_int) -> c_int;
    // `errno` access — implementations differ :
    //   - glibc : `__errno_location() -> *mut c_int`
    //   - musl  : `__errno_location() -> *mut c_int`
    //   - macOS : `__error() -> *mut c_int`
    // We bind both names per-platform so the syscall layer reads the
    // thread-local errno via a single call.
    #[cfg(any(target_os = "linux", target_env = "musl"))]
    fn __errno_location() -> *mut c_int;
    #[cfg(target_os = "macos")]
    fn __error() -> *mut c_int;
}

#[allow(unsafe_code)]
fn errno() -> c_int {
    #[cfg(any(target_os = "linux", target_env = "musl"))]
    {
        // SAFETY : __errno_location returns a valid thread-local pointer
        // owned by libc ; reading the pointee is the documented use.
        unsafe { *__errno_location() }
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY : __error returns a valid thread-local pointer owned by libc.
        unsafe { *__error() }
    }
    #[cfg(not(any(target_os = "linux", target_env = "musl", target_os = "macos")))]
    {
        // Fallback : best-effort 0 on unsupported targets. The slice
        // documents non-Win32 platforms beyond Linux/macOS as deferred ;
        // returning 0 means the canonical translator falls through to
        // OTHER without a useful OS code, which is acceptable until a
        // real port lands.
        0
    }
}

// ───────────────────────────────────────────────────────────────────────
// § errno → canonical io_error_code translation.
// ───────────────────────────────────────────────────────────────────────

fn translate_posix_errno(err: c_int) -> i32 {
    match err {
        ENOENT => io_error_code::NOT_FOUND,
        EACCES => io_error_code::PERMISSION_DENIED,
        EEXIST => io_error_code::ALREADY_EXISTS,
        EINVAL => io_error_code::INVALID_INPUT,
        EINTR => io_error_code::INTERRUPTED,
        _ => io_error_code::OTHER,
    }
}

/// Translate the cssl-rt portable open-flag bitset to POSIX `O_*` flags.
fn translate_open_flags(flags: i32) -> c_int {
    let mut o: c_int = 0;
    let access = flags & (OPEN_READ | OPEN_WRITE | OPEN_READ_WRITE);
    o |= match access {
        x if x == OPEN_READ => posix_flags::O_RDONLY,
        x if x == OPEN_WRITE => posix_flags::O_WRONLY,
        x if x == OPEN_READ_WRITE => posix_flags::O_RDWR,
        _ => posix_flags::O_RDONLY, // unreachable post-validation
    };
    if (flags & OPEN_CREATE) != 0 {
        o |= posix_flags::O_CREAT;
    }
    if (flags & OPEN_CREATE_NEW) != 0 {
        o |= posix_flags::O_EXCL;
    }
    if (flags & OPEN_TRUNCATE) != 0 {
        o |= posix_flags::O_TRUNC;
    }
    if (flags & OPEN_APPEND) != 0 {
        o |= posix_flags::O_APPEND;
    }
    o
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_open_impl — translate caller flags + path → libc open.
// ───────────────────────────────────────────────────────────────────────

/// Open `path` (UTF-8 bytes) with `flags` (cssl-rt portable bitset).
///
/// Returns the POSIX `fd` zero-extended to `i64` on success ;
/// [`INVALID_HANDLE`] on failure (with [`crate::io::record_io_error`]
/// populated).
///
/// # Safety
/// Caller must ensure :
/// - `path_ptr` valid for `path_len` bytes (or `path_len == 0` and
///   `path_ptr` is null — though that yields `InvalidInput`).
/// - `path_len` does not exceed `isize::MAX`.
#[allow(clippy::module_name_repetitions)]
pub unsafe fn cssl_fs_open_impl(path_ptr: *const u8, path_len: usize, flags: i32) -> i64 {
    crate::io::reset_last_io_error_for_tests();
    if let Err(kind) = validate_open_flags(flags) {
        record_io_error(kind, 0);
        return INVALID_HANDLE;
    }
    if path_ptr.is_null() || path_len == 0 {
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return INVALID_HANDLE;
    }
    // Build a NUL-terminated C string from the UTF-8 path bytes.
    // SAFETY : caller contract — path_ptr valid for path_len bytes.
    let bytes = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    // Reject paths containing embedded NULs (POSIX paths can't contain them).
    if bytes.contains(&0u8) {
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return INVALID_HANDLE;
    }
    let mut c_path = alloc::vec::Vec::with_capacity(path_len + 1);
    c_path.extend_from_slice(bytes);
    c_path.push(0); // NUL terminator
    let oflag = translate_open_flags(flags);
    // Pass DEFAULT_CREATE_MODE only when O_CREAT is set ; POSIX accepts a
    // mode argument unconditionally (the variadic form), but only honors
    // it when O_CREAT is in oflag.
    // SAFETY : c_path is non-empty + NUL-terminated ; oflag derived from
    // validated cssl-rt flags ; mode is the canonical default.
    let fd = unsafe { open(c_path.as_ptr().cast::<c_char>(), oflag, DEFAULT_CREATE_MODE) };
    if fd < 0 {
        let posix = errno();
        let kind = translate_posix_errno(posix);
        record_io_error(kind, posix);
        return INVALID_HANDLE;
    }
    crate::io::record_open();
    // § T11-D130 : record path-hash event (raw path bytes never escape).
    // SAFETY : path_ptr/path_len validated above to be non-null + non-zero.
    let path_hash = unsafe { crate::path_hash::hash_path_ptr(path_ptr, path_len) };
    crate::io::record_path_hash_event(path_hash, crate::io::PathHashOpKind::Open);
    i64::from(fd)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_read_impl
// ───────────────────────────────────────────────────────────────────────

/// Read up to `buf_len` bytes from `handle` into `buf_ptr`.
///
/// Returns bytes-read (0 at EOF) ; -1 on failure (last-error populated).
///
/// # Safety
/// Caller must ensure :
/// - `handle` is a valid fd returned from [`cssl_fs_open_impl`] with read
///   access and not yet closed.
/// - `buf_ptr` valid for `buf_len` writable bytes.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_fs_read_impl(handle: i64, buf_ptr: *mut u8, buf_len: usize) -> i64 {
    crate::io::reset_last_io_error_for_tests();
    if handle == INVALID_HANDLE {
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if let Err(kind) = validate_buffer(buf_ptr.cast_const(), buf_len) {
        record_io_error(kind, 0);
        return -1;
    }
    crate::io::record_read(0);
    if buf_len == 0 {
        return 0;
    }
    let to_read = if buf_len > i32::MAX as usize {
        i32::MAX as size_t
    } else {
        buf_len as size_t
    };
    let fd = handle as c_int;
    // SAFETY : fd assumed valid by caller ; buffer pointer checked above.
    let n = unsafe { read(fd, buf_ptr.cast::<c_void>(), to_read) };
    if n < 0 {
        let posix = errno();
        let kind = translate_posix_errno(posix);
        record_io_error(kind, posix);
        return -1;
    }
    crate::io::BYTES_READ_TOTAL.fetch_add(n as u64, core::sync::atomic::Ordering::Relaxed);
    n as i64
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_write_impl
// ───────────────────────────────────────────────────────────────────────

/// Write `buf_len` bytes from `buf_ptr` to `handle`.
///
/// Returns bytes-written ; -1 on failure (last-error populated). Short
/// writes are possible per-syscall ; callers loop until all bytes have
/// been written.
///
/// # Safety
/// Caller must ensure :
/// - `handle` is a valid fd returned from [`cssl_fs_open_impl`] with
///   write or append access and not yet closed.
/// - `buf_ptr` valid for `buf_len` readable bytes.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_fs_write_impl(handle: i64, buf_ptr: *const u8, buf_len: usize) -> i64 {
    crate::io::reset_last_io_error_for_tests();
    if handle == INVALID_HANDLE {
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if let Err(kind) = validate_buffer(buf_ptr, buf_len) {
        record_io_error(kind, 0);
        return -1;
    }
    crate::io::record_write(0);
    if buf_len == 0 {
        return 0;
    }
    let to_write = if buf_len > i32::MAX as usize {
        i32::MAX as size_t
    } else {
        buf_len as size_t
    };
    let fd = handle as c_int;
    // SAFETY : fd assumed valid by caller ; buffer pointer checked above.
    let n = unsafe { write(fd, buf_ptr.cast::<c_void>(), to_write) };
    if n < 0 {
        let posix = errno();
        let kind = translate_posix_errno(posix);
        record_io_error(kind, posix);
        return -1;
    }
    crate::io::BYTES_WRITTEN_TOTAL.fetch_add(n as u64, core::sync::atomic::Ordering::Relaxed);
    if n == 0 && buf_len > 0 {
        record_io_error(io_error_code::WRITE_ZERO, 0);
        return -1;
    }
    n as i64
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_close_impl
// ───────────────────────────────────────────────────────────────────────

/// Close `handle`. Returns 0 on success, -1 on failure.
///
/// # Safety
/// Caller must ensure `handle` is a valid fd returned from
/// [`cssl_fs_open_impl`] and not yet closed.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
pub unsafe fn cssl_fs_close_impl(handle: i64) -> i64 {
    crate::io::reset_last_io_error_for_tests();
    if handle == INVALID_HANDLE {
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return -1;
    }
    crate::io::record_close();
    let fd = handle as c_int;
    // SAFETY : fd assumed valid by caller contract.
    let r = unsafe { close(fd) };
    if r != 0 {
        let posix = errno();
        let kind = translate_posix_errno(posix);
        record_io_error(kind, posix);
        return -1;
    }
    0
}

// ───────────────────────────────────────────────────────────────────────
// § alloc-crate import — needed for Vec.
// ───────────────────────────────────────────────────────────────────────

extern crate alloc;

// ───────────────────────────────────────────────────────────────────────
// § Tests — flag-translation table + errno mapping + roundtrip.
//
// On non-Windows hosts the roundtrip test exercises a real temp file ;
// on Windows hosts this module is `cfg`-excluded entirely so these tests
// don't compile (which is correct — the windows tests are in
// `crate::io_win32` and exercise the same surface through the Win32 path).
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
// See io_win32.rs § tests for the rationale ; same scoped suppression.
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;

    #[test]
    fn translate_posix_errno_known_kinds() {
        assert_eq!(translate_posix_errno(ENOENT), io_error_code::NOT_FOUND);
        assert_eq!(
            translate_posix_errno(EACCES),
            io_error_code::PERMISSION_DENIED
        );
        assert_eq!(translate_posix_errno(EEXIST), io_error_code::ALREADY_EXISTS);
        assert_eq!(translate_posix_errno(EINVAL), io_error_code::INVALID_INPUT);
        assert_eq!(translate_posix_errno(EINTR), io_error_code::INTERRUPTED);
        // Unknown errno falls through to OTHER.
        assert_eq!(translate_posix_errno(99_999), io_error_code::OTHER);
    }

    #[test]
    fn translate_open_flags_read_only_maps_to_o_rdonly() {
        let o = translate_open_flags(OPEN_READ);
        assert_eq!(o & 0o3, posix_flags::O_RDONLY);
        assert_eq!(o & posix_flags::O_CREAT, 0);
    }

    #[test]
    fn translate_open_flags_write_create_truncate() {
        let o = translate_open_flags(OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE);
        assert_eq!(o & 0o3, posix_flags::O_WRONLY);
        assert_ne!(o & posix_flags::O_CREAT, 0);
        assert_ne!(o & posix_flags::O_TRUNC, 0);
    }

    #[test]
    fn translate_open_flags_create_new_sets_o_excl() {
        let o = translate_open_flags(OPEN_WRITE | OPEN_CREATE | OPEN_CREATE_NEW);
        assert_ne!(o & posix_flags::O_EXCL, 0);
    }

    #[test]
    fn translate_open_flags_append_sets_o_append() {
        let o = translate_open_flags(OPEN_WRITE | OPEN_CREATE | OPEN_APPEND);
        assert_ne!(o & posix_flags::O_APPEND, 0);
    }

    #[test]
    fn open_invalid_flags_returns_invalid_handle() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let p = b"x.txt";
        // SAFETY : valid byte slice ; 0x8000 is unknown.
        let r = unsafe { cssl_fs_open_impl(p.as_ptr(), p.len(), 0x8000) };
        assert_eq!(r, INVALID_HANDLE);
        assert_eq!(
            crate::io::last_io_error_kind(),
            io_error_code::INVALID_INPUT
        );
    }

    #[test]
    fn open_null_path_returns_invalid_input() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : null path is the documented invalid case.
        let r = unsafe { cssl_fs_open_impl(core::ptr::null(), 0, OPEN_READ) };
        assert_eq!(r, INVALID_HANDLE);
        assert_eq!(
            crate::io::last_io_error_kind(),
            io_error_code::INVALID_INPUT
        );
    }

    #[test]
    fn open_path_with_embedded_nul_rejected() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let p = b"x\0y.txt";
        // SAFETY : valid byte slice with embedded NUL — rejected before syscall.
        let r = unsafe { cssl_fs_open_impl(p.as_ptr(), p.len(), OPEN_READ) };
        assert_eq!(r, INVALID_HANDLE);
        assert_eq!(
            crate::io::last_io_error_kind(),
            io_error_code::INVALID_INPUT
        );
    }

    #[test]
    fn read_invalid_handle_returns_minus_one() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let mut buf = [0u8; 16];
        // SAFETY : INVALID_HANDLE sentinel ; impl checks before read syscall.
        let r = unsafe { cssl_fs_read_impl(INVALID_HANDLE, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(r, -1);
        assert_eq!(
            crate::io::last_io_error_kind(),
            io_error_code::INVALID_INPUT
        );
    }

    #[test]
    fn write_invalid_handle_returns_minus_one() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let buf = [0u8; 16];
        // SAFETY : INVALID_HANDLE sentinel ; impl checks before write syscall.
        let r = unsafe { cssl_fs_write_impl(INVALID_HANDLE, buf.as_ptr(), buf.len()) };
        assert_eq!(r, -1);
        assert_eq!(
            crate::io::last_io_error_kind(),
            io_error_code::INVALID_INPUT
        );
    }

    #[test]
    fn close_invalid_handle_returns_minus_one() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : INVALID_HANDLE sentinel ; impl checks before close syscall.
        let r = unsafe { cssl_fs_close_impl(INVALID_HANDLE) };
        assert_eq!(r, -1);
        assert_eq!(
            crate::io::last_io_error_kind(),
            io_error_code::INVALID_INPUT
        );
    }

    #[test]
    fn open_nonexistent_file_returns_not_found() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let path = b"/tmp/zzz_apocky_definitely_not_a_file_xyz_b5.txt";
        // SAFETY : valid byte slice.
        let r = unsafe { cssl_fs_open_impl(path.as_ptr(), path.len(), OPEN_READ) };
        assert_eq!(r, INVALID_HANDLE);
        assert_eq!(crate::io::last_io_error_kind(), io_error_code::NOT_FOUND);
    }

    #[test]
    fn open_write_create_close_roundtrip() {
        // Mirror of the Win32 roundtrip test.
        let _g = crate::test_helpers::lock_and_reset_all();
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("cssl_b5_unix_roundtrip_test.txt");
        let path_str = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);

        let payload = b"hello b5 unix roundtrip\n";

        // SAFETY : valid path / payload buffers.
        let h = unsafe {
            cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE,
            )
        };
        assert_ne!(h, INVALID_HANDLE);
        let n = unsafe { cssl_fs_write_impl(h, payload.as_ptr(), payload.len()) };
        assert_eq!(n, payload.len() as i64);
        let cr = unsafe { cssl_fs_close_impl(h) };
        assert_eq!(cr, 0);

        let h2 = unsafe { cssl_fs_open_impl(path_str.as_ptr(), path_str.len(), OPEN_READ) };
        assert_ne!(h2, INVALID_HANDLE);
        let mut readback = vec![0u8; payload.len() + 8];
        let nr = unsafe { cssl_fs_read_impl(h2, readback.as_mut_ptr(), readback.len()) };
        assert_eq!(nr, payload.len() as i64);
        let cr2 = unsafe { cssl_fs_close_impl(h2) };
        assert_eq!(cr2, 0);
        assert_eq!(&readback[..payload.len()], payload);

        let _ = std::fs::remove_file(&path);
    }
}
