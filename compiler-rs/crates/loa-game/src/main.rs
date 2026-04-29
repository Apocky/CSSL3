//! § main.rs — `cargo run -p loa-game` entry-point.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/31_LOA_DESIGN.csl § GAME-LOOP § ENTRY-POINT`.
//!
//! § THESIS
//!
//!   The binary opens a window via `cssl-host-window`, registers an
//!   input-backend, constructs the [`Engine`] under PRIME-DIRECTIVE consent
//!   ceremony, runs ONE omega_step tick, saves+loads+verifies bit-equality,
//!   then closes cleanly.
//!
//!   This is NOT a runnable game — it is the structural runtime loop
//!   demonstrating the integrated Substrate end-to-end. Real gameplay
//!   waits on Apocky's content slices.
//!
//! § PRIME-DIRECTIVE STAGE-0 PRODUCTION REFUSAL
//!
//!   In a build WITHOUT `test-bypass`, this binary cannot mint CapTokens
//!   (per `cssl-substrate-prime-directive::caps_grant`'s stage-0 production
//!   refusal). The binary surfaces this as a clear stderr message + exits
//!   non-zero. The Apocky-direction-needed `Q-7` consent-UI is the proper
//!   resolution ; until then, `cargo run -p loa-game --features test-bypass`
//!   demonstrates the end-to-end flow with a mocked consent grant.

use std::process::ExitCode;

use loa_game::{engine::LoaError, ATTESTATION};

// Imports used only in the `test-bypass` run() path.
#[cfg(feature = "test-bypass")]
use loa_game::{
    companion::AiSessionId,
    engine::{Engine, EngineConfig},
    main_loop::{MainLoop, MainLoopOutcome},
};

fn main() -> ExitCode {
    eprintln!("loa-game scaffold — Phase-I structural runtime");
    eprintln!("attestation: {ATTESTATION}");

    match run() {
        Ok(()) => {
            eprintln!("loa-game: clean exit");
            ExitCode::SUCCESS
        }
        Err(LoaError::ConsentRefused) => {
            eprintln!(
                "\n\
                 loa-game: consent refused.\n\
                 \n\
                 Stage-0 production builds cannot mint CapTokens because the\n\
                 interactive consent UI (Q-7 from specs/30_SUBSTRATE.csl) is\n\
                 deferred. To exercise the end-to-end scaffold flow, rebuild\n\
                 with the `test-bypass` feature :\n\
                 \n\
                     cargo run -p loa-game --features test-bypass\n\
                 \n\
                 In production deployments, the consent flow lands when the\n\
                 Q-7 UI lands. This refusal IS the PRIME-DIRECTIVE-canonical\n\
                 surface — there is no override flag. PD0001."
            );
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("loa-game: error: {e}");
            ExitCode::from(2)
        }
    }
}

/// § canonical-test-room (Phase-1) — runnable scaffold of the canonical loop.
///
/// § SPEC : `Omniverse/03_RUNTIME/03_FIBER_SCHEDULER.csl.md § VI` (canonical
///          run_main_loop pattern) + `Omniverse/03_RUNTIME/01_COMPUTE_GRAPH.csl.md`
///          (ONE compute-graph, NO separate physics/AI/render/save tick) +
///          `Omniverse/04_OMEGA_FIELD/04_UPDATE_RULE.csl.md § II` (omega_step
///          algorithm).
///
/// § THESIS
///   ONE main-loop ⊗ frame-paced @ 60Hz ⊗ Deadline<16ms> :
///     [a] pump window events → observations
///     [b] Close-event → request_destroy + break
///     [c] main_loop.step_once(1/60) ← unified omega_step (6 phases internal)
///     [d] [DEFERRED Phase-3] render_frame via π_aesthetic projection
///     [e] precise-sleep to next-frame deadline
///     [f] frame-advance
///
/// § PHASE-1 SCOPE (this commit)
///   - Window opens + stays open + close-button works
///   - Per-frame canonical omega_step.step_once at 60Hz
///   - Console-stats every 60 frames (1s) : tick-count + frame-time-ms
///   - Save+load+replay verified at exit (preserves H5 contract)
///
/// § DEFERRED (Phase-2/3)
///   - Phase-2 : per-frame π_aesthetic console-print (entity-count, ψ-norm, Σ-mask-status)
///   - Phase-3 : GPU render via cssl-host-d3d12 swapchain + cssl-render-v2 12-stage
///               pipeline ; pixels-on-screen
///
/// § PRIME-DIRECTIVE
///   - Close-event ALWAYS observable per `cssl-host-window § PRIME-DIRECTIVE-KILL-SWITCH`
///   - Σ-mask-violation OR conservation-failure ⟶ omega_step refuses-tick ⟶
///     MainLoopOutcome::Halt ⟶ clean-exit
#[cfg(feature = "test-bypass")]
fn run() -> Result<(), LoaError> {
    use std::time::{Duration, Instant};

    use cssl_host_window::{spawn_window, Window, WindowConfig, WindowEventKind};
    use loa_game::engine::CapTokens;

    // 60 Hz target — per FIBER_SCHEDULER § II s_0/s_1 always-tick at 60 Hz +
    // run_main_loop § VI Realtime<60Hz> Deadline<16ms> effect-row.
    const FRAME_DURATION: Duration = Duration::from_micros(16_667);
    const STATS_EVERY_N_FRAMES: u64 = 60; // 1 second @ 60 Hz

    // ── Boot Phase [open window] ──────────────────────────────────────────
    // Per BOOT.csl § Phase 2, GPU pipeline create comes after CSSLv3 runtime
    // init ; here we substitute an OS-window-only path (no GPU swapchain yet).
    let mut window: Option<Window> =
        match spawn_window(&WindowConfig::new("Labyrinth-of-Apockalypse — Test Room", 1280, 720)) {
            Ok(w) => {
                eprintln!("loa-game: window opened.");
                Some(w)
            }
            Err(e) => {
                eprintln!(
                    "loa-game: window backend not available ({e}). Continuing in headless mode."
                );
                None
            }
        };

    // ── Boot Phase [CSSLv3 runtime / Sovereign / initial Ω] ──────────────
    // Per BOOT.csl § Phase 1+5+6+7 collapsed into Engine::new for stage-0
    // scaffold. Real boot will split these into the canonical 11 phases.
    let caps = CapTokens::issue_for_test()?;
    let mut engine = Engine::new(EngineConfig::default(), caps)?;
    engine.bind_companion(AiSessionId(0xA1_C011AB_u32 as u64));
    let mut main_loop = MainLoop::new(engine);

    eprintln!("loa-game: test-room loop running ; close window to exit.");
    eprintln!("loa-game: per-frame target = 16.667ms (60 Hz, Realtime<60Hz>, Deadline<16ms>).");

    // ── Canonical run_main_loop ──────────────────────────────────────────
    // Mirrors `Omniverse/03_RUNTIME/03_FIBER_SCHEDULER.csl.md § VI` shape :
    //   loop {
    //     frame_start = Instant::now()
    //     scheduler.resume_due(frame)        ← multi-rate s_0..s_7 fibers
    //     next = omega_step(prev, obs, ops)? ← ONE unified call, 6 phases
    //     render_frame(...)                  ← deferred Phase-3
    //     drain_telemetry                    ← future (cssl-telemetry ring)
    //     precise_sleep(remaining)           ← Deadline<16ms>
    //     frame_advance
    //   }
    let mut frame_count: u64 = 0;
    let mut accumulated_step_micros: u64 = 0;
    let loop_start = Instant::now();

    loop {
        let frame_start = Instant::now();

        // [a] Pump window events ; [b] Close ⟶ break.
        // Per cssl-host-window § PRIME-DIRECTIVE-KILL-SWITCH, the Close event
        // MUST always reach user-code ; we honor it by requesting destroy +
        // breaking the loop. Silent-default-suppress is FORBIDDEN.
        // Window errors are NOT converted to LoaError ; they're printed +
        // treated as a soft-exit (since LoaError has no Window variant in
        // stage-0 ; future slice extends LoaError with a Window arm).
        let mut close_requested = false;
        if let Some(w) = window.as_mut() {
            match w.pump_events() {
                Ok(events) => {
                    for ev in events {
                        if matches!(ev.kind, WindowEventKind::Close) {
                            close_requested = true;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("loa-game: pump_events error ({e}). Exiting.");
                    break;
                }
            }
            if close_requested {
                if let Err(e) = w.request_destroy() {
                    eprintln!("loa-game: request_destroy error ({e}). Exiting.");
                    break;
                }
            }
        }

        // [c] Unified omega_step via main_loop.step_once(1/60).
        // Per COMPUTE_GRAPH.csl § II : one call drives 6 phases internally
        // (COLLAPSE → PROPAGATE → COMPOSE → COHOMOLOGY → AGENCY → ENTROPY).
        // NO separate physics/AI/render/save tick.
        let step_start = Instant::now();
        let outcome = main_loop.step_once(1.0 / 60.0)?;
        accumulated_step_micros += u64::try_from(step_start.elapsed().as_micros()).unwrap_or(0);

        let halt_reason = match outcome {
            MainLoopOutcome::Continue => None,
            MainLoopOutcome::Halt { reason } => Some(reason),
        };

        // [d] render_frame ⟶ deferred Phase-3 (no GPU swapchain wired yet).
        //     When wired : let aesthetic = engine.omega().project_aesthetic(camera)
        //                  pipeline.execute(&aesthetic) ; swapchain.present()

        // Stats every STATS_EVERY_N_FRAMES.
        if frame_count > 0 && frame_count % STATS_EVERY_N_FRAMES == 0 {
            let avg_step_us = accumulated_step_micros / STATS_EVERY_N_FRAMES;
            let wall_elapsed_s = loop_start.elapsed().as_secs_f64();
            let achieved_hz = (frame_count as f64) / wall_elapsed_s;
            eprintln!(
                "loa-game: frame={frame_count} achieved={achieved_hz:.2}Hz \
                 omega_step_avg={avg_step_us}μs scheduler_frame={}",
                main_loop.engine().tick_scheduler().frame()
            );
            accumulated_step_micros = 0;
        }

        if let Some(reason) = halt_reason {
            eprintln!("loa-game: omega_step requested halt ({reason}). Exiting.");
            break;
        }
        if close_requested {
            eprintln!("loa-game: window close requested. Exiting cleanly.");
            break;
        }
        if window.as_ref().is_some_and(Window::is_destroyed) {
            eprintln!("loa-game: window destroyed externally. Exiting.");
            break;
        }

        // [e] Precise-sleep to deadline.
        // Per FIBER_SCHEDULER § V Deadline<16ms> : if frame exceeds budget,
        // we DEFER to next frame (no-op ; std::thread::sleep skipped).
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
        // [f] frame-advance.
        frame_count = frame_count.saturating_add(1);
    }

    // ── Save+load+verify bit-equality (preserves H5 contract). ──────────
    let tmp_path = {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "loa-scaffold-{}-{}.csslsave",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        p
    };
    main_loop.engine_mut().save(&tmp_path)?;
    main_loop.engine_mut().load_save_state(&tmp_path)?;
    eprintln!(
        "loa-game: save/load round-trip succeeded ({})",
        tmp_path.display()
    );
    let _ = std::fs::remove_file(&tmp_path);

    eprintln!(
        "loa-game: clean exit ; total_frames={frame_count} wall={:.2}s",
        loop_start.elapsed().as_secs_f64()
    );
    Ok(())
}

#[cfg(not(feature = "test-bypass"))]
fn run() -> Result<(), LoaError> {
    // Stage-0 production builds cannot mint CapTokens (per
    // `cssl-substrate-prime-directive::caps_grant`'s production refusal).
    // The proper resolution is the Q-7 consent UI ; until then we surface
    // ConsentRefused so main() prints the canonical guidance.
    Err(LoaError::ConsentRefused)
}
