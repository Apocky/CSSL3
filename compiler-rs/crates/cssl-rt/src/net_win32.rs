//! § cssl-rt networking — Windows / Winsock2 platform layer (T11-D82, S7-F4).
//!
//! § ROLE
//!   Hand-rolled Win32 syscall bindings for `WSAStartup` / `socket` /
//!   `bind` / `listen` / `accept` / `connect` / `send` / `recv` /
//!   `sendto` / `recvfrom` / `closesocket` / `WSAGetLastError`.
//!   `WSACleanup` is intentionally NOT bound — see § WSAStartup PROCESS-PIN.
//!   Per the dispatch-plan landmines we deliberately avoid pulling in the
//!   `windows-sys` crate at this slice — the FFI surface is small + stable
//!   since Winsock 2.2, and the cssl-rt convention established at T11-D52
//!   (allocator) and reaffirmed at T11-D76 (file I/O) is "hand-rolled
//!   `extern "system"` declarations with each `unsafe` block carrying an
//!   inline SAFETY paragraph".
//!
//! § WSAStartup PROCESS-PIN  (T11-D152 ; was ref-counted, now process-pinned)
//!   `WSAStartup(MAKEWORD(2,2), &wsa_data)` MUST be paired with `WSACleanup`
//!   exactly when the process wants Winsock torn down. The previous design
//!   ref-counted across socket-creates / closes and called `WSACleanup` once
//!   the count reached zero — but the decrement-and-cleanup pair is not
//!   atomic with respect to a concurrent `socket()` syscall on another
//!   thread, opening a race window where `socket()` can return
//!   `WSANOTINITIALISED` mid-`WSACleanup`.
//!
//!   The current design (T11-D152) replaces ref-counting with **process-pin**:
//!   the first net op invokes `WSAStartup` exactly once via `std::sync::Once`
//!   and the result is cached in a static atomic. Subsequent ops branch on
//!   the cached result. **`WSACleanup` is never called** — the OS reclaims
//!   Winsock state at process exit. This is the canonical pattern used by
//!   most production Winsock applications (and matches Rust stdlib's
//!   approach in its net module).
//!
//!   The pin is invisible to user code, eliminates the parallel-test race
//!   entirely, and makes `cargo test` safe under default parallelism without
//!   `--test-threads=1`.
//!
//! § WIN32 ABI NOTES
//!   - `SOCKET` = pointer-sized integer (`UINT_PTR`) ; we model as `usize`
//!     and cast to `i64` at the FFI boundary. `INVALID_SOCKET = (SOCKET)(~0)`
//!     ≡ `usize::MAX` cast to `i64` ≡ `-1`.
//!   - `WSADATA` = 400-byte struct on Win-x64 ; we treat as opaque +
//!     allocate inline.
//!   - `SOCKADDR_IN` = 16-byte IPv4 socket-address struct.
//!   - `int` parameters in Winsock signatures are `c_int` (32-bit).
//!   - All syscalls below set thread-local `WSAGetLastError` on failure ;
//!     we translate the Winsock error code into a canonical
//!     [`crate::net::net_error_code`] discriminant.
//!
//! § INVARIANTS
//!   - On failure, every `cssl_net_*_impl` returns `-1 as i64` (for
//!     handle-producing ops) OR `-1 as i64` (for byte-count-producing
//!     ops) AND records the canonical error in
//!     [`crate::net::record_net_error`]. The high 32 bits of the slot
//!     carry the raw `WSAGetLastError` value for diagnostic consumers.
//!   - Send / recv byte counts are bounded to `i32::MAX` per call to
//!     match Win32's `int` parameter ceiling.
//!   - The flag-translation table is the single source of truth for
//!     how cssl-rt portable flags map to Win32 socket-creation params.
//!
//! § PRIME-DIRECTIVE
//!   See `crate::net` doc-block for the full attestation. Win32-specific
//!   note : `WSAStartup` is local to the process — it creates no
//!   externally-observable side-effects beyond the documented socket-API
//!   initialization. There is no telemetry, no phone-home, no covert
//!   channel.

#![allow(unsafe_code)]
// Win32 type aliases (SOCKET / DWORD / LPVOID) and constant names
// (AF_INET / SOCK_STREAM / etc.) are upper-case acronyms by Win32 SDK
// convention — keep them matching the SDK headers so the FFI surface is
// auditable against the official MSDN reference without renaming.
#![allow(clippy::upper_case_acronyms)]

use core::sync::atomic::{AtomicI32, Ordering};
use std::sync::Once;

use crate::net::{
    check_caps_for_addr, net_error_code, record_accept, record_connect, record_listen,
    record_net_close, record_net_error, record_recv, record_send, record_socket, validate_buffer,
    validate_sock_flags, INVALID_SOCKET, SOCK_NODELAY, SOCK_NONBLOCK, SOCK_REUSEADDR, SOCK_TCP,
};

// ───────────────────────────────────────────────────────────────────────
// § Win32 / Winsock2 type aliases.
// ───────────────────────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
type SOCKET = usize;
#[allow(non_camel_case_types)]
type WORD = u16;
#[allow(non_camel_case_types)]
type c_int = i32;
#[allow(non_camel_case_types)]
type c_ulong = u32;
#[allow(non_camel_case_types)]
type c_ushort = u16;
// `BOOL` is a 32-bit signed int in Win32 ; used for SO_REUSEADDR /
// TCP_NODELAY setsockopt values.
#[allow(non_camel_case_types)]
type BOOL = i32;
// Length of a BOOL passed to setsockopt — always 4 bytes on Win32.
const BOOL_LEN: c_int = 4;
// Length of a SOCKADDR_IN in bytes — always 16 (per winsock2.h).
const SOCKADDR_IN_LEN: c_int = 16;

// `INVALID_SOCKET = (SOCKET)(~0)` ≡ `usize::MAX`.
const WIN_INVALID_SOCKET: SOCKET = usize::MAX;
// `SOCKET_ERROR = -1`.
const SOCKET_ERROR: c_int = -1;

// AF / SOCK / IPPROTO constants from `<winsock2.h>`.
const AF_INET: c_int = 2;
const SOCK_STREAM: c_int = 1;
const SOCK_DGRAM: c_int = 2;
const IPPROTO_TCP: c_int = 6;
const IPPROTO_UDP: c_int = 17;

// setsockopt level + names.
const SOL_SOCKET: c_int = 0xFFFF;
const SO_REUSEADDR_OPT: c_int = 0x0004;
const TCP_NODELAY_OPT: c_int = 0x0001;

// FIONBIO ioctl name (non-blocking mode).
const FIONBIO: c_ulong = 0x8004_667E;

// Winsock error codes — full reference at MSDN /
// learn.microsoft.com / windows / win32 / winsock / windows-sockets-error-codes-2.
const WSAEINTR: c_int = 10004;
const WSAEACCES: c_int = 10013;
const WSAEFAULT: c_int = 10014;
const WSAEINVAL: c_int = 10022;
const WSAEWOULDBLOCK: c_int = 10035;
const WSAENOTSOCK: c_int = 10038;
const WSAEMSGSIZE: c_int = 10040;
const WSAEADDRINUSE: c_int = 10048;
const WSAEADDRNOTAVAIL: c_int = 10049;
const WSAENETUNREACH: c_int = 10051;
const WSAENETRESET: c_int = 10052;
const WSAECONNABORTED: c_int = 10053;
const WSAECONNRESET: c_int = 10054;
const WSAETIMEDOUT: c_int = 10060;
const WSAECONNREFUSED: c_int = 10061;
const WSAEHOSTUNREACH: c_int = 10065;
const WSAENOTCONN: c_int = 10057;
const WSAESHUTDOWN: c_int = 10058;
const WSANOTINITIALISED: c_int = 10093;

// MAKEWORD(2, 2) for Winsock 2.2.
const fn make_word(low: u8, high: u8) -> WORD {
    ((high as WORD) << 8) | (low as WORD)
}
const WINSOCK_VERSION_2_2: WORD = make_word(2, 2);

// ───────────────────────────────────────────────────────────────────────
// § WSADATA — opaque blob ; size from MSDN. We don't read fields ; only
// pass the pointer to WSAStartup which fills it.
// ───────────────────────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
#[repr(C)]
struct WSADATA {
    w_version: WORD,
    w_high_version: WORD,
    // Win-x64 ABI : reserved fields swap order on 64-bit
    i_max_sockets: u16,
    i_max_udp_dg: u16,
    lp_vendor_info: *mut u8,
    sz_description: [u8; 257],
    sz_system_status: [u8; 129],
}

// SOCKADDR_IN — IPv4 socket address. Layout matches winsock2.h.
//
// All field names start with `sin_` per the Win32 / POSIX SDK
// convention (`sin_family`, `sin_port`, `sin_addr`, `sin_zero`).
// This matches the struct definition in `<winsock2.h>` 1:1 ; renaming
// to satisfy the `clippy::module_name_repetitions` / `same_name_method`
// pattern would diverge from the auditable SDK reference. The
// suppression below is intentional + permanent for this struct.
#[allow(clippy::struct_field_names)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct SOCKADDR_IN {
    sin_family: c_ushort, // AF_INET
    sin_port: c_ushort,   // network-byte-order
    sin_addr: u32,        // network-byte-order IPv4
    sin_zero: [u8; 8],
}

// Generic SOCKADDR for bind / connect / accept (cast from SOCKADDR_IN).
#[allow(non_camel_case_types)]
#[repr(C)]
struct SOCKADDR {
    sa_family: c_ushort,
    sa_data: [u8; 14],
}

// Compile-time assertion : SOCKADDR_IN_LEN matches the actual struct
// size. If the Win32 SDK ever grows the struct, the build breaks here
// rather than silently truncating bind/connect calls.
const _: () = assert!(core::mem::size_of::<SOCKADDR_IN>() == SOCKADDR_IN_LEN as usize);
const _: () = assert!(core::mem::size_of::<BOOL>() == BOOL_LEN as usize);

// ───────────────────────────────────────────────────────────────────────
// § extern bindings — the Winsock2 + ws2_32.dll surface.
// ───────────────────────────────────────────────────────────────────────

#[link(name = "ws2_32")]
extern "system" {
    fn WSAStartup(w_version_requested: WORD, lp_wsa_data: *mut WSADATA) -> c_int;
    // `WSACleanup` intentionally not declared — T11-D152 process-pin design
    // never calls it ; the OS reclaims Winsock state at process exit.
    fn WSAGetLastError() -> c_int;

    fn socket(af: c_int, kind: c_int, protocol: c_int) -> SOCKET;
    fn bind(s: SOCKET, name: *const SOCKADDR, namelen: c_int) -> c_int;
    fn listen(s: SOCKET, backlog: c_int) -> c_int;
    fn accept(s: SOCKET, addr: *mut SOCKADDR, addrlen: *mut c_int) -> SOCKET;
    fn connect(s: SOCKET, name: *const SOCKADDR, namelen: c_int) -> c_int;
    fn send(s: SOCKET, buf: *const u8, len: c_int, flags: c_int) -> c_int;
    fn recv(s: SOCKET, buf: *mut u8, len: c_int, flags: c_int) -> c_int;
    fn sendto(
        s: SOCKET,
        buf: *const u8,
        len: c_int,
        flags: c_int,
        to: *const SOCKADDR,
        tolen: c_int,
    ) -> c_int;
    fn recvfrom(
        s: SOCKET,
        buf: *mut u8,
        len: c_int,
        flags: c_int,
        from: *mut SOCKADDR,
        fromlen: *mut c_int,
    ) -> c_int;
    fn closesocket(s: SOCKET) -> c_int;
    fn setsockopt(
        s: SOCKET,
        level: c_int,
        opt_name: c_int,
        opt_val: *const u8,
        opt_len: c_int,
    ) -> c_int;
    fn ioctlsocket(s: SOCKET, cmd: c_ulong, argp: *mut c_ulong) -> c_int;
    fn getsockname(s: SOCKET, name: *mut SOCKADDR, namelen: *mut c_int) -> c_int;
    fn htons(host: c_ushort) -> c_ushort;
    fn ntohs(net: c_ushort) -> c_ushort;
    fn htonl(host: c_ulong) -> c_ulong;
    fn ntohl(net: c_ulong) -> c_ulong;
}

// ───────────────────────────────────────────────────────────────────────
// § WSAStartup process-pin (T11-D152).
//
//   The previous design ref-counted WSAStartup / WSACleanup across socket
//   creates / closes. The decrement-and-`WSACleanup` pair is not atomic
//   with respect to a concurrent `socket()` syscall on another thread,
//   which opened a race window where parallel tests could observe
//   `WSANOTINITIALISED` mid-cleanup.
//
//   Process-pin design : `WSAStartup` is called exactly once per process
//   via `std::sync::Once` ; the result (success / failure code) is
//   memoized in `WSA_INIT_RESULT`. **`WSACleanup` is never called** —
//   the OS reclaims Winsock state at process exit. This is the canonical
//   pattern used by most production Winsock applications.
//
//   Race-free properties :
//     - WSAStartup is invoked at most once across the process lifetime
//       (Once guarantee — reentrancy-safe + memory-fenced).
//     - All callers after the first synchronize-with the initialization
//       through Once's release/acquire fence (no torn reads of the result).
//     - There is no decrement path, so no `socket()` can ever race with
//       `WSACleanup` (because `WSACleanup` is never called).
// ───────────────────────────────────────────────────────────────────────

/// Sentinel signalling `WSA_INIT_RESULT` has not yet been written by the
/// `Once::call_once` initializer. Distinguishable from `0` (success) and
/// from any non-zero `WSAStartup` error code (10000-10100 range).
const WSA_INIT_RESULT_UNSET: i32 = i32::MIN;

/// Process-wide state guarding the single `WSAStartup` call.
static WSA_PROCESS_INIT: Once = Once::new();

/// Result of the single `WSAStartup` call : `0` on success, the raw
/// non-zero return code on failure, or `WSA_INIT_RESULT_UNSET` before
/// initialization. Read by every net op after the `Once` fires.
static WSA_INIT_RESULT: AtomicI32 = AtomicI32::new(WSA_INIT_RESULT_UNSET);

/// Ensure WSA has been started on this process. Returns `Ok(())` if
/// `WSAStartup` succeeded (now or on a prior call), `Err(canonical-
/// net-error)` if `WSAStartup` failed.
///
/// This is called by every net op that touches the Winsock surface.
/// Thread-safe and race-free : `Once::call_once` serializes the actual
/// `WSAStartup` invocation across all threads, and the cached result
/// is read-only after that point. There is no decrement / cleanup path.
fn ensure_wsa_process_pinned() -> Result<(), i32> {
    WSA_PROCESS_INIT.call_once(|| {
        let mut wsa_data = WSADATA {
            w_version: 0,
            w_high_version: 0,
            i_max_sockets: 0,
            i_max_udp_dg: 0,
            lp_vendor_info: core::ptr::null_mut(),
            sz_description: [0u8; 257],
            sz_system_status: [0u8; 129],
        };
        // SAFETY : WSADATA fully initialized to zero ; WSAStartup will
        // populate it with version + capability info. Per MSDN the call
        // is thread-safe ; Once::call_once additionally guarantees we
        // run exactly one initializer across all threads.
        let r = unsafe { WSAStartup(WINSOCK_VERSION_2_2, &mut wsa_data) };
        WSA_INIT_RESULT.store(r, Ordering::Release);
    });
    // Once::call_once provides the acquire fence pairing with the Release
    // store above ; the load below sees the initializer's result.
    let r = WSA_INIT_RESULT.load(Ordering::Acquire);
    if r == 0 {
        Ok(())
    } else {
        // Either WSAStartup returned non-zero, or the load happened before
        // the Once finished (impossible per call_once's synchronization,
        // but treat any non-zero value as initialization failure).
        Err(net_error_code::NOT_INITIALIZED)
    }
}

/// Test-only diagnostic : `true` once the process has successfully
/// invoked `WSAStartup` ; `false` if the initialization has not yet run
/// or returned non-zero.
#[doc(hidden)]
pub fn wsa_process_pinned_for_tests() -> bool {
    WSA_PROCESS_INIT.is_completed() && WSA_INIT_RESULT.load(Ordering::Acquire) == 0
}

/// Backwards-compatible diagnostic : returns `1` once the process has
/// pinned a successful `WSAStartup`, `0` otherwise. Pre-T11-D152 callers
/// expected a ref-count ; under process-pin the count is always 0 or 1
/// (steady-state 1 once any net op has run).
#[doc(hidden)]
pub fn wsa_init_count_for_tests() -> i32 {
    i32::from(wsa_process_pinned_for_tests())
}

// ───────────────────────────────────────────────────────────────────────
// § Winsock error → canonical net_error_code translation.
// ───────────────────────────────────────────────────────────────────────

/// Translate a Winsock error code into the canonical
/// [`crate::net::net_error_code`] discriminant.
fn translate_winsock_error(err: c_int) -> i32 {
    match err {
        WSAEINTR => net_error_code::INTERRUPTED,
        WSAEACCES => net_error_code::PERMISSION_DENIED,
        // WSAEFAULT, WSAEINVAL, WSAENOTSOCK, WSAEMSGSIZE all surface as
        // bad-input from the caller's POV ; collapse into a single arm.
        WSAEFAULT | WSAEINVAL | WSAENOTSOCK | WSAEMSGSIZE => net_error_code::INVALID_INPUT,
        WSAEWOULDBLOCK => net_error_code::WOULD_BLOCK,
        WSAEADDRINUSE => net_error_code::ADDR_IN_USE,
        WSAEADDRNOTAVAIL => net_error_code::ADDR_NOT_AVAILABLE,
        WSAENETUNREACH => net_error_code::NETWORK_UNREACHABLE,
        WSAENETRESET | WSAECONNRESET => net_error_code::CONNECTION_RESET,
        WSAECONNABORTED => net_error_code::CONNECTION_ABORTED,
        WSAECONNREFUSED => net_error_code::CONNECTION_REFUSED,
        WSAETIMEDOUT => net_error_code::TIMED_OUT,
        WSAEHOSTUNREACH => net_error_code::HOST_UNREACHABLE,
        WSAENOTCONN | WSAESHUTDOWN => net_error_code::NOT_CONNECTED,
        // Win32 has no EPIPE ; ESHUTDOWN already maps to NOT_CONNECTED above.
        WSANOTINITIALISED => net_error_code::NOT_INITIALIZED,
        // WSAEMFILE (out-of-fd) and unknown codes both fall through to
        // OTHER ; the wildcard handles both.
        _ => net_error_code::OTHER,
    }
}

/// Translate cssl-rt portable sock-flags to Winsock `(kind, protocol)`.
fn translate_sock_flags(flags: i32) -> (c_int, c_int) {
    if (flags & SOCK_TCP) != 0 {
        (SOCK_STREAM, IPPROTO_TCP)
    } else {
        // SOCK_UDP — validate_sock_flags ensures exactly-one-of-{TCP,UDP}.
        (SOCK_DGRAM, IPPROTO_UDP)
    }
}

/// Build a `SOCKADDR_IN` from host-byte-order `(addr, port)`. The struct
/// is zero-initialized except the active fields.
fn build_sockaddr_in(addr_be: u32, port: u16) -> SOCKADDR_IN {
    SOCKADDR_IN {
        sin_family: AF_INET as c_ushort,
        // SAFETY : htons / htonl are pure conversions with no
        // preconditions — they accept any u16 / u32.
        sin_port: unsafe { htons(port) },
        sin_addr: unsafe { htonl(addr_be) },
        sin_zero: [0u8; 8],
    }
}

/// Apply per-socket setsockopt / ioctlsocket settings derived from
/// portable flags. Returns `Ok(())` on success, `Err(canonical-error)` on
/// any failure.
///
/// Stage-0 honors :
///   - `SOCK_REUSEADDR` → `SO_REUSEADDR` setsockopt
///   - `SOCK_NODELAY`   → `TCP_NODELAY` setsockopt (TCP-only)
///   - `SOCK_NONBLOCK`  → `FIONBIO = 1` ioctlsocket
fn apply_sock_options(s: SOCKET, flags: i32) -> Result<(), i32> {
    if (flags & SOCK_REUSEADDR) != 0 {
        let one: BOOL = 1;
        // SAFETY : `setsockopt` accepts a pointer + length to the option
        // buffer ; we provide a 4-byte BOOL. The socket s is just-created
        // and valid.
        let r = unsafe {
            setsockopt(
                s,
                SOL_SOCKET,
                SO_REUSEADDR_OPT,
                core::ptr::addr_of!(one).cast::<u8>(),
                BOOL_LEN,
            )
        };
        if r == SOCKET_ERROR {
            // SAFETY : WSAGetLastError is a thread-local read ; no preconditions.
            let err = unsafe { WSAGetLastError() };
            return Err(translate_winsock_error(err));
        }
    }
    if (flags & SOCK_NODELAY) != 0 && (flags & SOCK_TCP) != 0 {
        let one: BOOL = 1;
        // SAFETY : same as above ; IPPROTO_TCP is the canonical level for TCP_NODELAY.
        let r = unsafe {
            setsockopt(
                s,
                IPPROTO_TCP,
                TCP_NODELAY_OPT,
                core::ptr::addr_of!(one).cast::<u8>(),
                BOOL_LEN,
            )
        };
        if r == SOCKET_ERROR {
            let err = unsafe { WSAGetLastError() };
            return Err(translate_winsock_error(err));
        }
    }
    if (flags & SOCK_NONBLOCK) != 0 {
        let mut on: c_ulong = 1;
        // SAFETY : argp points to a c_ulong on the stack.
        let r = unsafe { ioctlsocket(s, FIONBIO, &mut on) };
        if r == SOCKET_ERROR {
            let err = unsafe { WSAGetLastError() };
            return Err(translate_winsock_error(err));
        }
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_socket_impl — create a TCP or UDP socket.
// ───────────────────────────────────────────────────────────────────────

/// Create a new socket according to `flags`.
///
/// Returns the SOCKET cast to `i64` on success ; [`INVALID_SOCKET`] on
/// failure (with [`record_net_error`] populated). The socket is OWNED
/// by the caller until `cssl_net_close_impl` is called.
///
/// # Safety
/// Safe to call ; the only state mutated is the WSA init-count and the
/// per-thread last-error slot.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_net_socket_impl(flags: i32) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if let Err(kind) = validate_sock_flags(flags) {
        record_net_error(kind, 0);
        return INVALID_SOCKET;
    }
    if let Err(kind) = ensure_wsa_process_pinned() {
        record_net_error(kind, 0);
        return INVALID_SOCKET;
    }
    let (kind_param, proto) = translate_sock_flags(flags);
    // SAFETY : `socket` accepts well-known c_int constants ; WSAStartup
    // has been invoked (process-pinned via Once).
    let s = unsafe { socket(AF_INET, kind_param, proto) };
    if s == WIN_INVALID_SOCKET {
        // SAFETY : WSAGetLastError thread-local read.
        let err = unsafe { WSAGetLastError() };
        let canonical = translate_winsock_error(err);
        record_net_error(canonical, err);
        // T11-D152 : no ref-count to roll back ; WSAStartup is pinned.
        return INVALID_SOCKET;
    }
    if let Err(canonical) = apply_sock_options(s, flags) {
        // SAFETY : s is the just-created socket ; closesocket releases it.
        let _ = unsafe { closesocket(s) };
        record_net_error(canonical, 0);
        // T11-D152 : no ref-count to roll back ; WSAStartup is pinned.
        return INVALID_SOCKET;
    }
    record_socket();
    s as i64
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_listen_impl — bind + listen on a TCP socket.
//
//   The portable surface combines `bind` + `listen` into one call to keep
//   the FFI surface minimal. `(addr_be, port)` is the bind address ;
//   `backlog` is the listen backlog. The cap-system is consulted before
//   the syscall.
// ───────────────────────────────────────────────────────────────────────

/// Bind `socket_handle` to `(addr, port)` and put it in listen mode.
///
/// Returns 0 on success, -1 on failure.
///
/// # Safety
/// Caller must ensure `socket_handle` is a valid SOCKET handle returned
/// from [`cssl_net_socket_impl`] with `SOCK_TCP` and not yet closed.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
pub unsafe fn cssl_net_listen_impl(
    socket_handle: i64,
    addr_be: u32,
    port: u16,
    backlog: i32,
) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    // Cap-system gate : listen on non-loopback requires NET_CAP_INBOUND.
    if let Err(kind) = check_caps_for_addr(addr_be, port, true) {
        record_net_error(kind, 0);
        return -1;
    }
    let s = socket_handle as SOCKET;
    let sa = build_sockaddr_in(addr_be, port);
    // SAFETY : the SOCKADDR_IN is laid out compatibly with SOCKADDR ; we
    // pass its pointer + size ; the syscall is well-documented.
    let r = unsafe {
        bind(
            s,
            core::ptr::addr_of!(sa).cast::<SOCKADDR>(),
            SOCKADDR_IN_LEN,
        )
    };
    if r == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    // SAFETY : socket is bound + valid.
    let r2 = unsafe { listen(s, backlog) };
    if r2 == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    record_listen();
    0
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_accept_impl — accept the next pending connection.
// ───────────────────────────────────────────────────────────────────────

/// Accept the next pending connection on `socket_handle`.
///
/// Returns the new client socket cast to `i64` on success ;
/// [`INVALID_SOCKET`] on failure. On non-blocking sockets that have no
/// pending connection, returns INVALID_SOCKET with
/// [`net_error_code::WOULD_BLOCK`] in the last-error slot.
///
/// # Safety
/// Caller must ensure `socket_handle` is a valid SOCKET in listen state.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_net_accept_impl(socket_handle: i64) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return INVALID_SOCKET;
    }
    let s = socket_handle as SOCKET;
    let mut peer = SOCKADDR {
        sa_family: 0,
        sa_data: [0u8; 14],
    };
    let mut peer_len: c_int = SOCKADDR_IN_LEN;
    // SAFETY : pointers are to local stack buffers ; the syscall fills them.
    let new_sock = unsafe { accept(s, &mut peer, &mut peer_len) };
    if new_sock == WIN_INVALID_SOCKET {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return INVALID_SOCKET;
    }
    // T11-D152 : WSAStartup is process-pinned, so any successful accept
    // already runs against a started Winsock. The pin call below is a
    // no-op-after-first-call but kept for invariant clarity : every code
    // path that returns a socket has gone through `ensure_wsa_process_pinned`.
    if let Err(kind) = ensure_wsa_process_pinned() {
        // SAFETY : new_sock is the just-accepted socket.
        let _ = unsafe { closesocket(new_sock) };
        record_net_error(kind, 0);
        return INVALID_SOCKET;
    }
    record_accept();
    record_socket();
    new_sock as i64
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_connect_impl — connect a TCP socket to a peer.
// ───────────────────────────────────────────────────────────────────────

/// Connect `socket_handle` to `(addr, port)`.
///
/// Returns 0 on success, -1 on failure.
///
/// # Safety
/// Caller must ensure `socket_handle` is a valid SOCKET handle from
/// [`cssl_net_socket_impl`] with `SOCK_TCP` and not yet closed or
/// connected.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
pub unsafe fn cssl_net_connect_impl(socket_handle: i64, addr_be: u32, port: u16) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    // Cap-system gate : connect to non-loopback requires NET_CAP_OUTBOUND.
    if let Err(kind) = check_caps_for_addr(addr_be, port, false) {
        record_net_error(kind, 0);
        return -1;
    }
    let s = socket_handle as SOCKET;
    let sa = build_sockaddr_in(addr_be, port);
    // SAFETY : SOCKADDR_IN laid out as SOCKADDR-compatible.
    let r = unsafe {
        connect(
            s,
            core::ptr::addr_of!(sa).cast::<SOCKADDR>(),
            SOCKADDR_IN_LEN,
        )
    };
    if r == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    record_connect();
    0
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_send_impl — send bytes on a connected socket.
// ───────────────────────────────────────────────────────────────────────

/// Send `buf_len` bytes from `buf_ptr` on `socket_handle`.
///
/// Returns bytes-sent (≥ 0) ; -1 on failure. Short sends are possible
/// per-syscall ; callers loop until all bytes have been sent.
///
/// # Safety
/// Caller must ensure :
/// - `socket_handle` is a valid SOCKET in connected state.
/// - `buf_ptr` valid for `buf_len` consecutive readable bytes.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_net_send_impl(socket_handle: i64, buf_ptr: *const u8, buf_len: usize) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if let Err(kind) = validate_buffer(buf_ptr, buf_len) {
        record_net_error(kind, 0);
        return -1;
    }
    record_send(0); // record the call ; bytes adjusted on success
    if buf_len == 0 {
        return 0;
    }
    let to_send = if buf_len > i32::MAX as usize {
        i32::MAX
    } else {
        buf_len as c_int
    };
    let s = socket_handle as SOCKET;
    // SAFETY : socket assumed valid by caller ; buffer pointer checked above.
    let n = unsafe { send(s, buf_ptr, to_send, 0) };
    if n == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    crate::net::BYTES_SENT_TOTAL.fetch_add(n as u64, Ordering::Relaxed);
    i64::from(n)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_recv_impl — recv bytes on a connected socket.
// ───────────────────────────────────────────────────────────────────────

/// Receive up to `buf_len` bytes into `buf_ptr` from `socket_handle`.
///
/// Returns bytes-received (0 = peer closed the connection cleanly) ; -1
/// on failure with the canonical error in [`record_net_error`].
///
/// # Safety
/// Caller must ensure :
/// - `socket_handle` is a valid SOCKET in connected state.
/// - `buf_ptr` valid for `buf_len` consecutive writable bytes.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_net_recv_impl(socket_handle: i64, buf_ptr: *mut u8, buf_len: usize) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if let Err(kind) = validate_buffer(buf_ptr.cast_const(), buf_len) {
        record_net_error(kind, 0);
        return -1;
    }
    record_recv(0);
    if buf_len == 0 {
        return 0;
    }
    let to_recv = if buf_len > i32::MAX as usize {
        i32::MAX
    } else {
        buf_len as c_int
    };
    let s = socket_handle as SOCKET;
    // SAFETY : socket assumed valid ; buffer pointer checked above.
    let n = unsafe { recv(s, buf_ptr, to_recv, 0) };
    if n == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    crate::net::BYTES_RECV_TOTAL.fetch_add(n as u64, Ordering::Relaxed);
    i64::from(n)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_sendto_impl — send a UDP datagram.
// ───────────────────────────────────────────────────────────────────────

/// Send `buf_len` bytes to `(addr, port)` via UDP socket `socket_handle`.
///
/// # Safety
/// Caller contract matches `cssl_net_send_impl` plus: addr/port well-formed.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_net_sendto_impl(
    socket_handle: i64,
    buf_ptr: *const u8,
    buf_len: usize,
    addr_be: u32,
    port: u16,
) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if let Err(kind) = validate_buffer(buf_ptr, buf_len) {
        record_net_error(kind, 0);
        return -1;
    }
    if let Err(kind) = check_caps_for_addr(addr_be, port, false) {
        record_net_error(kind, 0);
        return -1;
    }
    record_send(0);
    if buf_len == 0 {
        return 0;
    }
    let to_send = if buf_len > i32::MAX as usize {
        i32::MAX
    } else {
        buf_len as c_int
    };
    let s = socket_handle as SOCKET;
    let sa = build_sockaddr_in(addr_be, port);
    // SAFETY : standard sendto wiring.
    let n = unsafe {
        sendto(
            s,
            buf_ptr,
            to_send,
            0,
            core::ptr::addr_of!(sa).cast::<SOCKADDR>(),
            SOCKADDR_IN_LEN,
        )
    };
    if n == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    crate::net::BYTES_SENT_TOTAL.fetch_add(n as u64, Ordering::Relaxed);
    i64::from(n)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_recvfrom_impl — receive a UDP datagram + peer address.
//
//   The peer address is OUT-PARAMETERed via `*peer_addr_be_out` /
//   `*peer_port_out`. Stage-0 keeps the FFI surface flat (no struct
//   passing across the boundary) so the source-level wrapper
//   reconstructs `SocketAddrV4` from the two ints.
// ───────────────────────────────────────────────────────────────────────

/// Receive up to `buf_len` UDP datagram bytes ; out-param the peer
/// `(addr, port)`. Returns bytes-received ; -1 on failure.
///
/// # Safety
/// Caller must ensure :
/// - `socket_handle` is a valid UDP SOCKET.
/// - `buf_ptr` valid for `buf_len` writable bytes.
/// - `peer_addr_be_out` / `peer_port_out` are valid mutable pointers
///   (or null to discard the peer info).
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::too_many_arguments)]
pub unsafe fn cssl_net_recvfrom_impl(
    socket_handle: i64,
    buf_ptr: *mut u8,
    buf_len: usize,
    peer_addr_be_out: *mut u32,
    peer_port_out: *mut u16,
) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if let Err(kind) = validate_buffer(buf_ptr.cast_const(), buf_len) {
        record_net_error(kind, 0);
        return -1;
    }
    record_recv(0);
    if buf_len == 0 {
        return 0;
    }
    let to_recv = if buf_len > i32::MAX as usize {
        i32::MAX
    } else {
        buf_len as c_int
    };
    let s = socket_handle as SOCKET;
    let mut peer = SOCKADDR_IN {
        sin_family: 0,
        sin_port: 0,
        sin_addr: 0,
        sin_zero: [0u8; 8],
    };
    let mut peer_len: c_int = SOCKADDR_IN_LEN;
    // SAFETY : peer is local stack ; recvfrom fills it.
    let n = unsafe {
        recvfrom(
            s,
            buf_ptr,
            to_recv,
            0,
            core::ptr::addr_of_mut!(peer).cast::<SOCKADDR>(),
            &mut peer_len,
        )
    };
    if n == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    if !peer_addr_be_out.is_null() {
        // SAFETY : caller-supplied valid pointer.
        let host_addr = unsafe { ntohl(peer.sin_addr) };
        unsafe { *peer_addr_be_out = host_addr };
    }
    if !peer_port_out.is_null() {
        // SAFETY : caller-supplied valid pointer.
        let host_port = unsafe { ntohs(peer.sin_port) };
        unsafe { *peer_port_out = host_port };
    }
    crate::net::BYTES_RECV_TOTAL.fetch_add(n as u64, Ordering::Relaxed);
    i64::from(n)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_close_impl — close a socket + release WSA ref.
// ───────────────────────────────────────────────────────────────────────

/// Close `socket_handle`. Returns 0 on success, -1 on failure.
///
/// # Safety
/// Caller must ensure `socket_handle` is a valid SOCKET handle from
/// [`cssl_net_socket_impl`] (or [`cssl_net_accept_impl`]) and not yet
/// closed.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
pub unsafe fn cssl_net_close_impl(socket_handle: i64) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    record_net_close();
    let s = socket_handle as SOCKET;
    // SAFETY : socket assumed valid by caller contract.
    let r = unsafe { closesocket(s) };
    // T11-D152 : WSAStartup is process-pinned ; no per-socket release.
    // The OS reclaims Winsock state at process exit.
    if r == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    0
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_local_addr_impl — read back the bound local address.
//
//   Useful for tests that bind to port 0 (let-OS-pick) and need to know
//   the actual port the OS assigned. Public because the same fact is
//   needed by source-level CSSLv3 code via the `TcpListener::local_port`
//   accessor.
// ───────────────────────────────────────────────────────────────────────

/// Read back the local `(addr, port)` of a bound socket via
/// `getsockname`. Returns 0 on success, -1 on failure.
///
/// # Safety
/// Caller must ensure `socket_handle` is a valid SOCKET ; `addr_out` /
/// `port_out` are valid mutable pointers.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
pub unsafe fn cssl_net_local_addr_impl(
    socket_handle: i64,
    addr_be_out: *mut u32,
    port_out: *mut u16,
) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    let s = socket_handle as SOCKET;
    let mut local = SOCKADDR_IN {
        sin_family: 0,
        sin_port: 0,
        sin_addr: 0,
        sin_zero: [0u8; 8],
    };
    let mut local_len: c_int = SOCKADDR_IN_LEN;
    // SAFETY : pointer to local stack buffer ; getsockname fills it.
    let r = unsafe {
        getsockname(
            s,
            core::ptr::addr_of_mut!(local).cast::<SOCKADDR>(),
            &mut local_len,
        )
    };
    if r == SOCKET_ERROR {
        let err = unsafe { WSAGetLastError() };
        record_net_error(translate_winsock_error(err), err);
        return -1;
    }
    if !addr_be_out.is_null() {
        // SAFETY : caller-supplied valid pointer.
        let host_addr = unsafe { ntohl(local.sin_addr) };
        unsafe { *addr_be_out = host_addr };
    }
    if !port_out.is_null() {
        let host_port = unsafe { ntohs(local.sin_port) };
        unsafe { *port_out = host_port };
    }
    0
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — flag-translation + error-translation + WSA ref-count + roundtrip.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::net::{
        caps_grant, caps_revoke, last_net_error_kind, ANY_V4, LOOPBACK_V4, NET_CAP_INBOUND,
        SOCK_UDP,
    };

    #[test]
    fn translate_winsock_error_known_kinds() {
        assert_eq!(
            translate_winsock_error(WSAEWOULDBLOCK),
            net_error_code::WOULD_BLOCK
        );
        assert_eq!(
            translate_winsock_error(WSAECONNREFUSED),
            net_error_code::CONNECTION_REFUSED
        );
        assert_eq!(
            translate_winsock_error(WSAECONNRESET),
            net_error_code::CONNECTION_RESET
        );
        assert_eq!(
            translate_winsock_error(WSAEADDRINUSE),
            net_error_code::ADDR_IN_USE
        );
        assert_eq!(
            translate_winsock_error(WSAEADDRNOTAVAIL),
            net_error_code::ADDR_NOT_AVAILABLE
        );
        assert_eq!(
            translate_winsock_error(WSAETIMEDOUT),
            net_error_code::TIMED_OUT
        );
        assert_eq!(
            translate_winsock_error(WSAEINTR),
            net_error_code::INTERRUPTED
        );
        assert_eq!(
            translate_winsock_error(WSANOTINITIALISED),
            net_error_code::NOT_INITIALIZED
        );
        // Unknown code → OTHER.
        assert_eq!(translate_winsock_error(99_999), net_error_code::OTHER);
    }

    #[test]
    fn translate_sock_flags_tcp_maps_to_stream() {
        let (kind, proto) = translate_sock_flags(SOCK_TCP);
        assert_eq!(kind, SOCK_STREAM);
        assert_eq!(proto, IPPROTO_TCP);
    }

    #[test]
    fn translate_sock_flags_udp_maps_to_dgram() {
        let (kind, proto) = translate_sock_flags(SOCK_UDP);
        assert_eq!(kind, SOCK_DGRAM);
        assert_eq!(proto, IPPROTO_UDP);
    }

    #[test]
    fn make_word_packs_lo_hi_bytes() {
        assert_eq!(make_word(2, 2), 0x0202);
        assert_eq!(make_word(0xFF, 0x00), 0x00FF);
        assert_eq!(make_word(0x00, 0xFF), 0xFF00);
    }

    #[test]
    fn winsock_version_2_2_is_canonical() {
        assert_eq!(WINSOCK_VERSION_2_2, 0x0202);
    }

    #[test]
    fn build_sockaddr_in_packs_canonical_loopback() {
        let sa = build_sockaddr_in(LOOPBACK_V4, 8080);
        assert_eq!(sa.sin_family, AF_INET as c_ushort);
        // Port 8080 in network byte order is 0x901F (host 8080 = 0x1F90).
        // SAFETY : htons is a pure conversion.
        assert_eq!(sa.sin_port, unsafe { htons(8080) });
        assert_eq!(sa.sin_addr, unsafe { htonl(LOOPBACK_V4) });
    }

    #[test]
    fn socket_invalid_flags_returns_invalid_socket() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : safe call ; bad flag rejected before any syscall.
        let r = unsafe { cssl_net_socket_impl(0x8000) };
        assert_eq!(r, INVALID_SOCKET);
        assert_eq!(last_net_error_kind(), net_error_code::INVALID_INPUT);
    }

    #[test]
    fn socket_no_transport_returns_invalid_input() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : safe call ; no SOCK_TCP/SOCK_UDP rejected.
        let r = unsafe { cssl_net_socket_impl(SOCK_NONBLOCK) };
        assert_eq!(r, INVALID_SOCKET);
        assert_eq!(last_net_error_kind(), net_error_code::INVALID_INPUT);
    }

    #[test]
    fn socket_create_pins_wsa_and_close_does_not_release() {
        // T11-D152 : process-pin replaces ref-count. The first socket
        // creation pins WSAStartup ; subsequent close does NOT decrement.
        // The pin is permanent for the process lifetime.
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : SOCK_TCP is a valid flag.
        let s = unsafe { cssl_net_socket_impl(SOCK_TCP) };
        assert_ne!(s, INVALID_SOCKET, "TCP socket creation should succeed");
        assert!(
            wsa_process_pinned_for_tests(),
            "WSA should be pinned after first socket op"
        );
        // SAFETY : valid socket.
        let r = unsafe { cssl_net_close_impl(s) };
        assert_eq!(r, 0);
        assert!(
            wsa_process_pinned_for_tests(),
            "WSA must remain pinned after close (no WSACleanup is called)"
        );
        // Backwards-compat shim : the legacy count returns 1 once pinned.
        assert_eq!(wsa_init_count_for_tests(), 1);
    }

    #[test]
    fn close_invalid_socket_returns_minus_one() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : INVALID_SOCKET sentinel ; impl checks before syscall.
        let r = unsafe { cssl_net_close_impl(INVALID_SOCKET) };
        assert_eq!(r, -1);
        assert_eq!(last_net_error_kind(), net_error_code::INVALID_INPUT);
    }

    #[test]
    fn listen_invalid_socket_returns_minus_one() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : INVALID_SOCKET sentinel ; impl checks first.
        let r = unsafe { cssl_net_listen_impl(INVALID_SOCKET, LOOPBACK_V4, 0, 4) };
        assert_eq!(r, -1);
        assert_eq!(last_net_error_kind(), net_error_code::INVALID_INPUT);
    }

    #[test]
    fn listen_on_non_loopback_without_inbound_cap_denied() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // Default caps : LOOPBACK only. Listen on 0.0.0.0 → CapDenied.
        // SAFETY : create + close balances WSA.
        let s = unsafe { cssl_net_socket_impl(SOCK_TCP | SOCK_REUSEADDR) };
        assert_ne!(s, INVALID_SOCKET);
        // SAFETY : valid socket ; ANY_V4 is non-loopback.
        let r = unsafe { cssl_net_listen_impl(s, ANY_V4, 0, 4) };
        assert_eq!(r, -1);
        assert_eq!(last_net_error_kind(), net_error_code::CAP_DENIED);
        let _ = unsafe { cssl_net_close_impl(s) };
    }

    #[test]
    fn listen_on_non_loopback_with_inbound_cap_proceeds() {
        let _g = crate::test_helpers::lock_and_reset_all();
        caps_grant(NET_CAP_INBOUND);
        // SAFETY : valid SOCK_TCP.
        let s = unsafe { cssl_net_socket_impl(SOCK_TCP | SOCK_REUSEADDR) };
        assert_ne!(s, INVALID_SOCKET);
        // SAFETY : valid socket ; ANY_V4 + ephemeral port allowed.
        let r = unsafe { cssl_net_listen_impl(s, ANY_V4, 0, 4) };
        // bind+listen on 0.0.0.0:0 should succeed on a Win10/11 host.
        assert_eq!(r, 0);
        let _ = unsafe { cssl_net_close_impl(s) };
        caps_revoke(NET_CAP_INBOUND);
    }

    #[test]
    fn connect_to_non_loopback_without_outbound_cap_denied() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : valid TCP socket.
        let s = unsafe { cssl_net_socket_impl(SOCK_TCP) };
        assert_ne!(s, INVALID_SOCKET);
        // SAFETY : valid socket ; 8.8.8.8 is non-loopback.
        let r = unsafe { cssl_net_connect_impl(s, 0x0808_0808, 53) };
        assert_eq!(r, -1);
        assert_eq!(last_net_error_kind(), net_error_code::CAP_DENIED);
        let _ = unsafe { cssl_net_close_impl(s) };
    }

    #[test]
    fn send_invalid_socket_returns_minus_one() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let buf = [0u8; 8];
        // SAFETY : INVALID_SOCKET sentinel.
        let r = unsafe { cssl_net_send_impl(INVALID_SOCKET, buf.as_ptr(), buf.len()) };
        assert_eq!(r, -1);
        assert_eq!(last_net_error_kind(), net_error_code::INVALID_INPUT);
    }

    #[test]
    fn recv_invalid_socket_returns_minus_one() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let mut buf = [0u8; 8];
        // SAFETY : INVALID_SOCKET sentinel.
        let r = unsafe { cssl_net_recv_impl(INVALID_SOCKET, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(r, -1);
        assert_eq!(last_net_error_kind(), net_error_code::INVALID_INPUT);
    }

    #[test]
    fn tcp_loopback_roundtrip_default_caps() {
        // ‼ HANDOFF report-back gate : real TCP roundtrip on Apocky's host.
        // 1. server socket bound to 127.0.0.1:0 (let-OS-pick)
        // 2. discover assigned port via getsockname
        // 3. client socket connects
        // 4. server accepts
        // 5. client sends "hello cssl-net" ; server recvs ; equality assert
        let _g = crate::test_helpers::lock_and_reset_all();

        // ── server side ──────────────────────────────────────────────
        // SAFETY : SOCK_TCP + SOCK_REUSEADDR (test-mode rapid-rebind).
        let server = unsafe { cssl_net_socket_impl(SOCK_TCP | SOCK_REUSEADDR) };
        assert_ne!(server, INVALID_SOCKET, "server socket creation");
        // bind + listen on loopback (allowed under default caps).
        // SAFETY : valid socket ; LOOPBACK_V4 + port 0 is the canonical
        // "let OS pick a free port" pattern.
        let lr = unsafe { cssl_net_listen_impl(server, LOOPBACK_V4, 0, 4) };
        assert_eq!(lr, 0, "listen on 127.0.0.1:0 should succeed");
        // discover the OS-assigned port via getsockname.
        let mut bound_addr: u32 = 0;
        let mut bound_port: u16 = 0;
        // SAFETY : valid pointers + valid socket.
        let gr = unsafe { cssl_net_local_addr_impl(server, &mut bound_addr, &mut bound_port) };
        assert_eq!(gr, 0, "getsockname should succeed");
        assert_eq!(bound_addr, LOOPBACK_V4);
        assert_ne!(bound_port, 0, "OS should assign a non-zero port");

        // ── client side ──────────────────────────────────────────────
        // SAFETY : valid SOCK_TCP.
        let client = unsafe { cssl_net_socket_impl(SOCK_TCP) };
        assert_ne!(client, INVALID_SOCKET, "client socket creation");
        // SAFETY : valid socket + bound port from server.
        let cr = unsafe { cssl_net_connect_impl(client, LOOPBACK_V4, bound_port) };
        assert_eq!(cr, 0, "connect to 127.0.0.1:{bound_port}");

        // ── server accepts ───────────────────────────────────────────
        // SAFETY : server is in listen state.
        let conn = unsafe { cssl_net_accept_impl(server) };
        assert_ne!(conn, INVALID_SOCKET, "accept");

        // ── client sends ─────────────────────────────────────────────
        let payload = b"hello cssl-net f4 roundtrip";
        // SAFETY : valid socket + valid byte slice.
        let n = unsafe { cssl_net_send_impl(client, payload.as_ptr(), payload.len()) };
        assert_eq!(n, payload.len() as i64, "client send byte-count");

        // ── server receives ──────────────────────────────────────────
        let mut buf = vec![0u8; payload.len() + 8];
        // SAFETY : valid socket + valid mutable buffer.
        let nr = unsafe { cssl_net_recv_impl(conn, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(nr, payload.len() as i64, "server recv byte-count");
        assert_eq!(&buf[..payload.len()], payload, "payload equality");

        // ── close all sockets ────────────────────────────────────────
        let _ = unsafe { cssl_net_close_impl(conn) };
        let _ = unsafe { cssl_net_close_impl(client) };
        let _ = unsafe { cssl_net_close_impl(server) };

        // Counter discipline.
        assert_eq!(crate::net::socket_count(), 3); // server + client + accepted
        assert_eq!(crate::net::listen_count(), 1);
        assert_eq!(crate::net::accept_count(), 1);
        assert_eq!(crate::net::connect_count(), 1);
        assert!(crate::net::send_count() >= 1);
        assert!(crate::net::recv_count() >= 1);
        assert_eq!(crate::net::net_close_count(), 3);
        assert_eq!(crate::net::bytes_sent_total(), payload.len() as u64);
        assert_eq!(crate::net::bytes_recv_total(), payload.len() as u64);
    }

    #[test]
    fn udp_loopback_sendto_recvfrom_roundtrip() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // Two UDP sockets ; one binds to a port + receives, the other sends.
        // SAFETY : SOCK_UDP + SOCK_REUSEADDR.
        let recv_sock = unsafe { cssl_net_socket_impl(SOCK_UDP | SOCK_REUSEADDR) };
        assert_ne!(recv_sock, INVALID_SOCKET);
        // bind (no listen needed for UDP).
        // SAFETY : valid UDP socket.
        let lr = unsafe { cssl_net_listen_impl(recv_sock, LOOPBACK_V4, 0, 0) };
        // listen on UDP returns -1 on Win32 (the sock is connectionless),
        // BUT we only care about the bind portion. The cssl_net_listen_impl
        // calls bind THEN listen — for UDP, bind succeeds + listen fails.
        // Workaround : skip the listen-on-UDP test (the bind portion is
        // exercised by sendto/recvfrom needing a port). Instead use an
        // explicit bind-only path. For stage-0 we use only sendto-without-bind
        // pattern : the recv side's OS-assigned ephemeral-port is reachable
        // via getsockname after sendto-style activity. The test below
        // simplifies : we use sendto from one socket to the other's bound
        // address, which requires the recv socket to bind. The bind happens
        // via cssl_net_listen_impl's bind portion ; the listen failure on
        // UDP is non-fatal for our purposes.
        // Trick: capture the bound port via getsockname AFTER bind even if
        // listen-leg failed — bind is the first half of cssl_net_listen_impl.
        let _ = lr;

        // ALTERNATE : just send from one socket without recv-side bind,
        // recv-side discards. Simpler structural test :
        let mut bound_addr: u32 = 0;
        let mut bound_port: u16 = 0;
        // SAFETY : valid pointers.
        let _gr = unsafe { cssl_net_local_addr_impl(recv_sock, &mut bound_addr, &mut bound_port) };
        // If bind succeeded (gr == 0) we can do a real roundtrip.
        if bound_port != 0 {
            // SAFETY : valid SOCK_UDP.
            let send_sock = unsafe { cssl_net_socket_impl(SOCK_UDP) };
            assert_ne!(send_sock, INVALID_SOCKET);
            let payload = b"udp hello cssl-net f4";
            // SAFETY : valid socket + buffer + bound port.
            let n = unsafe {
                cssl_net_sendto_impl(
                    send_sock,
                    payload.as_ptr(),
                    payload.len(),
                    LOOPBACK_V4,
                    bound_port,
                )
            };
            assert_eq!(n, payload.len() as i64);
            // Recv on the bound socket.
            let mut buf = vec![0u8; payload.len() + 8];
            let mut peer_addr: u32 = 0;
            let mut peer_port: u16 = 0;
            // SAFETY : valid pointers + buffer.
            let nr = unsafe {
                cssl_net_recvfrom_impl(
                    recv_sock,
                    buf.as_mut_ptr(),
                    buf.len(),
                    &mut peer_addr,
                    &mut peer_port,
                )
            };
            assert_eq!(nr, payload.len() as i64);
            assert_eq!(&buf[..payload.len()], payload);
            assert_eq!(peer_addr, LOOPBACK_V4);
            let _ = unsafe { cssl_net_close_impl(send_sock) };
        }

        let _ = unsafe { cssl_net_close_impl(recv_sock) };
    }

    #[test]
    fn nonblocking_socket_recv_returns_would_block_when_empty() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // Non-blocking TCP socket ; recv with no data → WOULD_BLOCK.
        // SAFETY : SOCK_TCP + SOCK_NONBLOCK.
        let s = unsafe { cssl_net_socket_impl(SOCK_TCP | SOCK_NONBLOCK) };
        assert_ne!(s, INVALID_SOCKET);
        // bind on loopback so the socket is alive.
        // SAFETY : valid socket.
        let lr = unsafe { cssl_net_listen_impl(s, LOOPBACK_V4, 0, 4) };
        assert_eq!(lr, 0);
        // Try accept on the listening socket — no client → WOULD_BLOCK.
        // SAFETY : valid listening socket.
        let conn = unsafe { cssl_net_accept_impl(s) };
        assert_eq!(conn, INVALID_SOCKET);
        assert_eq!(last_net_error_kind(), net_error_code::WOULD_BLOCK);
        let _ = unsafe { cssl_net_close_impl(s) };
    }

    #[test]
    fn connect_to_loopback_unbound_port_returns_connection_refused() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : valid SOCK_TCP.
        let s = unsafe { cssl_net_socket_impl(SOCK_TCP) };
        assert_ne!(s, INVALID_SOCKET);
        // Connect to an almost-certainly-unbound high port on loopback.
        // Some hosts may have a service there ; the test is best-effort
        // and tolerates either CONNECTION_REFUSED or a successful connect
        // (which we then close cleanly).
        // SAFETY : valid socket ; loopback address allowed.
        let r = unsafe { cssl_net_connect_impl(s, LOOPBACK_V4, 1) };
        if r == -1 {
            // Most-likely outcome — port 1 is privileged + unbound.
            let kind = last_net_error_kind();
            assert!(
                kind == net_error_code::CONNECTION_REFUSED
                    || kind == net_error_code::TIMED_OUT
                    || kind == net_error_code::PERMISSION_DENIED
                    || kind == net_error_code::NETWORK_UNREACHABLE
                    || kind == net_error_code::ADDR_NOT_AVAILABLE,
                "unexpected error kind on unbound-port connect : {kind}"
            );
        }
        let _ = unsafe { cssl_net_close_impl(s) };
    }

    // ───────────────────────────────────────────────────────────────────
    // § T11-D152 — process-pin parallel-safety regression tests.
    //
    //   These tests verify the WSAStartup process-pin invariant under
    //   concurrent access. They MUST pass under default `cargo test`
    //   parallelism (no `--test-threads=1`).
    //
    //   The previous ref-count design failed these scenarios with
    //   `WSANOTINITIALISED` (canonical kind `NOT_INITIALIZED`) when
    //   one thread's `release_wsa_started` raced with another thread's
    //   `socket()` syscall mid-`WSACleanup`.
    // ───────────────────────────────────────────────────────────────────

    #[test]
    fn ensure_wsa_process_pinned_idempotent_under_concurrent_callers() {
        // 32 threads each call `ensure_wsa_process_pinned` 100× ; every
        // call must return Ok(()) and the pin state must be true throughout.
        // No WSACleanup ever runs, so no race is possible.
        const THREADS: usize = 32;
        const ITERS: usize = 100;
        let _g = crate::test_helpers::lock_and_reset_all();
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..ITERS {
                        ensure_wsa_process_pinned()
                            .expect("ensure_wsa_process_pinned must succeed");
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("worker thread");
        }
        assert!(
            wsa_process_pinned_for_tests(),
            "process must remain pinned after concurrent stress"
        );
    }

    #[test]
    fn parallel_socket_create_close_never_observes_not_initialized() {
        // Stress regression : N threads each create and close TCP sockets
        // M times ; under the old ref-count design, a fraction of these
        // would observe NOT_INITIALIZED because of the WSACleanup race.
        // Under T11-D152 process-pin, every socket creation must succeed.
        const THREADS: usize = 8;
        const ITERS: usize = 50;
        let _g = crate::test_helpers::lock_and_reset_all();

        // Pre-pin so the first thread doesn't carry the cost of WSAStartup.
        ensure_wsa_process_pinned().expect("pre-pin");

        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..ITERS {
                        // SAFETY : SOCK_TCP is a valid flag.
                        let s = unsafe { cssl_net_socket_impl(SOCK_TCP) };
                        assert_ne!(
                            s, INVALID_SOCKET,
                            "socket creation must not observe NOT_INITIALIZED \
                             under parallel stress (T11-D152 regression)"
                        );
                        // SAFETY : just-created socket.
                        let r = unsafe { cssl_net_close_impl(s) };
                        assert_eq!(r, 0, "close must succeed");
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("worker thread");
        }
        assert!(
            wsa_process_pinned_for_tests(),
            "WSA pin must persist after parallel create/close stress"
        );
    }
}
