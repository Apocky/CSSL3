//! § cssl-host-mp-transport-real — REAL ureq-backed Supabase `MpTransport`
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   `RealSupabaseTransport` is the production-grade replacement for
//!   `StubSupabaseTransport` (see `cssl-host-mp-transport::stub_supabase`).
//!   It speaks the Supabase REST dialect against the `signaling_messages`
//!   table defined by W4 migration `0004_signaling.sql` :
//!
//!   ```text
//!   POST  {base_url}/rest/v1/signaling_messages         -> send
//!   GET   {base_url}/rest/v1/signaling_messages?...     -> poll
//!   ```
//!
//!   Both calls carry the `apikey` + `Authorization: Bearer ...` header
//!   pair per the [PostgREST + Supabase auth flow][1] ; RLS policies in
//!   `0005_signaling_rls.sql` gate row visibility per-peer.
//!
//!   [1]: https://supabase.com/docs/guides/api/rest/auth
//!
//! § PRIME-DIRECTIVE STRUCTURAL SAFEGUARDS
//!   1. CAP-GATED — `caps & TRANSPORT_CAP_SEND` is checked before every
//!      `send` ; same for `_RECV` + `poll`. Default-deny.
//!   2. AUDIT-EMIT — every operation produces an `AuditEvent` via the
//!      injected `AuditSink`. Audit failures NEVER crash the transport :
//!      the sink trait returns `Result<(), AuditErr>` and the transport
//!      ignores it. Best-effort observability.
//!   3. SOVEREIGN-BYPASS-RECORDED — when the `SovereignBypassRecorder` is
//!      enabled, every send emits an `mp.sovereign.bypass` event BEFORE
//!      the actual send so the bypass is loud + auditable.
//!   4. BACKOFF-RESPECTING — Supabase 429s are parsed with Retry-After
//!      and surfaced as `TransportErr::Backoff(ms)`. Servers ≥ 500 surface
//!      as `ServerErr` ; ureq timeouts as `Timeout`.
//!   5. NO `.unwrap()` IN PROD — all `Result`s are matched explicitly. The
//!      one `.unwrap()` allowed is in `#[cfg(test)]` modules.
//!
//! § TEST APPROACH
//!   The HTTP layer is abstracted behind an `HttpClient` trait. Production
//!   uses `UreqClient` (real HTTP). Tests use `MockHttpClient` which
//!   records calls + returns canned responses — no network I/O, no flaky
//!   timing, deterministic. This is the cheapest object-level mock that
//!   lets the `RealSupabaseTransport` pipeline get tested end-to-end.
//!
//! § FFI / ABI
//!   None — this crate is a pure-Rust library consumed by `loa-host` /
//!   `cssl-host-multiplayer-signaling` integrators. Re-exports the trait
//!   from `cssl-host-mp-transport` for caller convenience.

#![forbid(unsafe_code)]

pub mod backoff;
pub mod config;
pub mod http_client;
pub mod sovereign_bypass;
pub mod transport;

// ─── re-exports ────────────────────────────────────────────────────────────
//
// Callers should be able to `use cssl_host_mp_transport_real::*` and get
// the full surface they need without reaching into the source crate.

pub use crate::backoff::{exponential_backoff, parse_retry_after};
pub use crate::config::{
    AuditErr, AuditEvent, AuditSink, NoopAuditSink, RecordingAuditSink, SupabaseConfig,
};
pub use crate::http_client::{
    HttpClient, HttpReq, HttpResp, HttpTransportErr, MockHttpClient, UreqClient,
};
pub use crate::sovereign_bypass::SovereignBypassRecorder;
pub use crate::transport::RealSupabaseTransport;

// Re-export the trait + cap-bits + result/error types from the source
// crate so a single `use cssl_host_mp_transport_real::*` is enough to wire
// a transport end-to-end without a second `use` of the trait crate.
pub use cssl_host_mp_transport::{
    MpTransport, TransportErr, TransportResult, TRANSPORT_CAP_BOTH, TRANSPORT_CAP_RECV,
    TRANSPORT_CAP_SEND,
};
