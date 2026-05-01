//! § stream — orchestrator : poll · verify · stage · apply · rollback.
//!
//! Wires the modules together behind a single `HotfixStream` API.
//! Production hosts construct one of these per session ; tests
//! supply mock implementations of `SigmaChainPoll`, `Clock`, and
//! `AuditSink`.
//!
//! § BALANCE-TIER 30-SECOND REVERT WINDOW :
//!   When a Balance-tier hotfix is applied, the apply-time is
//!   recorded via the injected `Clock`. Calling
//!   [`HotfixStream::tick_revert_window`] mass-reverts any Applied
//!   Balance-tier hotfix whose time-since-apply exceeds 30 seconds
//!   AND has not received a `confirm_keep` from the player.

use crate::apply::{ApplyHandler, ApplyOutcome, ApplyRegistry, NoopApplyHandler};
use crate::class::{Hotfix, HotfixClass, HotfixId, HotfixState, HotfixTier};
use crate::policy::{decide_apply, PolicyDecision, SovereignCaps};
use crate::rollback::{validate_rollback, RollbackError, RollbackOutcome};
use crate::stage::{StageError, StagingArea};
use crate::verify::{verify, VerifyError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// 30-second revert window per spec § 16 :
/// `BALANCE → opt-in-prompt-with-preview` + roll-back-anytime.
pub const REVERT_WINDOW_NANOS: u128 = 30 * 1_000_000_000;

// ════════════════════════════════════════════════════════════════
// § Trait surface — host-injection points
// ════════════════════════════════════════════════════════════════

/// § Σ-Chain poll source. Sibling W8-C1 crate `cssl-host-sigma-chain`
/// is not yet merged ; this trait is the adapter seam. Production
/// wires the real Σ-Chain client ; tests use `MockSigmaChain`.
pub trait SigmaChainPoll: Send + Sync {
    /// Return all hotfixes published since the last poll. Order
    /// MUST be stable for a given poll instant.
    fn poll(&self) -> Vec<Hotfix>;
}

/// § Clock — injectable so revert-window tests are deterministic.
pub trait Clock: Send + Sync {
    /// Nanoseconds since some monotonic epoch. Only relative
    /// differences matter for revert-window logic.
    fn now_nanos(&self) -> u128;
}

/// § Audit sink — `cssl-host-attestation` is the production target.
/// Tests use `MockAuditSink` to assert audit-emit-every-application.
pub trait AuditSink: Send + Sync {
    fn emit(&self, event: AuditEvent);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub kind: AuditKind,
    pub hotfix_id: HotfixId,
    pub class: HotfixClass,
    pub note: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AuditKind {
    Polled,
    Verified,
    Rejected,
    Staged,
    PromptIssued,
    Applied,
    AutoReverted,
    ManualReverted,
    SovereignCapMissing,
}

// ════════════════════════════════════════════════════════════════
// § Default implementations (production-ready)
// ════════════════════════════════════════════════════════════════

/// Real wall-clock implementation of `Clock`. Uses `SystemTime` —
/// monotonicity is "good enough" for revert windows.
#[derive(Debug, Default, Copy, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_nanos(&self) -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }
}

// ════════════════════════════════════════════════════════════════
// § Mock implementations — for tests + pre-W8-C1 integration
// ════════════════════════════════════════════════════════════════

/// In-memory mock Σ-Chain ; lets tests pre-seed the poll queue.
#[derive(Debug, Default)]
pub struct MockSigmaChain {
    pub queued: Mutex<Vec<Hotfix>>,
}

impl MockSigmaChain {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, h: Hotfix) {
        self.queued.lock().unwrap().push(h);
    }
}

impl SigmaChainPoll for MockSigmaChain {
    fn poll(&self) -> Vec<Hotfix> {
        std::mem::take(&mut *self.queued.lock().unwrap())
    }
}

/// Injectable clock ; tests advance time deterministically.
#[derive(Debug, Default)]
pub struct MockClock {
    now: Mutex<u128>,
}

impl MockClock {
    #[must_use]
    pub fn at(now: u128) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    pub fn advance(&self, nanos: u128) {
        let mut g = self.now.lock().unwrap();
        *g = g.saturating_add(nanos);
    }
}

impl Clock for MockClock {
    fn now_nanos(&self) -> u128 {
        *self.now.lock().unwrap()
    }
}

/// Captures every `AuditEvent` for inspection.
#[derive(Debug, Default)]
pub struct MockAuditSink {
    pub events: Mutex<Vec<AuditEvent>>,
}

impl MockAuditSink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain all captured events (and reset the buffer).
    pub fn drain(&self) -> Vec<AuditEvent> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }

    /// Snapshot without draining.
    pub fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl AuditSink for MockAuditSink {
    fn emit(&self, event: AuditEvent) {
        self.events.lock().unwrap().push(event);
    }
}

// ════════════════════════════════════════════════════════════════
// § Stream errors
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum HotfixError {
    #[error("verify failed : {0}")]
    Verify(#[from] VerifyError),
    #[error("staging failed : {0}")]
    Stage(#[from] StageError),
    #[error("rollback failed : {0}")]
    Rollback(#[from] RollbackError),
    #[error("hotfix `{0}` not staged")]
    NotStaged(String),
    #[error("policy refused apply for hotfix `{0}` (security-tier without sovereign cap)")]
    SovereignCapMissing(String),
    #[error(
        "hotfix `{id}` is in unexpected state `{state:?}` ; cannot {action}"
    )]
    BadState {
        id: String,
        state: HotfixState,
        action: &'static str,
    },
}

// ════════════════════════════════════════════════════════════════
// § HotfixStream — the orchestrator
// ════════════════════════════════════════════════════════════════

/// § The stream. `'src` ties the poll/clock/audit references to
/// host-owned implementations ; the registry is owned and grows
/// over the host's lifetime.
pub struct HotfixStream<'src> {
    master_pubkey: [u8; 32],
    sigma: &'src dyn SigmaChainPoll,
    clock: &'src dyn Clock,
    audit: &'src dyn AuditSink,
    pub staging: StagingArea,
    pub registry: ApplyRegistry,
    /// Hotfix-ids whose `confirm_keep` was received before the
    /// 30-second revert window ran out. Excluded from auto-revert.
    confirmed_keeps: BTreeMap<HotfixId, ()>,
}

impl<'src> HotfixStream<'src> {
    /// Construct a new stream. `master_pubkey` is the
    /// Apocky-master-key bytes (NEVER hardcoded ; injected).
    #[must_use]
    pub fn new(
        master_pubkey: [u8; 32],
        sigma: &'src dyn SigmaChainPoll,
        clock: &'src dyn Clock,
        audit: &'src dyn AuditSink,
    ) -> Self {
        Self {
            master_pubkey,
            sigma,
            clock,
            audit,
            staging: StagingArea::new(),
            registry: ApplyRegistry::new(),
            confirmed_keeps: BTreeMap::new(),
        }
    }

    fn audit_emit(&self, kind: AuditKind, h: &Hotfix, note: &str) {
        self.audit.emit(AuditEvent {
            kind,
            hotfix_id: h.id.clone(),
            class: h.class,
            note: note.to_string(),
        });
    }

    /// Poll the Σ-Chain and return the hotfixes for verify-pipeline.
    /// Pure intake — no state mutation beyond the audit-emit.
    pub fn poll(&self) -> Vec<Hotfix> {
        let polled = self.sigma.poll();
        for h in &polled {
            self.audit_emit(AuditKind::Polled, h, "polled-from-sigma-chain");
        }
        polled
    }

    /// Verify a single hotfix against the master key and commit it
    /// to staging. Returns the resulting `HotfixState`.
    pub fn verify_and_stage(&mut self, hotfix: Hotfix) -> Result<HotfixState, HotfixError> {
        match verify(&hotfix, &self.master_pubkey) {
            Ok(_) => {
                self.audit_emit(AuditKind::Verified, &hotfix, "ed25519+blake3-ok");
                self.staging.insert_verified(hotfix.clone())?;
                self.staging.promote_to_staged(&hotfix.id)?;
                self.audit_emit(AuditKind::Staged, &hotfix, "staged");
                Ok(HotfixState::Staged)
            }
            Err(e) => {
                self.audit_emit(AuditKind::Rejected, &hotfix, &format!("{e}"));
                Err(e.into())
            }
        }
    }

    /// Render the policy decision for a staged hotfix, given the
    /// current sovereign-caps. Does NOT mutate state.
    #[must_use]
    pub fn policy_for(&self, id: &HotfixId, caps: SovereignCaps) -> Option<PolicyDecision> {
        let entry = self.staging.get(id)?;
        Some(decide_apply(entry.hotfix.class, caps))
    }

    /// Apply a staged hotfix. Enforces tier-policy :
    ///   - `Cosmetic` : auto-apply.
    ///   - `Balance`  : apply only if `user_confirmed = true`.
    ///   - `Security` : apply only if caps include SOV_HOTFIX_APPLY.
    pub fn apply(
        &mut self,
        id: &HotfixId,
        caps: SovereignCaps,
        user_confirmed: bool,
    ) -> Result<HotfixState, HotfixError> {
        let entry = self
            .staging
            .get(id)
            .ok_or_else(|| HotfixError::NotStaged(id.0.clone()))?;
        if entry.state != HotfixState::Staged {
            return Err(HotfixError::BadState {
                id: id.0.clone(),
                state: entry.state,
                action: "apply",
            });
        }
        let class = entry.hotfix.class;
        let decision = decide_apply(class, caps);

        match decision {
            PolicyDecision::AutoApply => self.apply_inner(id),
            PolicyDecision::PromptUser => {
                if user_confirmed {
                    self.audit_emit(
                        AuditKind::PromptIssued,
                        &entry.hotfix.clone(),
                        "user-confirmed",
                    );
                    self.apply_inner(id)
                } else {
                    self.audit_emit(
                        AuditKind::PromptIssued,
                        &entry.hotfix.clone(),
                        "awaiting-user-confirm",
                    );
                    Ok(HotfixState::Staged)
                }
            }
            PolicyDecision::RequireSovereign => {
                // already validated by decide_apply : caps present.
                self.apply_inner(id)
            }
            PolicyDecision::Reject => {
                let h = entry.hotfix.clone();
                self.audit_emit(
                    AuditKind::SovereignCapMissing,
                    &h,
                    "security-tier-needs-SOV_HOTFIX_APPLY",
                );
                if let Some(e) = self.staging.get_mut(id) {
                    e.state = HotfixState::Rejected;
                }
                Err(HotfixError::SovereignCapMissing(id.0.clone()))
            }
        }
    }

    fn apply_inner(&mut self, id: &HotfixId) -> Result<HotfixState, HotfixError> {
        let now = self.clock.now_nanos();
        let (class, payload, hotfix_for_audit) = {
            let entry = self
                .staging
                .get(id)
                .ok_or_else(|| HotfixError::NotStaged(id.0.clone()))?;
            (entry.hotfix.class, entry.hotfix.payload.clone(), entry.hotfix.clone())
        };
        let outcome: ApplyOutcome = if let Some(handler) = self.registry.get(class) {
            handler.apply(&payload)
        } else {
            NoopApplyHandler.apply(&payload)
        };
        if let Some(entry) = self.staging.get_mut(id) {
            entry.state = HotfixState::Applied;
            entry.pre_apply_snapshot = Some(outcome.pre_apply_snapshot);
            entry.applied_at_nanos = Some(now);
        }
        self.audit_emit(
            AuditKind::Applied,
            &hotfix_for_audit,
            &format!("applied;{}", outcome.note),
        );
        Ok(HotfixState::Applied)
    }

    /// Player confirms the Balance-tier hotfix should be kept past
    /// the 30-second window.
    pub fn confirm_keep(&mut self, id: &HotfixId) {
        self.confirmed_keeps.insert(id.clone(), ());
    }

    /// Manual rollback. Applied → Reverted ; runs handler.rollback.
    pub fn rollback(&mut self, id: &HotfixId) -> Result<RollbackOutcome, HotfixError> {
        let (state, class, snapshot, hotfix_for_audit) = {
            let entry = self
                .staging
                .get(id)
                .ok_or_else(|| HotfixError::NotStaged(id.0.clone()))?;
            (
                entry.state,
                entry.hotfix.class,
                entry.pre_apply_snapshot.clone().unwrap_or_default(),
                entry.hotfix.clone(),
            )
        };
        let outcome = validate_rollback(id, state)?;
        if matches!(outcome, RollbackOutcome::Reverted) {
            if let Some(handler) = self.registry.get(class) {
                handler.rollback(&snapshot);
            } else {
                NoopApplyHandler.rollback(&snapshot);
            }
            if let Some(entry) = self.staging.get_mut(id) {
                entry.state = HotfixState::Reverted;
            }
            self.audit_emit(AuditKind::ManualReverted, &hotfix_for_audit, "rollback-manual");
        }
        Ok(outcome)
    }

    /// Tick the 30-second revert window. Applied Balance-tier
    /// hotfixes whose age ≥ `REVERT_WINDOW_NANOS` AND lack a
    /// `confirm_keep` are auto-reverted.
    ///
    /// Returns the list of ids that were auto-reverted.
    pub fn tick_revert_window(&mut self) -> Vec<HotfixId> {
        let now = self.clock.now_nanos();
        // Collect candidates first (avoid borrow-while-mutating).
        let mut to_revert: Vec<HotfixId> = Vec::new();
        for (id, entry) in &self.staging.entries {
            if entry.state != HotfixState::Applied {
                continue;
            }
            if entry.hotfix.class.tier() != HotfixTier::Balance {
                continue;
            }
            if self.confirmed_keeps.contains_key(id) {
                continue;
            }
            if let Some(applied_at) = entry.applied_at_nanos {
                if now.saturating_sub(applied_at) >= REVERT_WINDOW_NANOS {
                    to_revert.push(id.clone());
                }
            }
        }
        let mut reverted: Vec<HotfixId> = Vec::with_capacity(to_revert.len());
        for id in to_revert {
            // Snapshot data for audit + handler-rollback.
            let (class, snapshot, hotfix_for_audit) = {
                let entry = match self.staging.get(&id) {
                    Some(e) => e,
                    None => continue,
                };
                (
                    entry.hotfix.class,
                    entry.pre_apply_snapshot.clone().unwrap_or_default(),
                    entry.hotfix.clone(),
                )
            };
            if let Some(handler) = self.registry.get(class) {
                handler.rollback(&snapshot);
            } else {
                NoopApplyHandler.rollback(&snapshot);
            }
            if let Some(entry) = self.staging.get_mut(&id) {
                entry.state = HotfixState::Reverted;
            }
            self.audit_emit(AuditKind::AutoReverted, &hotfix_for_audit, "revert-window-expired");
            reverted.push(id);
        }
        reverted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::{Hotfix, HotfixId};
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn signed_hotfix(class: HotfixClass, id: &str) -> (Hotfix, [u8; 32]) {
        let mut csprng = OsRng;
        let signing = SigningKey::generate(&mut csprng);
        let pubkey = signing.verifying_key().to_bytes();
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let payload_hex = blake3::hash(&payload).to_hex().to_string();
        let mut h = Hotfix {
            id: HotfixId::new(id),
            class,
            payload,
            payload_blake3: payload_hex,
            ed25519_sig: [0u8; 64],
            issuer_pubkey: pubkey,
            ts: 1_700_000_000_000_000_000,
            class_tier: class.tier(),
        };
        let sig = signing.sign(&h.envelope_bytes());
        h.ed25519_sig = sig.to_bytes();
        (h, pubkey)
    }

    /// stage-then-apply happy-path : cosmetic auto-applies.
    #[test]
    fn cosmetic_auto_applies_end_to_end() {
        let (h, pk) = signed_hotfix(HotfixClass::KanWeightUpdate, "hf-1");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(1_000);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        let state = s
            .apply(&HotfixId::new("hf-1"), SovereignCaps::empty(), false)
            .unwrap();
        assert_eq!(state, HotfixState::Applied);
        let kinds: Vec<_> = audit.snapshot().into_iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&AuditKind::Verified));
        assert!(kinds.contains(&AuditKind::Staged));
        assert!(kinds.contains(&AuditKind::Applied));
    }

    /// stage-then-apply happy-path : balance requires user-confirm.
    #[test]
    fn balance_apply_requires_user_confirm() {
        let (h, pk) = signed_hotfix(HotfixClass::BalanceConstantAdjust, "hf-3");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        // Without confirm, stays Staged.
        let st = s
            .apply(&HotfixId::new("hf-3"), SovereignCaps::empty(), false)
            .unwrap();
        assert_eq!(st, HotfixState::Staged);
        // With confirm, transitions to Applied.
        let st = s
            .apply(&HotfixId::new("hf-3"), SovereignCaps::empty(), true)
            .unwrap();
        assert_eq!(st, HotfixState::Applied);
    }

    /// stage-then-apply happy-path : security needs sovereign-cap.
    #[test]
    fn security_without_cap_rejected() {
        let (h, pk) = signed_hotfix(HotfixClass::SovereignCapPolicyFix, "hf-6");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        let res = s.apply(&HotfixId::new("hf-6"), SovereignCaps::empty(), false);
        assert!(matches!(res, Err(HotfixError::SovereignCapMissing(_))));
        // State must reflect rejection.
        assert_eq!(
            s.staging.get(&HotfixId::new("hf-6")).unwrap().state,
            HotfixState::Rejected
        );
    }

    /// sovereign-required WITH cap applies.
    #[test]
    fn security_with_cap_applies() {
        let (h, pk) = signed_hotfix(HotfixClass::SovereignCapPolicyFix, "hf-6");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        let st = s
            .apply(
                &HotfixId::new("hf-6"),
                SovereignCaps::with_hotfix_apply(),
                false,
            )
            .unwrap();
        assert_eq!(st, HotfixState::Applied);
    }

    /// rollback restores (1) : manual.
    #[test]
    fn manual_rollback_transitions_to_reverted() {
        let (h, pk) = signed_hotfix(HotfixClass::KanWeightUpdate, "hf-1");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        s.apply(&HotfixId::new("hf-1"), SovereignCaps::empty(), false)
            .unwrap();
        let outcome = s.rollback(&HotfixId::new("hf-1")).unwrap();
        assert_eq!(outcome, RollbackOutcome::Reverted);
        assert_eq!(
            s.staging.get(&HotfixId::new("hf-1")).unwrap().state,
            HotfixState::Reverted
        );
    }

    /// rollback restores (2) : idempotent.
    #[test]
    fn rollback_is_idempotent() {
        let (h, pk) = signed_hotfix(HotfixClass::KanWeightUpdate, "hf-1");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        s.apply(&HotfixId::new("hf-1"), SovereignCaps::empty(), false)
            .unwrap();
        s.rollback(&HotfixId::new("hf-1")).unwrap();
        let again = s.rollback(&HotfixId::new("hf-1")).unwrap();
        assert_eq!(again, RollbackOutcome::AlreadyReverted);
    }

    /// 30-sec-revert-window timing (1) : NOT triggered before 30s.
    #[test]
    fn revert_window_does_not_fire_within_30_seconds() {
        let (h, pk) = signed_hotfix(HotfixClass::BalanceConstantAdjust, "hf-3");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        s.apply(&HotfixId::new("hf-3"), SovereignCaps::empty(), true)
            .unwrap();
        // Advance 29 seconds — window not yet expired.
        clock.advance(29 * 1_000_000_000);
        let reverted = s.tick_revert_window();
        assert!(reverted.is_empty());
        assert_eq!(
            s.staging.get(&HotfixId::new("hf-3")).unwrap().state,
            HotfixState::Applied
        );
    }

    /// 30-sec-revert-window timing (2) : DOES trigger past 30s.
    #[test]
    fn revert_window_fires_after_30_seconds() {
        let (h, pk) = signed_hotfix(HotfixClass::ProcgenBiasNudge, "hf-2");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        s.apply(&HotfixId::new("hf-2"), SovereignCaps::empty(), true)
            .unwrap();
        clock.advance(31 * 1_000_000_000);
        let reverted = s.tick_revert_window();
        assert_eq!(reverted, vec![HotfixId::new("hf-2")]);
        assert_eq!(
            s.staging.get(&HotfixId::new("hf-2")).unwrap().state,
            HotfixState::Reverted
        );
    }

    #[test]
    fn confirm_keep_skips_auto_revert() {
        let (h, pk) = signed_hotfix(HotfixClass::ProcgenBiasNudge, "hf-2");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        s.apply(&HotfixId::new("hf-2"), SovereignCaps::empty(), true)
            .unwrap();
        s.confirm_keep(&HotfixId::new("hf-2"));
        clock.advance(60 * 1_000_000_000);
        let reverted = s.tick_revert_window();
        assert!(reverted.is_empty());
        assert_eq!(
            s.staging.get(&HotfixId::new("hf-2")).unwrap().state,
            HotfixState::Applied
        );
    }

    #[test]
    fn cosmetic_class_immune_to_revert_window() {
        // Cosmetic classes are auto-apply ; the 30s window is for
        // BALANCE-tier only. Confirm cosmetic stays Applied past 30s.
        let (h, pk) = signed_hotfix(HotfixClass::RenderPipelineParam, "hf-8");
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        s.verify_and_stage(h).unwrap();
        s.apply(&HotfixId::new("hf-8"), SovereignCaps::empty(), false)
            .unwrap();
        clock.advance(60 * 1_000_000_000);
        let reverted = s.tick_revert_window();
        assert!(reverted.is_empty());
        assert_eq!(
            s.staging.get(&HotfixId::new("hf-8")).unwrap().state,
            HotfixState::Applied
        );
    }

    #[test]
    fn poll_emits_audit_per_hotfix() {
        let (h, pk) = signed_hotfix(HotfixClass::KanWeightUpdate, "hf-1");
        let sigma = MockSigmaChain::new();
        sigma.push(h);
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let s = HotfixStream::new(pk, &sigma, &clock, &audit);
        let polled = s.poll();
        assert_eq!(polled.len(), 1);
        let kinds: Vec<_> = audit.snapshot().into_iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![AuditKind::Polled]);
    }

    #[test]
    fn rejected_signature_emits_rejected_audit() {
        let (mut h, pk) = signed_hotfix(HotfixClass::KanWeightUpdate, "hf-1");
        h.ed25519_sig[0] ^= 0xFF;
        let sigma = MockSigmaChain::new();
        let clock = MockClock::at(0);
        let audit = MockAuditSink::new();
        let mut s = HotfixStream::new(pk, &sigma, &clock, &audit);
        let res = s.verify_and_stage(h);
        assert!(res.is_err());
        let kinds: Vec<_> = audit.snapshot().into_iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&AuditKind::Rejected));
    }
}
