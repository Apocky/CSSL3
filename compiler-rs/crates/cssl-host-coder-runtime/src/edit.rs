// edit.rs — core edit-types: id, kind, state, staged-record
// ══════════════════════════════════════════════════════════════════
// § narrow-list ; new EditKind variants require spec-update + review
// § state-machine : Draft → Staged → ValidationPending → ValidationPassed
//                    → ApprovalPending → Approved → Applied → (AutoReverted | ManualReverted | Permanent)
// § Rejected is a terminal sink from any pre-Apply state
// ══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Monotonic edit identifier (allocated by [`crate::CoderRuntime`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CoderEditId(pub u64);

/// Narrow set of permitted edit-kinds.
///
/// Substrate-class kinds (`AstNodeReplace`, `AstNodeInsert`, `AstNodeDelete`,
/// `NarrowReshape`) require sovereign-bit per [`Self::requires_sovereign`].
/// `BalanceConstantTune` and `CosmeticTweak` are soft-cap (still rate-limited
/// + audit-emitted, but no sovereign).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EditKind {
    /// Replace a single AST node.
    AstNodeReplace,
    /// Insert a new AST node.
    AstNodeInsert,
    /// Delete an AST node.
    AstNodeDelete,
    /// Tune a balance-constant (e.g. damage multiplier).
    BalanceConstantTune,
    /// Cosmetic-only tweak (rename, doc-comment).
    CosmeticTweak,
    /// Narrow structural reshape (rare, sovereign-required).
    NarrowReshape,
}

impl EditKind {
    /// Returns `true` iff this kind needs the sovereign-bit set.
    pub const fn requires_sovereign(self) -> bool {
        matches!(
            self,
            Self::AstNodeReplace | Self::AstNodeInsert | Self::AstNodeDelete | Self::NarrowReshape,
        )
    }
}

/// Edit lifecycle states. Pre-Apply states transition through validation +
/// approval gates ; post-Apply states either remain `Applied` (after the
/// 30-second revert-window closes, becoming `Permanent` semantically) or
/// transition to `AutoReverted` / `ManualReverted` within the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EditState {
    /// Just constructed, not yet sandbox-resident.
    Draft,
    /// Stored in sandbox, awaiting validation.
    Staged,
    /// Validation in-flight.
    ValidationPending,
    /// Validation succeeded, awaiting human-approval prompt.
    ValidationPassed,
    /// Approval prompt shown to player.
    ApprovalPending,
    /// Player approved ; ready to apply.
    Approved,
    /// Successfully written to real file ; revert-window armed.
    Applied,
    /// Auto-revert fired (crash-detect, watchdog) within window.
    AutoReverted,
    /// Player explicitly reverted within window.
    ManualReverted,
    /// Edit explicitly rejected (validation-fail, approval-deny, hard-cap).
    Rejected,
}

/// Staged edit record (sandbox-resident before Apply).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedEdit {
    /// Unique id (monotonic).
    pub id: CoderEditId,
    /// Narrow edit-kind.
    pub kind: EditKind,
    /// Target file path (validated by hard-cap policy on submit).
    pub target_file: String,
    /// blake3 of file-bytes BEFORE the edit.
    pub before_blake3: [u8; 32],
    /// blake3 of file-bytes AFTER the edit (sandbox-staged content).
    pub after_blake3: [u8; 32],
    /// Human-readable diff summary.
    pub diff_summary: String,
    /// Wall-clock millis when staged.
    pub staged_at_ms: u64,
    /// Player who staged (Ed25519 pubkey).
    pub staged_by_player_pubkey: [u8; 32],
    /// Current lifecycle state.
    pub state: EditState,
}
