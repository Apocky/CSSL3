//! § cssl-rt networking — Unix / BSD-sockets platform layer (T11-D82, S7-F4).
//!
//! § ROLE
//!   Hand-rolled libc syscall bindings for `socket` / `bind` / `listen` /
//!   `accept` / `connect` / `send` / `recv` / `sendto` / `recvfrom` /
//!   `close` / `setsockopt` / `fcntl` / `getsockname` / `htons` / `htonl` /
//!   `ntohs` / `ntohl`. Per the dispatch-plan landmines we deliberately
//!   avoid pulling in the `libc` crate — the BSD-sockets surface is
//!   small + stable since the Berkeley 4.2BSD release (1983), and the
//!   cssl-rt convention established at T11-D52 (allocator) and T11-D76
//!   (file I/O) is "hand-rolled `extern "C"` declarations with each
//!   `unsafe` block carrying an inline SAFETY paragraph".
//!
//! § PLATFORMS
//!   Compiled for any `cfg(not(target_os = "windows"))` host — Linux +
//!   macOS share this implementation. AF_INET / SOCK_STREAM / SOCK_DGRAM
//!   constants are identical across SUSv2 hosts ; the `errno` accessor
//!   differs (`__errno_location` on glibc / musl, `__error` on macOS).
//!
//! § ABI NOTES
//!   - `c_int` = 32-bit `i32` on every supported platform.
//!   - `socklen_t` = 32-bit `u32` on every supported platform.
//!   - `ssize_t` = signed 64-bit on every supported 64-bit Unix.
//!   - `sa_family_t` = 16-bit `u16` on every supported platform.
//!   - SOCKADDR_IN layout is canonical : `(family, port, addr, padding)`.
//!
//! § INVARIANTS
//!   - On failure, every `cssl_net_*_impl` returns `-1 as i64` AND
//!     records the canonical error in [`crate::net::record_net_error`].
//!     The high 32 bits of the slot carry the raw `errno` value.
//!   - Send / recv byte counts are bounded to `i32::MAX` per call to
//!     match the Win32 path. POSIX `send / recv` accept up to
//!     `SSIZE_MAX` but we keep the unified ceiling.
//!   - The `O_NONBLOCK` ioctl + flag-translation table is the single
//!     source of truth for how cssl-rt portable flags map to BSD
//!     socket-creation params.
//!
//! § DEFERRED — non-Windows CI
//!   Apocky's host is Windows ; the Unix path is structurally tested
//!   (compile-only) until a non-Windows CI runner exists. The Linux /
//!   macOS path is structurally identical to glibc's `socket / bind /
//!   listen / accept / connect / send / recv / close` signatures, so
//!   behavior is determined by well-known POSIX semantics rather than
//!   per-platform quirks. Mirrors the `crate::io_unix` discipline from
//!   T11-D76.
//!
//! § PRIME-DIRECTIVE
//!   See `crate::net` doc-block for the full attestation. POSIX-specific
//!   note : `errno` is thread-local ; our error-translation reads it
//!   after every failing syscall and never inspects it speculatively.

#![allow(unsafe_code)]
#![allow(clippy::upper_case_acronyms)]

use core::ffi::c_void;
use core::sync::atomic::Ordering;

use crate::net::{
    check_caps_for_addr, net_error_code, record_accept, record_connect, record_listen,
    record_net_close, record_net_error, record_recv, record_send, record_socket, validate_buffer,
    validate_sock_flags, INVALID_SOCKET, SOCK_NODELAY, SOCK_NONBLOCK, SOCK_REUSEADDR, SOCK_TCP,
};

// ───────────────────────────────────────────────────────────────────────
// § POSIX type aliases.
// ───────────────────────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
type c_int = i32;
#[allow(non_camel_case_types)]
type c_uint = u32;
#[allow(non_camel_case_types)]
type c_ushort = u16;
#[allow(non_camel_case_types)]
type c_ulong = u32;
#[allow(non_camel_case_types)]
type size_t = usize;
#[allow(non_camel_case_types)]
type ssize_t = isize;
#[allow(non_camel_case_types)]
type socklen_t = u32;
#[allow(non_camel_case_types)]
type sa_family_t = u16;
#[allow(non_camel_case_types)]
type in_port_t = u16;
#[allow(non_camel_case_types)]
type in_addr_t = u32;

// ───────────────────────────────────────────────────────────────────────
// § BSD socket constants — values from POSIX.1-2017.
// ───────────────────────────────────────────────────────────────────────

const AF_INET: c_int = 2;
const SOCK_STREAM: c_int = 1;
const SOCK_DGRAM: c_int = 2;

#[cfg(target_os = "linux")]
const IPPROTO_TCP: c_int = 6;
#[cfg(target_os = "linux")]
const IPPROTO_UDP: c_int = 17;
#[cfg(target_os = "macos")]
const IPPROTO_TCP: c_int = 6;
#[cfg(target_os = "macos")]
const IPPROTO_UDP: c_int = 17;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const IPPROTO_TCP: c_int = 6;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const IPPROTO_UDP: c_int = 17;

const SOL_SOCKET: c_int = 1;
const SO_REUSEADDR_OPT: c_int = 2;
const TCP_NODELAY_OPT: c_int = 1;

const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const O_NONBLOCK: c_int = 0o4000; // Linux + macOS agree on the bit pattern

// errno values — agree across Linux + macOS for this set.
const EINTR: c_int = 4;
const EBADF: c_int = 9;
const EAGAIN_LINUX: c_int = 11;
const EWOULDBLOCK_LINUX: c_int = 11; // == EAGAIN on Linux
const EAGAIN_MACOS: c_int = 35;
const EACCES: c_int = 13;
const EFAULT: c_int = 14;
const EINVAL: c_int = 22;
const EPIPE: c_int = 32;
const ENOTSOCK: c_int = 88; // (Linux) — macOS uses 38, fallback OTHER ok
const EADDRINUSE: c_int = 98; // Linux ; macOS = 48
const EADDRINUSE_MACOS: c_int = 48;
const EADDRNOTAVAIL: c_int = 99; // Linux ; macOS = 49
const EADDRNOTAVAIL_MACOS: c_int = 49;
const ENETDOWN: c_int = 100;
const ENETUNREACH: c_int = 101; // Linux ; macOS = 51
const ENETUNREACH_MACOS: c_int = 51;
const ENETRESET: c_int = 102;
const ECONNABORTED: c_int = 103; // Linux ; macOS = 53
const ECONNABORTED_MACOS: c_int = 53;
const ECONNRESET: c_int = 104; // Linux ; macOS = 54
const ECONNRESET_MACOS: c_int = 54;
const ENOTCONN: c_int = 107; // Linux ; macOS = 57
const ENOTCONN_MACOS: c_int = 57;
const ETIMEDOUT: c_int = 110; // Linux ; macOS = 60
const ETIMEDOUT_MACOS: c_int = 60;
const ECONNREFUSED: c_int = 111; // Linux ; macOS = 61
const ECONNREFUSED_MACOS: c_int = 61;
const EHOSTUNREACH: c_int = 113; // Linux ; macOS = 65
const EHOSTUNREACH_MACOS: c_int = 65;

// ───────────────────────────────────────────────────────────────────────
// § Wire-format SOCKADDR_IN — matches POSIX `<netinet/in.h>` exactly.
// ───────────────────────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
#[repr(C)]
struct sockaddr_in {
    sin_family: sa_family_t,
    sin_port: in_port_t,
    sin_addr: in_addr_t,
    sin_zero: [u8; 8],
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct sockaddr {
    sa_family: sa_family_t,
    sa_data: [u8; 14],
}

// ───────────────────────────────────────────────────────────────────────
// § extern bindings — the BSD-sockets surface.
// ───────────────────────────────────────────────────────────────────────

extern "C" {
    fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int;
    fn bind(sockfd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int;
    fn listen(sockfd: c_int, backlog: c_int) -> c_int;
    fn accept(sockfd: c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> c_int;
    fn connect(sockfd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int;
    fn send(sockfd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t;
    fn recv(sockfd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t;
    fn sendto(
        sockfd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        dest: *const sockaddr,
        addrlen: socklen_t,
    ) -> ssize_t;
    fn recvfrom(
        sockfd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        src: *mut sockaddr,
        addrlen: *mut socklen_t,
    ) -> ssize_t;
    fn close(fd: c_int) -> c_int;
    fn setsockopt(
        sockfd: c_int,
        level: c_int,
        optname: c_int,
        optval: *const c_void,
        optlen: socklen_t,
    ) -> c_int;
    fn getsockname(sockfd: c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> c_int;
    fn fcntl(fd: c_int, cmd: c_int, arg: c_int) -> c_int;
    fn htons(host: c_ushort) -> c_ushort;
    fn ntohs(net: c_ushort) -> c_ushort;
    fn htonl(host: c_ulong) -> c_ulong;
    fn ntohl(net: c_ulong) -> c_ulong;

    // errno accessor — implementation differs per-platform.
    #[cfg(any(target_os = "linux", target_env = "musl"))]
    fn __errno_location() -> *mut c_int;
    #[cfg(target_os = "macos")]
    fn __error() -> *mut c_int;
}

#[allow(unsafe_code)]
fn errno() -> c_int {
    #[cfg(any(target_os = "linux", target_env = "musl"))]
    {
        // SAFETY : __errno_location returns a valid thread-local pointer.
        unsafe { *__errno_location() }
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY : __error returns a valid thread-local pointer.
        unsafe { *__error() }
    }
    #[cfg(not(any(target_os = "linux", target_env = "musl", target_os = "macos")))]
    {
        0
    }
}

// ───────────────────────────────────────────────────────────────────────
// § errno → canonical net_error_code translation.
// ───────────────────────────────────────────────────────────────────────

fn translate_posix_errno(err: c_int) -> i32 {
    match err {
        EINTR => net_error_code::INTERRUPTED,
        EACCES => net_error_code::PERMISSION_DENIED,
        EFAULT | EINVAL | EBADF | ENOTSOCK => net_error_code::INVALID_INPUT,
        // EAGAIN ≡ EWOULDBLOCK on Linux ; macOS uses 35.
        EAGAIN_LINUX => net_error_code::WOULD_BLOCK,
        EAGAIN_MACOS => net_error_code::WOULD_BLOCK,
        EPIPE => net_error_code::BROKEN_PIPE,
        EADDRINUSE | EADDRINUSE_MACOS => net_error_code::ADDR_IN_USE,
        EADDRNOTAVAIL | EADDRNOTAVAIL_MACOS => net_error_code::ADDR_NOT_AVAILABLE,
        ENETDOWN => net_error_code::NETWORK_UNREACHABLE,
        ENETUNREACH | ENETUNREACH_MACOS => net_error_code::NETWORK_UNREACHABLE,
        ENETRESET | ECONNRESET | ECONNRESET_MACOS => net_error_code::CONNECTION_RESET,
        ECONNABORTED | ECONNABORTED_MACOS => net_error_code::CONNECTION_ABORTED,
        ENOTCONN | ENOTCONN_MACOS => net_error_code::NOT_CONNECTED,
        ETIMEDOUT | ETIMEDOUT_MACOS => net_error_code::TIMED_OUT,
        ECONNREFUSED | ECONNREFUSED_MACOS => net_error_code::CONNECTION_REFUSED,
        EHOSTUNREACH | EHOSTUNREACH_MACOS => net_error_code::HOST_UNREACHABLE,
        _ => {
            // EWOULDBLOCK_LINUX is the same as EAGAIN_LINUX (== 11) so it's
            // already covered above. Reference here to suppress "unused"
            // lints in builds where the constant is folded.
            let _ = EWOULDBLOCK_LINUX;
            net_error_code::OTHER
        }
    }
}

/// Translate cssl-rt portable sock-flags to BSD `(kind, protocol)`.
fn translate_sock_flags(flags: i32) -> (c_int, c_int) {
    if (flags & SOCK_TCP) != 0 {
        (SOCK_STREAM, IPPROTO_TCP)
    } else {
        (SOCK_DGRAM, IPPROTO_UDP)
    }
}

/// Build a `sockaddr_in` from host-byte-order `(addr, port)`.
fn build_sockaddr_in(addr_be: u32, port: u16) -> sockaddr_in {
    sockaddr_in {
        sin_family: AF_INET as sa_family_t,
        // SAFETY : htons / htonl are pure conversions.
        sin_port: unsafe { htons(port) },
        sin_addr: unsafe { htonl(addr_be) },
        sin_zero: [0u8; 8],
    }
}

/// Apply per-socket setsockopt / fcntl settings derived from portable
/// flags. Returns `Ok(())` on success, `Err(canonical-error)` on failure.
fn apply_sock_options(s: c_int, flags: i32) -> Result<(), i32> {
    if (flags & SOCK_REUSEADDR) != 0 {
        let one: c_int = 1;
        // SAFETY : optval is a c_int on the stack ; setsockopt accepts
        // the matching pointer + length.
        let r = unsafe {
            setsockopt(
                s,
                SOL_SOCKET,
                SO_REUSEADDR_OPT,
                core::ptr::addr_of!(one).cast::<c_void>(),
                core::mem::size_of::<c_int>() as socklen_t,
            )
        };
        if r != 0 {
            let err = errno();
            return Err(translate_posix_errno(err));
        }
    }
    if (flags & SOCK_NODELAY) != 0 && (flags & SOCK_TCP) != 0 {
        let one: c_int = 1;
        // SAFETY : same as above.
        let r = unsafe {
            setsockopt(
                s,
                IPPROTO_TCP,
                TCP_NODELAY_OPT,
                core::ptr::addr_of!(one).cast::<c_void>(),
                core::mem::size_of::<c_int>() as socklen_t,
            )
        };
        if r != 0 {
            let err = errno();
            return Err(translate_posix_errno(err));
        }
    }
    if (flags & SOCK_NONBLOCK) != 0 {
        // Read current flags, OR in O_NONBLOCK, write back.
        // SAFETY : fcntl is a well-known syscall.
        let cur = unsafe { fcntl(s, F_GETFL, 0) };
        if cur < 0 {
            let err = errno();
            return Err(translate_posix_errno(err));
        }
        let new = cur | O_NONBLOCK;
        let r = unsafe { fcntl(s, F_SETFL, new) };
        if r < 0 {
            let err = errno();
            return Err(translate_posix_errno(err));
        }
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_socket_impl — create a TCP or UDP socket.
// ───────────────────────────────────────────────────────────────────────

/// Create a new socket according to `flags`.
///
/// # Safety
/// Safe to call ; only state mutated is the per-thread last-error slot.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn cssl_net_socket_impl(flags: i32) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if let Err(kind) = validate_sock_flags(flags) {
        record_net_error(kind, 0);
        return INVALID_SOCKET;
    }
    let (kind_param, proto) = translate_sock_flags(flags);
    // SAFETY : socket accepts well-known c_int constants.
    let s = unsafe { socket(AF_INET, kind_param, proto) };
    if s < 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return INVALID_SOCKET;
    }
    if let Err(canonical) = apply_sock_options(s, flags) {
        // SAFETY : s is the just-created fd.
        let _ = unsafe { close(s) };
        record_net_error(canonical, 0);
        return INVALID_SOCKET;
    }
    record_socket();
    i64::from(s)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_listen_impl — bind + listen.
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
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
    if let Err(kind) = check_caps_for_addr(addr_be, port, true) {
        record_net_error(kind, 0);
        return -1;
    }
    let s = socket_handle as c_int;
    let sa = build_sockaddr_in(addr_be, port);
    // SAFETY : sockaddr_in is sockaddr-compatible on POSIX.
    let r = unsafe {
        bind(
            s,
            core::ptr::addr_of!(sa).cast::<sockaddr>(),
            core::mem::size_of::<sockaddr_in>() as socklen_t,
        )
    };
    if r != 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    // SAFETY : s is bound + valid.
    let r2 = unsafe { listen(s, backlog) };
    if r2 != 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    record_listen();
    0
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_accept_impl — accept the next pending connection.
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
pub unsafe fn cssl_net_accept_impl(socket_handle: i64) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return INVALID_SOCKET;
    }
    let s = socket_handle as c_int;
    let mut peer = sockaddr {
        sa_family: 0,
        sa_data: [0u8; 14],
    };
    let mut peer_len: socklen_t = core::mem::size_of::<sockaddr_in>() as socklen_t;
    // SAFETY : pointers to local stack buffers ; syscall fills them.
    let new_fd = unsafe { accept(s, &mut peer, &mut peer_len) };
    if new_fd < 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return INVALID_SOCKET;
    }
    record_accept();
    record_socket();
    i64::from(new_fd)
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_connect_impl — connect a TCP socket to a peer.
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
pub unsafe fn cssl_net_connect_impl(socket_handle: i64, addr_be: u32, port: u16) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if let Err(kind) = check_caps_for_addr(addr_be, port, false) {
        record_net_error(kind, 0);
        return -1;
    }
    let s = socket_handle as c_int;
    let sa = build_sockaddr_in(addr_be, port);
    // SAFETY : sockaddr_in is sockaddr-compatible.
    let r = unsafe {
        connect(
            s,
            core::ptr::addr_of!(sa).cast::<sockaddr>(),
            core::mem::size_of::<sockaddr_in>() as socklen_t,
        )
    };
    if r != 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    record_connect();
    0
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_send_impl + cssl_net_recv_impl.
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
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
    record_send(0);
    if buf_len == 0 {
        return 0;
    }
    let to_send = if buf_len > i32::MAX as usize {
        i32::MAX as size_t
    } else {
        buf_len as size_t
    };
    let s = socket_handle as c_int;
    // SAFETY : socket assumed valid by caller ; buffer pointer checked.
    let n = unsafe { send(s, buf_ptr.cast::<c_void>(), to_send, 0) };
    if n < 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    crate::net::BYTES_SENT_TOTAL.fetch_add(n as u64, Ordering::Relaxed);
    n as i64
}

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
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
        i32::MAX as size_t
    } else {
        buf_len as size_t
    };
    let s = socket_handle as c_int;
    // SAFETY : socket assumed valid ; buffer pointer checked.
    let n = unsafe { recv(s, buf_ptr.cast::<c_void>(), to_recv, 0) };
    if n < 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    crate::net::BYTES_RECV_TOTAL.fetch_add(n as u64, Ordering::Relaxed);
    n as i64
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_sendto_impl + cssl_net_recvfrom_impl.
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
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
        i32::MAX as size_t
    } else {
        buf_len as size_t
    };
    let s = socket_handle as c_int;
    let sa = build_sockaddr_in(addr_be, port);
    // SAFETY : standard sendto wiring.
    let n = unsafe {
        sendto(
            s,
            buf_ptr.cast::<c_void>(),
            to_send,
            0,
            core::ptr::addr_of!(sa).cast::<sockaddr>(),
            core::mem::size_of::<sockaddr_in>() as socklen_t,
        )
    };
    if n < 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    crate::net::BYTES_SENT_TOTAL.fetch_add(n as u64, Ordering::Relaxed);
    n as i64
}

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
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
        i32::MAX as size_t
    } else {
        buf_len as size_t
    };
    let s = socket_handle as c_int;
    let mut peer = sockaddr_in {
        sin_family: 0,
        sin_port: 0,
        sin_addr: 0,
        sin_zero: [0u8; 8],
    };
    let mut peer_len: socklen_t = core::mem::size_of::<sockaddr_in>() as socklen_t;
    // SAFETY : peer is local stack ; recvfrom fills it.
    let n = unsafe {
        recvfrom(
            s,
            buf_ptr.cast::<c_void>(),
            to_recv,
            0,
            core::ptr::addr_of_mut!(peer).cast::<sockaddr>(),
            &mut peer_len,
        )
    };
    if n < 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
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
    n as i64
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_close_impl.
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
pub unsafe fn cssl_net_close_impl(socket_handle: i64) -> i64 {
    crate::net::reset_last_net_error_for_tests();
    if socket_handle == INVALID_SOCKET {
        record_net_error(net_error_code::INVALID_INPUT, 0);
        return -1;
    }
    record_net_close();
    let s = socket_handle as c_int;
    // SAFETY : socket assumed valid by caller contract.
    let r = unsafe { close(s) };
    if r != 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    0
}

// ───────────────────────────────────────────────────────────────────────
// § cssl_net_local_addr_impl — read back the bound local address.
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
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
    let s = socket_handle as c_int;
    let mut local = sockaddr_in {
        sin_family: 0,
        sin_port: 0,
        sin_addr: 0,
        sin_zero: [0u8; 8],
    };
    let mut local_len: socklen_t = core::mem::size_of::<sockaddr_in>() as socklen_t;
    // SAFETY : pointer to local stack buffer.
    let r = unsafe {
        getsockname(
            s,
            core::ptr::addr_of_mut!(local).cast::<sockaddr>(),
            &mut local_len,
        )
    };
    if r != 0 {
        let err = errno();
        record_net_error(translate_posix_errno(err), err);
        return -1;
    }
    if !addr_be_out.is_null() {
        let host_addr = unsafe { ntohl(local.sin_addr) };
        unsafe { *addr_be_out = host_addr };
    }
    if !port_out.is_null() {
        let host_port = unsafe { ntohs(local.sin_port) };
        unsafe { *port_out = host_port };
    }
    0
}

// Suppress unused-c_uint lint when no specific cfg-arm consumes it.
#[allow(dead_code)]
const fn _force_c_uint_use() -> c_uint {
    0
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — flag-translation + errno mapping.
//
//   On Windows hosts this module is `cfg`-excluded entirely so these tests
//   don't compile (which is correct — the windows tests are in
//   `crate::net_win32` and exercise the same surface through Winsock2).
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::net::SOCK_UDP;

    #[test]
    fn translate_posix_errno_known_kinds() {
        assert_eq!(translate_posix_errno(EINTR), net_error_code::INTERRUPTED);
        assert_eq!(
            translate_posix_errno(EACCES),
            net_error_code::PERMISSION_DENIED
        );
        assert_eq!(
            translate_posix_errno(EAGAIN_LINUX),
            net_error_code::WOULD_BLOCK
        );
        assert_eq!(
            translate_posix_errno(EAGAIN_MACOS),
            net_error_code::WOULD_BLOCK
        );
        assert_eq!(
            translate_posix_errno(ECONNREFUSED),
            net_error_code::CONNECTION_REFUSED
        );
        assert_eq!(
            translate_posix_errno(ECONNREFUSED_MACOS),
            net_error_code::CONNECTION_REFUSED
        );
        assert_eq!(translate_posix_errno(ETIMEDOUT), net_error_code::TIMED_OUT);
        assert_eq!(
            translate_posix_errno(EADDRINUSE),
            net_error_code::ADDR_IN_USE
        );
        assert_eq!(
            translate_posix_errno(EADDRINUSE_MACOS),
            net_error_code::ADDR_IN_USE
        );
        // Unknown errno → OTHER.
        assert_eq!(translate_posix_errno(99_999), net_error_code::OTHER);
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
}
