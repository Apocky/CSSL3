// § scheduler.rs — deterministic next-due cycle selection.
//
// § thesis
//   Each cycle has a fixed cadence + a "last-ran" timestamp. The scheduler
//   computes the next-due deadline per cycle, picks the most-overdue cycle
//   first, and returns up to N cycles to run on a single tick.
//
//   Deterministic ordering : when two cycles tie on overdue-amount, the
//   priority-rank in [`CycleKind::all`] breaks the tie. This makes journal
//   replay fully deterministic across runs with the same `tick(now_ms)`
//   sequence.

use serde::{Deserialize, Serialize};

use crate::config::OrchestratorConfig;
use crate::cycles::CycleKind;

/// One due-cycle snapshot.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct NextDue {
    pub kind: CycleKind,
    pub due_at_ms: u64,
    pub last_ran_ms: u64,
}

/// Per-cycle schedule state. Compact ; fits in cache.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CycleSchedule {
    schedule: [NextDue; 5],
}

impl CycleSchedule {
    /// Initial schedule — every cycle is due NOW so the daemon does
    /// productive work on its very first tick. This matches Apocky's
    /// "engine isn't running while we design it" complaint : we want
    /// IMMEDIATE useful work, not a 30-min wait for the first cycle.
    pub fn fresh(now_ms: u64) -> Self {
        let mut schedule = [NextDue {
            kind: CycleKind::SelfAuthor,
            due_at_ms: now_ms,
            last_ran_ms: 0,
        }; 5];
        for (i, kind) in CycleKind::all().into_iter().enumerate() {
            schedule[i] = NextDue {
                kind,
                due_at_ms: now_ms,
                last_ran_ms: 0,
            };
        }
        Self { schedule }
    }

    /// Compute the next-due cycles, up to `max`, ordered by overdue-amount.
    /// Cycles whose deadline is in the future are excluded.
    pub fn pick_due(&self, now_ms: u64, max: usize) -> Vec<CycleKind> {
        let mut overdue: Vec<(CycleKind, u64, usize)> = Vec::new();
        for (rank, slot) in self.schedule.iter().enumerate() {
            if now_ms >= slot.due_at_ms {
                let lateness = now_ms.saturating_sub(slot.due_at_ms);
                overdue.push((slot.kind, lateness, rank));
            }
        }
        // Sort : most-overdue first, then priority-rank ascending (deterministic
        // tie-break via the order CycleKind::all returns).
        overdue.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
        overdue.into_iter().take(max).map(|(k, _, _)| k).collect()
    }

    /// Mark a cycle as just-ran. Updates the next-due deadline.
    pub fn mark_ran(&mut self, kind: CycleKind, now_ms: u64, cfg: &OrchestratorConfig) {
        let cadence = cfg.cadence_for(kind);
        for slot in &mut self.schedule {
            if slot.kind == kind {
                slot.last_ran_ms = now_ms;
                slot.due_at_ms = now_ms.saturating_add(cadence);
            }
        }
    }

    /// Defer a cycle (used when it was throttled or paused) — pushes the
    /// next-due forward by HALF a cadence so the throttled cycle is retried
    /// soon without spinning.
    pub fn defer(&mut self, kind: CycleKind, now_ms: u64, cfg: &OrchestratorConfig) {
        let half = cfg.cadence_for(kind) / 2;
        for slot in &mut self.schedule {
            if slot.kind == kind {
                slot.due_at_ms = now_ms.saturating_add(half.max(1));
            }
        }
    }

    /// Read-only borrow of the full schedule (for tests + UI).
    pub fn as_slice(&self) -> &[NextDue; 5] {
        &self.schedule
    }
}
