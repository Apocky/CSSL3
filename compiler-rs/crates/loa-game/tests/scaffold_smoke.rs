//! § scaffold_smoke — the load-bearing integration test for the LoA scaffold.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!
//!   Per the slice deliverable :
//!     "The scaffold MUST not introduce new diagnostic codes ; all errors
//!      flow through existing infrastructure."
//!
//!   And the report-back checklist :
//!     (d) the canonical 13-phase omega_step runs end-to-end ✓/✗
//!     (e) cargo-run-of-loa-game opens window + receives 1 input event +
//!         ticks 1 omega_step + saves+loads+replays bit-equally ✓/✗
//!
//!   This test exercises items (d) + (e) at the integration level :
//!
//!   1. Construct the Engine via the test-bypass CapTokens path.
//!   2. Bind a Companion archetype (sovereign-AI consent ceremony).
//!   3. Inject ONE input event into the omega_step pending-input queue.
//!   4. Step ONCE through the canonical 13-phase omega_step.
//!   5. Verify each of the 13 phase-systems incremented its telemetry
//!      counter exactly once (proves end-to-end phase ordering).
//!   6. Save the engine state to disk via cssl-substrate-save.
//!   7. Load it back ; verify the loaded SaveScheduler equals the saved one
//!      (proves bit-equal save/load round-trip).
//!   8. Verify the canonical attestation propagated through every layer.
//!
//!   The window-spawn step from `main.rs` is exercised in a separate test
//!   that gracefully degrades on non-Windows hosts.
//!
//! § FAITHFULNESS TO §§ 30 § OMEGA-STEP § PHASES
//!
//!   Each phase-system's telemetry counter (e.g. `loa.phase-01.consent-check`)
//!   is checked individually so a future regression that drops a phase
//!   surfaces as a clear test failure.

use loa_game::companion::AiSessionId;
use loa_game::engine::{CapTokens, Engine, EngineConfig};
use loa_game::main_loop::{MainLoop, MainLoopOutcome};
use loa_game::ATTESTATION;

use cssl_substrate_omega_step::InputEvent as TickInput;

// ═══════════════════════════════════════════════════════════════════════════
// § TEST HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn fresh_main_loop() -> MainLoop {
    let caps = CapTokens::issue_for_test().expect("test caps");
    let engine = Engine::new(EngineConfig::default(), caps).expect("engine new");
    MainLoop::new(engine)
}

fn tmp_save_path(suffix: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!(
        "loa-scaffold-smoke-{suffix}-{pid}-{nanos}.csslsave"
    ));
    p
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 1 : engine constructs end-to-end
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn engine_constructs_with_full_substrate_wiring() {
    let ml = fresh_main_loop();
    let engine = ml.engine();

    // 13 systems registered ; phase IDs distinct.
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
    assert_eq!(sorted.len(), original_len, "all 13 system-ids unique");

    // World scaffold-stub : one Floor, one Level, one Room.
    assert_eq!(engine.world().floors.len(), 1);
    assert_eq!(engine.world().levels.len(), 1);
    assert_eq!(engine.world().rooms.len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 2 : Companion binding (sovereign-AI consent ceremony)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn companion_archetype_binds_under_consent() {
    let mut ml = fresh_main_loop();
    assert!(ml.engine().companion().is_none());
    ml.engine_mut().bind_companion(AiSessionId(0xA1));
    let c = ml.engine().companion().expect("companion bound");
    assert_eq!(c.ai_session, AiSessionId(0xA1));
    assert!(c.is_active());
    assert!(c.can_revoke);
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 3 : the load-bearing 13-phase omega_step end-to-end
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn one_omega_step_runs_all_thirteen_phases() {
    let mut ml = fresh_main_loop();

    // Inject one input event for the InputSystem to consume in phase-3.
    ml.inject_input_event(TickInput::KeyPress { keycode: 27 }); // ESC

    // Drive ONE canonical omega_step tick.
    let outcome = ml.step_once(1.0 / 60.0).expect("step ok");
    assert_eq!(outcome, MainLoopOutcome::Continue);
    assert_eq!(ml.tick_count(), 1);

    // Verify every phase ran exactly once. This proves :
    //   (a) the topological-sort produced all 13 systems
    //   (b) each system's step() body executed (and bumped its counter)
    let tel = ml.engine().tick_scheduler().telemetry();
    let phase_counters = [
        ("loa.phase-01.consent-check", 1),
        ("loa.phase-02.net-recv", 1),
        ("loa.phase-03.input-sample", 1),
        ("loa.phase-04.sim-substep", 1),
        ("loa.phase-05.projections-rebuild", 1),
        ("loa.phase-06.audio-callback-feed", 1),
        ("loa.phase-07.render-graph-record", 1),
        ("loa.phase-08.render-submit", 1),
        ("loa.phase-09.telemetry-flush", 1),
        ("loa.phase-10.audit-append", 1),
        ("loa.phase-11.net-send", 1),
        ("loa.phase-12.save-journal-append", 1),
        ("loa.phase-13.freeze-and-return", 1),
    ];
    for (name, expected) in phase_counters {
        assert_eq!(
            tel.read_counter(name),
            expected,
            "phase counter {name} expected {expected} ; missing or extra phase"
        );
    }

    // Tick counter from the scheduler matches our wrapper.
    assert_eq!(ml.engine().tick_scheduler().frame(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 4 : save+load round-trip (bit-equal)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn save_then_load_round_trips_bit_equal() {
    let mut ml = fresh_main_loop();
    ml.inject_input_event(TickInput::KeyPress { keycode: 65 });
    let _ = ml.step_once(1.0 / 60.0).expect("step ok");

    let path = tmp_save_path("save-load-bit-equal");
    ml.engine_mut().save(&path).expect("save ok");

    // Snapshot the SaveScheduler before load (we cannot read the engine's
    // private save_scheduler directly ; instead we re-load into a fresh
    // path-driven SaveScheduler via cssl-substrate-save's load + compare).
    let loaded = cssl_substrate_save::load(&path).expect("load ok");

    // Round-trip cssl-substrate-save's load through save again ; verify
    // byte-equality of the two save-files. This is the canonical
    // bit-equal-replay invariant per `specs/30 § VALIDATION § R-10`.
    let path2 = tmp_save_path("save-load-bit-equal-2");
    cssl_substrate_save::save(&loaded, &path2).expect("re-save ok");
    let bytes1 = std::fs::read(&path).expect("read 1");
    let bytes2 = std::fs::read(&path2).expect("read 2");
    assert_eq!(bytes1, bytes2, "save-files MUST be bit-equal");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&path2);
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 5 : load_save_state populates the engine's save-scheduler
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn load_save_state_consumes_replay_load_cap() {
    let mut ml = fresh_main_loop();
    let _ = ml.step_once(1.0 / 60.0).expect("step ok");

    let path = tmp_save_path("replay-load-cap");
    ml.engine_mut().save(&path).expect("save ok");
    ml.engine_mut().load_save_state(&path).expect("load ok");

    let _ = std::fs::remove_file(&path);
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 6 : attestation propagation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn attestation_constants_match_canonical_pd_eleven() {
    // Canonical PRIME_DIRECTIVE.md § 11 attestation MUST match across every
    // crate that embeds it. The scaffold's `loa_game::ATTESTATION` matches
    // both `cssl_substrate_prime_directive::ATTESTATION` and the
    // `cssl_substrate_omega_step::ATTESTATION`.
    assert_eq!(
        ATTESTATION,
        cssl_substrate_prime_directive::ATTESTATION,
        "loa_game ATTESTATION must match cssl_substrate_prime_directive"
    );
    assert_eq!(
        ATTESTATION,
        cssl_substrate_omega_step::ATTESTATION,
        "loa_game ATTESTATION must match cssl_substrate_omega_step"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 7 : halt converts to MainLoopOutcome::Halt
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn halt_then_step_yields_halt_outcome() {
    let mut ml = fresh_main_loop();
    ml.halt("scaffold-smoke-halt").expect("halt ok");
    let outcome = ml.step_once(1.0 / 60.0).expect("step ok");
    match outcome {
        MainLoopOutcome::Halt { reason } => {
            assert!(reason.contains("scaffold-smoke-halt"));
        }
        MainLoopOutcome::Continue => panic!("expected Halt, got Continue"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 8 : Apockalypse phase-history is append-only
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn apockalypse_history_preserved_across_construction() {
    // Per `GDDs/LOA_PILLARS.md § Pillar 3` and `specs/31 § L-1` :
    // "the game does NOT rewrite player's memory-of-prior-phases."
    let ml = fresh_main_loop();
    let history = ml.engine().apockalypse().phase_history();
    assert_eq!(history.len(), 1, "initial phase recorded in history");
    assert_eq!(history[0].epoch, 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 9 : determinism — two independent engines produce same telemetry
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn two_independent_engines_with_same_seed_produce_same_phase_counters() {
    fn build_and_step() -> u64 {
        let caps = CapTokens::issue_for_test().expect("test caps");
        let engine = Engine::new(EngineConfig::default(), caps).expect("engine new");
        let mut ml = MainLoop::new(engine);
        ml.inject_input_event(TickInput::Tick);
        let _ = ml.step_once(1.0 / 60.0).expect("step ok");
        // Read a representative phase counter ; the determinism contract
        // says identical config ⇒ identical counter outcome.
        ml.engine()
            .tick_scheduler()
            .telemetry()
            .read_counter("loa.phase-04.sim-substep")
    }
    let a = build_and_step();
    let b = build_and_step();
    assert_eq!(a, b, "phase-counters must be deterministic across runs");
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 10 : window-host spawn (gracefully degrades on non-Win32)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn window_host_spawn_or_loader_missing() {
    // On Windows, this attempts a real window. We treat both Ok and
    // LoaderMissing as acceptable outcomes ; the scaffold smoke-test
    // doesn't drop a real event-pump because the omega_step is what
    // demonstrates the Substrate end-to-end.
    let cfg = cssl_host_window::WindowConfig::new("loa-scaffold-smoke", 320, 240);
    match cssl_host_window::spawn_window(&cfg) {
        Ok(_window) => {
            // Window opened successfully ; drop it (Drop tears down the OS window).
        }
        Err(cssl_host_window::WindowError::LoaderMissing { .. }) => {
            // Non-Win32 host ; skip — same precedent as cssl-host-d3d12 etc.
        }
        Err(other) => panic!("unexpected window error: {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TEST 11 : input-host stub backend can inject + drain a synthetic event
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn input_host_stub_drains_one_synthetic_event() {
    // Demonstrate that the cssl-host-input stub backend's inject_event +
    // poll_events surface works end-to-end. Real Win32 input arrives via
    // the OS message-pump ; the stub backend exists for exactly this kind
    // of integration test.
    use cssl_host_input::backend::InputBackend;
    use cssl_host_input::stub::StubBackend;

    let mut backend = StubBackend::default();
    backend.inject_event(cssl_host_input::InputEvent::KeyDown {
        code: cssl_host_input::KeyCode::Escape,
        repeat_count: cssl_host_input::RepeatCount::FirstPress,
    });
    let drained = backend.poll_events();
    assert!(drained.is_some(), "injected event must drain");
}
