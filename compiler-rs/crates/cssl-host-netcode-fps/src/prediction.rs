// § prediction.rs : ClientPrediction — input → predict-state → render-locally
//
// Quake/Source/Valve-style client-side prediction. The client doesn't wait for
// the server to confirm an input before rendering its effect ; it runs the
// SAME deterministic simulation locally, tagging each predicted frame with
// the originating input's tick-id so reconciliation can later compare.
//
// Flow per tick :
//   1. Client samples raw HID → `InputFrame` at tick=N.
//   2. `ClientPrediction::predict` :
//        a. stores input in `inputs` ring (for resend on packet-loss + replay
//           during reconciliation),
//        b. runs `Simulation::step(state[N], input[N]) → state[N+1]`,
//        c. stores predicted state in `predicted` ring,
//        d. returns predicted-state for the renderer to consume.
//
// `last_authoritative` tracks the most-recent server-confirmed tick ; the
// `Reconciliation` module uses this with the input-ring to detect + correct
// mispredictions.

use crate::input::InputFrame;
use crate::ring::TickRing;
use crate::sim::Simulation;
use crate::state::SimState;
use crate::tick::TickId;

/// Default ring-cap for client-prediction (1 sec @ 60 Hz × 2 head-room).
pub const DEFAULT_PREDICT_RING: usize = 128;

/// Errors from the prediction pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredictErr {
    /// Tried to predict from a baseline state we no longer hold.
    BaselineGone,
    /// Input tick doesn't match the current prediction tick.
    InputTickMismatch { expected: TickId, got: TickId },
}

/// Per-client prediction state-machine.
#[derive(Debug, Clone)]
pub struct ClientPrediction {
    /// Most-recent input we predicted from, indexed by tick.
    pub inputs: TickRing<InputFrame>,
    /// Locally-predicted simulation states, indexed by tick.
    pub predicted: TickRing<SimState>,
    /// The newest tick the server has acknowledged (state we will rewind from).
    pub last_authoritative_tick: Option<TickId>,
    /// The tick we are about to predict NEXT (state at this tick is in
    /// `predicted` once `predict` runs).
    pub next_predict_tick: TickId,
}

impl ClientPrediction {
    /// Build a fresh predictor seeded with `genesis` state at tick=0.
    #[must_use]
    pub fn new(genesis: SimState, ring_cap: usize) -> Self {
        let mut predicted = TickRing::new(ring_cap);
        let next = genesis.tick.next();
        predicted.set(genesis.tick, genesis);
        Self {
            inputs: TickRing::new(ring_cap),
            predicted,
            last_authoritative_tick: None,
            next_predict_tick: next,
        }
    }

    /// Run one prediction step. Returns the freshly-predicted state.
    pub fn predict<S: Simulation>(
        &mut self,
        sim: &S,
        input: InputFrame,
    ) -> Result<&SimState, PredictErr> {
        if input.tick != self.next_predict_tick.back(1) {
            // We always predict using the input timestamped at the PREVIOUS tick :
            // state @ N + input @ N → state @ N+1.
            return Err(PredictErr::InputTickMismatch {
                expected: self.next_predict_tick.back(1),
                got: input.tick,
            });
        }
        let baseline_tick = self.next_predict_tick.back(1);
        let baseline = self
            .predicted
            .get(baseline_tick)
            .ok_or(PredictErr::BaselineGone)?
            .clone();

        let next_state = sim.step(&baseline, &input);
        let next_tick = next_state.tick;

        self.inputs.set(input.tick, input);
        self.predicted.set(next_tick, next_state);
        self.next_predict_tick = next_tick.next();
        // SAFE : we just inserted ; non-None.
        Ok(self.predicted.get(next_tick).unwrap())
    }

    /// Latest predicted tick that has a state present.
    #[must_use]
    pub fn latest_predicted_tick(&self) -> TickId {
        self.next_predict_tick.back(1)
    }

    /// Look up the predicted state for `tick`.
    #[must_use]
    pub fn predicted_at(&self, tick: TickId) -> Option<&SimState> {
        self.predicted.get(tick)
    }

    /// Look up the input at `tick`.
    #[must_use]
    pub fn input_at(&self, tick: TickId) -> Option<&InputFrame> {
        self.inputs.get(tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::action;
    use crate::sigma::SigmaMask;
    use crate::sim::{MovementStub, NoopSim};
    use crate::state::{Cell, CellId, CellValue};

    #[test]
    fn predict_advances_tick() {
        let s0 = SimState::new(TickId::ZERO);
        let mut cp = ClientPrediction::new(s0, 64);
        let input = InputFrame::empty(TickId::ZERO, 1);
        let s1 = cp.predict(&NoopSim, input).unwrap();
        assert_eq!(s1.tick, TickId(1));
        assert_eq!(cp.latest_predicted_tick(), TickId(1));
    }

    #[test]
    fn predict_records_input_action_via_noop() {
        let s0 = SimState::new(TickId::ZERO);
        let mut cp = ClientPrediction::new(s0, 64);
        let mut input = InputFrame::empty(TickId::ZERO, 1);
        input.actions = action::FIRE;
        let _ = cp.predict(&NoopSim, input).unwrap();
        let s1 = cp.predicted_at(TickId(1)).unwrap();
        match s1.get(CellId(0)).unwrap().value {
            CellValue::Count(c) => assert_eq!(c, action::FIRE),
            _ => panic!("expected Count"),
        }
    }

    #[test]
    fn predict_input_tick_mismatch_errors() {
        let s0 = SimState::new(TickId::ZERO);
        let mut cp = ClientPrediction::new(s0, 64);
        let input = InputFrame::empty(TickId(99), 1);
        let r = cp.predict(&NoopSim, input);
        assert!(matches!(r, Err(PredictErr::InputTickMismatch { .. })));
    }

    #[test]
    fn predict_chain_movement_accumulates() {
        let mut s0 = SimState::new(TickId::ZERO);
        s0.set(Cell {
            id: CellId(1),
            value: CellValue::V3(0, 0, 0),
            mask: SigmaMask::public(),
        });
        let mut cp = ClientPrediction::new(s0, 64);
        for t in 0..5u32 {
            let mut i = InputFrame::empty(TickId(t), t + 1);
            i.move_x = 10;
            cp.predict(&MovementStub, i).unwrap();
        }
        let s5 = cp.predicted_at(TickId(5)).unwrap();
        let p = s5.get(CellId(1)).unwrap();
        assert_eq!(p.value, CellValue::V3(50, 0, 0));
    }
}
