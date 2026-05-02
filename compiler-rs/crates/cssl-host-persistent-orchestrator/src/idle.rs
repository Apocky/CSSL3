// § idle.rs — sustained-idle detector.
//
// § thesis
//   Apocky-AFK ≥ 5 min ⇒ idle-mode. The detector is fed [`ActivityHint`]s
//   on every tick + maintains the wall-clock "last-active-ms" timestamp.
//   The orchestrator queries [`is_idle`] to enable elevated-priority
//   IdleDeepProcgen experiments.

/// Lightweight feed-in struct — host signals when keyboard/mouse/audio
/// activity was last seen.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub struct ActivityHint {
    pub last_input_at_ms: u64,
    pub force_active: bool,
    pub force_idle: bool,
}

/// Sustained-idle detector. Persisted via [`Self::snapshot`] / [`Self::restore`]
/// so journal-replay survives restarts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IdleDetector {
    last_input_at_ms: u64,
    threshold_ms: u64,
}

impl IdleDetector {
    pub fn new(threshold_ms: u64) -> Self {
        Self {
            last_input_at_ms: 0,
            threshold_ms,
        }
    }

    pub fn observe(&mut self, hint: ActivityHint) {
        if hint.last_input_at_ms > self.last_input_at_ms {
            self.last_input_at_ms = hint.last_input_at_ms;
        }
    }

    pub fn is_idle(&self, now_ms: u64, hint: ActivityHint) -> bool {
        if hint.force_idle {
            return true;
        }
        if hint.force_active {
            return false;
        }
        let last = self.last_input_at_ms.max(hint.last_input_at_ms);
        let age = now_ms.saturating_sub(last);
        age >= self.threshold_ms
    }

    pub fn last_input_at_ms(&self) -> u64 {
        self.last_input_at_ms
    }

    pub fn snapshot(&self) -> (u64, u64) {
        (self.last_input_at_ms, self.threshold_ms)
    }

    pub fn restore(last_input_at_ms: u64, threshold_ms: u64) -> Self {
        Self {
            last_input_at_ms,
            threshold_ms,
        }
    }
}
