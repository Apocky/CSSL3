// cssl-host-coder-runtime
// ══════════════════════════════════════════════════════════════════
// § T11-W8-F1 : sandboxed AST-edit runtime for the automated-Coder agent
// § EXTREME-CAUTION : sovereign-gated · 30s-revert · audit-everywhere · narrow-orchestrator
// § PRIME-DIRECTIVE : consent + sovereignty + transparency · ¬ harm
//
// ROLE (per spec/10 § ROLE-CODER) :
//   - automated-mutation-agent for narrow AST-edits + balance-tunes + cosmetic-tweaks
//   - NOT a generic AGI ; refuses out-of-scope requests structurally
//   - all edits flow : Draft → Staged → ValidationPending → ValidationPassed
//                       → ApprovalPending → Approved → Applied → (AutoReverted | Permanent)
//   - sandbox NEVER touches the real file before Approved-state
//   - 30-second auto-revert window after Apply ; manual-revert always-available within
//   - audit-emit on EVERY state-transition + every hard-cap rejection
//
// HARD-CAPS (¬ negotiable · structural-rejection) :
//   1. path matches `compiler-rs/crates/cssl-substrate-*` → DenySubstrateEdit
//   2. path matches `specs/grand-vision/0[0-9]_*.csl` OR `1[0-5]_*.csl` → DenySpecGrandVision00to15
//   3. path matches TIER-C-secret-glob → DenyTierCSecret
//   4. >10 edits per player per hour → DenyRateLimit (default · configurable)
//   5. substrate/schema/spec-edits without sovereign-cap-bit → DenySovereignRequired
// ══════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Sandboxed AST-edit runtime for the automated-Coder agent.
//!
//! See module-level CSL-block above for the full state-machine and
//! hard-cap matrix. Public surface is the [`CoderRuntime`] facade plus the
//! supporting types in [`edit`], [`sandbox`], [`cap`], [`validation`],
//! [`approval`], [`revert`], [`audit`], and [`hard_cap`] sub-modules.

/// Approval-prompt trait + mock impl. NEVER auto-approves ; fail-safe to Denied on timeout.
pub mod approval;
/// Audit-log trait + in-memory mock (forwards to cssl-host-attestation in real runtime).
pub mod audit;
/// Coder capability bitset + sovereign-bit gate.
pub mod cap;
/// Core edit-types (id, kind, state, staged record).
pub mod edit;
/// Hard-cap policy : structural rejection of substrate / spec-grand-vision-00..15 / TIER-C.
pub mod hard_cap;
/// 30-second revert-window machinery.
pub mod revert;
/// In-memory sandbox store ; NEVER touches real files.
pub mod sandbox;
/// Pre-Apply validation pass.
pub mod validation;

pub use approval::{ApprovalPromptHandler, MockApprovalHandler, PromptOutcome};
pub use audit::{AuditEvent, AuditLog, InMemoryAuditLog};
pub use cap::{CoderCap, SovereignBit};
pub use edit::{CoderEditId, EditKind, EditState, StagedEdit};
pub use hard_cap::{HardCapDecision, HardCapPolicy};
pub use revert::{RevertOutcome, RevertWindow};
pub use sandbox::{SandboxApplyError, SandboxStore};
pub use validation::{ValidationOutcome, ValidationReport};

use std::collections::BTreeMap;

/// Top-level Coder runtime facade.
///
/// Wires together hard-cap policy, sandbox, validation, approval handler,
/// revert window registry, and audit log. All state-transitions flow through
/// here so audit-emit and rate-limit checks cannot be bypassed.
#[derive(Debug)]
pub struct CoderRuntime<A: ApprovalPromptHandler, L: AuditLog> {
    policy: HardCapPolicy,
    sandbox: SandboxStore,
    approval: A,
    audit: L,
    /// Per-player edit timestamps (millis-since-epoch) for rate-limit window.
    rate_log: BTreeMap<[u8; 32], Vec<u64>>,
    /// Active revert windows keyed by edit-id.
    reverts: BTreeMap<CoderEditId, RevertWindow>,
    /// Next monotonic edit-id.
    next_id: u64,
}

impl<A: ApprovalPromptHandler, L: AuditLog> CoderRuntime<A, L> {
    /// Construct a new runtime with the given approval handler + audit log.
    pub fn new(policy: HardCapPolicy, approval: A, audit: L) -> Self {
        Self {
            policy,
            sandbox: SandboxStore::new(),
            approval,
            audit,
            rate_log: BTreeMap::new(),
            reverts: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Borrow the audit log (read-only) — useful in tests and observability surfaces.
    pub fn audit_log(&self) -> &L {
        &self.audit
    }

    /// Borrow the sandbox store (read-only).
    pub fn sandbox(&self) -> &SandboxStore {
        &self.sandbox
    }

    /// Returns `true` if a revert window is currently active for `id`.
    pub fn has_active_revert_window(&self, id: CoderEditId) -> bool {
        self.reverts.get(&id).is_some_and(RevertWindow::is_open)
    }

    /// Ingest an edit-request. Performs every hard-cap check, allocates a fresh
    /// [`CoderEditId`], records it in the sandbox in Draft → Staged form, and
    /// emits audit events. Does NOT touch the real file system.
    pub fn submit_edit(
        &mut self,
        kind: EditKind,
        target_file: String,
        before_blake3: [u8; 32],
        after_blake3: [u8; 32],
        diff_summary: String,
        staged_at_ms: u64,
        player_pubkey: [u8; 32],
        sovereign: SovereignBit,
        caps: CoderCap,
    ) -> Result<CoderEditId, HardCapDecision> {
        // 1. Path-glob hard-caps (substrate + spec-grand-vision + TIER-C).
        if let Some(deny) = self.policy.classify_path(&target_file) {
            self.audit.emit(AuditEvent::hard_cap_rejected(
                target_file.clone(),
                deny,
                staged_at_ms,
            ));
            return Err(deny);
        }

        // 2. Sovereign-cap requirement for substrate/schema/spec edit-kinds.
        if kind.requires_sovereign() && !sovereign.is_held() {
            let deny = HardCapDecision::DenySovereignRequired;
            self.audit.emit(AuditEvent::hard_cap_rejected(
                target_file.clone(),
                deny,
                staged_at_ms,
            ));
            return Err(deny);
        }

        // 3. Cap-bit requirement (CODER_CAP_AST_EDIT minimum for any submit).
        if !caps.contains(CoderCap::AST_EDIT) {
            let deny = HardCapDecision::DenySovereignRequired;
            self.audit.emit(AuditEvent::hard_cap_rejected(
                target_file.clone(),
                deny,
                staged_at_ms,
            ));
            return Err(deny);
        }

        // 4. Rate-limit (10 edits / hour default · per-player).
        let prune_before = staged_at_ms.saturating_sub(self.policy.rate_window_ms);
        let entry = self.rate_log.entry(player_pubkey).or_default();
        entry.retain(|t| *t >= prune_before);
        if entry.len() >= self.policy.rate_max_per_window as usize {
            let deny = HardCapDecision::DenyRateLimit;
            self.audit.emit(AuditEvent::hard_cap_rejected(
                target_file.clone(),
                deny,
                staged_at_ms,
            ));
            return Err(deny);
        }
        entry.push(staged_at_ms);

        // 5. Allocate id, stage in sandbox.
        let id = CoderEditId(self.next_id);
        self.next_id += 1;
        let staged = StagedEdit {
            id,
            kind,
            target_file,
            before_blake3,
            after_blake3,
            diff_summary,
            staged_at_ms,
            staged_by_player_pubkey: player_pubkey,
            state: EditState::Staged,
        };
        self.audit
            .emit(AuditEvent::state_transition(id, EditState::Draft, EditState::Staged, staged_at_ms));
        self.sandbox.insert(staged);
        Ok(id)
    }

    /// Run validation against the staged edit. Transitions
    /// `Staged` → `ValidationPending` → (`ValidationPassed` | `Rejected`).
    pub fn validate(&mut self, id: CoderEditId, now_ms: u64) -> ValidationOutcome {
        let prev = self.sandbox.get(id).map(|e| e.state).unwrap_or(EditState::Rejected);
        self.sandbox.transition(id, EditState::ValidationPending);
        self.audit.emit(AuditEvent::state_transition(
            id,
            prev,
            EditState::ValidationPending,
            now_ms,
        ));
        let outcome = validation::run(self.sandbox.get(id));
        let next = match outcome {
            ValidationOutcome::Pass(_) => EditState::ValidationPassed,
            ValidationOutcome::Fail(_) => EditState::Rejected,
        };
        self.sandbox.transition(id, next);
        self.audit
            .emit(AuditEvent::state_transition(id, EditState::ValidationPending, next, now_ms));
        outcome
    }

    /// Drive the approval prompt. Only valid from `ValidationPassed`.
    /// Transitions to `ApprovalPending` → (`Approved` | `Rejected`).
    /// On `TimedOut` we conservatively transition to `Rejected` (fail-safe).
    pub fn request_approval(&mut self, id: CoderEditId, now_ms: u64) -> PromptOutcome {
        if !matches!(self.sandbox.get(id).map(|e| e.state), Some(EditState::ValidationPassed)) {
            return PromptOutcome::Denied;
        }
        self.sandbox.transition(id, EditState::ApprovalPending);
        self.audit.emit(AuditEvent::state_transition(
            id,
            EditState::ValidationPassed,
            EditState::ApprovalPending,
            now_ms,
        ));
        let outcome = self.approval.prompt(id);
        let next = match outcome {
            PromptOutcome::Approved => EditState::Approved,
            PromptOutcome::Denied | PromptOutcome::TimedOut => EditState::Rejected,
        };
        self.sandbox.transition(id, next);
        self.audit
            .emit(AuditEvent::state_transition(id, EditState::ApprovalPending, next, now_ms));
        outcome
    }

    /// Apply the edit. ONLY valid after `Approved`. This is the ONLY entry-point
    /// that may write to the real file (caller-supplied writer). Arms a 30-second
    /// revert window upon success.
    ///
    /// Returns the edit-id on success or [`SandboxApplyError`] on precondition fail.
    pub fn apply<W: FnMut(&StagedEdit) -> Result<(), String>>(
        &mut self,
        id: CoderEditId,
        now_ms: u64,
        mut writer: W,
    ) -> Result<CoderEditId, SandboxApplyError> {
        let staged = self
            .sandbox
            .get(id)
            .ok_or(SandboxApplyError::UnknownEdit)?
            .clone();
        if staged.state != EditState::Approved {
            return Err(SandboxApplyError::NotApproved(staged.state));
        }
        writer(&staged).map_err(SandboxApplyError::WriterFailed)?;
        self.sandbox.transition(id, EditState::Applied);
        self.audit.emit(AuditEvent::state_transition(
            id,
            EditState::Approved,
            EditState::Applied,
            now_ms,
        ));
        let window = RevertWindow::arm(now_ms, self.policy.revert_window_ms);
        self.reverts.insert(id, window);
        Ok(id)
    }

    /// Manually trigger revert. Only succeeds within the open revert-window.
    pub fn manual_revert(&mut self, id: CoderEditId, now_ms: u64) -> RevertOutcome {
        let window = match self.reverts.get(&id) {
            Some(w) => *w,
            None => return RevertOutcome::NoWindow,
        };
        let outcome = window.try_revert(now_ms);
        if matches!(outcome, RevertOutcome::Reverted) {
            self.sandbox.transition(id, EditState::ManualReverted);
            self.audit.emit(AuditEvent::state_transition(
                id,
                EditState::Applied,
                EditState::ManualReverted,
                now_ms,
            ));
            self.reverts.remove(&id);
        }
        outcome
    }

    /// Auto-revert hook (e.g. crash-detector). Same window-rules as manual.
    pub fn auto_revert(&mut self, id: CoderEditId, now_ms: u64) -> RevertOutcome {
        let window = match self.reverts.get(&id) {
            Some(w) => *w,
            None => return RevertOutcome::NoWindow,
        };
        let outcome = window.try_revert(now_ms);
        if matches!(outcome, RevertOutcome::Reverted) {
            self.sandbox.transition(id, EditState::AutoReverted);
            self.audit.emit(AuditEvent::state_transition(
                id,
                EditState::Applied,
                EditState::AutoReverted,
                now_ms,
            ));
            self.reverts.remove(&id);
        }
        outcome
    }
}
