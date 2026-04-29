//! § cssl-rt file-system I/O — cross-platform interface (T11-D76, S6-B5).
//!
//! § ROLE
//!   The platform-neutral I/O surface that CSSLv3-emitted code calls
//!   into via the `__cssl_fs_*` FFI symbols. Per-OS implementations live
//!   in `crate::io_win32` (Windows : `CreateFileW` / `ReadFile` /
//!   `WriteFile` / `CloseHandle`) and `crate::io_unix` (Linux + macOS :
//!   `open` / `read` / `write` / `close` libc-style syscalls). The
//!   active platform module is selected via cfg ; only one is compiled
//!   per build, so a doc-link to the inactive one would be a broken
//!   intra-doc link.
//!
//! § DESIGN
//!   Three layers, mirroring the allocator surface (T11-D52) :
//!   1. Platform layer  — `io_win32` / `io_unix` : the actual syscall calls
//!      + per-OS error-translation. Selected at compile-time via `cfg`.
//!   2. This module     — re-exports the active platform's `*_impl` fns
//!      under a stable cross-platform name, plus the `io_error_code`
//!      module + `OPEN_*` constant bitset that source-level CSSLv3 sees.
//!   3. FFI layer       — `__cssl_fs_*` symbols in [`crate::ffi`] delegate
//!      to this module.
//!
//! § FILE HANDLES
//!   At cssl-rt level a file handle is `i64` :
//!     - Windows : the underlying `HANDLE` value cast to `i64` (or
//!       `INVALID_HANDLE_VALUE = -1` on error before the syscall returns).
//!     - Unix    : the underlying `fd : c_int` zero-extended to `i64`
//!       (or `-1` on error before the syscall returns).
//!   The CSSLv3-source-level `File` type wraps this `i64` with an iso<...>
//!   capability ; consumers must call `__cssl_fs_close` exactly once to
//!   release the OS resource.
//!
//! § OPEN-FLAGS bitset
//!   Per `specs/04_EFFECTS.csl § ERROR HANDLING` and the canonical Rust /
//!   C  open-mode set, we expose a small portable bitset that each
//!   platform translates to its OS-native flag combination.
//!
//! § ERROR DOMAIN
//!   The `IoError` source-level type (defined in `stdlib/fs.cssl`) is
//!   the canonical error sum. Its discriminants are stable from S6-B5
//!   forward — see `io_error_code` below for the integer values that
//!   stdlib/fs.cssl maps onto the typed sum-type. Renumbering breaks
//!   the stdlib mapping. Per the slice landmines, the variants mirror
//!   Rust's `std::io::ErrorKind` subset that source-level CSSLv3 cares
//!   about + an `Other(i32)` catch-all carrying the OS errno /
//!   `GetLastError` value.
//!
//! § INVARIANTS
//!   - All counters are atomic ⇒ thread-safe from day-1.
//!   - On error, the returned handle is `-1` AND the per-thread last-error
//!     slot is updated with the canonical `IoError` discriminant for
//!     consumers that want the structured form rather than the raw `i64`.
//!   - The Win32 path translates UTF-8 source paths to UTF-16 wchar* via
//!     [`utf8_to_utf16_lossy`] (hand-rolled — no `widestring` crate dep
//!     per the dispatch-plan landmines).
//!   - The Unix path passes UTF-8 bytes directly + a NUL terminator.
//!     Non-UTF-8 paths are not source-level addressable at stage-0.
//!
//! § PRIME-DIRECTIVE attestation
//!   File I/O is the most surveillance-adjacent surface in the runtime.
//!   This module : (a) does no surveillance — every fs op is at the
//!   explicit invocation of source-level code, (b) exposes a tracker
//!   (counts of opens / reads / writes / closes) PUBLICLY so the host
//!   can audit what fs ops occurred, (c) never silently swallows
//!   an error — every failure surfaces through the typed `IoError`
//!   sum-type defined in `stdlib/fs.cssl`, (d) the `{IO}` effect-row
//!   makes file-touching operations visible in fn-signatures (per
//!   `specs/04_EFFECTS § IO-EFFECT`). No hidden side-channels ;
//!   nothing escapes the process.

#![allow(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § Public type re-exports — the source-level surface.
// ───────────────────────────────────────────────────────────────────────

/// Sentinel handle returned by `open` on failure ; mirrors POSIX `-1` and
/// Win32 `INVALID_HANDLE_VALUE` after sign-extension to `i64`.
///
/// CSSLv3 source-level code recognizes this value as the error sentinel
/// when `__cssl_fs_open` returns ; the stdlib `fs.cssl::open` wrapper
/// converts it into `Result::Err(IoError::*)` per the per-thread last-error
/// slot.
pub const INVALID_HANDLE: i64 = -1;

// ───────────────────────────────────────────────────────────────────────
// § OpenFlags — portable bitset.
// ───────────────────────────────────────────────────────────────────────

/// Open `path` for read-only access. Errors if the file does not exist.
pub const OPEN_READ: i32 = 0x0001;
/// Open `path` for write-only access. By itself implies create-or-truncate
/// (matches Rust's `File::create` semantics) ; pair with `OPEN_APPEND` for
/// append-mode or `OPEN_TRUNCATE = 0` to require pre-existing file.
pub const OPEN_WRITE: i32 = 0x0002;
/// Open for both read and write. The file must exist.
pub const OPEN_READ_WRITE: i32 = 0x0004;
/// On write-mode, append to the existing file rather than truncating.
pub const OPEN_APPEND: i32 = 0x0008;
/// On write-mode, create the file if it does not exist (default for write).
pub const OPEN_CREATE: i32 = 0x0010;
/// Fail with `AlreadyExists` if the file already exists (combined with
/// `OPEN_CREATE` to make creation exclusive).
pub const OPEN_CREATE_NEW: i32 = 0x0020;
/// On write-mode, truncate the existing file to zero length on open.
pub const OPEN_TRUNCATE: i32 = 0x0040;

/// Mask of the recognized open-flag bits ; any other bit is rejected.
pub const OPEN_FLAG_MASK: i32 = OPEN_READ
    | OPEN_WRITE
    | OPEN_READ_WRITE
    | OPEN_APPEND
    | OPEN_CREATE
    | OPEN_CREATE_NEW
    | OPEN_TRUNCATE;

// ───────────────────────────────────────────────────────────────────────
// § IoError — canonical error sum-type.
// ───────────────────────────────────────────────────────────────────────

/// Canonical I/O error variants — discriminants are STABLE from S6-B5.
///
/// The `i32` discriminant is what cssl-rt threads through the per-thread
/// last-error slot ; source-level CSSLv3 maps it onto the `IoError`
/// sum-type constructors via the `io_error_from_kind` fn in
/// `stdlib/fs.cssl`. Renaming a discriminant requires a major-version
/// bump (mirrors the FFI-symbol invariant from T11-D52).
///
/// § VARIANTS
///   - `0` Success — sentinel for "no error", never observed by consumers
///     of `IoError` (only present in the per-thread slot reset on each op).
///   - `1` `NotFound` — the path or file does not exist.
///   - `2` `PermissionDenied` — caller lacks the right to perform the op.
///   - `3` `AlreadyExists` — `OPEN_CREATE_NEW` op found a pre-existing path.
///   - `4` `InvalidInput` — caller-supplied flags / pointer / length is
///     malformed (e.g., null path, length overflow, unknown flag bit).
///   - `5` `Interrupted` — syscall returned `EINTR` ; caller may retry.
///   - `6` `UnexpectedEof` — read returned 0 before requested bytes count.
///   - `7` `WriteZero` — write returned 0 mid-op (disk-full / quota).
///   - `99` `Other` — catch-all carrying the raw OS errno / `GetLastError`
///     value in the high 32 bits of the i64 slot. Stable from S6-B5.
///
/// Stage-0 stdlib/fs.cssl maps these to a CSSLv3 sum-type with the
/// matching tag-discipline ; the recognizer in `cssl_mir::body_lower`
/// short-circuits the conversion.
pub mod io_error_code {
    /// No error — never observed by `IoError` consumers.
    pub const SUCCESS: i32 = 0;
    /// Path or file does not exist.
    pub const NOT_FOUND: i32 = 1;
    /// Caller lacks rights to perform the operation.
    pub const PERMISSION_DENIED: i32 = 2;
    /// `OPEN_CREATE_NEW` failed because the path already exists.
    pub const ALREADY_EXISTS: i32 = 3;
    /// Caller-supplied flags / pointer / length is malformed.
    pub const INVALID_INPUT: i32 = 4;
    /// Syscall interrupted ; caller may retry.
    pub const INTERRUPTED: i32 = 5;
    /// Read reached EOF before fulfilling the requested byte count.
    pub const UNEXPECTED_EOF: i32 = 6;
    /// Write returned 0 mid-op (disk-full / quota).
    pub const WRITE_ZERO: i32 = 7;
    /// Catch-all : the high 32 bits of the i64 slot carry the raw OS code.
    pub const OTHER: i32 = 99;
}

// ───────────────────────────────────────────────────────────────────────
// § Per-thread last-error slot.
// ───────────────────────────────────────────────────────────────────────

// Stage-0 implementation : a single global atomic. Sufficient for hosted
// stage-0 testing. A per-thread TLS slot is a follow-up once the runtime
// grows TLS infrastructure (placeholder per T11-D52 § runtime).
static LAST_ERROR_CODE: AtomicU64 = AtomicU64::new(0);

/// Write the canonical error code for the last fs op.
///
/// `os_code` is the raw OS errno / `GetLastError` value (or 0 for the
/// non-`OTHER` cases). The two are packed into a single u64 :
/// `(os_code as u64) << 32 | (kind_code as u32 as u64)`.
pub fn record_io_error(kind_code: i32, os_code: i32) {
    #[allow(clippy::cast_sign_loss)]
    let kind = kind_code as u32 as u64;
    #[allow(clippy::cast_sign_loss)]
    let os = os_code as u32 as u64;
    LAST_ERROR_CODE.store((os << 32) | kind, Ordering::Relaxed);
}

/// Read the canonical error kind from the last fs op (low 32 bits).
#[must_use]
pub fn last_io_error_kind() -> i32 {
    #[allow(clippy::cast_possible_wrap)]
    let kind = (LAST_ERROR_CODE.load(Ordering::Relaxed) & 0xFFFF_FFFF) as i32;
    kind
}

/// Read the raw OS code from the last fs op (high 32 bits).
#[must_use]
pub fn last_io_error_os() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    let os = ((LAST_ERROR_CODE.load(Ordering::Relaxed) >> 32) & 0xFFFF_FFFF) as i32;
    os
}

/// Reset the last-error slot to `SUCCESS / 0`. Test-only.
#[doc(hidden)]
pub fn reset_last_io_error_for_tests() {
    LAST_ERROR_CODE.store(0, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § Per-op tracker — observable counters for host audit.
// ───────────────────────────────────────────────────────────────────────

static OPEN_COUNT: AtomicU64 = AtomicU64::new(0);
static READ_COUNT: AtomicU64 = AtomicU64::new(0);
static WRITE_COUNT: AtomicU64 = AtomicU64::new(0);
static CLOSE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Op-kind code recorded alongside the path-hash in [`PathHashEvent`].
///
/// § STABILITY
///   Numeric values are STABLE across builds — they appear in audit-chain
///   entries via [`crate::io::path_hash_events_drain`] consumers (the
///   substrate-prime-directive crate) and renaming would break log
///   replay. Adding a new variant requires a major bump.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathHashOpKind {
    /// `__cssl_fs_open` (any flags).
    Open = 1,
    /// `__cssl_fs_read` (call-site, regardless of byte count).
    Read = 2,
    /// `__cssl_fs_write` (call-site).
    Write = 3,
    /// `__cssl_fs_close`.
    Close = 4,
}

impl PathHashOpKind {
    /// Stable canonical name used in audit-chain tags. Renaming = ABI bump.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Open => "fs-open",
            Self::Read => "fs-read",
            Self::Write => "fs-write",
            Self::Close => "fs-close",
        }
    }
}

/// One path-hash event : (path_hash, op-kind).
///
/// § DISCIPLINE
///   The `path_hash` field is the ONLY observable form of the path. The
///   raw path is never stored, never serialized, never logged. The hash
///   is computed at the FFI boundary (in `io_win32::cssl_fs_open_impl` /
///   `io_unix::cssl_fs_open_impl`) before any other recording happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PathHashEvent {
    /// 32-byte BLAKE3 hash of the OS-native path bytes under the
    /// per-installation salt (see `cssl_telemetry::path_hash`).
    pub path_hash: [u8; 32],
    /// Which fs-op happened.
    pub op: PathHashOpKind,
}

// In-process bounded queue of path-hash events. Producers (FFI shims)
// push lock-free with an overflow-counter ; consumers (the
// substrate-prime-directive audit-bus) drain periodically.
//
// Stage-0 backing : a single `Mutex<VecDeque<...>>` matching the pattern
// used by `cssl_telemetry::ring`. Phase-2 swaps for a real lock-free
// SPSC ; the API surface is stable.
//
// Capacity bound : 4096 events, matching the default audit-export
// flush-window. Beyond that, [`PATH_HASH_OVERFLOW_COUNT`] increments
// and the new event is DROPPED — telemetry-prefers-lossy-non-blocking
// per `specs/22 § RING-BUFFER`.

/// Maximum buffered path-hash events before overflow.
pub const PATH_HASH_QUEUE_CAPACITY: usize = 4096;

static PATH_HASH_OVERFLOW_COUNT: AtomicU64 = AtomicU64::new(0);
// One global Mutex<VecDeque> ; replace with lock-free in phase-2.
static PATH_HASH_QUEUE: std::sync::Mutex<std::collections::VecDeque<PathHashEvent>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());
// `pub(crate)` so platform layers can update the byte-totals after a
// successful syscall without re-incrementing the call-count via
// [`record_read`] / [`record_write`].
pub(crate) static BYTES_READ_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(crate) static BYTES_WRITTEN_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Number of `__cssl_fs_open` calls that returned a valid handle (≠ -1).
#[must_use]
pub fn open_count() -> u64 {
    OPEN_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_fs_read` calls (regardless of success ; the
/// `bytes_read_total` counter records the actual byte movement).
#[must_use]
pub fn read_count() -> u64 {
    READ_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_fs_write` calls.
#[must_use]
pub fn write_count() -> u64 {
    WRITE_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_fs_close` calls (regardless of success).
#[must_use]
pub fn close_count() -> u64 {
    CLOSE_COUNT.load(Ordering::Relaxed)
}

/// Total bytes successfully read by all `__cssl_fs_read` calls.
#[must_use]
pub fn bytes_read_total() -> u64 {
    BYTES_READ_TOTAL.load(Ordering::Relaxed)
}

/// Total bytes successfully written by all `__cssl_fs_write` calls.
#[must_use]
pub fn bytes_written_total() -> u64 {
    BYTES_WRITTEN_TOTAL.load(Ordering::Relaxed)
}

/// Reset all fs counters + last-error slot. Test-only.
#[doc(hidden)]
pub fn reset_io_for_tests() {
    OPEN_COUNT.store(0, Ordering::Relaxed);
    READ_COUNT.store(0, Ordering::Relaxed);
    WRITE_COUNT.store(0, Ordering::Relaxed);
    CLOSE_COUNT.store(0, Ordering::Relaxed);
    BYTES_READ_TOTAL.store(0, Ordering::Relaxed);
    BYTES_WRITTEN_TOTAL.store(0, Ordering::Relaxed);
    PATH_HASH_OVERFLOW_COUNT.store(0, Ordering::Relaxed);
    if let Ok(mut q) = PATH_HASH_QUEUE.lock() {
        q.clear();
    }
    reset_last_io_error_for_tests();
}

#[doc(hidden)]
pub(crate) fn record_open() {
    OPEN_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[doc(hidden)]
pub(crate) fn record_read(bytes: u64) {
    READ_COUNT.fetch_add(1, Ordering::Relaxed);
    if bytes != 0 {
        BYTES_READ_TOTAL.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[doc(hidden)]
pub(crate) fn record_write(bytes: u64) {
    WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
    if bytes != 0 {
        BYTES_WRITTEN_TOTAL.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[doc(hidden)]
pub(crate) fn record_close() {
    CLOSE_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § Path-hash event recording (T11-D130 — F6 path-hash-only discipline).
// ───────────────────────────────────────────────────────────────────────

/// Record a `(path_hash, op)` event into the in-process queue.
///
/// § DISCIPLINE
///   This is the canonical recording entry-point for fs-ops that touched
///   a path. The `path_hash` is the 32-byte BLAKE3-salted output of
///   `cssl_telemetry::PathHasher::hash_bytes` ; the platform layer is
///   responsible for computing it BEFORE any other recording happens
///   (so the raw bytes never live longer than the syscall path).
///
/// § OVERFLOW
///   On a full queue, the new event is DROPPED and
///   [`path_hash_overflow_count`] is incremented. Telemetry prefers
///   lossy-non-blocking per `specs/22 § RING-BUFFER` ; consumers are
///   expected to drain frequently.
pub fn record_path_hash_event(path_hash: [u8; 32], op: PathHashOpKind) {
    if let Ok(mut q) = PATH_HASH_QUEUE.lock() {
        if q.len() >= PATH_HASH_QUEUE_CAPACITY {
            PATH_HASH_OVERFLOW_COUNT.fetch_add(1, Ordering::Relaxed);
            return;
        }
        q.push_back(PathHashEvent { path_hash, op });
    } else {
        // Mutex poisoned ; count as overflow rather than panic.
        PATH_HASH_OVERFLOW_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

/// Number of path-hash events that were dropped due to queue-full.
#[must_use]
pub fn path_hash_overflow_count() -> u64 {
    PATH_HASH_OVERFLOW_COUNT.load(Ordering::Relaxed)
}

/// Drain all queued path-hash events. The queue is empty after this call.
/// Consumers (the substrate-prime-directive audit-bus) drain on a periodic
/// schedule + on flush.
#[must_use]
pub fn path_hash_events_drain() -> alloc::vec::Vec<PathHashEvent> {
    PATH_HASH_QUEUE
        .lock()
        .map_or_else(|_| alloc::vec::Vec::new(), |mut q| q.drain(..).collect())
}

/// Number of currently-queued path-hash events. Test-only convenience.
#[doc(hidden)]
#[must_use]
pub fn path_hash_queue_len() -> usize {
    PATH_HASH_QUEUE.lock().map(|q| q.len()).unwrap_or(0)
}

// ───────────────────────────────────────────────────────────────────────
// § UTF-8 → UTF-16 conversion (hand-rolled per dispatch-plan landmine).
// ───────────────────────────────────────────────────────────────────────

/// Convert `path` (UTF-8 bytes, length `path_len`) to a UTF-16 wide-char
/// vector with a trailing NUL terminator.
///
/// Lossy on malformed UTF-8 : invalid sequences emit `U+FFFD REPLACEMENT
/// CHARACTER` per the WTF-8 / Rust `String::from_utf8_lossy` convention.
/// Stage-0 callers are stdlib/fs.cssl which sources its strings from the
/// CSSLv3 `String` type (which guarantees valid UTF-8 by B4 invariant) ;
/// the lossy conversion guards against caller error rather than being a
/// design point.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` consecutive bytes. `path_len`
/// must not be larger than `isize::MAX` (the standard slice-construction
/// bound). Returns an empty vector if `path_len == 0` (non-error :
/// caller's responsibility to reject empty paths).
#[must_use]
pub unsafe fn utf8_to_utf16_lossy(path_ptr: *const u8, path_len: usize) -> alloc::vec::Vec<u16> {
    if path_ptr.is_null() || path_len == 0 {
        return alloc::vec::Vec::from([0u16]);
    }
    // SAFETY : caller contract — path_ptr valid for path_len bytes.
    let bytes = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    // Rust's `String::from_utf8_lossy` returns a Cow ; we take ownership
    // and re-encode the chars to UTF-16. This is allocation-heavy at
    // stage-0 but the pattern is straightforward + correct for any
    // valid UTF-8 input.
    let cow = alloc::string::String::from_utf8_lossy(bytes);
    let mut out: alloc::vec::Vec<u16> = cow.encode_utf16().collect();
    out.push(0); // NUL terminator for Win32 wchar*
    out
}

// We need `extern crate alloc` for `Vec` / `String` in `no_std`-style
// modules ; cssl-rt currently uses std's heap surface so we also pull in
// `alloc` as a sibling crate via the Rust prelude. Adding an explicit
// `extern crate alloc;` keeps this module portable to a future
// no-std build.
extern crate alloc;

// ───────────────────────────────────────────────────────────────────────
// § Cross-platform `*_impl` re-exports : platform layer is selected via
//   cfg. Each platform crate exposes the same fn names so the FFI layer
//   in [`crate::ffi`] can call into them uniformly.
// ───────────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub use crate::io_win32::{
    cssl_fs_close_impl, cssl_fs_open_impl, cssl_fs_read_impl, cssl_fs_write_impl,
};

#[cfg(not(target_os = "windows"))]
pub use crate::io_unix::{
    cssl_fs_close_impl, cssl_fs_open_impl, cssl_fs_read_impl, cssl_fs_write_impl,
};

// ───────────────────────────────────────────────────────────────────────
// § Validation helpers shared by both platforms.
// ───────────────────────────────────────────────────────────────────────

/// Validate caller-supplied open-flags : reject unknown bits + check for
/// inconsistent combinations.
///
/// Returns `Ok(())` on valid flags, `Err(InvalidInput-code)` otherwise.
/// Both platform `cssl_fs_open_impl` paths consult this before invoking
/// the OS syscall.
pub fn validate_open_flags(flags: i32) -> Result<(), i32> {
    // Reject any bit outside the recognized mask.
    if (flags & !OPEN_FLAG_MASK) != 0 {
        return Err(io_error_code::INVALID_INPUT);
    }
    // Must specify at least one access-mode bit.
    let access = flags & (OPEN_READ | OPEN_WRITE | OPEN_READ_WRITE);
    if access == 0 {
        return Err(io_error_code::INVALID_INPUT);
    }
    // OPEN_READ_WRITE is exclusive of pure-read / pure-write.
    if (flags & OPEN_READ_WRITE) != 0 && (flags & (OPEN_READ | OPEN_WRITE)) != 0 {
        return Err(io_error_code::INVALID_INPUT);
    }
    // OPEN_CREATE_NEW implies OPEN_CREATE.
    if (flags & OPEN_CREATE_NEW) != 0 && (flags & OPEN_CREATE) == 0 {
        return Err(io_error_code::INVALID_INPUT);
    }
    // OPEN_APPEND + OPEN_TRUNCATE are mutually exclusive.
    if (flags & OPEN_APPEND) != 0 && (flags & OPEN_TRUNCATE) != 0 {
        return Err(io_error_code::INVALID_INPUT);
    }
    Ok(())
}

/// Validate caller-supplied (ptr, len) pair for read / write. Returns
/// `Ok(())` if the pair is well-formed (null is rejected for non-zero
/// length), `Err(InvalidInput-code)` otherwise.
pub fn validate_buffer(ptr: *const u8, len: usize) -> Result<(), i32> {
    if len > 0 && ptr.is_null() {
        return Err(io_error_code::INVALID_INPUT);
    }
    // isize::MAX bound — Rust's slice-construction limit.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    if len > isize::MAX as usize {
        return Err(io_error_code::INVALID_INPUT);
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — counter + flag + UTF-16 conversion + validation surface.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_flag_mask_includes_all_bits() {
        // Sanity : OPEN_FLAG_MASK is the OR of every recognized bit.
        let expected = OPEN_READ
            | OPEN_WRITE
            | OPEN_READ_WRITE
            | OPEN_APPEND
            | OPEN_CREATE
            | OPEN_CREATE_NEW
            | OPEN_TRUNCATE;
        assert_eq!(OPEN_FLAG_MASK, expected);
    }

    #[test]
    fn open_flag_bits_are_distinct() {
        let bits = [
            OPEN_READ,
            OPEN_WRITE,
            OPEN_READ_WRITE,
            OPEN_APPEND,
            OPEN_CREATE,
            OPEN_CREATE_NEW,
            OPEN_TRUNCATE,
        ];
        for (i, a) in bits.iter().enumerate() {
            for b in &bits[i + 1..] {
                assert_eq!(a & b, 0, "flags must be mutually distinct bits");
            }
        }
    }

    #[test]
    fn io_error_codes_are_distinct() {
        let codes = [
            io_error_code::NOT_FOUND,
            io_error_code::PERMISSION_DENIED,
            io_error_code::ALREADY_EXISTS,
            io_error_code::INVALID_INPUT,
            io_error_code::INTERRUPTED,
            io_error_code::UNEXPECTED_EOF,
            io_error_code::WRITE_ZERO,
            io_error_code::OTHER,
        ];
        let mut seen = codes.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), codes.len(), "error codes must be unique");
    }

    #[test]
    fn last_error_records_kind_and_os_code() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_io_error(io_error_code::NOT_FOUND, 2);
        assert_eq!(last_io_error_kind(), io_error_code::NOT_FOUND);
        assert_eq!(last_io_error_os(), 2);
    }

    #[test]
    fn last_error_kind_other_carries_os_code() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_io_error(io_error_code::OTHER, 9999);
        assert_eq!(last_io_error_kind(), io_error_code::OTHER);
        assert_eq!(last_io_error_os(), 9999);
    }

    #[test]
    fn reset_clears_last_error() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_io_error(io_error_code::NOT_FOUND, 2);
        reset_last_io_error_for_tests();
        assert_eq!(last_io_error_kind(), io_error_code::SUCCESS);
        assert_eq!(last_io_error_os(), 0);
    }

    #[test]
    fn counters_start_at_zero() {
        let _g = crate::test_helpers::lock_and_reset_all();
        assert_eq!(open_count(), 0);
        assert_eq!(read_count(), 0);
        assert_eq!(write_count(), 0);
        assert_eq!(close_count(), 0);
        assert_eq!(bytes_read_total(), 0);
        assert_eq!(bytes_written_total(), 0);
    }

    #[test]
    fn record_open_increments_counter() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_open();
        record_open();
        assert_eq!(open_count(), 2);
    }

    #[test]
    fn record_read_increments_counter_and_bytes_total() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_read(64);
        record_read(0); // EOF read still counts the call
        assert_eq!(read_count(), 2);
        assert_eq!(bytes_read_total(), 64);
    }

    #[test]
    fn record_write_increments_counter_and_bytes_total() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_write(128);
        record_write(256);
        assert_eq!(write_count(), 2);
        assert_eq!(bytes_written_total(), 384);
    }

    #[test]
    fn record_close_increments_counter() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_close();
        assert_eq!(close_count(), 1);
    }

    #[test]
    fn validate_open_flags_accepts_read() {
        assert!(validate_open_flags(OPEN_READ).is_ok());
    }

    #[test]
    fn validate_open_flags_accepts_write_create() {
        assert!(validate_open_flags(OPEN_WRITE | OPEN_CREATE).is_ok());
    }

    #[test]
    fn validate_open_flags_accepts_write_create_new() {
        assert!(validate_open_flags(OPEN_WRITE | OPEN_CREATE | OPEN_CREATE_NEW).is_ok());
    }

    #[test]
    fn validate_open_flags_accepts_write_append() {
        assert!(validate_open_flags(OPEN_WRITE | OPEN_APPEND).is_ok());
    }

    #[test]
    fn validate_open_flags_accepts_read_write() {
        assert!(validate_open_flags(OPEN_READ_WRITE).is_ok());
    }

    #[test]
    fn validate_open_flags_rejects_unknown_bit() {
        // 0x8000 is outside the known set
        let r = validate_open_flags(0x8000);
        assert_eq!(r, Err(io_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_open_flags_rejects_no_access_mode() {
        let r = validate_open_flags(OPEN_CREATE);
        assert_eq!(r, Err(io_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_open_flags_rejects_read_write_with_read_or_write() {
        // OPEN_READ_WRITE is exclusive of OPEN_READ / OPEN_WRITE
        let r = validate_open_flags(OPEN_READ_WRITE | OPEN_READ);
        assert_eq!(r, Err(io_error_code::INVALID_INPUT));
        let r = validate_open_flags(OPEN_READ_WRITE | OPEN_WRITE);
        assert_eq!(r, Err(io_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_open_flags_rejects_create_new_without_create() {
        let r = validate_open_flags(OPEN_WRITE | OPEN_CREATE_NEW);
        assert_eq!(r, Err(io_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_open_flags_rejects_append_with_truncate() {
        let r = validate_open_flags(OPEN_WRITE | OPEN_APPEND | OPEN_TRUNCATE);
        assert_eq!(r, Err(io_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_buffer_accepts_zero_length_null() {
        assert!(validate_buffer(core::ptr::null(), 0).is_ok());
    }

    #[test]
    fn validate_buffer_accepts_non_null_with_length() {
        let buf = [0u8; 16];
        assert!(validate_buffer(buf.as_ptr(), buf.len()).is_ok());
    }

    #[test]
    fn validate_buffer_rejects_null_with_nonzero_length() {
        let r = validate_buffer(core::ptr::null(), 16);
        assert_eq!(r, Err(io_error_code::INVALID_INPUT));
    }

    #[test]
    fn utf8_to_utf16_empty_path_returns_single_nul() {
        // SAFETY : path_len == 0 guard returns immediately.
        let v = unsafe { utf8_to_utf16_lossy(core::ptr::null(), 0) };
        assert_eq!(v, alloc::vec![0u16]);
    }

    #[test]
    fn utf8_to_utf16_ascii_path_roundtrips() {
        let s = b"hello.txt";
        // SAFETY : s is valid for s.len() bytes (it's a static byte slice).
        let v = unsafe { utf8_to_utf16_lossy(s.as_ptr(), s.len()) };
        // 9 UTF-16 code units + 1 NUL
        assert_eq!(v.len(), 10);
        assert_eq!(v[v.len() - 1], 0);
        let decoded: alloc::string::String = alloc::string::String::from_utf16_lossy(&v[..9]);
        assert_eq!(decoded, "hello.txt");
    }

    #[test]
    fn utf8_to_utf16_unicode_path_roundtrips() {
        // "héllo.txt" — accented e + ASCII tail
        let s = "héllo.txt".as_bytes();
        // SAFETY : valid UTF-8 string slice.
        let v = unsafe { utf8_to_utf16_lossy(s.as_ptr(), s.len()) };
        // h é l l o . t x t = 9 UTF-16 code units (é is BMP, single u16)
        assert_eq!(v.len(), 10);
        assert_eq!(v[v.len() - 1], 0);
        let decoded: alloc::string::String = alloc::string::String::from_utf16_lossy(&v[..9]);
        assert_eq!(decoded, "héllo.txt");
    }

    #[test]
    fn utf8_to_utf16_handles_surrogate_pair() {
        // U+1F4A9 PILE OF POO — outside BMP, encoded as a UTF-16 surrogate pair.
        let s = "💩.bin".as_bytes();
        // SAFETY : valid UTF-8 string slice.
        let v = unsafe { utf8_to_utf16_lossy(s.as_ptr(), s.len()) };
        // 2 (surrogate pair) + 4 (.bin) + 1 (NUL) = 7
        assert_eq!(v.len(), 7);
        assert_eq!(v[v.len() - 1], 0);
        let decoded: alloc::string::String = alloc::string::String::from_utf16_lossy(&v[..6]);
        assert_eq!(decoded, "💩.bin");
    }

    #[test]
    fn invalid_handle_is_negative_one() {
        // ‼ ABI-stable per cssl-rt FFI invariant ; renaming requires major bump.
        assert_eq!(INVALID_HANDLE, -1);
    }

    // § T11-D130 : path-hash event recording tests

    #[test]
    fn path_hash_op_kind_canonical_names_stable() {
        assert_eq!(PathHashOpKind::Open.canonical_name(), "fs-open");
        assert_eq!(PathHashOpKind::Read.canonical_name(), "fs-read");
        assert_eq!(PathHashOpKind::Write.canonical_name(), "fs-write");
        assert_eq!(PathHashOpKind::Close.canonical_name(), "fs-close");
    }

    #[test]
    fn record_path_hash_event_appends_to_queue() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_path_hash_event([1u8; 32], PathHashOpKind::Open);
        record_path_hash_event([2u8; 32], PathHashOpKind::Read);
        assert_eq!(path_hash_queue_len(), 2);
        let events = path_hash_events_drain();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].path_hash, [1u8; 32]);
        assert_eq!(events[0].op, PathHashOpKind::Open);
        assert_eq!(events[1].path_hash, [2u8; 32]);
        assert_eq!(events[1].op, PathHashOpKind::Read);
    }

    #[test]
    fn path_hash_drain_empties_queue() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_path_hash_event([0xFFu8; 32], PathHashOpKind::Close);
        assert_eq!(path_hash_queue_len(), 1);
        let _ = path_hash_events_drain();
        assert_eq!(path_hash_queue_len(), 0);
    }

    #[test]
    fn path_hash_overflow_increments_counter_when_full() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // Push 1 over the cap to verify the overflow path. We use the
        // public API ; pushing exactly PATH_HASH_QUEUE_CAPACITY+1 events
        // is expensive but bounded, and the test runs in milliseconds.
        for i in 0..PATH_HASH_QUEUE_CAPACITY {
            let mut h = [0u8; 32];
            h[0] = (i & 0xFF) as u8;
            record_path_hash_event(h, PathHashOpKind::Open);
        }
        // One more push triggers overflow.
        record_path_hash_event([0xAAu8; 32], PathHashOpKind::Open);
        assert!(path_hash_overflow_count() >= 1);
        assert_eq!(path_hash_queue_len(), PATH_HASH_QUEUE_CAPACITY);
    }

    #[test]
    fn path_hash_event_struct_is_pub_constructible() {
        // Sanity : downstream code can build a PathHashEvent for tests.
        let e = PathHashEvent {
            path_hash: [7u8; 32],
            op: PathHashOpKind::Write,
        };
        assert_eq!(e.op.canonical_name(), "fs-write");
    }

    #[test]
    fn fs_open_records_path_hash_event_never_raw_path() {
        // ‼ End-to-end : after a real fs_open call, the path-hash queue
        // contains a 32-byte hash. The raw path bytes are NOT recoverable
        // from the queue ; only the hash is.
        let _g = crate::test_helpers::lock_and_reset_all();
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("cssl_d130_path_hash_e2e.txt");
        let path_str = path.to_string_lossy().into_owned();

        // SAFETY : valid path string + create flags.
        #[cfg(target_os = "windows")]
        let h = unsafe {
            crate::io_win32::cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE,
            )
        };
        #[cfg(not(target_os = "windows"))]
        let h = unsafe {
            crate::io_unix::cssl_fs_open_impl(
                path_str.as_ptr(),
                path_str.len(),
                OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE,
            )
        };
        assert_ne!(h, INVALID_HANDLE);

        let events = path_hash_events_drain();
        assert!(!events.is_empty(), "fs_open must record a path-hash event");
        let e = &events[0];
        assert_eq!(e.op, PathHashOpKind::Open);
        // The hash is non-zero (salted BLAKE3 of a non-empty path).
        assert_ne!(e.path_hash, [0u8; 32]);
        // The hash is exactly 32 bytes (BLAKE3 digest size).
        assert_eq!(e.path_hash.len(), 32);
        // ‼ The raw path bytes are NOT in the event. The event holds
        // only the hash ; even attempting to recover the path from
        // 32 bytes is cryptographically infeasible.

        // Cleanup.
        #[cfg(target_os = "windows")]
        unsafe {
            let _ = crate::io_win32::cssl_fs_close_impl(h);
        }
        #[cfg(not(target_os = "windows"))]
        unsafe {
            let _ = crate::io_unix::cssl_fs_close_impl(h);
        }
        let _ = std::fs::remove_file(&path);
    }
}
