// § attribution.rs · author-pubkey + AttributionLedger
// § eternal-attribution one-time-per (author · scene-name) · NEVER-revoked

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 32-byte Ed25519 public-key newtype (canonical fixed-width).
///
/// Serializes as a 64-char hex-string · this allows use as JSON-map-key
/// (BTreeMap<AuthorPubkey, _>) which is required for ledger serde round-trip.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AuthorPubkey(pub [u8; 32]);

impl AuthorPubkey {
    /// Construct from raw 32 bytes.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Canonical bytes (used for BLAKE3-content-hash).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex-string representation (lowercase, 64 chars).
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// Parse from 64-char hex-string.
    ///
    /// # Errors
    /// Returns an error message string if hex-decoding fails.
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        if hex.len() != 64 {
            return Err(format!("expected 64 hex chars, got {}", hex.len()));
        }
        let mut out = [0u8; 32];
        for i in 0..32 {
            let pair = &hex[i * 2..i * 2 + 2];
            out[i] =
                u8::from_str_radix(pair, 16).map_err(|e| format!("hex parse @ {i}: {e}"))?;
        }
        Ok(Self(out))
    }
}

impl fmt::Display for AuthorPubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for AuthorPubkey {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for AuthorPubkey {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// Tracks which (author, scene-name) pairs have already-claimed
/// `EternalAttribution` · enforces one-time-permanence (spec/18 line 62).
///
/// Eternal-attribution NEVER-revoked · NEVER-removed · t∞.
///
/// Serializes via tuple-list shim (JSON-spec forbids non-string-keys).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttributionLedger {
    /// (author, scene-name) → first imprint-id that claimed eternal.
    #[serde(with = "tuple_map_shim")]
    eternal_claims: BTreeMap<(AuthorPubkey, String), super::imprint::ImprintId>,
    /// Distinct (author, scene-name) pairs claimed eternal · for fast rejection
    /// without scanning the map.
    claimed_pairs: BTreeSet<(AuthorPubkey, String)>,
}

mod tuple_map_shim {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::AuthorPubkey;

    pub(super) fn serialize<S: Serializer, V: Serialize>(
        m: &BTreeMap<(AuthorPubkey, String), V>,
        ser: S,
    ) -> Result<S::Ok, S::Error> {
        let v: Vec<(&AuthorPubkey, &String, &V)> = m.iter().map(|((a, s), v)| (a, s, v)).collect();
        v.serialize(ser)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        de: D,
    ) -> Result<BTreeMap<(AuthorPubkey, String), V>, D::Error> {
        let v: Vec<(AuthorPubkey, String, V)> = Vec::deserialize(de)?;
        Ok(v.into_iter().map(|(a, s, val)| ((a, s), val)).collect())
    }
}

impl AttributionLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` iff this (author, scene-name) pair has already claimed eternal.
    #[must_use]
    pub fn has_claimed(&self, author: &AuthorPubkey, scene_name: &str) -> bool {
        self.claimed_pairs
            .contains(&(*author, scene_name.to_owned()))
    }

    /// Record a one-time eternal-claim · returns `false` if pair was already
    /// claimed (caller should then return `AlreadyOwnedEternal`).
    pub fn try_claim(
        &mut self,
        author: AuthorPubkey,
        scene_name: &str,
        imprint_id: super::imprint::ImprintId,
    ) -> bool {
        let key = (author, scene_name.to_owned());
        if self.claimed_pairs.contains(&key) {
            return false;
        }
        self.claimed_pairs.insert(key.clone());
        self.eternal_claims.insert(key, imprint_id);
        true
    }

    /// Return imprint-id of the original eternal-claim, if any.
    #[must_use]
    pub fn original_claim(
        &self,
        author: &AuthorPubkey,
        scene_name: &str,
    ) -> Option<super::imprint::ImprintId> {
        self.eternal_claims
            .get(&(*author, scene_name.to_owned()))
            .copied()
    }

    #[must_use]
    pub fn claim_count(&self) -> usize {
        self.claimed_pairs.len()
    }
}
