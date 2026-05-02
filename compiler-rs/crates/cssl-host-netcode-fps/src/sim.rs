// § sim.rs : Simulation trait — pure-functional step from (state, input) → state
//
// Every netcode primitive in this crate is generic over a `Simulation` trait
// implementor. The crate ships a `NoopSim` for tests and exposes the trait
// for upstream W13-2 weapons / W13-6 movement / W13-8 loot to plug their own
// step-function into. The contract :
//
//   given (state @ tick=N, input @ tick=N) → state @ tick=N+1
//
// MUST be pure — no I/O, no clock-reads, no RNG outside the seeded PRNG
// passed via state. This is what makes rollback / reconciliation correct.

use crate::input::InputFrame;
use crate::state::SimState;

/// Pure-functional one-tick advance. `prev` MUST not be mutated by impl.
pub trait Simulation {
    /// Advance one tick. `prev.tick` + 1 = returned state's tick.
    fn step(&self, prev: &SimState, input: &InputFrame) -> SimState;
}

/// Test/doc-grade no-op simulation : copies state forward, increments tick,
/// records input.actions into cell-id 0 as a Count. Useful for prediction +
/// reconciliation tests without dragging in real game-logic.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSim;

impl Simulation for NoopSim {
    fn step(&self, prev: &SimState, input: &InputFrame) -> SimState {
        let mut next = prev.clone();
        next.tick = prev.tick.next();
        // Record action bitmask in cell-0 so tests can observe state-evolution.
        let cell = crate::state::Cell {
            id: crate::state::CellId(0),
            value: crate::state::CellValue::Count(input.actions),
            mask: crate::sigma::SigmaMask::public(),
        };
        next.set(cell);
        next
    }
}

/// "Movement-stub" sim : advances move_x/move_y into cell-id 1 as V3 position
/// (z = 0). Used by prediction tests to verify input changes propagate.
#[derive(Debug, Default, Clone, Copy)]
pub struct MovementStub;

impl Simulation for MovementStub {
    fn step(&self, prev: &SimState, input: &InputFrame) -> SimState {
        let mut next = prev.clone();
        next.tick = prev.tick.next();
        let prev_pos = prev
            .get(crate::state::CellId(1))
            .map(|c| match c.value {
                crate::state::CellValue::V3(x, y, z) => (x, y, z),
                _ => (0, 0, 0),
            })
            .unwrap_or((0, 0, 0));
        let nx = prev_pos.0.saturating_add(input.move_x);
        let ny = prev_pos.1.saturating_add(input.move_y);
        let cell = crate::state::Cell {
            id: crate::state::CellId(1),
            value: crate::state::CellValue::V3(nx, ny, prev_pos.2),
            mask: crate::sigma::SigmaMask::public(),
        };
        next.set(cell);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigma::SigmaMask;
    use crate::state::{Cell, CellId, CellValue};
    use crate::tick::TickId;

    #[test]
    fn noop_sim_advances_tick() {
        let s0 = SimState::new(TickId(5));
        let i = InputFrame::empty(TickId(5), 1);
        let s1 = NoopSim.step(&s0, &i);
        assert_eq!(s1.tick, TickId(6));
    }

    #[test]
    fn movement_stub_accumulates_position() {
        let mut s0 = SimState::new(TickId::ZERO);
        s0.set(Cell {
            id: CellId(1),
            value: CellValue::V3(0, 0, 0),
            mask: SigmaMask::public(),
        });
        let mut i = InputFrame::empty(TickId::ZERO, 1);
        i.move_x = 100;
        i.move_y = 50;
        let s1 = MovementStub.step(&s0, &i);
        let s2 = MovementStub.step(&s1, &i);
        let p = s2.get(CellId(1)).unwrap();
        assert_eq!(p.value, CellValue::V3(200, 100, 0));
    }
}
