// § tour : TourSession. Memory-only ; no actual render-engine.
// § Deterministic-recompute : same merkle-root + same frame-index →
// § identical render-payload digest. Verified via BLAKE3-of-payload.

#![allow(clippy::similar_names)] // `audio` (stub) + `audit` (sink) are intentionally distinct domains

use crate::audit::{AuditEvent, AuditSink};
use crate::cohort_query::HistoricalCohort;
use crate::render_stub::{AudioStub, TourRenderPipeline};
use crate::token::{TtlToken, TokenError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TourError {
    Token(TokenError),
    /// Tour was already finalized (cannot step further).
    Finalized,
}

impl core::fmt::Display for TourError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Token(e)   => write!(f, "tour-token-error: {e}"),
            Self::Finalized  => write!(f, "tour-finalized"),
        }
    }
}
impl std::error::Error for TourError {}

/// § TourSession — bundle of (token · cohort · pipeline · audio · frame-cursor).
/// Pipeline is `Box<dyn TourRenderPipeline>` so callers can swap mock for
/// real cssl-render-v2 / cssl-spectral-render / cssl-fractal-amp at G1-tier-3.
pub struct TourSession {
    pub token: TtlToken,
    pub cohort_id: [u8; 32],
    pub merkle_root: [u8; 32],
    pub frame_index: u64,
    pub render_pipeline: Box<dyn TourRenderPipeline>,
    pub audio: AudioStub,
    finalized: bool,
}

impl TourSession {
    /// § Spawn a new tour session. Caller proves token is fresh by passing
    /// `now_secs`. We emit `TokenIssued` to the audit sink.
    pub fn spawn(
        token: TtlToken,
        cohort: &HistoricalCohort,
        render_pipeline: Box<dyn TourRenderPipeline>,
        audio: AudioStub,
        now_secs: u64,
        audit: &mut dyn AuditSink,
    ) -> Result<Self, TourError> {
        token.validate(now_secs).map_err(TourError::Token)?;
        audit.emit(AuditEvent::TokenIssued {
            token_id:  token.token_id,
            holder:    token.holder_pubkey,
            cohort_id: cohort.cohort_id,
            at_secs:   now_secs,
        });
        Ok(Self {
            token,
            cohort_id: cohort.cohort_id,
            merkle_root: cohort.merkle_root,
            frame_index: 0,
            render_pipeline,
            audio,
            finalized: false,
        })
    }

    /// § Step exactly one frame. Deterministic in (merkle_root · frame_index).
    /// Emits `TourStepped` to audit sink. Returns the 32-byte payload digest.
    pub fn step_frame(&mut self, now_secs: u64, audit: &mut dyn AuditSink) -> Result<[u8; 32], TourError> {
        if self.finalized {
            return Err(TourError::Finalized);
        }
        if self.token.is_expired_at(now_secs) {
            audit.emit(AuditEvent::TokenExpired { token_id: self.token.token_id, at_secs: now_secs });
            self.finalized = true;
            return Err(TourError::Token(TokenError::Expired));
        }
        // seed = BLAKE3(merkle_root · frame_index_le)  → 32-byte deterministic
        let mut h = blake3::Hasher::new();
        h.update(&self.merkle_root);
        h.update(&self.frame_index.to_le_bytes());
        let seed = *h.finalize().as_bytes();

        let digest = self.render_pipeline.step(self.frame_index, seed);
        audit.emit(AuditEvent::TourStepped {
            token_id: self.token.token_id,
            frame:    self.frame_index,
            payload_digest: digest,
        });
        self.frame_index = self.frame_index.saturating_add(1);
        Ok(digest)
    }

    /// § Convenience : step `n` frames. Stops on first error.
    pub fn step_n(&mut self, n: u64, now_secs: u64, audit: &mut dyn AuditSink) -> Result<Vec<[u8; 32]>, TourError> {
        let mut out = Vec::with_capacity(n as usize);
        for _ in 0..n {
            out.push(self.step_frame(now_secs, audit)?);
        }
        Ok(out)
    }

    /// § Finalize on TTL-expiry without stepping. Emits `TokenExpired` once.
    pub fn finalize_on_expiry(&mut self, now_secs: u64, audit: &mut dyn AuditSink) -> bool {
        if !self.finalized && self.token.is_expired_at(now_secs) {
            audit.emit(AuditEvent::TokenExpired { token_id: self.token.token_id, at_secs: now_secs });
            self.finalized = true;
            return true;
        }
        false
    }

    pub fn is_finalized(&self) -> bool { self.finalized }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cohort_query::{CohortEvent, HistoricalCohort};
    use crate::render_stub::MockRenderPipeline;

    fn cohort() -> HistoricalCohort {
        HistoricalCohort::new(
            [9u8; 32],
            [0xAA; 32],
            vec![CohortEvent { scene_id: [1; 16], player_id: [2; 16], ts_secs: 10, payload_digest: [0; 32] }],
        )
    }

    fn pipeline() -> Box<dyn TourRenderPipeline> {
        Box::new(MockRenderPipeline::new("test", [0u8; 32]))
    }

    #[test]
    fn spawn_emits_issued() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let _t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        assert_eq!(audit.count_issued(), 1);
    }

    #[test]
    fn step_frame_deterministic() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit_a = crate::audit::VecAuditSink::new();
        let mut a = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit_a).unwrap();
        let mut audit_b = crate::audit::VecAuditSink::new();
        let mut b = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit_b).unwrap();

        let r1 = a.step_n(5, 1_000_010, &mut audit_a).unwrap();
        let r2 = b.step_n(5, 1_000_010, &mut audit_b).unwrap();
        assert_eq!(r1, r2, "same merkle-root + frame-index → identical digests");
    }

    #[test]
    fn step_frame_varies_with_frame_index() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let mut t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        let r0 = t.step_frame(1_000_001, &mut audit).unwrap();
        let r1 = t.step_frame(1_000_002, &mut audit).unwrap();
        assert_ne!(r0, r1);
    }

    #[test]
    fn step_after_expiry_errors_and_emits_expired() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let mut t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        let err = t.step_frame(1_000_000 + 1800, &mut audit).unwrap_err();
        assert!(matches!(err, TourError::Token(TokenError::Expired)));
        assert_eq!(audit.count_expired(), 1);
        assert!(t.is_finalized());
    }

    #[test]
    fn step_after_finalize_returns_finalized_error() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let mut t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        let _ = t.step_frame(1_000_000 + 1800, &mut audit);
        let err = t.step_frame(1_000_002, &mut audit).unwrap_err();
        assert_eq!(err, TourError::Finalized);
    }

    #[test]
    fn finalize_on_expiry_emits_once() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let mut t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        assert!(t.finalize_on_expiry(1_000_000 + 1800, &mut audit));
        assert!(!t.finalize_on_expiry(1_000_000 + 1900, &mut audit));
        assert_eq!(audit.count_expired(), 1);
    }

    #[test]
    fn step_count_audited() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let mut t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        t.step_n(8, 1_000_005, &mut audit).unwrap();
        assert_eq!(audit.count_stepped(), 8);
    }

    #[test]
    fn distinct_merkle_roots_distinct_digests() {
        let c1 = HistoricalCohort::new([9u8; 32], [0xAA; 32], vec![]);
        let c2 = HistoricalCohort::new([9u8; 32], [0xBB; 32], vec![]);
        let token = TtlToken::mint([1u8; 32], [9u8; 32], 1_000_000);
        let mut a_audit = crate::audit::VecAuditSink::new();
        let mut b_audit = crate::audit::VecAuditSink::new();
        let mut a = TourSession::spawn(token, &c1, pipeline(), AudioStub::default(), 1_000_001, &mut a_audit).unwrap();
        let mut b = TourSession::spawn(token, &c2, pipeline(), AudioStub::default(), 1_000_001, &mut b_audit).unwrap();
        let r_a = a.step_frame(1_000_002, &mut a_audit).unwrap();
        let r_b = b.step_frame(1_000_002, &mut b_audit).unwrap();
        assert_ne!(r_a, r_b);
    }

    #[test]
    fn frame_index_advances() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let mut t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        assert_eq!(t.frame_index, 0);
        t.step_frame(1_000_002, &mut audit).unwrap();
        assert_eq!(t.frame_index, 1);
        t.step_frame(1_000_003, &mut audit).unwrap();
        assert_eq!(t.frame_index, 2);
    }

    #[test]
    fn payload_digest_blake3_stable() {
        let c = cohort();
        let token = TtlToken::mint([1u8; 32], c.cohort_id, 1_000_000);
        let mut audit = crate::audit::VecAuditSink::new();
        let mut t = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit).unwrap();
        let d1 = t.step_frame(1_000_002, &mut audit).unwrap();
        // Re-spawn fresh at same frame-0 → same digest (deterministic).
        let mut audit2 = crate::audit::VecAuditSink::new();
        let mut t2 = TourSession::spawn(token, &c, pipeline(), AudioStub::default(), 1_000_001, &mut audit2).unwrap();
        let d2 = t2.step_frame(1_000_002, &mut audit2).unwrap();
        assert_eq!(d1, d2);
    }
}
