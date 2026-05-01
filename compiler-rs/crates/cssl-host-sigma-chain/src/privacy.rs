// § privacy.rs — 4 privacy-tiers + LocalOnly never-egress structural-guard
// §§ spec/14 § PRIVACY § privacy-tiers (P0..P3)

use serde::{Deserialize, Serialize};

use crate::event::SigmaEvent;
use crate::sign::PUBKEY_LEN;

/// Privacy tier per spec/14 § PRIVACY (default-private · transparency-as-option).
///
/// Repr-u8 + explicit discriminants chosen for stable on-wire encoding (Public=3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PrivacyTier {
    /// P0 — never egresses · ω-field full-state local-canonical.
    LocalOnly = 0,
    /// P1 — pubkey-rotated per-event · ¬ longitudinal-tracking.
    Anonymized = 1,
    /// P2 — stable-pubkey · player-history aggregable @ Bazaar-tier-system.
    Pseudonymous = 2,
    /// P3 — handle-attached for streamers/content-creators.
    Public = 3,
}

impl Default for PrivacyTier {
    fn default() -> Self {
        Self::LocalOnly
    }
}

impl PrivacyTier {
    /// Returns true iff this tier permits egress beyond the local machine.
    #[must_use]
    pub fn permits_egress(self) -> bool {
        !matches!(self, PrivacyTier::LocalOnly)
    }

    /// Stable wire-tag (NEVER change for an existing tier — included in canonical-bytes).
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            PrivacyTier::LocalOnly => "local_only",
            PrivacyTier::Anonymized => "anonymized",
            PrivacyTier::Pseudonymous => "pseudonymous",
            PrivacyTier::Public => "public",
        }
    }
}

/// Structural egress guard — REJECTS LocalOnly events on any egress-attempt.
///
/// Returns Err(EgressViolation) so callers cannot accidentally bypass via panic-suppression.
/// Sites that send to TIER-3 (Supabase relay etc.) MUST call this before transmit.
///
/// # Errors
/// Returns [`EgressViolation`] when the event's tier is `LocalOnly`.
pub fn egress_check(event: &SigmaEvent) -> Result<(), EgressViolation> {
    if matches!(event.privacy_tier, PrivacyTier::LocalOnly) {
        return Err(EgressViolation::LocalOnlyEvent {
            event_id_prefix_hex: hex_prefix_8(&event.id),
        });
    }
    Ok(())
}

/// Region-anonymized pubkey replacement = BLAKE3("region_anon" || original_pubkey)[..32].
///
/// Used for `PrivacyTier::Anonymized` — strips longitudinal linkage while keeping
/// regional aggregate-state (Living-Multiverse aesthetic-drift) representable.
#[must_use]
pub fn anonymized_pubkey_replacement(original: &[u8; PUBKEY_LEN]) -> [u8; PUBKEY_LEN] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"sigma_chain/region_anon/v1");
    hasher.update(original);
    let mut out = [0u8; PUBKEY_LEN];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}

/// Returns a copy of `event` adjusted for egress-tier rules :
/// - LocalOnly  → Err (caller must not egress)
/// - Anonymized → emitter_pubkey replaced via region-hash · parent-id stripped
/// - Pseudonymous → keep emitter_pubkey · keep lineage
/// - Public → unchanged
///
/// IMPORTANT : this returns a NEW event with the SAME signature bytes. Verifiers
/// at TIER-3 must run [`crate::verify::verify_event`] against the ORIGINAL emitter_pubkey
/// supplied via the player-pubkey-registry · the Anonymized hashed-pubkey is for
/// audit-trail/aggregation only — not signature-verification.
///
/// # Errors
/// Returns [`EgressViolation::LocalOnlyEvent`] if `event.privacy_tier == LocalOnly`.
pub fn sanitize_for_egress(event: &SigmaEvent) -> Result<SigmaEvent, EgressViolation> {
    egress_check(event)?;
    let mut sanitized = event.clone();
    match sanitized.privacy_tier {
        PrivacyTier::LocalOnly => unreachable!("egress_check enforces"),
        PrivacyTier::Anonymized => {
            sanitized.emitter_pubkey = anonymized_pubkey_replacement(&event.emitter_pubkey);
            sanitized.parent_event_id = None;
        }
        PrivacyTier::Pseudonymous | PrivacyTier::Public => {}
    }
    Ok(sanitized)
}

/// Sensitive-field tags structurally-stripped @ emit per F5-IFC + spec/14 § NEVER-EGRESSABLE.
/// Any field with one of these substring-tags in its key MUST be removed before sign-pipeline.
pub const SENSITIVE_FIELD_TAGS: &[&str] = &[
    "biometric",
    "gaze",
    "face",
    "body",
    "raw_field_cell",
    "sigma_overlay_full",
    "companion_history_full",
    "private_key",
];

/// Returns `true` iff `field_name` matches any sensitive tag (case-insensitive substring).
#[must_use]
pub fn is_sensitive_field(field_name: &str) -> bool {
    let lower = field_name.to_ascii_lowercase();
    SENSITIVE_FIELD_TAGS
        .iter()
        .any(|tag| lower.contains(tag))
}

/// Egress violation — caller attempted to send a LocalOnly event off-machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressViolation {
    /// LocalOnly tier ; never egress.
    LocalOnlyEvent {
        /// Hex-prefix (8 chars) of event id for audit-log without leaking full id.
        event_id_prefix_hex: String,
    },
}

impl core::fmt::Display for EgressViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EgressViolation::LocalOnlyEvent { event_id_prefix_hex } => {
                write!(
                    f,
                    "egress refused : LocalOnly event {event_id_prefix_hex} attempted to leave TIER-1 boundary"
                )
            }
        }
    }
}

impl std::error::Error for EgressViolation {}

#[inline]
fn hex_prefix_8(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(8);
    for &b in &bytes[..4] {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}
