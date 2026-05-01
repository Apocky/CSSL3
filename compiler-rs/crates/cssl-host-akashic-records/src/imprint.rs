// § imprint.rs · core types · Imprint + ImprintId + ImprintState + SceneMeta
// § cosmetic-channel-only-axiom structural-guard
// § BLAKE3 content-hash over canonical-bytes (scene_metadata · author_pubkey · ts)

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::attribution::AuthorPubkey;
use crate::fidelity::FidelityTier;

/// 64-bit imprint identifier · monotonic from `AkashicLedger`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImprintId(pub u64);

impl ImprintId {
    #[must_use]
    pub fn new(v: u64) -> Self {
        Self(v)
    }

    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ImprintId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "akashic-imprint-{:016x}", self.0)
    }
}

/// 30-min-token (default · configurable) for `HistoricalReconstructionTour`.
///
/// `expires_at` is opaque-monotonic-seconds (caller's clock-frame).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TtlToken {
    pub issued_at: u64,
    pub expires_at: u64,
}

impl TtlToken {
    #[must_use]
    pub fn new(issued_at: u64, ttl_secs: u64) -> Self {
        Self {
            issued_at,
            expires_at: issued_at.saturating_add(ttl_secs),
        }
    }

    /// `true` iff `now` is within `[issued_at, expires_at)`.
    #[must_use]
    pub fn valid_at(self, now: u64) -> bool {
        now >= self.issued_at && now < self.expires_at
    }
}

/// Cosmetic-only scene-metadata · explicit allow-list of fields.
///
/// Per spec/18 § asset-data + landmine "cosmetic-only-axiom · explicit-list-of-allowed-fields".
/// NO field here is consumable by gameplay-stat-systems.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SceneMeta {
    /// Player-visible scene-name (e.g. "Boss-Kill: Crowned-Hollow").
    pub scene_name: String,
    /// Free-form location-tag (e.g. "Verdant-Spire").
    pub location: String,
    /// Identifier for rune-set cosmetic-bundle.
    pub runeset: String,
    /// `true` iff 16-band-spectral-render snapshot was-attached.
    pub spectral_16band_rendered: bool,
    /// `true` iff audio-loop snapshot was-attached.
    pub audio_loop: bool,
}

impl SceneMeta {
    /// Maximum byte-length permitted for any single string-field.
    /// Defends against externally-crafted oversized payloads.
    pub const MAX_STRING_BYTES: usize = 256;

    /// Validate cosmetic-only constraints · used by [`Imprint::assert_cosmetic_only`].
    pub(crate) fn validate(&self) -> Result<(), AkashicError> {
        for (label, s) in [
            ("scene_name", &self.scene_name),
            ("location", &self.location),
            ("runeset", &self.runeset),
        ] {
            if s.len() > Self::MAX_STRING_BYTES {
                return Err(AkashicError::CosmeticAxiomViolation {
                    field: label,
                    reason: "string exceeds MAX_STRING_BYTES",
                });
            }
            // reject non-printable / control bytes (potential serialized stat smuggle)
            for b in s.bytes() {
                if b < 0x20 && b != b'\t' {
                    return Err(AkashicError::CosmeticAxiomViolation {
                        field: label,
                        reason: "control bytes in string",
                    });
                }
            }
        }
        Ok(())
    }

    /// Canonical-bytes for BLAKE3-hashing (scene_metadata portion).
    pub(crate) fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            self.scene_name.len() + self.location.len() + self.runeset.len() + 16,
        );
        // length-prefixed to defeat field-boundary-collisions
        out.extend_from_slice(&(self.scene_name.len() as u32).to_le_bytes());
        out.extend_from_slice(self.scene_name.as_bytes());
        out.extend_from_slice(&(self.location.len() as u32).to_le_bytes());
        out.extend_from_slice(self.location.as_bytes());
        out.extend_from_slice(&(self.runeset.len() as u32).to_le_bytes());
        out.extend_from_slice(self.runeset.as_bytes());
        out.push(u8::from(self.spectral_16band_rendered));
        out.push(u8::from(self.audio_loop));
        out
    }
}

/// Imprint record · cosmetic-channel-only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Imprint {
    pub id: ImprintId,
    pub fidelity: FidelityTier,
    /// BLAKE3 hash of `canonical_bytes(scene_metadata · author_pubkey · ts)`.
    pub content_blake3: [u8; 32],
    pub author_pubkey: AuthorPubkey,
    /// Opaque timestamp · caller-supplied · canonical-clock-frame.
    pub ts: u64,
    /// Shard-cost charged @ imprint (FREE = 0).
    pub shard_cost: u32,
    /// `true` iff fidelity == EternalAttribution (mirror-flag for fast filter).
    pub eternal: bool,
    pub scene_metadata: SceneMeta,
    pub state: ImprintState,
    /// Optional GM-narration payload (Commissioned only).
    pub commissioned_narration: Option<String>,
    /// Optional TTL token (HistoricalReconstructionTour only).
    pub ttl_token: Option<TtlToken>,
}

impl Imprint {
    /// Compute canonical BLAKE3 content-hash over (scene-metadata, author-pubkey, ts).
    ///
    /// Excludes `id`, `state`, `shard_cost` so the same logical-event hashes the
    /// same regardless of ledger-side bookkeeping.
    #[must_use]
    pub fn compute_content_hash(
        scene_metadata: &SceneMeta,
        author_pubkey: &AuthorPubkey,
        ts: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        let scene_bytes = scene_metadata.canonical_bytes();
        hasher.update(&(scene_bytes.len() as u32).to_le_bytes());
        hasher.update(&scene_bytes);
        hasher.update(author_pubkey.as_bytes());
        hasher.update(&ts.to_le_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Cosmetic-channel-only structural-guard.
    ///
    /// Validates :
    /// - `eternal` mirror-flag matches `fidelity == EternalAttribution`
    /// - `commissioned_narration` set iff `fidelity == Commissioned`
    /// - `ttl_token` set iff `fidelity == HistoricalReconstructionTour`
    /// - `SceneMeta` validates (lengths · printable-bytes)
    /// - `shard_cost == 0` iff fidelity is FREE (Basic)
    /// - revoked-state is impossible for eternal-attribution
    ///
    /// # Errors
    /// Returns [`AkashicError::CosmeticAxiomViolation`] if any invariant breaks.
    pub fn assert_cosmetic_only(&self) -> Result<(), AkashicError> {
        // mirror-flag
        if self.eternal != self.fidelity.is_eternal() {
            return Err(AkashicError::CosmeticAxiomViolation {
                field: "eternal",
                reason: "mirror-flag disagrees with fidelity tier",
            });
        }

        // narration field-presence
        let is_commissioned = matches!(self.fidelity, FidelityTier::Commissioned);
        if is_commissioned && self.commissioned_narration.is_none() {
            return Err(AkashicError::CosmeticAxiomViolation {
                field: "commissioned_narration",
                reason: "Commissioned tier missing narration",
            });
        }
        if !is_commissioned && self.commissioned_narration.is_some() {
            return Err(AkashicError::CosmeticAxiomViolation {
                field: "commissioned_narration",
                reason: "narration present on non-Commissioned tier",
            });
        }

        // narration length-bounded
        if let Some(n) = &self.commissioned_narration {
            if n.len() > 8 * SceneMeta::MAX_STRING_BYTES {
                return Err(AkashicError::CosmeticAxiomViolation {
                    field: "commissioned_narration",
                    reason: "narration exceeds 8× MAX_STRING_BYTES",
                });
            }
        }

        // ttl token field-presence
        let is_tour = matches!(self.fidelity, FidelityTier::HistoricalReconstructionTour);
        if is_tour && self.ttl_token.is_none() {
            return Err(AkashicError::CosmeticAxiomViolation {
                field: "ttl_token",
                reason: "tour fidelity missing TTL token",
            });
        }
        if !is_tour && self.ttl_token.is_some() {
            return Err(AkashicError::CosmeticAxiomViolation {
                field: "ttl_token",
                reason: "TTL present on non-tour fidelity",
            });
        }

        // shard_cost vs free
        if self.fidelity.is_free() && self.shard_cost != 0 {
            return Err(AkashicError::CosmeticAxiomViolation {
                field: "shard_cost",
                reason: "Basic-tier must be free",
            });
        }

        // eternal-never-revoked invariant
        if self.eternal && matches!(self.state, ImprintState::Revoked(_)) {
            return Err(AkashicError::CosmeticAxiomViolation {
                field: "state",
                reason: "EternalAttribution can NEVER be revoked",
            });
        }

        // scene-metadata validation
        self.scene_metadata.validate()?;

        Ok(())
    }
}

/// Imprint lifecycle · `Verified` is the on-chain confirmation gate ; `Permanent`
/// is the steady-state ; `Revoked(reason)` filters out from browse-results.
///
/// `EternalAttribution` imprints CAN-NOT enter `Revoked` (axiom).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImprintState {
    Pending,
    Verified,
    Permanent,
    Revoked(RevokedReason),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RevokedReason {
    AuthorRequested,
    PolicyViolation,
    AccountAnonymized,
}

/// Errors emitted by the Akashic-Records crate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AkashicError {
    InsufficientShards { have: u64, need: u64 },
    BalanceOverflow,
    AlreadyOwnedEternal { original: ImprintId },
    CosmeticAxiomViolation { field: &'static str, reason: &'static str },
    UnknownImprint(ImprintId),
    InvariantViolation(&'static str),
}

impl fmt::Display for AkashicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientShards { have, need } => {
                write!(f, "insufficient shards: have {have}, need {need}")
            }
            Self::BalanceOverflow => write!(f, "shard balance overflow"),
            Self::AlreadyOwnedEternal { original } => {
                write!(f, "eternal-attribution already claimed (orig {original})")
            }
            Self::CosmeticAxiomViolation { field, reason } => {
                write!(f, "cosmetic-axiom violation @ {field}: {reason}")
            }
            Self::UnknownImprint(id) => write!(f, "unknown imprint {id}"),
            Self::InvariantViolation(msg) => write!(f, "invariant violation: {msg}"),
        }
    }
}

impl std::error::Error for AkashicError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> AuthorPubkey {
        AuthorPubkey::new([b; 32])
    }

    fn meta(scene: &str) -> SceneMeta {
        SceneMeta {
            scene_name: scene.to_owned(),
            location: "Verdant-Spire".to_owned(),
            runeset: "spec-18-default".to_owned(),
            spectral_16band_rendered: false,
            audio_loop: false,
        }
    }

    #[test]
    fn imprint_id_display() {
        let id = ImprintId::new(0xdead_beef);
        assert_eq!(format!("{id}"), "akashic-imprint-00000000deadbeef");
    }

    #[test]
    fn ttl_token_validity_window() {
        let t = TtlToken::new(1000, 1800);
        assert!(t.valid_at(1000));
        assert!(t.valid_at(2799));
        assert!(!t.valid_at(2800));
        assert!(!t.valid_at(999));
    }

    #[test]
    fn scene_meta_validate_rejects_oversized() {
        let big = "a".repeat(SceneMeta::MAX_STRING_BYTES + 1);
        let m = SceneMeta {
            scene_name: big,
            location: "x".into(),
            runeset: "y".into(),
            spectral_16band_rendered: false,
            audio_loop: false,
        };
        let err = m.validate().unwrap_err();
        assert!(matches!(err, AkashicError::CosmeticAxiomViolation { .. }));
    }

    #[test]
    fn scene_meta_validate_rejects_control_bytes() {
        let m = SceneMeta {
            scene_name: "ok".into(),
            location: "bad\x01".into(),
            runeset: "y".into(),
            spectral_16band_rendered: false,
            audio_loop: false,
        };
        assert!(m.validate().is_err());
    }

    #[test]
    fn imprint_blake3_deterministic() {
        let m1 = meta("scene-A");
        let h1 = Imprint::compute_content_hash(&m1, &pk(1), 100);
        let h2 = Imprint::compute_content_hash(&m1, &pk(1), 100);
        assert_eq!(h1, h2);

        // changing any field changes the hash
        let h3 = Imprint::compute_content_hash(&meta("scene-B"), &pk(1), 100);
        assert_ne!(h1, h3);
        let h4 = Imprint::compute_content_hash(&m1, &pk(2), 100);
        assert_ne!(h1, h4);
        let h5 = Imprint::compute_content_hash(&m1, &pk(1), 101);
        assert_ne!(h1, h5);
    }

    #[test]
    fn imprint_blake3_length_prefix_defeats_collision() {
        // ("ab", "c") and ("a", "bc") must NOT collide
        let m1 = SceneMeta {
            scene_name: "ab".into(),
            location: "c".into(),
            runeset: String::new(),
            spectral_16band_rendered: false,
            audio_loop: false,
        };
        let m2 = SceneMeta {
            scene_name: "a".into(),
            location: "bc".into(),
            runeset: String::new(),
            spectral_16band_rendered: false,
            audio_loop: false,
        };
        let h1 = Imprint::compute_content_hash(&m1, &pk(0), 0);
        let h2 = Imprint::compute_content_hash(&m2, &pk(0), 0);
        assert_ne!(h1, h2);
    }
}
