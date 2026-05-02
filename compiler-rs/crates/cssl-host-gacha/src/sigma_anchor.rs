// § sigma_anchor.rs — Σ-Chain anchor every-pull (+ every-refund)
// ════════════════════════════════════════════════════════════════════
// § ROLE : produce a deterministic anchor-payload for Σ-Chain (cssl-host-
//   sigma-chain). Every pull AND every refund is anchored for attribution-
//   immutable-history. The host crate (cssl-host-sigma-chain) consumes
//   `SigmaAnchor` and emits the on-chain commitment ; this crate only
//   produces the payload (decoupled testing + clear ownership).
//
// § DETERMINISTIC : SigmaAnchor is hashed via blake3 over the canonical
//   serialization (BTreeMap-ordered fields). Anchor-id is the first 16
//   hex-chars of the blake3 hash (sufficient uniqueness for in-game
//   timeline).
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// § SIGMA_ANCHOR_VERSION — schema-version for the anchor payload.
/// Bump whenever the canonical-serialization changes.
pub const SIGMA_ANCHOR_VERSION: u32 = 1;

/// § SigmaAnchor — single payload-row for Σ-Chain. One per pull · one per refund.
///
/// The downstream cssl-host-sigma-chain consumes this and emits a Coherence-
/// Proof-bound on-chain row · stage-0 stores the BLAKE3 hash + serialized
/// payload in `gacha_pulls.sigma_anchor` (TEXT column).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigmaAnchor {
    pub version: u32,
    pub kind: SigmaAnchorKind,
    /// Player-pubkey hex-encoded (32-byte Ed25519 ⇒ 64 chars).
    pub player_pubkey_hex: String,
    pub banner_id: String,
    pub pull_id: String,
    pub ts_epoch_secs: u64,
    /// Hex-encoded blake3 hash of the canonical serialization.
    pub anchor_id_hex: String,
    /// JSON-serialized payload (deterministic via BTreeMap-style serde).
    pub payload_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigmaAnchorKind {
    PullAnchor,
    RefundAnchor,
}

impl SigmaAnchor {
    /// Construct a pull-anchor. `payload_json` is the serialized PullOutcome (or
    /// PullResult for bundle-anchors). The `anchor_id_hex` is computed from the
    /// concatenation of (kind || pubkey || banner || pull_id || ts || payload).
    pub fn pull_anchor(
        player_pubkey: &[u8],
        banner_id: &str,
        pull_id: &str,
        ts_epoch_secs: u64,
        payload_json: String,
    ) -> Result<Self, SigmaAnchorErr> {
        Self::build(
            SigmaAnchorKind::PullAnchor,
            player_pubkey,
            banner_id,
            pull_id,
            ts_epoch_secs,
            payload_json,
        )
    }

    /// Construct a refund-anchor. `payload_json` is the serialized RefundOutcome.
    pub fn refund_anchor(
        player_pubkey: &[u8],
        banner_id: &str,
        pull_id: &str,
        ts_epoch_secs: u64,
        payload_json: String,
    ) -> Result<Self, SigmaAnchorErr> {
        Self::build(
            SigmaAnchorKind::RefundAnchor,
            player_pubkey,
            banner_id,
            pull_id,
            ts_epoch_secs,
            payload_json,
        )
    }

    fn build(
        kind: SigmaAnchorKind,
        player_pubkey: &[u8],
        banner_id: &str,
        pull_id: &str,
        ts_epoch_secs: u64,
        payload_json: String,
    ) -> Result<Self, SigmaAnchorErr> {
        if player_pubkey.is_empty() {
            return Err(SigmaAnchorErr::EmptyPubkey);
        }
        if banner_id.is_empty() || pull_id.is_empty() {
            return Err(SigmaAnchorErr::EmptyId);
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(&SIGMA_ANCHOR_VERSION.to_le_bytes());
        // Domain-separated kind tag.
        let kind_tag: &[u8] = match kind {
            SigmaAnchorKind::PullAnchor => b"|kind=pull|",
            SigmaAnchorKind::RefundAnchor => b"|kind=refund|",
        };
        hasher.update(kind_tag);
        hasher.update(player_pubkey);
        hasher.update(b"|banner=");
        hasher.update(banner_id.as_bytes());
        hasher.update(b"|pull=");
        hasher.update(pull_id.as_bytes());
        hasher.update(b"|ts=");
        hasher.update(&ts_epoch_secs.to_le_bytes());
        hasher.update(b"|payload=");
        hasher.update(payload_json.as_bytes());
        let hash = hasher.finalize();
        // First 16 bytes (32 hex chars) is sufficient · matches Stripe-id
        // shape and stage-0's anchor-id convention.
        let anchor_id_hex = hex_first_n(hash.as_bytes(), 16);

        Ok(Self {
            version: SIGMA_ANCHOR_VERSION,
            kind,
            player_pubkey_hex: hex_first_n(player_pubkey, player_pubkey.len()),
            banner_id: banner_id.to_string(),
            pull_id: pull_id.to_string(),
            ts_epoch_secs,
            anchor_id_hex,
            payload_json,
        })
    }
}

/// § SigmaAnchorErr — public error-enum.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SigmaAnchorErr {
    #[error("empty pubkey")]
    EmptyPubkey,
    #[error("empty banner_id or pull_id")]
    EmptyId,
}

/// Hex-encode first N bytes (lowercase). Stage-0 manual impl avoids
/// pulling `hex` crate.
fn hex_first_n(bytes: &[u8], n: usize) -> String {
    let take = bytes.len().min(n);
    let mut out = String::with_capacity(take * 2);
    for &b in &bytes[..take] {
        out.push(hex_nibble(b >> 4));
        out.push(hex_nibble(b & 0x0F));
    }
    out
}

const fn hex_nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_anchor_includes_kind_separation() {
        let p = SigmaAnchor::pull_anchor(
            b"pubkey32-bytes-fixed-content!!!!",
            "banner-A",
            "pull-001",
            1_700_000_000,
            "{\"rarity\":\"rare\"}".into(),
        )
        .unwrap();
        let r = SigmaAnchor::refund_anchor(
            b"pubkey32-bytes-fixed-content!!!!",
            "banner-A",
            "pull-001",
            1_700_000_000,
            "{\"rarity\":\"rare\"}".into(),
        )
        .unwrap();
        // Different kinds MUST hash differently even with identical other-fields.
        assert_ne!(p.anchor_id_hex, r.anchor_id_hex);
    }

    #[test]
    fn anchor_id_is_32_hex_chars() {
        let a = SigmaAnchor::pull_anchor(
            b"k",
            "b",
            "p",
            1,
            "{}".into(),
        )
        .unwrap();
        assert_eq!(a.anchor_id_hex.len(), 32);
        // hex characters only
        assert!(a.anchor_id_hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn deterministic_anchor_id() {
        let a = SigmaAnchor::pull_anchor(
            b"pubkey",
            "banner-A",
            "pull-001",
            1_700_000_000,
            "{\"rarity\":\"common\"}".into(),
        )
        .unwrap();
        let b = SigmaAnchor::pull_anchor(
            b"pubkey",
            "banner-A",
            "pull-001",
            1_700_000_000,
            "{\"rarity\":\"common\"}".into(),
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_pubkey_rejects() {
        let err = SigmaAnchor::pull_anchor(b"", "b", "p", 1, "{}".into()).unwrap_err();
        assert!(matches!(err, SigmaAnchorErr::EmptyPubkey));
    }

    #[test]
    fn empty_banner_or_pull_rejects() {
        assert!(matches!(
            SigmaAnchor::pull_anchor(b"k", "", "p", 1, "{}".into()),
            Err(SigmaAnchorErr::EmptyId)
        ));
        assert!(matches!(
            SigmaAnchor::pull_anchor(b"k", "b", "", 1, "{}".into()),
            Err(SigmaAnchorErr::EmptyId)
        ));
    }

    #[test]
    fn version_const_exposed() {
        assert_eq!(SIGMA_ANCHOR_VERSION, 1);
        let a = SigmaAnchor::pull_anchor(b"k", "b", "p", 1, "{}".into()).unwrap();
        assert_eq!(a.version, SIGMA_ANCHOR_VERSION);
    }

    #[test]
    fn payload_change_changes_anchor_id() {
        let a = SigmaAnchor::pull_anchor(b"k", "b", "p", 1, "{\"x\":1}".into()).unwrap();
        let b = SigmaAnchor::pull_anchor(b"k", "b", "p", 1, "{\"x\":2}".into()).unwrap();
        assert_ne!(a.anchor_id_hex, b.anchor_id_hex);
    }
}
