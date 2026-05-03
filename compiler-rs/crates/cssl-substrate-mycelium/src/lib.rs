//! `cssl-substrate-mycelium` — Mycelium-Network federation primitives.
//!
//! ============================================================
//! § PRIVACY ATTESTATION (load-first · sovereignty-respecting)
//! ============================================================
//!
//! This crate is the **foundation** of the Mycelial-Network federation
//! described in `spec/grand-vision/16` (substrate-as-mycelium). It is the
//! cross-user federated bias-learning surface where local KAN-bias shards,
//! ω-field patches, Σ-mask updates, recipe unlocks, and live hotfix
//! capsules **may** be exchanged — IF the user explicitly consents.
//!
//! ## What this crate is
//!
//! - **Purely-local** data structures + verification + merge heuristics.
//! - **Offline-first** : every primitive in this file works with **zero**
//!   network access. There is no `tokio`, `hyper`, `reqwest`, `quinn`,
//!   `libp2p`, or any other network dependency. There **never** will be
//!   in this crate.
//! - **Σ-mask aware** : every `PeerCapsuleRecord` carries a `sigma_mask`
//!   that is verified against an expected mask before any merge. A peer
//!   capsule whose Σ-mask does not match the local consent envelope is
//!   silently refused — never merged, never persisted.
//! - **Deterministic** : `LocalPeerCache` uses `BTreeMap<[u8; 32], usize>`
//!   for the peer index so iteration order is byte-stable across hosts
//!   and replay boundaries (see Apocky memory § BTreeMap-deterministic-serde).
//!
//! ## What this crate is NOT (and never will be)
//!
//! - **NOT telemetry.** Nothing here calls home. No metrics flow upward.
//! - **NOT a phone-home.** No automatic check-ins. No "anonymized usage".
//! - **NOT a DHT.** There is no peer-discovery mechanism. Peers are added
//!   ONLY by the user, ONLY via explicit local API calls, ONLY with
//!   public keys the user has personally verified.
//! - **NOT pay-for-power.** No cosmetic-channel coupling to gameplay.
//! - **NOT a truth-claim layer.** `merge_kan_bias_shards` is a *heuristic*
//!   weighted average — it does NOT claim peer data is correct. The
//!   caller treats merged output as a **suggestion**, never as gospel.
//!
//! ## Sovereignty-respecting opt-out
//!
//! Three layers of opt-out, in increasing order of strength :
//!
//! 1. **Default-off.** A fresh `LocalPeerCache::new(capacity)` contains
//!    zero peers. Until the user adds one (`peer_add`), zero capsules can
//!    flow in or out.
//! 2. **Per-capsule Σ-mask refusal.** Even after a peer is added, every
//!    incoming capsule is filtered by `capsule_verify_sigma_mask` against
//!    the user's *local* expected mask. Mismatch = silent drop.
//! 3. **Hard wipe.** `LocalPeerCache::clear` (or simply dropping the
//!    cache) erases all peer state. There is no "Are you sure?" because
//!    there is no remote copy.
//!
//! Future networking layers (NOT in this crate) MUST honor all three.
//! See `spec/grand-vision/16` § Mycelial-Network · § Sovereignty-Boundary.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ════════════════════════════════════════════════════════════════════
// § §I MyceliumPeer — sovereign peer-record (32B id + 32B pubkey + ts)
// ════════════════════════════════════════════════════════════════════

/// A peer the user has explicitly added to their local mycelium-cache.
///
/// `peer_id` is a 32-byte stable identifier (typically BLAKE3 of the
/// public key). `public_key` is the Ed25519 signing key the peer's
/// capsules MUST be signed with. `last_synced_at` is microseconds since
/// UNIX epoch the user-host last accepted a capsule from this peer.
/// `capabilities` is a bitfield of capsule-kinds this peer is permitted
/// to send (1 << kind-discriminant).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyceliumPeer {
    pub peer_id: [u8; 32],
    pub public_key: [u8; 32],
    pub last_synced_at: u64,
    pub capabilities: u32,
}

impl MyceliumPeer {
    /// Build a peer-record. `capabilities` defaults to ALL kinds permitted ;
    /// callers tighten as policy dictates.
    #[must_use]
    pub fn new(peer_id: [u8; 32], public_key: [u8; 32]) -> Self {
        Self {
            peer_id,
            public_key,
            last_synced_at: 0,
            capabilities: u32::MAX,
        }
    }

    /// Returns `true` iff this peer is authorized to send the given kind.
    #[must_use]
    pub fn permits(self, kind: PeerCapsuleKind) -> bool {
        let bit = 1u32 << (kind as u32);
        (self.capabilities & bit) != 0
    }
}

// ════════════════════════════════════════════════════════════════════
// § §II PeerCapsuleKind — federation-payload taxonomy (5 classes)
// ════════════════════════════════════════════════════════════════════

/// Discriminants for the 5 federation capsule classes.
///
/// Stable ordinals — these are persisted in capsule bytes, do NOT
/// renumber. Add new classes by appending new ordinals.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum PeerCapsuleKind {
    /// 8-band KAN-bias shard. Most common. See `merge_kan_bias_shards`.
    KanBiasShard = 0,
    /// Sparse ω-field cell-patch. Reserved for future use.
    OmegaFieldPatch = 1,
    /// Σ-mask consent-envelope update. Reserved for future use.
    SigmaMaskUpdate = 2,
    /// Recipe-unlock attestation (cross-user crafting discovery sharing).
    RecipeUnlock = 3,
    /// Live-hotfix attestation. Mirrors the 8 hotfix-classes in the
    /// `loa-host` crate (KAN, balance, recipe, Nemesis, security,
    /// storylet, render). The `payload` carries the class-specific delta.
    HotfixClass = 4,
}

impl PeerCapsuleKind {
    /// Stable u8 wire-encoding.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode a u8 back to a kind. `None` if unknown discriminant.
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::KanBiasShard),
            1 => Some(Self::OmegaFieldPatch),
            2 => Some(Self::SigmaMaskUpdate),
            3 => Some(Self::RecipeUnlock),
            4 => Some(Self::HotfixClass),
            _ => None,
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// § §III PeerCapsuleRecord — signed federation capsule
// ════════════════════════════════════════════════════════════════════

/// One federation capsule the user-host has *received* (or could send).
///
/// `payload` is the kind-specific bytes (interpretation lives in the
/// receiving sibling crate ; this crate only **transports + validates**).
/// `sigma_mask` carries the consent-envelope. `ts_us` is microseconds
/// since UNIX epoch when the originating peer signed. `signer` is the
/// peer's 32-byte public key (matches `MyceliumPeer::public_key`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerCapsuleRecord {
    pub kind: PeerCapsuleKind,
    pub payload: Vec<u8>,
    pub sigma_mask: u64,
    pub ts_us: u64,
    pub signer: [u8; 32],
}

impl PeerCapsuleRecord {
    /// Total in-memory bytes occupied (header + payload). Used by
    /// `cache_stats` for capacity-budgeting.
    #[must_use]
    pub fn approx_bytes(&self) -> usize {
        // header : kind(1) + sigma_mask(8) + ts_us(8) + signer(32)  = 49
        // plus Vec<u8> length-prefix(8) + payload-bytes
        49 + 8 + self.payload.len()
    }
}

/// Verify that a capsule's `sigma_mask` matches the expected mask. The
/// match rule is **bitwise-equality** : a peer offering a *superset* mask
/// is rejected (privacy-tightening), and a peer offering a *subset* mask
/// is rejected (consent-erosion). Only exact match accepts.
///
/// This is intentionally strict. The user expressed a Σ-mask ; capsules
/// flow only on exact-match. If a peer wants a different envelope they
/// MUST renegotiate via the (yet-to-be-built) consent-handshake layer.
#[must_use]
pub fn capsule_verify_sigma_mask(record: &PeerCapsuleRecord, expected_mask: u64) -> bool {
    record.sigma_mask == expected_mask
}

// ════════════════════════════════════════════════════════════════════
// § §IV LocalPeerCache — bounded LRU + deterministic peer-index
// ════════════════════════════════════════════════════════════════════

/// Bounded local-only cache of accepted peer-capsules.
///
/// Capacity is in *records* (not bytes ; byte-budget is observed via
/// `cache_stats(...).total_bytes`). When `records.len() > capacity`,
/// `cache_evict_lru` drops the oldest record (smallest `ts_us`) and
/// rebuilds the peer index. The peer index is a `BTreeMap` keyed by
/// the originating peer's public-key, mapping to the *most-recent*
/// record-index for that peer (used for fast "do I have anything from
/// this peer?" queries).
///
/// `BTreeMap` (not `HashMap`) is mandatory — see Apocky memory
/// § BTreeMap-deterministic-serde. Iteration order MUST be byte-stable
/// across hosts so that serde-roundtrip preserves bit-identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalPeerCache {
    pub capacity: usize,
    pub records: Vec<PeerCapsuleRecord>,
    pub peer_index: BTreeMap<[u8; 32], usize>,
    pub peers: BTreeMap<[u8; 32], MyceliumPeer>,
}

impl LocalPeerCache {
    /// Construct an empty cache with the given record-capacity.
    /// `capacity` MUST be >= 1 ; lower values are clamped to 1 to keep
    /// the eviction algorithm well-defined.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            records: Vec::new(),
            peer_index: BTreeMap::new(),
            peers: BTreeMap::new(),
        }
    }

    /// Add (or replace) a peer record. Returns the previous record if any.
    /// User-explicit call site — this is the *only* way a peer enters
    /// the local cache. There is no auto-discovery.
    pub fn peer_add(&mut self, peer: MyceliumPeer) -> Option<MyceliumPeer> {
        self.peers.insert(peer.peer_id, peer)
    }

    /// Remove a peer + every capsule signed by that peer. Returns the
    /// removed `MyceliumPeer` if present.
    pub fn peer_remove(&mut self, peer_id: &[u8; 32]) -> Option<MyceliumPeer> {
        let removed = self.peers.remove(peer_id);
        if let Some(p) = &removed {
            // drop all records signed by this peer's public_key
            self.records.retain(|r| r.signer != p.public_key);
            self.peer_index.remove(&p.public_key);
            self.rebuild_index();
        }
        removed
    }

    /// Append a new capsule record. Caller is expected to have already
    /// run `capsule_verify_sigma_mask` + (future) signature-verify ; this
    /// function is *transport*, not validation.
    ///
    /// If `records.len()` exceeds `capacity` after the push, the caller
    /// should invoke `cache_evict_lru` (kept as a separate fn so callers
    /// can batch evictions over multiple pushes).
    pub fn push_record(&mut self, record: PeerCapsuleRecord) {
        let idx = self.records.len();
        let signer = record.signer;
        self.records.push(record);
        // Update peer-index with the most-recent record-index for this signer.
        self.peer_index.insert(signer, idx);
    }

    /// Wipe **everything**. Sovereignty-respecting hard-stop.
    pub fn clear(&mut self) {
        self.records.clear();
        self.peer_index.clear();
        self.peers.clear();
    }

    /// Rebuild the peer-index after eviction or peer-removal.
    /// The index points to the most-recent (highest-ts_us) record per
    /// signer.
    fn rebuild_index(&mut self) {
        self.peer_index.clear();
        for (i, r) in self.records.iter().enumerate() {
            // Always keep the highest-ts_us record-idx for a signer.
            let entry = self.peer_index.entry(r.signer).or_insert(i);
            if self.records[*entry].ts_us < r.ts_us {
                *entry = i;
            }
        }
    }
}

/// Evict the oldest record(s) until `cache.records.len() <= cache.capacity`.
///
/// "Oldest" is `ts_us` ascending — peer signed-time, NOT host-receive
/// time. Ties broken by record-vector position (lower index evicted
/// first).
///
/// After eviction the peer-index is rebuilt deterministically.
pub fn cache_evict_lru(cache: &mut LocalPeerCache) {
    while cache.records.len() > cache.capacity {
        // Find the index of the oldest record.
        let mut oldest_idx = 0;
        let mut oldest_ts = cache.records[0].ts_us;
        for (i, r) in cache.records.iter().enumerate().skip(1) {
            if r.ts_us < oldest_ts {
                oldest_ts = r.ts_us;
                oldest_idx = i;
            }
        }
        cache.records.remove(oldest_idx);
    }
    cache.rebuild_index();
}

// ════════════════════════════════════════════════════════════════════
// § §V CacheStats — telemetry-free observability
// ════════════════════════════════════════════════════════════════════

/// Local-only stats for capacity-monitoring + UI dashboards.
/// **Nothing leaves the host** — these numbers are computed on demand
/// + handed back to the user-facing layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheStats {
    pub count: usize,
    pub total_bytes: usize,
    pub oldest_us: u64,
    pub peer_count: usize,
}

/// Compute current stats. O(N) over records — N is bounded by capacity
/// so this is fine to call every frame if desired.
#[must_use]
pub fn cache_stats(cache: &LocalPeerCache) -> CacheStats {
    let count = cache.records.len();
    let total_bytes: usize = cache.records.iter().map(PeerCapsuleRecord::approx_bytes).sum();
    let oldest_us = cache
        .records
        .iter()
        .map(|r| r.ts_us)
        .min()
        .unwrap_or(0);
    CacheStats {
        count,
        total_bytes,
        oldest_us,
        peer_count: cache.peers.len(),
    }
}

// ════════════════════════════════════════════════════════════════════
// § §VI merge_kan_bias_shards — Σ-mask-aware weighted average
// ════════════════════════════════════════════════════════════════════

/// Weighted-average merge of an 8-band KAN-bias shard.
///
/// `local` is the user-host's current 8-band bias vector (u32 each).
/// `peer_shards` is the slice of received peer-shards (already
/// Σ-mask + signature verified by the caller).
///
/// **Algorithm** :
/// - Local weight = `peer_shards.len() + 1` (local always carries the
///   majority vote against any single peer ; sovereignty-respecting).
/// - Each peer shard contributes weight 1.
/// - Per-band : `(local * local_weight + Σ peer_band) / total_weight`.
/// - Saturating arithmetic — overflow is clamped, never wraps.
///
/// Determinism : iteration is in-order over `peer_shards`. Same inputs
/// produce same outputs across hosts.
///
/// **No truth-claim** : the merged output is a *heuristic suggestion*.
/// The caller MUST keep the original `local` value retrievable for
/// sovereignty-rollback.
#[must_use]
pub fn merge_kan_bias_shards(local: [u32; 8], peer_shards: &[[u32; 8]]) -> [u32; 8] {
    if peer_shards.is_empty() {
        return local;
    }
    let local_weight = peer_shards.len() as u64 + 1;
    let total_weight = local_weight + peer_shards.len() as u64;
    let mut out = [0u32; 8];
    for band in 0..8 {
        let local_contrib = u64::from(local[band]).saturating_mul(local_weight);
        let mut peer_sum: u64 = 0;
        for shard in peer_shards {
            peer_sum = peer_sum.saturating_add(u64::from(shard[band]));
        }
        let merged = (local_contrib.saturating_add(peer_sum)) / total_weight;
        // clamp to u32 range — saturating arithmetic above means we
        // can hit u64::MAX/total_weight which is still < u32::MAX after
        // division for sane inputs ; defensive clamp anyway.
        out[band] = u32::try_from(merged).unwrap_or(u32::MAX);
    }
    out
}

// ════════════════════════════════════════════════════════════════════
// § §VII Tests — 12+ unit-tests as required by the wave-prompt
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_peer(seed: u8) -> MyceliumPeer {
        let mut id = [0u8; 32];
        let mut pk = [0u8; 32];
        id[0] = seed;
        pk[0] = seed.wrapping_add(0x80);
        MyceliumPeer::new(id, pk)
    }

    fn fixed_record(kind: PeerCapsuleKind, sigma_mask: u64, ts: u64, signer_seed: u8) -> PeerCapsuleRecord {
        let mut signer = [0u8; 32];
        signer[0] = signer_seed.wrapping_add(0x80);
        PeerCapsuleRecord {
            kind,
            payload: vec![signer_seed; 16],
            sigma_mask,
            ts_us: ts,
            signer,
        }
    }

    // -- TEST 1 : peer-add -------------------------------------------
    #[test]
    fn peer_add_returns_none_first_time() {
        let mut c = LocalPeerCache::new(4);
        let p = fixed_peer(1);
        assert!(c.peer_add(p).is_none());
        assert_eq!(c.peers.len(), 1);
    }

    // -- TEST 2 : peer-add-then-remove -------------------------------
    #[test]
    fn peer_remove_purges_records_signed_by_peer() {
        let mut c = LocalPeerCache::new(8);
        let p = fixed_peer(2);
        c.peer_add(p);
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0xCAFE, 100, 2));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0xCAFE, 200, 7));
        assert_eq!(c.records.len(), 2);
        let removed = c.peer_remove(&p.peer_id);
        assert!(removed.is_some());
        // Only signer=7 record survives.
        assert_eq!(c.records.len(), 1);
        assert_eq!(c.records[0].signer[0], 7u8.wrapping_add(0x80));
    }

    // -- TEST 3 : capsule-roundtrip-bytes ----------------------------
    #[test]
    fn capsule_roundtrip_bytes_via_serde_json() {
        let r = fixed_record(PeerCapsuleKind::HotfixClass, 0x1234_5678, 999, 5);
        let bytes = serde_json::to_vec(&r).expect("serde encode");
        let back: PeerCapsuleRecord = serde_json::from_slice(&bytes).expect("serde decode");
        assert_eq!(r, back);
    }

    // -- TEST 4 : sigma-mask-verify-rejects-mismatch -----------------
    #[test]
    fn sigma_mask_mismatch_is_rejected() {
        let r = fixed_record(PeerCapsuleKind::KanBiasShard, 0xAAAA_BBBB, 1, 1);
        assert!(!capsule_verify_sigma_mask(&r, 0xAAAA_BBBC));
        assert!(capsule_verify_sigma_mask(&r, 0xAAAA_BBBB));
    }

    // -- TEST 5 : LRU-eviction-correct -------------------------------
    #[test]
    fn lru_evicts_oldest_first_until_under_capacity() {
        let mut c = LocalPeerCache::new(2);
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 100, 1));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 50, 2));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 200, 3));
        assert_eq!(c.records.len(), 3);
        cache_evict_lru(&mut c);
        assert_eq!(c.records.len(), 2);
        // ts=50 (signer-seed=2) should be gone ; surviving signer-seeds = {1, 3}.
        let surviving_ts: Vec<u64> = c.records.iter().map(|r| r.ts_us).collect();
        assert!(surviving_ts.contains(&100));
        assert!(surviving_ts.contains(&200));
        assert!(!surviving_ts.contains(&50));
    }

    // -- TEST 6 : merge-kan-bias-determinism -------------------------
    #[test]
    fn merge_kan_bias_is_deterministic() {
        let local = [10u32, 20, 30, 40, 50, 60, 70, 80];
        let peers = vec![
            [11u32, 21, 31, 41, 51, 61, 71, 81],
            [9u32, 19, 29, 39, 49, 59, 69, 79],
        ];
        let m1 = merge_kan_bias_shards(local, &peers);
        let m2 = merge_kan_bias_shards(local, &peers);
        let m3 = merge_kan_bias_shards(local, &peers);
        assert_eq!(m1, m2);
        assert_eq!(m2, m3);
    }

    // -- TEST 7 : weighted-average-bounds ----------------------------
    #[test]
    fn merge_kan_bias_stays_within_bounds() {
        // All-zero local + all-u32::MAX peers : merged < u32::MAX (local-vote majority).
        let local = [0u32; 8];
        let peers = vec![[u32::MAX; 8]; 3];
        let merged = merge_kan_bias_shards(local, &peers);
        for &b in &merged {
            // local_weight = 4 ; total_weight = 7 ; (0*4 + 3*MAX)/7 < MAX ; ✓
            assert!(b < u32::MAX);
        }
        // Empty peers -> identity.
        assert_eq!(merge_kan_bias_shards(local, &[]), local);
    }

    // -- TEST 8 : BTreeMap-deterministic-iter ------------------------
    #[test]
    fn peer_index_iteration_order_is_byte_stable() {
        let mut c = LocalPeerCache::new(16);
        // Add peers in mixed order ; BTreeMap MUST sort by peer-pubkey ascending.
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 1, 50));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 2, 10));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 3, 200));
        c.rebuild_index();
        let keys: Vec<[u8; 32]> = c.peer_index.keys().copied().collect();
        // Verify ascending key-byte order.
        for w in keys.windows(2) {
            assert!(w[0] < w[1], "peer_index iteration not byte-stable");
        }
    }

    // -- TEST 9 : cache-stats-correct --------------------------------
    #[test]
    fn cache_stats_returns_count_bytes_oldest_peer_count() {
        let mut c = LocalPeerCache::new(8);
        c.peer_add(fixed_peer(1));
        c.peer_add(fixed_peer(2));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 500, 1));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, 100, 2));
        let s = cache_stats(&c);
        assert_eq!(s.count, 2);
        assert_eq!(s.peer_count, 2);
        assert_eq!(s.oldest_us, 100);
        assert!(s.total_bytes > 0);
    }

    // -- TEST 10 : capacity-respected-after-evict --------------------
    #[test]
    fn capacity_respected_after_evict() {
        let mut c = LocalPeerCache::new(3);
        for i in 0..10u8 {
            c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0, u64::from(i) * 10, i));
        }
        assert_eq!(c.records.len(), 10);
        cache_evict_lru(&mut c);
        assert_eq!(c.records.len(), 3);
        // Survivors are the 3 highest-ts records (i = 7, 8, 9).
        let mut tss: Vec<u64> = c.records.iter().map(|r| r.ts_us).collect();
        tss.sort_unstable();
        assert_eq!(tss, vec![70, 80, 90]);
    }

    // -- TEST 11 : peer-permits-bitfield -----------------------------
    #[test]
    fn peer_permits_bitfield_works() {
        let mut p = fixed_peer(7);
        p.capabilities = 1 << (PeerCapsuleKind::KanBiasShard as u32);
        assert!(p.permits(PeerCapsuleKind::KanBiasShard));
        assert!(!p.permits(PeerCapsuleKind::HotfixClass));
        assert!(!p.permits(PeerCapsuleKind::OmegaFieldPatch));
    }

    // -- TEST 12 : kind-roundtrip-u8 ---------------------------------
    #[test]
    fn kind_u8_roundtrip_is_lossless() {
        for k in [
            PeerCapsuleKind::KanBiasShard,
            PeerCapsuleKind::OmegaFieldPatch,
            PeerCapsuleKind::SigmaMaskUpdate,
            PeerCapsuleKind::RecipeUnlock,
            PeerCapsuleKind::HotfixClass,
        ] {
            let b = k.as_u8();
            assert_eq!(PeerCapsuleKind::from_u8(b), Some(k));
        }
        assert_eq!(PeerCapsuleKind::from_u8(99), None);
    }

    // -- TEST 13 : clear-wipes-everything-no-trace -------------------
    #[test]
    fn clear_wipes_everything_no_trace() {
        let mut c = LocalPeerCache::new(8);
        c.peer_add(fixed_peer(1));
        c.push_record(fixed_record(PeerCapsuleKind::KanBiasShard, 0xDEAD, 1, 1));
        assert!(!c.records.is_empty());
        assert!(!c.peers.is_empty());
        c.clear();
        assert!(c.records.is_empty());
        assert!(c.peers.is_empty());
        assert!(c.peer_index.is_empty());
    }

    // -- TEST 14 : merge-kan-bias-empty-peers-is-identity ------------
    #[test]
    fn merge_kan_bias_with_no_peers_returns_local_identity() {
        let local = [42u32, 43, 44, 45, 46, 47, 48, 49];
        let merged = merge_kan_bias_shards(local, &[]);
        assert_eq!(merged, local);
    }

    // -- TEST 15 : record-vec-roundtrip-via-serde --------------------
    #[test]
    fn record_vec_roundtrip_via_serde_preserves_state() {
        // Note : full LocalPeerCache JSON-roundtrip is gated behind a
        // map-key-aware encoder (bincode / CBOR / postcard) — JSON's
        // string-key restriction makes BTreeMap<[u8; 32], _> unencodable
        // without a custom serializer. We test the records vector
        // separately ; cache state is reconstructed by replaying records.
        let recs = vec![
            fixed_record(PeerCapsuleKind::KanBiasShard, 0xAAAA, 100, 1),
            fixed_record(PeerCapsuleKind::HotfixClass, 0xBBBB, 200, 2),
        ];
        let json = serde_json::to_vec(&recs).expect("encode");
        let back: Vec<PeerCapsuleRecord> = serde_json::from_slice(&json).expect("decode");
        assert_eq!(recs, back);
    }
}
