// § reconciliation.rs : ServerReconciliation — rewind + replay on misprediction
//
// When the server sends back its authoritative state-at-tick=N, the client :
//   1. Compares server-state[N] with predicted-state[N] in the ring.
//   2. If equal → no-op (perfect prediction).
//   3. If unequal → MISPREDICTION :
//        a. Replace predicted[N] with server[N].
//        b. Re-run sim from N forward over the still-pending inputs in the
//           input-ring, replacing predicted[N+1..latest].
//   4. Updates `last_authoritative_tick` regardless.
//
// This is the Quake/Source/Overwatch core trick : the client renders ahead of
// the server but corrects without visible jumps so long as the simulation is
// deterministic and the input-ring is intact.

use crate::prediction::ClientPrediction;
use crate::sim::Simulation;
use crate::state::SimState;

/// Result of a reconciliation pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileReport {
    /// True if the server-state matched the client's prediction exactly.
    pub matched: bool,
    /// Number of ticks replayed forward from the corrected baseline.
    pub replayed: u32,
    /// True if the server's tick was older than `last_authoritative_tick`
    /// (out-of-order packet ; ignored).
    pub stale: bool,
}

/// Apply a server-authoritative state. Returns a report describing whether
/// a misprediction happened + how many ticks were replayed.
pub fn reconcile<S: Simulation>(
    cp: &mut ClientPrediction,
    sim: &S,
    server_state: SimState,
) -> ReconcileReport {
    let server_tick = server_state.tick;

    if let Some(last) = cp.last_authoritative_tick {
        if server_tick.delta(last) >= 0 {
            // server_tick <= last → stale ; ignore
            return ReconcileReport {
                matched: false,
                replayed: 0,
                stale: true,
            };
        }
    }

    let predicted_at = cp.predicted_at(server_tick).cloned();
    let matched = match &predicted_at {
        Some(p) => p.cells_eq(&server_state),
        None => false,
    };

    if matched {
        cp.last_authoritative_tick = Some(server_tick);
        return ReconcileReport {
            matched: true,
            replayed: 0,
            stale: false,
        };
    }

    // Misprediction (or no prior prediction at this tick) → overwrite + replay.
    cp.predicted.set(server_tick, server_state);
    cp.last_authoritative_tick = Some(server_tick);

    let latest = cp.latest_predicted_tick();
    let mut replayed = 0u32;
    if server_tick.delta(latest) > 0 {
        let mut cur = server_tick;
        while cur.delta(latest) > 0 {
            let baseline = match cp.predicted.get(cur) {
                Some(s) => s.clone(),
                None => break,
            };
            let input = match cp.inputs.get(cur).cloned() {
                Some(i) => i,
                None => break, // missing input ; can't replay further
            };
            let next = sim.step(&baseline, &input);
            let next_tick = next.tick;
            cp.predicted.set(next_tick, next);
            cur = next_tick;
            replayed += 1;
        }
    }

    ReconcileReport {
        matched: false,
        replayed,
        stale: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputFrame;
    use crate::prediction::ClientPrediction;
    use crate::sigma::SigmaMask;
    use crate::sim::{MovementStub, NoopSim};
    use crate::state::{Cell, CellId, CellValue};
    use crate::tick::TickId;

    fn seeded_movement_predictor() -> ClientPrediction {
        let mut s0 = SimState::new(TickId::ZERO);
        s0.set(Cell {
            id: CellId(1),
            value: CellValue::V3(0, 0, 0),
            mask: SigmaMask::public(),
        });
        ClientPrediction::new(s0, 64)
    }

    #[test]
    fn perfect_prediction_matches_no_replay() {
        let mut cp = seeded_movement_predictor();
        for t in 0..5u32 {
            let mut i = InputFrame::empty(TickId(t), t + 1);
            i.move_x = 10;
            cp.predict(&MovementStub, i).unwrap();
        }
        // Server confirms exactly the same outcome at tick 3.
        let server_t3 = cp.predicted_at(TickId(3)).cloned().unwrap();
        let r = reconcile(&mut cp, &MovementStub, server_t3);
        assert!(r.matched);
        assert_eq!(r.replayed, 0);
        assert_eq!(cp.last_authoritative_tick, Some(TickId(3)));
    }

    #[test]
    fn misprediction_replays_remaining_inputs() {
        let mut cp = seeded_movement_predictor();
        for t in 0..5u32 {
            let mut i = InputFrame::empty(TickId(t), t + 1);
            i.move_x = 10;
            cp.predict(&MovementStub, i).unwrap();
        }
        // Server says : at tick 3, position is actually (5, 0, 0) not (30, 0, 0).
        let mut server_t3 = SimState::new(TickId(3));
        server_t3.set(Cell {
            id: CellId(1),
            value: CellValue::V3(5, 0, 0),
            mask: SigmaMask::public(),
        });
        let r = reconcile(&mut cp, &MovementStub, server_t3);
        assert!(!r.matched);
        assert_eq!(r.replayed, 2, "ticks 4 + 5 replayed from corrected baseline");
        // After replay : tick 5 = (5 + 10 + 10) = 25
        let s5 = cp.predicted_at(TickId(5)).unwrap();
        match s5.get(CellId(1)).unwrap().value {
            CellValue::V3(x, _, _) => assert_eq!(x, 25),
            _ => panic!(),
        }
    }

    #[test]
    fn stale_authoritative_state_is_ignored() {
        let mut cp = seeded_movement_predictor();
        for t in 0..3u32 {
            let i = InputFrame::empty(TickId(t), t + 1);
            cp.predict(&NoopSim, i).unwrap();
        }
        let s3 = cp.predicted_at(TickId(3)).cloned().unwrap();
        let _ = reconcile(&mut cp, &NoopSim, s3);
        // Now ship a stale server-state (older tick).
        let stale_state = SimState::new(TickId(1));
        let r = reconcile(&mut cp, &NoopSim, stale_state);
        assert!(r.stale);
        assert_eq!(cp.last_authoritative_tick, Some(TickId(3)));
    }
}
