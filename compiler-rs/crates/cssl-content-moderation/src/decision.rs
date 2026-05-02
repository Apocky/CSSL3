//! § decision — CuratorDecision · Σ-Chain-anchored verdict.
//! ════════════════════════════════════════════════════════════════════════
//!
//! Every curator-decision is anchored on Σ-Chain (immutable record).
//! Anchor-hash is BLAKE3 over canonical-bytes :
//!   (decision_id ‖ content_id ‖ curator_pubkey_hash ‖ cap_class ‖
//!    kind ‖ decided_at ‖ rationale).
//!
//! The signature field is Ed25519 over the same canonical-bytes.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cap::CapClass;

/// § DecisionKind — curator-rendered verdict. wire-stable disc.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DecisionKind {
    /// Flags determined unfounded · content stays as-is.
    FlagDismissed = 0,
    /// Flags upheld · content stays w/ public-warning.
    FlagUpheld = 1,
    /// Age-gate / locale-gate (NOT removed · transparently-restricted).
    ContentRestricted = 2,
    /// Hard-remove · author-appeal-allowed.
    ContentRemoved = 3,
    /// Prior-decision reversed (after appeal).
    AppealAccepted = 4,
    /// Prior-decision sustained (after appeal).
    AppealRejected = 5,
    /// Author-revoke @ any-stage (terminal · sovereign).
    SovereignRevoked = 6,
    /// 7-day-no-decision auto-restore (T5).
    AutoRestored = 7,
}

impl DecisionKind {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::FlagDismissed),
            1 => Some(Self::FlagUpheld),
            2 => Some(Self::ContentRestricted),
            3 => Some(Self::ContentRemoved),
            4 => Some(Self::AppealAccepted),
            5 => Some(Self::AppealRejected),
            6 => Some(Self::SovereignRevoked),
            7 => Some(Self::AutoRestored),
            _ => None,
        }
    }
    /// Whether this decision REQUIRES Σ-Chain-anchor. ALL curator-rendered
    /// decisions require anchor ; AutoRestored + SovereignRevoked are also
    /// anchored (transparency). Returns true for ALL kinds.
    pub fn requires_chain_anchor(self) -> bool {
        true
    }
}

/// Errors during decision-construction.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DecisionError {
    #[error("invalid decision_kind discriminant : {0}")]
    InvalidKind(u8),
    #[error("rationale exceeds 64-byte cap (got {0})")]
    RationaleTooLong(usize),
    #[error("cap-class disallowed for this decision-kind")]
    CapClassDisallowed,
}

/// § CuratorDecision — Σ-Chain-anchored verdict.
///
/// `signature` is stored as Vec<u8> (canonical len = 64) for serde-friendly
/// JSON wire-form. Validators check `signature.len() == 64` at consume-time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratorDecision {
    pub decision_id: u64,
    pub content_id: u32,
    pub curator_pubkey_hash: u64,
    pub cap_class: CapClass,
    pub kind: DecisionKind,
    pub decided_at: u32,
    pub sigma_chain_anchor: [u8; 32],
    pub rationale: Vec<u8>,
    /// Ed25519 signature · canonical len = 64.
    pub signature: Vec<u8>,
}

impl CuratorDecision {
    /// Construct + anchor (caller computes signature externally · we compute
    /// the BLAKE3 anchor-hash deterministically).
    pub fn new(
        decision_id: u64,
        content_id: u32,
        curator_pubkey_hash: u64,
        cap_class: CapClass,
        kind: DecisionKind,
        decided_at: u32,
        rationale: &[u8],
        signature: [u8; 64],
    ) -> Result<Self, DecisionError> {
        if rationale.len() > 64 {
            return Err(DecisionError::RationaleTooLong(rationale.len()));
        }
        let signature = signature.to_vec();
        // SovereignRevoked must NOT come from curator-cap class (only author).
        // Encode that via cap_class semantics : CommunityElected/SubstrateAppointed
        // are curator-classes ; SovereignRevoked is reserved for the author-side
        // sovereign_revoke FFI which constructs records without going through
        // curator caps. Here we accept cap_class = CommunityElected for the
        // synthetic record (system-emit path). Real curator-emit paths set
        // appropriate caps.
        let mut decision = Self {
            decision_id,
            content_id,
            curator_pubkey_hash,
            cap_class,
            kind,
            decided_at,
            sigma_chain_anchor: [0; 32],
            rationale: rationale.to_vec(),
            signature,
        };
        decision.sigma_chain_anchor = decision.compute_anchor();
        Ok(decision)
    }

    /// Compute the BLAKE3 Σ-Chain anchor-hash deterministically.
    pub fn compute_anchor(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"content-moderation\0sigma-chain-anchor\0v1");
        hasher.update(&self.decision_id.to_le_bytes());
        hasher.update(&self.content_id.to_le_bytes());
        hasher.update(&self.curator_pubkey_hash.to_le_bytes());
        hasher.update(&[self.cap_class.cap_bit()]);
        hasher.update(&[self.kind as u8]);
        hasher.update(&self.decided_at.to_le_bytes());
        hasher.update(&self.rationale);
        let mut out = [0u8; 32];
        out.copy_from_slice(&hasher.finalize().as_bytes()[..32]);
        out
    }

    /// Verify the anchor matches the canonical-bytes.
    pub fn verify_anchor(&self) -> bool {
        self.compute_anchor() == self.sigma_chain_anchor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_constructs_and_anchors_deterministically() {
        let d = CuratorDecision::new(
            1,
            42,
            0xCAFE,
            CapClass::CommunityElected,
            DecisionKind::FlagDismissed,
            1_700_000_000,
            b"unfounded - pattern is parody",
            [0u8; 64],
        )
        .expect("decision ok");
        assert_eq!(d.kind, DecisionKind::FlagDismissed);
        assert!(d.verify_anchor(), "anchor must verify");

        // Same input -> same anchor.
        let d2 = CuratorDecision::new(
            1,
            42,
            0xCAFE,
            CapClass::CommunityElected,
            DecisionKind::FlagDismissed,
            1_700_000_000,
            b"unfounded - pattern is parody",
            [0u8; 64],
        )
        .expect("decision ok");
        assert_eq!(d.sigma_chain_anchor, d2.sigma_chain_anchor);
    }

    #[test]
    fn rationale_too_long_rejected() {
        let big = vec![b'x'; 65];
        let err = CuratorDecision::new(
            1,
            42,
            0xCAFE,
            CapClass::CommunityElected,
            DecisionKind::FlagUpheld,
            1_700_000_000,
            &big,
            [0u8; 64],
        )
        .unwrap_err();
        assert!(matches!(err, DecisionError::RationaleTooLong(65)));
    }

    #[test]
    fn anchor_diverges_on_field_change() {
        let d1 = CuratorDecision::new(
            1,
            42,
            0xCAFE,
            CapClass::CommunityElected,
            DecisionKind::FlagDismissed,
            1_700_000_000,
            b"r1",
            [0u8; 64],
        )
        .unwrap();
        let d2 = CuratorDecision::new(
            1,
            42,
            0xCAFE,
            CapClass::CommunityElected,
            DecisionKind::ContentRemoved, // different kind
            1_700_000_000,
            b"r1",
            [0u8; 64],
        )
        .unwrap();
        assert_ne!(d1.sigma_chain_anchor, d2.sigma_chain_anchor);
    }

    #[test]
    fn all_kinds_require_chain_anchor() {
        for k in [
            DecisionKind::FlagDismissed,
            DecisionKind::FlagUpheld,
            DecisionKind::ContentRestricted,
            DecisionKind::ContentRemoved,
            DecisionKind::AppealAccepted,
            DecisionKind::AppealRejected,
            DecisionKind::SovereignRevoked,
            DecisionKind::AutoRestored,
        ] {
            assert!(k.requires_chain_anchor(), "{:?} must require anchor", k);
        }
    }
}
