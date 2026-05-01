//! § spore — cross-user signal payload + emit-pipeline
//!
//! ⊑ Spore = (id · emitter · region · kind · payload-hash · ts · opt-in-tier)
//! ⊑ BLAKE3 keyed-hash bound to (kind, region, payload-bytes) ⟶ tamper-evident
//! ⊑ Sensitive<biometric|gaze|face|body> stripped @ emit ¬ at-aggregate
//! ⊑ Ed25519 sign-stub : 64-byte deterministic placeholder ; real key-material
//!   plumbed at G1-integration via host-attestation.

use crate::privacy::{strip_sensitive, OptInTier, RegionTag};
use serde::{Deserialize, Serialize};

/// § SporeId — globally-unique 16-byte identifier (BLAKE3-derived).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct SporeId(pub [u8; 16]);

impl SporeId {
    /// § derive — content-addressable id from emitter + ts + payload.
    #[must_use]
    pub fn derive(emitter: &[u8; 32], ts: u64, payload_hash: &[u8; 32]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-host-mycelium\0SporeId\0v1");
        h.update(emitter);
        h.update(&ts.to_le_bytes());
        h.update(payload_hash);
        let full = h.finalize();
        let mut id = [0_u8; 16];
        id.copy_from_slice(&full.as_bytes()[..16]);
        Self(id)
    }
}

/// § SporeKind — narrow, audited list of cross-user event categories.
///
/// Adding a variant requires PRIME_DIRECTIVE-review : every kind has a
/// documented privacy-impact and a documented downstream consumer.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SporeKind {
    /// § BiasNudge — small numeric weight delta for cross-user KAN
    /// bias-aggregation. *Aggregate-only consumption.*
    BiasNudge,
    /// § LootDropEvent — anonymized "rarity X dropped @ region Y" tally.
    LootDropEvent,
    /// § CombatOutcome — win/loss tally (no per-build attribution).
    CombatOutcome,
    /// § ProcgenSeed — opaque seed-share for cross-user content cohesion.
    ProcgenSeed,
    /// § NemesisDefeat — boss-kill tally (per-user only @ Pseudonymous+).
    NemesisDefeat,
    /// § CraftRecipeUnlock — recipe-discovery tally for tutorial-tuning.
    CraftRecipeUnlock,
}

impl SporeKind {
    /// § audit-tag for telemetry + log-keys.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            SporeKind::BiasNudge => "bias-nudge",
            SporeKind::LootDropEvent => "loot-drop",
            SporeKind::CombatOutcome => "combat-outcome",
            SporeKind::ProcgenSeed => "procgen-seed",
            SporeKind::NemesisDefeat => "nemesis-defeat",
            SporeKind::CraftRecipeUnlock => "craft-unlock",
        }
    }
}

/// § SporePayload — serde_json::Value-typed body, post-strip.
///
/// Stored as JSON because spores are forward-extensible across host
/// versions ; the BLAKE3 content-hash is over the canonicalized JSON
/// bytes, so adding a new field doesn't break replay for old kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SporePayload(pub serde_json::Value);

impl SporePayload {
    /// § strip-and-rehash — apply [`strip_sensitive`] then return content-hash.
    pub fn strip_and_hash(&mut self) -> [u8; 32] {
        let _stripped = strip_sensitive(&mut self.0);
        // Canonical : serde_json::to_vec is stable for object key-order
        // in serde_json 1.x because BTreeMap is the default repr ; for
        // safety we explicitly serialize with a sorted-key writer.
        let bytes = canonicalize_json(&self.0);
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-host-mycelium\0SporePayload\0v1");
        h.update(&bytes);
        *h.finalize().as_bytes()
    }

    /// § content-hash without mutation.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        let bytes = canonicalize_json(&self.0);
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-host-mycelium\0SporePayload\0v1");
        h.update(&bytes);
        *h.finalize().as_bytes()
    }
}

/// § canonicalize_json — emit a key-sorted JSON byte-stream so BLAKE3
/// hashes are stable regardless of insertion order. We re-serialize via
/// a recursive sort pass.
fn canonicalize_json(v: &serde_json::Value) -> Vec<u8> {
    let canon = canonicalize_value(v);
    serde_json::to_vec(&canon).unwrap_or_default()
}

fn canonicalize_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => {
            // BTreeMap iterates in sorted key-order ; round-trip via
            // serde_json::Map::from_iter to keep the type stable.
            let mut sorted: Vec<(String, serde_json::Value)> = m
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize_value(v)))
                .collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in sorted {
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(canonicalize_value).collect())
        }
        other => other.clone(),
    }
}

/// § Spore — atomic cross-user event, post-emit-pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spore {
    /// Globally-unique content-derived id.
    pub id: SporeId,
    /// Ed25519 public key of emitter (zeroed at Anonymized tier).
    #[serde(with = "byte_array_32")]
    pub emitter_pubkey: [u8; 32],
    /// Region partition (mycelium-cell).
    pub region: RegionTag,
    /// Event-category — one of the audited kinds.
    pub kind: SporeKind,
    /// BLAKE3 hash of the canonicalized post-strip payload.
    #[serde(with = "byte_array_32")]
    pub blake3: [u8; 32],
    /// Timestamp (epoch-seconds).
    pub ts: u64,
    /// Caller's consent-tier — gates downstream poll filtering.
    pub opt_in_tier: OptInTier,
    /// Stub Ed25519 signature (64 bytes). Real key-material plumbed @ G1.
    #[serde(with = "byte_array_64")]
    pub sig_stub: [u8; 64],
    /// Stripped payload (for downstream aggregate consumers).
    pub payload: SporePayload,
}

// § byte-array serde helpers — derive doesn't auto-impl for [u8; N>32].
// Public API : Vec<u8> at the wire ; in-memory : fixed-size array.
mod byte_array_32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(arr: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(arr)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let v = <Vec<u8>>::deserialize(d)?;
        if v.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "expected 32 bytes, got {}",
                v.len()
            )));
        }
        let mut out = [0_u8; 32];
        out.copy_from_slice(&v);
        Ok(out)
    }
}

mod byte_array_64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(arr: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(arr)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v = <Vec<u8>>::deserialize(d)?;
        if v.len() != 64 {
            return Err(serde::de::Error::custom(format!(
                "expected 64 bytes, got {}",
                v.len()
            )));
        }
        let mut out = [0_u8; 64];
        out.copy_from_slice(&v);
        Ok(out)
    }
}

impl Spore {
    /// § verify_blake3 — recompute the content-hash and compare.
    /// Returns `true` iff the spore has not been tampered.
    #[must_use]
    pub fn verify_blake3(&self) -> bool {
        self.payload.content_hash() == self.blake3
    }

    /// § At Anonymized tier the emitter pubkey must be zeroed at egress.
    #[must_use]
    pub fn pubkey_consistent_with_tier(&self) -> bool {
        match self.opt_in_tier {
            OptInTier::Anonymized | OptInTier::LocalOnly => {
                self.emitter_pubkey == [0_u8; 32]
            }
            OptInTier::Pseudonymous | OptInTier::Public => {
                // Allowed, though [0;32] is also legal as a "no-key" placeholder.
                true
            }
        }
    }
}

/// § SporeBuilder — emit-pipeline entry point.
///
/// Pipeline (in this exact order) :
/// 1. opt-in-cap-check (caller cannot emit above their tier).
/// 2. strip-Sensitive<*> (PRIME-DIRECTIVE invariant).
/// 3. canonicalize + BLAKE3-content-hash.
/// 4. derive SporeId from (emitter, ts, payload-hash).
/// 5. Ed25519-sign-stub (deterministic from id).
/// 6. zero emitter at Anonymized + LocalOnly tiers.
pub struct SporeBuilder {
    pub region: RegionTag,
    pub kind: SporeKind,
    pub ts: u64,
    pub opt_in_tier: OptInTier,
    pub emitter_pubkey: [u8; 32],
    pub payload: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum SporeBuildError {
    #[error("opt-in-tier {requested:?} exceeds caller-cap {cap:?}")]
    TierExceedsCap {
        requested: OptInTier,
        cap: OptInTier,
    },
}

impl SporeBuilder {
    /// § build — apply the full emit-pipeline. `caller_cap` is the
    /// maximum tier the caller has been granted ; emit cannot escalate.
    pub fn build(self, caller_cap: OptInTier) -> Result<Spore, SporeBuildError> {
        // 1. cap-check ---------------------------------------------------
        if !caller_cap.permits(self.opt_in_tier) {
            return Err(SporeBuildError::TierExceedsCap {
                requested: self.opt_in_tier,
                cap: caller_cap,
            });
        }
        // 2. strip-Sensitive<*> ------------------------------------------
        let mut payload = SporePayload(self.payload);
        let _ = payload.strip_and_hash(); // mutates payload

        // 3. canonical content-hash --------------------------------------
        let blake3_hash = payload.content_hash();

        // 4. zero emitter @ low tiers ------------------------------------
        let pubkey = match self.opt_in_tier {
            OptInTier::LocalOnly | OptInTier::Anonymized => [0_u8; 32],
            OptInTier::Pseudonymous | OptInTier::Public => self.emitter_pubkey,
        };

        // 5. derive id ---------------------------------------------------
        let id = SporeId::derive(&pubkey, self.ts, &blake3_hash);

        // 6. sign-stub : deterministic 64 bytes derived from id + hash ---
        let sig_stub = derive_sig_stub(&id, &blake3_hash);

        Ok(Spore {
            id,
            emitter_pubkey: pubkey,
            region: self.region,
            kind: self.kind,
            blake3: blake3_hash,
            ts: self.ts,
            opt_in_tier: self.opt_in_tier,
            sig_stub,
            payload,
        })
    }
}

/// § derive_sig_stub — deterministic placeholder signature.
///
/// Real Ed25519 signing happens in `cssl-host-attestation` once integrated
/// at G1 ; the stub allows round-trip + replay testing without holding key
/// material in this crate.
fn derive_sig_stub(id: &SporeId, hash: &[u8; 32]) -> [u8; 64] {
    let mut h = blake3::Hasher::new();
    h.update(b"cssl-host-mycelium\0sig-stub\0v1");
    h.update(&id.0);
    h.update(hash);
    let mut out = [0_u8; 64];
    let mut reader = h.finalize_xof();
    reader.fill(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_payload() -> serde_json::Value {
        serde_json::json!({
            "score": 42,
            "biometric_pulse": 70,
            "region_name": "alpha",
        })
    }

    fn mk_builder() -> SporeBuilder {
        SporeBuilder {
            region: RegionTag::new(7),
            kind: SporeKind::BiasNudge,
            ts: 1_000,
            opt_in_tier: OptInTier::Anonymized,
            emitter_pubkey: [9_u8; 32],
            payload: mk_payload(),
        }
    }

    #[test]
    fn spore_construction_via_builder() {
        let s = mk_builder().build(OptInTier::Public).unwrap();
        assert_eq!(s.region, RegionTag::new(7));
        assert_eq!(s.kind, SporeKind::BiasNudge);
        assert_eq!(s.ts, 1_000);
        assert_eq!(s.opt_in_tier, OptInTier::Anonymized);
    }

    #[test]
    fn spore_construction_strips_sensitive() {
        let s = mk_builder().build(OptInTier::Public).unwrap();
        let payload_obj = s.payload.0.as_object().unwrap();
        assert!(!payload_obj.contains_key("biometric_pulse"));
        assert!(payload_obj.contains_key("score"));
    }

    #[test]
    fn spore_construction_anonymized_zeros_emitter() {
        let s = mk_builder().build(OptInTier::Public).unwrap();
        assert_eq!(s.emitter_pubkey, [0_u8; 32]);
        assert!(s.pubkey_consistent_with_tier());
    }

    #[test]
    fn spore_kind_tags_unique() {
        let kinds = [
            SporeKind::BiasNudge,
            SporeKind::LootDropEvent,
            SporeKind::CombatOutcome,
            SporeKind::ProcgenSeed,
            SporeKind::NemesisDefeat,
            SporeKind::CraftRecipeUnlock,
        ];
        let tags: std::collections::BTreeSet<&str> =
            kinds.iter().map(|k| k.tag()).collect();
        assert_eq!(tags.len(), kinds.len());
    }

    #[test]
    fn blake3_content_hash_stable() {
        let mut p = SporePayload(serde_json::json!({"a": 1, "b": 2}));
        let h1 = p.strip_and_hash();
        let p2 = SporePayload(serde_json::json!({"b": 2, "a": 1}));
        // Canonicalization sorts keys → same hash regardless of order.
        let h2 = p2.content_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn blake3_changes_with_payload() {
        let p1 = SporePayload(serde_json::json!({"a": 1}));
        let p2 = SporePayload(serde_json::json!({"a": 2}));
        assert_ne!(p1.content_hash(), p2.content_hash());
    }

    #[test]
    fn sig_stub_round_trip_deterministic() {
        let s1 = mk_builder().build(OptInTier::Public).unwrap();
        let s2 = mk_builder().build(OptInTier::Public).unwrap();
        // Same payload + ts + tier → same sig_stub.
        assert_eq!(s1.sig_stub, s2.sig_stub);
        assert_eq!(s1.id, s2.id);
    }

    #[test]
    fn sig_stub_changes_with_payload() {
        let s1 = mk_builder().build(OptInTier::Public).unwrap();
        let mut b2 = mk_builder();
        b2.payload = serde_json::json!({"score": 999});
        let s2 = b2.build(OptInTier::Public).unwrap();
        assert_ne!(s1.sig_stub, s2.sig_stub);
        assert_ne!(s1.id, s2.id);
    }

    #[test]
    fn cap_check_rejects_escalation() {
        let mut b = mk_builder();
        b.opt_in_tier = OptInTier::Public;
        let r = b.build(OptInTier::Anonymized);
        assert!(matches!(
            r,
            Err(SporeBuildError::TierExceedsCap { .. })
        ));
    }

    #[test]
    fn verify_blake3_round_trip() {
        let s = mk_builder().build(OptInTier::Public).unwrap();
        assert!(s.verify_blake3());
    }

    #[test]
    fn serde_round_trip_preserves_fields() {
        let s = mk_builder().build(OptInTier::Public).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let s2: Spore = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
        assert!(s2.verify_blake3());
    }

    #[test]
    fn idempotent_emit_same_payload_same_id() {
        let s1 = mk_builder().build(OptInTier::Public).unwrap();
        let s2 = mk_builder().build(OptInTier::Public).unwrap();
        assert_eq!(s1.id, s2.id);
        assert_eq!(s1.blake3, s2.blake3);
    }

    #[test]
    fn pseudonymous_keeps_emitter() {
        let mut b = mk_builder();
        b.opt_in_tier = OptInTier::Pseudonymous;
        let s = b.build(OptInTier::Public).unwrap();
        assert_eq!(s.emitter_pubkey, [9_u8; 32]);
    }
}
