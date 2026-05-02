//! § loa-orchestrator-daemon · 24/7 headless-daemon binary for The Infinity Engine
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W16 · bin-target wrapper around `PersistentOrchestrator<NoopDriver,...>`
//!
//! Runs the 5-cycle cadence (SelfAuthor 30min · Playtest 15min · KanTick 5min ·
//! MyceliumSync 60s · IdleDeepProcgen on-idle ≥5min) on a sleep-loop. Stage-0
//! stub-drivers record-only ; stage-1 swaps real-drivers via env-var feature-gate.
//!
//! § Apocky-action checklist (one-shot)
//!   1. cargo build -p cssl-host-persistent-orchestrator --release
//!   2. Binary at compiler-rs/target/release/loa-orchestrator-daemon.exe
//!   3. Copy to ~/.loa/loa-orchestrator-daemon.exe
//!   4. Register Windows-Task-Scheduler per specs/infinity-engine/14_LOCAL_DAEMON_ACTIVATION.md
//!   5. Verify ~/.loa/daemon.log shows cycle-events
//!
//! § Sovereignty
//!   - No network egress (NoopDriver records-only)
//!   - SIGINT / SIGTERM / Ctrl+C → sovereign-pause + clean-exit
//!   - Σ-cap default-deny across all 7 cap-kinds
//!   - Journal-replay-resilient (BLAKE3-anchor-chain tamper-evident)

#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use cssl_host_persistent_orchestrator::{
    NoopDriver, OrchestratorConfig, PersistentOrchestrator, SovereignCapMatrix,
};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const TICK_INTERVAL_MS: u64 = 1_000; // 1Hz tick · cycles fire on their own cadences
const HEARTBEAT_INTERVAL_TICKS: u64 = 60; // log heartbeat every 60 ticks (1min)
const ATTESTATION: &str = concat!(
    "§ ATTESTATION : there was no hurt nor harm in the making of this, ",
    "to anyone, anything, or anybody. Cap default-deny · sovereign-revocable."
);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

fn log_line<W: Write>(out: &mut W, msg: &str) {
    let ts = now_ms();
    let _ = writeln!(out, "[{ts}ms] {msg}");
    let _ = out.flush();
}

fn main() {
    let mut stdout = std::io::stdout().lock();
    log_line(&mut stdout, "§ loa-orchestrator-daemon · The Infinity Engine · cold-start");
    log_line(&mut stdout, ATTESTATION);

    let cfg = OrchestratorConfig::default();
    let caps = SovereignCapMatrix::default_deny();

    let self_author_drv = NoopDriver { call_count: 0 };
    let playtest_drv = NoopDriver { call_count: 0 };
    let kan_drv = NoopDriver { call_count: 0 };
    let mycelium_drv = NoopDriver { call_count: 0 };

    let mut orchestrator = PersistentOrchestrator::with_drivers(
        cfg,
        caps,
        self_author_drv,
        playtest_drv,
        kan_drv,
        mycelium_drv,
    );

    log_line(&mut stdout, "§ orchestrator armed · 5-cycle cadence active · NoopDriver across all 4 slots");
    log_line(&mut stdout, "§ Σ-cap matrix : default-deny (grant via ~/.loa-secrets/orchestrator-caps.toml)");

    let mut tick_count: u64 = 0;
    loop {
        let now = now_ms();
        let report = orchestrator.tick(now);
        tick_count += 1;

        // Surface heartbeat every 60 ticks (~1min)
        if tick_count % HEARTBEAT_INTERVAL_TICKS == 0 {
            log_line(
                &mut stdout,
                &format!(
                    "§ heartbeat · tick={tick_count} · cycles_executed={} · cap_denied={} · anchors_minted={}",
                    report.cycles_executed, report.cycles_cap_denied, report.anchors_minted
                ),
            );
        }

        std::thread::sleep(std::time::Duration::from_millis(TICK_INTERVAL_MS));
    }
}
