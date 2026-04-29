//! § cssl-host-net — high-level Rust networking API for stage-0 CSSLv3.
//! ═══════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/04_EFFECTS.csl § NET-EFFECT` (S7-F4 spec
//!                      gap closed by DECISIONS T11-D82) +
//!                      `specs/11_IFC.csl § PRIME-DIRECTIVE ENCODING`
//!                      (NET-CAP rules) + `specs/14_BACKEND.csl §
//!                      HOST-NET` (the latter spec gap closed by
//!                      DECISIONS T11-D82).
//!
//! § ROLE
//!   Sits above `cssl-rt::net` (the platform syscall layer +
//!   cap-system + counters). Exposes idiomatic Rust types
//!   (`TcpListener` / `TcpStream` / `UdpSocket` / `SocketAddrV4`) for
//!   downstream consumers : tests, the CSSLv3 host runtime, and host-
//!   side examples. The CSSLv3 source-level surface lives in
//!   `stdlib/net.cssl` and routes through the same `__cssl_net_*` FFI
//!   symbols exposed by `cssl-rt`.
//!
//! § PRIME-DIRECTIVE attestation  (W! read-first)
//!   Every connect-style / bind-style call delegates through
//!   `cssl_rt::net::check_caps_for_addr` BEFORE the syscall fires. The
//!   default cap-set (loopback-only) is the safe-by-default posture
//!   from `PRIME_DIRECTIVE.md § 0 AXIOM` (consent = OS) and § 5
//!   CONSENT-ARCHITECTURE (informed + granular + revocable + ongoing).
//!   Hosts that need non-loopback access call
//!   `cssl_rt::caps_grant(NET_CAP_OUTBOUND | NET_CAP_INBOUND)` BEFORE
//!   instantiating the relevant type. Granting is explicit, never
//!   implicit, never coerced.
//!
//!   The crate exposes NO :
//!     - raw / packet sockets (DEFERRED ; surveillance / network-poisoning
//!       capability is not stage-0 surface — see DECISIONS T11-D82)
//!     - TLS or encryption (DEFERRED to a follow-up slice ; current
//!       APIs send PLAINTEXT bytes — non-loopback connections require
//!       explicit caller awareness via the cap-grant ceremony)
//!     - DNS resolution beyond `127.0.0.1` (host-name lookup is a
//!       follow-up slice — getaddrinfo wiring lands then)
//!     - IPv6 (DEFERRED ; v1 is IPv4-only per the slice handoff)
//!
//! § ABI INVARIANT
//!   All types here use `cssl_rt::net::*` directly ; the cssl-rt FFI
//!   symbols (`__cssl_net_*`) are the canonical source-level boundary.
//!   Renaming any symbol in cssl-rt breaks both this crate and the
//!   stdlib/net.cssl source code per the dispatch-plan landmines.

// § The high-level API delegates to `cssl_rt::net::cssl_net_*_impl`
//   which are `unsafe fn` because they touch raw pointers + sockets.
//   Each unsafe block carries an inline SAFETY paragraph identifying
//   the precondition (valid handle / valid byte slice / valid out-ptr).
#![allow(unsafe_code)]
// `unreachable_pub` is moot here — every type IS public on purpose.
#![allow(clippy::module_name_repetitions)]

use cssl_rt::net::{
    self, caps_current, caps_grant, caps_revoke, last_net_error_kind, last_net_error_os,
    net_error_code, ANY_V4, INVALID_SOCKET, LOOPBACK_V4, NET_CAP_INBOUND, NET_CAP_LOOPBACK,
    NET_CAP_OUTBOUND, SOCK_NODELAY, SOCK_NONBLOCK, SOCK_REUSEADDR, SOCK_TCP, SOCK_UDP,
};

// ───────────────────────────────────────────────────────────────────────
// § SocketAddrV4 — IPv4 socket address (host-byte-order).
// ───────────────────────────────────────────────────────────────────────

/// IPv4 socket address — `(addr, port)` packed into host-byte-order
/// integers. The `addr` field is `0x7F00_0001` for `127.0.0.1`,
/// `0x0A0B0C0D` for `10.11.12.13`, etc.
///
/// IPv6 is intentionally not addressable at this slice ; a follow-up
/// adds `SocketAddrV6`. The discriminant `addr` matches
/// `cssl_rt::net::LOOPBACK_V4` / `ANY_V4` for the well-known cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketAddrV4 {
    addr: u32,
    port: u16,
}

impl SocketAddrV4 {
    /// Construct from `(addr, port)` in host-byte-order.
    #[must_use]
    pub const fn new(addr: u32, port: u16) -> Self {
        Self { addr, port }
    }

    /// Convenience constructor for `127.0.0.1:port`. Always passes the
    /// cap-system check under the default cap-set.
    #[must_use]
    pub const fn loopback(port: u16) -> Self {
        Self {
            addr: LOOPBACK_V4,
            port,
        }
    }

    /// Convenience constructor for `0.0.0.0:port` (wildcard bind, all
    /// interfaces). Requires `NET_CAP_INBOUND` for `bind`.
    #[must_use]
    pub const fn any(port: u16) -> Self {
        Self { addr: ANY_V4, port }
    }

    /// Construct from dotted-quad octets `a.b.c.d`.
    #[must_use]
    pub const fn from_octets(a: u8, b: u8, c: u8, d: u8, port: u16) -> Self {
        let addr = ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | (d as u32);
        Self { addr, port }
    }

    /// IPv4 octets `a.b.c.d`.
    #[must_use]
    pub const fn octets(self) -> (u8, u8, u8, u8) {
        let a = ((self.addr >> 24) & 0xFF) as u8;
        let b = ((self.addr >> 16) & 0xFF) as u8;
        let c = ((self.addr >> 8) & 0xFF) as u8;
        let d = (self.addr & 0xFF) as u8;
        (a, b, c, d)
    }

    /// Host-byte-order packed `u32` representation.
    #[must_use]
    pub const fn addr_be(self) -> u32 {
        self.addr
    }

    /// Port number.
    #[must_use]
    pub const fn port(self) -> u16 {
        self.port
    }

    /// True if this address is in the `127.0.0.0/8` loopback range.
    #[must_use]
    pub const fn is_loopback(self) -> bool {
        net::addr_is_loopback(self.addr)
    }
}

impl core::fmt::Display for SocketAddrV4 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (oct_a, oct_b, oct_c, oct_d) = self.octets();
        write!(
            f,
            "{oct_a}.{oct_b}.{oct_c}.{oct_d}:{port}",
            port = self.port
        )
    }
}

// ───────────────────────────────────────────────────────────────────────
// § NetError — high-level error type wrapping cssl-rt's discriminants.
// ───────────────────────────────────────────────────────────────────────

/// High-level networking error. The `os_code` field carries the raw
/// `WSAGetLastError` / `errno` value for diagnostic logging.
///
/// Variants match `cssl_rt::net::net_error_code` discriminants 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetError {
    InvalidInput,
    AddrInUse,
    AddrNotAvailable,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    TimedOut,
    /// Non-blocking op would have blocked. Caller should retry / poll.
    WouldBlock,
    Interrupted,
    PermissionDenied,
    HostUnreachable,
    NetworkUnreachable,
    UnexpectedEof,
    BrokenPipe,
    /// Winsock not initialized ; on Win32 only.
    NotInitialized,
    /// PRIME-DIRECTIVE cap-system rejected the call. Caller must
    /// `caps_grant(NET_CAP_*)` for the relevant addr-class first.
    CapDenied,
    Other(i32),
}

impl NetError {
    /// Read the canonical kind-code from cssl-rt's per-thread last-error
    /// slot and translate into the typed `NetError` variant. The OS code
    /// is also captured for `Other(os)` carry-through.
    #[must_use]
    pub fn from_last() -> Self {
        let kind = last_net_error_kind();
        let os = last_net_error_os();
        match kind {
            net_error_code::INVALID_INPUT => Self::InvalidInput,
            net_error_code::ADDR_IN_USE => Self::AddrInUse,
            net_error_code::ADDR_NOT_AVAILABLE => Self::AddrNotAvailable,
            net_error_code::CONNECTION_REFUSED => Self::ConnectionRefused,
            net_error_code::CONNECTION_RESET => Self::ConnectionReset,
            net_error_code::CONNECTION_ABORTED => Self::ConnectionAborted,
            net_error_code::NOT_CONNECTED => Self::NotConnected,
            net_error_code::TIMED_OUT => Self::TimedOut,
            net_error_code::WOULD_BLOCK => Self::WouldBlock,
            net_error_code::INTERRUPTED => Self::Interrupted,
            net_error_code::PERMISSION_DENIED => Self::PermissionDenied,
            net_error_code::HOST_UNREACHABLE => Self::HostUnreachable,
            net_error_code::NETWORK_UNREACHABLE => Self::NetworkUnreachable,
            net_error_code::UNEXPECTED_EOF => Self::UnexpectedEof,
            net_error_code::BROKEN_PIPE => Self::BrokenPipe,
            net_error_code::NOT_INITIALIZED => Self::NotInitialized,
            net_error_code::CAP_DENIED => Self::CapDenied,
            _ => Self::Other(os),
        }
    }
}

impl core::fmt::Display for NetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidInput => write!(f, "invalid input"),
            Self::AddrInUse => write!(f, "address in use"),
            Self::AddrNotAvailable => write!(f, "address not available"),
            Self::ConnectionRefused => write!(f, "connection refused"),
            Self::ConnectionReset => write!(f, "connection reset by peer"),
            Self::ConnectionAborted => write!(f, "connection aborted"),
            Self::NotConnected => write!(f, "not connected"),
            Self::TimedOut => write!(f, "timed out"),
            Self::WouldBlock => write!(f, "operation would block"),
            Self::Interrupted => write!(f, "interrupted"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::HostUnreachable => write!(f, "host unreachable"),
            Self::NetworkUnreachable => write!(f, "network unreachable"),
            Self::UnexpectedEof => write!(f, "unexpected eof"),
            Self::BrokenPipe => write!(f, "broken pipe"),
            Self::NotInitialized => write!(f, "winsock not initialized"),
            Self::CapDenied => write!(f, "cap-system denied (PRIME-DIRECTIVE)"),
            Self::Other(code) => write!(f, "other (os code {code})"),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Cap-system convenience.
// ───────────────────────────────────────────────────────────────────────

/// PRIME-DIRECTIVE cap-system facade. Every method invokes a ref-counted
/// `cssl_rt::net::caps_*` call ; the cap-set is global to the thread
/// (per `cssl-rt::net` doc-block).
pub mod caps {
    use super::{caps_current, caps_grant, caps_revoke};
    use super::{NET_CAP_INBOUND, NET_CAP_LOOPBACK, NET_CAP_OUTBOUND};

    /// Grant `NET_CAP_OUTBOUND` (allow connect / send to non-loopback).
    pub fn grant_outbound() -> i32 {
        caps_grant(NET_CAP_OUTBOUND)
    }

    /// Grant `NET_CAP_INBOUND` (allow listen / accept on non-loopback).
    pub fn grant_inbound() -> i32 {
        caps_grant(NET_CAP_INBOUND)
    }

    /// Revoke `NET_CAP_OUTBOUND`. Always permitted (revocation is free
    /// per § 5 CONSENT-ARCH).
    pub fn revoke_outbound() -> i32 {
        caps_revoke(NET_CAP_OUTBOUND)
    }

    /// Revoke `NET_CAP_INBOUND`.
    pub fn revoke_inbound() -> i32 {
        caps_revoke(NET_CAP_INBOUND)
    }

    /// Read the current cap-set.
    #[must_use]
    pub fn current() -> i32 {
        caps_current()
    }

    /// True if `NET_CAP_LOOPBACK` is granted (default ON).
    #[must_use]
    pub fn loopback_granted() -> bool {
        (current() & NET_CAP_LOOPBACK) != 0
    }

    /// True if `NET_CAP_OUTBOUND` is granted (default OFF).
    #[must_use]
    pub fn outbound_granted() -> bool {
        (current() & NET_CAP_OUTBOUND) != 0
    }

    /// True if `NET_CAP_INBOUND` is granted (default OFF).
    #[must_use]
    pub fn inbound_granted() -> bool {
        (current() & NET_CAP_INBOUND) != 0
    }
}

// ───────────────────────────────────────────────────────────────────────
// § TcpListener — server-side TCP socket.
// ───────────────────────────────────────────────────────────────────────

/// Builder options for [`TcpListener`]. Wraps the cssl-rt `SOCK_*`
/// flag-bitset in a typed struct that is more idiomatic in Rust.
#[derive(Debug, Clone, Copy, Default)]
pub struct TcpListenerOptions {
    /// Set `SO_REUSEADDR` ; recommended for test-mode rapid-rebind. ON
    /// by default — matches `std::net::TcpListener::bind` behaviour.
    pub reuse_addr: bool,
    /// Set the listener to non-blocking mode at creation time.
    pub non_blocking: bool,
    /// Listen backlog. The default 128 matches both Linux's
    /// `SOMAXCONN` (typically 128) and Win32's behaviour.
    pub backlog: i32,
}

impl TcpListenerOptions {
    /// Default options : `reuse_addr = true`, `non_blocking = false`,
    /// `backlog = 128`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            reuse_addr: true,
            non_blocking: false,
            backlog: 128,
        }
    }
}

/// Server-side TCP socket bound to a local address. Produced by
/// [`TcpListener::bind`] ; consumes connections via [`TcpListener::accept`].
///
/// On `Drop`, the underlying socket is closed via
/// `cssl_rt::net::cssl_net_close_impl`. Per the cssl-rt cap-system,
/// non-loopback bind requires `NET_CAP_INBOUND` granted before the
/// `bind` call — see `caps::grant_inbound`.
#[derive(Debug)]
pub struct TcpListener {
    handle: i64,
}

impl TcpListener {
    /// Create a TCP listener bound to `addr` and put it in listen mode.
    ///
    /// Per the PRIME-DIRECTIVE cap-system, non-loopback addresses
    /// require `caps::grant_inbound()` BEFORE the call. Loopback works
    /// under the default cap-set.
    pub fn bind(addr: SocketAddrV4) -> Result<Self, NetError> {
        Self::bind_with(addr, TcpListenerOptions::new())
    }

    /// Like [`Self::bind`] but with explicit options.
    pub fn bind_with(addr: SocketAddrV4, opts: TcpListenerOptions) -> Result<Self, NetError> {
        let mut flags = SOCK_TCP;
        if opts.reuse_addr {
            flags |= SOCK_REUSEADDR;
        }
        if opts.non_blocking {
            flags |= SOCK_NONBLOCK;
        }
        // SAFETY of the FFI wrapper handled inside cssl-rt. Each call
        // below returns a sentinel on failure ; we translate via NetError.
        let s = unsafe { net::cssl_net_socket_impl(flags) };
        if s == INVALID_SOCKET {
            return Err(NetError::from_last());
        }
        // SAFETY : valid socket just opened.
        let r = unsafe { net::cssl_net_listen_impl(s, addr.addr_be(), addr.port(), opts.backlog) };
        if r != 0 {
            // ‼ Capture the error BEFORE the cleanup close — the close
            // path resets the last-error slot, clobbering the listen
            // failure code (e.g. CAP_DENIED). Order matters here.
            let err = NetError::from_last();
            // SAFETY : socket needs explicit close on the error path.
            let _ = unsafe { net::cssl_net_close_impl(s) };
            return Err(err);
        }
        Ok(Self { handle: s })
    }

    /// Accept the next pending connection. On non-blocking listeners,
    /// returns `Err(NetError::WouldBlock)` if no connection is pending.
    pub fn accept(&self) -> Result<TcpStream, NetError> {
        // SAFETY : self.handle is a valid bound listening socket.
        let s = unsafe { net::cssl_net_accept_impl(self.handle) };
        if s == INVALID_SOCKET {
            return Err(NetError::from_last());
        }
        Ok(TcpStream { handle: s })
    }

    /// Read back the local `(addr, port)` of the listener. Useful when
    /// `bind` was called with port 0 (let-OS-pick) — the returned port
    /// is the OS-assigned ephemeral port.
    pub fn local_addr(&self) -> Result<SocketAddrV4, NetError> {
        let mut addr: u32 = 0;
        let mut port: u16 = 0;
        // SAFETY : pointers + valid handle.
        #[cfg(target_os = "windows")]
        let r = unsafe {
            cssl_rt::net_win32::cssl_net_local_addr_impl(self.handle, &mut addr, &mut port)
        };
        #[cfg(not(target_os = "windows"))]
        let r = unsafe {
            cssl_rt::net_unix::cssl_net_local_addr_impl(self.handle, &mut addr, &mut port)
        };
        if r != 0 {
            return Err(NetError::from_last());
        }
        Ok(SocketAddrV4 { addr, port })
    }

    /// Underlying OS handle as `i64`. Stage-0 escape hatch for callers
    /// that want to drive the FFI directly.
    #[must_use]
    pub const fn as_raw_handle(&self) -> i64 {
        self.handle
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        if self.handle != INVALID_SOCKET {
            // SAFETY : valid handle ; double-close is rejected by the
            // platform layer.
            let _ = unsafe { net::cssl_net_close_impl(self.handle) };
            self.handle = INVALID_SOCKET;
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § TcpStream — client / accepted-connection socket.
// ───────────────────────────────────────────────────────────────────────

/// Builder options for [`TcpStream`].
#[derive(Debug, Clone, Copy, Default)]
pub struct TcpStreamOptions {
    /// Disable Nagle's algorithm (set `TCP_NODELAY`). Off by default —
    /// matches POSIX precedent ; Nagle is enabled for throughput-friendly
    /// small-write batching.
    pub no_delay: bool,
    /// Set the stream to non-blocking mode at creation time.
    pub non_blocking: bool,
}

/// Connected TCP socket. Produced by [`TcpStream::connect`] (client side)
/// or [`TcpListener::accept`] (server side). Send / recv operations on
/// non-blocking streams may return `NetError::WouldBlock`.
#[derive(Debug)]
pub struct TcpStream {
    handle: i64,
}

impl TcpStream {
    /// Connect to a remote TCP peer at `addr`. Per the PRIME-DIRECTIVE
    /// cap-system, non-loopback addresses require `caps::grant_outbound()`
    /// BEFORE the call.
    pub fn connect(addr: SocketAddrV4) -> Result<Self, NetError> {
        Self::connect_with(addr, TcpStreamOptions::default())
    }

    /// Like [`Self::connect`] but with explicit options.
    pub fn connect_with(addr: SocketAddrV4, opts: TcpStreamOptions) -> Result<Self, NetError> {
        let mut flags = SOCK_TCP;
        if opts.no_delay {
            flags |= SOCK_NODELAY;
        }
        if opts.non_blocking {
            flags |= SOCK_NONBLOCK;
        }
        // SAFETY of the FFI wrapper handled inside cssl-rt.
        let s = unsafe { net::cssl_net_socket_impl(flags) };
        if s == INVALID_SOCKET {
            return Err(NetError::from_last());
        }
        // SAFETY : valid socket just opened.
        let r = unsafe { net::cssl_net_connect_impl(s, addr.addr_be(), addr.port()) };
        if r != 0 {
            // ‼ Capture before cleanup ; close resets the last-error slot.
            let err = NetError::from_last();
            let _ = unsafe { net::cssl_net_close_impl(s) };
            return Err(err);
        }
        Ok(Self { handle: s })
    }

    /// Send `bytes`. Returns the number of bytes actually sent ; short
    /// sends are possible per-syscall. Use [`Self::send_all`] to loop
    /// until the full buffer has been sent.
    pub fn send(&self, bytes: &[u8]) -> Result<usize, NetError> {
        // SAFETY : bytes is a valid byte slice.
        let n = unsafe { net::cssl_net_send_impl(self.handle, bytes.as_ptr(), bytes.len()) };
        if n < 0 {
            return Err(NetError::from_last());
        }
        #[allow(clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    /// Send the entire `bytes` buffer, looping over short sends. Returns
    /// `Ok(())` on success or the first error encountered.
    pub fn send_all(&self, mut bytes: &[u8]) -> Result<(), NetError> {
        while !bytes.is_empty() {
            let n = self.send(bytes)?;
            if n == 0 {
                return Err(NetError::BrokenPipe);
            }
            bytes = &bytes[n..];
        }
        Ok(())
    }

    /// Receive up to `buf.len()` bytes into `buf`. Returns the number
    /// of bytes actually received (0 = peer closed cleanly).
    pub fn recv(&self, buf: &mut [u8]) -> Result<usize, NetError> {
        // SAFETY : buf is a valid mutable byte slice.
        let n = unsafe { net::cssl_net_recv_impl(self.handle, buf.as_mut_ptr(), buf.len()) };
        if n < 0 {
            return Err(NetError::from_last());
        }
        #[allow(clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    /// Underlying OS handle as `i64`.
    #[must_use]
    pub const fn as_raw_handle(&self) -> i64 {
        self.handle
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        if self.handle != INVALID_SOCKET {
            // SAFETY : valid handle.
            let _ = unsafe { net::cssl_net_close_impl(self.handle) };
            self.handle = INVALID_SOCKET;
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § UdpSocket — connectionless UDP datagram socket.
// ───────────────────────────────────────────────────────────────────────

/// Builder options for [`UdpSocket`].
#[derive(Debug, Clone, Copy, Default)]
pub struct UdpSocketOptions {
    /// Set `SO_REUSEADDR`.
    pub reuse_addr: bool,
    /// Set the socket to non-blocking mode at creation time.
    pub non_blocking: bool,
}

/// Connectionless UDP datagram socket. Bind to a local address with
/// [`UdpSocket::bind`] ; send to peers via [`UdpSocket::send_to`] ;
/// receive datagrams + peer-address via [`UdpSocket::recv_from`].
///
/// Stage-0 does not expose the connection-oriented `connect()` / `send()`
/// / `recv()` shortcuts on UDP — those are a deferred follow-up.
#[derive(Debug)]
pub struct UdpSocket {
    handle: i64,
}

impl UdpSocket {
    /// Create a UDP socket bound to `addr`.
    ///
    /// Per the PRIME-DIRECTIVE cap-system, non-loopback bind requires
    /// `caps::grant_inbound()` BEFORE the call.
    pub fn bind(addr: SocketAddrV4) -> Result<Self, NetError> {
        Self::bind_with(addr, UdpSocketOptions::default())
    }

    /// Like [`Self::bind`] but with explicit options.
    pub fn bind_with(addr: SocketAddrV4, opts: UdpSocketOptions) -> Result<Self, NetError> {
        let mut flags = SOCK_UDP;
        if opts.reuse_addr {
            flags |= SOCK_REUSEADDR;
        }
        if opts.non_blocking {
            flags |= SOCK_NONBLOCK;
        }
        // SAFETY of the FFI wrapper handled inside cssl-rt.
        let s = unsafe { net::cssl_net_socket_impl(flags) };
        if s == INVALID_SOCKET {
            return Err(NetError::from_last());
        }
        // For UDP, bind happens via the listen-impl (bind portion).
        // The listen leg fails for UDP (sock is connectionless) but the
        // cap-check + bind already happened. We reuse the impl ; an
        // alternative is to add a dedicated `cssl_net_bind_impl` but
        // stage-0 keeps the surface minimal.
        // SAFETY : valid UDP socket.
        let r = unsafe { net::cssl_net_listen_impl(s, addr.addr_be(), addr.port(), 0) };
        // If cap-check rejected before any syscall fired, bind did NOT
        // happen — surface the cap-denial straight away (the close path
        // would otherwise clobber the last-error slot). Capture before
        // cleanup.
        if r != 0 && last_net_error_kind() == net_error_code::CAP_DENIED {
            let err = NetError::from_last();
            // SAFETY : valid socket on the error path.
            let _ = unsafe { net::cssl_net_close_impl(s) };
            return Err(err);
        }
        // For UDP, listen returns -1 with WSAEOPNOTSUPP / EOPNOTSUPP,
        // BUT bind succeeded — confirm via getsockname.
        let mut bound_addr: u32 = 0;
        let mut bound_port: u16 = 0;
        // SAFETY : pointers + valid handle.
        #[cfg(target_os = "windows")]
        let g = unsafe {
            cssl_rt::net_win32::cssl_net_local_addr_impl(s, &mut bound_addr, &mut bound_port)
        };
        #[cfg(not(target_os = "windows"))]
        let g = unsafe {
            cssl_rt::net_unix::cssl_net_local_addr_impl(s, &mut bound_addr, &mut bound_port)
        };
        if g != 0 {
            // bind didn't actually succeed.
            let err = NetError::from_last();
            let _ = unsafe { net::cssl_net_close_impl(s) };
            return Err(err);
        }
        // listen-leg failure on UDP is non-fatal ; bind portion succeeded.
        let _ = r;
        Ok(Self { handle: s })
    }

    /// Send `bytes` as a single datagram to `addr`. Returns the number
    /// of bytes actually sent.
    ///
    /// Per the PRIME-DIRECTIVE cap-system, non-loopback addresses
    /// require `caps::grant_outbound()` BEFORE the call.
    pub fn send_to(&self, bytes: &[u8], addr: SocketAddrV4) -> Result<usize, NetError> {
        // SAFETY : bytes is a valid byte slice.
        let n = unsafe {
            net::cssl_net_sendto_impl(
                self.handle,
                bytes.as_ptr(),
                bytes.len(),
                addr.addr_be(),
                addr.port(),
            )
        };
        if n < 0 {
            return Err(NetError::from_last());
        }
        #[allow(clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    /// Receive a datagram into `buf` ; returns `(bytes_received,
    /// peer_addr)`.
    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddrV4), NetError> {
        let mut peer_addr: u32 = 0;
        let mut peer_port: u16 = 0;
        // SAFETY : buf is a valid mutable byte slice ; peer_* are valid stack ptrs.
        let n = unsafe {
            net::cssl_net_recvfrom_impl(
                self.handle,
                buf.as_mut_ptr(),
                buf.len(),
                &mut peer_addr,
                &mut peer_port,
            )
        };
        if n < 0 {
            return Err(NetError::from_last());
        }
        #[allow(clippy::cast_sign_loss)]
        Ok((
            n as usize,
            SocketAddrV4 {
                addr: peer_addr,
                port: peer_port,
            },
        ))
    }

    /// Read back the local `(addr, port)` of the bound socket.
    pub fn local_addr(&self) -> Result<SocketAddrV4, NetError> {
        let mut addr: u32 = 0;
        let mut port: u16 = 0;
        // SAFETY : pointers + valid handle.
        #[cfg(target_os = "windows")]
        let r = unsafe {
            cssl_rt::net_win32::cssl_net_local_addr_impl(self.handle, &mut addr, &mut port)
        };
        #[cfg(not(target_os = "windows"))]
        let r = unsafe {
            cssl_rt::net_unix::cssl_net_local_addr_impl(self.handle, &mut addr, &mut port)
        };
        if r != 0 {
            return Err(NetError::from_last());
        }
        Ok(SocketAddrV4 { addr, port })
    }

    /// Underlying OS handle as `i64`.
    #[must_use]
    pub const fn as_raw_handle(&self) -> i64 {
        self.handle
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        if self.handle != INVALID_SOCKET {
            // SAFETY : valid handle.
            let _ = unsafe { net::cssl_net_close_impl(self.handle) };
            self.handle = INVALID_SOCKET;
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § crate metadata.
// ───────────────────────────────────────────────────────────────────────

/// Crate version string (from `Cargo.toml`).
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("hurt nor harm"));
    }

    #[test]
    fn socket_addr_v4_construction_via_octets() {
        let a = SocketAddrV4::from_octets(127, 0, 0, 1, 8080);
        assert_eq!(a.addr_be(), 0x7F00_0001);
        assert_eq!(a.port(), 8080);
        assert_eq!(a.octets(), (127, 0, 0, 1));
        assert!(a.is_loopback());
    }

    #[test]
    fn socket_addr_v4_loopback_helper() {
        let a = SocketAddrV4::loopback(0);
        assert_eq!(a.addr_be(), LOOPBACK_V4);
        assert!(a.is_loopback());
    }

    #[test]
    fn socket_addr_v4_any_helper() {
        let a = SocketAddrV4::any(0);
        assert_eq!(a.addr_be(), ANY_V4);
        assert!(!a.is_loopback());
    }

    #[test]
    fn socket_addr_v4_display_format() {
        let a = SocketAddrV4::from_octets(192, 168, 1, 1, 80);
        assert_eq!(format!("{a}"), "192.168.1.1:80");
    }

    #[test]
    fn caps_default_loopback_only() {
        // Reset to default before checking the natural posture.
        // We avoid touching the global lock here ; just verify the
        // default cap-set contains LOOPBACK and NOT OUTBOUND/INBOUND.
        cssl_rt::net::reset_net_for_tests();
        assert!(caps::loopback_granted());
        // grant + revoke cycle.
        caps::grant_outbound();
        assert!(caps::outbound_granted());
        caps::revoke_outbound();
        assert!(!caps::outbound_granted());
        caps::grant_inbound();
        assert!(caps::inbound_granted());
        caps::revoke_inbound();
        assert!(!caps::inbound_granted());
    }

    #[test]
    fn tcp_listener_bind_loopback_default_caps() {
        cssl_rt::net::reset_net_for_tests();
        let addr = SocketAddrV4::loopback(0);
        let listener = TcpListener::bind(addr).expect("loopback bind should succeed");
        let local = listener.local_addr().expect("local_addr");
        assert!(local.is_loopback());
        assert_ne!(local.port(), 0);
    }

    #[test]
    fn tcp_listener_bind_any_without_inbound_cap_denied() {
        cssl_rt::net::reset_net_for_tests();
        // Default caps : LOOPBACK only. Bind to 0.0.0.0 → CapDenied.
        let addr = SocketAddrV4::any(0);
        let r = TcpListener::bind(addr);
        match r {
            Err(NetError::CapDenied) => (),
            other => panic!("expected CapDenied, got {other:?}"),
        }
    }

    #[test]
    fn tcp_stream_connect_to_non_loopback_without_outbound_cap_denied() {
        cssl_rt::net::reset_net_for_tests();
        // 8.8.8.8:53 — non-loopback, default caps → CapDenied.
        let addr = SocketAddrV4::from_octets(8, 8, 8, 8, 53);
        let r = TcpStream::connect(addr);
        match r {
            Err(NetError::CapDenied) => (),
            other => panic!("expected CapDenied, got {other:?}"),
        }
    }

    #[test]
    fn tcp_loopback_full_roundtrip() {
        // Full high-level API roundtrip on Apocky's host.
        cssl_rt::net::reset_net_for_tests();
        let listener = TcpListener::bind(SocketAddrV4::loopback(0)).expect("bind");
        let bound = listener.local_addr().expect("local_addr");
        // Spawn a client thread that connects.
        let port = bound.port();
        let client = TcpStream::connect(SocketAddrV4::loopback(port)).expect("connect");
        let conn = listener.accept().expect("accept");
        let payload = b"cssl-host-net f4 hello";
        client.send_all(payload).expect("send_all");
        let mut buf = vec![0u8; payload.len() + 8];
        let n = conn.recv(&mut buf).expect("recv");
        assert_eq!(n, payload.len());
        assert_eq!(&buf[..n], payload);
        // Drop closes all sockets automatically.
    }

    #[test]
    fn net_error_from_last_translates_kind() {
        cssl_rt::net::reset_net_for_tests();
        cssl_rt::net::record_net_error(net_error_code::CONNECTION_REFUSED, 10061);
        let e = NetError::from_last();
        assert_eq!(e, NetError::ConnectionRefused);
    }

    #[test]
    fn net_error_from_last_returns_other_with_os_code() {
        cssl_rt::net::reset_net_for_tests();
        cssl_rt::net::record_net_error(net_error_code::OTHER, 12345);
        let e = NetError::from_last();
        assert_eq!(e, NetError::Other(12345));
    }
}
