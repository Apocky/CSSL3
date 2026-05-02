// § lag_comp.rs : LagCompensation — server-side world-rewind for hit validation
//
// Valve "lag compensation" : when a client fires at tick=N seeing the world as
// it was render_t = N - server_lag_ticks ago, the server must REWIND the world
// to render_t before validating the hit, then advance back. Otherwise the
// shooter has to lead targets by their RTT.
//
// We implement this by keeping a sliding window of past `SimState` snapshots
// on the server (`HistoryRing`). The `validate_hit_at` function takes a
// hit-event timestamped at tick=T_render, looks up state[T_render], runs the
// caller-supplied predicate against it, and returns the validation outcome.
//
// The history-window is BOUNDED — typical FPS ~ 200ms (12 ticks @ 60Hz, 25 @
// 128Hz). Events older than the window are rejected to bound CPU + prevent
// historical-revisionism abuse.
//
// ─ NO surveillance / NO heuristic-based validity guesses ; the predicate
//   the caller supplies is the hit-test (raycast vs hitbox or Σ-coherence
//   check). This crate just provides time-rewind.

use crate::ring::TickRing;
use crate::state::SimState;
use crate::tick::TickId;

/// Default lag-compensation window (ticks). 200ms @ 60Hz = 12 ticks ; we
/// pick 64 for headroom + ring power-of-two fast-path.
pub const DEFAULT_LAGCOMP_WINDOW: usize = 64;

/// History of past server-authoritative states.
#[derive(Debug, Clone)]
pub struct HistoryRing {
    states: TickRing<SimState>,
    newest_tick: Option<TickId>,
    /// Maximum age (in ticks) for a hit-validation request.
    pub max_age_ticks: u32,
}

impl HistoryRing {
    #[must_use]
    pub fn new(window: usize) -> Self {
        Self {
            states: TickRing::new(window),
            newest_tick: None,
            max_age_ticks: window as u32 - 1,
        }
    }

    /// Record an authoritative snapshot at its tick.
    pub fn record(&mut self, state: SimState) {
        let t = state.tick;
        self.states.set(t, state);
        match self.newest_tick {
            None => self.newest_tick = Some(t),
            Some(prev) if t.delta(prev) < 0 => self.newest_tick = Some(t),
            _ => {}
        }
    }

    /// Newest recorded tick.
    #[must_use]
    pub fn newest(&self) -> Option<TickId> {
        self.newest_tick
    }

    /// Look up a state at `tick` if still in window.
    #[must_use]
    pub fn at(&self, tick: TickId) -> Option<&SimState> {
        self.states.get(tick)
    }
}

/// Outcome of a lag-compensated hit-validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LagCompOutcome {
    /// Predicate accepted at the rewound tick.
    Accept,
    /// Predicate rejected at the rewound tick.
    Reject,
    /// Render-tick is older than the lag-comp window ; rejected for safety.
    OutOfWindow,
    /// Render-tick is in the future ; clock-skew or attack ; rejected.
    FutureTick,
    /// History didn't have the requested tick (server boot-up gap, etc).
    NoSnapshot,
}

/// Validate a hit at `render_tick` using a caller-supplied predicate against
/// the server's snapshot AT that tick.
///
/// The predicate receives `&SimState` (the rewound world) and returns whether
/// the hit-test passes (e.g., raycast vs target's hitbox-at-render-tick).
pub fn validate_hit_at<F>(
    history: &HistoryRing,
    render_tick: TickId,
    server_now_tick: TickId,
    predicate: F,
) -> LagCompOutcome
where
    F: FnOnce(&SimState) -> bool,
{
    let age = render_tick.delta(server_now_tick);
    if age < 0 {
        return LagCompOutcome::FutureTick;
    }
    if (age as u32) > history.max_age_ticks {
        return LagCompOutcome::OutOfWindow;
    }
    match history.at(render_tick) {
        None => LagCompOutcome::NoSnapshot,
        Some(state) => {
            if predicate(state) {
                LagCompOutcome::Accept
            } else {
                LagCompOutcome::Reject
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigma::SigmaMask;
    use crate::state::{Cell, CellId, CellValue};

    fn snap_with_target(tick: TickId, target_x: i32) -> SimState {
        let mut s = SimState::new(tick);
        s.set(Cell {
            id: CellId(42),
            value: CellValue::V3(target_x, 0, 0),
            mask: SigmaMask::public(),
        });
        s
    }

    #[test]
    fn rewind_validates_hit_at_past_position() {
        let mut h = HistoryRing::new(64);
        // Target moves rightward 10 units / tick.
        for t in 0..10u32 {
            h.record(snap_with_target(TickId(t), (t * 10) as i32));
        }
        // Client fires at server-tick=9 but its render-tick was 4. At tick 4,
        // target was at x=40. Predicate : "target x within ±5 of 40".
        let outcome = validate_hit_at(&h, TickId(4), TickId(9), |s| {
            let p = s.get(CellId(42)).unwrap();
            match p.value {
                CellValue::V3(x, _, _) => (x - 40).abs() <= 5,
                _ => false,
            }
        });
        assert_eq!(outcome, LagCompOutcome::Accept);
    }

    #[test]
    fn out_of_window_rejected() {
        let mut h = HistoryRing::new(8);
        h.max_age_ticks = 4;
        for t in 0..8u32 {
            h.record(snap_with_target(TickId(t), 0));
        }
        let outcome = validate_hit_at(&h, TickId(0), TickId(7), |_| true);
        assert_eq!(outcome, LagCompOutcome::OutOfWindow);
    }

    #[test]
    fn future_tick_rejected() {
        let mut h = HistoryRing::new(64);
        h.record(snap_with_target(TickId(5), 0));
        let outcome = validate_hit_at(&h, TickId(99), TickId(5), |_| true);
        assert_eq!(outcome, LagCompOutcome::FutureTick);
    }

    #[test]
    fn missing_snapshot_returns_no_snapshot() {
        let h = HistoryRing::new(64);
        let outcome = validate_hit_at(&h, TickId(0), TickId(0), |_| true);
        assert_eq!(outcome, LagCompOutcome::NoSnapshot);
    }

    #[test]
    fn predicate_reject_returns_reject() {
        let mut h = HistoryRing::new(64);
        h.record(snap_with_target(TickId(5), 100));
        let outcome = validate_hit_at(&h, TickId(5), TickId(5), |_| false);
        assert_eq!(outcome, LagCompOutcome::Reject);
    }
}
