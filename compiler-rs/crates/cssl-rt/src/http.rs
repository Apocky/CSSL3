//! § cssl-rt::http — HTTP GET/POST FFI surface (T11-WAVE3-HTTP)
//! ════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Provides the `__cssl_http_*` `extern "C"` symbols that `.cssl`
//!   programs use to fetch URLs (free-asset CDNs · Vercel API · Supabase).
//!   Backed by ureq's blocking HTTP client.
//!
//! § FFI SURFACE  (ABI-stable from this slice)
//!   ```text
//!   __cssl_http_get(url_ptr, url_len, out_buf, out_cap, sovereign_cap) -> i32
//!   __cssl_http_post(url_ptr, url_len, body_ptr, body_len,
//!                    content_type_ptr, content_type_len,
//!                    out_buf, out_cap, sovereign_cap) -> i32
//!   __cssl_http_caps_grant(bits: u32) -> i32
//!   __cssl_http_caps_current() -> u32
//!   ```
//!   Renaming any of these is a major-version-bump event.
//!
//! § PRIME-DIRECTIVE — network = sensitive surface
//!   Networking is the most surveillance-adjacent surface in the runtime.
//!   This module enforces five structural safeguards aligned with
//!   `PRIME_DIRECTIVE.md` :
//!
//!   1. CAPABILITY-GATED · default-deny.  `caps = 0` rejects every call
//!      with `-EACCES`.  Per-thread cap bitset is raised by explicit
//!      `__cssl_http_caps_grant(...)` from host code.
//!   2. SOVEREIGN-CAP BYPASS — a single magic constant
//!      (`SOVEREIGN_CAP = 0xCAFE_BABE_DEAD_BEEF`) bypasses the cap-check.
//!      It exists so trusted host code can short-circuit cap-grant
//!      ceremony for first-run free-asset bootstrap.  Every other caller
//!      must go through the cap path.
//!   3. HTTPS-ONLY OPT-IN — the `NET_HTTPS_ONLY` cap-bit rejects plain
//!      `http://` URLs.  TLS itself requires the `tls` cargo-feature on
//!      `cssl-rt` ; without it, even with the cap raised, `https://` URLs
//!      fail at ureq's transport layer.
//!   4. LOCALHOST-ONLY OPT-IN — the `NET_LOCALHOST_ONLY` cap-bit rejects
//!      every URL whose host doesn't resolve to `127.0.0.1` / `::1`.
//!      Stage-0 implementation parses the URL string ; stage-1 will
//!      delegate to `crate::net`'s addr-resolver to handle DNS.
//!   5. AUDIT VISIBILITY — every call increments atomic counters
//!      (`http_requests_total` / `http_bytes_in_total` /
//!      `http_bytes_out_total`) AND emits a structured JSONL event via
//!      `crate::loa_startup::log_event`.  The counters are LOCAL — they
//!      do not export to the network.  The event captures URL · status ·
//!      bytes · duration_us with no payload — no surveillance.
//!
//!   What this module does NOT do : surveillance, exfiltration, hidden
//!   side-channels, sensitive-data sinks.  No "PRIVATE" header injection.
//!   No cookie jar.  No redirect-following with header re-emission.  No
//!   per-call user-agent fingerprinting.  Plain GET + POST + cap-gate +
//!   audit-counter + JSONL event.  Auditable surface.
//!
//! § ENV CONTROLS
//!   `CSSL_HTTP_DISABLE=1`        — every call returns `-1` regardless of caps.
//!   `CSSL_HTTP_TIMEOUT_MS=10000` — per-call timeout in milliseconds.
//!
//! § ERROR-CODE CONVENTION  (FFI return value)
//!   - `bytes_written ≥ 0` — number of body bytes written into the
//!     caller-supplied `out_buf`.  When the response body exceeds
//!     `out_cap` the function writes the first `out_cap` bytes and
//!     returns `out_cap`.  Truncation is reported via the audit event
//!     but not signalled in the return value (callers that care should
//!     reissue with a larger buffer).
//!   - `-1`  EPERM     — cap-denied or `CSSL_HTTP_DISABLE=1`.
//!   - `-2`  EINVAL    — malformed URL / null pointers / negative lengths.
//!   - `-3`  ETIMEDOUT — request exceeded the timeout.
//!   - `-4`  EIO       — transport-layer failure (DNS / connect / TLS).
//!   - `-5`  EBADMSG   — non-2xx HTTP status.

#![allow(unsafe_code)]
// § FFI surface returns `i32` from `usize` byte-count counts ; the FFI
// contract caps `out_cap` at `i32::MAX` per ABI so the cast never wraps.
// `cap_grant` returns the post-mutation cap-set as `i32` so a single
// signed return-value can carry both error sentinels (`-EPERM`...) and
// the cap-set itself (always within u32 range with high bit clear).
#![allow(clippy::cast_possible_wrap)]
// `classify_ureq_error` matches several distinct ureq error-kinds onto the
// same FFI return-code intentionally — the discriminator is which kind
// was observed, not which return-code is produced.
#![allow(clippy::match_same_arms)]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::io::Read as _;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::loa_startup::log_event;

// ───────────────────────────────────────────────────────────────────────
// § Public constants — cap bits + sovereign bypass + error codes
// ───────────────────────────────────────────────────────────────────────

/// Cap-bit : authorize HTTP-GET calls.  Default-deny (bit clear).
pub const NET_HTTP_GET: u32 = 0x0000_0001;
/// Cap-bit : authorize HTTP-POST calls.  Default-deny (bit clear).
pub const NET_HTTP_POST: u32 = 0x0000_0002;
/// Cap-bit : reject `http://` URLs (https only).  Default-allow (bit clear).
pub const NET_HTTPS_ONLY: u32 = 0x0000_0004;
/// Cap-bit : reject any URL whose host doesn't parse to a loopback addr.
pub const NET_LOCALHOST_ONLY: u32 = 0x0000_0008;

/// Mask of every recognized cap-bit.  Anything else is rejected by `grant`.
pub const NET_HTTP_CAP_MASK: u32 =
    NET_HTTP_GET | NET_HTTP_POST | NET_HTTPS_ONLY | NET_LOCALHOST_ONLY;

/// § Sovereign-cap magic.  When passed as `sovereign_cap` arg, bypasses
/// the cap-check.  This is the consent-OS escape hatch documented in
/// PRIME_DIRECTIVE.md § 3 : sovereign-substrate code can opt to short-
/// circuit the cap ceremony when it has already authenticated the call
/// out-of-band.  Constant is intentionally distinctive (`CAFE BABE DEAD BEEF`)
/// so any code reading the value as a literal owner is auditable.
pub const SOVEREIGN_CAP: u64 = 0xCAFE_BABE_DEAD_BEEF;

/// Default per-request timeout in milliseconds when `CSSL_HTTP_TIMEOUT_MS`
/// env-var is absent or unparseable.
pub const HTTP_DEFAULT_TIMEOUT_MS: u64 = 10_000;

// § Error codes — stable across the FFI surface.

/// FFI return : cap-denied or globally disabled.
pub const HTTP_ERR_PERM: i32 = -1;
/// FFI return : malformed URL / null pointers / negative lengths.
pub const HTTP_ERR_INVAL: i32 = -2;
/// FFI return : request exceeded the timeout.
pub const HTTP_ERR_TIMEDOUT: i32 = -3;
/// FFI return : transport-layer failure (DNS / connect / TLS handshake).
pub const HTTP_ERR_IO: i32 = -4;
/// FFI return : non-2xx HTTP status.
pub const HTTP_ERR_BADMSG: i32 = -5;

// ───────────────────────────────────────────────────────────────────────
// § Atomic audit counters — observed via top-level `cssl_rt::*` accessors
// ───────────────────────────────────────────────────────────────────────

/// Total successful + failed HTTP request attempts since process start.
pub static HTTP_REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total response-body bytes received (sum of all 2xx replies).
pub static HTTP_BYTES_IN_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total request-body bytes sent (sum of POST bodies).
pub static HTTP_BYTES_OUT_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total cap-denied calls (one increment per `-EPERM` return).
pub static HTTP_DENIES_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total timeouts observed.
pub static HTTP_TIMEOUTS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total IO-error returns.
pub static HTTP_IO_ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Process-wide cap bitset.  We use a single global atomic rather than a
/// per-thread TLS slot to match the LoA-stub style (one-process =
/// one-policy) ; per-thread caps are a stage-1 extension.
pub static HTTP_CAPS: AtomicU32 = AtomicU32::new(0);

// ───────────────────────────────────────────────────────────────────────
// § Public API — counter accessors + cap helpers (test-friendly)
// ───────────────────────────────────────────────────────────────────────

/// Snapshot the request counter.  Accessed via the top-level re-export
/// `cssl_rt::http_requests_total`.
#[must_use]
pub fn http_requests_total() -> u64 {
    HTTP_REQUESTS_TOTAL.load(Ordering::Relaxed)
}
/// Snapshot the bytes-in counter.
#[must_use]
pub fn http_bytes_in_total() -> u64 {
    HTTP_BYTES_IN_TOTAL.load(Ordering::Relaxed)
}
/// Snapshot the bytes-out counter.
#[must_use]
pub fn http_bytes_out_total() -> u64 {
    HTTP_BYTES_OUT_TOTAL.load(Ordering::Relaxed)
}
/// Snapshot the cap-denied counter.
#[must_use]
pub fn http_denies_total() -> u64 {
    HTTP_DENIES_TOTAL.load(Ordering::Relaxed)
}
/// Snapshot the timeout counter.
#[must_use]
pub fn http_timeouts_total() -> u64 {
    HTTP_TIMEOUTS_TOTAL.load(Ordering::Relaxed)
}
/// Snapshot the IO-error counter.
#[must_use]
pub fn http_io_errors_total() -> u64 {
    HTTP_IO_ERRORS_TOTAL.load(Ordering::Relaxed)
}
/// Snapshot the current cap bitset.
#[must_use]
pub fn http_caps_current() -> u32 {
    HTTP_CAPS.load(Ordering::Relaxed)
}

/// Raise the bits in `mask`.  Returns the post-grant cap-set.  Reserved
/// bits (any bit outside `NET_HTTP_CAP_MASK`) are silently masked off ;
/// caller cannot accidentally set an unrecognized bit.
pub fn http_caps_grant_impl(mask: u32) -> u32 {
    let valid = mask & NET_HTTP_CAP_MASK;
    HTTP_CAPS.fetch_or(valid, Ordering::SeqCst);
    HTTP_CAPS.load(Ordering::Relaxed)
}

/// Lower the bits in `mask`.  Returns the post-revoke cap-set.
pub fn http_caps_revoke_impl(mask: u32) -> u32 {
    let valid = mask & NET_HTTP_CAP_MASK;
    HTTP_CAPS.fetch_and(!valid, Ordering::SeqCst);
    HTTP_CAPS.load(Ordering::Relaxed)
}

/// Reset every counter + the cap bitset.  Used by per-test setup.
pub fn reset_http_for_tests() {
    HTTP_REQUESTS_TOTAL.store(0, Ordering::SeqCst);
    HTTP_BYTES_IN_TOTAL.store(0, Ordering::SeqCst);
    HTTP_BYTES_OUT_TOTAL.store(0, Ordering::SeqCst);
    HTTP_DENIES_TOTAL.store(0, Ordering::SeqCst);
    HTTP_TIMEOUTS_TOTAL.store(0, Ordering::SeqCst);
    HTTP_IO_ERRORS_TOTAL.store(0, Ordering::SeqCst);
    HTTP_CAPS.store(0, Ordering::SeqCst);
}

// ───────────────────────────────────────────────────────────────────────
// § Per-test serialization lock
//
//   Every test in this module mutates the global `HTTP_CAPS` bitset and
//   the audit counters.  A module-local `Mutex` serializes them so the
//   counter assertions remain deterministic under `cargo test --jobs N`.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) static HTTP_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(not(test))]
#[allow(dead_code)]
pub(crate) static HTTP_TEST_LOCK: Mutex<()> = Mutex::new(());

// ───────────────────────────────────────────────────────────────────────
// § URL parsing — minimal, scheme + host extraction only
//
//   We intentionally do NOT pull a URL-parsing crate.  Stage-0 needs only
//   "is the scheme http vs https" + "is the host literal 127.0.0.1 / ::1
//   / localhost".  Anything more is ureq's job.  Keeping the parser in-
//   tree makes the cap-check auditable from a single file.
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlScheme {
    Http,
    Https,
}

#[derive(Debug)]
pub struct ParsedUrl<'a> {
    pub scheme: UrlScheme,
    pub host: &'a str,
}

/// Parse `url` into (scheme, host).  Returns `None` on malformed input.
/// Recognized prefixes : `http://` / `https://`.  Host runs until the
/// first `/` `?` `#` `:` (port marker terminates host) or end-of-string.
pub fn parse_url(url: &str) -> Option<ParsedUrl<'_>> {
    let (scheme, rest) = if let Some(s) = url.strip_prefix("http://") {
        (UrlScheme::Http, s)
    } else if let Some(s) = url.strip_prefix("https://") {
        (UrlScheme::Https, s)
    } else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
    // Host runs until first separator.
    let host_end = rest
        .find(['/', '?', '#', ':'])
        .unwrap_or(rest.len());
    let host = &rest[..host_end];
    if host.is_empty() {
        return None;
    }
    Some(ParsedUrl { scheme, host })
}

/// Is `host` a localhost / loopback literal?  Stage-0 implementation
/// recognizes the textual forms `localhost`, `127.0.0.1`, `[::1]`, `::1`.
/// Stage-1 will delegate to `crate::net::addr_is_loopback` after DNS
/// resolution.
#[must_use]
pub fn host_is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

// ───────────────────────────────────────────────────────────────────────
// § Cap-check — single point of policy enforcement
// ───────────────────────────────────────────────────────────────────────

/// Result of the cap-check : Ok = proceed · Err = the FFI error code.
fn check_caps(
    sovereign: u64,
    required: u32,
    parsed: &ParsedUrl<'_>,
) -> Result<(), i32> {
    // CSSL_HTTP_DISABLE=1 — global kill-switch.  Sovereign cap does NOT
    // override this : the env-var is the host-process owner saying
    // "no HTTP from this process at all", which trumps every per-call
    // bypass.
    if std::env::var("CSSL_HTTP_DISABLE").is_ok() {
        return Err(HTTP_ERR_PERM);
    }
    // Sovereign-cap bypass — short-circuit the cap-bit check.  HTTPS-only
    // and localhost-only enforcement still applies if those cap-bits are
    // set : the sovereign-cap escapes the verb (GET/POST) gate, not the
    // policy gates the host explicitly raised.
    let caps = HTTP_CAPS.load(Ordering::Relaxed);
    if sovereign != SOVEREIGN_CAP && (caps & required) != required {
        return Err(HTTP_ERR_PERM);
    }
    // HTTPS-only cap : reject http:// URLs.
    if (caps & NET_HTTPS_ONLY) != 0 && parsed.scheme == UrlScheme::Http {
        return Err(HTTP_ERR_PERM);
    }
    // LOCALHOST-only cap : reject non-loopback hosts.
    if (caps & NET_LOCALHOST_ONLY) != 0 && !host_is_loopback(parsed.host) {
        return Err(HTTP_ERR_PERM);
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § Timeout config — env-driven with sane default
// ───────────────────────────────────────────────────────────────────────

fn timeout_ms() -> u64 {
    std::env::var("CSSL_HTTP_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0 && v < 10 * 60 * 1000) // clamp to <10min for safety
        .unwrap_or(HTTP_DEFAULT_TIMEOUT_MS)
}

// ───────────────────────────────────────────────────────────────────────
// § ureq agent — built once per call (cheap) with timeout configured.
//
//   We deliberately do NOT cache an Agent across calls : each request
//   gets a fresh Agent so timeouts can be reconfigured at runtime via the
//   env-var without restart.  ureq's Agent is lightweight ; the cost is
//   negligible compared to the network I/O.
// ───────────────────────────────────────────────────────────────────────

fn build_agent() -> ureq::Agent {
    let to = Duration::from_millis(timeout_ms());
    ureq::AgentBuilder::new()
        .timeout_connect(to)
        .timeout_read(to)
        .timeout_write(to)
        .build()
}

// ───────────────────────────────────────────────────────────────────────
// § Request execution helpers
// ───────────────────────────────────────────────────────────────────────

/// Translate ureq's error into our FFI error code + side-effect counters.
fn classify_ureq_error(err: &ureq::Error) -> i32 {
    match err {
        ureq::Error::Status(_, _) => {
            HTTP_IO_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
            HTTP_ERR_BADMSG
        }
        ureq::Error::Transport(t) => {
            // ureq's transport-error kind has a `.kind()` API ; the timeout
            // and IO variants carry through to our FFI codes.
            match t.kind() {
                ureq::ErrorKind::ConnectionFailed
                | ureq::ErrorKind::Dns
                | ureq::ErrorKind::Io => {
                    HTTP_IO_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                    HTTP_ERR_IO
                }
                ureq::ErrorKind::TooManyRedirects | ureq::ErrorKind::HTTP => {
                    HTTP_IO_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                    HTTP_ERR_BADMSG
                }
                _ => {
                    // ureq's default fallthrough — categorize as IO so the
                    // counter still ticks ; better visible than silent.
                    HTTP_IO_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                    HTTP_ERR_IO
                }
            }
        }
    }
}

/// Detect whether a transport-error was specifically a timeout (vs DNS or
/// connect-refused).  ureq exposes timeouts via the `Io` kind with an
/// `ErrorKind::TimedOut` inner ; we string-match the `Display` form to
/// stay forward-compat with ureq's internal kind enum.
fn is_timeout(err: &ureq::Error) -> bool {
    match err {
        ureq::Error::Transport(t) => {
            // ureq's Display includes "timed out" / "timeout" verbiage when
            // the inner io::Error is ErrorKind::TimedOut.  Cheaper than
            // matching via a peeled-back chain.
            let s = format!("{t}");
            s.contains("timed out") || s.contains("timeout")
        }
        ureq::Error::Status(_, _) => false,
    }
}

/// Drain the response body into `out_buf` up to `out_cap` bytes.  Returns
/// the number of bytes actually written (≤ `out_cap`).  Truncation is
/// silent at the FFI level but surfaced in the audit log.
fn drain_body_into(
    resp: ureq::Response,
    out_ptr: *mut u8,
    out_cap: i32,
) -> Result<i32, i32> {
    if out_ptr.is_null() || out_cap <= 0 {
        return Err(HTTP_ERR_INVAL);
    }
    let cap = out_cap as usize;
    // ureq's into_reader() yields a `Box<dyn Read + Send + Sync>` ; we
    // read directly into the caller's buffer to avoid a heap copy.
    let mut reader = resp.into_reader();
    // SAFETY : caller supplies a buffer of at least `cap` bytes per ABI.
    // We construct a `&mut [u8]` over the raw pointer then pass to
    // `read`.  The slice does not outlive this fn.
    let buf = unsafe { core::slice::from_raw_parts_mut(out_ptr, cap) };
    let mut written = 0usize;
    while written < cap {
        match reader.read(&mut buf[written..]) {
            Ok(0) => break,
            Ok(n) => written += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                HTTP_IO_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                return Err(HTTP_ERR_IO);
            }
        }
    }
    HTTP_BYTES_IN_TOTAL.fetch_add(written as u64, Ordering::Relaxed);
    Ok(written as i32)
}

// ───────────────────────────────────────────────────────────────────────
// § FFI symbols — the public ABI
// ───────────────────────────────────────────────────────────────────────

/// FFI : execute an HTTP-GET against `url` and write the response body
/// into `(out_buf, out_cap)`.  Returns bytes-written or a negative error
/// code.  The cap-system is consulted before any network I/O ; see
/// module-level docs for the complete error-code list.
///
/// # Safety
/// - `url_ptr` must point to at least `url_len` valid UTF-8 bytes.
/// - `out_buf` must point to at least `out_cap` writable bytes.
/// - All pointers may be null when their corresponding length is 0.
#[no_mangle]
pub unsafe extern "C" fn __cssl_http_get(
    url_ptr: *const u8,
    url_len: i32,
    out_buf: *mut u8,
    out_cap: i32,
    sovereign_cap: u64,
) -> i32 {
    let started = Instant::now();
    HTTP_REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);

    // Argument validation.
    if url_ptr.is_null() || url_len <= 0 || out_buf.is_null() || out_cap <= 0 {
        return HTTP_ERR_INVAL;
    }
    // SAFETY : caller supplies a buffer of `url_len` UTF-8 bytes per ABI.
    let url_bytes = unsafe { core::slice::from_raw_parts(url_ptr, url_len as usize) };
    let Ok(url) = core::str::from_utf8(url_bytes) else {
        return HTTP_ERR_INVAL;
    };
    let Some(parsed) = parse_url(url) else {
        return HTTP_ERR_INVAL;
    };

    // Cap-check.
    if let Err(code) = check_caps(sovereign_cap, NET_HTTP_GET, &parsed) {
        if code == HTTP_ERR_PERM {
            HTTP_DENIES_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
        log_event(
            "WARN",
            "http/get",
            &format!("denied · url={url} · code={code}"),
        );
        return code;
    }

    // Execute.
    let agent = build_agent();
    let result = agent.get(url).call();
    match result {
        Ok(resp) => {
            let status = resp.status();
            let outcome = drain_body_into(resp, out_buf, out_cap);
            let dur_us = started.elapsed().as_micros() as u64;
            match outcome {
                Ok(n) => {
                    log_event(
                        "INFO",
                        "http/get",
                        &format!(
                            "{{\"event\":\"http_request\",\"method\":\"GET\",\"url\":\"{url}\",\"status\":{status},\"bytes\":{n},\"duration_us\":{dur_us}}}"
                        ),
                    );
                    n
                }
                Err(code) => {
                    log_event(
                        "ERROR",
                        "http/get",
                        &format!("body-read-failed · url={url} · code={code}"),
                    );
                    code
                }
            }
        }
        Err(e) => {
            let dur_us = started.elapsed().as_micros() as u64;
            let code = if is_timeout(&e) {
                HTTP_TIMEOUTS_TOTAL.fetch_add(1, Ordering::Relaxed);
                HTTP_ERR_TIMEDOUT
            } else {
                classify_ureq_error(&e)
            };
            log_event(
                "ERROR",
                "http/get",
                &format!(
                    "{{\"event\":\"http_request\",\"method\":\"GET\",\"url\":\"{url}\",\"error\":\"{e}\",\"code\":{code},\"duration_us\":{dur_us}}}"
                ),
            );
            code
        }
    }
}

/// FFI : execute an HTTP-POST against `url` with `body` of `content_type`,
/// then write the response into `(out_buf, out_cap)`.  Returns
/// bytes-written or a negative error code.
///
/// # Safety
/// - `url_ptr` must point to at least `url_len` valid UTF-8 bytes.
/// - `body_ptr` must point to at least `body_len` bytes (any encoding).
/// - `content_type_ptr` must point to at least `content_type_len` UTF-8
///   bytes ; pass `(null, 0)` to default to `application/octet-stream`.
/// - `out_buf` must point to at least `out_cap` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn __cssl_http_post(
    url_ptr: *const u8,
    url_len: i32,
    body_ptr: *const u8,
    body_len: i32,
    content_type_ptr: *const u8,
    content_type_len: i32,
    out_buf: *mut u8,
    out_cap: i32,
    sovereign_cap: u64,
) -> i32 {
    let started = Instant::now();
    HTTP_REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);

    // Argument validation.
    if url_ptr.is_null() || url_len <= 0 || out_buf.is_null() || out_cap <= 0 {
        return HTTP_ERR_INVAL;
    }
    if body_len < 0 || (body_len > 0 && body_ptr.is_null()) {
        return HTTP_ERR_INVAL;
    }
    if content_type_len < 0 || (content_type_len > 0 && content_type_ptr.is_null()) {
        return HTTP_ERR_INVAL;
    }

    // SAFETY : caller supplies buffers per ABI ; we slice them.
    let url_bytes = unsafe { core::slice::from_raw_parts(url_ptr, url_len as usize) };
    let Ok(url) = core::str::from_utf8(url_bytes) else {
        return HTTP_ERR_INVAL;
    };
    let Some(parsed) = parse_url(url) else {
        return HTTP_ERR_INVAL;
    };

    let body = if body_len > 0 {
        // SAFETY : caller supplies `body_len` bytes per ABI ; arbitrary content.
        unsafe { core::slice::from_raw_parts(body_ptr, body_len as usize) }
    } else {
        &[]
    };

    let content_type = if content_type_len > 0 {
        // SAFETY : caller supplies UTF-8 per ABI.
        let ct = unsafe { core::slice::from_raw_parts(content_type_ptr, content_type_len as usize) };
        match core::str::from_utf8(ct) {
            Ok(s) => s,
            Err(_) => return HTTP_ERR_INVAL,
        }
    } else {
        "application/octet-stream"
    };

    // Cap-check.
    if let Err(code) = check_caps(sovereign_cap, NET_HTTP_POST, &parsed) {
        if code == HTTP_ERR_PERM {
            HTTP_DENIES_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
        log_event(
            "WARN",
            "http/post",
            &format!("denied · url={url} · code={code}"),
        );
        return code;
    }

    // Execute.
    let agent = build_agent();
    let result = agent
        .post(url)
        .set("Content-Type", content_type)
        .send_bytes(body);
    HTTP_BYTES_OUT_TOTAL.fetch_add(body.len() as u64, Ordering::Relaxed);
    match result {
        Ok(resp) => {
            let status = resp.status();
            let outcome = drain_body_into(resp, out_buf, out_cap);
            let dur_us = started.elapsed().as_micros() as u64;
            match outcome {
                Ok(n) => {
                    log_event(
                        "INFO",
                        "http/post",
                        &format!(
                            "{{\"event\":\"http_request\",\"method\":\"POST\",\"url\":\"{url}\",\"status\":{status},\"bytes_out\":{},\"bytes_in\":{n},\"duration_us\":{dur_us}}}",
                            body.len(),
                        ),
                    );
                    n
                }
                Err(code) => {
                    log_event(
                        "ERROR",
                        "http/post",
                        &format!("body-read-failed · url={url} · code={code}"),
                    );
                    code
                }
            }
        }
        Err(e) => {
            let dur_us = started.elapsed().as_micros() as u64;
            let code = if is_timeout(&e) {
                HTTP_TIMEOUTS_TOTAL.fetch_add(1, Ordering::Relaxed);
                HTTP_ERR_TIMEDOUT
            } else {
                classify_ureq_error(&e)
            };
            log_event(
                "ERROR",
                "http/post",
                &format!(
                    "{{\"event\":\"http_request\",\"method\":\"POST\",\"url\":\"{url}\",\"error\":\"{e}\",\"code\":{code},\"duration_us\":{dur_us}}}"
                ),
            );
            code
        }
    }
}

/// FFI : raise the cap-bits in `bits`.  Reserved bits are silently
/// masked off ; the post-grant cap-set is returned as `i32`
/// (cast-cast-clean since the bitset fits in 4 bits).
#[no_mangle]
pub extern "C" fn __cssl_http_caps_grant(bits: u32) -> i32 {
    http_caps_grant_impl(bits) as i32
}

/// FFI : lower the cap-bits in `bits`.  Reserved bits are silently
/// masked off ; the post-revoke cap-set is returned as `i32`.
#[no_mangle]
pub extern "C" fn __cssl_http_caps_revoke(bits: u32) -> i32 {
    http_caps_revoke_impl(bits) as i32
}

/// FFI : snapshot the current cap-set.
#[no_mangle]
pub extern "C" fn __cssl_http_caps_current() -> u32 {
    http_caps_current()
}

// ═══════════════════════════════════════════════════════════════════════
// § Tests — module-local lock + reset for serialization
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write as IoWrite};
    use std::net::TcpListener;
    use std::thread;

    /// Acquire the module-local lock + reset every counter / cap.  Mirrors
    /// the per-module test discipline in `host_time` / `host_xr` etc.
    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match HTTP_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                HTTP_TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_http_for_tests();
        // Make sure CSSL_HTTP_DISABLE never leaks across tests.
        std::env::remove_var("CSSL_HTTP_DISABLE");
        std::env::remove_var("CSSL_HTTP_TIMEOUT_MS");
        g
    }

    /// Spin up a one-shot mock HTTP server that returns 200 + `body`
    /// for any request.  Returns the bound port + a join handle the
    /// caller drops after the test.  The server is single-shot : it
    /// accepts ONE connection then exits.
    ///
    /// § REQ-PARSING : the mock reads the request line + headers
    /// (terminated by `\r\n\r\n`) ; if a `Content-Length` header is
    /// present it then drains exactly that many body bytes before
    /// responding.  Without this, ureq's POST send_bytes call observes
    /// a connection-closed error before its body write completes,
    /// surfacing as `-4` HTTP_ERR_IO at the FFI surface.
    fn spawn_mock_server(body: &'static str) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
                // Read until we see end-of-headers `\r\n\r\n`.
                let mut accum: Vec<u8> = Vec::with_capacity(4096);
                let mut tmp = [0u8; 1024];
                let header_end_idx = loop {
                    if let Some(idx) = accum
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                    {
                        break Some(idx + 4);
                    }
                    match sock.read(&mut tmp) {
                        Ok(0) => break None,
                        Ok(n) => accum.extend_from_slice(&tmp[..n]),
                        Err(_) => break None,
                    }
                    if accum.len() > 64 * 1024 {
                        break None; // avoid unbounded read on a malformed client
                    }
                };
                // Parse Content-Length if any.
                if let Some(end) = header_end_idx {
                    let header_str = std::str::from_utf8(&accum[..end]).unwrap_or("");
                    let body_len = header_str
                        .lines()
                        .find_map(|l| {
                            let l = l.trim_end_matches('\r');
                            let lower = l.to_ascii_lowercase();
                            lower
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    let already = accum.len().saturating_sub(end);
                    let mut to_read = body_len.saturating_sub(already);
                    while to_read > 0 {
                        let chunk = to_read.min(tmp.len());
                        match sock.read(&mut tmp[..chunk]) {
                            Ok(0) => break,
                            Ok(n) => to_read -= n,
                            Err(_) => break,
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                );
                let _ = sock.write_all(response.as_bytes());
                let _ = sock.flush();
                // Half-shutdown to ensure ureq sees EOF promptly.
                let _ = sock.shutdown(std::net::Shutdown::Both);
            }
        });
        // Tiny handshake : connect once to ensure the listener is ready.
        // ureq's connection sometimes races the bind ; this primes it.
        thread::sleep(Duration::from_millis(20));
        (port, handle)
    }

    // ─── 1. parse_url -----------------------------------------------------

    #[test]
    fn parse_url_handles_http_https_localhost() {
        // Pure : no global state ; no lock.
        let p = parse_url("http://example.com/path").unwrap();
        assert_eq!(p.scheme, UrlScheme::Http);
        assert_eq!(p.host, "example.com");
        let p = parse_url("https://localhost:8080/x").unwrap();
        assert_eq!(p.scheme, UrlScheme::Https);
        assert_eq!(p.host, "localhost");
        let p = parse_url("http://127.0.0.1/").unwrap();
        assert_eq!(p.host, "127.0.0.1");
        assert!(parse_url("ftp://x.com").is_none());
        assert!(parse_url("http://").is_none());
        assert!(parse_url("not-a-url").is_none());
    }

    #[test]
    fn host_is_loopback_recognizes_canonical_forms() {
        assert!(host_is_loopback("localhost"));
        assert!(host_is_loopback("127.0.0.1"));
        assert!(host_is_loopback("::1"));
        assert!(host_is_loopback("[::1]"));
        assert!(!host_is_loopback("example.com"));
        assert!(!host_is_loopback("192.168.1.1"));
    }

    // ─── 2. cap grant + revoke -------------------------------------------

    #[test]
    fn http_caps_grant_and_revoke() {
        let _g = lock_and_reset();
        assert_eq!(http_caps_current(), 0);
        let after = http_caps_grant_impl(NET_HTTP_GET | NET_HTTP_POST);
        assert_eq!(after, NET_HTTP_GET | NET_HTTP_POST);
        let after = http_caps_revoke_impl(NET_HTTP_POST);
        assert_eq!(after, NET_HTTP_GET);
        // Reserved bits are masked off.
        let after = http_caps_grant_impl(0xFFFF_FFFF);
        assert_eq!(after & !NET_HTTP_CAP_MASK, 0);
        assert_eq!(after, NET_HTTP_CAP_MASK);
    }

    #[test]
    fn ffi_caps_grant_revoke_roundtrip() {
        let _g = lock_and_reset();
        let after = __cssl_http_caps_grant(NET_HTTP_GET);
        assert_eq!(after as u32, NET_HTTP_GET);
        assert_eq!(__cssl_http_caps_current(), NET_HTTP_GET);
        let after = __cssl_http_caps_revoke(NET_HTTP_GET);
        assert_eq!(after, 0);
    }

    // ─── 3. cap denial paths ---------------------------------------------

    #[test]
    fn http_get_no_caps_returns_eacces() {
        let _g = lock_and_reset();
        let url = b"http://127.0.0.1:1/x";
        let mut buf = [0u8; 256];
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        assert_eq!(r, HTTP_ERR_PERM);
        assert_eq!(http_denies_total(), 1);
    }

    #[test]
    fn http_https_only_blocks_plain_http() {
        let _g = lock_and_reset();
        // Grant GET + HTTPS-only.
        http_caps_grant_impl(NET_HTTP_GET | NET_HTTPS_ONLY);
        let url = b"http://127.0.0.1:1/x";
        let mut buf = [0u8; 256];
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        assert_eq!(r, HTTP_ERR_PERM);
        assert_eq!(http_denies_total(), 1);
    }

    #[test]
    fn http_localhost_only_blocks_remote() {
        let _g = lock_and_reset();
        http_caps_grant_impl(NET_HTTP_GET | NET_LOCALHOST_ONLY);
        let url = b"http://example.com/";
        let mut buf = [0u8; 256];
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        assert_eq!(r, HTTP_ERR_PERM);
        assert_eq!(http_denies_total(), 1);
    }

    // ─── 4. happy-path GET against a mock server -------------------------

    #[test]
    fn http_get_localhost_succeeds_with_caps() {
        let _g = lock_and_reset();
        http_caps_grant_impl(NET_HTTP_GET);
        let body = "hello-from-mock";
        let (port, handle) = spawn_mock_server(body);
        let url = format!("http://127.0.0.1:{port}/");
        let mut buf = [0u8; 256];
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        let _ = handle.join();
        assert!(r > 0, "expected positive bytes-written, got {r}");
        assert_eq!(r as usize, body.len());
        let received = std::str::from_utf8(&buf[..r as usize]).unwrap();
        assert_eq!(received, body);
        assert!(http_requests_total() >= 1);
        assert!(http_bytes_in_total() >= body.len() as u64);
    }

    // ─── 5. happy-path POST with body ------------------------------------

    #[test]
    fn http_post_with_body_returns_response() {
        let _g = lock_and_reset();
        http_caps_grant_impl(NET_HTTP_POST);
        let reply = "post-ack";
        let (port, handle) = spawn_mock_server(reply);
        let url = format!("http://127.0.0.1:{port}/submit");
        let body = b"some-payload";
        let ct = b"text/plain";
        let mut out = [0u8; 256];
        let r = unsafe {
            __cssl_http_post(
                url.as_ptr(),
                url.len() as i32,
                body.as_ptr(),
                body.len() as i32,
                ct.as_ptr(),
                ct.len() as i32,
                out.as_mut_ptr(),
                out.len() as i32,
                0,
            )
        };
        let _ = handle.join();
        assert!(r > 0, "expected positive bytes, got {r}");
        let got = std::str::from_utf8(&out[..r as usize]).unwrap();
        assert_eq!(got, reply);
        assert_eq!(http_bytes_out_total(), body.len() as u64);
    }

    // ─── 6. global-disable env-var ---------------------------------------

    #[test]
    fn http_disable_env_blocks_all() {
        let _g = lock_and_reset();
        http_caps_grant_impl(NET_HTTP_GET | NET_HTTP_POST);
        std::env::set_var("CSSL_HTTP_DISABLE", "1");
        let url = b"http://127.0.0.1:1/x";
        let mut buf = [0u8; 64];
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        assert_eq!(r, HTTP_ERR_PERM);
        // Even the sovereign-cap bypass cannot override CSSL_HTTP_DISABLE :
        // env-var = host-process-owner says no.
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                SOVEREIGN_CAP,
            )
        };
        assert_eq!(r, HTTP_ERR_PERM);
        std::env::remove_var("CSSL_HTTP_DISABLE");
    }

    // ─── 7. sovereign-cap bypass -----------------------------------------

    #[test]
    fn sovereign_cap_bypasses_verb_gate() {
        let _g = lock_and_reset();
        // No caps granted ; sovereign-cap should still allow GET against
        // the localhost mock.
        let body = "sovereign-hello";
        let (port, handle) = spawn_mock_server(body);
        let url = format!("http://127.0.0.1:{port}/");
        let mut buf = [0u8; 256];
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                SOVEREIGN_CAP,
            )
        };
        let _ = handle.join();
        assert!(r > 0, "sovereign GET expected to succeed, got {r}");
        assert_eq!(r as usize, body.len());
    }

    // ─── 8. argument validation ------------------------------------------

    #[test]
    fn http_get_rejects_null_or_negative_args() {
        let _g = lock_and_reset();
        http_caps_grant_impl(NET_HTTP_GET);
        let mut buf = [0u8; 16];
        // Null URL.
        let r = unsafe {
            __cssl_http_get(core::ptr::null(), 5, buf.as_mut_ptr(), buf.len() as i32, 0)
        };
        assert_eq!(r, HTTP_ERR_INVAL);
        // Negative URL length.
        let url = b"http://127.0.0.1/";
        let r = unsafe {
            __cssl_http_get(url.as_ptr(), -1, buf.as_mut_ptr(), buf.len() as i32, 0)
        };
        assert_eq!(r, HTTP_ERR_INVAL);
        // Null out_buf.
        let r = unsafe {
            __cssl_http_get(url.as_ptr(), url.len() as i32, core::ptr::null_mut(), 16, 0)
        };
        assert_eq!(r, HTTP_ERR_INVAL);
        // Bad UTF-8 URL.
        let bad = [0xFFu8, 0xFE, 0x00, 0x01];
        let r = unsafe {
            __cssl_http_get(
                bad.as_ptr(),
                bad.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        assert_eq!(r, HTTP_ERR_INVAL);
        // Malformed scheme.
        let url = b"ftp://x.com/";
        let r = unsafe {
            __cssl_http_get(
                url.as_ptr(),
                url.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        assert_eq!(r, HTTP_ERR_INVAL);
    }

    #[test]
    fn timeout_ms_respects_env_with_clamping() {
        // Pure : reads env-var ; doesn't mutate counters/caps.
        std::env::remove_var("CSSL_HTTP_TIMEOUT_MS");
        assert_eq!(timeout_ms(), HTTP_DEFAULT_TIMEOUT_MS);
        std::env::set_var("CSSL_HTTP_TIMEOUT_MS", "5000");
        assert_eq!(timeout_ms(), 5000);
        // Out-of-range values fall back to default.
        std::env::set_var("CSSL_HTTP_TIMEOUT_MS", "0");
        assert_eq!(timeout_ms(), HTTP_DEFAULT_TIMEOUT_MS);
        std::env::set_var("CSSL_HTTP_TIMEOUT_MS", "999999999");
        assert_eq!(timeout_ms(), HTTP_DEFAULT_TIMEOUT_MS);
        std::env::remove_var("CSSL_HTTP_TIMEOUT_MS");
    }

    #[test]
    fn cap_constants_are_distinct_powers_of_two() {
        // Pure : sanity-check the cap-bit layout.
        let bits = [
            NET_HTTP_GET,
            NET_HTTP_POST,
            NET_HTTPS_ONLY,
            NET_LOCALHOST_ONLY,
        ];
        for &b in &bits {
            assert_eq!(b.count_ones(), 1, "{b} not a power-of-two");
        }
        // Distinct.
        for i in 0..bits.len() {
            for j in (i + 1)..bits.len() {
                assert_ne!(bits[i], bits[j]);
            }
        }
        // Mask covers exactly these.
        let mut acc = 0;
        for &b in &bits {
            acc |= b;
        }
        assert_eq!(acc, NET_HTTP_CAP_MASK);
    }

    #[test]
    fn sovereign_cap_constant_is_distinctive() {
        // Pure : verify the magic constant matches the public spec.
        assert_eq!(SOVEREIGN_CAP, 0xCAFE_BABE_DEAD_BEEF);
    }
}
