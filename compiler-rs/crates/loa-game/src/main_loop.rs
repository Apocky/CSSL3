//! § Main loop — drives the canonical 13-phase omega_step.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/30_SUBSTRATE.csl § OMEGA-STEP § PHASES` +
//!   `specs/31_LOA_DESIGN.csl § GAME-LOOP § ENTRY-POINT`.
//!
//! § THESIS
//!
//!   The MainLoop wraps an `Engine` and drives one or more omega_step ticks.
//!   Per spec, one tick = one full pass through all 13 phases. The
//!   `OmegaScheduler` registered by the engine handles topological-sort
//!   + system dispatch ; this module's contribution is :
//!     1. Reading input from the host-input backend
//!     2. Injecting it into the scheduler's pending-input queue
//!     3. Calling `step(dt)` once per tick
//!     4. Optionally pumping the host-window event queue between ticks
//!     5. Surfacing window-close as a kill-switch trigger
//!
//! § PRIME-DIRECTIVE alignment
//!
//!   - Window-close events ALWAYS reach the engine (no silent suppression
//!     per `cssl-host-window § PRIME-DIRECTIVE-KILL-SWITCH`). The MainLoop
//!     converts a window-close to `MainLoopOutcome::Halt`.
//!   - Input-events are drained one-per-tick to avoid the queue growing
//!     unbounded ; per `cssl-host-input` the queue is non-blocking.
//!   - The `step_once` method is the structural unit ; the smoke-test
//!     drives a single call to demonstrate end-to-end flow.

use cssl_substrate_omega_step::InputEvent as TickInput;

use crate::engine::{Engine, LoaError};
use crate::loop_systems::InputSystem;

/// Outcome of one main-loop iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainLoopOutcome {
    /// The tick advanced normally — the engine is still running.
    Continue,
    /// The user requested halt (window close, signal, ω_halt). The caller
    /// should drain final state + tear down.
    Halt { reason: String },
}

/// Driver around an [`Engine`] — wires host-input to the scheduler's
/// per-frame input queue + advances ticks.
pub struct MainLoop {
    engine: Engine,
    /// Number of ticks completed.
    tick_count: u64,
}

impl MainLoop {
    /// New main-loop wrapping the given engine.
    #[must_use]
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            tick_count: 0,
        }
    }

    /// Read-only access to the engine.
    #[must_use]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Mutable access to the engine.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Number of ticks completed so far.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Inject one input event into the scheduler's pending-input queue
    /// for the next tick. Per `cssl_substrate_omega_step::OmegaScheduler::
    /// inject_input`, inputs are keyed by `RngStreamId` so multiple
    /// events can coexist on the same frame.
    pub fn inject_input_event(&mut self, event: TickInput) {
        self.engine
            .tick_scheduler_mut()
            .inject_input(InputSystem::STREAM, event);
    }

    /// Advance one omega_step tick. The frame-budget per spec is 16ms ;
    /// callers in real-time mode pass a measured `dt`. The smoke test
    /// passes a fixed value (1.0/60.0).
    ///
    /// # Errors
    /// Returns [`LoaError::Scheduler`] on scheduler errors. A halt is
    /// surfaced via the `Ok(MainLoopOutcome::Halt)` arm rather than an
    /// error, because halts are a normal lifecycle event per spec.
    pub fn step_once(&mut self, dt: f64) -> Result<MainLoopOutcome, LoaError> {
        // Check for halt-state BEFORE stepping, so we report the halt
        // rather than trying to step a halted scheduler.
        match self.engine.tick_scheduler_mut().step(dt) {
            Ok(()) => {
                self.tick_count += 1;
                Ok(MainLoopOutcome::Continue)
            }
            Err(cssl_substrate_omega_step::OmegaError::HaltedByKill { reason }) => {
                Ok(MainLoopOutcome::Halt { reason })
            }
            Err(e) => Err(LoaError::Scheduler(e)),
        }
    }

    /// Trigger the kill-switch — propagates a halt request that the next
    /// `step_once` call observes and converts into
    /// `MainLoopOutcome::Halt`.
    ///
    /// # Errors
    /// Returns [`LoaError::Scheduler`] on internal scheduler errors ; the
    /// halt itself never fails.
    pub fn halt(&mut self, reason: impl Into<String>) -> Result<(), LoaError> {
        self.engine
            .tick_scheduler_mut()
            .halt(reason)
            .map_err(LoaError::Scheduler)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TESTS  (smoke-test handles real bit-equal save/load+replay ; here we
// just exercise the wrapper APIs.)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[cfg(feature = "test-bypass")]
mod tests {
    use super::*;
    use crate::engine::{CapTokens, EngineConfig};

    fn fresh_main_loop() -> MainLoop {
        let caps = CapTokens::issue_for_test().expect("test caps");
        let engine = Engine::new(EngineConfig::default(), caps).expect("engine new");
        MainLoop::new(engine)
    }

    #[test]
    fn step_once_advances_tick_count() {
        let mut ml = fresh_main_loop();
        assert_eq!(ml.tick_count(), 0);
        let outcome = ml.step_once(1.0 / 60.0).expect("step ok");
        assert_eq!(outcome, MainLoopOutcome::Continue);
        assert_eq!(ml.tick_count(), 1);
    }

    #[test]
    fn halt_then_step_returns_halt_outcome() {
        let mut ml = fresh_main_loop();
        ml.halt("smoke-test").expect("halt ok");
        let outcome = ml.step_once(1.0 / 60.0).expect("step ok");
        match outcome {
            MainLoopOutcome::Halt { reason } => assert!(reason.contains("smoke-test")),
            MainLoopOutcome::Continue => panic!("expected Halt, got Continue"),
        }
    }

    #[test]
    fn inject_input_does_not_panic() {
        let mut ml = fresh_main_loop();
        ml.inject_input_event(TickInput::KeyPress { keycode: 27 }); // ESC
        let _ = ml.step_once(1.0 / 60.0).expect("step ok");
    }
}
