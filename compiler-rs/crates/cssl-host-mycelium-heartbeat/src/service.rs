//! § service — `HeartbeatService` orchestrator · digest-loop · revoke
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   The service is the orchestrator. It owns :
//!     ─ the local `HeartbeatRing`            (producer)
//!     ─ the local `BackpressureQueue`        (cloud-down absorption)
//!     ─ the `FederationCapPolicy`            (Σ-mask gate · default-deny)
//!     ─ the cloud-health tracker             (heartbeat liveness)
//!
//!   Every heartbeat-tick (default 60s) :
//!     1. Drain the local ring → vector of `FederationPattern`s
//!     2. Σ-mask gate at emit-time : drop any pattern lacking
//!        `CAP_FED_EMIT_ALLOWED`
//!     3. Bundle survivors into a `FederationBundle` (with anchor)
//!     4. Compress + enqueue OR direct-emit (depending on cloud-health)
//!     5. Drain backpressure queue if cloud is up + queue is non-empty
//!
//! § HOST-DRIVEN SCHEDULING
//!   We avoid a hard tokio dep ; the host crate decides how to schedule
//!   `tick`. The W14-J persistent-orchestrator calls `tick` from its
//!   60s-cadence loop ; the W14-K cron calls `mark_cloud_up` /
//!   `mark_cloud_down` from network-probe results.
//!
//! § REVOKE FLOW
//!   `revoke_emitter` :
//!     1. Drop the cap-policy → outgoing-zero on subsequent ticks
//!     2. Purge the local ring    (defense-in-depth)
//!     3. Drop the backpressure queue (zero queued bundles for this emitter
//!        survive — we drop the WHOLE queue rather than per-emitter
//!        filter, since bundles are post-anchor + per-emitter filtering
//!        would invalidate the anchor)
//!     4. Emit a `PurgeRequest` (caller responsible for HTTP-POST)
//!
//! § DETERMINISM
//!   `tick` is deterministic given : ring-state · cap-policy · cohort-size.
//!   Ts-bucketing is the only time-coupled element ; given a fixed
//!   ts-bucket, replay produces identical bundle anchors.

use crate::backpressure::{enqueue_bundle, BackpressureQueue, DEFAULT_QUEUE_CAPACITY};
use crate::bundle::{BundleError, FederationBundle};
use crate::pattern::{FederationPattern, CAP_FED_EMIT_ALLOWED, CAP_FED_PURGE_ON_REVOKE};
use crate::ring::{HeartbeatRing, DEFAULT_RING_CAPACITY};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

/// § `DEFAULT_HEARTBEAT_PERIOD_SECS` — once per minute.
pub const DEFAULT_HEARTBEAT_PERIOD_SECS: u64 = 60;

/// § `K_ANON_FLOOR` — k=10 (tighter than chat-sync's k=5).
pub const K_ANON_FLOOR: u32 = 10;

/// § `CloudHealth` — outgoing-pipe state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudHealth {
    /// § `Up` — recent heartbeat acknowledged ; emit direct.
    Up,
    /// § `Down` — recent failure ; route to backpressure queue.
    Down,
    /// § `Unknown` — never-pinged ; act as Down (safe default).
    Unknown,
}

/// § `HeartbeatStats` — observability snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatStats {
    pub ticks_total: u64,
    pub patterns_emitted: u64,
    pub patterns_dropped_by_cap: u64,
    pub bundles_built: u64,
    pub bundles_direct_emit: u64,
    pub bundles_queued: u64,
    pub bundles_drained_on_reconnect: u64,
    pub revokes_processed: u64,
}

/// § `PurgeRequest` — broadcast at sovereign-revoke time. The CLOUD endpoint
/// applies this purge to every federation row whose `emitter_handle`
/// matches ; peers see the new digest on next pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeRequest {
    pub emitter_handle: u64,
    pub ts_unix: u64,
    /// 32-byte BLAKE3 anchor of (emitter_handle ‖ ts_unix) ; tamper-evidence.
    pub anchor: [u8; 32],
}

impl PurgeRequest {
    /// § new — derive the anchor inline.
    #[must_use]
    pub fn new(emitter_handle: u64, ts_unix: u64) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"federation\0purge\0v1");
        h.update(&emitter_handle.to_le_bytes());
        h.update(&ts_unix.to_le_bytes());
        let bytes = h.finalize();
        let mut anchor = [0_u8; 32];
        anchor.copy_from_slice(bytes.as_bytes());
        Self {
            emitter_handle,
            ts_unix,
            anchor,
        }
    }

    /// § verify_anchor — tamper-evidence.
    #[must_use]
    pub fn verify_anchor(&self) -> bool {
        let recomputed = Self::new(self.emitter_handle, self.ts_unix);
        recomputed.anchor == self.anchor
    }
}

/// § `FederationCapPolicy` — Σ-mask state for the local emitter set.
///
/// Multi-tenant : a single host may run multiple emitter-keys (e.g. a
/// household-shared install). The policy gates emit per-emitter.
/// Default-deny : any emitter not in the map has cap_flags = 0.
#[derive(Default, Debug, Clone)]
pub struct FederationCapPolicy {
    grants: BTreeMap<u64, u8>,
    required: u8,
}

impl FederationCapPolicy {
    /// § new — required cap-bits for emit (typically `CAP_FED_EMIT_ALLOWED`).
    #[must_use]
    pub const fn new(required: u8) -> Self {
        Self {
            grants: BTreeMap::new(),
            required,
        }
    }

    /// § grant — set the cap-flags for an emitter (idempotent).
    pub fn grant(&mut self, emitter_handle: u64, cap_flags: u8) {
        self.grants.insert(emitter_handle, cap_flags);
    }

    /// § revoke — zero the cap for an emitter.
    pub fn revoke(&mut self, emitter_handle: u64) {
        self.grants.remove(&emitter_handle);
    }

    /// § check — true iff the emitter holds ALL `required` bits.
    #[must_use]
    pub fn check(&self, emitter_handle: u64) -> bool {
        self.grants
            .get(&emitter_handle)
            .copied()
            .is_some_and(|f| (f & self.required) == self.required)
    }

    /// § get — current cap-flags for emitter (None if no grant).
    pub fn get(&self, emitter_handle: u64) -> Option<u8> {
        self.grants.get(&emitter_handle).copied()
    }
}

/// § `HeartbeatService` — the orchestrator.
pub struct HeartbeatService {
    ring: Arc<HeartbeatRing>,
    queue: Arc<BackpressureQueue>,
    cap_policy: Arc<RwLock<FederationCapPolicy>>,
    cloud_health: Arc<RwLock<CloudHealth>>,
    inner: Mutex<ServiceInner>,
    /// Local-node emitter handle (BLAKE3-trunc of host pubkey).
    node_handle: u64,
    period_secs: u64,
}

struct ServiceInner {
    tick_id_next: u64,
    stats: HeartbeatStats,
}

impl HeartbeatService {
    /// § builder — start a typed-builder.
    #[must_use]
    pub fn builder() -> HeartbeatServiceBuilder {
        HeartbeatServiceBuilder::default()
    }

    /// § ring — handle to push observed patterns.
    #[must_use]
    pub fn ring(&self) -> Arc<HeartbeatRing> {
        Arc::clone(&self.ring)
    }

    /// § queue — handle to inspect / drain backpressure (cron-callable).
    #[must_use]
    pub fn queue(&self) -> Arc<BackpressureQueue> {
        Arc::clone(&self.queue)
    }

    /// § grant_emitter — Σ-mask grant (consent-arch).
    pub fn grant_emitter(&self, emitter_handle: u64, cap_flags: u8) {
        let mut p = self.cap_policy.write().expect("cap-policy lock");
        p.grant(emitter_handle, cap_flags);
    }

    /// § cap_policy — read-only snapshot for tests + observability.
    pub fn cap_policy_snapshot(&self) -> FederationCapPolicy {
        self.cap_policy.read().expect("cap-policy lock").clone()
    }

    /// § cloud_health — observed pipe state.
    pub fn cloud_health(&self) -> CloudHealth {
        *self.cloud_health.read().expect("cloud-health lock")
    }

    /// § mark_cloud_up — switches direct-emit on. Returns the previous state.
    pub fn mark_cloud_up(&self) -> CloudHealth {
        let mut h = self.cloud_health.write().expect("cloud-health lock");
        let prev = *h;
        *h = CloudHealth::Up;
        prev
    }

    /// § mark_cloud_down — routes new bundles to the queue.
    pub fn mark_cloud_down(&self) -> CloudHealth {
        let mut h = self.cloud_health.write().expect("cloud-health lock");
        let prev = *h;
        *h = CloudHealth::Down;
        prev
    }

    /// § stats — observability snapshot.
    pub fn stats(&self) -> HeartbeatStats {
        self.inner.lock().expect("svc inner lock").stats
    }

    /// § revoke_emitter — sovereign-revoke. Returns a `PurgeRequest` the
    /// caller should HTTP-POST to peers.
    ///
    /// The `_purge_ok` arg toggles `CAP_FED_PURGE_ON_REVOKE` checking. If
    /// the emitter never granted that cap, we still revoke locally but
    /// the returned PurgeRequest is `None` (peer-purge-propagation requires
    /// explicit consent).
    pub fn revoke_emitter(&self, emitter_handle: u64, ts_unix: u64) -> Option<PurgeRequest> {
        // 1. Read current cap-flags BEFORE revoking ; we need to know if
        //    the emitter had granted CAP_FED_PURGE_ON_REVOKE.
        let purge_ok = {
            let p = self.cap_policy.read().expect("cap-policy lock");
            p.get(emitter_handle)
                .is_some_and(|f| (f & CAP_FED_PURGE_ON_REVOKE) != 0)
        };

        // 2. Zero cap-policy for this emitter (idempotent).
        {
            let mut p = self.cap_policy.write().expect("cap-policy lock");
            p.revoke(emitter_handle);
        }

        // 3. Purge local ring (defense-in-depth).
        self.ring.purge_emitter(emitter_handle);

        // 4. Drop the backpressure queue (anchor-bound bundles can't be
        //    selectively-edited, so we trade a small amount of stale
        //    federation-data for a clean revoke).
        self.queue.drain_all();

        // 5. Bump revokes counter.
        {
            let mut inner = self.inner.lock().expect("svc inner lock");
            inner.stats.revokes_processed += 1;
        }

        // 6. Return PurgeRequest iff the emitter had consented to peer-purge.
        if purge_ok {
            Some(PurgeRequest::new(emitter_handle, ts_unix))
        } else {
            None
        }
    }

    /// § tick — single-step the heartbeat loop. Returns the bundle that
    /// was emitted (or `None` on empty ring).
    ///
    /// Caller responsibility :
    ///   ─ Schedule `tick` at `period_secs` cadence.
    ///   ─ HTTP-POST the returned compressed-blob to `/api/mycelium/heartbeat`
    ///     when `cloud_health == Up` ; on success, mark_cloud_up + drain
    ///     queue ; on failure, mark_cloud_down (already enqueued).
    pub fn tick(&self, now_unix: u64) -> Result<Option<FederationBundle>, BundleError> {
        // 1. Drain ring.
        let raw = self.ring.drain();

        // 2. Σ-mask gate at emit-side : drop patterns lacking the required
        //    cap-bit. Patterns whose emitter is not in the cap-policy are
        //    treated as default-deny (cap_flags=0 → no bits set).
        let cap = self.cap_policy.read().expect("cap-policy lock").clone();
        let mut filtered: Vec<FederationPattern> = Vec::with_capacity(raw.len());
        let mut dropped = 0_u64;
        for p in raw {
            let emitter_ok = cap.check(p.emitter_handle());
            let pattern_ok = p.cap_check(CAP_FED_EMIT_ALLOWED);
            if emitter_ok && pattern_ok {
                filtered.push(p);
            } else {
                dropped += 1;
            }
        }

        // 3. Update stats for drops + ticks.
        {
            let mut inner = self.inner.lock().expect("svc inner lock");
            inner.stats.ticks_total += 1;
            inner.stats.patterns_dropped_by_cap += dropped;
            inner.stats.patterns_emitted += filtered.len() as u64;
        }

        if filtered.is_empty() {
            return Ok(None);
        }

        // 4. Build bundle (with anchor).
        let (tick_id, ts_bucketed) = {
            let mut inner = self.inner.lock().expect("svc inner lock");
            let t = inner.tick_id_next;
            inner.tick_id_next += 1;
            inner.stats.bundles_built += 1;
            (t, ((now_unix / 60) & 0xFFFF_FFFF) as u32)
        };

        let bundle = FederationBundle::build(tick_id, self.node_handle, ts_bucketed, filtered)?;

        // 5. Route based on cloud-health.
        match self.cloud_health() {
            CloudHealth::Up => {
                let mut inner = self.inner.lock().expect("svc inner lock");
                inner.stats.bundles_direct_emit += 1;
            }
            CloudHealth::Down | CloudHealth::Unknown => {
                let _ = enqueue_bundle(&self.queue, &bundle);
                let mut inner = self.inner.lock().expect("svc inner lock");
                inner.stats.bundles_queued += 1;
            }
        }

        Ok(Some(bundle))
    }

    /// § drain_queue_on_reconnect — when cloud transitions Down→Up, the
    /// service drains the backpressure queue. Returns the count drained.
    /// Caller is responsible for the HTTP-POST loop ; this method just
    /// pops blobs and bumps the drained counter.
    pub fn drain_queue_one(&self) -> Option<Vec<u8>> {
        let blob = self.queue.drain_one();
        if blob.is_some() {
            let mut inner = self.inner.lock().expect("svc inner lock");
            inner.stats.bundles_drained_on_reconnect += 1;
        }
        blob
    }

    /// § period_secs — heartbeat cadence (default 60).
    #[must_use]
    pub const fn period_secs(&self) -> u64 {
        self.period_secs
    }

    /// § node_handle — our local emitter handle (8-byte BLAKE3-trunc).
    #[must_use]
    pub const fn node_handle(&self) -> u64 {
        self.node_handle
    }
}

/// § `HeartbeatServiceBuilder` — typed-builder for the service.
#[derive(Debug, Default)]
pub struct HeartbeatServiceBuilder {
    ring_capacity: Option<usize>,
    queue_capacity: Option<usize>,
    period_secs: Option<u64>,
    node_pubkey: Option<[u8; 32]>,
    initial_cloud_health: Option<CloudHealth>,
    required_cap: Option<u8>,
}

impl HeartbeatServiceBuilder {
    #[must_use]
    pub const fn ring_capacity(mut self, n: usize) -> Self {
        self.ring_capacity = Some(n);
        self
    }
    #[must_use]
    pub const fn queue_capacity(mut self, n: usize) -> Self {
        self.queue_capacity = Some(n);
        self
    }
    #[must_use]
    pub const fn period_secs(mut self, n: u64) -> Self {
        self.period_secs = Some(n);
        self
    }
    #[must_use]
    pub const fn node_pubkey(mut self, k: [u8; 32]) -> Self {
        self.node_pubkey = Some(k);
        self
    }
    #[must_use]
    pub const fn initial_cloud_health(mut self, h: CloudHealth) -> Self {
        self.initial_cloud_health = Some(h);
        self
    }
    #[must_use]
    pub const fn required_cap(mut self, c: u8) -> Self {
        self.required_cap = Some(c);
        self
    }

    /// § build — finalize.
    #[must_use]
    pub fn build(self) -> HeartbeatService {
        let ring_capacity = self.ring_capacity.unwrap_or(DEFAULT_RING_CAPACITY);
        let queue_capacity = self.queue_capacity.unwrap_or(DEFAULT_QUEUE_CAPACITY);
        let period_secs = self.period_secs.unwrap_or(DEFAULT_HEARTBEAT_PERIOD_SECS);
        let node_pubkey = self.node_pubkey.unwrap_or([0_u8; 32]);
        let initial_cloud_health = self.initial_cloud_health.unwrap_or(CloudHealth::Unknown);
        let required_cap = self.required_cap.unwrap_or(CAP_FED_EMIT_ALLOWED);

        // Derive node_handle from node_pubkey.
        let node_handle = {
            let mut h = blake3::Hasher::new();
            h.update(b"federation\0node\0v1");
            h.update(&node_pubkey);
            let bytes = h.finalize();
            let mut buf = [0_u8; 8];
            buf.copy_from_slice(&bytes.as_bytes()[..8]);
            u64::from_le_bytes(buf)
        };

        HeartbeatService {
            ring: Arc::new(HeartbeatRing::with_capacity(ring_capacity)),
            queue: Arc::new(BackpressureQueue::with_capacity(queue_capacity)),
            cap_policy: Arc::new(RwLock::new(FederationCapPolicy::new(required_cap))),
            cloud_health: Arc::new(RwLock::new(initial_cloud_health)),
            inner: Mutex::new(ServiceInner {
                tick_id_next: 0,
                stats: HeartbeatStats::default(),
            }),
            node_handle,
            period_secs,
        }
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{FederationKind, FederationPatternBuilder, CAP_FED_FLAGS_ALL};

    fn mk_pattern(seed: u8, cap_flags: u8) -> FederationPattern {
        FederationPatternBuilder {
            kind: FederationKind::CellState,
            cap_flags,
            k_anon_cohort_size: 12,
            confidence: 0.5,
            ts_unix: 60 * u64::from(seed),
            payload: vec![seed; 16],
            emitter_pubkey: [seed; 32],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn cap_policy_default_deny() {
        let p = FederationCapPolicy::new(CAP_FED_EMIT_ALLOWED);
        assert!(!p.check(0xDEAD_BEEF));
    }

    #[test]
    fn cap_policy_grants_unlock_emit() {
        let mut p = FederationCapPolicy::new(CAP_FED_EMIT_ALLOWED);
        p.grant(123, CAP_FED_FLAGS_ALL);
        assert!(p.check(123));
        p.revoke(123);
        assert!(!p.check(123));
    }

    #[test]
    fn tick_drops_when_cap_missing() {
        let svc = HeartbeatService::builder().build();
        let p = mk_pattern(1, 0); // cap_flags=0 → no bits set
        svc.ring().push(p);
        let r = svc.tick(60).unwrap();
        assert!(r.is_none());
        let s = svc.stats();
        assert_eq!(s.patterns_dropped_by_cap, 1);
    }

    #[test]
    fn tick_emits_when_cap_granted() {
        let svc = HeartbeatService::builder()
            .initial_cloud_health(CloudHealth::Up)
            .build();
        let p = mk_pattern(1, CAP_FED_FLAGS_ALL);
        svc.grant_emitter(p.emitter_handle(), CAP_FED_FLAGS_ALL);
        svc.ring().push(p);
        let bundle = svc.tick(60).unwrap();
        assert!(bundle.is_some());
        let s = svc.stats();
        assert_eq!(s.patterns_emitted, 1);
        assert_eq!(s.bundles_direct_emit, 1);
    }

    #[test]
    fn tick_routes_to_queue_when_cloud_down() {
        let svc = HeartbeatService::builder()
            .initial_cloud_health(CloudHealth::Down)
            .build();
        let p = mk_pattern(1, CAP_FED_FLAGS_ALL);
        svc.grant_emitter(p.emitter_handle(), CAP_FED_FLAGS_ALL);
        svc.ring().push(p);
        let bundle = svc.tick(60).unwrap();
        assert!(bundle.is_some());
        let s = svc.stats();
        assert_eq!(s.bundles_queued, 1);
        assert_eq!(s.bundles_direct_emit, 0);
        assert!(svc.queue().len() >= 1);
    }

    #[test]
    fn drain_queue_one_increments_counter() {
        let svc = HeartbeatService::builder()
            .initial_cloud_health(CloudHealth::Down)
            .build();
        let p = mk_pattern(1, CAP_FED_FLAGS_ALL);
        svc.grant_emitter(p.emitter_handle(), CAP_FED_FLAGS_ALL);
        svc.ring().push(p);
        svc.tick(60).unwrap();
        // Now reconnect.
        svc.mark_cloud_up();
        let blob = svc.drain_queue_one();
        assert!(blob.is_some());
        let s = svc.stats();
        assert_eq!(s.bundles_drained_on_reconnect, 1);
    }

    #[test]
    fn revoke_emitter_clears_cap_and_returns_purge_when_consented() {
        let svc = HeartbeatService::builder().build();
        let p = mk_pattern(1, CAP_FED_FLAGS_ALL);
        let handle = p.emitter_handle();
        svc.grant_emitter(handle, CAP_FED_FLAGS_ALL); // consents to purge-on-revoke
        let purge = svc.revoke_emitter(handle, 12345);
        assert!(purge.is_some());
        let pr = purge.unwrap();
        assert_eq!(pr.emitter_handle, handle);
        assert!(pr.verify_anchor());
        // Cap was zeroed.
        assert!(!svc.cap_policy_snapshot().check(handle));
    }

    #[test]
    fn revoke_emitter_no_purge_when_not_consented() {
        let svc = HeartbeatService::builder().build();
        // Grant only EMIT bit (no PURGE_ON_REVOKE).
        svc.grant_emitter(0x1234, CAP_FED_EMIT_ALLOWED);
        let purge = svc.revoke_emitter(0x1234, 12345);
        assert!(purge.is_none());
    }

    #[test]
    fn revoke_drains_queue() {
        let svc = HeartbeatService::builder()
            .initial_cloud_health(CloudHealth::Down)
            .build();
        let p = mk_pattern(1, CAP_FED_FLAGS_ALL);
        let handle = p.emitter_handle();
        svc.grant_emitter(handle, CAP_FED_FLAGS_ALL);
        svc.ring().push(p);
        svc.tick(60).unwrap();
        assert!(svc.queue().len() >= 1);
        svc.revoke_emitter(handle, 60);
        assert_eq!(svc.queue().len(), 0);
    }

    #[test]
    fn tick_id_monotonic() {
        let svc = HeartbeatService::builder()
            .initial_cloud_health(CloudHealth::Up)
            .build();
        for i in 1..=3_u8 {
            let p = mk_pattern(i, CAP_FED_FLAGS_ALL);
            svc.grant_emitter(p.emitter_handle(), CAP_FED_FLAGS_ALL);
            svc.ring().push(p);
        }
        let b = svc.tick(60).unwrap().unwrap();
        assert_eq!(b.tick_id, 0);
        // Push another batch for a second tick.
        let p2 = mk_pattern(99, CAP_FED_FLAGS_ALL);
        svc.grant_emitter(p2.emitter_handle(), CAP_FED_FLAGS_ALL);
        svc.ring().push(p2);
        let b2 = svc.tick(120).unwrap().unwrap();
        assert_eq!(b2.tick_id, 1);
    }

    #[test]
    fn cloud_health_transitions() {
        let svc = HeartbeatService::builder().build();
        assert!(matches!(svc.cloud_health(), CloudHealth::Unknown));
        let prev = svc.mark_cloud_up();
        assert!(matches!(prev, CloudHealth::Unknown));
        assert!(matches!(svc.cloud_health(), CloudHealth::Up));
        svc.mark_cloud_down();
        assert!(matches!(svc.cloud_health(), CloudHealth::Down));
    }

    #[test]
    fn purge_request_anchor_round_trips() {
        let pr = PurgeRequest::new(0xC0FFEE, 9999);
        assert!(pr.verify_anchor());
        // Tampering should invalidate.
        let mut bad = pr;
        bad.ts_unix = 1234;
        assert!(!bad.verify_anchor());
    }

    #[test]
    fn period_secs_default_is_60() {
        let svc = HeartbeatService::builder().build();
        assert_eq!(svc.period_secs(), DEFAULT_HEARTBEAT_PERIOD_SECS);
        assert_eq!(DEFAULT_HEARTBEAT_PERIOD_SECS, 60);
    }
}
