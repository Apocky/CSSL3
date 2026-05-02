//! § wired_coder_runtime — wrapper around `cssl-host-coder-runtime`.
//!
//! § T11-W8-CHAT-WIRE
//!   Re-exports the sandboxed AST-edit Coder runtime + provides a process-
//!   global singleton (`OnceLock<Mutex<...>>`) that the 4 NEW `coder.*` MCP
//!   tools delegate to. The runtime itself is sovereign-gated, sandboxed,
//!   and 30-second-auto-revertable — see `cssl-host-coder-runtime` crate
//!   docs for the full state-machine.
//!
//! § Q-12 RESOLVED 2026-05-01 (Apocky-canonical) :
//!   verbatim : "Sovereign choice."
//!   binding-matrix : 6 archetypes × 4 roles (Coder-cell sovereign-revocable)
//!   default-fallback = Phantasia (archetype_id = 0) if-no-cap-set
//!   spec : Labyrinth of Apocalypse/systems/draconic_choice.csl
//!
//! § wrapped surface
//!   - [`CoderRuntime`] — top-level runtime facade.
//!   - [`CoderCap`] / [`SovereignBit`] — cap-bit gate.
//!   - [`HardCapPolicy`] / [`HardCapDecision`] — structural rejection.
//!   - [`EditKind`] / [`EditState`] / [`StagedEdit`] / [`CoderEditId`] —
//!     core edit-types.
//!   - [`InMemoryAuditLog`] / [`MockApprovalHandler`] — stage-0 mocks.
//!
//! § GLOBAL SINGLETON
//!   The singleton is initialized lazily on first MCP-tool call so the
//!   runtime can be constructed with the canonical `HardCapPolicy::default()`
//!   without forcing a static-init order. Tests that need to reset state
//!   call `reset_for_test()`.
//!
//! § ATTESTATION
//!   ¬ harm — wrapper is a re-export shim with a sandboxed runtime ;
//!   sovereign-required for substrate edits ; ALL state-transitions
//!   audit-emit via `cssl-host-attestation` (FUTURE wave) ; current
//!   stage-0 routes to the in-memory `InMemoryAuditLog`.

#![forbid(unsafe_code)]

use std::sync::{Mutex, MutexGuard, OnceLock};

pub use cssl_host_coder_runtime::{
    ApprovalPromptHandler, AuditEvent, AuditLog, CoderCap, CoderEditId, CoderRuntime, EditKind,
    EditState, HardCapDecision, HardCapPolicy, InMemoryAuditLog, MockApprovalHandler,
    PromptOutcome, RevertOutcome, SandboxApplyError, SovereignBit, StagedEdit, ValidationOutcome,
};

/// § T11-W8-CHAT-WIRE : MCP-driven approval handler.
///
/// In stage-0 we route approval through the MCP `coder.approve` tool : the
/// Sovereign explicitly calls the tool, which stashes a decision in the
/// process-wide `APPROVAL_STASH` singleton. The handler reads + drains the
/// stash on every `prompt()` call. If no decision was stashed, returns
/// [`PromptOutcome::TimedOut`] (fail-safe → Rejected).
///
/// This keeps the approval gate sovereign-driven (¬ auto-approve) without
/// requiring the host to spin up a UI thread synchronously, AND avoids
/// reaching into the private `approval` field of `CoderRuntime`.
#[derive(Debug, Default)]
pub struct McpApprovalHandler {
    /// Total prompts driven through this handler (for telemetry).
    prompts_total: std::sync::atomic::AtomicU64,
}

impl McpApprovalHandler {
    /// Total prompts processed since startup.
    pub fn prompts_total(&self) -> u64 {
        self.prompts_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl ApprovalPromptHandler for McpApprovalHandler {
    fn prompt(&mut self, _id: CoderEditId) -> PromptOutcome {
        self.prompts_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        drain_approval_stash().unwrap_or(PromptOutcome::TimedOut)
    }
}

/// Process-wide approval stash. Drained by `McpApprovalHandler::prompt`,
/// populated by `stash_next_approval`.
fn approval_stash() -> &'static Mutex<Option<PromptOutcome>> {
    static S: OnceLock<Mutex<Option<PromptOutcome>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

/// Stash the decision the next `request_approval` will read. The runtime
/// argument is unused but accepted so callers visually correlate stash-then-
/// prompt against a specific runtime instance ; in stage-0 the singleton is
/// process-wide.
pub fn stash_next_approval(_rt: &mut LoaCoderRuntime, outcome: PromptOutcome) {
    if let Ok(mut g) = approval_stash().lock() {
        *g = Some(outcome);
    }
}

/// Drain (and clear) the stashed decision. Used by `McpApprovalHandler::prompt`.
fn drain_approval_stash() -> Option<PromptOutcome> {
    approval_stash().lock().ok().and_then(|mut g| g.take())
}

/// Concrete runtime type used by the global singleton.
///
/// Wires `McpApprovalHandler` (sovereign-driven via the `coder.approve` MCP
/// tool) + `InMemoryAuditLog` (in-memory · forwards to cssl-host-attestation
/// in a future wave).
pub type LoaCoderRuntime = CoderRuntime<McpApprovalHandler, InMemoryAuditLog>;

fn singleton() -> &'static Mutex<LoaCoderRuntime> {
    static RT: OnceLock<Mutex<LoaCoderRuntime>> = OnceLock::new();
    RT.get_or_init(|| {
        Mutex::new(CoderRuntime::new(
            HardCapPolicy::default(),
            McpApprovalHandler::default(),
            InMemoryAuditLog::new(),
        ))
    })
}

/// Acquire the global Coder runtime guard. Used by the 4 `coder.*` MCP
/// tool handlers.
///
/// Returns a `MutexGuard` ; tests + production share this single instance
/// so the audit-log + sandbox observations are unified.
#[must_use]
pub fn lock<'a>() -> MutexGuard<'a, LoaCoderRuntime> {
    singleton()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// Reset the global runtime back to a fresh `default()` state. ONLY safe
/// to call from tests — production callers must not reset the runtime
/// while edits are in flight.
#[doc(hidden)]
pub fn reset_for_test() {
    let mut g = lock();
    *g = CoderRuntime::new(
        HardCapPolicy::default(),
        McpApprovalHandler::default(),
        InMemoryAuditLog::new(),
    );
    // Also clear any stashed approval so the next test starts clean.
    if let Ok(mut s) = approval_stash().lock() {
        *s = None;
    }
}

/// Process-wide mutex acquired by tests in `mcp_tools::tests::coder_*` so
/// parallel cargo-test runners don't trample one another's expected runtime
/// state. Production code never acquires this lock — the cost is the one
/// `OnceLock` initialization at first use.
#[doc(hidden)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// Convenience : count of cap-bits the Coder runtime defines.
///
/// `CODER_CAP_AST_EDIT (1) | HOT_RELOAD (2) | SCHEMA_EVOLVE (4) = 7` (3 bits).
#[must_use]
pub fn coder_cap_bit_count() -> u32 {
    let bits = CoderCap::AST_EDIT
        .union(CoderCap::HOT_RELOAD)
        .union(CoderCap::SCHEMA_EVOLVE);
    bits.0.count_ones()
}

/// Stable label for an `EditState` (used by the JSON-shape of `coder.list_pending`).
#[must_use]
pub fn edit_state_label(s: EditState) -> &'static str {
    match s {
        EditState::Draft => "draft",
        EditState::Staged => "staged",
        EditState::ValidationPending => "validation_pending",
        EditState::ValidationPassed => "validation_passed",
        EditState::ApprovalPending => "approval_pending",
        EditState::Approved => "approved",
        EditState::Applied => "applied",
        EditState::AutoReverted => "auto_reverted",
        EditState::ManualReverted => "manual_reverted",
        EditState::Rejected => "rejected",
    }
}

/// Stable label for an `EditKind`.
#[must_use]
pub fn edit_kind_label(k: EditKind) -> &'static str {
    match k {
        EditKind::AstNodeReplace => "ast_node_replace",
        EditKind::AstNodeInsert => "ast_node_insert",
        EditKind::AstNodeDelete => "ast_node_delete",
        EditKind::BalanceConstantTune => "balance_constant_tune",
        EditKind::CosmeticTweak => "cosmetic_tweak",
        EditKind::NarrowReshape => "narrow_reshape",
    }
}

/// Stable label for a `HardCapDecision`.
#[must_use]
pub fn hard_cap_label(d: HardCapDecision) -> &'static str {
    match d {
        HardCapDecision::Allow => "allow",
        HardCapDecision::DenySubstrateEdit => "deny_substrate_edit",
        HardCapDecision::DenySpecGrandVision00to15 => "deny_spec_grand_vision_00_15",
        HardCapDecision::DenyTierCSecret => "deny_tier_c_secret",
        HardCapDecision::DenyRateLimit => "deny_rate_limit",
        HardCapDecision::DenySovereignRequired => "deny_sovereign_required",
    }
}

// ─── § Q-12 · Draconic-archetype binding-cap (Coder cell) ─────────────────
// Apocky 2026-05-01 verbatim : "Sovereign choice."

/// Default-fallback archetype-id for the Coder role · per Q-12.
pub const CODER_ARCHETYPE_FALLBACK: u8 = 0; // Phantasia

/// Resolve `archetype_id` to a valid archetype for Coder cell · falls back to
/// Phantasia(0) per Q-12 sovereign-choice.
#[must_use]
pub fn coder_resolve_archetype(archetype_id: u8) -> u8 {
    if archetype_id < crate::wired_dm::DRACONIC_ARCHETYPE_COUNT {
        archetype_id
    } else {
        CODER_ARCHETYPE_FALLBACK
    }
}

/// Parse a kind-string from MCP params into a typed `EditKind`. Returns
/// `None` if the string isn't recognized — caller surfaces an `error`
/// envelope back to the MCP client.
#[must_use]
pub fn edit_kind_from_str(s: &str) -> Option<EditKind> {
    match s {
        "ast_node_replace" | "ast.replace" | "replace" => Some(EditKind::AstNodeReplace),
        "ast_node_insert" | "ast.insert" | "insert" => Some(EditKind::AstNodeInsert),
        "ast_node_delete" | "ast.delete" | "delete" => Some(EditKind::AstNodeDelete),
        "balance_constant_tune" | "balance" | "tune" => Some(EditKind::BalanceConstantTune),
        "cosmetic_tweak" | "cosmetic" | "tweak" => Some(EditKind::CosmeticTweak),
        "narrow_reshape" | "reshape" => Some(EditKind::NarrowReshape),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coder_cap_bit_count_is_three() {
        assert_eq!(coder_cap_bit_count(), 3);
    }

    #[test]
    fn edit_state_labels_are_stable() {
        assert_eq!(edit_state_label(EditState::Draft), "draft");
        assert_eq!(edit_state_label(EditState::Staged), "staged");
        assert_eq!(edit_state_label(EditState::Approved), "approved");
        assert_eq!(edit_state_label(EditState::Applied), "applied");
        assert_eq!(edit_state_label(EditState::Rejected), "rejected");
    }

    #[test]
    fn edit_kind_round_trip() {
        for k in [
            EditKind::AstNodeReplace,
            EditKind::AstNodeInsert,
            EditKind::AstNodeDelete,
            EditKind::BalanceConstantTune,
            EditKind::CosmeticTweak,
            EditKind::NarrowReshape,
        ] {
            let label = edit_kind_label(k);
            let back = edit_kind_from_str(label).unwrap();
            assert_eq!(back, k, "round-trip failed for {label}");
        }
    }

    #[test]
    fn singleton_is_reusable_after_reset() {
        reset_for_test();
        // Lock + drop — should not deadlock.
        {
            let _g = lock();
        }
        reset_for_test();
        let _g = lock();
    }
}
