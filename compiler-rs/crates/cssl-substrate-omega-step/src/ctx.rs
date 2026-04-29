//! `OmegaStepCtx` — the per-step interface a system uses to read+write state.
//!
//! § THESIS
//!   `OmegaStepCtx` is the ONLY interface an `OmegaSystem::step()` has to
//!   the outside world. By construction it does not expose :
//!     - the system clock (no `Instant::now()`)
//!     - `thread_rng()` or other entropy sources
//!     - the file-system (consent-gated; only via {Save} effect handler)
//!     - the network (consent-gated; only via {Net} effect handler)
//!   This makes systems pure-functional in `(ctx, dt)`, which is the
//!   foundation for replay-determinism.
//!
//! § PROVIDED FACILITIES
//!   - `omega()` : `&mut OmegaSnapshot` — the Ω-tensor state to mutate.
//!   - `frame()` : `u64` — the current tick-counter (monotonic).
//!   - `rng(stream)` : `&mut DetRng` — deterministic per-stream RNG.
//!   - `telemetry()` : `&mut TelemetryHook` — emit a counter, span, or event.
//!   - `halt_requested()` : `bool` — read the kill-switch ; systems may
//!     short-circuit gracefully if the scheduler has been halted.
//!   - `input_at_frame(stream)` : `Option<&InputEvent>` — fetch the
//!     replay-driven input event for this frame from the replay log
//!     (returns `None` if no event was recorded for this frame+stream).

use std::collections::BTreeMap;

use crate::omega_stub::OmegaSnapshot;
use crate::rng::{DetRng, RngStreamId};

/// Input event delivered into a step. Stage-0 form covers a small set of
/// shapes — additional variants are append-only per ABI-stability rule.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// A logical "no-op" tick — used by replay-logs to mark frames
    /// where no real input arrived.
    Tick,
    /// A keyboard keypress, by virtual keycode.
    KeyPress { keycode: u32 },
    /// A keyboard key release.
    KeyRelease { keycode: u32 },
    /// A mouse-move event in window pixels.
    MouseMove { x: i32, y: i32 },
    /// A mouse-button press (`button` 0=left, 1=right, 2=middle).
    MouseDown { button: u8 },
    /// A mouse-button release.
    MouseUp { button: u8 },
    /// A custom user-defined input variant carrying an opaque payload.
    /// Stage-0 stores the payload as a u64 so downstream systems can
    /// repurpose without bloating the enum.
    Custom { tag: u32, payload: u64 },
}

/// Telemetry hook the scheduler exposes to systems during `step()`.
///
/// § STAGE-0 FORM
///   The hook records counter increments + span events into an in-memory
///   buffer that the scheduler drains at end-of-step (phase-9 of omega_step
///   per `specs/30 § PHASES`). A real implementation routes through
///   `cssl-telemetry::ring::TelemetryRing` ; stage-0 keeps it self-contained
///   so the H2 crate stays light + the replay-determinism contract isn't
///   muddied by telemetry-ring overflow concerns.
#[derive(Debug, Default)]
pub struct TelemetryHook {
    /// Per-name monotonic counters. Replay-determinism expects identical
    /// counter outcomes across runs ; stage-0 form ensures this by using
    /// `BTreeMap` for sorted iteration.
    counters: BTreeMap<String, u64>,
    /// Number of `tick()` events the scheduler emitted across all steps.
    tick_count: u64,
}

impl TelemetryHook {
    /// Construct a fresh, empty hook.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment a named counter by 1.
    pub fn count(&mut self, name: impl Into<String>) {
        *self.counters.entry(name.into()).or_insert(0) += 1;
    }

    /// Increment a named counter by `delta`.
    pub fn count_by(&mut self, name: impl Into<String>, delta: u64) {
        *self.counters.entry(name.into()).or_insert(0) += delta;
    }

    /// Read a counter value.
    #[must_use]
    pub fn read_counter(&self, name: &str) -> u64 {
        self.counters.get(name).copied().unwrap_or(0)
    }

    /// All counters, in sorted name order. Used by replay-tests to
    /// verify determinism.
    #[must_use]
    pub fn all_counters(&self) -> Vec<(&String, &u64)> {
        self.counters.iter().collect()
    }

    /// Record a scheduler-internal "tick" event ; called by the scheduler
    /// at the end of each `step()`.
    pub fn record_tick(&mut self) {
        self.tick_count += 1;
    }

    /// Read the tick count.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }
}

/// Per-step context handed to `OmegaSystem::step()`.
///
/// § LIFETIMES
///   The context holds a borrow of the scheduler's `OmegaSnapshot` +
///   `BTreeMap<RngStreamId, DetRng>` + telemetry hook for the duration of
///   the call. Systems may NOT capture the borrow ; the scheduler reuses
///   the context across systems within a single step.
pub struct OmegaStepCtx<'a> {
    omega: &'a mut OmegaSnapshot,
    rngs: &'a mut BTreeMap<RngStreamId, DetRng>,
    telemetry: &'a mut TelemetryHook,
    frame: u64,
    halt_requested: bool,
    /// Per-frame input map : (stream-id) -> InputEvent for replay-driven systems.
    inputs: &'a BTreeMap<RngStreamId, InputEvent>,
}

impl<'a> OmegaStepCtx<'a> {
    /// Construct from the scheduler's per-step component refs. Internal —
    /// the scheduler is the only legitimate caller, but `pub` so test
    /// fixtures can build a synthetic context.
    pub fn new(
        omega: &'a mut OmegaSnapshot,
        rngs: &'a mut BTreeMap<RngStreamId, DetRng>,
        telemetry: &'a mut TelemetryHook,
        frame: u64,
        halt_requested: bool,
        inputs: &'a BTreeMap<RngStreamId, InputEvent>,
    ) -> Self {
        Self {
            omega,
            rngs,
            telemetry,
            frame,
            halt_requested,
            inputs,
        }
    }

    /// Mutable access to the Ω-tensor snapshot — the canonical state.
    pub fn omega(&mut self) -> &mut OmegaSnapshot {
        self.omega
    }

    /// Read-only access to the snapshot. Useful when a system needs to
    /// inspect state without committing to a mutation.
    #[must_use]
    pub fn omega_ref(&self) -> &OmegaSnapshot {
        self.omega
    }

    /// Current frame number.
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Whether the scheduler has been halted. Systems should short-circuit
    /// gracefully when this is `true` (sim ← skip, render ← black-frame).
    #[must_use]
    pub fn halt_requested(&self) -> bool {
        self.halt_requested
    }

    /// Mutable access to the telemetry hook.
    pub fn telemetry(&mut self) -> &mut TelemetryHook {
        self.telemetry
    }

    /// Borrow a deterministic RNG stream by id. Returns `None` if the
    /// system did not declare this stream in its `rng_streams()` ; the
    /// caller should surface this as `OmegaError::RngStreamUnregistered`.
    pub fn rng(&mut self, stream: RngStreamId) -> Option<&mut DetRng> {
        self.rngs.get_mut(&stream)
    }

    /// Look up the input event for this frame at the given stream-id.
    /// Returns `None` if no event was recorded for this frame.
    #[must_use]
    pub fn input(&self, stream: RngStreamId) -> Option<&InputEvent> {
        self.inputs.get(&stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_hook_count_basic() {
        let mut h = TelemetryHook::new();
        h.count("frames");
        h.count("frames");
        h.count_by("bytes", 1024);
        assert_eq!(h.read_counter("frames"), 2);
        assert_eq!(h.read_counter("bytes"), 1024);
        assert_eq!(h.read_counter("nonexistent"), 0);
    }

    #[test]
    fn telemetry_hook_counters_sorted() {
        let mut h = TelemetryHook::new();
        h.count("zebra");
        h.count("apple");
        h.count("mango");
        let names: Vec<&String> = h.all_counters().iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            [
                &"apple".to_string(),
                &"mango".to_string(),
                &"zebra".to_string()
            ]
        );
    }

    #[test]
    fn telemetry_tick_count() {
        let mut h = TelemetryHook::new();
        assert_eq!(h.tick_count(), 0);
        h.record_tick();
        h.record_tick();
        h.record_tick();
        assert_eq!(h.tick_count(), 3);
    }

    #[test]
    fn ctx_exposes_frame() {
        let mut omega = OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let ctx = OmegaStepCtx::new(&mut omega, &mut rngs, &mut telem, 7, false, &inputs);
        assert_eq!(ctx.frame(), 7);
        assert!(!ctx.halt_requested());
    }

    #[test]
    fn ctx_input_lookup() {
        let mut omega = OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let mut inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        inputs.insert(RngStreamId(0), InputEvent::KeyPress { keycode: 65 });
        let ctx = OmegaStepCtx::new(&mut omega, &mut rngs, &mut telem, 0, false, &inputs);
        assert!(matches!(
            ctx.input(RngStreamId(0)),
            Some(InputEvent::KeyPress { keycode: 65 })
        ));
        assert!(ctx.input(RngStreamId(99)).is_none());
    }

    #[test]
    fn ctx_rng_lookup_unregistered_is_none() {
        let mut omega = OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let mut ctx = OmegaStepCtx::new(&mut omega, &mut rngs, &mut telem, 0, false, &inputs);
        assert!(ctx.rng(RngStreamId(0)).is_none());
    }

    #[test]
    fn ctx_omega_mutation_visible() {
        let mut omega = OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        {
            let mut ctx = OmegaStepCtx::new(&mut omega, &mut rngs, &mut telem, 0, false, &inputs);
            ctx.omega()
                .sim
                .insert("counter".into(), crate::omega_stub::OmegaStubField::Int(42));
        }
        assert_eq!(
            omega.sim.get("counter"),
            Some(&crate::omega_stub::OmegaStubField::Int(42))
        );
    }

    #[test]
    fn input_event_variants_distinguishable() {
        let a = InputEvent::Tick;
        let b = InputEvent::KeyPress { keycode: 1 };
        assert_ne!(a, b);
    }
}
