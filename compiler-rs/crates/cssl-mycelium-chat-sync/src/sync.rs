//! § sync — `MyceliumChatSync` service · digest-loop · broadcast · ingest
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   The service is the orchestrator. It owns :
//!     ─ the local `ChatPatternRing`              (producer)
//!     ─ the local `ChatPatternFederation`        (receiver-of-peer-broadcasts)
//!     ─ the cross-user `TransportAdapter`        (mycelium pipe)
//!     ─ the per-emitter Σ-mask cap-policy        (default-deny gate)
//!
//!   Every digest-tick (default 60s) :
//!     1. Drain the local ring → vector of `ChatPattern`s
//!     2. Σ-mask gate at emit : drop any pattern lacking `CAP_EMIT_ALLOWED`
//!     3. Wrap surviving patterns into a `ChatPatternDifferential` blob
//!     4. Broadcast via `TransportAdapter::emit` (best-effort)
//!     5. Ingest received-from-peers patterns into the federation
//!
//! § RUN-LOOP DRIVING
//!   We avoid a hard tokio dep here ; the host crate decides how to schedule.
//!   `tick()` is the single-step api ; `run_loop` is a blocking convenience
//!   that sleeps `tick_period` between ticks. Hosts using tokio can wrap
//!   `tick()` in their own scheduler.
//!
//! § REVOKE FLOW
//!   `revoke_emitter()` :
//!     1. Drop the cap-policy → outgoing-zero on subsequent ticks
//!     2. Purge the local-ring (defense-in-depth)
//!     3. Locally-purge the federation for that emitter-handle
//!     4. Emit a `Spore::PurgeRequest` to peers (cap-gated by spec)
//!
//! § DETERMINISM
//!   `tick()` is deterministic given : input ring-state · cap-policy ·
//!   federation-state · transport queue. Ts-bucketing is the only time-
//!   coupled element ; given a fixed ts-bucket, replay produces identical
//!   federation-blake3.

use crate::federation::ChatPatternFederation;
use crate::pattern::{ChatPattern, CAP_EMIT_ALLOWED};
use crate::ring::ChatPatternRing;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

/// § DEFAULT_TICK_PERIOD_SECS — once per minute.
pub const DEFAULT_TICK_PERIOD_SECS: u64 = 60;

/// § ChatPatternDifferential — the wire-blob a digest-tick broadcasts.
///
/// "Differential" because each tick emits ONLY the patterns observed since
/// the previous tick — not the cumulative ring-history. Bandwidth = O(k)
/// rather than O(N).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPatternDifferential {
    /// Patterns emitted in this tick (post-Σ-mask filtering).
    pub patterns: Vec<ChatPattern>,
    /// Tick number ; monotonic per-service-instance.
    pub tick_id: u64,
    /// Emitter pubkey-trunc (8-byte) — same as ChatPattern.emitter_handle.
    pub emitter_handle: u64,
    /// Wall-clock ts of the tick (epoch seconds, post-bucketing).
    pub ts_bucketed: u32,
}

/// § PurgeRequest — broadcast at `revoke_emitter` time so peers can purge
/// their copies of patterns from this emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeRequest {
    pub emitter_handle: u64,
    /// Wall-clock ts of the revoke event ; informational, not authoritative.
    pub ts_unix: u64,
}

/// § ChatSyncStats — observability snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSyncStats {
    pub ticks_total: u64,
    pub patterns_emitted: u64,
    pub patterns_dropped_by_cap: u64,
    pub patterns_received: u64,
    pub revokes_processed: u64,
    pub purges_received: u64,
}

/// § CapPolicy — Σ-mask state for the local emitter set.
///
/// Multi-tenant : a single MyceliumChatSync may host multiple emitter-keys
/// (e.g. a household-shared install). The policy gates emit per-emitter.
#[derive(Default, Debug, Clone)]
pub struct CapPolicy {
    /// Emitter-handle ↦ Σ-mask bits the emitter holds. Default-deny : any
    /// emitter not in the map has cap_flags = 0 ⟶ all emit-checks fail.
    grants: BTreeMap<u64, u8>,
    /// Required bits ALL emitters must hold to be allowed to broadcast.
    required: u8,
}

impl CapPolicy {
    #[must_use]
    pub const fn new(required: u8) -> Self {
        Self {
            grants: BTreeMap::new(),
            required,
        }
    }

    /// § grant — set the Σ-mask flags for an emitter.
    pub fn grant(&mut self, emitter_handle: u64, flags: u8) {
        self.grants.insert(emitter_handle, flags);
    }

    /// § revoke — zero the flags for an emitter (idempotent).
    pub fn revoke(&mut self, emitter_handle: u64) {
        self.grants.remove(&emitter_handle);
    }

    /// § is_allowed — consults the grant + required mask.
    #[must_use]
    pub fn is_allowed(&self, emitter_handle: u64) -> bool {
        let flags = self.grants.get(&emitter_handle).copied().unwrap_or(0);
        (flags & self.required) == self.required
    }

    #[must_use]
    pub const fn required(&self) -> u8 {
        self.required
    }

    pub fn set_required(&mut self, required: u8) {
        self.required = required;
    }

    #[must_use]
    pub fn known_emitters(&self) -> Vec<u64> {
        self.grants.keys().copied().collect()
    }
}

/// § BroadcastSink — minimal trait the host implements to fan a differential
/// out to mycelium-peers. Defaults to a no-op for stage-0 / unit-tests.
pub trait BroadcastSink: Send + Sync {
    /// § broadcast — emit a differential to peers. Best-effort ; failures
    /// MUST NOT propagate as panics. Errors are logged via `tracing` by the
    /// caller.
    fn broadcast(&self, diff: &ChatPatternDifferential);
    /// § broadcast_purge — emit a purge request so peers drop emitter-state.
    fn broadcast_purge(&self, req: PurgeRequest);
}

/// § NullBroadcastSink — discards all output. Default ; unit-tests rely on it.
#[derive(Default, Debug, Clone, Copy)]
pub struct NullBroadcastSink;

impl BroadcastSink for NullBroadcastSink {
    fn broadcast(&self, _diff: &ChatPatternDifferential) {}
    fn broadcast_purge(&self, _req: PurgeRequest) {}
}

/// § InMemoryBroadcastSink — records emitted differentials for testing.
#[derive(Default)]
pub struct InMemoryBroadcastSink {
    pub emitted: Mutex<Vec<ChatPatternDifferential>>,
    pub purges: Mutex<Vec<PurgeRequest>>,
}

impl InMemoryBroadcastSink {
    /// § emitted_snapshot — clone of recorded differentials. Returns empty
    /// `Vec` if the lock is poisoned.
    #[must_use]
    pub fn emitted_snapshot(&self) -> Vec<ChatPatternDifferential> {
        self.emitted.lock().map_or_else(|_| Vec::new(), |g| g.clone())
    }

    /// § purges_snapshot — clone of recorded purge-requests.
    #[must_use]
    pub fn purges_snapshot(&self) -> Vec<PurgeRequest> {
        self.purges.lock().map_or_else(|_| Vec::new(), |g| g.clone())
    }
}

impl BroadcastSink for InMemoryBroadcastSink {
    fn broadcast(&self, diff: &ChatPatternDifferential) {
        if let Ok(mut g) = self.emitted.lock() {
            g.push(diff.clone());
        }
    }
    fn broadcast_purge(&self, req: PurgeRequest) {
        if let Ok(mut g) = self.purges.lock() {
            g.push(req);
        }
    }
}

/// § MyceliumChatSync — the service.
///
/// Construct once at app-bootstrap. Hold an `Arc<MyceliumChatSync>` clone in
/// each surface that needs to push (chat-input handler) or pull (GM/DM
/// persona modulator).
pub struct MyceliumChatSync {
    ring: ChatPatternRing,
    federation: ChatPatternFederation,
    cap_policy: Arc<RwLock<CapPolicy>>,
    sink: Arc<dyn BroadcastSink>,
    stats: Arc<RwLock<ChatSyncStats>>,
    tick_counter: Arc<std::sync::atomic::AtomicU64>,
    tick_period_secs: u64,
}

impl MyceliumChatSync {
    /// § new — default cap-policy (`CAP_EMIT_ALLOWED` required) +
    /// null-broadcast-sink + default ring-cap + default federation k-floor.
    #[must_use]
    pub fn new() -> Self {
        Self::builder().build()
    }

    #[must_use]
    pub fn builder() -> MyceliumChatSyncBuilder {
        MyceliumChatSyncBuilder::default()
    }

    /// § observe — push a `ChatPattern` into the local ring. Called by the
    /// chat-input handler. Σ-mask gate is checked on the digest-tick, not
    /// here — local observation is always allowed (the local ring never
    /// leaves the sovereign boundary).
    pub fn observe(&self, pattern: ChatPattern) {
        self.ring.push(pattern);
    }

    /// § grant_emitter — extend the cap-policy for an emitter. Idempotent.
    pub fn grant_emitter(&self, emitter_handle: u64, flags: u8) {
        if let Ok(mut g) = self.cap_policy.write() {
            g.grant(emitter_handle, flags);
        }
    }

    /// § revoke_emitter — sovereign-revoke flow.
    ///   1. zero cap-policy   (outgoing-zero next-tick)
    ///   2. purge local ring  (defense-in-depth)
    ///   3. purge local federation
    ///   4. broadcast purge-request to peers
    ///   5. bump revoke-stat counter
    pub fn revoke_emitter(&self, emitter_handle: u64, ts_unix: u64) {
        if let Ok(mut g) = self.cap_policy.write() {
            g.revoke(emitter_handle);
        }
        let _ = self.ring.purge();
        self.federation.purge_emitter(emitter_handle);
        let req = PurgeRequest {
            emitter_handle,
            ts_unix,
        };
        self.sink.broadcast_purge(req);
        if let Ok(mut s) = self.stats.write() {
            s.revokes_processed += 1;
        }
    }

    /// § tick — advance the digest-loop one step. Returns the
    /// `ChatPatternDifferential` that was broadcast (or None if all
    /// patterns were Σ-mask-denied).
    ///
    /// `now_unix` is injected so tests + replay can advance time
    /// deterministically.
    pub fn tick(&self, now_unix: u64) -> Option<ChatPatternDifferential> {
        let tick_id = self
            .tick_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let drained = self.ring.drain_all();
        let mut surviving: Vec<ChatPattern> = Vec::with_capacity(drained.len());
        let mut dropped = 0_u64;
        let mut emitter_handle = 0_u64;
        let policy = self.cap_policy.read().ok();
        for p in drained {
            // Σ-mask gate at-emit (defense-in-depth ; ingest-side gates
            // again).
            if !p.cap_check(CAP_EMIT_ALLOWED) {
                dropped += 1;
                continue;
            }
            let h = p.emitter_handle();
            let allowed = policy.as_ref().is_some_and(|g| g.is_allowed(h));
            if !allowed {
                dropped += 1;
                continue;
            }
            emitter_handle = h;
            surviving.push(p);
        }
        drop(policy);

        if let Ok(mut s) = self.stats.write() {
            s.ticks_total += 1;
            s.patterns_dropped_by_cap += dropped;
            s.patterns_emitted += surviving.len() as u64;
        }

        if surviving.is_empty() {
            return None;
        }

        let diff = ChatPatternDifferential {
            patterns: surviving,
            tick_id,
            emitter_handle,
            ts_bucketed: ((now_unix / 60) & 0xFFFF_FFFF) as u32,
        };
        self.sink.broadcast(&diff);
        Some(diff)
    }

    /// § ingest_peer_differential — receive-side : a peer's broadcast
    /// arrived ; push its patterns into the federation. Σ-mask gating runs
    /// inside `ChatPatternFederation::ingest`. Returns the count actually
    /// accepted into the federation.
    pub fn ingest_peer_differential(&self, diff: &ChatPatternDifferential) -> usize {
        let n = self.federation.ingest_batch(&diff.patterns);
        if let Ok(mut s) = self.stats.write() {
            s.patterns_received += diff.patterns.len() as u64;
        }
        n
    }

    /// § handle_purge_request — peer asked us to drop their patterns. Apply
    /// to local federation + bump counter.
    pub fn handle_purge_request(&self, req: &PurgeRequest) {
        self.federation.purge_emitter(req.emitter_handle);
        if let Ok(mut s) = self.stats.write() {
            s.purges_received += 1;
        }
    }

    /// § run_loop — blocking convenience that ticks every `tick_period_secs`.
    ///
    /// `should_stop` is polled each iteration. Hosts that don't want a
    /// dedicated thread should call `tick()` themselves.
    pub fn run_loop<F>(&self, mut should_stop: F, mut now_fn: impl FnMut() -> u64)
    where
        F: FnMut() -> bool,
    {
        while !should_stop() {
            let now = now_fn();
            let _ = self.tick(now);
            std::thread::sleep(std::time::Duration::from_secs(self.tick_period_secs));
        }
    }

    /// § federation — read-handle for GM/DM persona-modulators.
    #[must_use]
    pub fn federation(&self) -> &ChatPatternFederation {
        &self.federation
    }

    /// § ring — read-handle for local-stats / inspection.
    #[must_use]
    pub fn ring(&self) -> &ChatPatternRing {
        &self.ring
    }

    /// § stats — observability snapshot.
    #[must_use]
    pub fn stats(&self) -> ChatSyncStats {
        self.stats.read().map_or_else(|_| ChatSyncStats::default(), |g| *g)
    }

    #[must_use]
    pub const fn tick_period_secs(&self) -> u64 {
        self.tick_period_secs
    }
}

impl Default for MyceliumChatSync {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MyceliumChatSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MyceliumChatSync")
            .field("ring", &self.ring)
            .field("federation", &self.federation)
            .field("stats", &self.stats())
            .field("tick_period_secs", &self.tick_period_secs)
            .finish()
    }
}

// ─── builder ────────────────────────────────────────────────────────────────

pub struct MyceliumChatSyncBuilder {
    ring_capacity: usize,
    k_floor: usize,
    cap_required: u8,
    sink: Arc<dyn BroadcastSink>,
    tick_period_secs: u64,
}

impl Default for MyceliumChatSyncBuilder {
    fn default() -> Self {
        Self {
            ring_capacity: crate::ring::DEFAULT_CAPACITY,
            k_floor: crate::federation::DEFAULT_K_FLOOR,
            cap_required: CAP_EMIT_ALLOWED,
            sink: Arc::new(NullBroadcastSink),
            tick_period_secs: DEFAULT_TICK_PERIOD_SECS,
        }
    }
}

impl MyceliumChatSyncBuilder {
    #[must_use]
    pub fn ring_capacity(mut self, c: usize) -> Self {
        self.ring_capacity = c;
        self
    }

    #[must_use]
    pub fn k_floor(mut self, k: usize) -> Self {
        self.k_floor = k;
        self
    }

    #[must_use]
    pub fn cap_required(mut self, flags: u8) -> Self {
        self.cap_required = flags;
        self
    }

    #[must_use]
    pub fn sink(mut self, sink: Arc<dyn BroadcastSink>) -> Self {
        self.sink = sink;
        self
    }

    #[must_use]
    pub fn tick_period_secs(mut self, secs: u64) -> Self {
        self.tick_period_secs = secs;
        self
    }

    #[must_use]
    pub fn build(self) -> MyceliumChatSync {
        MyceliumChatSync {
            ring: ChatPatternRing::new(self.ring_capacity),
            federation: ChatPatternFederation::with_k_floor(self.k_floor),
            cap_policy: Arc::new(RwLock::new(CapPolicy::new(self.cap_required))),
            sink: self.sink,
            stats: Arc::new(RwLock::new(ChatSyncStats::default())),
            tick_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            tick_period_secs: self.tick_period_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{
        ArcPhase, ChatPatternBuilder, IntentKind, ResponseShape, CAP_FLAGS_ALL,
    };

    fn mk_pattern(emitter_seed: u8, intent: IntentKind, cap_flags: u8) -> ChatPattern {
        ChatPatternBuilder {
            intent_kind: intent,
            response_shape: ResponseShape::ScenicNarrative,
            arc_phase: ArcPhase::RisingAction,
            confidence: 0.7,
            ts_unix: 60 * 100,
            region_tag: 1,
            opt_in_tier: 1,
            cap_flags,
            emitter_pubkey: [emitter_seed; 32],
            co_signers: vec![],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn observe_then_tick_emits_when_granted() {
        let sink = Arc::new(InMemoryBroadcastSink::default());
        let svc = MyceliumChatSync::builder()
            .sink(sink.clone() as Arc<dyn BroadcastSink>)
            .build();
        let p = mk_pattern(1, IntentKind::Question, CAP_FLAGS_ALL);
        svc.grant_emitter(p.emitter_handle(), CAP_FLAGS_ALL);
        svc.observe(p);
        let diff = svc.tick(60 * 100);
        assert!(diff.is_some());
        let recorded = sink.emitted_snapshot();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].patterns.len(), 1);
    }

    #[test]
    fn tick_drops_patterns_without_cap_emit_allowed() {
        let sink = Arc::new(InMemoryBroadcastSink::default());
        let svc = MyceliumChatSync::builder()
            .sink(sink.clone() as Arc<dyn BroadcastSink>)
            .build();
        let p = mk_pattern(1, IntentKind::Question, 0);
        // No cap-flags ⟶ pattern itself is denied at the cap_check call.
        svc.grant_emitter(p.emitter_handle(), CAP_FLAGS_ALL);
        svc.observe(p);
        let diff = svc.tick(60 * 100);
        assert!(diff.is_none(), "denied patterns must not broadcast");
        let recorded = sink.emitted_snapshot();
        assert_eq!(recorded.len(), 0);
        let s = svc.stats();
        assert_eq!(s.patterns_dropped_by_cap, 1);
    }

    #[test]
    fn tick_drops_patterns_when_emitter_not_in_policy() {
        let sink = Arc::new(InMemoryBroadcastSink::default());
        let svc = MyceliumChatSync::builder()
            .sink(sink.clone() as Arc<dyn BroadcastSink>)
            .build();
        let p = mk_pattern(1, IntentKind::Question, CAP_FLAGS_ALL);
        // grant a DIFFERENT emitter
        svc.grant_emitter(999, CAP_FLAGS_ALL);
        svc.observe(p);
        let diff = svc.tick(60 * 100);
        assert!(diff.is_none());
    }

    #[test]
    fn ingest_peer_differential_promotes_to_public_at_k() {
        let svc = MyceliumChatSync::builder().k_floor(3).build();
        // 3 distinct emitters with same intent ⟶ pattern crosses k-floor.
        let diff = ChatPatternDifferential {
            patterns: (1..=3_u8)
                .map(|i| mk_pattern(i, IntentKind::Question, CAP_FLAGS_ALL))
                .collect(),
            tick_id: 1,
            emitter_handle: 1,
            ts_bucketed: 100,
        };
        let n = svc.ingest_peer_differential(&diff);
        assert_eq!(n, 3);
        assert_eq!(svc.federation().snapshot_public().len(), 1);
    }

    #[test]
    fn revoke_emitter_zeroes_outgoing_and_broadcasts_purge() {
        let sink = Arc::new(InMemoryBroadcastSink::default());
        let svc = MyceliumChatSync::builder()
            .sink(sink.clone() as Arc<dyn BroadcastSink>)
            .build();
        let p = mk_pattern(1, IntentKind::Question, CAP_FLAGS_ALL);
        let h = p.emitter_handle();
        svc.grant_emitter(h, CAP_FLAGS_ALL);
        svc.observe(p);
        svc.revoke_emitter(h, 1234);
        let diff = svc.tick(60 * 100);
        // ring was purged AND policy zeroed ⟶ no broadcast
        assert!(diff.is_none());
        let purges = sink.purges_snapshot();
        assert_eq!(purges.len(), 1);
        assert_eq!(purges[0].emitter_handle, h);
    }

    #[test]
    fn revoke_emitter_removes_from_federation() {
        let svc = MyceliumChatSync::builder().k_floor(2).build();
        let p1 = mk_pattern(1, IntentKind::Question, CAP_FLAGS_ALL);
        let p2 = mk_pattern(2, IntentKind::Question, CAP_FLAGS_ALL);
        let h1 = p1.emitter_handle();
        let diff = ChatPatternDifferential {
            patterns: vec![p1, p2],
            tick_id: 1,
            emitter_handle: 1,
            ts_bucketed: 100,
        };
        svc.ingest_peer_differential(&diff);
        assert_eq!(svc.federation().snapshot_public().len(), 1);
        svc.revoke_emitter(h1, 1000);
        assert_eq!(svc.federation().snapshot_public().len(), 0);
    }

    #[test]
    fn handle_purge_request_drops_emitter_state() {
        let svc = MyceliumChatSync::builder().k_floor(2).build();
        let p1 = mk_pattern(1, IntentKind::Question, CAP_FLAGS_ALL);
        let p2 = mk_pattern(2, IntentKind::Question, CAP_FLAGS_ALL);
        let h1 = p1.emitter_handle();
        svc.ingest_peer_differential(&ChatPatternDifferential {
            patterns: vec![p1, p2],
            tick_id: 1,
            emitter_handle: 0,
            ts_bucketed: 0,
        });
        assert_eq!(svc.federation().snapshot_public().len(), 1);
        svc.handle_purge_request(&PurgeRequest {
            emitter_handle: h1,
            ts_unix: 0,
        });
        assert_eq!(svc.federation().snapshot_public().len(), 0);
        assert_eq!(svc.stats().purges_received, 1);
    }

    #[test]
    fn cap_policy_default_deny() {
        let pol = CapPolicy::new(CAP_EMIT_ALLOWED);
        assert!(!pol.is_allowed(42));
    }

    #[test]
    fn cap_policy_grant_then_revoke() {
        let mut pol = CapPolicy::new(CAP_EMIT_ALLOWED);
        pol.grant(42, CAP_EMIT_ALLOWED);
        assert!(pol.is_allowed(42));
        pol.revoke(42);
        assert!(!pol.is_allowed(42));
    }

    #[test]
    fn deterministic_tick_yields_same_federation_blake3_after_replay() {
        let svc1 = MyceliumChatSync::builder().k_floor(2).build();
        let svc2 = MyceliumChatSync::builder().k_floor(2).build();
        let p1 = mk_pattern(1, IntentKind::Question, CAP_FLAGS_ALL);
        let p2 = mk_pattern(2, IntentKind::Question, CAP_FLAGS_ALL);
        let diff = ChatPatternDifferential {
            patterns: vec![p1, p2],
            tick_id: 7,
            emitter_handle: 0,
            ts_bucketed: 100,
        };
        svc1.ingest_peer_differential(&diff);
        svc2.ingest_peer_differential(&diff);
        assert_eq!(
            svc1.federation().federation_blake3(),
            svc2.federation().federation_blake3()
        );
    }

    #[test]
    fn tick_with_no_observations_returns_none() {
        let svc = MyceliumChatSync::new();
        assert!(svc.tick(60 * 100).is_none());
    }

    #[test]
    fn grant_then_observe_multiple_emitters() {
        let sink = Arc::new(InMemoryBroadcastSink::default());
        let svc = MyceliumChatSync::builder()
            .sink(sink.clone() as Arc<dyn BroadcastSink>)
            .build();
        for seed in 1..=3_u8 {
            let p = mk_pattern(seed, IntentKind::Question, CAP_FLAGS_ALL);
            svc.grant_emitter(p.emitter_handle(), CAP_FLAGS_ALL);
            svc.observe(p);
        }
        let _ = svc.tick(60 * 100);
        let recorded = sink.emitted_snapshot();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].patterns.len(), 3);
    }
}
