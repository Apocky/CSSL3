// § config.rs — orchestrator configuration + error types.
//
// § cycle-cadences are EXACT default values from spec/persistent_orchestrator.csl
//   The constants are public so the W11-W10 Mycelium-Desktop UI can echo them
//   back to Apocky in a config-confirmation panel.

use thiserror::Error;

use crate::cycles::CycleKind;

/// Default cadence : 30 minutes.
pub const SELF_AUTHOR_CADENCE_MS: u64 = 30 * 60 * 1_000;
/// Default cadence : 15 minutes.
pub const PLAYTEST_CADENCE_MS: u64 = 15 * 60 * 1_000;
/// Default cadence : 5 minutes.
pub const KAN_TICK_CADENCE_MS: u64 = 5 * 60 * 1_000;
/// Default cadence : 60 seconds.
pub const MYCELIUM_SYNC_CADENCE_MS: u64 = 60 * 1_000;
/// Default cadence : on idle ≥ 5 min.
pub const IDLE_THRESHOLD_MS: u64 = 5 * 60 * 1_000;

/// Σ-Chain anchor cadence — one anchor every 1024 successful KAN updates,
/// matching the W12-3 KAN-anchor invariant. The orchestrator uses the same
/// constant so anchors stay aligned across the two crates.
pub const SIGMA_CHAIN_ANCHOR_EVERY_N_UPDATES: u64 = 1024;

/// Throttle threshold : pause non-idle cycles when CPU% exceeds this.
pub const CPU_THROTTLE_THRESHOLD_PCT: u8 = 50;

/// Maximum journal-entries kept in-memory before forced compaction. Keeps the
/// daemon's heap-footprint bounded even on a multi-day run.
pub const JOURNAL_RING_CAPACITY: usize = 8192;

/// Configuration for [`PersistentOrchestrator`](crate::PersistentOrchestrator).
///
/// Construct via [`OrchestratorConfig::default`] for the spec-canon cadence
/// values, or build a custom instance for tests / Apocky-tuning.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OrchestratorConfig {
    pub self_author_cadence_ms: u64,
    pub playtest_cadence_ms: u64,
    pub kan_tick_cadence_ms: u64,
    pub mycelium_sync_cadence_ms: u64,
    pub idle_threshold_ms: u64,
    pub anchor_every_n_kan_updates: u64,
    pub cpu_throttle_threshold_pct: u8,
    pub journal_ring_capacity: usize,
    pub max_cycles_per_tick: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            self_author_cadence_ms: SELF_AUTHOR_CADENCE_MS,
            playtest_cadence_ms: PLAYTEST_CADENCE_MS,
            kan_tick_cadence_ms: KAN_TICK_CADENCE_MS,
            mycelium_sync_cadence_ms: MYCELIUM_SYNC_CADENCE_MS,
            idle_threshold_ms: IDLE_THRESHOLD_MS,
            anchor_every_n_kan_updates: SIGMA_CHAIN_ANCHOR_EVERY_N_UPDATES,
            cpu_throttle_threshold_pct: CPU_THROTTLE_THRESHOLD_PCT,
            journal_ring_capacity: JOURNAL_RING_CAPACITY,
            max_cycles_per_tick: 4,
        }
    }
}

impl OrchestratorConfig {
    /// Per-cycle cadence lookup helper.
    pub fn cadence_for(&self, kind: CycleKind) -> u64 {
        match kind {
            CycleKind::SelfAuthor => self.self_author_cadence_ms,
            CycleKind::Playtest => self.playtest_cadence_ms,
            CycleKind::KanTick => self.kan_tick_cadence_ms,
            CycleKind::MyceliumSync => self.mycelium_sync_cadence_ms,
            CycleKind::IdleDeepProcgen => self.idle_threshold_ms,
        }
    }
}

/// Top-level error class for the orchestrator. All variants are non-panicking.
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("cap-deny : {kind} cycle requires capability {required:?} but none granted")]
    CapDeny { kind: &'static str, required: &'static str },
    #[error("paused : sovereign-pause held ; cycle {kind} skipped")]
    Paused { kind: &'static str },
    #[error("throttled : busy-hint over threshold ; cycle {kind} deferred")]
    Throttled { kind: &'static str },
    #[error("driver-error : {0}")]
    Driver(String),
    #[error("journal-error : {0}")]
    Journal(String),
}
