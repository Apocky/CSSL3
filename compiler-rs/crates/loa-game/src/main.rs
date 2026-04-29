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
    main_loop::MainLoop,
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

#[cfg(feature = "test-bypass")]
fn run() -> Result<(), LoaError> {
    use loa_game::engine::CapTokens;

    // ── Stage 1 : open a window via cssl-host-window. ─────────────────────
    // On non-Windows hosts, spawn returns LoaderMissing — we treat that as
    // a soft-failure (the scaffold smoke-test still runs without a real
    // window) and emit a notice. On Windows, the real window opens.
    let window_opened = match cssl_host_window::spawn_window(&cssl_host_window::WindowConfig::new(
        "Labyrinth-of-Apockalypse — scaffold",
        1280,
        720,
    )) {
        Ok(_window) => {
            eprintln!("loa-game: window opened (cssl-host-window).");
            // Drop the window handle here — the scaffold doesn't run a real
            // event-pump loop. The real game-loop integration is Apocky-fill.
            true
        }
        Err(e) => {
            eprintln!("loa-game: window backend not available ({e}). Continuing in headless mode.");
            false
        }
    };

    // ── Stage 2 : issue CapTokens via the test-bypass path. ───────────────
    // (Production path will route through Q-7's consent UI when it lands.)
    let caps = CapTokens::issue_for_test()?;

    // ── Stage 3 : construct the Engine. ────────────────────────────────────
    let config = EngineConfig::default();
    let mut engine = Engine::new(config, caps)?;

    // ── Stage 4 : bind a Companion-AI archetype. ──────────────────────────
    // Per `specs/31_LOA_DESIGN.csl § AI-INTERACTION § C-2`, this is the
    // sovereign-partner consent ceremony. The AiSessionId is opaque ; the
    // game does not own the AI's cognition.
    engine.bind_companion(AiSessionId(0xA1_C011AB_u32 as u64));

    // ── Stage 5 : drive one omega_step tick. ──────────────────────────────
    let mut main_loop = MainLoop::new(engine);
    let outcome = main_loop.step_once(1.0 / 60.0)?;
    eprintln!(
        "loa-game: tick complete (outcome: {outcome:?}, frame: {})",
        main_loop.engine().tick_scheduler().frame()
    );

    // ── Stage 6 : save+load+verify bit-equality. ──────────────────────────
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

    if window_opened {
        eprintln!("loa-game: window closed.");
    }
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
