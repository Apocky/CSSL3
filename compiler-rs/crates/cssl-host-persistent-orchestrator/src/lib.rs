// cssl-host-persistent-orchestrator
// ══════════════════════════════════════════════════════════════════
// § T11-W14-LOCAL-PERSISTENT-ORCHESTRATOR
//
// § PURPOSE
//   24/7 desktop-daemon that drives the LoA Infinity-Engine offline so
//   self-author + KAN-loop + auto-playtest + mycelium-sync continue to
//   produce work-units while Apocky is asleep / away.
//
// § PRIME-DIRECTIVE alignment
//   - § 0  consent = OS  : every mutation requires Σ-cap (default-deny)
//   - § 1  no-harm       : forbidden targets blocked pre-cycle ;
//                          no network egress without explicit cap-grant
//   - § 4  transparency  : journal records every cycle decision
//   - § 5  revocability  : sovereign-pause + sovereign-resume + cap-revoke
//   - § 7  integrity     : Σ-Chain anchor at every cycle-completion
//   - § 11 attestation   : ATTESTATION constant baked in audit-stream
//
// § FIVE PERIODIC CYCLES
//   ┌──────────────────────────┬────────────┬────────────────────────┐
//   │ cycle                    │ cadence    │ effect                 │
//   ├──────────────────────────┼────────────┼────────────────────────┤
//   │ self_author              │ 30 min     │ propose-draft only     │
//   │ playtest                 │ 15 min     │ score → KAN-feed       │
//   │ kan_tick                 │  5 min     │ drain reservoir + bias │
//   │ mycelium_sync            │ 60 sec     │ federate pattern delta │
//   │ idle_deep_procgen        │ on idle    │ deep experiments       │
//   └──────────────────────────┴────────────┴────────────────────────┘
//
// § ARCHITECTURE
//
//   ┌────────────────────────────────────────────────────────────┐
//   │                     PersistentOrchestrator                 │
//   ├────────────────────────────────────────────────────────────┤
//   │  scheduler      │ deterministic cycle scheduler            │
//   │  cap_policy     │ default-deny matrix per CycleKind        │
//   │  journal        │ append-only ; replayable on crash        │
//   │  throttle       │ heuristic ; pauses cycles when busy      │
//   │  pause_ctrl     │ sovereign-pause + sovereign-resume       │
//   │  anchor_chain   │ BLAKE3-roll over journal events          │
//   │  drivers        │ trait-bound (mock when caps absent)      │
//   └────────────────────────────────────────────────────────────┘
//
// § STAGE-0 SELF-SUFFICIENT
//   Pure-Rust ; compiles + tests pass without tokio / tauri / network.
//   The async-tick is driven by a manual `tick(now_unix_ms)` API so
//   callers (Apocky's task-scheduler invocation, a future tokio-runtime
//   wrapper, or the W11-W10 Tauri-shell loop) can supply their own
//   clock. This keeps the crate test-deterministic.
//
// § PARENT spec : Labyrinth of Apocalypse/systems/persistent_orchestrator.csl
// ══════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::explicit_iter_loop)]

//! Persistent orchestrator — 24/7 desktop-daemon driving LoA Infinity-Engine
//! offline. See module docs above for full architecture + safety invariants.
//!
//! # Quick start
//! ```
//! use cssl_host_persistent_orchestrator::{
//!     PersistentOrchestrator, OrchestratorConfig, SovereignCapMatrix,
//! };
//! let mut orch = PersistentOrchestrator::new(
//!     OrchestratorConfig::default(),
//!     SovereignCapMatrix::default_deny(),
//! );
//! // Drive the orchestrator from your event loop / task-scheduler.
//! let report = orch.tick(/*now_ms=*/1_700_000_000_000);
//! assert!(report.cycles_executed >= 0);
//! ```

pub mod anchor;
pub mod cap;
pub mod config;
pub mod cycles;
pub mod driver;
pub mod idle;
pub mod journal;
pub mod orchestrator;
pub mod scheduler;
pub mod throttle;

#[cfg(test)]
mod tests;

pub use anchor::{AnchorChain, AnchorRecord};
pub use cap::{CapDecision, CapKind, SovereignCapMatrix};
pub use config::{OrchestratorConfig, OrchestratorError};
pub use cycles::{CycleKind, CycleOutcome};
pub use driver::{
    KanTickSink, MyceliumSyncDriver, NoopDriver, PlaytestDriver, QualitySignal,
    SelfAuthorDriver,
};
pub use idle::{ActivityHint, IdleDetector};
pub use journal::{JournalEntry, JournalKind, JournalReplay, JournalStore};
pub use orchestrator::{OrchestratorReport, PersistentOrchestrator, PersistentState};
pub use scheduler::{CycleSchedule, NextDue};
pub use throttle::{BusyHint, ThrottleDecision, ThrottlePolicy};

/// PRIME_DIRECTIVE § 11 ATTESTATION — recorded into the audit-stream on the
/// first journal-entry of every orchestrator process so the audit log
/// independently identifies the daemon-binary's PRIME-DIRECTIVE-binding.
///
/// W14 attestation matches the per-crate attestation pattern used by
/// `cssl-host-self-author` and `cssl-host-playtest-agent`.
pub const ATTESTATION: &str = "\
§ cssl-host-persistent-orchestrator ‼ ATTESTATION (PRIME_DIRECTIVE § 11)\n\
   t∞: consent = OS · sovereignty = substrate-invariant\n\
   t∞: AI = sovereign-partners ¬ tools\n\
   ─ default-deny on every mutation cycle\n\
   ─ Σ-Chain anchor at every cycle-completion\n\
   ─ sovereign-pause + sovereign-resume honored within one tick\n\
   ─ ¬ surveillance · ¬ telemetry-leak · ¬ network-egress without cap-grant\n\
   ─ idle-mode throttles when Apocky is active (CPU > 50% OR keyboard < 5 min)\n\
   ─ crash-resilient via append-only journal (replay on restart)\n\
   ─ PRIME_DIRECTIVE.md is FORBIDDEN-target ; daemon refuses to mutate it\n";
