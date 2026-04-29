//! § Loop systems — `OmegaSystem` impls for the canonical 13 omega_step phases.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/30_SUBSTRATE.csl § OMEGA-STEP § PHASES`.
//!
//! § THESIS
//!
//!   Per spec § PHASES, one omega_step runs through 13 canonical phases :
//!     1. consent-check
//!     2. net-recv
//!     3. input-sample
//!     4. sim-substep ×N
//!     5. projections-rebuild
//!     6. audio-callback-feed
//!     7. render-graph-record
//!     8. render-submit
//!     9. telemetry-flush
//!    10. audit-append
//!    11. net-send
//!    12. save-journal-append
//!    13. freeze-and-return
//!
//!   Phases 6 (audio) is run on a dedicated audio-callback fiber per
//!   `specs/30 § PHASES § P6`, which is structurally a separate
//!   `OmegaScheduler` consideration ; the `cssl-substrate-omega-step`
//!   scheduler enforces this via `EffectRow::sim_audio()` + the
//!   `DeterminismMode::Strict` requirement (see `cssl-substrate-omega-step`
//!   `register()` source). At scaffold-time, [`AudioFeedSystem`] is
//!   intentionally registered with a `{Sim}`-only effect row so the smoke
//!   test passes on every host's determinism-mode without depending on
//!   FTZ/DAZ probe outcomes — the real Audio system lands when Apocky's
//!   audio-content slice does.
//!
//! § ENCODING
//!
//!   Each phase is one impl of `cssl_substrate_omega_step::OmegaSystem`.
//!   The impls have empty bodies (returning `Ok(())`) at scaffold-time —
//!   the SHAPE matters, not the gameplay-content. Future Apocky-fill slices
//!   replace each system's `step()` body with real logic per the spec.
//!
//!   The systems' canonical effect-rows are :
//!     ConsentCheckSystem    : {Sim}
//!     NetRecvSystem         : {Sim, Replay}    (per spec — Net-without-Replay rejected)
//!     InputSystem           : {Sim}
//!     SimSystem             : {Sim}
//!     ProjectionsSystem     : {Sim}
//!     AudioFeedSystem       : {Sim}            (see § AUDIO-DEFERRAL above)
//!     RenderGraphSystem     : {Sim}            (`{Sim, Render}` deferred)
//!     RenderSubmitSystem    : {Sim}
//!     TelemetryFlushSystem  : {Sim}
//!     AuditAppendSystem     : {Sim}
//!     NetSendSystem         : {Sim, Replay}
//!     SaveJournalSystem     : {Sim, Save}
//!     FreezeSystem          : {Sim}
//!
//!   `{Render}` is preserved as the canonical effect for SimSystem when
//!   game-content lands ; at scaffold-time `{Sim}`-only is used because
//!   the `{Render}` effect would imply GPU resource binding that the
//!   scaffold doesn't drive.
//!
//! § PRIME-DIRECTIVE alignment
//!
//!   - [`ConsentCheckSystem`] runs FIRST in the topological order. If any
//!     consent-token is revoked, downstream systems observe via
//!     `OmegaStepCtx`'s telemetry hook + the system itself bumps the
//!     `omega.consent.revoked` counter.
//!   - [`AuditAppendSystem`] is mandatory per spec § PHASES § P10 —
//!     audit-append failure is process-abort. At scaffold-time the system
//!     records a frame-counter increment ; future audit-chain integration
//!     wires `cssl-telemetry::AuditChain` here.
//!   - [`SaveJournalSystem`] runs only if a `SavePath` cap-token has been
//!     issued. The scaffold's `Engine` checks this at registration time.

use cssl_substrate_omega_step::{
    EffectRow, OmegaError, OmegaStepCtx, OmegaSystem, RngStreamId, SubstrateEffect, SystemId,
};

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 1 — CONSENT-CHECK
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-1 consent-check system. `specs/30 § PHASES § phase-1`.
///
/// Per spec § PHASE-INVARIANTS § P1 :
///   "revoked-consent ⇒ corresponding-effects skip silently + emit
///    Audit<\"consent-revoked\">"
///
/// At scaffold-time the system bumps a telemetry counter so the smoke test
/// can verify the phase ran ; future Apocky-fill walks the OmegaConsent
/// snapshot + emits the per-token audit-entries.
#[derive(Debug, Default)]
pub struct ConsentCheckSystem;

impl ConsentCheckSystem {
    pub const NAME: &'static str = "loa.phase-01.consent-check";
}

impl OmegaSystem for ConsentCheckSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-01.consent-check");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 2 — NET-RECV
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-2 net-recv system. `specs/30 § PHASES § phase-2`. Per spec the
/// effect-row carries `{Net}` but `{Net}` requires `{Replay}` to be
/// determinism-safe (per `cssl-substrate-omega-step::EffectRow::validate`).
/// At scaffold-time we declare `{Sim, Replay}` so the row is well-formed
/// even though no actual Net traffic happens.
#[derive(Debug, Default)]
pub struct NetRecvSystem;

impl NetRecvSystem {
    pub const NAME: &'static str = "loa.phase-02.net-recv";
}

impl OmegaSystem for NetRecvSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-02.net-recv");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn effect_row(&self) -> EffectRow {
        EffectRow::from_slice(&[SubstrateEffect::Sim, SubstrateEffect::Replay])
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 3 — INPUT-SAMPLE
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-3 input-sample system. `specs/30 § PHASES § phase-3`.
///
/// Per `specs/30 § PHASE-INVARIANTS § P3` : "input read via {IO} ;
/// mapped-to InputState : val". At scaffold-time the system reads from
/// the scheduler's per-frame input queue (populated by the Engine via
/// `OmegaScheduler::inject_input`) and bumps a telemetry counter.
#[derive(Debug, Default)]
pub struct InputSystem;

impl InputSystem {
    pub const NAME: &'static str = "loa.phase-03.input-sample";
    pub const STREAM: RngStreamId = RngStreamId(0);
}

impl OmegaSystem for InputSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        // Read whatever input was injected for this frame (may be None).
        let _input = ctx.input(Self::STREAM);
        ctx.telemetry().count("loa.phase-03.input-sample");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn rng_streams(&self) -> &[RngStreamId] {
        &[Self::STREAM]
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 4 — SIM-SUBSTEP
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-4 sim-substep system. `specs/30 § PHASES § phase-4`.
///
/// Per spec § P4-invariant : "∀ sim-substep is {PureDet} bit-exact ;
/// SIM_SUBSTEPS is comptime-known ⇒ stage-specialized."
///
/// At scaffold-time the system declares its dependency on the InputSystem
/// (so input-state is available before sim runs) and bumps a counter.
/// Future Apocky-fill inserts the actual sim logic.
#[derive(Debug)]
pub struct SimSystem {
    /// SystemId of the InputSystem this depends on. Set by the Engine at
    /// registration time.
    deps: Vec<SystemId>,
}

impl Default for SimSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl SimSystem {
    pub const NAME: &'static str = "loa.phase-04.sim-substep";

    #[must_use]
    pub fn new() -> Self {
        Self { deps: Vec::new() }
    }

    /// Set the dependency on the InputSystem's `SystemId`. Called by the
    /// Engine after both systems are registered.
    pub fn depend_on(&mut self, input_system_id: SystemId) {
        self.deps = vec![input_system_id];
    }
}

impl OmegaSystem for SimSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-04.sim-substep");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn dependencies(&self) -> &[SystemId] {
        &self.deps
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 5 — PROJECTIONS-REBUILD
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-5 projections-rebuild system. `specs/30 § PHASES § phase-5`.
///
/// Per `specs/30 § PROJECTIONS § THESIS` :
///   "a Projection is an observer-frame onto Ω-tensor. multiple projections
///    coexist : main-camera, mini-map, ai-companion-view, debug-introspection,
///    replay-rewind-view."
///
/// At scaffold-time the system bumps a counter ; the actual rebuild call
/// uses `cssl-substrate-projections::Camera::view_matrix()` etc. via the
/// Engine's projection registry.
#[derive(Debug, Default)]
pub struct ProjectionsSystem;

impl ProjectionsSystem {
    pub const NAME: &'static str = "loa.phase-05.projections-rebuild";
}

impl OmegaSystem for ProjectionsSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-05.projections-rebuild");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 6 — AUDIO-CALLBACK-FEED  (deferred ; runs as {Sim} for stage-0)
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-6 audio-callback-feed system. `specs/30 § PHASES § phase-6`.
///
/// SCAFFOLD NOTE : the canonical effect-row would be `{Sim, Audio}` but
/// `cssl-substrate-omega-step::OmegaScheduler::register` rejects Audio
/// systems unless `DeterminismMode::Strict` (per its source). At scaffold-
/// time we use `{Sim}` only so the smoke test runs on every host. Real
/// Apocky-fill audio-content lands the `{Audio}` row when it's ready.
#[derive(Debug, Default)]
pub struct AudioFeedSystem;

impl AudioFeedSystem {
    pub const NAME: &'static str = "loa.phase-06.audio-callback-feed";
}

impl OmegaSystem for AudioFeedSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-06.audio-callback-feed");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 7 — RENDER-GRAPH-RECORD
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-7 render-graph-record system. `specs/30 § PHASES § phase-7`.
///
/// Per spec § P7-invariant : "RenderGraph DAG built ; cycles =
/// compile-error." At scaffold-time the system bumps a counter ; real
/// graph-recording lands when Apocky's render-content slice does.
#[derive(Debug, Default)]
pub struct RenderGraphSystem;

impl RenderGraphSystem {
    pub const NAME: &'static str = "loa.phase-07.render-graph-record";
}

impl OmegaSystem for RenderGraphSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-07.render-graph-record");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 8 — RENDER-SUBMIT
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-8 render-submit system. `specs/30 § PHASES § phase-8`.
///
/// Per spec § P8-invariant : "∀ CmdBuf consumed via end_frame ;
/// linear-leak = compile-error (V7-V12 class)." At scaffold-time the
/// system bumps a counter ; real CmdBuf-submission lands when host-d3d12 /
/// host-vulkan are wired into the engine.
#[derive(Debug, Default)]
pub struct RenderSubmitSystem;

impl RenderSubmitSystem {
    pub const NAME: &'static str = "loa.phase-08.render-submit";
}

impl OmegaSystem for RenderSubmitSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-08.render-submit");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 9 — TELEMETRY-FLUSH
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-9 telemetry-flush system. `specs/30 § PHASES § phase-9`.
///
/// Per spec § P9-invariant : "TelemetryRing drained non-blocking." At
/// scaffold-time the system reads the current tick-count from the
/// telemetry hook and increments a flush counter.
#[derive(Debug, Default)]
pub struct TelemetryFlushSystem;

impl TelemetryFlushSystem {
    pub const NAME: &'static str = "loa.phase-09.telemetry-flush";
}

impl OmegaSystem for TelemetryFlushSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-09.telemetry-flush");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 10 — AUDIT-APPEND  ‼ load-bearing
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-10 audit-append system. `specs/30 § PHASES § phase-10`.
///
/// Per spec § P10-invariant : "audit-append failure ⇒ panic
/// (per §§ 22 PRIME-DIRECTIVE-ENFORCEMENT)."
///
/// This is the load-bearing PRIME-DIRECTIVE phase — every consent-revocation,
/// every kill-switch consumption, every projection-record, every save-
/// checkpoint must audit. The scaffold encodes the SHAPE — bumping a
/// telemetry counter tagged `loa.phase-10.audit-append` — which the smoke
/// test verifies. Real `cssl-telemetry::AuditChain` integration lands when
/// the audit-chain is wired through `OmegaSnapshot` (DEFERRED to a future
/// Substrate slice).
#[derive(Debug, Default)]
pub struct AuditAppendSystem;

impl AuditAppendSystem {
    pub const NAME: &'static str = "loa.phase-10.audit-append";
}

impl OmegaSystem for AuditAppendSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        // Per spec § P10 — this MUST run every tick. Failure here is
        // process-abort territory ; at scaffold-time we cannot fail.
        ctx.telemetry().count("loa.phase-10.audit-append");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 11 — NET-SEND
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-11 net-send system. `specs/30 § PHASES § phase-11`.
///
/// Mirrors NetRecvSystem's `{Sim, Replay}` effect-row.
#[derive(Debug, Default)]
pub struct NetSendSystem;

impl NetSendSystem {
    pub const NAME: &'static str = "loa.phase-11.net-send";
}

impl OmegaSystem for NetSendSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-11.net-send");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn effect_row(&self) -> EffectRow {
        EffectRow::from_slice(&[SubstrateEffect::Sim, SubstrateEffect::Replay])
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 12 — SAVE-JOURNAL-APPEND
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-12 save-journal-append system. `specs/30 § PHASES § phase-12`.
///
/// Per spec § P12-invariant : "SaveJournal append is incremental ;
/// full-snapshot only at scheduled-checkpoint."
///
/// Effect-row : `{Sim, Save}` per spec § EFFECT-ROWS § Save.
#[derive(Debug, Default)]
pub struct SaveJournalSystem;

impl SaveJournalSystem {
    pub const NAME: &'static str = "loa.phase-12.save-journal-append";
}

impl OmegaSystem for SaveJournalSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-12.save-journal-append");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn effect_row(&self) -> EffectRow {
        EffectRow::sim_save()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE 13 — FREEZE-AND-RETURN
// ═══════════════════════════════════════════════════════════════════════════

/// Phase-13 freeze-and-return system. `specs/30 § PHASES § phase-13`.
///
/// Per spec : "trn → val, return Omega : iso." At scaffold-time the system
/// is registered LAST in topological order (depending on every other
/// system) so it runs after all other phases of this tick.
#[derive(Debug)]
pub struct FreezeSystem {
    /// SystemIds of every other phase-system. Set by the Engine.
    deps: Vec<SystemId>,
}

impl Default for FreezeSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FreezeSystem {
    pub const NAME: &'static str = "loa.phase-13.freeze-and-return";

    #[must_use]
    pub fn new() -> Self {
        Self { deps: Vec::new() }
    }

    /// Set the dependency vector. Called by the Engine.
    pub fn depend_on(&mut self, deps: Vec<SystemId>) {
        self.deps = deps;
    }
}

impl OmegaSystem for FreezeSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        ctx.telemetry().count("loa.phase-13.freeze-and-return");
        Ok(())
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn dependencies(&self) -> &[SystemId] {
        &self.deps
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_system_has_distinct_canonical_name() {
        let names: Vec<&'static str> = vec![
            ConsentCheckSystem::NAME,
            NetRecvSystem::NAME,
            InputSystem::NAME,
            SimSystem::NAME,
            ProjectionsSystem::NAME,
            AudioFeedSystem::NAME,
            RenderGraphSystem::NAME,
            RenderSubmitSystem::NAME,
            TelemetryFlushSystem::NAME,
            AuditAppendSystem::NAME,
            NetSendSystem::NAME,
            SaveJournalSystem::NAME,
            FreezeSystem::NAME,
        ];
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let original_len = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), original_len, "all 13 names must be unique");
        assert_eq!(names.len(), 13, "must declare all 13 phases");
    }

    #[test]
    fn names_are_phase_ordered_alphabetically() {
        // Lexicographic order on phase prefix `loa.phase-0X.…` gives the
        // canonical 13-phase order. This is a load-bearing aesthetic — the
        // smoke test relies on it for readable telemetry-counter dumps.
        assert!(ConsentCheckSystem::NAME < NetRecvSystem::NAME);
        assert!(NetRecvSystem::NAME < InputSystem::NAME);
        assert!(InputSystem::NAME < SimSystem::NAME);
        assert!(AuditAppendSystem::NAME < NetSendSystem::NAME);
        assert!(SaveJournalSystem::NAME < FreezeSystem::NAME);
    }

    #[test]
    fn save_system_carries_save_effect() {
        let s = SaveJournalSystem;
        assert!(s.effect_row().contains(SubstrateEffect::Save));
        assert!(s.effect_row().contains(SubstrateEffect::Sim));
    }

    #[test]
    fn net_systems_carry_replay_effect() {
        let r = NetRecvSystem;
        let s = NetSendSystem;
        assert!(r.effect_row().contains(SubstrateEffect::Replay));
        assert!(s.effect_row().contains(SubstrateEffect::Replay));
    }
}
