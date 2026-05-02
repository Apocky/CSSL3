//! § manifest — apocky.com manifest-of-truth, JSON wire format.
//!
//! § DESIGN
//!   The manifest is a single JSON object listing the current canonical
//!   version per channel + a top-level signature tying the whole thing
//!   to a specific cap-key (typically cap-A or cap-D, depending on the
//!   release type).
//!
//! § CANONICAL BYTES FOR SIGNING
//!   `canonical_bytes_for_signing()` produces a stable byte sequence by
//!   serializing every field EXCEPT `signature` in a fixed order, with
//!   `BTreeMap`-backed maps for deterministic key-ordering. This is
//!   what gets Ed25519-signed.

use crate::bundle::SIGNATURE_BYTES;
use crate::cap::CapRole;
use crate::channel::Channel;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// § One row of the manifest, one per channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelEntry {
    /// Semver string `"M.m.p"`.
    pub current_version: String,
    /// Hex-encoded BLAKE3 of the bundle file (matches header.payload_blake3
    /// on extraction · note the header's blake3 is over PAYLOAD only ; this
    /// blake3 is over the WHOLE bundle file — used for download integrity).
    pub bundle_sha256: String,
    /// Effective-from epoch nanoseconds.
    pub effective_from_ns: u64,
    /// URL fragment (relative to /api/hotfix/download/) for this channel
    /// at this version, e.g. `cssl.bundle/1.2.3.csslfix`.
    pub download_path: String,
    /// Bytes of bundle file. Clients use this for range-request planning.
    pub size_bytes: u64,
}

/// § Revocation entry. Clients pull these on every manifest-poll and
/// uninstall any local bundle matching (channel, version).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationEntry {
    pub channel: Channel,
    pub version: String,
    pub ts_ns: u64,
    pub reason: String,
}

/// § Top-level manifest. Signed by a specific cap-key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u16,
    pub generated_at_ns: u64,
    pub signed_by: CapRole,
    pub channels: BTreeMap<Channel, ChannelEntry>,
    pub revocations: Vec<RevocationEntry>,
    /// Ed25519 signature over `canonical_bytes_for_signing()`.
    /// Hex-encoded for serde transport.
    #[serde(with = "hex_sig64")]
    pub signature: [u8; SIGNATURE_BYTES],
}

mod hex_sig64 {
    use crate::hex_lower;
    use serde::{Deserialize, Deserializer, Serializer};
    pub(super) fn serialize<S: Serializer>(b: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex_lower(b))
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        if s.len() != 128 {
            return Err(D::Error::custom("expected 128 hex chars for sig"));
        }
        let mut out = [0u8; 64];
        for (i, c) in s.as_bytes().chunks(2).enumerate() {
            let hex = std::str::from_utf8(c).map_err(D::Error::custom)?;
            out[i] = u8::from_str_radix(hex, 16).map_err(D::Error::custom)?;
        }
        Ok(out)
    }
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("json error : {0}")]
    Json(String),
    #[error("missing channel : {0}")]
    MissingChannel(&'static str),
}

impl Manifest {
    /// Canonical bytes used for signing : everything except the signature
    /// itself, serialized in a stable form.
    #[must_use]
    pub fn canonical_bytes_for_signing(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256 + 96 * self.channels.len());
        buf.extend_from_slice(b"CSFX-MANIFEST/v");
        buf.extend_from_slice(self.schema_version.to_string().as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(&self.generated_at_ns.to_le_bytes());
        buf.push(self.signed_by as u8);
        buf.push(b'\n');
        // BTreeMap iteration order is deterministic over Channel discriminants.
        for (chan, entry) in &self.channels {
            buf.push(*chan as u8);
            buf.push(b':');
            buf.extend_from_slice(entry.current_version.as_bytes());
            buf.push(b'|');
            buf.extend_from_slice(entry.bundle_sha256.as_bytes());
            buf.push(b'|');
            buf.extend_from_slice(&entry.effective_from_ns.to_le_bytes());
            buf.push(b'|');
            buf.extend_from_slice(entry.download_path.as_bytes());
            buf.push(b'|');
            buf.extend_from_slice(&entry.size_bytes.to_le_bytes());
            buf.push(b'\n');
        }
        buf.extend_from_slice(b"---REVOKES---\n");
        for r in &self.revocations {
            buf.push(r.channel as u8);
            buf.push(b':');
            buf.extend_from_slice(r.version.as_bytes());
            buf.push(b'|');
            buf.extend_from_slice(&r.ts_ns.to_le_bytes());
            buf.push(b'|');
            buf.extend_from_slice(r.reason.as_bytes());
            buf.push(b'\n');
        }
        buf
    }

    /// JSON serialize for HTTP transport.
    pub fn to_json(&self) -> Result<String, ManifestError> {
        serde_json::to_string(self).map_err(|e| ManifestError::Json(e.to_string()))
    }

    /// JSON deserialize from HTTP body.
    pub fn from_json(s: &str) -> Result<Self, ManifestError> {
        serde_json::from_str(s).map_err(|e| ManifestError::Json(e.to_string()))
    }

    /// Look up the entry for a given channel.
    pub fn entry(&self, ch: Channel) -> Option<&ChannelEntry> {
        self.channels.get(&ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Manifest {
        let mut channels = BTreeMap::new();
        channels.insert(
            Channel::SecurityPatch,
            ChannelEntry {
                current_version: "1.0.1".to_string(),
                bundle_sha256: "00".repeat(32),
                effective_from_ns: 1_700_000_000_000_000_000,
                download_path: "security.patch/1.0.1.csslfix".to_string(),
                size_bytes: 4096,
            },
        );
        Manifest {
            schema_version: 1,
            generated_at_ns: 1_700_000_000_000_000_001,
            signed_by: CapRole::CapD,
            channels,
            revocations: vec![],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn canonical_bytes_deterministic() {
        let m = fixture();
        let a = m.canonical_bytes_for_signing();
        let b = m.canonical_bytes_for_signing();
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_bytes_changes_with_field() {
        let m = fixture();
        let a = m.canonical_bytes_for_signing();
        let mut m2 = m;
        m2.generated_at_ns += 1;
        let b = m2.canonical_bytes_for_signing();
        assert_ne!(a, b);
    }

    #[test]
    fn json_roundtrip() {
        let m = fixture();
        let s = m.to_json().unwrap();
        let back = Manifest::from_json(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn entry_lookup() {
        let m = fixture();
        assert!(m.entry(Channel::SecurityPatch).is_some());
        assert!(m.entry(Channel::LoaBinary).is_none());
    }

    #[test]
    fn revocation_in_canonical_bytes() {
        let mut m = fixture();
        let a = m.canonical_bytes_for_signing();
        m.revocations.push(RevocationEntry {
            channel: Channel::SecurityPatch,
            version: "0.9.0".to_string(),
            ts_ns: 0,
            reason: "exploit-found".to_string(),
        });
        let b = m.canonical_bytes_for_signing();
        assert_ne!(a, b);
    }
}
