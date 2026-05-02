//! § store — in-memory ModerationStore (stage-0 self-sufficient).
//! ════════════════════════════════════════════════════════════════════════
//!
//! Stage-0 store keeps everything RAM-side so the substrate works without
//! Supabase wiring. Production endpoints persist via SQL (see migration
//! 0031) ; the Rust substrate is the source-of-truth for INVARIANTS, and
//! the SQL layer mirrors the same invariants via RLS + CHECK constraints.

use std::collections::HashMap;
use std::sync::RwLock;

use thiserror::Error;

use crate::aggregate::ModerationAggregate;
use crate::appeal::Appeal;
use crate::cap::{CapPolicy, MOD_CAP_AGGREGATE_READ, MOD_CAP_FLAG_SUBMIT};
use crate::decision::{CuratorDecision, DecisionKind};
use crate::record::FlagRecord;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("cap denied : 0x{required:02x} (caller=0x{caller:02x})")]
    CapDenied { required: u8, caller: u8 },
    #[error("flag-submission rejected : sigma_mask missing FLAG_SUBMIT bit")]
    FlagMaskMissing,
    #[error("revoke target not found : flagger_handle=0x{0:016x}")]
    RevokeTargetMissing(u64),
    #[error("store-lock poisoned")]
    LockPoisoned,
}

/// § ModerationStore — stage-0 in-memory store.
#[derive(Default)]
pub struct ModerationStore {
    inner: RwLock<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Per-content flag records (insertion-ordered).
    flags_by_content: HashMap<u32, Vec<FlagRecord>>,
    /// Per-content curator decisions.
    decisions_by_content: HashMap<u32, Vec<CuratorDecision>>,
    /// Per-content appeals.
    appeals_by_content: HashMap<u32, Vec<Appeal>>,
    /// Sovereign-revoke trail (content_id → ts).
    sovereign_revokes: HashMap<u32, u32>,
    next_decision_id: u64,
    next_appeal_id: u64,
}

impl ModerationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit a flag · cap-flagger REQUIRED (0x01) at flagger-side AND record
    /// must carry sigma_mask & MOD_CAP_FLAG_SUBMIT.
    pub fn submit_flag(
        &self,
        flagger_cap: CapPolicy,
        record: FlagRecord,
        now: u32,
    ) -> Result<(), StoreError> {
        if !flagger_cap.allows(MOD_CAP_FLAG_SUBMIT, now) {
            return Err(StoreError::CapDenied {
                required: MOD_CAP_FLAG_SUBMIT,
                caller: flagger_cap.bits,
            });
        }
        if record.sigma_mask() & MOD_CAP_FLAG_SUBMIT == 0 {
            return Err(StoreError::FlagMaskMissing);
        }
        let mut inner = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        inner
            .flags_by_content
            .entry(record.content_id())
            .or_default()
            .push(record);
        Ok(())
    }

    /// Flagger-side own-flag-revoke (any-stage).
    pub fn revoke_own_flag(
        &self,
        content_id: u32,
        flagger_handle: u64,
    ) -> Result<u32, StoreError> {
        let mut inner = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        let v = inner
            .flags_by_content
            .get_mut(&content_id)
            .ok_or(StoreError::RevokeTargetMissing(flagger_handle))?;
        let before = v.len();
        v.retain(|f| f.flagger_pubkey_hash() != flagger_handle);
        let removed = (before - v.len()) as u32;
        if removed == 0 {
            return Err(StoreError::RevokeTargetMissing(flagger_handle));
        }
        Ok(removed)
    }

    /// Compute aggregate · author-side cap REQUIRED for transparency-read.
    /// When author_cap is None, returns the aggregate WITHOUT visibility-bit
    /// honored (admin path). When Some, honors visibility-bit.
    pub fn aggregate(
        &self,
        content_id: u32,
        author_cap: Option<CapPolicy>,
        now: u32,
    ) -> Result<ModerationAggregate, StoreError> {
        if let Some(cap) = author_cap {
            if !cap.allows(MOD_CAP_AGGREGATE_READ, now) {
                return Err(StoreError::CapDenied {
                    required: MOD_CAP_AGGREGATE_READ,
                    caller: cap.bits,
                });
            }
        }
        let inner = self.inner.read().map_err(|_| StoreError::LockPoisoned)?;
        let empty: Vec<FlagRecord> = Vec::new();
        let flags = inner.flags_by_content.get(&content_id).unwrap_or(&empty);
        Ok(ModerationAggregate::compute(flags))
    }

    /// File appeal · author-side cap-bit (MOD_CAP_APPEAL) verified by caller.
    pub fn file_appeal(&self, appeal: Appeal) -> Result<u64, StoreError> {
        let mut inner = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        inner.next_appeal_id += 1;
        let mut a = appeal;
        a.appeal_id = inner.next_appeal_id;
        let id = a.appeal_id;
        inner.appeals_by_content.entry(a.content_id).or_default().push(a);
        Ok(id)
    }

    /// Curator-decision · cap-curator REQUIRED + Σ-Chain-anchor verified.
    pub fn record_decision(
        &self,
        curator_cap: CapPolicy,
        decision: CuratorDecision,
        now: u32,
    ) -> Result<u64, StoreError> {
        let required = decision.cap_class.cap_bit() | crate::cap::MOD_CAP_CHAIN_ANCHOR;
        if !curator_cap.allows(required, now) {
            return Err(StoreError::CapDenied {
                required,
                caller: curator_cap.bits,
            });
        }
        let mut inner = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        inner.next_decision_id += 1;
        let mut d = decision;
        d.decision_id = inner.next_decision_id;
        // Re-anchor to ensure determinism after id assignment.
        d.sigma_chain_anchor = d.compute_anchor();
        let id = d.decision_id;
        inner.decisions_by_content.entry(d.content_id).or_default().push(d);
        Ok(id)
    }

    /// Sovereign-revoke · author wins UNCONDITIONALLY (even mid-review).
    /// Records the revoke trail + emits a synthetic decision-record with
    /// kind=SovereignRevoked anchored on Σ-Chain.
    pub fn sovereign_revoke(
        &self,
        content_id: u32,
        author_pubkey_hash: u64,
        now: u32,
    ) -> Result<[u8; 32], StoreError> {
        let mut inner = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        inner.sovereign_revokes.insert(content_id, now);
        inner.next_decision_id += 1;
        let synthetic = CuratorDecision::new(
            inner.next_decision_id,
            content_id,
            author_pubkey_hash,
            crate::cap::CapClass::CommunityElected,
            DecisionKind::SovereignRevoked,
            now,
            b"author-sovereign-revoke",
            [0u8; 64],
        )
        .expect("sovereign-revoke construct");
        let anchor = synthetic.sigma_chain_anchor;
        inner.decisions_by_content.entry(content_id).or_default().push(synthetic);
        Ok(anchor)
    }

    /// Whether content is sovereign-revoked.
    pub fn is_sovereign_revoked(&self, content_id: u32) -> bool {
        self.inner
            .read()
            .map(|i| i.sovereign_revokes.contains_key(&content_id))
            .unwrap_or(false)
    }

    /// Full transparency-history (decision-list).
    pub fn decisions_for(&self, content_id: u32) -> Vec<CuratorDecision> {
        self.inner
            .read()
            .map(|i| i.decisions_by_content.get(&content_id).cloned().unwrap_or_default())
            .unwrap_or_default()
    }

    /// Flag count (for tests + admin-only paths).
    pub fn flag_count(&self, content_id: u32) -> u32 {
        self.inner
            .read()
            .map(|i| i.flags_by_content.get(&content_id).map(|v| v.len()).unwrap_or(0))
            .unwrap_or(0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cap::{CapClass, CapPolicy, MOD_CAP_CHAIN_ANCHOR, MOD_CAP_CURATE_A};
    use crate::record::{FlagKind, FlagRecord};

    fn flagger_cap() -> CapPolicy {
        CapPolicy::new(MOD_CAP_FLAG_SUBMIT, 0)
    }
    fn curator_cap() -> CapPolicy {
        CapPolicy::new(MOD_CAP_CURATE_A | MOD_CAP_CHAIN_ANCHOR, 0)
    }
    fn mk_flag(handle: u64, content_id: u32, severity: u8) -> FlagRecord {
        FlagRecord::pack(
            handle,
            content_id,
            FlagKind::HarmTowardOthers,
            severity,
            MOD_CAP_FLAG_SUBMIT,
            1_700_000_000,
            0,
            0,
        )
        .unwrap()
    }

    #[test]
    fn submit_flag_ok_then_aggregate_below_floor_invisible() {
        let s = ModerationStore::new();
        s.submit_flag(flagger_cap(), mk_flag(0xA, 7, 50), 1_700_000_001).unwrap();
        let a = s.aggregate(7, None, 1_700_000_002).unwrap();
        assert_eq!(a.total_flags, 1);
        assert!(!a.visible_to_author);
    }

    #[test]
    fn aggregate_visible_at_three_flags() {
        let s = ModerationStore::new();
        s.submit_flag(flagger_cap(), mk_flag(0xA, 7, 50), 1_700_000_001).unwrap();
        s.submit_flag(flagger_cap(), mk_flag(0xB, 7, 50), 1_700_000_002).unwrap();
        s.submit_flag(flagger_cap(), mk_flag(0xC, 7, 50), 1_700_000_003).unwrap();
        let a = s.aggregate(7, None, 1_700_000_004).unwrap();
        assert!(a.visible_to_author, "T2 floor reached");
    }

    #[test]
    fn flagger_revoke_own_flag() {
        let s = ModerationStore::new();
        s.submit_flag(flagger_cap(), mk_flag(0xAA, 9, 30), 1_700_000_001).unwrap();
        s.submit_flag(flagger_cap(), mk_flag(0xBB, 9, 30), 1_700_000_002).unwrap();
        assert_eq!(s.flag_count(9), 2);
        let removed = s.revoke_own_flag(9, 0xAA).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(s.flag_count(9), 1);
    }

    #[test]
    fn curator_decision_records_with_anchor() {
        let s = ModerationStore::new();
        let d = CuratorDecision::new(
            0,
            7,
            0xCAFE,
            CapClass::CommunityElected,
            DecisionKind::FlagDismissed,
            1_700_000_100,
            b"reviewed - spurious",
            [0u8; 64],
        )
        .unwrap();
        let id = s.record_decision(curator_cap(), d, 1_700_000_101).unwrap();
        assert!(id > 0);
        let history = s.decisions_for(7);
        assert_eq!(history.len(), 1);
        assert!(history[0].verify_anchor(), "anchor must verify");
    }

    #[test]
    fn sovereign_revoke_during_review_wins() {
        let s = ModerationStore::new();
        // 5 flags filed.
        for i in 0..5u64 {
            s.submit_flag(flagger_cap(), mk_flag(i + 1, 11, 80), 1_700_000_000).unwrap();
        }
        assert!(!s.is_sovereign_revoked(11));
        // Author revokes mid-review.
        let anchor = s.sovereign_revoke(11, 0xA071, 1_700_000_500).unwrap();
        assert!(s.is_sovereign_revoked(11));
        assert_ne!(anchor, [0u8; 32], "anchor must be non-zero");
        // Decision history includes the SovereignRevoked entry.
        let history = s.decisions_for(11);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].kind, DecisionKind::SovereignRevoked);
    }

    #[test]
    fn flag_mask_missing_rejected() {
        let s = ModerationStore::new();
        let bad = FlagRecord::pack(
            1,
            7,
            FlagKind::Spam,
            10,
            0, // sigma_mask missing FLAG_SUBMIT bit
            1_700_000_000,
            0,
            0,
        )
        .unwrap();
        let err = s.submit_flag(flagger_cap(), bad, 1_700_000_001).unwrap_err();
        assert!(matches!(err, StoreError::FlagMaskMissing));
    }

    #[test]
    fn cap_denied_when_curator_lacks_chain_anchor() {
        let s = ModerationStore::new();
        let weak = CapPolicy::new(MOD_CAP_CURATE_A, 0); // missing CHAIN_ANCHOR
        let d = CuratorDecision::new(
            0,
            7,
            0xCAFE,
            CapClass::CommunityElected,
            DecisionKind::FlagDismissed,
            1_700_000_100,
            b"r",
            [0u8; 64],
        )
        .unwrap();
        let err = s.record_decision(weak, d, 1_700_000_101).unwrap_err();
        assert!(matches!(err, StoreError::CapDenied { .. }));
    }
}
