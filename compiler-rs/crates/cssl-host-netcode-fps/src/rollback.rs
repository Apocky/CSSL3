// § rollback.rs : RollbackNetcode — N-frames rollback (GGPO-tradition)
//
// Fighting-game / low-latency-FPS style rollback. Rather than waiting for a
// remote-input round-trip, the client predicts remote-inputs (typically last-
// known + zero-delta), advances the simulation, and WHEN the real remote
// inputs arrive, rolls back N frames + replays with the corrected inputs.
//
// Differences from `Reconciliation` :
//   - Reconciliation is server-authoritative (server says X, client agrees).
//   - Rollback is symmetric peer-to-peer (every peer locally replays as soon
//     as remote inputs arrive — there's no "server").
//   - Rollback budget is tighter (typically 7..16 frames) for visual smoothness ;
//     beyond budget = visible glitch.
//
// `RollbackEngine` keeps :
//   - per-peer input rings (last-known + speculative-fill),
//   - per-tick state ring,
//   - frame-count budget DEFAULT 8 (≈ 133ms @ 60Hz, classic GGPO baseline).

use std::collections::BTreeMap;

use crate::input::InputFrame;
use crate::ring::TickRing;
use crate::sigma::PeerSlot;
use crate::sim::Simulation;
use crate::state::SimState;
use crate::tick::TickId;

/// Default rollback frame budget. 8 = GGPO-tradition fighting-game baseline.
pub const DEFAULT_ROLLBACK_FRAMES: u32 = 8;

/// Errors returned by the rollback engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackErr {
    /// Rollback would exceed the configured frame budget.
    BudgetExceeded { requested: u32, budget: u32 },
    /// Required state-snapshot has been evicted from the ring.
    StateGone,
}

/// Per-peer input streams + speculative fills.
#[derive(Debug, Clone)]
struct PeerInputs {
    /// Confirmed inputs (received from network).
    confirmed: TickRing<InputFrame>,
    /// Last-known confirmed tick (for synthesizing speculative fill).
    last_confirmed: Option<TickId>,
}

impl PeerInputs {
    fn new(cap: usize) -> Self {
        Self {
            confirmed: TickRing::new(cap),
            last_confirmed: None,
        }
    }

    fn record(&mut self, frame: InputFrame) {
        self.confirmed.set(frame.tick, frame);
        match self.last_confirmed {
            None => self.last_confirmed = Some(frame.tick),
            Some(prev) if prev.delta(frame.tick) > 0 => self.last_confirmed = Some(frame.tick),
            _ => {}
        }
    }

    /// Get input at tick : confirmed or last-known-frozen if speculative.
    fn at_or_speculative(&self, tick: TickId) -> Option<InputFrame> {
        if let Some(c) = self.confirmed.get(tick) {
            return Some(*c);
        }
        // Speculative fill : freeze last-known.
        let last = self.last_confirmed?;
        if last.delta(tick) >= 0 {
            // Future tick & no confirmation → use last-known input
            self.confirmed
                .get(last)
                .map(|f| InputFrame { tick, ..*f })
        } else {
            None
        }
    }
}

/// Rollback engine with N-frame budget.
#[derive(Debug, Clone)]
pub struct RollbackEngine {
    /// Local-tick = the latest tick we have a state for.
    pub current_tick: TickId,
    /// State history ; cap >= max_rollback_frames + small headroom.
    pub states: TickRing<SimState>,
    peers: BTreeMap<PeerSlot, PeerInputs>,
    /// Maximum rollback frames before declaring desync.
    pub max_rollback_frames: u32,
}

impl RollbackEngine {
    /// Build a rollback engine seeded with `genesis` and the given budget.
    /// Ring-cap is `frame_budget * 4` to give head-room.
    #[must_use]
    pub fn new(genesis: SimState, frame_budget: u32) -> Self {
        let cap = ((frame_budget as usize) * 4).max(16).next_power_of_two();
        let mut states = TickRing::new(cap);
        let cur = genesis.tick;
        states.set(cur, genesis);
        Self {
            current_tick: cur,
            states,
            peers: BTreeMap::new(),
            max_rollback_frames: frame_budget,
        }
    }

    /// Register a peer slot with its own input ring.
    pub fn add_peer(&mut self, slot: PeerSlot) {
        let cap = self.states.capacity();
        self.peers.insert(slot, PeerInputs::new(cap));
    }

    /// Record a confirmed input from `peer` (may arrive late).
    /// Returns the tick distance from `current_tick` to the input ; positive
    /// means the input is in the past (rollback needed).
    pub fn record_input(&mut self, peer: PeerSlot, frame: InputFrame) -> Option<i32> {
        let entry = self.peers.entry(peer).or_insert_with(|| {
            PeerInputs::new(self.states.capacity())
        });
        let delta = frame.tick.delta(self.current_tick); // current - frame
        entry.record(frame);
        Some(delta)
    }

    /// Advance one tick using a per-peer-input collector :
    /// `inputs[peer] = confirmed-or-speculative input @ current_tick`.
    pub fn advance<S: Simulation>(&mut self, sim: &S) {
        // For determinism, fold inputs by sorted-peer-slot ; simulation impl
        // is responsible for combining a multi-peer input set into one logical
        // step. Here we synthesize a single representative `InputFrame` from
        // peer 0's stream as a simple aggregator ; production would extend
        // `Simulation::step` to take a slice.
        let cur_state = match self.states.get(self.current_tick) {
            Some(s) => s.clone(),
            None => return,
        };
        let aggregator = self
            .peers
            .iter()
            .next()
            .and_then(|(_, p)| p.at_or_speculative(self.current_tick))
            .unwrap_or_else(|| InputFrame::empty(self.current_tick, 0));
        let next = sim.step(&cur_state, &aggregator);
        let nt = next.tick;
        self.states.set(nt, next);
        self.current_tick = nt;
    }

    /// Roll back to `target_tick` and replay forward to `current_tick`.
    /// Returns Ok with the number of frames re-simulated, or BudgetExceeded.
    pub fn rollback_and_replay<S: Simulation>(
        &mut self,
        sim: &S,
        target_tick: TickId,
    ) -> Result<u32, RollbackErr> {
        let frames = self.current_tick.delta(target_tick).unsigned_abs();
        if frames > self.max_rollback_frames {
            return Err(RollbackErr::BudgetExceeded {
                requested: frames,
                budget: self.max_rollback_frames,
            });
        }
        let _baseline = self
            .states
            .get(target_tick)
            .ok_or(RollbackErr::StateGone)?
            .clone();
        let final_tick = self.current_tick;
        let mut cur = target_tick;
        let mut replayed = 0u32;
        while cur.delta(final_tick) > 0 {
            let baseline = match self.states.get(cur) {
                Some(s) => s.clone(),
                None => break,
            };
            let input = self
                .peers
                .iter()
                .next()
                .and_then(|(_, p)| p.at_or_speculative(cur))
                .unwrap_or_else(|| InputFrame::empty(cur, 0));
            let next = sim.step(&baseline, &input);
            let nt = next.tick;
            self.states.set(nt, next);
            cur = nt;
            replayed += 1;
        }
        Ok(replayed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigma::SigmaMask;
    use crate::sim::MovementStub;
    use crate::state::{Cell, CellId, CellValue};

    fn fresh_engine() -> RollbackEngine {
        let mut s0 = SimState::new(TickId::ZERO);
        s0.set(Cell {
            id: CellId(1),
            value: CellValue::V3(0, 0, 0),
            mask: SigmaMask::public(),
        });
        RollbackEngine::new(s0, DEFAULT_ROLLBACK_FRAMES)
    }

    #[test]
    fn rollback_default_budget_is_8() {
        let e = fresh_engine();
        assert_eq!(e.max_rollback_frames, 8);
    }

    #[test]
    fn rollback_within_budget_replays() {
        let mut e = fresh_engine();
        e.add_peer(PeerSlot(0));
        // Speculative fill = empty input → MovementStub leaves position unchanged.
        // Manually inject confirmed inputs in chronological order to advance.
        for t in 0..6u32 {
            let mut i = InputFrame::empty(TickId(t), t + 1);
            i.move_x = 10;
            e.record_input(PeerSlot(0), i);
            e.advance(&MovementStub);
        }
        // Now a delayed peer-0 input for tick 2 says move_x=99 instead.
        let mut late = InputFrame::empty(TickId(2), 999);
        late.move_x = 99;
        e.record_input(PeerSlot(0), late);
        let r = e.rollback_and_replay(&MovementStub, TickId(2)).unwrap();
        assert_eq!(r, 4); // ticks 3..6 replayed
    }

    #[test]
    fn rollback_exceed_budget_errors() {
        let mut e = fresh_engine();
        e.max_rollback_frames = 4;
        e.add_peer(PeerSlot(0));
        for t in 0..10u32 {
            let i = InputFrame::empty(TickId(t), t + 1);
            e.record_input(PeerSlot(0), i);
            e.advance(&MovementStub);
        }
        let r = e.rollback_and_replay(&MovementStub, TickId(0));
        assert!(matches!(r, Err(RollbackErr::BudgetExceeded { .. })));
    }
}
