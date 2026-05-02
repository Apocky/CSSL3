// § orchestrator.rs — top-level driver-loop.
//
// § thesis
//   `PersistentOrchestrator::tick(now_ms)` is the single entry point :
//     1. Refresh idle-detector + busy-hint
//     2. Honor sovereign-pause (return early ; emit Paused journal entry)
//     3. Pick due cycles via scheduler
//     4. For each due cycle :
//        a. Cap-policy decision   → CapDenied
//        b. Throttle-policy       → Throttled
//        c. Drive the cycle (via trait) → Success / DriverError
//     5. Fold each outcome into the anchor-chain
//     6. If KAN-update count crosses anchor-threshold → mint a SigmaAnchor
//     7. Append everything to the journal
//
//   The whole tick is synchronous + deterministic given the same inputs.
//   This is how we keep tests reliable + how journal-replay reconstructs
//   state without re-running cycles.

use crate::anchor::{AnchorChain, AnchorReason};
use crate::cap::{CapDecision, CapKind, SovereignCapMatrix};
use crate::config::{OrchestratorConfig, OrchestratorError};
use crate::cycles::{CycleKind, CycleOutcome};
use crate::driver::{
    KanTickSink, MyceliumSyncDriver, PlaytestDriver, QualitySignal, SelfAuthorDriver,
};
use crate::idle::{ActivityHint, IdleDetector};
use crate::journal::{JournalKind, JournalStore};
use crate::scheduler::CycleSchedule;
use crate::throttle::{BusyHint, ThrottleDecision, ThrottlePolicy};

/// Mutable state — owned by [`PersistentOrchestrator`] but exposed read-only
/// via accessors so the W11-W10 UI can render the live status panel.
#[derive(Debug, Clone)]
pub struct PersistentState {
    pub schedule: CycleSchedule,
    pub anchors: AnchorChain,
    pub idle: IdleDetector,
    pub paused: bool,
    pub kan_updates_since_last_anchor: u64,
    pub last_tick_ms: u64,
}

/// Outcome of one orchestrator tick.
#[derive(Debug, Clone, Default)]
pub struct OrchestratorReport {
    pub cycles_executed: u64,
    pub cycles_throttled: u64,
    pub cycles_cap_denied: u64,
    pub anchors_minted: u64,
    pub kan_updates: u64,
    pub paused: bool,
    pub idle: bool,
    pub last_outcomes: Vec<CycleOutcome>,
}

/// The 24/7 daemon-driver. Compose it with concrete drivers via
/// [`with_self_author_driver`] / etc, then call [`tick`] from your event loop.
pub struct PersistentOrchestrator<S, P, K, M>
where
    S: SelfAuthorDriver,
    P: PlaytestDriver,
    K: KanTickSink,
    M: MyceliumSyncDriver,
{
    cfg: OrchestratorConfig,
    pub caps: SovereignCapMatrix,
    pub state: PersistentState,
    pub journal: JournalStore,
    pub throttle: ThrottlePolicy,
    pub self_author: S,
    pub playtest: P,
    pub kan: K,
    pub mycelium: M,
}

impl<S, P, K, M> PersistentOrchestrator<S, P, K, M>
where
    S: SelfAuthorDriver,
    P: PlaytestDriver,
    K: KanTickSink,
    M: MyceliumSyncDriver,
{
    /// Construct with explicit drivers. For the spec-default no-op layout,
    /// use [`PersistentOrchestrator::new`] instead.
    pub fn with_drivers(
        cfg: OrchestratorConfig,
        caps: SovereignCapMatrix,
        self_author: S,
        playtest: P,
        kan: K,
        mycelium: M,
    ) -> Self {
        let throttle = ThrottlePolicy::new(
            cfg.cpu_throttle_threshold_pct,
            cfg.idle_threshold_ms,
        );
        let mut journal = JournalStore::new(cfg.journal_ring_capacity);
        // ATTESTATION : every daemon process emits its attestation-hash on
        // the first journal entry. PRIME_DIRECTIVE § 11.
        let attestation_hash = blake3::hash(crate::ATTESTATION.as_bytes());
        journal.append(
            0,
            JournalKind::Bootstrap {
                attestation_blake3: *attestation_hash.as_bytes(),
            },
        );
        Self {
            state: PersistentState {
                schedule: CycleSchedule::fresh(0),
                anchors: AnchorChain::genesis(),
                idle: IdleDetector::new(cfg.idle_threshold_ms),
                paused: false,
                kan_updates_since_last_anchor: 0,
                last_tick_ms: 0,
            },
            cfg,
            caps,
            journal,
            throttle,
            self_author,
            playtest,
            kan,
            mycelium,
        }
    }

    /// Sovereign-pause — Apocky says "halt all work NOW". Honored within one tick.
    pub fn sovereign_pause(&mut self, now_ms: u64) {
        self.state.paused = true;
        self.journal.append(now_ms, JournalKind::SovereignPause);
        // Mint a pause-anchor so the audit-trail records the exact tick.
        let rec = self
            .state
            .anchors
            .fold(b"sovereign-pause", now_ms, AnchorReason::SovereignPause);
        self.journal.append(now_ms, JournalKind::Anchor(rec));
    }

    /// Sovereign-resume — Apocky says "go". Honored within one tick.
    pub fn sovereign_resume(&mut self, now_ms: u64) {
        self.state.paused = false;
        self.journal.append(now_ms, JournalKind::SovereignResume);
        let rec = self
            .state
            .anchors
            .fold(b"sovereign-resume", now_ms, AnchorReason::SovereignResume);
        self.journal.append(now_ms, JournalKind::Anchor(rec));
    }

    /// Apocky-only : grant a cap mid-run.
    pub fn grant_cap(&mut self, cap: CapKind, now_ms: u64) {
        self.caps.grant(cap);
        self.journal.append(
            now_ms,
            JournalKind::CapPolicyChange {
                cap_name: cap.name().to_string(),
                granted: true,
            },
        );
    }

    /// Apocky-only : revoke a cap mid-run. PRIME-DIRECTIVE § 5 revocability.
    pub fn revoke_cap(&mut self, cap: CapKind, now_ms: u64) {
        self.caps.revoke(cap);
        self.journal.append(
            now_ms,
            JournalKind::CapPolicyChange {
                cap_name: cap.name().to_string(),
                granted: false,
            },
        );
    }

    /// Drive one tick. The MAIN entry point.
    ///
    /// `now_ms` : wall-clock ms — host-supplied so tests stay deterministic.
    /// Returns a [`OrchestratorReport`] summarizing what happened on this tick.
    pub fn tick(&mut self, now_ms: u64) -> OrchestratorReport {
        self.tick_with_hints(now_ms, BusyHint::default(), ActivityHint::default())
    }

    /// Drive one tick with explicit busy + activity hints. Real hosts pull
    /// CPU% from `sysinfo` + last-input-age from the OS-input-monitor APIs.
    pub fn tick_with_hints(
        &mut self,
        now_ms: u64,
        busy: BusyHint,
        activity: ActivityHint,
    ) -> OrchestratorReport {
        self.state.last_tick_ms = now_ms;
        self.state.idle.observe(activity);
        let mut report = OrchestratorReport::default();
        report.idle = self.state.idle.is_idle(now_ms, activity);
        report.paused = self.state.paused;

        // Sovereign-pause short-circuits the whole tick. We still log a
        // QuiescentTick so the audit-trail shows the daemon was alive.
        if self.state.paused {
            self.journal.append(
                now_ms,
                JournalKind::QuiescentTick {
                    hint_summary: format!("paused cpu={} idle={}", busy.cpu_pct, report.idle),
                },
            );
            return report;
        }

        let due = self.state.schedule.pick_due(now_ms, self.cfg.max_cycles_per_tick);
        if due.is_empty() {
            self.journal.append(
                now_ms,
                JournalKind::QuiescentTick {
                    hint_summary: format!(
                        "no-cycle-due cpu={} idle_ms={}",
                        busy.cpu_pct, busy.last_input_age_ms
                    ),
                },
            );
            return report;
        }

        for kind in due {
            // Cap-policy first — never run a cycle whose caps are missing.
            let cap_decision = self.caps.check(kind);
            if let CapDecision::Deny { cycle: _, missing } = cap_decision {
                let outcome = CycleOutcome::CapDenied {
                    kind,
                    required: missing.name().to_string(),
                };
                self.journal
                    .append(now_ms, JournalKind::CycleOutcome(outcome.clone()));
                report.cycles_cap_denied += 1;
                report.last_outcomes.push(outcome);
                // Defer the cycle so we re-check soon (Apocky might grant the cap).
                self.state.schedule.defer(kind, now_ms, &self.cfg);
                continue;
            }

            // Throttle-policy second — defer if system is busy.
            let throttle_decision = self.throttle.decide(kind, busy);
            if matches!(throttle_decision, ThrottleDecision::Throttle) {
                let outcome = CycleOutcome::Throttled { kind };
                self.journal
                    .append(now_ms, JournalKind::CycleOutcome(outcome.clone()));
                report.cycles_throttled += 1;
                report.last_outcomes.push(outcome);
                self.state.schedule.defer(kind, now_ms, &self.cfg);
                continue;
            }

            // Run the cycle.
            let outcome = self.run_cycle(kind, now_ms);
            if outcome.is_success() {
                report.cycles_executed += 1;
                self.state.schedule.mark_ran(kind, now_ms, &self.cfg);
            } else {
                self.state.schedule.defer(kind, now_ms, &self.cfg);
            }
            self.journal
                .append(now_ms, JournalKind::CycleOutcome(outcome.clone()));

            // Fold outcome bytes into anchor-chain (every cycle-close anchors).
            let payload = serde_json::to_vec(&outcome).unwrap_or_default();
            let rec = self
                .state
                .anchors
                .fold(&payload, now_ms, AnchorReason::CycleClose);
            self.journal.append(now_ms, JournalKind::Anchor(rec));
            report.anchors_minted += 1;
            report.last_outcomes.push(outcome);
        }

        // KAN-anchor-cadence : every N updates, mint a dedicated KAN anchor.
        if self.state.kan_updates_since_last_anchor >= self.cfg.anchor_every_n_kan_updates {
            let bias_digest = self.kan.bias_digest();
            let rec = self
                .state
                .anchors
                .fold(&bias_digest, now_ms, AnchorReason::KanThreshold);
            self.journal.append(now_ms, JournalKind::Anchor(rec));
            report.anchors_minted += 1;
            self.state.kan_updates_since_last_anchor = 0;
        }

        report.kan_updates = self.state.kan_updates_since_last_anchor;
        report
    }

    /// Read-only accessor for the latest anchor — UI panels use this.
    pub fn latest_anchor(&self) -> ([u8; 32], u64) {
        (
            self.state.anchors.current_digest(),
            self.state.anchors.current_seq(),
        )
    }

    /// Run one cycle. Returns its outcome.
    fn run_cycle(&mut self, kind: CycleKind, now_ms: u64) -> CycleOutcome {
        // Deterministic per-cycle seed derived from (kind, now_ms, anchor-seq)
        // so replays produce identical driver-input even when wall-clock differs.
        let seed = blake3::Hasher::new()
            .update(&[kind as u8])
            .update(&now_ms.to_le_bytes())
            .update(&self.state.anchors.current_seq().to_le_bytes())
            .finalize();
        let seed_u64 = u64::from_le_bytes(seed.as_bytes()[..8].try_into().unwrap());

        match kind {
            CycleKind::SelfAuthor => match self.self_author.run_self_author_cycle(now_ms, seed_u64)
            {
                Ok(stat) => CycleOutcome::Success { kind, stat },
                Err(message) => CycleOutcome::DriverError { kind, message },
            },
            CycleKind::Playtest => match self.playtest.run_playtest_cycle(now_ms, seed_u64) {
                Ok((stat, signals)) => {
                    for sig in signals {
                        self.kan.submit(sig);
                    }
                    CycleOutcome::Success { kind, stat }
                }
                Err(message) => CycleOutcome::DriverError { kind, message },
            },
            CycleKind::KanTick => {
                let updates = self.kan.tick(now_ms);
                self.state.kan_updates_since_last_anchor = self
                    .state
                    .kan_updates_since_last_anchor
                    .saturating_add(updates);
                CycleOutcome::Success {
                    kind,
                    stat: format!("kan_updates={updates}"),
                }
            }
            CycleKind::MyceliumSync => match self.mycelium.tick(now_ms) {
                Ok(deltas) => CycleOutcome::Success {
                    kind,
                    stat: format!("mycelium_deltas={deltas}"),
                },
                Err(message) => CycleOutcome::DriverError { kind, message },
            },
            CycleKind::IdleDeepProcgen => {
                // Deep-procgen uses self-author driver under elevated-priority semantics.
                // The cap-check above already verified IdleEscalate is granted.
                match self.self_author.run_self_author_cycle(now_ms, !seed_u64) {
                    Ok(stat) => CycleOutcome::Success {
                        kind,
                        stat: format!("idle_procgen({stat})"),
                    },
                    Err(message) => CycleOutcome::DriverError { kind, message },
                }
            }
        }
    }
}

/// Convenience constructor : noop-driver layout. Real production caller
/// uses [`PersistentOrchestrator::with_drivers`] + supplies the wireup-crate's
/// concrete drivers.
impl PersistentOrchestrator<crate::driver::NoopDriver, crate::driver::NoopDriver, crate::driver::NoopDriver, crate::driver::NoopDriver> {
    pub fn new(cfg: OrchestratorConfig, caps: SovereignCapMatrix) -> Self {
        Self::with_drivers(
            cfg,
            caps,
            crate::driver::NoopDriver::default(),
            crate::driver::NoopDriver::default(),
            crate::driver::NoopDriver::default(),
            crate::driver::NoopDriver::default(),
        )
    }
}

// Avoid unused-import warning when only some modules use the type.
#[allow(dead_code)]
fn _force_use(_o: OrchestratorError, _q: QualitySignal) {}

/// Re-export the AnchorRecord here for ergonomic use-sites.
pub use crate::anchor::AnchorRecord as PublicAnchorRecord;
