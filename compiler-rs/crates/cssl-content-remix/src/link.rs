//! § RemixLink — the IMMUTABLE attribution-record anchored on Σ-Chain.
//!
//! Layout matches the prompt's required design 1:1.

use crate::kind::RemixKind;
use crate::royalty::RoyaltyShareGift;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Content-id : 16-byte UUID-shaped string (kebab-hex). Kept as String for
/// JSON serde + ergonomic IDs, with a length-bound enforced at construction.
pub type ContentId = String;

/// Semantic-version : "MAJOR.MINOR.PATCH" string. We do not parse beyond
/// regex-shape — comparisons live in the W12-4 dependency-resolver.
pub type SemVer = String;

/// Maximum attribution-text length (creator-authored note shown on the
/// /content/[slug] page beside the link).
pub const ATTRIBUTION_TEXT_MAX: usize = 200;

/// Length of a BLAKE3 digest in bytes (Σ-Chain anchor width).
pub const BLAKE3_LEN: usize = 32;

/// IMMUTABLE attribution-record. Once `sigma_chain_anchor` + signature are
/// computed, ANY mutation invalidates the signature → verify rejects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemixLink {
    /// The newly-forked content (the child).
    pub remixed_id: ContentId,
    /// The parent content being remixed.
    pub parent_id: ContentId,
    /// Parent version pinned at fork-time (so attribution survives parent-
    /// version-bump). Format : "MAJOR.MINOR.PATCH".
    pub parent_version: SemVer,
    /// Relationship descriptor (Fork · Extension · Translation · Adaptation
    /// · Improvement · Bundle).
    pub remix_kind: RemixKind,
    /// Creator-authored note ≤ 200 chars. Empty-string allowed.
    pub attribution_text: String,
    /// 32-byte BLAKE3 digest anchored on Σ-Chain at fork-time. IMMUTABLE.
    /// Encoded as 64-char lower-hex for JSON ergonomics.
    pub sigma_chain_anchor: String,
    /// Unix-seconds timestamp.
    pub created_at: u64,
    /// 32-byte Ed25519 pubkey of the remix-creator (64-char lower-hex).
    pub remix_creator_pubkey: String,
    /// 64-byte Ed25519 signature over canonical-bytes (128-char lower-hex).
    pub remix_signature: String,
    /// OPT-IN gift-only royalty pledge. Defaults to `RoyaltyShareGift::none()`.
    pub royalty_share_gift: RoyaltyShareGift,
}

#[derive(Debug, Error)]
pub enum RemixLinkError {
    #[error("attribution_text {got} chars exceeds {ATTRIBUTION_TEXT_MAX}")]
    AttributionTooLong { got: usize },
    #[error("content_id length {got} not in 1..=64")]
    BadContentId { got: usize },
    #[error("parent_version `{got}` does not match `MAJOR.MINOR.PATCH` shape")]
    BadSemVer { got: String },
    #[error("anchor must be 64 lower-hex chars (got {got})")]
    BadAnchor { got: usize },
    #[error("pubkey must be 64 lower-hex chars (got {got})")]
    BadPubkey { got: usize },
    #[error("signature must be 128 lower-hex chars (got {got})")]
    BadSignature { got: usize },
    #[error("self-remix forbidden : remixed_id == parent_id")]
    SelfRemix,
}

impl RemixLink {
    /// Construct a draft link prior to signing. Anchor + signature must be
    /// filled in by `sign_remix_link` before persistence. Validates lengths
    /// + shape eagerly so bad inputs fail-fast.
    ///
    /// Note : this is the unsigned draft. `sigma_chain_anchor` and
    /// `remix_signature` start empty-string and are populated by sign.
    pub fn new_draft(
        remixed_id: ContentId,
        parent_id: ContentId,
        parent_version: SemVer,
        remix_kind: RemixKind,
        attribution_text: String,
        created_at: u64,
        remix_creator_pubkey: String,
        royalty_share_gift: RoyaltyShareGift,
    ) -> Result<Self, RemixLinkError> {
        if remixed_id.is_empty() || remixed_id.len() > 64 {
            return Err(RemixLinkError::BadContentId {
                got: remixed_id.len(),
            });
        }
        if parent_id.is_empty() || parent_id.len() > 64 {
            return Err(RemixLinkError::BadContentId {
                got: parent_id.len(),
            });
        }
        if remixed_id == parent_id {
            return Err(RemixLinkError::SelfRemix);
        }
        if !is_semver_shape(&parent_version) {
            return Err(RemixLinkError::BadSemVer {
                got: parent_version,
            });
        }
        if attribution_text.chars().count() > ATTRIBUTION_TEXT_MAX {
            return Err(RemixLinkError::AttributionTooLong {
                got: attribution_text.chars().count(),
            });
        }
        if remix_creator_pubkey.len() != 64 || !is_lower_hex(&remix_creator_pubkey) {
            return Err(RemixLinkError::BadPubkey {
                got: remix_creator_pubkey.len(),
            });
        }
        Ok(RemixLink {
            remixed_id,
            parent_id,
            parent_version,
            remix_kind,
            attribution_text,
            sigma_chain_anchor: String::new(),
            created_at,
            remix_creator_pubkey,
            remix_signature: String::new(),
            royalty_share_gift,
        })
    }

    /// Final-validate post-sign. Rejects empty anchor/signature.
    pub fn ensure_signed(&self) -> Result<(), RemixLinkError> {
        if self.sigma_chain_anchor.len() != 64 || !is_lower_hex(&self.sigma_chain_anchor) {
            return Err(RemixLinkError::BadAnchor {
                got: self.sigma_chain_anchor.len(),
            });
        }
        if self.remix_signature.len() != 128 || !is_lower_hex(&self.remix_signature) {
            return Err(RemixLinkError::BadSignature {
                got: self.remix_signature.len(),
            });
        }
        Ok(())
    }
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_semver_shape(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_pubkey() -> String {
        "a".repeat(64)
    }

    #[test]
    fn draft_self_remix_rejected() {
        let err = RemixLink::new_draft(
            "x".to_string(),
            "x".to_string(),
            "1.0.0".to_string(),
            RemixKind::Fork,
            String::new(),
            0,
            fake_pubkey(),
            RoyaltyShareGift::none(),
        )
        .unwrap_err();
        matches!(err, RemixLinkError::SelfRemix);
    }

    #[test]
    fn attribution_text_too_long_rejected() {
        let too_long = "a".repeat(ATTRIBUTION_TEXT_MAX + 1);
        let err = RemixLink::new_draft(
            "child".to_string(),
            "parent".to_string(),
            "1.0.0".to_string(),
            RemixKind::Extension,
            too_long,
            0,
            fake_pubkey(),
            RoyaltyShareGift::none(),
        )
        .unwrap_err();
        matches!(err, RemixLinkError::AttributionTooLong { .. });
    }

    #[test]
    fn semver_shape_rejected() {
        let err = RemixLink::new_draft(
            "child".to_string(),
            "parent".to_string(),
            "v1.0".to_string(),
            RemixKind::Fork,
            String::new(),
            0,
            fake_pubkey(),
            RoyaltyShareGift::none(),
        )
        .unwrap_err();
        matches!(err, RemixLinkError::BadSemVer { .. });
    }

    #[test]
    fn ensure_signed_rejects_empty_anchor() {
        let l = RemixLink::new_draft(
            "child".to_string(),
            "parent".to_string(),
            "1.0.0".to_string(),
            RemixKind::Fork,
            String::new(),
            0,
            fake_pubkey(),
            RoyaltyShareGift::none(),
        )
        .unwrap();
        matches!(l.ensure_signed().unwrap_err(), RemixLinkError::BadAnchor { .. });
    }
}
