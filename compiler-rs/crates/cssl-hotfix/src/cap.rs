//! § cap — the 5 cap-key roles (cap-A..cap-E) that sign bundles + manifests.
//!
//! § PROGRESSIVE PRIVILEGE MODEL
//!   cap-A — `loa.binary` only             — RAREST · cold-storage
//!   cap-B — `cssl.bundle` only            — language-runtime release
//!   cap-C — `kan.weights` + `balance.config` — model + game-balance
//!   cap-D — `security.patch` only         — incident-response
//!   cap-E — `recipe.book` + `nemesis.bestiary` + `storylet.content` + `render.pipeline`
//!                                         — content / cosmetic-only
//!
//! Rotation policy lives in `specs/26b_HOTFIX_KEYS.csl`. Public keys are
//! shipped with LoA.exe at install-time and are upgrade-pinned : a
//! manifest claiming a new public key is REJECTED unless co-signed by the
//! prior key (chain-of-trust rotation).
//!
//! Private keys live ONLY in `~/.loa-secrets/` on Apocky's machine and are
//! NEVER committed to source control. `cssl-hotfix-client` carries only
//! the public-key array (`[u8; 32] × 5`) compiled-in.

use serde::{Deserialize, Serialize};

/// § The 5 cap-key roles. `repr(u8)` = stable wire byte for bundle headers.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum CapRole {
    CapA = 1,
    CapB = 2,
    CapC = 3,
    CapD = 4,
    CapE = 5,
}

/// Stable iteration order ; used for tests + manifest schema validation.
pub const CAP_KEYS: [CapRole; 5] = [
    CapRole::CapA,
    CapRole::CapB,
    CapRole::CapC,
    CapRole::CapD,
    CapRole::CapE,
];

impl CapRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapA => "cap-A",
            Self::CapB => "cap-B",
            Self::CapC => "cap-C",
            Self::CapD => "cap-D",
            Self::CapE => "cap-E",
        }
    }

    #[must_use]
    pub fn from_role_str(s: &str) -> Option<Self> {
        match s {
            "cap-A" => Some(Self::CapA),
            "cap-B" => Some(Self::CapB),
            "cap-C" => Some(Self::CapC),
            "cap-D" => Some(Self::CapD),
            "cap-E" => Some(Self::CapE),
            _ => None,
        }
    }
}

/// § A cap-key as carried in the client : just a 32-byte Ed25519 public-key
/// + the role-tag. The PRIVATE key never appears in the client crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapKey {
    pub role: CapRole,
    /// Ed25519 verifying-key bytes. Hex-encoded for serde.
    #[serde(with = "hex_pubkey")]
    pub pubkey: [u8; 32],
}

mod hex_pubkey {
    use crate::hex_lower;
    use serde::{Deserialize, Deserializer, Serializer};
    pub(super) fn serialize<S: Serializer>(b: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex_lower(b))
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        if s.len() != 64 {
            return Err(D::Error::custom(format!(
                "expected 64 hex chars, got {}",
                s.len()
            )));
        }
        let mut out = [0u8; 32];
        for (i, c) in s.as_bytes().chunks(2).enumerate() {
            let hex = std::str::from_utf8(c).map_err(D::Error::custom)?;
            out[i] = u8::from_str_radix(hex, 16).map_err(D::Error::custom)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_const_is_five_distinct() {
        assert_eq!(CAP_KEYS.len(), 5);
        for (i, r) in CAP_KEYS.iter().enumerate() {
            assert_eq!(*r as u8, (i as u8) + 1);
        }
    }

    #[test]
    fn cap_role_str_roundtrip() {
        for r in CAP_KEYS {
            assert_eq!(CapRole::from_role_str(r.as_str()), Some(r));
        }
    }

    #[test]
    fn cap_key_serde_roundtrip() {
        let k = CapKey {
            role: CapRole::CapD,
            pubkey: [0x42; 32],
        };
        let s = serde_json::to_string(&k).unwrap();
        assert!(s.contains("cap-D") || s.contains("CapD"));
        let back: CapKey = serde_json::from_str(&s).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn cap_key_pubkey_hex_short_rejected() {
        let bad_json = r#"{"role":"CapA","pubkey":"abcd"}"#;
        let r: Result<CapKey, _> = serde_json::from_str(bad_json);
        assert!(r.is_err());
    }
}
