//! § cssl-host-mycelium-heartbeat — bi-directional LOCAL↔CLOUD federation
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W14-MYCELIUM-HEARTBEAT : generalize the chat-pattern federation
//! pipe into a uniform `FederationPattern` carrier for ANY cell-state,
//! KAN-bias, or content-discovery signal traveling through the mycelial
//! mesh. Built on top of cssl-mycelium-chat-sync's 32-byte bit-pack
//! invariants ; tightened k-anonymity floor (k ≥ 10) for the broader
//! federation surface.
//!
//! § ARCHITECTURE
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────┐
//! │ LOCAL side  (this crate)                                           │
//! │                                                                    │
//! │  cell-tick / KAN-step / content-publish                            │
//! │             │                                                      │
//! │             ▼                                                      │
//! │   ┌─────────────────┐    Σ-mask-gate-1    ┌────────────────────┐   │
//! │   │ HeartbeatRing   │ ──────────────────► │ HeartbeatService   │   │
//! │   │  (in-memory)    │                     │   ::tick(now_unix) │   │
//! │   └─────────────────┘                     └────┬───────────────┘   │
//! │                                                │                   │
//! │                                                ▼                   │
//! │                                       ┌────────────────────┐       │
//! │                                       │ FederationBundle   │       │
//! │                                       │  (zstd-compressed) │       │
//! │                                       └────────┬───────────┘       │
//! │                                                │                   │
//! │                       cloud-up ?               │ POST /heartbeat   │
//! │                  ┌──── YES ──────┘             │                   │
//! │                  ▼                             ▼                   │
//! │             ┌─────────┐               ┌─────────────────────┐      │
//! │             │ enqueue │               │ BackpressureQueue   │      │
//! │             │ +drain  │               │  (bounded · drop-   │      │
//! │             └─────────┘               │   oldest on full)   │      │
//! │                                       └─────────────────────┘      │
//! └────────────────────────────────────────────────────────────────────┘
//!                                  │
//!                            heartbeat-protocol (HTTPS)
//!                                  │
//! ┌────────────────────────────────┼───────────────────────────────────┐
//! │ CLOUD side  (cssl-edge endpts) │                                   │
//! │                                ▼                                   │
//! │   POST /api/mycelium/heartbeat   ←  ingest-side Σ-mask-gate-2      │
//! │   GET  /api/mycelium/digest      ←  k-anon ≥ 10 enforced           │
//! │                                                                    │
//! └────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! § SOVEREIGNTY GUARANTEES
//!   1. ¬ raw cell-content EVER leaves the local machine. `FederationPattern`
//!      is a 32-byte fixed-shape blob (kind · payload-hash · sigma-mask · ts ·
//!      k-anon-cohort-size · sig).
//!   2. Σ-mask-gates appear at THREE points : (a) emit-side at ring-push, (b)
//!      bundle-side at tick (best-effort gate before broadcast), (c) cloud-
//!      ingest at endpoint (defense-in-depth · third gate).
//!   3. k-anonymity floor (k ≥ 10) : a kind+payload-hash pair is invisible to
//!      `digest` until ≥ 10 distinct emitter_handles have contributed. Below
//!      the floor, the pattern sits in cloud-side staging — never readable.
//!   4. Sovereign-revoke cascades : `revoke_emitter` zeroes local cap + emits
//!      a `PurgeRequest` ; cloud applies the purge to all federation rows ;
//!      peers see the new digest on next pull and drop their copies too.
//!   5. Σ-Chain-anchor : every emitted bundle gets a BLAKE3 anchor ; immutable
//!      attribution survives revoke (the anchor remains, the pattern doesn't).
//!   6. Determinism : `bundle_blake3` is replay-stable per-tick ; replay-safe
//!      auditing.
//!   7. Backpressure : if cloud is down, the local queue absorbs new bundles
//!      up to a bounded capacity ; on reconnect, drain happens FIFO.
//!
//! § PRIME-DIRECTIVE
//!   `#![forbid(unsafe_code)]`. ¬ surveillance. ¬ coercion. ¬ profiling-
//!   individual-players. Patterns federate ONLY in aggregate above k=10.
//!
//! § PARENT spec : `Labyrinth of Apocalypse/systems/mycelium_heartbeat.csl`
//!
//! § INTEGRATION
//!   ─ Driven by `cssl-host-persistent-orchestrator` (W14-J sibling) — that
//!     crate's 24/7 daemon calls `HeartbeatService::tick` every 60s.
//!   ─ Cloud-side is `cssl-edge/pages/api/mycelium/heartbeat.ts` +
//!     `digest.ts` ; SQL schema in `cssl-supabase/migrations/0035_*.sql`.
//!   ─ The W14-K cron-job sibling polls cloud-digest periodically to push
//!     federated bias-deltas back into local KAN reservoirs.
//!   ─ The W14-M status-page sibling reads `HeartbeatStats` for the live
//!     federation-health dashboard.

#![forbid(unsafe_code)]
#![doc(html_no_source)]

pub mod backpressure;
pub mod bundle;
pub mod compress;
pub mod pattern;
pub mod ring;
pub mod service;

// ─── re-exports for `use cssl_host_mycelium_heartbeat::*` ergonomics ───────

pub use backpressure::{BackpressureQueue, DEFAULT_QUEUE_CAPACITY};
pub use bundle::{FederationBundle, BundleError, BundleStats};
pub use compress::{compress_bundle, decompress_bundle, CompressError};
pub use pattern::{
    FederationKind, FederationPattern, FederationPatternBuilder, PatternError,
    CAP_FED_EMIT_ALLOWED, CAP_FED_INGEST, CAP_FED_PURGE_ON_REVOKE, CAP_FED_REPLAY_DETERMINISTIC,
    CAP_FED_FLAGS_ALL, CAP_FED_FLAGS_RESERVED_MASK, FEDERATION_PATTERN_SIZE,
};
pub use ring::{HeartbeatRing, DEFAULT_RING_CAPACITY};
pub use service::{
    CloudHealth, FederationCapPolicy, HeartbeatService, HeartbeatServiceBuilder, HeartbeatStats,
    PurgeRequest, DEFAULT_HEARTBEAT_PERIOD_SECS,
};

/// Crate-version stamp ; surfaced in audit lines + observability.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § `PROTOCOL_VERSION` — wire-format-version of `FederationBundle`.
/// Bumped only when the bit-pack layout changes. Currently 1.
pub const PROTOCOL_VERSION: u32 = 1;

/// § `K_ANONYMITY_FLOOR` — k=10 (tighter than chat-sync's k=5 because the
/// generalized federation surface has broader de-anonymization risk).
pub const K_ANONYMITY_FLOOR: u32 = 10;

/// § `BANDWIDTH_TARGET_BYTES_PER_MIN` — soft target ; observability metric.
/// 1KB/min/peer → ≈ 12-15 bundle records typical. zstd-dict typically
/// achieves 3-4× compression on shared-vocabulary federation patterns.
pub const BANDWIDTH_TARGET_BYTES_PER_MIN: u32 = 1_024;
