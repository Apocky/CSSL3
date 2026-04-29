//! `OmegaScheduler` — registers `OmegaSystem`s + drives them through `step()`.
//!
//! § THESIS
//!   The scheduler is the engine-room of the Substrate's TIME advance.
//!   Per `specs/30_SUBSTRATE.csl § OMEGA-STEP`, it :
//!     1. Stable-topo-sorts registered systems by their dep declarations.
//!     2. Calls each system's `step()` in canonical order, feeding it
//!        a per-step `OmegaStepCtx` constructed from the snapshot.
//!     3. Honors halt requests within at most 1 tick.
//!     4. Records replay-log entries when configured.
//!     5. Emits a telemetry tick at the end of every step.
//!
//! § CONFIGURATION
//!   Scheduler behavior is controlled via `SchedulerConfig` :
//!     - `master_seed` : seed for the deterministic RNG tree
//!     - `frame_budget_s` : wallclock budget per step (used by overbudget policy)
//!     - `overbudget_policy` : Halt | Degrade
//!     - `record_replay` : whether to populate a `ReplayLog` as we go
//!
//! § LOAD-BEARING DESIGN INVARIANTS
//!   - **Bit-identical replay** : Two schedulers with the same config +
//!     same registered systems (same names, same dep declarations, same
//!     `effect_row`) + same input stream produce bit-equal `OmegaSnapshot`
//!     outputs at every frame.
//!   - **Halt within 1 tick** : `halt(reason)` sets a flag that `step()`
//!     observes at the start of the next call ; `step()` then returns
//!     `Err(OmegaError::HaltedByKill)` after writing one final telemetry
//!     tick + audit-style entry to the replay log (if recording).
//!   - **No silent fallbacks** : a system's panic returns
//!     `OmegaError::SystemPanicked` ; consent revocation returns
//!     `OmegaError::ConsentRevoked` ; never "skip + continue".

use std::collections::BTreeMap;

use crate::consent::{CapsGrant, OmegaCapability};
use crate::ctx::{InputEvent, OmegaStepCtx, TelemetryHook};
use crate::dep_graph::{topo_sort_stable, DepGraphError};
use crate::determinism::{classify_determinism, DeterminismMode};
use crate::error::OmegaError;
use crate::halt::{HaltState, HaltToken};
use crate::omega_stub::OmegaSnapshot;
use crate::replay::{ReplayEntry, ReplayLog};
use crate::rng::{DetRng, RngStreamId};
use crate::system::{OmegaSystem, SystemId};

/// Scheduler-wide policy for handling frame-overbudget conditions.
///
/// § PER `specs/30 § omega_step § PRIME-DIRECTIVE`, the scheduler MUST
///   either halt or degrade gracefully when a step exceeds its time
///   budget. Silent over-budget steps are a bug per `§ OMEGA-STEP §
///   PRIME-DIRECTIVE-ALIGNMENT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverbudgetPolicy {
    /// Stop the scheduler + return `OmegaError::FrameOverbudget`.
    Halt,
    /// Continue but emit a telemetry warning frame. Default.
    Degrade,
}

/// Scheduler configuration. All fields are read at scheduler construction
/// time + are immutable thereafter.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Master seed for the per-stream RNG tree. Same seed + same input
    /// stream ⇒ bit-equal Ω-tensor outputs.
    pub master_seed: u64,
    /// Wallclock budget per `step()` in seconds. The scheduler doesn't
    /// measure wallclock itself ; the `dt` argument to `step()` is treated
    /// as the spent budget. A wrapper that measures real time + calls
    /// `step()` with the measured `dt` is the canonical pattern.
    pub frame_budget_s: f64,
    /// What to do when `dt > frame_budget_s`.
    pub overbudget_policy: OverbudgetPolicy,
    /// Whether the scheduler should populate a `ReplayLog` as it runs.
    pub record_replay: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            master_seed: 0,
            // 1/60 sec = 16.67 ms — the canonical 60Hz tick budget.
            frame_budget_s: 1.0_f64 / 60.0_f64,
            overbudget_policy: OverbudgetPolicy::Degrade,
            record_replay: false,
        }
    }
}

/// One registered system + its book-keeping.
struct Registered {
    id: SystemId,
    name: String,
    system: Box<dyn OmegaSystem>,
}

/// The omega-step scheduler. See module docs for behavior.
pub struct OmegaScheduler {
    config: SchedulerConfig,
    systems: Vec<Registered>,
    /// Insertion-ordered system-ids. Used by `topo_sort_stable` for tie-breaking.
    insertion_order: Vec<SystemId>,
    /// Per-(system, stream) deterministic RNGs. The scheduler indexes by
    /// stream-id only ; multiple systems sharing a stream-id intentionally
    /// share the same RNG state (this is rarely the right shape — most
    /// systems should pick distinct stream-ids — but the scheduler does
    /// not enforce uniqueness).
    rngs: BTreeMap<RngStreamId, DetRng>,
    /// Per-system declared stream-ids — used to guard `ctx.rng()` calls
    /// against undeclared stream usage.
    declared_streams: BTreeMap<SystemId, Vec<RngStreamId>>,
    /// Current telemetry hook (recreated each step, but the `tick_count`
    /// is preserved across steps).
    telemetry: TelemetryHook,
    /// The current Ω-tensor snapshot.
    omega: OmegaSnapshot,
    /// Frame counter — monotonically increasing.
    frame: u64,
    /// Halt token — shared with external halt() callers.
    halt: HaltToken,
    /// `Some(reason)` if a previous `step()` already returned `HaltedByKill`.
    halted_with: Option<String>,
    /// Determinism classification, recorded once at construction.
    determinism_mode: DeterminismMode,
    /// Replay log — populated only if `config.record_replay = true`.
    replay_log: Option<ReplayLog>,
    /// Next system-id to issue.
    next_id: u64,
    /// Per-frame queued input map. Set via `inject_input` ; consumed by
    /// the next `step()` call. After the step, the map is cleared.
    pending_inputs: BTreeMap<RngStreamId, InputEvent>,
    /// Replay-mode input queue, keyed by frame number. Populated by
    /// `replay_from(log)` ; consumed by `step_through_replay()`. Empty
    /// for normal scheduling.
    replay_inputs: BTreeMap<u64, Vec<(RngStreamId, InputEvent)>>,
}

impl OmegaScheduler {
    /// Construct a fresh scheduler with the supplied config. The scheduler
    /// runs the determinism probes once at construction time + records
    /// the resulting `DeterminismMode`.
    #[must_use]
    pub fn new(config: SchedulerConfig) -> Self {
        let determinism_mode = classify_determinism();
        let replay_log = if config.record_replay {
            Some(ReplayLog::new(config.master_seed))
        } else {
            None
        };
        Self {
            config,
            systems: Vec::new(),
            insertion_order: Vec::new(),
            rngs: BTreeMap::new(),
            declared_streams: BTreeMap::new(),
            telemetry: TelemetryHook::new(),
            omega: OmegaSnapshot::new(),
            frame: 0,
            halt: HaltToken::new(),
            halted_with: None,
            determinism_mode,
            replay_log,
            next_id: 0,
            pending_inputs: BTreeMap::new(),
            replay_inputs: BTreeMap::new(),
        }
    }

    /// Construct with the default config + the supplied master seed.
    /// Convenience for the common case.
    #[must_use]
    pub fn with_seed(master_seed: u64) -> Self {
        Self::new(SchedulerConfig {
            master_seed,
            ..Default::default()
        })
    }

    /// The determinism classification recorded at construction.
    #[must_use]
    pub fn determinism_mode(&self) -> DeterminismMode {
        self.determinism_mode
    }

    /// Read the current frame counter.
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Read the halt state.
    #[must_use]
    pub fn halt_state(&self) -> HaltState {
        if self.halted_with.is_some() || self.halt.is_triggered() {
            HaltState::Halted
        } else {
            HaltState::Running
        }
    }

    /// Read the current Ω-tensor snapshot.
    #[must_use]
    pub fn omega(&self) -> &OmegaSnapshot {
        &self.omega
    }

    /// Read the telemetry hook.
    #[must_use]
    pub fn telemetry(&self) -> &TelemetryHook {
        &self.telemetry
    }

    /// Read the replay log if recording is enabled.
    #[must_use]
    pub fn replay_log(&self) -> Option<&ReplayLog> {
        self.replay_log.as_ref()
    }

    /// Take the replay log out of the scheduler ; subsequent steps will
    /// not record (recording can be re-enabled by setting one back).
    pub fn take_replay_log(&mut self) -> Option<ReplayLog> {
        self.replay_log.take()
    }

    /// Set / replace the replay log used for recording.
    pub fn set_replay_log(&mut self, log: Option<ReplayLog>) {
        self.replay_log = log;
    }

    /// Inject an input event for the next `step()`. Multiple events can
    /// be injected per step ; they are keyed by `RngStreamId` so a
    /// keyboard event + a mouse event can coexist on the same frame.
    pub fn inject_input(&mut self, stream: RngStreamId, event: InputEvent) {
        self.pending_inputs.insert(stream, event);
    }

    /// Register a new `OmegaSystem`. Requires a valid `CapsGrant` for
    /// `OmegaCapability::OmegaRegister`. Returns the assigned `SystemId`.
    ///
    /// § VALIDATIONS
    ///   - Grant must cover `OmegaRegister` + still be active.
    ///   - System name must be unique.
    ///   - Effect row must validate (must contain `{Sim}` at minimum).
    ///   - For `{PureDet}`-style systems, the determinism mode must be
    ///     `Strict`. We approximate this stage-0 by requiring `{Audio}`
    ///     systems (which imply `{PureDet}`) to be registered only on
    ///     `Strict` schedulers.
    ///   - All declared `rng_streams()` get a fresh `DetRng` allocated.
    pub fn register<S: OmegaSystem + 'static>(
        &mut self,
        system: S,
        grant: &CapsGrant,
    ) -> Result<SystemId, OmegaError> {
        // Consent gate.
        grant
            .require(OmegaCapability::OmegaRegister)
            .map_err(|_| OmegaError::ConsentRevoked {
                system: SystemId(self.next_id),
                gate: "omega_register",
            })?;

        let name = system.name().to_string();
        // Uniqueness check.
        if self.systems.iter().any(|r| r.name == name) {
            return Err(OmegaError::DuplicateName { name });
        }

        // Effect-row validity.
        let row = system.effect_row();
        if let Some(reason) = row.validate() {
            return Err(OmegaError::DeterminismViolation {
                frame: self.frame,
                kind: match reason {
                    "no-Sim" => "effect-row-missing-Sim",
                    "Net-without-Replay-or-consent" => "effect-row-Net-bare",
                    _ => "effect-row-invalid",
                },
            });
        }

        // PureDet check : Audio systems require Strict mode (per
        // specs/30 § SUBSTRATE-EFFECTS Audio implies PureDet).
        if row.contains(crate::effect_row::SubstrateEffect::Audio)
            && self.determinism_mode != DeterminismMode::Strict
        {
            return Err(OmegaError::DeterminismViolation {
                frame: self.frame,
                kind: "Audio-system-on-non-Strict-determinism",
            });
        }

        // Allocate the SystemId.
        let id = SystemId(self.next_id);
        self.next_id += 1;

        // Pre-allocate per-stream RNGs.
        let streams: Vec<RngStreamId> = system.rng_streams().to_vec();
        for stream in &streams {
            self.rngs
                .entry(*stream)
                .or_insert_with(|| DetRng::new(self.config.master_seed, *stream));
        }
        self.declared_streams.insert(id, streams);

        self.systems.push(Registered {
            id,
            name,
            system: Box::new(system),
        });
        self.insertion_order.push(id);
        Ok(id)
    }

    /// Trigger the kill-switch with a reason. The next `step()` will
    /// return `Err(OmegaError::HaltedByKill)`.
    ///
    /// § INVARIANT (per `specs/30 § ω_halt()`) : honored within 1 tick.
    pub fn halt(&mut self, reason: impl Into<String>) -> Result<(), OmegaError> {
        let reason: String = reason.into();
        self.halt.consume(reason.clone());
        // Mirror the reason into the snapshot for inspection.
        self.omega.halt_reason = Some(reason);
        self.omega.kill_consumed = true;
        Ok(())
    }

    /// Read the halt token (clones share state). Useful when an external
    /// caller (e.g., an OS signal handler) needs to trigger halt without
    /// holding a `&mut OmegaScheduler`.
    #[must_use]
    pub fn halt_token(&self) -> HaltToken {
        self.halt.clone()
    }

    /// Advance one tick. Returns `Err` on dependency cycle, halted state,
    /// system panic, frame-overbudget (per policy), or determinism violation.
    pub fn step(&mut self, dt: f64) -> Result<(), OmegaError> {
        // Halt first — within 1 tick.
        if let Some(reason) = self.halted_with.clone() {
            return Err(OmegaError::HaltedByKill { reason });
        }
        if self.halt.is_triggered() {
            let reason = self
                .halt
                .reason()
                .unwrap_or_else(|| "kill-switch".to_string());
            self.halted_with = Some(reason.clone());
            // Final telemetry tick on the halt frame (mirrors phase-9 +
            // phase-10 of `specs/30 § PHASES` collapsed for stage-0).
            self.telemetry.record_tick();
            self.telemetry.count("omega.halt.frames");
            if let Some(log) = self.replay_log.as_mut() {
                log.append(ReplayEntry::Marker {
                    frame: self.frame,
                    label: format!("halt: {reason}"),
                });
            }
            return Err(OmegaError::HaltedByKill { reason });
        }

        // Frame-overbudget check (deterministic — based on dt only).
        if dt > self.config.frame_budget_s {
            match self.config.overbudget_policy {
                OverbudgetPolicy::Halt => {
                    return Err(OmegaError::FrameOverbudget {
                        frame: self.frame,
                        dt_used: dt,
                        budget: self.config.frame_budget_s,
                        policy: "Halt",
                    });
                }
                OverbudgetPolicy::Degrade => {
                    self.telemetry.count("omega.overbudget");
                }
            }
        }

        // Topologically order the systems (stable form).
        let order = {
            let deps_lookup = |id: SystemId| -> Vec<SystemId> {
                self.systems
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| r.system.dependencies().to_vec())
                    .unwrap_or_default()
            };
            topo_sort_stable(&self.insertion_order, deps_lookup)
        };
        let order = match order {
            Ok(o) => o,
            Err(DepGraphError::Cycle { at }) => {
                return Err(OmegaError::DependencyCycle { system: at });
            }
            Err(DepGraphError::UnknownDependency { dependent, .. }) => {
                return Err(OmegaError::UnknownSystem { id: dependent });
            }
        };

        // Snapshot the inputs for this frame, then clear pending.
        let inputs_for_frame = std::mem::take(&mut self.pending_inputs);

        // Record inputs into the replay log BEFORE running systems —
        // this is the canonical "what happened on frame N" entry.
        if let Some(log) = self.replay_log.as_mut() {
            // Sorted iteration (BTreeMap) ⇒ deterministic order.
            for (stream, event) in &inputs_for_frame {
                log.append(ReplayEntry::Input {
                    frame: self.frame,
                    event: event.clone(),
                });
                let _ = stream; // keep var name documented
            }
        }

        // Run systems in topological order.
        for id in order {
            // Clone the (immutable) view of the registered entry's name
            // so we can dispatch step() with `&mut self.systems[i].system`
            // without holding an immutable borrow of self.systems.
            let (idx, name) = {
                let i = self
                    .systems
                    .iter()
                    .position(|r| r.id == id)
                    .ok_or(OmegaError::UnknownSystem { id })?;
                (i, self.systems[i].name.clone())
            };
            let halt_requested = self.halt.is_triggered();
            let result = {
                let registered = &mut self.systems[idx];
                let mut ctx = OmegaStepCtx::new(
                    &mut self.omega,
                    &mut self.rngs,
                    &mut self.telemetry,
                    self.frame,
                    halt_requested,
                    &inputs_for_frame,
                );
                registered.system.step(&mut ctx, dt)
            };
            if let Err(e) = result {
                return Err(OmegaError::SystemPanicked {
                    system: id,
                    name,
                    frame: self.frame,
                    msg: format!("{e}"),
                });
            }
        }

        // End-of-step bookkeeping.
        self.omega.epoch = self.omega.epoch.saturating_add(1);
        self.frame = self.frame.saturating_add(1);
        self.telemetry.record_tick();
        self.telemetry.count("omega.frames");
        Ok(())
    }

    /// Advance `n` ticks of `dt` each, short-circuiting on the first
    /// error. Returns `Ok(())` if all `n` ticks succeeded.
    pub fn step_n(&mut self, n: u32, dt: f64) -> Result<(), OmegaError> {
        for _ in 0..n {
            self.step(dt)?;
        }
        Ok(())
    }

    /// Replay-construct a fresh scheduler from a `ReplayLog`.
    ///
    /// § STAGE-0 SCOPE
    ///   The caller MUST register the same systems against this scheduler
    ///   *after* construction (in the same order, with the same names +
    ///   dep declarations + effect rows) for replay to produce bit-equal
    ///   output. Stage-0 does not serialize `dyn OmegaSystem` — that
    ///   would require trait-object registry which is a non-trivial
    ///   ABI design.
    ///
    /// § WHAT THIS FUNCTION DOES
    ///   - Constructs a scheduler with the seed from the log.
    ///   - Pre-loads the inputs from the log into a per-frame queue.
    ///   - The caller invokes `step_through_replay()` to drive the systems.
    pub fn replay_from(log: ReplayLog) -> Self {
        let mut sched = Self::with_seed(log.master_seed());
        sched.replay_inputs = build_replay_input_queue(&log);
        sched
    }

    /// Step through the loaded replay-input queue. Each call dequeues the
    /// inputs scheduled for the current frame, injects them, runs `step(dt)`
    /// once, and advances to the next frame's slot. Returns the frame number
    /// that was just processed, or `None` when the queue is exhausted.
    pub fn step_through_replay(&mut self, dt: f64) -> Result<Option<u64>, OmegaError> {
        let frame = self.frame;
        // Dequeue inputs for this frame.
        let mut have_any = false;
        if let Some(inputs) = self.replay_inputs.remove(&frame) {
            for (stream, event) in inputs {
                self.inject_input(stream, event);
                have_any = true;
            }
        }
        // If the queue is exhausted AND no inputs for this frame AND no
        // pending inputs, exit early — no more recorded work to do.
        if !have_any && self.replay_inputs.is_empty() {
            return Ok(None);
        }
        self.step(dt)?;
        Ok(Some(frame))
    }
}

/// Build a frame-keyed replay input queue from a flat `ReplayLog`.
fn build_replay_input_queue(log: &ReplayLog) -> BTreeMap<u64, Vec<(RngStreamId, InputEvent)>> {
    let mut queue: BTreeMap<u64, Vec<(RngStreamId, InputEvent)>> = BTreeMap::new();
    for entry in log.entries() {
        if let ReplayEntry::Input { frame, event } = entry {
            queue
                .entry(*frame)
                .or_default()
                // Stage-0 form : streams are not preserved per-input in the
                // log (Input is keyed by (frame, event) ; a future slice may
                // refine this). For now, default to `RngStreamId(0)` so the
                // reverse-injection path round-trips.
                .push((RngStreamId(0), event.clone()));
        }
    }
    queue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::caps_grant;
    use crate::omega_stub::{OmegaSnapshot, OmegaStubField};
    use crate::system::OmegaSystem;

    /// Counter system : on each step, increment a named integer in the
    /// snapshot's `sim` map. Useful for replay-determinism asserts.
    struct CounterSys {
        name: String,
        key: String,
    }
    impl OmegaSystem for CounterSys {
        fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
            let entry = ctx
                .omega()
                .sim
                .entry(self.key.clone())
                .or_insert(OmegaStubField::Int(0));
            if let OmegaStubField::Int(n) = entry {
                *n += 1;
            }
            Ok(())
        }
        fn name(&self) -> &str {
            // Owned String ; lifetime tied to self.
            &self.name
        }
    }

    /// System with declared dependencies + RNG streams.
    struct DepSys {
        name: String,
        deps: Vec<SystemId>,
        streams: Vec<RngStreamId>,
    }
    impl OmegaSystem for DepSys {
        fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
            // Use the first declared stream if any.
            if let Some(&s) = self.streams.first() {
                if let Some(rng) = ctx.rng(s) {
                    let _ = rng.next_u32();
                }
            }
            Ok(())
        }
        fn dependencies(&self) -> &[SystemId] {
            &self.deps
        }
        fn name(&self) -> &str {
            // Owned String ; lifetime tied to self.
            &self.name
        }
        fn rng_streams(&self) -> &[RngStreamId] {
            &self.streams
        }
    }

    /// System whose step() always errors.
    struct PanicSys;
    impl OmegaSystem for PanicSys {
        fn step(&mut self, _ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
            Err(OmegaError::DeterminismViolation {
                frame: 0,
                kind: "synthetic-test-panic",
            })
        }
        fn name(&self) -> &'static str {
            "panic"
        }
    }

    fn fresh_scheduler() -> OmegaScheduler {
        OmegaScheduler::with_seed(0xCAFE_F00D)
    }

    #[test]
    fn fresh_scheduler_runs_zero_systems() {
        let mut s = fresh_scheduler();
        assert_eq!(s.frame(), 0);
        assert!(s.step(0.001).is_ok());
        assert_eq!(s.frame(), 1);
        assert_eq!(s.telemetry().tick_count(), 1);
    }

    #[test]
    fn register_with_revoked_grant_rejected() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        g.revoke();
        let err = s
            .register(
                CounterSys {
                    name: "c".into(),
                    key: "k".into(),
                },
                &g,
            )
            .unwrap_err();
        assert!(matches!(err, OmegaError::ConsentRevoked { .. }));
    }

    #[test]
    fn register_with_wrong_grant_rejected() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaHalt);
        let err = s
            .register(
                CounterSys {
                    name: "c".into(),
                    key: "k".into(),
                },
                &g,
            )
            .unwrap_err();
        assert!(matches!(err, OmegaError::ConsentRevoked { .. }));
    }

    #[test]
    fn duplicate_name_rejected() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        s.register(
            CounterSys {
                name: "alpha".into(),
                key: "k1".into(),
            },
            &g,
        )
        .unwrap();
        let err = s
            .register(
                CounterSys {
                    name: "alpha".into(),
                    key: "k2".into(),
                },
                &g,
            )
            .unwrap_err();
        assert!(matches!(err, OmegaError::DuplicateName { .. }));
    }

    #[test]
    fn register_assigns_monotone_ids() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        let a = s
            .register(
                CounterSys {
                    name: "a".into(),
                    key: "k".into(),
                },
                &g,
            )
            .unwrap();
        let b = s
            .register(
                CounterSys {
                    name: "b".into(),
                    key: "k".into(),
                },
                &g,
            )
            .unwrap();
        assert_eq!(a, SystemId(0));
        assert_eq!(b, SystemId(1));
    }

    #[test]
    fn step_runs_registered_system() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        s.register(
            CounterSys {
                name: "tick".into(),
                key: "n".into(),
            },
            &g,
        )
        .unwrap();
        s.step(0.001).unwrap();
        s.step(0.001).unwrap();
        s.step(0.001).unwrap();
        assert_eq!(s.omega().sim.get("n"), Some(&OmegaStubField::Int(3)));
    }

    #[test]
    fn unknown_dep_surfaces_unknown_system() {
        // A system declaring a dep on a SystemId that was never registered
        // surfaces `OmegaError::UnknownSystem` at the first `step()`.
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        s.register(
            DepSys {
                name: "x".into(),
                deps: vec![SystemId(99)],
                streams: vec![],
            },
            &g,
        )
        .unwrap();
        let err = s.step(0.001).unwrap_err();
        assert!(matches!(err, OmegaError::UnknownSystem { .. }));
    }

    #[test]
    fn real_dependency_cycle_detected() {
        // Construct an actual cycle : two systems whose `dependencies()` lists
        // point at each other. We rely on the monotone-id invariant of
        // `OmegaScheduler::register()` — first system gets `SystemId(0)`,
        // second gets `SystemId(1)`, etc. — so we can pre-bake the dep
        // vectors before registration to form a direct A↔B cycle.
        struct PreBakedCycle {
            name: String,
            deps: Vec<SystemId>,
        }
        impl OmegaSystem for PreBakedCycle {
            fn step(&mut self, _ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
                Ok(())
            }
            fn dependencies(&self) -> &[SystemId] {
                &self.deps
            }
            fn name(&self) -> &str {
                // Owned String ; lifetime tied to self.
                &self.name
            }
        }
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        // System A gets id 0 ; system B gets id 1. A depends on B,
        // B depends on A — direct cycle.
        s.register(
            PreBakedCycle {
                name: "A".into(),
                deps: vec![SystemId(1)],
            },
            &g,
        )
        .unwrap();
        s.register(
            PreBakedCycle {
                name: "B".into(),
                deps: vec![SystemId(0)],
            },
            &g,
        )
        .unwrap();
        let err = s.step(0.001).unwrap_err();
        assert!(matches!(err, OmegaError::DependencyCycle { .. }));
    }

    #[test]
    fn system_panic_propagates_as_error() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        s.register(PanicSys, &g).unwrap();
        let err = s.step(0.001).unwrap_err();
        assert!(matches!(err, OmegaError::SystemPanicked { .. }));
    }

    #[test]
    fn halt_returns_error_on_next_step() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        s.register(
            CounterSys {
                name: "c".into(),
                key: "n".into(),
            },
            &g,
        )
        .unwrap();
        s.step(0.001).unwrap();
        s.halt("user-stop").unwrap();
        let err = s.step(0.001).unwrap_err();
        assert!(matches!(err, OmegaError::HaltedByKill { .. }));
        // Subsequent steps continue to return the same error.
        let err2 = s.step(0.001).unwrap_err();
        assert!(matches!(err2, OmegaError::HaltedByKill { .. }));
        assert_eq!(s.halt_state(), HaltState::Halted);
    }

    #[test]
    fn halt_token_external_trigger_works() {
        let mut s = fresh_scheduler();
        let token = s.halt_token();
        // External caller (e.g., signal handler) triggers the token.
        token.consume("sigint");
        let err = s.step(0.001).unwrap_err();
        assert!(matches!(err, OmegaError::HaltedByKill { .. }));
    }

    #[test]
    fn frame_overbudget_halt_policy() {
        let cfg = SchedulerConfig {
            master_seed: 0,
            frame_budget_s: 0.001,
            overbudget_policy: OverbudgetPolicy::Halt,
            record_replay: false,
        };
        let mut s = OmegaScheduler::new(cfg);
        let err = s.step(0.005).unwrap_err();
        assert!(matches!(err, OmegaError::FrameOverbudget { .. }));
    }

    #[test]
    fn frame_overbudget_degrade_policy_continues() {
        let cfg = SchedulerConfig {
            master_seed: 0,
            frame_budget_s: 0.001,
            overbudget_policy: OverbudgetPolicy::Degrade,
            record_replay: false,
        };
        let mut s = OmegaScheduler::new(cfg);
        s.step(0.005).unwrap();
        // Telemetry counter should have been bumped.
        assert!(s.telemetry().read_counter("omega.overbudget") >= 1);
    }

    #[test]
    fn replay_determinism_two_runs_bit_equal() {
        // Two scheduler instances with identical seed + identical inputs
        // produce bit-equal Ω-tensor outputs.
        fn build_and_run() -> OmegaSnapshot {
            let mut s = OmegaScheduler::with_seed(123);
            let g = caps_grant(OmegaCapability::OmegaRegister);
            s.register(
                CounterSys {
                    name: "alpha".into(),
                    key: "alpha".into(),
                },
                &g,
            )
            .unwrap();
            s.register(
                DepSys {
                    name: "beta".into(),
                    deps: vec![],
                    streams: vec![RngStreamId(0)],
                },
                &g,
            )
            .unwrap();
            for _ in 0..50 {
                s.step(0.001).unwrap();
            }
            s.omega().clone()
        }
        let a = build_and_run();
        let b = build_and_run();
        assert!(a.bit_eq(&b), "two runs MUST be bit-equal");
    }

    /// System that mutates omega based on RNG output ; used to verify
    /// that different seeds produce different state. Hoisted to module
    /// scope (vs an inner item-after-statements) for clippy hygiene.
    struct RngWriter {
        streams: Vec<RngStreamId>,
    }
    impl OmegaSystem for RngWriter {
        fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
            let v = ctx.rng(self.streams[0]).unwrap().next_u64();
            // Bit-cast u64 → i64 (preserves bit pattern, suppresses
            // possible-wrap lint via #[allow]). The Int field stores
            // arbitrary 64-bit data ; sign interpretation is irrelevant.
            #[allow(
                clippy::cast_possible_wrap,
                reason = "intentional bit-cast for storage"
            )]
            let bits = v as i64;
            ctx.omega()
                .sim
                .insert("rng".into(), OmegaStubField::Int(bits));
            Ok(())
        }
        fn name(&self) -> &'static str {
            "rng-writer"
        }
        fn rng_streams(&self) -> &[RngStreamId] {
            &self.streams
        }
    }

    #[test]
    fn replay_determinism_different_seeds_diverge() {
        fn build_and_run(seed: u64) -> OmegaSnapshot {
            let mut s = OmegaScheduler::with_seed(seed);
            let g = caps_grant(OmegaCapability::OmegaRegister);
            s.register(
                RngWriter {
                    streams: vec![RngStreamId(0)],
                },
                &g,
            )
            .unwrap();
            s.step(0.001).unwrap();
            s.omega().clone()
        }
        let a = build_and_run(1);
        let b = build_and_run(2);
        assert!(!a.bit_eq(&b), "different seeds MUST diverge");
    }

    #[test]
    fn replay_log_records_inputs() {
        let cfg = SchedulerConfig {
            master_seed: 7,
            frame_budget_s: 1.0,
            overbudget_policy: OverbudgetPolicy::Degrade,
            record_replay: true,
        };
        let mut s = OmegaScheduler::new(cfg);
        let g = caps_grant(OmegaCapability::OmegaRegister);
        s.register(
            CounterSys {
                name: "c".into(),
                key: "k".into(),
            },
            &g,
        )
        .unwrap();
        s.inject_input(RngStreamId(0), InputEvent::KeyPress { keycode: 65 });
        s.step(0.001).unwrap();
        let log = s.replay_log().expect("log enabled");
        assert!(log
            .entries()
            .iter()
            .any(|e| matches!(e, ReplayEntry::Input { .. })));
    }

    #[test]
    fn step_n_short_circuits_on_first_error() {
        let mut s = fresh_scheduler();
        let g = caps_grant(OmegaCapability::OmegaRegister);
        s.register(PanicSys, &g).unwrap();
        let err = s.step_n(10, 0.001).unwrap_err();
        assert!(matches!(err, OmegaError::SystemPanicked { .. }));
        // Frame counter should be 0 — we never completed a step.
        assert_eq!(s.frame(), 0);
    }

    #[test]
    fn determinism_mode_recorded_at_construction() {
        let s = fresh_scheduler();
        let m = s.determinism_mode();
        assert!(matches!(m, DeterminismMode::Strict | DeterminismMode::Soft));
    }
}
