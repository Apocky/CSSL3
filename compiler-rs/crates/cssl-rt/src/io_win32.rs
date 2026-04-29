//! § cssl-rt file-I/O — Windows platform layer (T11-D76, S6-B5).
//!
//! § ROLE
//!   Hand-rolled Win32 syscall bindings for `CreateFileW` /
//!   `ReadFile` / `WriteFile` / `CloseHandle` / `GetLastError`.
//!   Per the dispatch-plan landmines we deliberately avoid pulling in
//!   the `windows-sys` crate at this slice — the FFI surface is small +
//!   stable since Windows 95, and adding a heavyweight binding crate
//!   would require its own DECISIONS sub-entry. The cssl-rt convention
//!   established at T11-D52 is "hand-rolled `extern "C"` declarations
//!   with each `unsafe` block carrying an inline SAFETY paragraph".
//!
//! § WIN32 ABI NOTES
//!   - `HANDLE` = pointer-sized opaque ; we model as `*mut core::ffi::c_void`
//!     and cast to `i64` at the FFI boundary. `INVALID_HANDLE_VALUE` =
//!     `(HANDLE)-1` ≡ `usize::MAX` cast to `*mut c_void` ≡ `-1` as `i64`.
//!   - `BOOL` = 32-bit `i32` ; 0 = FALSE / failure, non-zero = TRUE / success.
//!   - `DWORD` = 32-bit `u32` ; widely used for sizes + flags.
//!   - `LPCWSTR` = wide-char pointer ; stage-0 paths come through
//!     [`crate::io::utf8_to_utf16_lossy`] which appends a NUL terminator
//!     so the buffer is C-string-compatible.
//!   - All syscalls below set thread-local `GetLastError` on failure ; we
//!     translate the Win32 error code into a canonical
//!     [`crate::io::io_error_code`] discriminant for the per-thread slot.
//!
//! § INVARIANTS
//!   - On failure, every `cssl_fs_*_impl` returns `-1 as i64` AND records
//!     the canonical error in [`crate::io::record_io_error`]. The high 32
//!     bits of the slot carry the raw `GetLastError` value for diagnostic
//!     consumers.
//!   - Read / write byte counts are bounded to `i32::MAX` per call to keep
//!     the Win32 `DWORD` (`u32`) parameter unambiguous + match Linux's
//!     `ssize_t` ceiling.
//!   - The `CreateFileW` flag-translation table is the single source of
//!     truth for how cssl-rt portable flags map to Win32 access /
//!     creation-disposition / sharing.
//!
//! § PRIME-DIRECTIVE
//!   File I/O = consent-arch concern ; see `crate::io` doc-block for the
//!   attestation. Win32-specific note : `CreateFileW` is the wide-char form
//!   so Unicode paths work — which is correct + required, not a side
//!   channel.

#![allow(unsafe_code)]
// Win32 type aliases (HANDLE / DWORD / BOOL / LPCWSTR / LPVOID / LPCVOID /
// LPDWORD) and constant names (CREATE_NEW / GENERIC_READ / etc.) are
// upper-case acronyms by Win32 SDK convention — keep them matching the
// header filenames + MSDN reference so the FFI surface is auditable
// against the official SDK without renaming. This is a permanent
// invariant of the file ; future Win32 additions follow the same rule.
#![allow(clippy::upper_case_acronyms)]
// File-level cfg-gating happens at the `pub mod io_win32` site in
// lib.rs (with `#[cfg(target_os = "windows")]`) — duplicating the
// cfg here trips clippy `duplicated_attributes`. The
// extern-system bindings below are only valid on Windows ; the lib.rs
// gate is the single source of truth for inclusion.

use core::ffi::c_void;

use crate::io::{
    io_error_code, record_io_error, validate_buffer, validate_open_flags, INVALID_HANDLE,
    OPEN_APPEND, OPEN_CREATE, OPEN_CREATE_NEW, OPEN_READ, OPEN_READ_WRITE, OPEN_TRUNCATE,
    OPEN_WRITE,
};

// ───────────────────────────────────────────────────────────────────────
// § Win32 type aliases — match the win32 SDK headers but keep them plain
//   primitive types so we don't need a binding crate.
// ───────────────────────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
type HANDLE = *mut c_void;
#[allow(non_camel_case_types)]
type DWORD = u32;
#[allow(non_camel_case_types)]
type BOOL = i32;
#[allow(non_camel_case_types)]
type LPCWSTR = *const u16;
#[allow(non_camel_case_types)]
type LPVOID = *mut c_void;
#[allow(non_camel_case_types)]
type LPCVOID = *const c_void;
#[allow(non_camel_case_types)]
type LPDWORD = *mut DWORD;

// `INVALID_HANDLE_VALUE = (HANDLE)-1` — the canonical Win32 error sentinel.
#[allow(clippy::as_conversions)]
const INVALID_HANDLE_VALUE: HANDLE = (-1_isize) as HANDLE;

// CreateFileW dwDesiredAccess flags.
const GENERIC_READ: DWORD = 0x8000_0000;
const GENERIC_WRITE: DWORD = 0x4000_0000;
const FILE_APPEND_DATA: DWORD = 0x0004;

// CreateFileW dwShareMode — match Rust std::fs default (read+write share).
const FILE_SHARE_READ: DWORD = 0x0000_0001;
const FILE_SHARE_WRITE: DWORD = 0x0000_0002;
const FILE_SHARE_DELETE: DWORD = 0x0000_0004;

// CreateFileW dwCreationDisposition.
const CREATE_NEW: DWORD = 1;
const CREATE_ALWAYS: DWORD = 2;
const OPEN_EXISTING: DWORD = 3;
const OPEN_ALWAYS: DWORD = 4;
const TRUNCATE_EXISTING: DWORD = 5;

// CreateFileW dwFlagsAndAttributes.
const FILE_ATTRIBUTE_NORMAL: DWORD = 0x0000_0080;

// SetFilePointer move methods (used to seek-to-end on append).
const FILE_END: DWORD = 2;
const INVALID_SET_FILE_POINTER: DWORD = 0xFFFF_FFFF;

// Win32 GetLastError codes — full reference at
// learn.microsoft.com / windows / win32 / debug / system-error-codes-base.
const ERROR_FILE_NOT_FOUND: DWORD = 2;
const ERROR_PATH_NOT_FOUND: DWORD = 3;
const ERROR_ACCESS_DENIED: DWORD = 5;
const ERROR_FILE_EXISTS: DWORD = 80;
const ERROR_INVALID_PARAMETER: DWORD = 87;
const ERROR_ALREADY_EXISTS: DWORD = 183;

// ───────────────────────────────────────────────────────────────────────
// § extern bindings — 5 syscalls (CreateFileW / ReadFile / WriteFile /
//   CloseHandle / GetLastError / SetFilePointer).
//
//   ‼ The kernel32.dll → KERNELBASE.dll forwarders make these stable
//   since Windows XP. SDK headers wrap them in `WINAPI` (`stdcall` on
//   x86 / `extern "C"` on x64) — we use `extern "system"` which Rust
//   maps to the same calling convention per platform.
// ───────────────────────────────────────────────────────────────────────

extern "system" {
    fn CreateFileW(
        lp_filename: LPCWSTR,
        dw_desired_access: DWORD,
        dw_share_mode: DWORD,
        lp_security_attributes: *const c_void,
        dw_creation_disposition: DWORD,
        dw_flags_and_attributes: DWORD,
        h_template_file: HANDLE,
    ) -> HANDLE;

    fn ReadFile(
        h_file: HANDLE,
        lp_buffer: LPVOID,
        n_number_of_bytes_to_read: DWORD,
        lp_number_of_bytes_read: LPDWORD,
        lp_overlapped: *mut c_void,
    ) -> BOOL;

    fn WriteFile(
        h_file: HANDLE,
        lp_buffer: LPCVOID,
        n_number_of_bytes_to_write: DWORD,
        lp_number_of_bytes_written: LPDWORD,
        lp_overlapped: *mut c_void,
    ) -> BOOL;

    fn CloseHandle(h_object: HANDLE) -> BOOL;

    fn GetLastError() -> DWORD;

    fn SetFilePointer(
        h_file: HANDLE,
        l_distance_to_move: i32,
        lp_distance_to_move_high: *mut i32,
        dw_move_method: DWORD,
    ) -> DWORD;
}

// ───────────────────────────────────────────────────────────────────────
// § Win32 error → canonical io_error_code translation.
// ───────────────────────────────────────────────────────────────────────

/// Translate a Win32 `GetLastError` code into the canonical
/// [`crate::io::io_error_code`] discriminant. Unknown codes fall through
/// to [`io_error_code::OTHER`] with the OS code preserved verbatim.
#[allow(clippy::cast_possible_wrap)]
fn translate_win32_error(code: DWORD) -> i32 {
    match code {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => io_error_code::NOT_FOUND,
        ERROR_ACCESS_DENIED => io_error_code::PERMISSION_DENIED,
        ERROR_FILE_EXISTS | ERROR_ALREADY_EXISTS => io_error_code::ALREADY_EXISTS,
        ERROR_INVALID_PARAMETER => io_error_code::INVALID_INPUT,
        _ => io_error_code::OTHER,
    }
}

/// Translate the cssl-rt portable open-flag bitset to the Win32 quartet
/// `(dwDesiredAccess, dwShareMode, dwCreationDisposition, append)`.
///
/// Stage-0 sharing : `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`
/// matches Rust's std::fs default (other processes may read / write while
/// we hold the handle ; this is the least-surprise default).
fn translate_open_flags(flags: i32) -> (DWORD, DWORD, DWORD, bool) {
    let mut access: DWORD = 0;
    let mut append = false;
    if (flags & OPEN_READ) != 0 {
        access |= GENERIC_READ;
    }
    if (flags & OPEN_WRITE) != 0 {
        access |= GENERIC_WRITE;
    }
    if (flags & OPEN_READ_WRITE) != 0 {
        access |= GENERIC_READ | GENERIC_WRITE;
    }
    if (flags & OPEN_APPEND) != 0 {
        // Append mode : strip GENERIC_WRITE (caller can't randomly write)
        // and set FILE_APPEND_DATA. This matches the Win32-recommended
        // append-only access mode per MSDN.
        access &= !GENERIC_WRITE;
        access |= FILE_APPEND_DATA;
        append = true;
    }

    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    let disposition: DWORD = match (
        (flags & OPEN_CREATE) != 0,
        (flags & OPEN_CREATE_NEW) != 0,
        (flags & OPEN_TRUNCATE) != 0,
    ) {
        // CREATE_NEW + CREATE — fail if exists.
        (true, true, _) => CREATE_NEW,
        // CREATE + TRUNCATE — replace if exists, else create.
        (true, false, true) => CREATE_ALWAYS,
        // CREATE — open if exists, else create.
        (true, false, false) => OPEN_ALWAYS,
        // No-create + TRUNCATE — must exist + truncate.
        (false, false, true) => TRUNCATE_EXISTING,
        // Plain open — must exist.
        (false, _, _) => OPEN_EXISTING,
    };

    (access, share, disposition, append)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_open_impl — translate caller flags + path → CreateFileW call.
// ───────────────────────────────────────────────────────────────────────

/// Open `path` (UTF-8 bytes) with `flags` (cssl-rt portable bitset).
///
/// Returns the Win32 `HANDLE` cast to `i64` on success ; [`INVALID_HANDLE`]
/// on failure (with [`crate::io::record_io_error`] populated).
///
/// # Safety
/// Caller must ensure :
/// - `path_ptr` valid for `path_len` bytes (or `path_len == 0` and `path_ptr`
///   is null — though that yields `InvalidInput`).
/// - `path_len` does not exceed `isize::MAX`.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
pub unsafe fn cssl_fs_open_impl(path_ptr: *const u8, path_len: usize, flags: i32) -> i64 {
    crate::io::reset_last_io_error_for_tests();

    // 1. Validate flags.
    if let Err(kind) = validate_open_flags(flags) {
        record_io_error(kind, 0);
        return INVALID_HANDLE;
    }
    // 2. Validate path.
    if path_ptr.is_null() || path_len == 0 {
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return INVALID_HANDLE;
    }
    // 3. Translate flags to Win32 quartet.
    let (access, share, disposition, append) = translate_open_flags(flags);
    // 4. Translate UTF-8 path → UTF-16 wide-char NUL-terminated.
    // SAFETY : caller contract — path_ptr valid for path_len bytes.
    let utf16 = unsafe { crate::io::utf8_to_utf16_lossy(path_ptr, path_len) };
    if utf16.len() <= 1 {
        // Empty after conversion (only the NUL).
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return INVALID_HANDLE;
    }
    // 5. Invoke CreateFileW.
    // SAFETY : utf16 is non-empty and NUL-terminated ; access / share /
    // disposition come from the validated flag-translation table ;
    // template + security attribs are null which is the std default.
    let handle = unsafe {
        CreateFileW(
            utf16.as_ptr(),
            access,
            share,
            core::ptr::null(),
            disposition,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        // SAFETY : GetLastError is a thread-local with no preconditions.
        let win32_err = unsafe { GetLastError() };
        let kind = translate_win32_error(win32_err);
        record_io_error(kind, win32_err as i32);
        return INVALID_HANDLE;
    }
    // 6. Append mode : seek to EOF post-open so subsequent WriteFile
    //    appends rather than overwriting.
    if append {
        // SAFETY : handle is valid (just-opened) ; SetFilePointer with
        // FILE_END moves to the end ; lp_distance_to_move_high = null
        // bounds the seek to the low 32 bits which is sufficient for
        // stage-0 (test files are << 4GiB).
        let pos = unsafe { SetFilePointer(handle, 0, core::ptr::null_mut(), FILE_END) };
        if pos == INVALID_SET_FILE_POINTER {
            // SAFETY : GetLastError is a thread-local with no preconditions.
            let win32_err = unsafe { GetLastError() };
            // Best-effort cleanup — close the handle we just opened.
            // SAFETY : handle is the valid HANDLE we received from CreateFileW.
            let _ = unsafe { CloseHandle(handle) };
            record_io_error(io_error_code::OTHER, win32_err as i32);
            return INVALID_HANDLE;
        }
    }
    crate::io::record_open();
    // § T11-D130 : record path-hash event for the audit-bus drain. The
    // raw path bytes never escape this fn ; the hash is computed in the
    // upper-half (`hash_path_ptr`) which itself salts + BLAKE3-hashes
    // before any other recording occurs.
    // SAFETY : path_ptr/path_len validated above to be non-null + non-zero.
    let path_hash = unsafe { crate::path_hash::hash_path_ptr(path_ptr, path_len) };
    crate::io::record_path_hash_event(path_hash, crate::io::PathHashOpKind::Open);
    // Win32 HANDLE → i64 — round-trip preserves the bit-pattern.
    handle as i64
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_read_impl — read up to `buf_len` bytes into `buf_ptr`.
// ───────────────────────────────────────────────────────────────────────

/// Read up to `buf_len` bytes from `handle` into `buf_ptr`.
///
/// Returns the number of bytes actually read (0 at EOF) ; on syscall
/// failure returns -1 with [`crate::io::record_io_error`] populated.
///
/// # Safety
/// Caller must ensure :
/// - `handle` is a valid HANDLE returned from [`cssl_fs_open_impl`] with
///   read access and not yet closed.
/// - `buf_ptr` valid for `buf_len` consecutive writable bytes.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
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
    crate::io::record_read(0); // record the call ; bytes adjusted on success
    if buf_len == 0 {
        return 0;
    }
    // Bound buf_len to i32::MAX so the DWORD parameter is unambiguous.
    let to_read = if buf_len > i32::MAX as usize {
        i32::MAX as DWORD
    } else {
        buf_len as DWORD
    };
    let mut bytes_read: DWORD = 0;
    // SAFETY : handle assumed valid by caller contract ; buffer pointer
    // checked above ; lp_overlapped null = synchronous I/O.
    let ok = unsafe {
        ReadFile(
            handle as HANDLE,
            buf_ptr.cast::<c_void>(),
            to_read,
            &mut bytes_read,
            core::ptr::null_mut(),
        )
    };
    if ok == 0 {
        // SAFETY : GetLastError is a thread-local with no preconditions.
        let win32_err = unsafe { GetLastError() };
        let kind = translate_win32_error(win32_err);
        record_io_error(kind, win32_err as i32);
        return -1;
    }
    // Adjust the bytes-read counter (record_read above counted the call
    // with 0 bytes ; add the actual amount post-syscall).
    crate::io::BYTES_READ_TOTAL
        .fetch_add(u64::from(bytes_read), core::sync::atomic::Ordering::Relaxed);
    i64::from(bytes_read)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_write_impl — write `buf_len` bytes from `buf_ptr`.
// ───────────────────────────────────────────────────────────────────────

/// Write `buf_len` bytes from `buf_ptr` to `handle`.
///
/// Returns the number of bytes actually written ; on failure returns -1
/// with [`crate::io::record_io_error`] populated. A short write is
/// possible per-syscall ; callers (stdlib/fs.cssl::write_all) loop until
/// all bytes have been written.
///
/// # Safety
/// Caller must ensure :
/// - `handle` is a valid HANDLE returned from [`cssl_fs_open_impl`] with
///   write or append access and not yet closed.
/// - `buf_ptr` valid for `buf_len` consecutive readable bytes.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
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
    crate::io::record_write(0); // record the call ; bytes adjusted on success
    if buf_len == 0 {
        return 0;
    }
    let to_write = if buf_len > i32::MAX as usize {
        i32::MAX as DWORD
    } else {
        buf_len as DWORD
    };
    let mut bytes_written: DWORD = 0;
    // SAFETY : handle valid per caller contract ; buffer pointer checked
    // above ; lp_overlapped null = synchronous.
    let ok = unsafe {
        WriteFile(
            handle as HANDLE,
            buf_ptr.cast::<c_void>(),
            to_write,
            &mut bytes_written,
            core::ptr::null_mut(),
        )
    };
    if ok == 0 {
        // SAFETY : GetLastError is a thread-local with no preconditions.
        let win32_err = unsafe { GetLastError() };
        let kind = translate_win32_error(win32_err);
        record_io_error(kind, win32_err as i32);
        return -1;
    }
    // Adjust the bytes-written counter (record_write above counted the
    // call with 0 bytes ; add the actual amount post-syscall).
    crate::io::BYTES_WRITTEN_TOTAL.fetch_add(
        u64::from(bytes_written),
        core::sync::atomic::Ordering::Relaxed,
    );
    if bytes_written == 0 && buf_len > 0 {
        record_io_error(io_error_code::WRITE_ZERO, 0);
        return -1;
    }
    i64::from(bytes_written)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_fs_close_impl — close a previously-opened handle.
// ───────────────────────────────────────────────────────────────────────

/// Close `handle`. Returns 0 on success, -1 on failure (with the canonical
/// error in [`crate::io::record_io_error`]).
///
/// Closing `INVALID_HANDLE` is treated as `InvalidInput` (not a no-op) so
/// double-close is loud rather than silent.
///
/// # Safety
/// Caller must ensure `handle` is a valid HANDLE returned from
/// [`cssl_fs_open_impl`] and not yet closed.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
pub unsafe fn cssl_fs_close_impl(handle: i64) -> i64 {
    crate::io::reset_last_io_error_for_tests();
    if handle == INVALID_HANDLE {
        record_io_error(io_error_code::INVALID_INPUT, 0);
        return -1;
    }
    crate::io::record_close();
    // SAFETY : handle assumed valid per caller contract.
    let ok = unsafe { CloseHandle(handle as HANDLE) };
    if ok == 0 {
        // SAFETY : GetLastError is a thread-local with no preconditions.
        let win32_err = unsafe { GetLastError() };
        let kind = translate_win32_error(win32_err);
        record_io_error(kind, win32_err as i32);
        return -1;
    }
    0
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — flag-translation table + Win32 error mapping + roundtrip.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
// Test code routinely compares `cssl_fs_*` i64 returns against
// `payload.len() as i64` ; on 64-bit hosts the cast widens 0..=isize::MAX
// without loss but clippy's `cast_possible_wrap` lint fires regardless.
// The suppression is scoped to the test module (production code uses
// the per-fn `#[allow(...)]` annotations on the live syscall paths).
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;

    #[test]
    fn translate_win32_error_known_kinds() {
        assert_eq!(
            translate_win32_error(ERROR_FILE_NOT_FOUND),
            io_error_code::NOT_FOUND
        );
        assert_eq!(
            translate_win32_error(ERROR_PATH_NOT_FOUND),
            io_error_code::NOT_FOUND
        );
        assert_eq!(
            translate_win32_error(ERROR_ACCESS_DENIED),
            io_error_code::PERMISSION_DENIED
        );
        assert_eq!(
            translate_win32_error(ERROR_FILE_EXISTS),
            io_error_code::ALREADY_EXISTS
        );
        assert_eq!(
            translate_win32_error(ERROR_ALREADY_EXISTS),
            io_error_code::ALREADY_EXISTS
        );
        assert_eq!(
            translate_win32_error(ERROR_INVALID_PARAMETER),
            io_error_code::INVALID_INPUT
        );
        // Unknown code falls through to OTHER.
        assert_eq!(translate_win32_error(99_999), io_error_code::OTHER);
    }

    #[test]
    fn translate_open_flags_read_only() {
        let (access, _share, disp, append) = translate_open_flags(OPEN_READ);
        assert_eq!(access, GENERIC_READ);
        assert_eq!(disp, OPEN_EXISTING);
        assert!(!append);
    }

    #[test]
    fn translate_open_flags_write_create_truncate() {
        let (access, _share, disp, append) =
            translate_open_flags(OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE);
        assert_eq!(access, GENERIC_WRITE);
        assert_eq!(disp, CREATE_ALWAYS);
        assert!(!append);
    }

    #[test]
    fn translate_open_flags_write_create_only() {
        let (access, _share, disp, append) = translate_open_flags(OPEN_WRITE | OPEN_CREATE);
        assert_eq!(access, GENERIC_WRITE);
        assert_eq!(disp, OPEN_ALWAYS);
        assert!(!append);
    }

    #[test]
    fn translate_open_flags_write_create_new() {
        let (access, _share, disp, append) =
            translate_open_flags(OPEN_WRITE | OPEN_CREATE | OPEN_CREATE_NEW);
        assert_eq!(access, GENERIC_WRITE);
        assert_eq!(disp, CREATE_NEW);
        assert!(!append);
    }

    #[test]
    fn translate_open_flags_append_strips_generic_write() {
        let (access, _share, disp, append) =
            translate_open_flags(OPEN_WRITE | OPEN_CREATE | OPEN_APPEND);
        assert_eq!(access & GENERIC_WRITE, 0);
        assert_eq!(access & FILE_APPEND_DATA, FILE_APPEND_DATA);
        assert_eq!(disp, OPEN_ALWAYS);
        assert!(append);
    }

    #[test]
    fn translate_open_flags_read_write_modes_combine() {
        let (access, _share, _disp, _append) = translate_open_flags(OPEN_READ_WRITE);
        assert_eq!(access, GENERIC_READ | GENERIC_WRITE);
    }

    #[test]
    fn translate_open_flags_truncate_existing_without_create() {
        let (access, _share, disp, _append) = translate_open_flags(OPEN_WRITE | OPEN_TRUNCATE);
        assert_eq!(access, GENERIC_WRITE);
        assert_eq!(disp, TRUNCATE_EXISTING);
    }

    #[test]
    fn open_invalid_flags_returns_invalid_handle() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let p = b"x.txt";
        // SAFETY : valid byte slice ; OPEN_FLAG_MASK<<1 is an unknown bit.
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
        // SAFETY : null path is the documented invalid-input case.
        let r = unsafe { cssl_fs_open_impl(core::ptr::null(), 0, OPEN_READ) };
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
        // SAFETY : INVALID_HANDLE is the documented sentinel ; impl
        // checks before invoking ReadFile.
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
        // SAFETY : INVALID_HANDLE sentinel ; impl checks before WriteFile.
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
        // SAFETY : INVALID_HANDLE sentinel ; impl checks before CloseHandle.
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
        let path = b"C:\\zzz_apocky_definitely_not_a_file_xyz_b5.txt";
        // SAFETY : valid byte slice.
        let r = unsafe { cssl_fs_open_impl(path.as_ptr(), path.len(), OPEN_READ) };
        assert_eq!(r, INVALID_HANDLE);
        assert_eq!(crate::io::last_io_error_kind(), io_error_code::NOT_FOUND);
    }

    #[test]
    fn open_write_create_close_roundtrip() {
        // ‼ Win32 file-roundtrip integration test (HANDOFF report-back gate).
        // 1. write a temp file via cssl_fs_open + cssl_fs_write + cssl_fs_close
        // 2. read it back via cssl_fs_open + cssl_fs_read + cssl_fs_close
        // 3. assert byte-for-byte equality
        let _g = crate::test_helpers::lock_and_reset_all();

        // Build a unique temp path under %TEMP%.
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("cssl_b5_roundtrip_test.txt");
        let path_str = path.to_string_lossy().into_owned();

        let payload = b"hello b5 win32 roundtrip\n";

        // ── step 1 : open + write + close ──────────────────────────────
        // SAFETY : path_str is a valid String ; payload is a valid byte slice.
        let h = unsafe {
            cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE,
            )
        };
        assert_ne!(h, INVALID_HANDLE, "open failed");
        // SAFETY : h is a valid handle just opened with write access.
        let n = unsafe { cssl_fs_write_impl(h, payload.as_ptr(), payload.len()) };
        assert_eq!(n, payload.len() as i64, "short write");
        // SAFETY : h is the valid handle.
        let cr = unsafe { cssl_fs_close_impl(h) };
        assert_eq!(cr, 0, "close failed");

        // ── step 2 : open + read + close ───────────────────────────────
        // SAFETY : path_str is the same valid path.
        let h2 = unsafe { cssl_fs_open_impl(path_str.as_ptr(), path_str.len(), OPEN_READ) };
        assert_ne!(h2, INVALID_HANDLE, "reopen failed");
        let mut readback = vec![0u8; payload.len() + 8];
        // SAFETY : h2 is valid + readback buffer is valid.
        let nr = unsafe { cssl_fs_read_impl(h2, readback.as_mut_ptr(), readback.len()) };
        assert_eq!(nr, payload.len() as i64, "short read");
        let cr2 = unsafe { cssl_fs_close_impl(h2) };
        assert_eq!(cr2, 0, "close-2 failed");

        // ── step 3 : assert bytes match ─────────────────────────────────
        assert_eq!(&readback[..payload.len()], payload);

        // Cleanup — best-effort std::fs delete.
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_mode_writes_to_end_of_file() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("cssl_b5_append_test.txt");
        let path_str = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);

        let first = b"first\n";
        let second = b"second\n";

        // First write : create + truncate.
        // SAFETY : valid path / payload buffers.
        let h = unsafe {
            cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE,
            )
        };
        assert_ne!(h, INVALID_HANDLE);
        let _ = unsafe { cssl_fs_write_impl(h, first.as_ptr(), first.len()) };
        let _ = unsafe { cssl_fs_close_impl(h) };

        // Second write : append.
        // SAFETY : valid path.
        let h2 = unsafe {
            cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_APPEND,
            )
        };
        assert_ne!(h2, INVALID_HANDLE);
        let _ = unsafe { cssl_fs_write_impl(h2, second.as_ptr(), second.len()) };
        let _ = unsafe { cssl_fs_close_impl(h2) };

        // Read all back.
        // SAFETY : valid path / buffer.
        let hr = unsafe { cssl_fs_open_impl(path_str.as_ptr(), path_str.len(), OPEN_READ) };
        assert_ne!(hr, INVALID_HANDLE);
        let mut buf = vec![0u8; first.len() + second.len() + 8];
        let nr = unsafe { cssl_fs_read_impl(hr, buf.as_mut_ptr(), buf.len()) };
        let _ = unsafe { cssl_fs_close_impl(hr) };
        assert_eq!(nr, (first.len() + second.len()) as i64);

        let mut expected = Vec::new();
        expected.extend_from_slice(first);
        expected.extend_from_slice(second);
        assert_eq!(&buf[..nr as usize], expected.as_slice());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn create_new_fails_when_file_exists() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("cssl_b5_create_new_test.txt");
        let path_str = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);

        // First open : CREATE — succeeds.
        // SAFETY : valid path.
        let h = unsafe {
            cssl_fs_open_impl(path_str.as_ptr(), path_str.len(), OPEN_WRITE | OPEN_CREATE)
        };
        assert_ne!(h, INVALID_HANDLE);
        let _ = unsafe { cssl_fs_close_impl(h) };

        // Second open : CREATE_NEW — fails with AlreadyExists.
        // SAFETY : valid path.
        let h2 = unsafe {
            cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_CREATE_NEW,
            )
        };
        assert_eq!(h2, INVALID_HANDLE);
        assert_eq!(
            crate::io::last_io_error_kind(),
            io_error_code::ALREADY_EXISTS
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_zero_length_buffer_returns_zero() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("cssl_b5_zero_read_test.txt");
        let path_str = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&path);

        // Create an empty file.
        // SAFETY : valid path.
        let h = unsafe {
            cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE,
            )
        };
        assert_ne!(h, INVALID_HANDLE);
        let _ = unsafe { cssl_fs_close_impl(h) };

        // SAFETY : valid path.
        let hr = unsafe { cssl_fs_open_impl(path_str.as_ptr(), path_str.len(), OPEN_READ) };
        assert_ne!(hr, INVALID_HANDLE);

        // Zero-length read should return 0 (validated path, no syscall).
        // SAFETY : 0-length is valid even with null but we pass a real ptr.
        let mut dummy = [0u8; 1];
        let r = unsafe { cssl_fs_read_impl(hr, dummy.as_mut_ptr(), 0) };
        assert_eq!(r, 0);
        let _ = unsafe { cssl_fs_close_impl(hr) };

        let _ = std::fs::remove_file(&path);
    }
}
