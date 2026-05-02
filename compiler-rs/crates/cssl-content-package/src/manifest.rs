//! § manifest — `.ccpkg` manifest schema (JSON wire format).
//!
//! § DESIGN
//!   The manifest is a single JSON object describing the package's identity,
//!   provenance, dependencies, and Σ-mask audience-class. Embedded inside the
//!   `.ccpkg` bundle (NOT a separate file) and signed atomically with the
//!   payload-archive.
//!
//! § SCHEMA
//!   {
//!     id              : String           — globally-unique slug (e.g. "loa.scenes.darkforest")
//!     version         : String           — semver "M.m.p"
//!     kind            : ContentKind      — scene | npc | recipe | lore | system | ...
//!     author_pubkey   : [u8; 32]         — Ed25519 public-key (hex-32 in JSON)
//!     name            : String           — human-readable display name
//!     description     : String           — markdown-allowed description
//!     depends_on      : Vec<Dependency>  — recursive cross-package dependencies
//!     remix_of        : Option<RemixAttribution> — upstream attribution chain
//!     tags            : Vec<String>      — discovery tags (case-folded internally)
//!     sigma_mask      : u64              — bit-packed audience-class flags
//!     gift_economy_only : bool           — DEFAULT TRUE. Pay-for-power = structural BAN.
//!     license         : LicenseTier      — A-OPEN | B-PROPRIETARY | C-SERVER | D-PRIVATE | E-PROTOCOL
//!   }
//!
//! § AXIOM (cosmetic-only)
//!   `gift_economy_only = true` is the DEFAULT and CANNOT be set to `false`
//!   for any kind that grants in-game-power (System / Recipe / Npc with
//!   stats). Cosmetic-only kinds (ShaderPack / AudioPack / Lore) MAY opt-in
//!   to monetisation via the LicenseTier dimension. (See cosmetic-only-axiom
//!   memory-card · Apocky-mandate.)

use crate::hex_lower;
use crate::kind::ContentKind;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// § The 5 license tiers from spec/grand-vision/17_DISTRIBUTION.csl.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum LicenseTier {
    /// A: open · MIT/AGPL/CC-licensed · freely fork-able.
    Open = 1,
    /// B: proprietary · all-rights-reserved · cosmetic-only-monetisable.
    Proprietary = 2,
    /// C: server-side · gameplay-services protected · client open.
    Server = 3,
    /// D: private · personal-use only · not-for-distribution.
    Private = 4,
    /// E: protocol · spec-only · implementations-must-conform.
    Protocol = 5,
}

impl Serialize for LicenseTier {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LicenseTier {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        Self::parse_canonical(&s).ok_or_else(|| D::Error::custom(format!("unknown license tier '{s}'")))
    }
}

impl LicenseTier {
    /// Stable name for serde / discovery / UI.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "TIER-A",
            Self::Proprietary => "TIER-B",
            Self::Server => "TIER-C",
            Self::Private => "TIER-D",
            Self::Protocol => "TIER-E",
        }
    }

    /// Parse from `TIER-A` … `TIER-E`.
    #[must_use]
    pub fn parse_canonical(s: &str) -> Option<Self> {
        match s {
            "TIER-A" => Some(Self::Open),
            "TIER-B" => Some(Self::Proprietary),
            "TIER-C" => Some(Self::Server),
            "TIER-D" => Some(Self::Private),
            "TIER-E" => Some(Self::Protocol),
            _ => None,
        }
    }
}

/// § A single cross-package dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    /// Globally-unique package id (slug-format).
    pub id: String,
    /// Semver version-spec (`"M.m.p"` exact or `"^M.m.p"` semver-range).
    pub version: String,
}

/// § Remix attribution chain entry.
///
/// When a creator forks / remixes another package, this records the upstream
/// id / version / attribution-line. The chain is immutable in the signed
/// manifest — modifying any element invalidates the Ed25519 signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemixAttribution {
    /// Upstream package id.
    pub id: String,
    /// Upstream package version.
    pub version: String,
    /// Free-form attribution text (creator handle · license-credit · etc.).
    pub attribution: String,
}

/// § The full manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub kind: ContentKind,
    /// Ed25519 author public-key. Hex-encoded for JSON transport.
    #[serde(with = "hex_pubkey")]
    pub author_pubkey: [u8; 32],
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<Dependency>,
    #[serde(default)]
    pub remix_of: Option<RemixAttribution>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Bit-packed Σ-mask audience-class flags.
    #[serde(default)]
    pub sigma_mask: u64,
    /// Default TRUE. Pay-for-power kinds CANNOT set this false.
    #[serde(default = "default_true")]
    pub gift_economy_only: bool,
    pub license: LicenseTier,
}

const fn default_true() -> bool {
    true
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

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("json error : {0}")]
    Json(String),
    #[error("manifest id is empty")]
    EmptyId,
    #[error("manifest version is empty")]
    EmptyVersion,
    #[error("manifest name is empty")]
    EmptyName,
    #[error("kind {kind} (in-game-power) cannot have gift_economy_only=false")]
    PayForPowerForbidden { kind: &'static str },
}

impl Manifest {
    /// Validate the structural invariants : non-empty id/version/name +
    /// gift-economy-default for in-game-power kinds.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.is_empty() {
            return Err(ManifestError::EmptyId);
        }
        if self.version.is_empty() {
            return Err(ManifestError::EmptyVersion);
        }
        if self.name.is_empty() {
            return Err(ManifestError::EmptyName);
        }
        // Pay-for-power axiom : kinds that grant in-game power cannot opt out
        // of gift-economy. Cosmetic kinds may.
        if !self.gift_economy_only && grants_in_game_power(self.kind) {
            return Err(ManifestError::PayForPowerForbidden {
                kind: self.kind.name(),
            });
        }
        Ok(())
    }

    /// JSON serialise.
    pub fn to_json(&self) -> Result<String, ManifestError> {
        serde_json::to_string(self).map_err(|e| ManifestError::Json(e.to_string()))
    }

    /// Deterministic JSON serialise (BTreeMap-equivalent key ordering via
    /// struct-field order, which is stable in serde-json output).
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ManifestError> {
        // serde_json emits struct fields in source-order which is deterministic.
        let s = self.to_json()?;
        Ok(s.into_bytes())
    }

    /// JSON deserialise.
    pub fn from_json(s: &str) -> Result<Self, ManifestError> {
        serde_json::from_str(s).map_err(|e| ManifestError::Json(e.to_string()))
    }

    /// Total field-count of the manifest schema (for spec-output / docs).
    /// Equals 12 : id · version · kind · author_pubkey · name · description ·
    /// depends_on · remix_of · tags · sigma_mask · gift_economy_only · license.
    pub const FIELD_COUNT: usize = 12;
}

/// § Does this content-kind grant in-game power (vs. pure cosmetic) ?
/// In-game-power kinds CANNOT opt out of gift-economy.
const fn grants_in_game_power(kind: ContentKind) -> bool {
    match kind {
        ContentKind::Scene
        | ContentKind::Npc
        | ContentKind::Recipe
        | ContentKind::System
        | ContentKind::Bundle => true,
        ContentKind::Lore | ContentKind::ShaderPack | ContentKind::AudioPack => false,
    }
}

/// Hex-encode a 32-byte public-key for display / debug.
#[must_use]
pub fn pubkey_hex(b: &[u8; 32]) -> String {
    hex_lower(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Manifest {
        Manifest {
            id: "loa.scenes.darkforest".to_string(),
            version: "1.0.0".to_string(),
            kind: ContentKind::Scene,
            author_pubkey: [0x11; 32],
            name: "Dark Forest".to_string(),
            description: "A misty forest scene with whispering trees.".to_string(),
            depends_on: vec![Dependency {
                id: "loa.npcs.elven".to_string(),
                version: "^0.3.0".to_string(),
            }],
            remix_of: None,
            tags: vec!["forest".to_string(), "atmospheric".to_string()],
            sigma_mask: 0,
            gift_economy_only: true,
            license: LicenseTier::Open,
        }
    }

    #[test]
    fn json_roundtrip() {
        let m = fixture();
        let s = m.to_json().unwrap();
        let back = Manifest::from_json(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn canonical_bytes_deterministic() {
        let m = fixture();
        let a = m.to_canonical_bytes().unwrap();
        let b = m.to_canonical_bytes().unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_id_rejected() {
        let mut m = fixture();
        m.id = String::new();
        assert!(matches!(m.validate(), Err(ManifestError::EmptyId)));
    }

    #[test]
    fn empty_version_rejected() {
        let mut m = fixture();
        m.version = String::new();
        assert!(matches!(m.validate(), Err(ManifestError::EmptyVersion)));
    }

    #[test]
    fn empty_name_rejected() {
        let mut m = fixture();
        m.name = String::new();
        assert!(matches!(m.validate(), Err(ManifestError::EmptyName)));
    }

    #[test]
    fn pay_for_power_forbidden_for_scene() {
        let mut m = fixture();
        m.gift_economy_only = false;
        assert!(matches!(
            m.validate(),
            Err(ManifestError::PayForPowerForbidden { .. })
        ));
    }

    #[test]
    fn pay_for_power_forbidden_for_npc() {
        let mut m = fixture();
        m.kind = ContentKind::Npc;
        m.gift_economy_only = false;
        assert!(matches!(
            m.validate(),
            Err(ManifestError::PayForPowerForbidden { .. })
        ));
    }

    #[test]
    fn cosmetic_lore_can_opt_out_of_gift() {
        let mut m = fixture();
        m.kind = ContentKind::Lore;
        m.gift_economy_only = false;
        // Cosmetic kind : OK to monetise (under cosmetic-only-axiom).
        m.validate().unwrap();
    }

    #[test]
    fn cosmetic_shader_can_opt_out_of_gift() {
        let mut m = fixture();
        m.kind = ContentKind::ShaderPack;
        m.gift_economy_only = false;
        m.validate().unwrap();
    }

    #[test]
    fn cosmetic_audio_can_opt_out_of_gift() {
        let mut m = fixture();
        m.kind = ContentKind::AudioPack;
        m.gift_economy_only = false;
        m.validate().unwrap();
    }

    #[test]
    fn license_tier_roundtrip() {
        for tier in [
            LicenseTier::Open,
            LicenseTier::Proprietary,
            LicenseTier::Server,
            LicenseTier::Private,
            LicenseTier::Protocol,
        ] {
            assert_eq!(LicenseTier::parse_canonical(tier.as_str()), Some(tier));
        }
    }

    #[test]
    fn remix_attribution_in_manifest_serde() {
        let mut m = fixture();
        m.remix_of = Some(RemixAttribution {
            id: "loa.scenes.elderforest".to_string(),
            version: "0.5.0".to_string(),
            attribution: "Original by @Apocky · CC-BY-SA-4.0".to_string(),
        });
        let s = m.to_json().unwrap();
        let back = Manifest::from_json(&s).unwrap();
        assert_eq!(back.remix_of, m.remix_of);
        assert!(back.remix_of.is_some());
    }

    #[test]
    fn field_count_is_twelve() {
        assert_eq!(Manifest::FIELD_COUNT, 12);
    }

    #[test]
    fn pubkey_hex_format() {
        let h = pubkey_hex(&[0x11; 32]);
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn default_gift_economy_is_true_on_deserialize() {
        // Manifest JSON omitting `gift_economy_only` → default true.
        let json = r#"{
          "id":"x","version":"1.0.0","kind":"scene",
          "author_pubkey":"00000000000000000000000000000000000000000000000000000000000000ff",
          "name":"x","description":"",
          "license":"TIER-A"
        }"#;
        let m = Manifest::from_json(json).unwrap();
        assert!(m.gift_economy_only);
    }

    #[test]
    fn pubkey_short_hex_rejected() {
        let bad = r#"{
          "id":"x","version":"1.0.0","kind":"scene",
          "author_pubkey":"abcd",
          "name":"x","description":"",
          "license":"TIER-A"
        }"#;
        let r = Manifest::from_json(bad);
        assert!(r.is_err());
    }
}
