// § throttle.rs — busy-aware throttle policy.
//
// § thesis
//   The daemon must NOT compete with Apocky for CPU when she's actively
//   working. The throttle policy takes a [`BusyHint`] (CPU% + last-input-age
//   + a free-form override flag) and returns a [`ThrottleDecision`].
//
//   Two-tier policy :
//     1. CPU% > threshold (default 50)  → `Throttle` non-idle cycles.
//     2. last-input-age < 5 min          → `Throttle` IdleDeepProcgen ;
//                                          allow other cycles at normal cadence.
//     3. forced override flag            → `BypassAll` (Apocky-overridden).
//
//   The throttle decision is per-cycle so e.g. a 60s mycelium-sync can
//   still proceed even when CPU is high (it's networked + brief), while
//   a 30-min self-author cycle gets deferred.

use crate::cycles::CycleKind;

/// Hints fed in from the embedding host (Tauri-shell, task-scheduler entry,
/// or test fixture). All fields are optional ; absent ⇒ zero (idle).
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub struct BusyHint {
    pub cpu_pct: u8,
    pub last_input_age_ms: u64,
    pub apocky_actively_working: bool,
    pub force_bypass: bool,
}

/// Per-cycle throttle decision.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ThrottleDecision {
    Allow,
    Throttle,
    BypassAll,
}

/// Heuristic policy. See module-doc for the rule-set.
#[derive(Debug, Clone, Copy)]
pub struct ThrottlePolicy {
    pub cpu_threshold_pct: u8,
    pub idle_threshold_ms: u64,
}

impl ThrottlePolicy {
    pub fn new(cpu_threshold_pct: u8, idle_threshold_ms: u64) -> Self {
        Self {
            cpu_threshold_pct,
            idle_threshold_ms,
        }
    }

    pub fn decide(&self, cycle: CycleKind, hint: BusyHint) -> ThrottleDecision {
        if hint.force_bypass {
            return ThrottleDecision::BypassAll;
        }
        // Cheap cycles : 60s mycelium-sync + 5 min KAN-tick are allowed even
        // when busy because they're brief + critical for federation freshness.
        let is_cheap = matches!(cycle, CycleKind::MyceliumSync | CycleKind::KanTick);

        // Idle-mode : only run idle-deep-procgen when sustained-idle ≥ threshold.
        if matches!(cycle, CycleKind::IdleDeepProcgen)
            && hint.last_input_age_ms < self.idle_threshold_ms
        {
            return ThrottleDecision::Throttle;
        }

        // CPU pressure : non-cheap cycles get throttled when CPU > threshold.
        if hint.cpu_pct > self.cpu_threshold_pct && !is_cheap {
            return ThrottleDecision::Throttle;
        }

        // Apocky-active heuristic : if she's working, downshift heavy cycles.
        if hint.apocky_actively_working
            && matches!(
                cycle,
                CycleKind::SelfAuthor | CycleKind::Playtest | CycleKind::IdleDeepProcgen
            )
        {
            return ThrottleDecision::Throttle;
        }

        ThrottleDecision::Allow
    }
}
