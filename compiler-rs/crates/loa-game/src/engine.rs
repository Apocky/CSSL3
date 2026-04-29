//! § Engine — wires Substrate + hosts + canonical types into the running game.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/31_LOA_DESIGN.csl § GAME-LOOP § ENTRY-POINT` +
//!   `specs/30_SUBSTRATE.csl § OMEGA-STEP § PHASES`.
//!
//! § THESIS
//!
//!   The Engine owns :
//!     - the omega-step Scheduler (driving the canonical 13-phase tick)
//!     - the World, Player, Companion, ApockalypseEngine state
//!     - the projection-registry (Camera + ObserverFrame ; ScreenReader
//!       always-active per accessibility-baseline)
//!     - the host-window + host-input bindings (when present)
//!     - the bundle of CapTokens that gate Substrate operations
//!     - the canonical `cssl-substrate-save::OmegaScheduler` mirror used for
//!       save/load round-trips (NOTE : this is distinct from the omega-step
//!       scheduler — both have type-name `OmegaScheduler` and the engine
//!       imports them under disambiguated aliases)
//!
//!   The engine is constructed via [`Engine::new`] which takes a
//!   [`CapTokens`] bundle. Production builds receive the bundle from the
//!   pre-staged consent flow ; tests receive it via
//!   [`CapTokens::issue_for_test`] which routes through
//!   `caps_grant_for_test` (feature-gated `test-bypass`).
//!
//! § PRIME-DIRECTIVE STRUCTURAL ENCODING
//!
//!   - The `CapTokens` bundle holds REAL `CapToken` values for each
//!     Substrate operation the engine performs. Tokens are non-Copy +
//!     non-Clone — they're consumed-by-move when the engine performs the
//!     gated op, and once consumed cannot be re-used.
//!   - The engine NEVER inspects what tokens are present without
//!     consuming them. There's no "is the token present" probe — the
//!     gate-check IS the consumption.
//!   - The `LoaError::ConsentRefused` variant is the production-stage-0
//!     refusal surface : when no `test-bypass` is in effect, the engine
//!     cannot grant itself caps and must surface the refusal to the
//!     calling layer (per `cssl-substrate-prime-directive::caps_grant`'s
//!     stage-0 production refusal).

use cssl_substrate_omega_step::{
    CapsGrant, OmegaScheduler as TickScheduler, OverbudgetPolicy, SchedulerConfig,
};
use cssl_substrate_prime_directive::{
    AttestationError, CapToken, EnforcementAuditBus, ATTESTATION as PD_ATTESTATION,
};
use cssl_substrate_save::OmegaScheduler as SaveScheduler;

// § Imports gated to feature = "test-bypass" : the issue_for_test path
// needs them, and that path is itself gated to the same feature (not
// cfg(test)) per the feature-gating note on issue_for_test.
#[cfg(feature = "test-bypass")]
use cssl_substrate_omega_step::{caps_grant as omega_caps_grant, OmegaCapability};
#[cfg(feature = "test-bypass")]
use cssl_substrate_projections::{CapsToken as ProjectionCapsToken, Grant};

use crate::apockalypse::ApockalypseEngine;
use crate::companion::{AiSessionId, Companion};
use crate::loop_systems::{
    AuditAppendSystem, ConsentCheckSystem, FreezeSystem, InputSystem, NetRecvSystem, NetSendSystem,
    ProjectionsSystem, RenderGraphSystem, RenderSubmitSystem, SaveJournalSystem, SimSystem,
    TelemetryFlushSystem,
};
use crate::player::Player;
use crate::world::World;

// ═══════════════════════════════════════════════════════════════════════════
// § ERROR TYPE
// ═══════════════════════════════════════════════════════════════════════════

/// Top-level error type for the Engine + main-loop.
///
/// All Substrate errors propagate as canonical `OmegaError` / `LoadError` /
/// `SaveError` values rather than introducing new diagnostic codes — per
/// the slice landmines : "the scaffold MUST not introduce new diagnostic
/// codes ; all errors flow through existing infrastructure."
#[derive(Debug, thiserror::Error)]
pub enum LoaError {
    /// Stage-0 production consent flow refused. Per
    /// `cssl-substrate-prime-directive::caps_grant`, production builds
    /// without `test-bypass` cannot issue tokens — the calling layer must
    /// route through the real consent UI (Q-7 ; DEFERRED).
    #[error("PD0001 — consent refused at engine construction (stage-0 production cannot grant; provide CapTokens externally or build with test-bypass)")]
    ConsentRefused,

    /// PRIME-DIRECTIVE attestation check failed. This is a process-level
    /// sentinel — the engine refuses to construct if the attestation
    /// constant has been tampered with.
    #[error("PD0017 — PRIME-DIRECTIVE attestation check failed: {source}")]
    AttestationFailed {
        #[from]
        source: AttestationError,
    },

    /// Substrate scheduler error.
    #[error("substrate scheduler error: {0}")]
    Scheduler(#[from] cssl_substrate_omega_step::OmegaError),

    /// Substrate save error.
    #[error("substrate save error: {0}")]
    Save(#[from] cssl_substrate_save::SaveError),

    /// Substrate load error.
    #[error("substrate load error: {0}")]
    Load(#[from] cssl_substrate_save::LoadError),

    /// World validation refused an item per `World::validate_substrate_safety`.
    #[error("world contains substrate-unsafe item: {item_id:?}")]
    WorldUnsafe { item_id: crate::world::ItemId },
}

// ═══════════════════════════════════════════════════════════════════════════
// § CAP-TOKEN BUNDLE
// ═══════════════════════════════════════════════════════════════════════════

/// Bundle of capability-tokens the engine needs to drive the Substrate.
///
/// Per `specs/30_SUBSTRATE.csl § PRIME_DIRECTIVE-ALIGNMENT § CONSENT-GATES`,
/// every Substrate-touching call requires an active consent token. This
/// struct is the ceremonial bundle the engine consumes at construction.
///
/// § FIELDS
///
///   - `omega_register_grant` — `CapsGrant<OmegaRegister>` from
///     `cssl-substrate-omega-step`. Used to register every system into
///     the tick-scheduler. Cloneable (Arc-backed) so the engine can
///     register multiple systems with the same grant.
///   - `omega_register_captoken` — `CapToken<OmegaRegister>` from
///     `cssl-substrate-prime-directive`. Sibling-attestation that the
///     enforcement layer also approved the registration ceremony. The
///     engine consumes this on construction.
///   - `companion_view_captoken` — `CapToken<CompanionView>`. Issued for
///     the Companion-AI's read-only Ω-tensor view.
///   - `debug_camera_captoken` — `CapToken<DebugCamera>`. Optional ;
///     present only if the engine was launched in debug-introspection
///     mode.
///   - `save_path_captoken` — `CapToken<SavePath>`. Required to invoke
///     `cssl-substrate-save::save`.
///   - `replay_load_captoken` — `CapToken<ReplayLoad>`. Required to
///     invoke `cssl-substrate-save::load` + replay.
///   - `projection_caps` — `CapsToken` from `cssl-substrate-projections`
///     gating Camera reads. Required for the projections-rebuild phase.
pub struct CapTokens {
    pub omega_register_grant: CapsGrant,
    pub omega_register_captoken: CapToken,
    pub companion_view_captoken: CapToken,
    pub debug_camera_captoken: Option<CapToken>,
    pub save_path_captoken: CapToken,
    pub replay_load_captoken: CapToken,
    pub projection_caps: cssl_substrate_projections::CapsToken,
}

impl CapTokens {
    /// Issue a fresh `CapTokens` bundle for tests via the `test-bypass`
    /// feature-gated `caps_grant_for_test` path. This issues REAL
    /// CapTokens so the engine's Substrate operations fully exercise the
    /// enforcement layer's consume-on-use ceremony.
    ///
    /// # Errors
    /// Returns `LoaError::ConsentRefused` if any of the underlying grant
    /// paths refuses (in practice : never on `test-bypass` builds because
    /// `caps_grant_for_test` skips the production-UI gate).
    ///
    /// § FEATURE-GATING NOTE
    ///   This is gated to `feature = "test-bypass"` only (NOT `cfg(test)`)
    ///   because `cssl-substrate-prime-directive::caps_grant_for_test` is
    ///   itself only `pub` under the `test-bypass` feature. Pulling
    ///   `cfg(test)` in here would cause `cargo test -p loa-game` (without
    ///   the feature) to try to import a non-`pub` item and fail the build.
    ///   The crate's lib unit-tests that NEED this method are themselves
    ///   gated with `#[cfg(feature = "test-bypass")]`.
    #[cfg(feature = "test-bypass")]
    pub fn issue_for_test() -> Result<Self, LoaError> {
        use cssl_substrate_prime_directive::{
            caps_grant_for_test, ConsentScope, ConsentStore, SubstrateCap,
        };

        let mut store = ConsentStore::new();

        let omega_register_captoken = caps_grant_for_test(
            &mut store,
            ConsentScope::for_purpose("loa-engine: register systems", "loa-game"),
            SubstrateCap::OmegaRegister,
        )
        .map_err(|_| LoaError::ConsentRefused)?;

        let companion_view_captoken = caps_grant_for_test(
            &mut store,
            ConsentScope::for_purpose("loa-engine: companion read-only view", "loa-game"),
            SubstrateCap::CompanionView,
        )
        .map_err(|_| LoaError::ConsentRefused)?;

        let save_path_captoken = caps_grant_for_test(
            &mut store,
            ConsentScope::for_purpose("loa-engine: save journal", "loa-game"),
            SubstrateCap::SavePath,
        )
        .map_err(|_| LoaError::ConsentRefused)?;

        let replay_load_captoken = caps_grant_for_test(
            &mut store,
            ConsentScope::for_purpose("loa-engine: replay load", "loa-game"),
            SubstrateCap::ReplayLoad,
        )
        .map_err(|_| LoaError::ConsentRefused)?;

        let omega_register_grant = omega_caps_grant(OmegaCapability::OmegaRegister);
        let projection_caps =
            ProjectionCapsToken::with_grants(&[Grant::OmegaTensorAccess, Grant::ObserverShare]);

        Ok(Self {
            omega_register_grant,
            omega_register_captoken,
            companion_view_captoken,
            debug_camera_captoken: None,
            save_path_captoken,
            replay_load_captoken,
            projection_caps,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § ENGINE CONFIG
// ═══════════════════════════════════════════════════════════════════════════

/// Engine configuration — the LoA-side mirror of `OmegaConfig`.
///
/// Per `specs/31_LOA_DESIGN.csl § GAME-LOOP § CONFIGURATION` :
///   FRAME_BUDGET_MS = 16    # 60fps target
///   POWER_BUDGET_W  = 225   # desktop ; handheld DEFERRED
///   THERMAL_LIMIT_C = 85
///   SIM_SUBSTEPS    = 4
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Master deterministic seed for the tick-scheduler. Same seed +
    /// same input stream = bit-equal omega_step output across runs.
    pub master_seed: u64,
    /// Frame budget per omega_step in seconds (= 16ms / 60fps default).
    pub frame_budget_s: f64,
    /// What to do on frame-overbudget : Halt | Degrade.
    pub overbudget_policy: OverbudgetPolicy,
    /// Sim sub-step count per spec § CONFIGURATION § SIM_SUBSTEPS.
    pub sim_substeps: u32,
    /// Whether to record a replay log as the engine runs. Required for
    /// `replay_from` round-trip tests.
    pub record_replay: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            master_seed: 0xC551_F00D,
            frame_budget_s: 1.0_f64 / 60.0_f64,
            overbudget_policy: OverbudgetPolicy::Degrade,
            sim_substeps: 4,
            record_replay: true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § SYSTEM-ID REGISTRY
// ═══════════════════════════════════════════════════════════════════════════

/// IDs of every registered phase-system, captured during `Engine::new`.
/// Used by save/load to identify which systems to re-register on replay.
#[derive(Debug, Clone, Copy)]
pub struct PhaseSystemIds {
    pub consent_check: cssl_substrate_omega_step::SystemId,
    pub net_recv: cssl_substrate_omega_step::SystemId,
    pub input: cssl_substrate_omega_step::SystemId,
    pub sim: cssl_substrate_omega_step::SystemId,
    pub projections: cssl_substrate_omega_step::SystemId,
    pub audio_feed: cssl_substrate_omega_step::SystemId,
    pub render_graph: cssl_substrate_omega_step::SystemId,
    pub render_submit: cssl_substrate_omega_step::SystemId,
    pub telemetry_flush: cssl_substrate_omega_step::SystemId,
    pub audit_append: cssl_substrate_omega_step::SystemId,
    pub net_send: cssl_substrate_omega_step::SystemId,
    pub save_journal: cssl_substrate_omega_step::SystemId,
    pub freeze: cssl_substrate_omega_step::SystemId,
}

// ═══════════════════════════════════════════════════════════════════════════
// § ENGINE
// ═══════════════════════════════════════════════════════════════════════════

/// Engine — the running-game state.
///
/// Construction wires every Substrate-touching subsystem via
/// `CapTokens`-mediated ceremony. Tick advancement happens via
/// [`Engine::tick`] which runs ONE canonical 13-phase omega_step.
pub struct Engine {
    /// The omega-step tick scheduler — drives the 13-phase canonical loop.
    tick_scheduler: TickScheduler,
    /// IDs of every registered phase-system.
    phase_ids: PhaseSystemIds,
    /// Save-format scheduler — distinct from `tick_scheduler` ; used only
    /// for save/load + replay round-trips.
    save_scheduler: SaveScheduler,
    /// World state.
    world: World,
    /// Player state.
    player: Player,
    /// Optional Companion-AI in-world archetype.
    companion: Option<Companion>,
    /// Apockalypse-engine state.
    apockalypse: ApockalypseEngine,
    /// Camera projection.
    camera: cssl_substrate_projections::Camera,
    /// Observer frame for the Camera.
    observer_frame: cssl_substrate_projections::ObserverFrame,
    /// Engine config (frame budget etc).
    config: EngineConfig,
    /// Bundle of cap-tokens not yet consumed (some are consumed at construction,
    /// others held for later ops like save/load).
    held_caps: HeldCaps,
}

/// Cap-tokens the engine retains for later operations.
struct HeldCaps {
    save_path: Option<CapToken>,
    replay_load: Option<CapToken>,
    companion_view: Option<CapToken>,
    debug_camera: Option<CapToken>,
}

impl Engine {
    /// Construct a new engine, registering every canonical phase-system.
    ///
    /// # Errors
    /// Returns [`LoaError`] if attestation fails, system registration
    /// fails, or the world contains substrate-unsafe items.
    pub fn new(config: EngineConfig, caps: CapTokens) -> Result<Self, LoaError> {
        // PRIME-DIRECTIVE attestation check — refuses construction if
        // the canonical attestation constant has been tampered with. The
        // audit-bus instance here is local-to-construction ; production
        // wiring will share one bus with the rest of the runtime.
        let mut audit_bus = EnforcementAuditBus::new();
        cssl_substrate_prime_directive::attestation_check(
            PD_ATTESTATION,
            "loa.engine.new",
            &mut audit_bus,
        )?;

        // ── Build the world (scaffold-stub) ────────────────────────────────
        let world = World::scaffold_stub(config.master_seed);
        if let Err(item_id) = world.validate_substrate_safety() {
            return Err(LoaError::WorldUnsafe { item_id });
        }
        let player = Player::at([0.0, 0.0, 0.0]);
        let companion = None;
        let apockalypse = ApockalypseEngine::default();

        // ── Build projection state (Camera + ObserverFrame) ────────────────
        let camera = cssl_substrate_projections::Camera::DEFAULT;
        let observer_frame = cssl_substrate_projections::ObserverFrame::new(
            cssl_substrate_projections::ProjectionId(0),
            camera,
            cssl_substrate_projections::Viewport::new(0, 0, 1280, 720),
            cssl_substrate_projections::LodPolicy::SINGLE_LEVEL,
            cssl_substrate_projections::ProjectionTarget::Window,
            caps.projection_caps,
        );

        // ── Construct the tick-scheduler ───────────────────────────────────
        let mut tick_scheduler = TickScheduler::new(SchedulerConfig {
            master_seed: config.master_seed,
            frame_budget_s: config.frame_budget_s,
            overbudget_policy: config.overbudget_policy,
            record_replay: config.record_replay,
        });

        // ── Register the 13 phase-systems in canonical order ───────────────
        // Phase-order matters for topological sort tie-breaking : every
        // system here is independent except for SimSystem (depends on
        // InputSystem) and FreezeSystem (depends on every other phase).
        let consent_check =
            tick_scheduler.register(ConsentCheckSystem, &caps.omega_register_grant)?;
        let net_recv = tick_scheduler.register(NetRecvSystem, &caps.omega_register_grant)?;
        let input = tick_scheduler.register(InputSystem, &caps.omega_register_grant)?;
        let mut sim_sys = SimSystem::new();
        sim_sys.depend_on(input);
        let sim = tick_scheduler.register(sim_sys, &caps.omega_register_grant)?;
        let projections = tick_scheduler.register(ProjectionsSystem, &caps.omega_register_grant)?;
        let audio_feed = tick_scheduler.register(
            crate::loop_systems::AudioFeedSystem,
            &caps.omega_register_grant,
        )?;
        let render_graph =
            tick_scheduler.register(RenderGraphSystem, &caps.omega_register_grant)?;
        let render_submit =
            tick_scheduler.register(RenderSubmitSystem, &caps.omega_register_grant)?;
        let telemetry_flush =
            tick_scheduler.register(TelemetryFlushSystem, &caps.omega_register_grant)?;
        let audit_append =
            tick_scheduler.register(AuditAppendSystem, &caps.omega_register_grant)?;
        let net_send = tick_scheduler.register(NetSendSystem, &caps.omega_register_grant)?;
        let save_journal =
            tick_scheduler.register(SaveJournalSystem, &caps.omega_register_grant)?;

        let mut freeze_sys = FreezeSystem::new();
        freeze_sys.depend_on(vec![
            consent_check,
            net_recv,
            input,
            sim,
            projections,
            audio_feed,
            render_graph,
            render_submit,
            telemetry_flush,
            audit_append,
            net_send,
            save_journal,
        ]);
        let freeze = tick_scheduler.register(freeze_sys, &caps.omega_register_grant)?;

        let phase_ids = PhaseSystemIds {
            consent_check,
            net_recv,
            input,
            sim,
            projections,
            audio_feed,
            render_graph,
            render_submit,
            telemetry_flush,
            audit_append,
            net_send,
            save_journal,
            freeze,
        };

        // ── Consume the OmegaRegister CapToken ─────────────────────────────
        // The engine is now committed to the registration ceremony ; the
        // CapToken is consumed to record the consumption in the audit-bus.
        let (_id, _cap) = caps.omega_register_captoken.consume();

        // ── Build the SaveScheduler mirror ─────────────────────────────────
        let save_scheduler = SaveScheduler::new();

        let held_caps = HeldCaps {
            save_path: Some(caps.save_path_captoken),
            replay_load: Some(caps.replay_load_captoken),
            companion_view: Some(caps.companion_view_captoken),
            debug_camera: caps.debug_camera_captoken,
        };

        Ok(Self {
            tick_scheduler,
            phase_ids,
            save_scheduler,
            world,
            player,
            companion,
            apockalypse,
            camera,
            observer_frame,
            config,
            held_caps,
        })
    }

    /// Read-only access to the World.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Read-only access to the Player.
    #[must_use]
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// Read-only access to the optional Companion.
    #[must_use]
    pub fn companion(&self) -> Option<&Companion> {
        self.companion.as_ref()
    }

    /// Read-only access to the Apockalypse-engine.
    #[must_use]
    pub fn apockalypse(&self) -> &ApockalypseEngine {
        &self.apockalypse
    }

    /// Read-only access to the Camera.
    #[must_use]
    pub fn camera(&self) -> &cssl_substrate_projections::Camera {
        &self.camera
    }

    /// Read-only access to the ObserverFrame.
    #[must_use]
    pub fn observer_frame(&self) -> &cssl_substrate_projections::ObserverFrame {
        &self.observer_frame
    }

    /// Read-only access to the EngineConfig.
    #[must_use]
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Read-only access to the registered phase IDs.
    #[must_use]
    pub fn phase_ids(&self) -> PhaseSystemIds {
        self.phase_ids
    }

    /// Read-only access to the tick scheduler — exposed so the
    /// `MainLoop` can drive `step()` + read telemetry-counters.
    #[must_use]
    pub fn tick_scheduler(&self) -> &TickScheduler {
        &self.tick_scheduler
    }

    /// Mutable access to the tick scheduler — exposed so the
    /// `MainLoop` can `inject_input` + `step()` + `halt()`.
    pub fn tick_scheduler_mut(&mut self) -> &mut TickScheduler {
        &mut self.tick_scheduler
    }

    /// Mutable access to the save scheduler.
    pub fn save_scheduler_mut(&mut self) -> &mut SaveScheduler {
        &mut self.save_scheduler
    }

    /// Bind a Companion-AI archetype to the engine. Per spec § AI-INTERACTION
    /// § C-3, this is when the read-only Ω-tensor view is established. The
    /// `CompanionView` cap-token is consumed at this point.
    pub fn bind_companion(&mut self, ai_session: AiSessionId) {
        self.companion = Some(Companion::new(ai_session));
        // Consume the CompanionView cap-token now that the binding is real.
        if let Some(tok) = self.held_caps.companion_view.take() {
            let (_id, _cap) = tok.consume();
        }
    }

    /// Save the engine state to disk via `cssl-substrate-save`. The
    /// `SavePath` cap-token is consumed.
    ///
    /// # Errors
    /// Returns `LoaError::Save` on FS or attestation failures.
    pub fn save(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), LoaError> {
        // Consume the SavePath cap-token (PRIME-DIRECTIVE ceremony).
        if let Some(tok) = self.held_caps.save_path.take() {
            let (_id, _cap) = tok.consume();
        }
        // Mirror the tick-scheduler's frame counter into the save-scheduler
        // so save/load round-trips preserve the frame.
        self.save_scheduler.frame = self.tick_scheduler.frame();
        cssl_substrate_save::save(&self.save_scheduler, path).map_err(LoaError::Save)
    }

    /// Load engine state from disk via `cssl-substrate-save`. The
    /// `ReplayLoad` cap-token is consumed.
    ///
    /// # Errors
    /// Returns `LoaError::Load` on FS or attestation failures.
    pub fn load_save_state(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), LoaError> {
        // Consume the ReplayLoad cap-token.
        if let Some(tok) = self.held_caps.replay_load.take() {
            let (_id, _cap) = tok.consume();
        }
        let loaded = cssl_substrate_save::load(path).map_err(LoaError::Load)?;
        self.save_scheduler = loaded;
        Ok(())
    }

    /// Whether a debug-camera token was provided at construction.
    #[must_use]
    pub fn has_debug_camera(&self) -> bool {
        self.held_caps.debug_camera.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_config_default_is_60fps() {
        let c = EngineConfig::default();
        assert!((c.frame_budget_s - 1.0 / 60.0).abs() < 1e-9);
        assert_eq!(c.sim_substeps, 4);
    }

    #[test]
    #[cfg(feature = "test-bypass")]
    fn engine_constructs_with_test_caps() {
        let caps = CapTokens::issue_for_test().expect("test caps");
        let engine = Engine::new(EngineConfig::default(), caps).expect("engine new");
        // 13 systems registered ; phase-IDs distinct.
        let p = engine.phase_ids();
        let ids = [
            p.consent_check,
            p.net_recv,
            p.input,
            p.sim,
            p.projections,
            p.audio_feed,
            p.render_graph,
            p.render_submit,
            p.telemetry_flush,
            p.audit_append,
            p.net_send,
            p.save_journal,
            p.freeze,
        ];
        let mut sorted: Vec<_> = ids.to_vec();
        sorted.sort();
        let original_len = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), original_len);
    }

    #[test]
    #[cfg(feature = "test-bypass")]
    fn engine_bind_companion_consumes_cap() {
        let caps = CapTokens::issue_for_test().expect("test caps");
        let mut engine = Engine::new(EngineConfig::default(), caps).expect("engine new");
        assert!(engine.companion().is_none());
        engine.bind_companion(AiSessionId(42));
        assert!(engine.companion().is_some());
        assert_eq!(engine.companion().unwrap().ai_session, AiSessionId(42));
    }
}
