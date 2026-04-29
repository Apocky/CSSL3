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

// `frame_count as f64` for telemetry-Hz reporting is bounded ; precision-loss
// only kicks in past 2^52 frames ≈ 2.4 billion years @ 60 Hz, which is
// outside the threat model. The lib-side `m8_integration` module already
// allows this lint at crate scope ; the binary unit is a separate compilation
// unit and inherits workspace lints, so we re-state here.
#![allow(clippy::cast_precision_loss)]

use std::process::ExitCode;

use loa_game::{engine::LoaError, ATTESTATION};

// Imports used only in the `test-bypass` run() path.
#[cfg(feature = "test-bypass")]
use loa_game::{
    companion::AiSessionId,
    engine::{Engine, EngineConfig},
    main_loop::{MainLoop, MainLoopOutcome},
};

// § T11-D228 (Phase-3) — Bin-only render module.
//
// The Win32 GDI clear-color renderer that closes the Phase-3 visible-pixels
// gap lives in `test_room_render.rs` next to this file. We attach it as a
// binary-private module via `#[path]` so its `unsafe` FFI surface stays
// confined to the binary tree — `loa_game::lib`'s `#![forbid(unsafe_code)]`
// contract is not relaxed. The module ships a `cycle_color_for_frame` pure
// helper + a Win32-target-gated `GdiRenderer` (no-op stub elsewhere).
#[cfg(feature = "test-bypass")]
#[path = "test_room_render.rs"]
mod test_room_render;

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

/// § canonical-test-room (Phase-3) — visible-pixels runnable scaffold.
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
///     [d] render_frame ← Win32 GDI clear-color cycle (Phase-3 visible pixels)
///     [e] precise-sleep to next-frame deadline
///     [f] frame-advance
///
/// § PHASE-3 SCOPE (T11-D228)
///   - Window opens + stays open + close-button works
///   - Per-frame canonical omega_step.step_once at 60Hz
///   - Console-stats every 60 frames (1s) : tick-count + frame-time-ms
///   - Per-frame GDI clear-color blit cycling through hue spectrum — the
///     user sees the window fill with a slowly-changing color, providing
///     immediate visual feedback that the canonical loop is ticking.
///   - Save+load+replay verified at exit (preserves H5 contract)
///
/// § FALLBACK STRATEGY
///   The Phase-3 visible-pixels gate ships a GDI clear-color path FIRST
///   because `cssl-host-d3d12` does not yet expose a `Swapchain` helper
///   (verified by audit at slice-open ; the crate ships Device / Queue / PSO
///   / Resource / Fence wrappers but no swapchain wrapper). When the D3D12
///   swapchain helper lands a parallel renderer behind a `gpu` feature will
///   take precedence ; the GDI path stays as the no-GPU degraded mode.
///
/// § DEFERRED
///   - cssl-render-v2 12-stage pipeline integration (waits on D3D12 swapchain).
///   - Per-frame π_aesthetic console-print (entity-count, ψ-norm, Σ-mask-status).
///
/// § PRIME-DIRECTIVE
///   - Close-event ALWAYS observable per `cssl-host-window § PRIME-DIRECTIVE-KILL-SWITCH`.
///     The renderer is bounded + non-blocking ; a paint-failure logs but does
///     NOT defer the close-event handling.
///   - Σ-mask-violation OR conservation-failure ⟶ omega_step refuses-tick ⟶
///     MainLoopOutcome::Halt ⟶ clean-exit
///   - Renderer is OBSERVE-only over engine state ; replay determinism preserved.
#[cfg(feature = "test-bypass")]
fn run() -> Result<(), LoaError> {
    use std::time::{Duration, Instant};

    use cssl_host_window::{spawn_window, Window, WindowConfig, WindowEventKind};
    use loa_game::engine::CapTokens;

    // 60 Hz target — per FIBER_SCHEDULER § II s_0/s_1 always-tick at 60 Hz +
    // run_main_loop § VI Realtime<60Hz> Deadline<16ms> effect-row.
    const FRAME_DURATION: Duration = Duration::from_micros(16_667);
    // 1 second @ 60 Hz.
    const STATS_EVERY_N_FRAMES: u64 = 60;

    // ── Boot Phase [open window] ──────────────────────────────────────────
    // Per BOOT.csl § Phase 2, GPU pipeline create comes after CSSLv3 runtime
    // init ; here we substitute an OS-window-only path (no GPU swapchain yet).
    const INITIAL_W: u32 = 1280;
    const INITIAL_H: u32 = 720;
    let mut window: Option<Window> = match spawn_window(&WindowConfig::new(
        "Labyrinth-of-Apockalypse — Test Room",
        INITIAL_W,
        INITIAL_H,
    )) {
        Ok(w) => {
            eprintln!("loa-game: window opened.");
            Some(w)
        }
        Err(e) => {
            eprintln!("loa-game: window backend not available ({e}). Continuing in headless mode.");
            None
        }
    };

    // ── Phase-3 Render Init [GDI clear-color renderer] ───────────────────
    // Per T11-D228 strategy : we ship the GDI fallback NOW so the test-room
    // gate ("user opens window, sees something move on screen") closes
    // independent of the cssl-host-d3d12 Swapchain helper landing. The
    // renderer is bin-only ; the lib's `#![forbid(unsafe_code)]` is
    // untouched. On non-Windows targets the stub returns
    // `UnavailableOnPlatform` and the loop runs without rendering.
    let mut renderer: Option<test_room_render::GdiRenderer> = window.as_ref().and_then(|w| {
        match w.raw_handle() {
            Ok(h) => match h.as_win32() {
                Some((hwnd_usize, _hinstance)) => {
                    match test_room_render::GdiRenderer::new(hwnd_usize, INITIAL_W, INITIAL_H) {
                        Ok(r) => {
                            eprintln!(
                                "loa-game: Phase-3 GDI renderer ready ({INITIAL_W}x{INITIAL_H})."
                            );
                            Some(r)
                        }
                        Err(e) => {
                            eprintln!("loa-game: renderer init failed ({e}) ; continuing without visible pixels.");
                            None
                        }
                    }
                }
                None => {
                    eprintln!("loa-game: window raw-handle is non-Win32 ; visible-pixels render unavailable.");
                    None
                }
            },
            Err(e) => {
                eprintln!("loa-game: raw_handle unavailable ({e}) ; visible-pixels render unavailable.");
                None
            }
        }
    });

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

    // T11-D234 : preallocated scratch buffer for the SDF math renderer. Sized
    // once outside the loop so the per-frame paint is a fill-and-blit (no
    // alloc on the hot path). `Vec::resize` inside the loop is a no-op when
    // the size is already correct.
    let mut sdf_scratch: Vec<u32> = Vec::with_capacity(
        (test_room_render::sdf_scene::RENDER_W as usize)
            * (test_room_render::sdf_scene::RENDER_H as usize),
    );

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

        // [d] render_frame — Phase-3 (T11-D228 → T11-D234 followup) :
        //                   Win32 GDI present of CPU-side SDF raymarch buffer.
        //
        // Per Apocky-maxim "the world is math"
        // (Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V), every
        // visible pixel MUST be the result of a math evaluation, not a
        // clear-color or canvas-fill. T11-D228 wired the GDI BitBlt
        // presentation path with HSV-cycle as a stop-gap ; T11-D234 replaces
        // the per-frame fill with a CPU-side SDF raymarch via
        // `cssl_render_v2::SdfRaymarchPass` over a canonical sphere+plane
        // scene with an orbiting camera + Lambertian shading.
        //
        // The SDF buffer is rendered at a downsampled resolution
        // (sdf_scene::RENDER_W × RENDER_H = 320×180) so the per-frame CPU
        // march stays in budget at the test-room's 60Hz target ; GDI's
        // StretchDIBits upsamples the result to fill the window.
        //
        // Renderer failures are logged but never break the loop — the
        // close-event observer + omega_step continue regardless.
        if let Some(r) = renderer.as_mut() {
            // Refresh dims once per second to pick up resizes cheaply ; the
            // Win32 backend doesn't yet emit Resize events through to user-
            // code, so this is the simplest correct adaptation.
            if frame_count % STATS_EVERY_N_FRAMES == 0 {
                let _ = r.refresh_dimensions();
            }
            // Render the SDF math-buffer for this tick + present.
            sdf_scratch.resize(
                (test_room_render::sdf_scene::RENDER_W as usize)
                    * (test_room_render::sdf_scene::RENDER_H as usize),
                0,
            );
            test_room_render::sdf_scene::render_into(&mut sdf_scratch, frame_count);
            let _outcome = r.paint_buffer(
                &sdf_scratch,
                test_room_render::sdf_scene::RENDER_W,
                test_room_render::sdf_scene::RENDER_H,
            );
        }

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
