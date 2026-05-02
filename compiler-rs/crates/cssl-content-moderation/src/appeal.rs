//! § appeal — author-side appeals · 30-day-window · 7-day auto-restore.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THRESHOLDS  (PRIME-DIRECTIVE-CONST · SOVEREIGN-NUMBERED)
//!   T4 : APPEAL-QUORUM         — ≥ 3 distinct curators with cap-curator
//!   T5 : AUTO-RESTORE-WINDOW   — 7 days no-decision ⟶ auto-restore
//!   T6 : APPEAL-WINDOW         — 30 days from any curator-decision

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::decision::DecisionKind;

/// T4 — appeal-quorum required for sustaining appeal-rejection.
pub const K_APPEAL_CURATOR_QUORUM: u32 = 3;
/// T5 — days no-decision before auto-restore.
pub const T_AUTO_RESTORE_DAYS: u32 = 7;
/// T6 — days appeal-window after curator-decision.
pub const T_APPEAL_WINDOW_DAYS: u32 = 30;

const SECONDS_PER_DAY: u32 = 86_400;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AppealError {
    #[error("appeal-window expired (filed_at={filed_at}, decision_at={decision_at}, now={now})")]
    AppealWindowExpired {
        filed_at: u32,
        decision_at: u32,
        now: u32,
    },
    #[error("rationale exceeds 128-byte cap (got {0})")]
    RationaleTooLong(usize),
    #[error("no decision to appeal yet (decision_id_appealed=0 forbidden after restored)")]
    NoDecisionToAppeal,
}

/// § Appeal — author-filed challenge.
///
/// `signature` is stored as Vec<u8> (canonical len = 64) for serde-friendly
/// JSON wire-form. Validators check `signature.len() == 64` at consume-time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Appeal {
    pub appeal_id: u64,
    pub content_id: u32,
    pub author_pubkey_hash: u64,
    pub filed_at: u32,
    /// 0 if appealing flag-state directly (no curator-decision yet).
    pub decision_id_appealed: u64,
    pub rationale: Vec<u8>,
    /// Ed25519 signature · canonical len = 64.
    pub signature: Vec<u8>,
    pub curator_quorum_reached: bool,
    /// 0 = unresolved.
    pub resolved_at: u32,
    /// DecisionKind disc · 7 = AutoRestored.
    pub resolution_kind: u8,
}

impl Appeal {
    /// File a new appeal · validates 30-day-window @ filing.
    pub fn file(
        appeal_id: u64,
        content_id: u32,
        author_pubkey_hash: u64,
        filed_at: u32,
        decision_id_appealed: u64,
        decision_at: u32,
        rationale: &[u8],
        signature: [u8; 64],
    ) -> Result<Self, AppealError> {
        if rationale.len() > 128 {
            return Err(AppealError::RationaleTooLong(rationale.len()));
        }
        if decision_id_appealed != 0 {
            let window_end = decision_at.saturating_add(T_APPEAL_WINDOW_DAYS * SECONDS_PER_DAY);
            if filed_at > window_end {
                return Err(AppealError::AppealWindowExpired {
                    filed_at,
                    decision_at,
                    now: filed_at,
                });
            }
        }
        Ok(Self {
            appeal_id,
            content_id,
            author_pubkey_hash,
            filed_at,
            decision_id_appealed,
            rationale: rationale.to_vec(),
            signature: signature.to_vec(),
            curator_quorum_reached: false,
            resolved_at: 0,
            resolution_kind: 0,
        })
    }

    /// Check T5 auto-restore — 7 days no-decision ⟶ auto-restored.
    pub fn auto_restore_eligible(&self, now: u32) -> bool {
        if self.resolved_at != 0 {
            return false;
        }
        let restore_at = self.filed_at.saturating_add(T_AUTO_RESTORE_DAYS * SECONDS_PER_DAY);
        now >= restore_at
    }

    /// Resolve the appeal with a kind. resolution_kind=7 is auto-restored.
    pub fn resolve(&mut self, kind: DecisionKind, at: u32) {
        self.resolution_kind = kind as u8;
        self.resolved_at = at;
    }

    /// Mark quorum reached (≥ K_APPEAL_CURATOR_QUORUM distinct curators).
    pub fn mark_quorum(&mut self, distinct_curators: u32) {
        self.curator_quorum_reached = distinct_curators >= K_APPEAL_CURATOR_QUORUM;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_within_window_succeeds() {
        let a = Appeal::file(
            1,
            42,
            0xCAFE,
            1_700_001_000,                    // filed_at
            5,
            1_700_000_000,                    // decision_at (1000s earlier)
            b"appeal rationale",
            [0u8; 64],
        );
        assert!(a.is_ok());
    }

    #[test]
    fn file_outside_window_rejected() {
        // decision_at + 31 days < filed_at  ⟶  expired.
        let too_late =
            1_700_000_000 + (T_APPEAL_WINDOW_DAYS + 1) * SECONDS_PER_DAY;
        let a = Appeal::file(
            1,
            42,
            0xCAFE,
            too_late,
            5,
            1_700_000_000,
            b"appeal",
            [0u8; 64],
        );
        assert!(matches!(a, Err(AppealError::AppealWindowExpired { .. })));
    }

    #[test]
    fn auto_restore_after_seven_days() {
        let mut a = Appeal::file(
            1,
            42,
            0xCAFE,
            1_700_000_000,
            0,
            0,
            b"r",
            [0u8; 64],
        )
        .unwrap();
        // 6 days ⟶ NOT eligible.
        let day6 = 1_700_000_000 + 6 * SECONDS_PER_DAY;
        assert!(!a.auto_restore_eligible(day6));
        // 7 days ⟶ eligible.
        let day7 = 1_700_000_000 + T_AUTO_RESTORE_DAYS * SECONDS_PER_DAY;
        assert!(a.auto_restore_eligible(day7));
        // After resolve, no longer eligible.
        a.resolve(DecisionKind::AutoRestored, day7);
        assert!(!a.auto_restore_eligible(day7));
    }

    #[test]
    fn mark_quorum_threshold() {
        let mut a = Appeal::file(1, 42, 0xCAFE, 1_700_000_000, 0, 0, b"r", [0u8; 64]).unwrap();
        a.mark_quorum(2);
        assert!(!a.curator_quorum_reached);
        a.mark_quorum(K_APPEAL_CURATOR_QUORUM);
        assert!(a.curator_quorum_reached);
    }

    #[test]
    fn rationale_too_long_rejected() {
        let big = vec![b'x'; 129];
        let err = Appeal::file(1, 42, 0xCAFE, 1_700_000_000, 0, 0, &big, [0u8; 64]).unwrap_err();
        assert!(matches!(err, AppealError::RationaleTooLong(129)));
    }
}
