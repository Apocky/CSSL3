//! § cssl-rt networking I/O — cross-platform interface (T11-D82, S7-F4).
//!
//! § ROLE
//!   The platform-neutral networking surface that CSSLv3-emitted code calls
//!   into via the `__cssl_net_*` FFI symbols. Per-OS implementations live
//!   in `crate::net_win32` (Windows : Winsock2 `WSAStartup` / `socket` /
//!   `bind` / `listen` / `accept` / `connect` / `send` / `recv` /
//!   `closesocket`) and `crate::net_unix` (Linux + macOS : BSD-sockets
//!   `socket` / `bind` / `listen` / `accept` / `connect` / `send` /
//!   `recv` / `close` libc-style syscalls). The active platform module is
//!   selected via cfg ; only one is compiled per build.
//!
//! § DESIGN  (mirrors `crate::io` 3-layer pattern from S6-B5 / T11-D76)
//!   1. Platform layer — `net_win32` / `net_unix` : the actual syscall
//!      calls + per-OS error-translation. Selected at compile-time via
//!      `cfg`.
//!   2. This module    — re-exports the active platform's `*_impl` fns
//!      under a stable cross-platform name, plus the `net_error_code`
//!      module + `SOCK_*` flag-bitset + `NET_CAP_*` capability-bitset
//!      that source-level CSSLv3 sees.
//!   3. FFI layer      — `__cssl_net_*` symbols in [`crate::ffi`] delegate
//!      to this module.
//!
//! § SOCKET HANDLES
//!   At cssl-rt level a socket handle is `i64` :
//!     - Windows : the underlying `SOCKET` value cast to `i64` (or
//!       `INVALID_SOCKET = -1` on error before the syscall returns).
//!     - Unix    : the underlying `fd : c_int` zero-extended to `i64`
//!       (or `-1` on error before the syscall returns).
//!   The CSSLv3-source-level `TcpListener` / `TcpStream` / `UdpSocket`
//!   types wrap this `i64` ; consumers must call `__cssl_net_close`
//!   exactly once to release the OS resource.
//!
//! § ADDRESSES  (stage-0)
//!   IPv4 only at this slice ; IPv6 documented + deferred per the slice
//!   handoff ‼-landmine. Addresses are passed across the FFI as a packed
//!   pair :
//!     - `addr` : `u32` host-byte-order octets
//!         (`a.b.c.d` ⇒ `(a << 24) | (b << 16) | (c << 8) | d`).
//!         Internal translation flips to network-byte-order before the
//!         platform syscall.
//!     - `port` : `u16` host-byte-order port number.
//!   `127.0.0.1` ≡ `0x7F00_0001` ; the LOOPBACK constant matches.
//!
//! § BLOCKING vs NON-BLOCKING  (stage-0 default = blocking)
//!   The portable `SOCK_NONBLOCK` flag opt-ins to non-blocking I/O at
//!   socket-creation time. On non-blocking sockets, `send` / `recv` /
//!   `accept` / `connect` may return [`net_error_code::WOULD_BLOCK`]
//!   instead of completing ; callers loop / `poll` / select on the
//!   handle. A future slice adds a `__cssl_net_poll` syscall wrapper
//!   for unified poll/select dispatch.
//!
//! § PRIME-DIRECTIVE attestation  (W! read-first)
//!   Networking is the most surveillance-adjacent surface in the
//!   runtime — more so than file I/O, because it crosses the host
//!   boundary into the network. This module enforces structural
//!   safeguards aligned with `PRIME_DIRECTIVE.md`. The five layers are :
//!
//!   ```text
//!   1. CAPABILITY-GATED : every net op consults a per-thread
//!      capability bitset (NET_CAP_*). The default is
//!      NET_CAP_LOOPBACK ONLY — 127.0.0.0/8 outbound + loopback
//!      inbound. Non-loopback addresses require an explicit
//!      caps_grant(NET_CAP_OUTBOUND) / caps_grant(NET_CAP_INBOUND)
//!      from the host. This is the structural realization of
//!      specs/11_IFC.csl  § PRIME-DIRECTIVE ENCODING — surveillance-
//!      shaped network composition is rejected at the cap-system
//!      layer rather than relying on a downstream policy check.
//!
//!   2. NO COVERT CHANNELS : raw sockets / packet sockets are
//!      DEFERRED with explicit rationale (see net_win32 and net_unix
//!      doc-blocks). Stage-0 exposes ONLY connection-oriented TCP +
//!      datagram UDP. AF_PACKET / SOCK_RAW / IP_HDRINCL etc. are not
//!      addressable from CSSLv3 source-level code at this stage.
//!
//!   3. PLAINTEXT BY DEFAULT : every byte sent / received is plaintext
//!      on the wire. TLS / encryption is DEFERRED to a follow-up
//!      slice. The cap-system flags NET_CAP_OUTBOUND / NET_CAP_INBOUND
//!      only authorize the syscall ; they DO NOT authorize sending
//!      sensitive data. Source-level code that wants to send Sensitive
//!      labeled bytes must first satisfy the IFC-label compose rule
//!      from specs/11_IFC.csl (which rejects Sensitive surveillance
//!      composed with IO + Net outright at compile time).
//!
//!   4. AUDIT VISIBILITY : every successful syscall increments a
//!      public, atomic counter that the host can inspect. Counters are
//!      LOCAL — they do not export to the network. The pattern matches
//!      crate io's tracker discipline.
//!
//!   5. INFORMED CONSENT : the Net effect-row marker on every
//!      cssl.net.* MIR op makes networking-touching code visible in fn
//!      signatures (per specs/04_EFFECTS.csl  § NET-EFFECT, which
//!      mirrors the IO-EFFECT shape from S6-B5). A caller cannot
//!      accidentally invoke networking from a context that forbids it ;
//!      the type system rejects the composition.
//!   ```
//!
//!   What this module does NOT do : surveillance, exfiltration, hidden
//!   side-channels, raw-packet weaponization. The surface is
//!   intentionally minimal + auditable. No backdoors. No privileged
//!   override (per `PRIME_DIRECTIVE.md` § 6 SCOPE — flag / config /
//!   env-var / cli-arg / api-call / runtime-cond cannot disable these
//!   protections).

#![allow(unsafe_code)]

use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § Public type re-exports — the source-level surface.
// ───────────────────────────────────────────────────────────────────────

/// Sentinel handle returned by socket-producing ops on failure. Mirrors
/// POSIX `-1` and Win32 `INVALID_SOCKET = (SOCKET)(~0)` after sign-extension
/// to `i64`. CSSLv3 source-level code recognizes this value as the error
/// sentinel ; the stdlib `net.cssl::*` wrappers convert it into
/// `Result::Err(NetError::*)` per the per-thread last-error slot.
pub const INVALID_SOCKET: i64 = -1;

// ───────────────────────────────────────────────────────────────────────
// § SocketFlags — portable bitset.
// ───────────────────────────────────────────────────────────────────────

/// Open a TCP / stream socket (`SOCK_STREAM`). Mutually exclusive with
/// `SOCK_UDP`.
pub const SOCK_TCP: i32 = 0x0001;
/// Open a UDP / datagram socket (`SOCK_DGRAM`). Mutually exclusive with
/// `SOCK_TCP`.
pub const SOCK_UDP: i32 = 0x0002;
/// Set the socket to non-blocking mode at creation. Equivalent to
/// `O_NONBLOCK` on POSIX or `FIONBIO` on Win32. When set, send / recv /
/// accept / connect may return [`net_error_code::WOULD_BLOCK`] instead
/// of completing.
pub const SOCK_NONBLOCK: i32 = 0x0004;
/// Enable `SO_REUSEADDR` on the socket. Recommended for server sockets
/// in test mode to allow rapid rebind after `TIME_WAIT`. Off by default
/// matches POSIX precedent.
pub const SOCK_REUSEADDR: i32 = 0x0008;
/// Enable `TCP_NODELAY` (disable Nagle's algorithm) on TCP sockets.
/// Off by default matches POSIX precedent — Nagle is enabled for
/// throughput-friendly small-write batching.
pub const SOCK_NODELAY: i32 = 0x0010;

/// Mask of recognized socket-flag bits. Any other bit is rejected.
pub const SOCK_FLAG_MASK: i32 = SOCK_TCP | SOCK_UDP | SOCK_NONBLOCK | SOCK_REUSEADDR | SOCK_NODELAY;

// ───────────────────────────────────────────────────────────────────────
// § PRIME-DIRECTIVE capability bitset.
//
//   Default-deny : every net op consults a per-thread capability bitset.
//   The starting state is `NET_CAP_LOOPBACK` only (loopback addresses ok,
//   everything else rejected with `CAP_DENIED`). Source-level CSSLv3
//   code raises additional caps via `caps_grant(...)` per
//   `specs/11_IFC.csl`. Granting requires explicit consent — there is no
//   blanket-allow flag, no environment variable override, no admin
//   privilege escalation per `PRIME_DIRECTIVE.md § 6 SCOPE`.
// ───────────────────────────────────────────────────────────────────────

/// Allow connect / send / recv to / from `127.0.0.0/8` (loopback addresses).
/// Inbound listen on loopback also requires this bit. This is the default
/// (set on every cssl-rt thread) so unit tests can exercise the surface
/// without a host-side cap-grant ceremony.
pub const NET_CAP_LOOPBACK: i32 = 0x0001;
/// Allow connect / send / recv to / from non-loopback addresses
/// (anything outside `127.0.0.0/8`). Required for outbound connections
/// to remote hosts. Default OFF ; host code raises this explicitly.
pub const NET_CAP_OUTBOUND: i32 = 0x0002;
/// Allow listen / accept on non-loopback addresses (binding `0.0.0.0` or
/// a specific external interface). Required for server sockets that
/// accept remote connections. Default OFF ; host code raises this
/// explicitly.
pub const NET_CAP_INBOUND: i32 = 0x0004;

/// Mask of recognized cap bits.
pub const NET_CAP_MASK: i32 = NET_CAP_LOOPBACK | NET_CAP_OUTBOUND | NET_CAP_INBOUND;

/// Default cap-set on every thread : loopback only. Matches the
/// PRIME-DIRECTIVE default-deny posture for non-loopback networking.
pub const NET_CAP_DEFAULT: i32 = NET_CAP_LOOPBACK;

// ───────────────────────────────────────────────────────────────────────
// § NetError — canonical error sum-type.
// ───────────────────────────────────────────────────────────────────────

/// Canonical net error variants — discriminants are STABLE from S7-F4.
///
/// The `i32` discriminant is what cssl-rt threads through the per-thread
/// last-error slot ; source-level CSSLv3 maps it onto the `NetError`
/// sum-type constructors via the `net_error_from_kind` fn in
/// `stdlib/net.cssl`. Renaming a discriminant requires a major-version
/// bump (mirrors the FFI-symbol invariant from T11-D52 / T11-D76).
///
/// § VARIANTS
///   - `0`  Success — sentinel for "no error", never observed by consumers
///     of `NetError` (only present in the per-thread slot reset on each op).
///   - `1`  `InvalidInput` — malformed flags / pointer / length / address.
///   - `2`  `AddrInUse` — bind to a port another socket already holds.
///   - `3`  `AddrNotAvailable` — bind to an address not on this host.
///   - `4`  `ConnectionRefused` — peer actively rejected the connect.
///   - `5`  `ConnectionReset` — peer reset an active connection.
///   - `6`  `ConnectionAborted` — local side aborted the connection.
///   - `7`  `NotConnected` — op on a not-yet-connected socket.
///   - `8`  `TimedOut` — op exceeded the configured deadline.
///   - `9`  `WouldBlock` — non-blocking op would have blocked.
///   - `10` `Interrupted` — syscall returned `EINTR` ; caller may retry.
///   - `11` `PermissionDenied` — caller lacks rights or `EACCES`.
///   - `12` `HostUnreachable` — no route to host (`EHOSTUNREACH`).
///   - `13` `NetworkUnreachable` — no route to network (`ENETUNREACH`).
///   - `14` `UnexpectedEof` — peer closed mid-recv unexpectedly.
///   - `15` `BrokenPipe` — write to closed peer (`EPIPE`).
///   - `16` `NotInitialized` — Winsock not yet started.
///   - `17` `CapDenied` — PRIME-DIRECTIVE cap-system rejected the call.
///   - `99` `Other` — catch-all carrying the raw OS errno / `WSAGetLastError`
///     value in the high 32 bits of the i64 slot. Stable from S7-F4.
pub mod net_error_code {
    /// No error — never observed by `NetError` consumers.
    pub const SUCCESS: i32 = 0;
    /// Caller-supplied flags / pointer / length / address is malformed.
    pub const INVALID_INPUT: i32 = 1;
    /// Bind failed because the address is already bound by another socket.
    pub const ADDR_IN_USE: i32 = 2;
    /// Bind failed because the address is not assigned to this host.
    pub const ADDR_NOT_AVAILABLE: i32 = 3;
    /// Connect failed because the peer actively rejected the SYN.
    pub const CONNECTION_REFUSED: i32 = 4;
    /// An established connection was reset by the peer.
    pub const CONNECTION_RESET: i32 = 5;
    /// Local side aborted the connection.
    pub const CONNECTION_ABORTED: i32 = 6;
    /// Operation invoked on a not-yet-connected socket.
    pub const NOT_CONNECTED: i32 = 7;
    /// Operation exceeded the configured deadline.
    pub const TIMED_OUT: i32 = 8;
    /// Non-blocking operation would have blocked ; caller should retry.
    pub const WOULD_BLOCK: i32 = 9;
    /// Syscall interrupted by signal ; caller may retry.
    pub const INTERRUPTED: i32 = 10;
    /// Caller lacks rights to perform the operation, or `EACCES`.
    pub const PERMISSION_DENIED: i32 = 11;
    /// No route to host (`EHOSTUNREACH`).
    pub const HOST_UNREACHABLE: i32 = 12;
    /// No route to network (`ENETUNREACH`).
    pub const NETWORK_UNREACHABLE: i32 = 13;
    /// Peer closed mid-recv unexpectedly.
    pub const UNEXPECTED_EOF: i32 = 14;
    /// Write to closed peer (`EPIPE`).
    pub const BROKEN_PIPE: i32 = 15;
    /// Winsock not yet started ; on Win32 only.
    pub const NOT_INITIALIZED: i32 = 16;
    /// PRIME-DIRECTIVE cap-system rejected the call. Caller must
    /// `caps_grant(NET_CAP_*)` for the relevant addr-class first.
    pub const CAP_DENIED: i32 = 17;
    /// Catch-all : the high 32 bits of the i64 slot carry the raw OS code.
    pub const OTHER: i32 = 99;
}

// ───────────────────────────────────────────────────────────────────────
// § Per-thread last-error slot.
//
//   Stage-0 implementation : a single global atomic. Sufficient for
//   hosted stage-0 testing. A per-thread TLS slot is a follow-up once
//   the runtime grows TLS infrastructure (matches B5 file-IO precedent).
// ───────────────────────────────────────────────────────────────────────

static LAST_NET_ERROR_CODE: AtomicU64 = AtomicU64::new(0);

/// Write the canonical error code for the last net op.
///
/// `os_code` is the raw OS errno / `WSAGetLastError` value (or 0 for the
/// non-`OTHER` cases). The two are packed into a single u64 :
/// `(os_code as u64) << 32 | (kind_code as u32 as u64)`.
pub fn record_net_error(kind_code: i32, os_code: i32) {
    #[allow(clippy::cast_sign_loss)]
    let kind = kind_code as u32 as u64;
    #[allow(clippy::cast_sign_loss)]
    let os = os_code as u32 as u64;
    LAST_NET_ERROR_CODE.store((os << 32) | kind, Ordering::Relaxed);
}

/// Read the canonical error kind from the last net op (low 32 bits).
#[must_use]
pub fn last_net_error_kind() -> i32 {
    #[allow(clippy::cast_possible_wrap)]
    let kind = (LAST_NET_ERROR_CODE.load(Ordering::Relaxed) & 0xFFFF_FFFF) as i32;
    kind
}

/// Read the raw OS code from the last net op (high 32 bits).
#[must_use]
pub fn last_net_error_os() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    let os = ((LAST_NET_ERROR_CODE.load(Ordering::Relaxed) >> 32) & 0xFFFF_FFFF) as i32;
    os
}

/// Reset the last-error slot to `SUCCESS / 0`. Test-only.
#[doc(hidden)]
pub fn reset_last_net_error_for_tests() {
    LAST_NET_ERROR_CODE.store(0, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § PRIME-DIRECTIVE cap-system : per-thread capability slot.
//
//   The cap-set is a global atomic for stage-0 ; per-thread TLS will
//   replace this in a follow-up slice. Default = `NET_CAP_DEFAULT`
//   (loopback-only). `caps_grant` raises bits ; `caps_revoke` lowers
//   them. There is NO blanket-allow override per `PRIME_DIRECTIVE.md
//   § 6 SCOPE`.
// ───────────────────────────────────────────────────────────────────────

static NET_CAP_BITS: AtomicI32 = AtomicI32::new(NET_CAP_DEFAULT);

/// Grant `cap_bits` to the cap-set. Returns the new cap-set.
///
/// ‼ Granting requires explicit caller consent. There is NO mechanism
/// to grant `NET_CAP_OUTBOUND` / `NET_CAP_INBOUND` "by default" ; every
/// non-loopback net op requires an explicit grant from the host
/// process. This realizes the `CONSENT-ARCH` axiom from
/// `PRIME_DIRECTIVE.md § 5` : consent is informed, granular, revocable,
/// and ongoing.
pub fn caps_grant(cap_bits: i32) -> i32 {
    let masked = cap_bits & NET_CAP_MASK;
    let prev = NET_CAP_BITS.fetch_or(masked, Ordering::Relaxed);
    prev | masked
}

/// Revoke `cap_bits` from the cap-set. Returns the new cap-set.
///
/// Revocation is always permitted. Per `PRIME_DIRECTIVE.md § 5
/// CONSENT-ARCH`, consent is "revocable + granular + informed +
/// ongoing" — code that no longer needs a cap MAY surrender it without
/// penalty.
pub fn caps_revoke(cap_bits: i32) -> i32 {
    let masked = cap_bits & NET_CAP_MASK;
    let prev = NET_CAP_BITS.fetch_and(!masked, Ordering::Relaxed);
    prev & !masked
}

/// Read the current cap-set.
#[must_use]
pub fn caps_current() -> i32 {
    NET_CAP_BITS.load(Ordering::Relaxed)
}

/// Reset the cap-set to `NET_CAP_DEFAULT`. Test-only ; not exported via
/// FFI. Used by `reset_net_for_tests` to give every test a clean slate.
#[doc(hidden)]
pub fn reset_caps_for_tests() {
    NET_CAP_BITS.store(NET_CAP_DEFAULT, Ordering::Relaxed);
}

/// Check whether the address `(ip, port)` requires `NET_CAP_OUTBOUND`
/// or `NET_CAP_INBOUND` and the cap is granted. Returns
/// `Ok(())` if the call may proceed, `Err(CAP_DENIED)` otherwise.
///
/// `addr_is_loopback` returns true for `127.0.0.0/8`. Loopback never
/// requires `NET_CAP_OUTBOUND` / `NET_CAP_INBOUND` — the `NET_CAP_LOOPBACK`
/// default suffices.
pub fn check_caps_for_addr(addr_be: u32, port: u16, is_inbound: bool) -> Result<(), i32> {
    let _ = port; // port not part of cap decision at stage-0
    let caps = caps_current();
    if addr_is_loopback(addr_be) {
        if (caps & NET_CAP_LOOPBACK) != 0 {
            Ok(())
        } else {
            Err(net_error_code::CAP_DENIED)
        }
    } else if is_inbound {
        if (caps & NET_CAP_INBOUND) != 0 {
            Ok(())
        } else {
            Err(net_error_code::CAP_DENIED)
        }
    } else if (caps & NET_CAP_OUTBOUND) != 0 {
        Ok(())
    } else {
        Err(net_error_code::CAP_DENIED)
    }
}

/// `127.0.0.0/8` predicate. Matches both POSIX and Win32 loopback.
#[must_use]
pub const fn addr_is_loopback(addr_be: u32) -> bool {
    // High byte == 0x7F (127). The rest is unspecified ; 127.0.0.1 is
    // canonical but 127.0.0.5 is also loopback.
    (addr_be >> 24) & 0xFF == 0x7F
}

/// Canonical loopback-IPv4 address `127.0.0.1` packed into a `u32`
/// host-byte-order.
pub const LOOPBACK_V4: u32 = 0x7F00_0001;
/// Canonical wildcard-IPv4 address `0.0.0.0` packed into a `u32`
/// host-byte-order. Used for binding to all interfaces (requires
/// `NET_CAP_INBOUND`).
pub const ANY_V4: u32 = 0x0000_0000;

// ───────────────────────────────────────────────────────────────────────
// § Per-op tracker — observable counters for host audit.
// ───────────────────────────────────────────────────────────────────────

static SOCKET_COUNT: AtomicU64 = AtomicU64::new(0);
static LISTEN_COUNT: AtomicU64 = AtomicU64::new(0);
static ACCEPT_COUNT: AtomicU64 = AtomicU64::new(0);
static CONNECT_COUNT: AtomicU64 = AtomicU64::new(0);
static SEND_COUNT: AtomicU64 = AtomicU64::new(0);
static RECV_COUNT: AtomicU64 = AtomicU64::new(0);
static CLOSE_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static BYTES_SENT_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(crate) static BYTES_RECV_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Number of `__cssl_net_socket` calls that returned a valid socket.
#[must_use]
pub fn socket_count() -> u64 {
    SOCKET_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_net_listen` calls.
#[must_use]
pub fn listen_count() -> u64 {
    LISTEN_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_net_accept` calls (regardless of success).
#[must_use]
pub fn accept_count() -> u64 {
    ACCEPT_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_net_connect` calls (regardless of success).
#[must_use]
pub fn connect_count() -> u64 {
    CONNECT_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_net_send` calls (regardless of success).
#[must_use]
pub fn send_count() -> u64 {
    SEND_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_net_recv` calls (regardless of success).
#[must_use]
pub fn recv_count() -> u64 {
    RECV_COUNT.load(Ordering::Relaxed)
}

/// Number of `__cssl_net_close` calls (regardless of success).
#[must_use]
pub fn net_close_count() -> u64 {
    CLOSE_COUNT.load(Ordering::Relaxed)
}

/// Total bytes successfully sent across all `__cssl_net_send` calls.
#[must_use]
pub fn bytes_sent_total() -> u64 {
    BYTES_SENT_TOTAL.load(Ordering::Relaxed)
}

/// Total bytes successfully received across all `__cssl_net_recv` calls.
#[must_use]
pub fn bytes_recv_total() -> u64 {
    BYTES_RECV_TOTAL.load(Ordering::Relaxed)
}

/// Reset all net counters + last-error slot + cap-set. Test-only.
#[doc(hidden)]
pub fn reset_net_for_tests() {
    SOCKET_COUNT.store(0, Ordering::Relaxed);
    LISTEN_COUNT.store(0, Ordering::Relaxed);
    ACCEPT_COUNT.store(0, Ordering::Relaxed);
    CONNECT_COUNT.store(0, Ordering::Relaxed);
    SEND_COUNT.store(0, Ordering::Relaxed);
    RECV_COUNT.store(0, Ordering::Relaxed);
    CLOSE_COUNT.store(0, Ordering::Relaxed);
    BYTES_SENT_TOTAL.store(0, Ordering::Relaxed);
    BYTES_RECV_TOTAL.store(0, Ordering::Relaxed);
    reset_last_net_error_for_tests();
    reset_caps_for_tests();
}

#[doc(hidden)]
pub(crate) fn record_socket() {
    SOCKET_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[doc(hidden)]
pub(crate) fn record_listen() {
    LISTEN_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[doc(hidden)]
pub(crate) fn record_accept() {
    ACCEPT_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[doc(hidden)]
pub(crate) fn record_connect() {
    CONNECT_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[doc(hidden)]
pub(crate) fn record_send(bytes: u64) {
    SEND_COUNT.fetch_add(1, Ordering::Relaxed);
    if bytes != 0 {
        BYTES_SENT_TOTAL.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[doc(hidden)]
pub(crate) fn record_recv(bytes: u64) {
    RECV_COUNT.fetch_add(1, Ordering::Relaxed);
    if bytes != 0 {
        BYTES_RECV_TOTAL.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[doc(hidden)]
pub(crate) fn record_net_close() {
    CLOSE_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § Cross-platform `*_impl` re-exports : platform layer is selected via
//   cfg. Each platform crate exposes the same fn names so the FFI layer
//   in [`crate::ffi`] can call into them uniformly. Mirrors the
//   `crate::io` surface from S6-B5.
// ───────────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub use crate::net_win32::{
    cssl_net_accept_impl, cssl_net_close_impl, cssl_net_connect_impl, cssl_net_listen_impl,
    cssl_net_recv_impl, cssl_net_recvfrom_impl, cssl_net_send_impl, cssl_net_sendto_impl,
    cssl_net_socket_impl,
};

#[cfg(not(target_os = "windows"))]
pub use crate::net_unix::{
    cssl_net_accept_impl, cssl_net_close_impl, cssl_net_connect_impl, cssl_net_listen_impl,
    cssl_net_recv_impl, cssl_net_recvfrom_impl, cssl_net_send_impl, cssl_net_sendto_impl,
    cssl_net_socket_impl,
};

// ───────────────────────────────────────────────────────────────────────
// § Validation helpers shared by both platforms.
// ───────────────────────────────────────────────────────────────────────

/// Validate caller-supplied socket-flags : reject unknown bits + check
/// for inconsistent combinations.
///
/// Returns `Ok(())` on valid flags, `Err(InvalidInput-code)` otherwise.
pub fn validate_sock_flags(flags: i32) -> Result<(), i32> {
    // Reject any bit outside the recognized mask.
    if (flags & !SOCK_FLAG_MASK) != 0 {
        return Err(net_error_code::INVALID_INPUT);
    }
    // Must specify exactly one transport.
    let tcp = (flags & SOCK_TCP) != 0;
    let udp = (flags & SOCK_UDP) != 0;
    if tcp == udp {
        // Either both or neither — invalid.
        return Err(net_error_code::INVALID_INPUT);
    }
    // SOCK_NODELAY only meaningful for TCP — but accept on UDP as a no-op
    // rather than rejecting. POSIX silently ignores the flag on UDP.
    Ok(())
}

/// Validate caller-supplied `(ptr, len)` pair for send / recv. Returns
/// `Ok(())` if the pair is well-formed (null is rejected for non-zero
/// length), `Err(InvalidInput-code)` otherwise. Matches the
/// `crate::io::validate_buffer` contract.
pub fn validate_buffer(ptr: *const u8, len: usize) -> Result<(), i32> {
    if len > 0 && ptr.is_null() {
        return Err(net_error_code::INVALID_INPUT);
    }
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    if len > isize::MAX as usize {
        return Err(net_error_code::INVALID_INPUT);
    }
    Ok(())
}

/// Validate a port number. Port 0 is the canonical "OS picks a free
/// port" wildcard for `bind` (often used in tests) ; we accept it here
/// rather than rejecting.
pub fn validate_port(port: u16) -> Result<(), i32> {
    let _ = port;
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — counter + flag + cap + validation surface.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_socket_is_negative_one() {
        // ‼ ABI-stable per cssl-rt FFI invariant ; renaming requires major bump.
        assert_eq!(INVALID_SOCKET, -1);
    }

    #[test]
    fn sock_flag_mask_includes_all_bits() {
        let expected = SOCK_TCP | SOCK_UDP | SOCK_NONBLOCK | SOCK_REUSEADDR | SOCK_NODELAY;
        assert_eq!(SOCK_FLAG_MASK, expected);
    }

    #[test]
    fn sock_flag_bits_are_distinct() {
        let bits = [
            SOCK_TCP,
            SOCK_UDP,
            SOCK_NONBLOCK,
            SOCK_REUSEADDR,
            SOCK_NODELAY,
        ];
        for (i, a) in bits.iter().enumerate() {
            for b in &bits[i + 1..] {
                assert_eq!(a & b, 0, "flags must be mutually distinct bits");
            }
        }
    }

    #[test]
    fn net_cap_mask_includes_all_bits() {
        let expected = NET_CAP_LOOPBACK | NET_CAP_OUTBOUND | NET_CAP_INBOUND;
        assert_eq!(NET_CAP_MASK, expected);
    }

    #[test]
    fn net_cap_bits_are_distinct() {
        let bits = [NET_CAP_LOOPBACK, NET_CAP_OUTBOUND, NET_CAP_INBOUND];
        for (i, a) in bits.iter().enumerate() {
            for b in &bits[i + 1..] {
                assert_eq!(a & b, 0, "caps must be mutually distinct bits");
            }
        }
    }

    #[test]
    fn net_cap_default_is_loopback_only() {
        assert_eq!(NET_CAP_DEFAULT, NET_CAP_LOOPBACK);
    }

    #[test]
    fn net_error_codes_are_distinct() {
        let codes = [
            net_error_code::INVALID_INPUT,
            net_error_code::ADDR_IN_USE,
            net_error_code::ADDR_NOT_AVAILABLE,
            net_error_code::CONNECTION_REFUSED,
            net_error_code::CONNECTION_RESET,
            net_error_code::CONNECTION_ABORTED,
            net_error_code::NOT_CONNECTED,
            net_error_code::TIMED_OUT,
            net_error_code::WOULD_BLOCK,
            net_error_code::INTERRUPTED,
            net_error_code::PERMISSION_DENIED,
            net_error_code::HOST_UNREACHABLE,
            net_error_code::NETWORK_UNREACHABLE,
            net_error_code::UNEXPECTED_EOF,
            net_error_code::BROKEN_PIPE,
            net_error_code::NOT_INITIALIZED,
            net_error_code::CAP_DENIED,
            net_error_code::OTHER,
        ];
        let mut seen = codes.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), codes.len(), "error codes must be unique");
    }

    #[test]
    fn last_error_records_kind_and_os_code() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_net_error(net_error_code::CONNECTION_REFUSED, 10061);
        assert_eq!(last_net_error_kind(), net_error_code::CONNECTION_REFUSED);
        assert_eq!(last_net_error_os(), 10061);
    }

    #[test]
    fn last_error_kind_other_carries_os_code() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_net_error(net_error_code::OTHER, 99_999);
        assert_eq!(last_net_error_kind(), net_error_code::OTHER);
        assert_eq!(last_net_error_os(), 99_999);
    }

    #[test]
    fn reset_clears_last_error() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_net_error(net_error_code::CONNECTION_REFUSED, 10061);
        reset_last_net_error_for_tests();
        assert_eq!(last_net_error_kind(), net_error_code::SUCCESS);
        assert_eq!(last_net_error_os(), 0);
    }

    #[test]
    fn counters_start_at_zero() {
        let _g = crate::test_helpers::lock_and_reset_all();
        assert_eq!(socket_count(), 0);
        assert_eq!(listen_count(), 0);
        assert_eq!(accept_count(), 0);
        assert_eq!(connect_count(), 0);
        assert_eq!(send_count(), 0);
        assert_eq!(recv_count(), 0);
        assert_eq!(net_close_count(), 0);
        assert_eq!(bytes_sent_total(), 0);
        assert_eq!(bytes_recv_total(), 0);
    }

    #[test]
    fn record_socket_increments_counter() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_socket();
        record_socket();
        assert_eq!(socket_count(), 2);
    }

    #[test]
    fn record_send_increments_counter_and_bytes_total() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_send(64);
        record_send(0); // 0-byte send still counts the call
        assert_eq!(send_count(), 2);
        assert_eq!(bytes_sent_total(), 64);
    }

    #[test]
    fn record_recv_increments_counter_and_bytes_total() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_recv(128);
        record_recv(256);
        assert_eq!(recv_count(), 2);
        assert_eq!(bytes_recv_total(), 384);
    }

    #[test]
    fn record_net_close_increments_counter() {
        let _g = crate::test_helpers::lock_and_reset_all();
        record_net_close();
        assert_eq!(net_close_count(), 1);
    }

    #[test]
    fn validate_sock_flags_accepts_tcp() {
        assert!(validate_sock_flags(SOCK_TCP).is_ok());
    }

    #[test]
    fn validate_sock_flags_accepts_udp() {
        assert!(validate_sock_flags(SOCK_UDP).is_ok());
    }

    #[test]
    fn validate_sock_flags_accepts_tcp_with_modifiers() {
        assert!(
            validate_sock_flags(SOCK_TCP | SOCK_NONBLOCK | SOCK_REUSEADDR | SOCK_NODELAY).is_ok()
        );
    }

    #[test]
    fn validate_sock_flags_rejects_no_transport() {
        let r = validate_sock_flags(SOCK_NONBLOCK);
        assert_eq!(r, Err(net_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_sock_flags_rejects_both_transports() {
        let r = validate_sock_flags(SOCK_TCP | SOCK_UDP);
        assert_eq!(r, Err(net_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_sock_flags_rejects_unknown_bit() {
        let r = validate_sock_flags(0x8000);
        assert_eq!(r, Err(net_error_code::INVALID_INPUT));
    }

    #[test]
    fn validate_buffer_accepts_zero_length_null() {
        assert!(validate_buffer(core::ptr::null(), 0).is_ok());
    }

    #[test]
    fn validate_buffer_rejects_null_with_nonzero_length() {
        let r = validate_buffer(core::ptr::null(), 16);
        assert_eq!(r, Err(net_error_code::INVALID_INPUT));
    }

    #[test]
    fn loopback_v4_predicate_matches_127_0_0_1() {
        assert!(addr_is_loopback(LOOPBACK_V4));
        assert!(addr_is_loopback(0x7F00_0001));
        assert!(addr_is_loopback(0x7F00_0005));
        assert!(addr_is_loopback(0x7FFF_FFFF));
    }

    #[test]
    fn loopback_v4_predicate_rejects_non_127() {
        assert!(!addr_is_loopback(0x0808_0808)); // 8.8.8.8
        assert!(!addr_is_loopback(ANY_V4)); // 0.0.0.0
        assert!(!addr_is_loopback(0xC0A8_0001)); // 192.168.0.1
    }

    #[test]
    fn caps_default_is_loopback_only() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // After reset, caps == NET_CAP_DEFAULT == NET_CAP_LOOPBACK.
        assert_eq!(caps_current(), NET_CAP_LOOPBACK);
        assert_eq!(caps_current() & NET_CAP_OUTBOUND, 0);
        assert_eq!(caps_current() & NET_CAP_INBOUND, 0);
    }

    #[test]
    fn caps_grant_raises_bits() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let r = caps_grant(NET_CAP_OUTBOUND);
        assert_ne!(r & NET_CAP_OUTBOUND, 0);
        assert_ne!(caps_current() & NET_CAP_OUTBOUND, 0);
    }

    #[test]
    fn caps_revoke_lowers_bits() {
        let _g = crate::test_helpers::lock_and_reset_all();
        caps_grant(NET_CAP_OUTBOUND);
        let r = caps_revoke(NET_CAP_OUTBOUND);
        assert_eq!(r & NET_CAP_OUTBOUND, 0);
        assert_eq!(caps_current() & NET_CAP_OUTBOUND, 0);
    }

    #[test]
    fn caps_grant_rejects_unknown_bits() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // Unknown bit should be masked out by NET_CAP_MASK.
        let r = caps_grant(0x8000);
        assert_eq!(r & 0x8000, 0);
    }

    #[test]
    fn check_caps_for_loopback_outbound_succeeds_default() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // 127.0.0.1, port 80, outbound — should succeed under default cap-set.
        assert!(check_caps_for_addr(LOOPBACK_V4, 80, false).is_ok());
    }

    #[test]
    fn check_caps_for_loopback_inbound_succeeds_default() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // 127.0.0.1, port 0, inbound — should succeed under default cap-set.
        assert!(check_caps_for_addr(LOOPBACK_V4, 0, true).is_ok());
    }

    #[test]
    fn check_caps_for_non_loopback_outbound_denied_default() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // 8.8.8.8, port 53, outbound — denied under default cap-set.
        let r = check_caps_for_addr(0x0808_0808, 53, false);
        assert_eq!(r, Err(net_error_code::CAP_DENIED));
    }

    #[test]
    fn check_caps_for_non_loopback_outbound_allowed_after_grant() {
        let _g = crate::test_helpers::lock_and_reset_all();
        caps_grant(NET_CAP_OUTBOUND);
        // 8.8.8.8, port 53, outbound — allowed after grant.
        assert!(check_caps_for_addr(0x0808_0808, 53, false).is_ok());
    }

    #[test]
    fn check_caps_for_non_loopback_inbound_denied_default() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // 0.0.0.0, port 0, inbound — denied under default cap-set.
        let r = check_caps_for_addr(ANY_V4, 0, true);
        assert_eq!(r, Err(net_error_code::CAP_DENIED));
    }

    #[test]
    fn check_caps_for_non_loopback_inbound_allowed_after_grant() {
        let _g = crate::test_helpers::lock_and_reset_all();
        caps_grant(NET_CAP_INBOUND);
        // 0.0.0.0, port 0, inbound — allowed after grant.
        assert!(check_caps_for_addr(ANY_V4, 0, true).is_ok());
    }

    #[test]
    fn loopback_constants_canonical() {
        assert_eq!(LOOPBACK_V4, 0x7F00_0001);
        assert_eq!(ANY_V4, 0);
    }
}
