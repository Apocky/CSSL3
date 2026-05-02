//! § cssl-mycelium-chat-sync — federate chat-pattern shapes across peers.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W11-MYCELIUM-CHAT-SYNC : the GM/DM persona-agent learns SHAPE-of-
//! good-response from federated peers without ever observing raw player
//! content. Σ-mask-gated · k-anonymous · sovereign-revoke.
//!
//! § ARCHITECTURE
//!   ┌──────────────────┐    observe (per-line)
//!   │  chat-input      │ ─────────────────────►  ┌──────────────────┐
//!   │  (loa-host)      │                         │ ChatPatternRing  │
//!   └──────────────────┘                         │  (lock-free)     │
//!                                                └────────┬─────────┘
//!                                                         │ drain @ tick
//!                                                         ▼
//!   ┌──────────────────┐    Σ-mask-gate-1     ┌──────────────────────┐
//!   │  CapPolicy       │ ─────────────────►   │  MyceliumChatSync    │
//!   │  (per-emitter)   │                      │  ::tick(now)         │
//!   └──────────────────┘                      └─────┬───────────┬────┘
//!                                                   │           │
//!                                                   │ broadcast │ ingest peer
//!                                                   ▼           ▼
//!                                       ┌──────────────────┐  ┌──────────────┐
//!                                       │ BroadcastSink    │  │ Federation   │
//!                                       │ (transport)      │  │ (k-anon ≥ 5) │
//!                                       └──────────────────┘  └──────┬───────┘
//!                                                                    │
//!                                                                    │ snapshot_public
//!                                                                    ▼
//!                                                         ┌──────────────────┐
//!                                                         │ GM/DM persona    │
//!                                                         │  modulator       │
//!                                                         └──────────────────┘
//!
//! § SOVEREIGNTY GUARANTEES
//!   1. ¬ raw chat-text EVER leaves the local machine. ChatPattern is a
//!      32-byte fixed-shape blob. Pattern_id is content-addressable over
//!      (intent_kind · response_shape · arc_phase) ; emitter_handle is a
//!      non-recoverable BLAKE3-trunc of pubkey.
//!   2. Σ-mask-gates appear at TWO points : (a) emit-side in
//!      `MyceliumChatSync::tick`, before broadcast ; (b) ingest-side in
//!      `ChatPatternFederation::ingest`. Defense-in-depth.
//!   3. k-anonymity floor (default k=5) : a pattern_id is invisible to
//!      `snapshot_public` until ≥ k distinct emitter_handles have
//!      contributed. Below the floor, patterns sit in staging — never
//!      reachable from public-API.
//!   4. Sovereign-revoke : `revoke_emitter` zeroes the cap-policy + purges
//!      the local ring + purges the local federation + broadcasts a
//!      `PurgeRequest` so peers drop their copies.
//!   5. Determinism : `federation_blake3` is replay-stable per-snapshot ; a
//!      GM/DM persona seeded with `(persona_seed, federation_blake3)`
//!      yields identical modulation across replays.
//!
//! § PRIME-DIRECTIVE
//!   `#![forbid(unsafe_code)]`. ¬ surveillance. ¬ coercion. ¬ profiling-
//!   individual-players. Patterns federate ONLY in aggregate above k.
//!
//! § PARENT spec : `Labyrinth of Apocalypse/systems/mycelium_chat.csl`
//!
//! § INTEGRATION
//!   ─ Consumed by `cssl-host-mycelium-desktop` (Mycelium Desktop wires
//!     the service into its event loop ; see app.rs in that crate).
//!   ─ FFI surface in `ffi.rs` is the canonical extern-C contract that
//!     `loa-host` registers ; see also the `mycelium_chat.csl` spec.
//!   ─ Transport-side : `BroadcastSink` is the impl-trait point ; the
//!     mycelium-desktop integration can wire a bridge into
//!     `cssl-host-mycelium`'s `TransportAdapter` for cross-instance fan-out.

#![forbid(unsafe_code)]
#![doc(html_no_source)]

pub mod federation;
pub mod ffi;
pub mod pattern;
pub mod ring;
pub mod sync;

// ─── re-exports for `use cssl_mycelium_chat_sync::*` ergonomics ────────────

pub use federation::{
    AggregatedShape, ChatPatternFederation, FederationError, FederationStats, DEFAULT_K_FLOOR,
};
pub use pattern::{
    ArcPhase, ChatPattern, ChatPatternBuilder, IntentKind, PatternError, ResponseShape,
    CAP_EMIT_ALLOWED, CAP_FEDERATION_INGEST, CAP_FLAGS_ALL, CAP_FLAGS_RESERVED_MASK,
    CAP_PURGE_ON_REVOKE, CAP_REPLAY_DETERMINISTIC,
};
pub use ring::{ChatPatternRing, DEFAULT_CAPACITY};
pub use sync::{
    BroadcastSink, CapPolicy, ChatPatternDifferential, ChatSyncStats, InMemoryBroadcastSink,
    MyceliumChatSync, MyceliumChatSyncBuilder, NullBroadcastSink, PurgeRequest,
    DEFAULT_TICK_PERIOD_SECS,
};

/// Crate-version stamp ; surfaced in audit lines + observability.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § `PROTOCOL_VERSION` — wire-format-version of `ChatPatternDifferential`.
/// Bumped only when the bit-pack layout changes. Currently 1.
pub const PROTOCOL_VERSION: u32 = 1;

/// § `K_ANONYMITY_FLOOR` — back-compat alias for `DEFAULT_K_FLOOR`. Some
/// stage-0 callers reference this name ; preserved here for transition.
pub const K_ANONYMITY_FLOOR: u32 = DEFAULT_K_FLOOR as u32;
