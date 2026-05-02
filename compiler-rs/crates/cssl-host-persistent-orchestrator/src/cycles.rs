// § cycles.rs — strongly-typed cycle taxonomy.
//
// § why an enum
//   The orchestrator drives FIVE distinct cycles. Every cycle has its own
//   cadence + cap-requirement + outcome variant. Pattern-matching against
//   a closed enum statically forbids the daemon from gaining a sixth cycle
//   without updating cap-policy + journal-replay code paths.

use serde::{Deserialize, Serialize};

/// The five orchestrated cycle-kinds. Order matters : `Display` /
/// `Serialize` prints in priority-rank, used by the journal-replay code.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum CycleKind {
    /// Periodic self-author : every 30 min — generate-CSSL → sandbox → score → submit-as-draft.
    SelfAuthor,
    /// Periodic playtest : every 15 min — auto-playtest published content → KAN-feed.
    Playtest,
    /// Periodic KAN tick : every 5 min — drain reservoir + apply bias-updates.
    KanTick,
    /// Periodic mycelium sync : every 60s — federate chat-pattern deltas + pull peer anchors.
    MyceliumSync,
    /// Idle deep-procgen : on sustained idle (≥ 5min) — elevated-priority experiments.
    IdleDeepProcgen,
}

impl CycleKind {
    /// Human-readable short name for journal output + error messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::SelfAuthor => "self_author",
            Self::Playtest => "playtest",
            Self::KanTick => "kan_tick",
            Self::MyceliumSync => "mycelium_sync",
            Self::IdleDeepProcgen => "idle_deep_procgen",
        }
    }

    /// Closed-set iterator over all cycle kinds. Used by tests + the
    /// scheduler initial-due priming.
    pub fn all() -> [CycleKind; 5] {
        [
            Self::SelfAuthor,
            Self::Playtest,
            Self::KanTick,
            Self::MyceliumSync,
            Self::IdleDeepProcgen,
        ]
    }
}

impl core::fmt::Display for CycleKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// What happened when one cycle ran on a tick.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CycleOutcome {
    /// Cycle ran successfully ; carries a short-stat tag for journaling.
    Success { kind: CycleKind, stat: String },
    /// Cycle did not run because cap-grant was denied.
    CapDenied { kind: CycleKind, required: String },
    /// Cycle did not run because the orchestrator was paused.
    Paused { kind: CycleKind },
    /// Cycle did not run because system-was-busy.
    Throttled { kind: CycleKind },
    /// Cycle ran but its driver returned a non-fatal error.
    DriverError { kind: CycleKind, message: String },
}

impl CycleOutcome {
    pub fn kind(&self) -> CycleKind {
        match self {
            Self::Success { kind, .. }
            | Self::CapDenied { kind, .. }
            | Self::Paused { kind }
            | Self::Throttled { kind }
            | Self::DriverError { kind, .. } => *kind,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}
