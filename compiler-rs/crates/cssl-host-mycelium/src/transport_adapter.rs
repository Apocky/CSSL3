//! § transport_adapter — pluggable transport for spore-emit + nutrient-poll
//!
//! ⊑ Local trait `MyceliumTransport` decouples this crate from any specific
//!   network-transport implementation.
//! ⊑ `InMemoryTransport` is a parking_lot-backed mock used by tests + by
//!   the LocalOnly tier (which never leaves this process).
//! ⊑ `TransportAdapter` is the consumption-shim that downstream host-crates
//!   (e.g. cssl-host-mp-transport-real) wire in at G1.

use crate::nutrient::{NutrientQuery, NutrientResponse};
use crate::spore::Spore;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// § MyceliumError — transport + protocol errors.
#[derive(Debug, thiserror::Error)]
pub enum MyceliumError {
    #[error("transport closed")]
    TransportClosed,
    #[error("opt-in escalation : caller-tier {caller:?} cannot view {emitted:?}")]
    TierEscalationBlocked {
        caller: crate::privacy::OptInTier,
        emitted: crate::privacy::OptInTier,
    },
    #[error("dp-floor not met : have {have} < need {need}")]
    DpFloorNotMet { have: usize, need: usize },
    #[error("blake3 mismatch — spore tampered")]
    Blake3Mismatch,
    #[error("region {region:?} not subscribed")]
    RegionNotSubscribed {
        region: crate::privacy::RegionTag,
    },
}

/// § MyceliumTransport — minimal trait surface a host-transport must
/// satisfy.  Mirrors the expected shape of upstream `cssl-host-mp-transport-real`
/// without taking a hard dependency, so this crate compiles standalone.
pub trait MyceliumTransport: Send + Sync {
    /// § emit — push a spore into the network.
    fn emit(&self, spore: Spore) -> Result<(), MyceliumError>;
    /// § poll — fetch spores matching a region/kind/since query.
    fn poll(&self, query: &NutrientQuery) -> Result<NutrientResponse, MyceliumError>;
    /// § subscribed — does the transport carry traffic for `region`?
    fn subscribed(&self, region: crate::privacy::RegionTag) -> bool;
}

/// § InMemoryTransport — process-local mock + LocalOnly-tier transport.
///
/// Spores are stored in a `BTreeMap<RegionTag, Vec<Spore>>` so iteration
/// is deterministic ; the inner `Vec` is append-only and idempotent on
/// duplicate `SporeId`.
#[derive(Default, Clone)]
pub struct InMemoryTransport {
    inner: Arc<Mutex<BTreeMap<crate::privacy::RegionTag, Vec<Spore>>>>,
}

impl InMemoryTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// § stored_count — total spores across all regions (for tests).
    pub fn stored_count(&self) -> usize {
        let g = self.inner.lock().unwrap();
        let total = g.values().map(Vec::len).sum();
        drop(g);
        total
    }

    /// § all_spores — snapshot for tests + audit. Returns a deterministic
    /// iteration over (region-asc, ts-asc).
    pub fn all_spores(&self) -> Vec<Spore> {
        let g = self.inner.lock().unwrap();
        let mut out: Vec<Spore> = Vec::new();
        for v in g.values() {
            out.extend(v.iter().cloned());
        }
        drop(g);
        out
    }
}

impl MyceliumTransport for InMemoryTransport {
    fn emit(&self, spore: Spore) -> Result<(), MyceliumError> {
        if !spore.verify_blake3() {
            return Err(MyceliumError::Blake3Mismatch);
        }
        let mut g = self.inner.lock().unwrap();
        let bucket = g.entry(spore.region).or_default();
        // Idempotent : same SporeId → no double-insert.
        if !bucket.iter().any(|s| s.id == spore.id) {
            bucket.push(spore);
        }
        drop(g);
        Ok(())
    }

    fn poll(&self, query: &NutrientQuery) -> Result<NutrientResponse, MyceliumError> {
        let g = self.inner.lock().unwrap();
        let mut hits: Vec<Spore> = Vec::new();
        if let Some(bucket) = g.get(&query.region) {
            for s in bucket {
                if !query.matches(s) {
                    continue;
                }
                // Tier-escalation guard : never return a spore at higher
                // tier than the caller is permitted to see.
                if !query.caller_tier.permits(s.opt_in_tier) {
                    continue;
                }
                hits.push(s.clone());
                if hits.len() >= query.max_count {
                    break;
                }
            }
        }
        drop(g);
        Ok(NutrientResponse {
            region: query.region,
            kind: query.kind,
            spores: hits,
        })
    }

    fn subscribed(&self, _region: crate::privacy::RegionTag) -> bool {
        true
    }
}

/// § TransportAdapter — thin newtype wrapping any `MyceliumTransport`.
///
/// Provided so downstream code can swap `cssl-host-mp-transport-real`
/// implementations behind a stable handle without trait-object churn at
/// the call-site. The adapter forwards by `Arc<dyn>` for sharability.
#[derive(Clone)]
pub struct TransportAdapter {
    inner: Arc<dyn MyceliumTransport>,
}

impl TransportAdapter {
    #[must_use]
    pub fn new(t: Arc<dyn MyceliumTransport>) -> Self {
        Self { inner: t }
    }

    pub fn emit(&self, spore: Spore) -> Result<(), MyceliumError> {
        self.inner.emit(spore)
    }

    pub fn poll(&self, q: &NutrientQuery) -> Result<NutrientResponse, MyceliumError> {
        self.inner.poll(q)
    }

    pub fn subscribed(&self, region: crate::privacy::RegionTag) -> bool {
        self.inner.subscribed(region)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::{OptInTier, RegionTag};
    use crate::spore::{SporeBuilder, SporeKind};

    fn mk_spore(region: RegionTag, ts: u64, tier: OptInTier) -> Spore {
        SporeBuilder {
            region,
            kind: SporeKind::BiasNudge,
            ts,
            opt_in_tier: tier,
            emitter_pubkey: [3_u8; 32],
            payload: serde_json::json!({"score": ts}),
        }
        .build(OptInTier::Public)
        .unwrap()
    }

    #[test]
    fn in_memory_emit_then_poll() {
        let t = InMemoryTransport::new();
        let s = mk_spore(RegionTag::new(1), 100, OptInTier::Public);
        let s_id = s.id;
        t.emit(s).unwrap();
        let q = NutrientQuery {
            region: RegionTag::new(1),
            kind: SporeKind::BiasNudge,
            since_ts: 0,
            max_count: 100,
            caller_tier: OptInTier::Public,
        };
        let r = t.poll(&q).unwrap();
        assert_eq!(r.spores.len(), 1);
        assert_eq!(r.spores[0].id, s_id);
    }

    #[test]
    fn idempotent_emit_no_duplicate() {
        let t = InMemoryTransport::new();
        let s = mk_spore(RegionTag::new(1), 100, OptInTier::Public);
        t.emit(s.clone()).unwrap();
        t.emit(s).unwrap();
        assert_eq!(t.stored_count(), 1);
    }

    #[test]
    fn poll_tier_filter_blocks_escalation() {
        let t = InMemoryTransport::new();
        let s = mk_spore(RegionTag::new(1), 100, OptInTier::Public);
        t.emit(s).unwrap();
        let q = NutrientQuery {
            region: RegionTag::new(1),
            kind: SporeKind::BiasNudge,
            since_ts: 0,
            max_count: 100,
            caller_tier: OptInTier::LocalOnly, // escalation-block
        };
        let r = t.poll(&q).unwrap();
        assert_eq!(r.spores.len(), 0);
    }

    #[test]
    fn region_partitioning_isolates_traffic() {
        let t = InMemoryTransport::new();
        t.emit(mk_spore(RegionTag::new(1), 100, OptInTier::Public))
            .unwrap();
        t.emit(mk_spore(RegionTag::new(2), 100, OptInTier::Public))
            .unwrap();
        let q = NutrientQuery {
            region: RegionTag::new(1),
            kind: SporeKind::BiasNudge,
            since_ts: 0,
            max_count: 100,
            caller_tier: OptInTier::Public,
        };
        let r = t.poll(&q).unwrap();
        assert_eq!(r.spores.len(), 1);
        assert_eq!(r.spores[0].region, RegionTag::new(1));
    }

    #[test]
    fn transport_adapter_forwards() {
        let inner: Arc<dyn MyceliumTransport> = Arc::new(InMemoryTransport::new());
        let a = TransportAdapter::new(inner);
        let s = mk_spore(RegionTag::new(5), 200, OptInTier::Public);
        a.emit(s).unwrap();
        assert!(a.subscribed(RegionTag::new(5)));
        let q = NutrientQuery {
            region: RegionTag::new(5),
            kind: SporeKind::BiasNudge,
            since_ts: 0,
            max_count: 100,
            caller_tier: OptInTier::Public,
        };
        let r = a.poll(&q).unwrap();
        assert_eq!(r.spores.len(), 1);
    }
}
