// approval.rs — human-approval-prompt trait + mock impl
// ══════════════════════════════════════════════════════════════════
// § approval-prompt is NEVER bypassed by the runtime
// § real-implementation : UI-prompt with player-pubkey signature + per-action consent
// § timeout-default : Denied (fail-safe) — explicit consent required to Approve
// ══════════════════════════════════════════════════════════════════

use crate::edit::CoderEditId;
use std::cell::RefCell;

/// Approval-prompt outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptOutcome {
    /// Player explicitly approved.
    Approved,
    /// Player explicitly denied.
    Denied,
    /// Prompt timed-out before player responded ; treated as Denied (fail-safe).
    TimedOut,
}

/// Approval-prompt handler. Implementations MUST never auto-approve.
pub trait ApprovalPromptHandler: std::fmt::Debug {
    /// Prompt the player and return their decision.
    fn prompt(&mut self, id: CoderEditId) -> PromptOutcome;
}

/// Test-only mock that returns a script of pre-recorded outcomes in order.
///
/// Useful for deterministic state-machine testing. After the script is
/// exhausted, returns [`PromptOutcome::TimedOut`] (fail-safe default).
#[derive(Debug, Default)]
pub struct MockApprovalHandler {
    script: RefCell<Vec<PromptOutcome>>,
    /// Number of prompts received (for test assertions).
    pub prompts_received: RefCell<u32>,
}

impl MockApprovalHandler {
    /// Create a mock with a script of outcomes (consumed in order).
    pub fn with_script(script: Vec<PromptOutcome>) -> Self {
        Self {
            script: RefCell::new(script),
            prompts_received: RefCell::new(0),
        }
    }

    /// Number of prompt-calls made so far.
    pub fn call_count(&self) -> u32 {
        *self.prompts_received.borrow()
    }
}

impl ApprovalPromptHandler for MockApprovalHandler {
    fn prompt(&mut self, _id: CoderEditId) -> PromptOutcome {
        *self.prompts_received.borrow_mut() += 1;
        let mut script = self.script.borrow_mut();
        if script.is_empty() {
            PromptOutcome::TimedOut
        } else {
            script.remove(0)
        }
    }
}
