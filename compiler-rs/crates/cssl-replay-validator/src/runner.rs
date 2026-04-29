//! Replay-validator runner — runs two replay-strict runs from the same
//! seed and validates that they produce bit-equal metric histories.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.3 + AC-9 / AC-12.
//!
//! § DISCIPLINE
//!
//!   The runner is **scenario-driven** : a `ScenarioId` selects a
//!   canonical metric-op stream that simulates a representative subset
//!   of engine activity. Each scenario is replayed twice, and the
//!   resulting [`ReplayLogSnapshot`]s are diffed.
//!
//!   The five canonical scenarios (covers most of the L2 catalog) :
//!     S1 — engine-frame-tick     : Counter inc per frame
//!     S2 — omega-step-phases     : Timer record-ns per phase × 6 phases
//!     S3 — render-stage-distrib  : Histogram record-ns across 12 stages
//!     S4 — entity-tier-counts    : Gauge set per tier T0..T3
//!     S5 — sampling-decimation   : OneIn(N) decision recording
//!
//!   These are not the FULL catalog — they are a representative subset
//!   that exercises the four metric kinds + sampling discipline. The
//!   full catalog wires in via Wave-Jζ-1 + Wave-Jζ-2 once those slices
//!   land ; the validator tooling here is reusable across both.
//!
//! [`ReplayLogSnapshot`]: crate::ReplayLogSnapshot

use crate::determinism::{DeterminismMode, ReplayStrictConfig};
use crate::diff::{diff_snapshots, HistoryDiff, HistoryDiffError};
use crate::metric_event::{MetricEvent, MetricEventKind, MetricValue};
use crate::replay_log::{ReplayLog, ReplayLogSnapshot};
use crate::sampling::{SamplingDiscipline, SamplingDisciplineError};
use crate::strict_clock::{StrictClock, SubPhase};
use thiserror::Error;

/// Identifier for a canonical replay-scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScenarioId {
    /// Engine frame-tick — Counter inc per logical frame.
    EngineFrameTick,
    /// Omega-step phases — Timer record-ns per phase × 6 phases per frame.
    OmegaStepPhases,
    /// Render-stage distribution — Histogram record-ns across 12 stages.
    RenderStageDistribution,
    /// Entity-tier counts — Gauge set per tier T0..T3.
    EntityTierCounts,
    /// Sampling decimation — OneIn(N) sampler decision recording.
    SamplingDecimation,
}

impl ScenarioId {
    /// Stable string identifier for the scenario (canonical-bytes layer).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::EngineFrameTick => "S1_engine_frame_tick",
            Self::OmegaStepPhases => "S2_omega_step_phases",
            Self::RenderStageDistribution => "S3_render_stage_distribution",
            Self::EntityTierCounts => "S4_entity_tier_counts",
            Self::SamplingDecimation => "S5_sampling_decimation",
        }
    }

    /// All scenario ids (canonical iteration order).
    pub const ALL: [Self; 5] = [
        Self::EngineFrameTick,
        Self::OmegaStepPhases,
        Self::RenderStageDistribution,
        Self::EntityTierCounts,
        Self::SamplingDecimation,
    ];
}

/// Outcome of running a single scenario twice and diffing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioOutcome {
    pub scenario: ScenarioId,
    pub diff: HistoryDiff,
    pub run_a_snapshot: ReplayLogSnapshot,
    pub run_b_snapshot: ReplayLogSnapshot,
}

impl ScenarioOutcome {
    /// Whether this scenario passed (bit-equal across both runs).
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.diff.is_bit_equal()
    }
}

/// Errors from validator runs.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayRunError {
    /// The mode is not `Strict` ; validator only operates under Strict.
    #[error("PD0166 — replay-validator requires Strict mode ; got Lenient")]
    NotStrictMode,
    /// Scenario simulation failed to append into the replay-log.
    #[error("PD0167 — scenario simulation failed : {reason}")]
    ScenarioFailure { reason: &'static str },
    /// Sampling discipline failed to construct.
    #[error("PD0168 — sampling discipline construction failed")]
    SamplingDisciplineConstruction,
    /// Diff operation failed.
    #[error("PD0169 — diff failed : {0}")]
    DiffFailed(#[from] HistoryDiffError),
}

impl From<SamplingDisciplineError> for ReplayRunError {
    fn from(_e: SamplingDisciplineError) -> Self {
        Self::SamplingDisciplineConstruction
    }
}

/// One replay-run : a deterministic execution of a `ScenarioId` under a
/// `Strict` mode, capturing the resulting [`ReplayLogSnapshot`].
#[derive(Debug, Clone)]
pub struct ReplayRun {
    scenario: ScenarioId,
    mode: DeterminismMode,
    /// Number of logical-frames the run lasts. Default = 30.
    frames: u64,
}

impl ReplayRun {
    /// Construct a run for the given scenario + strict mode.
    pub fn new(scenario: ScenarioId, mode: DeterminismMode) -> Result<Self, ReplayRunError> {
        if !matches!(mode, DeterminismMode::Strict(_)) {
            return Err(ReplayRunError::NotStrictMode);
        }
        Ok(Self {
            scenario,
            mode,
            frames: 30,
        })
    }

    /// Override the number of logical-frames.
    #[must_use]
    pub const fn with_frames(mut self, frames: u64) -> Self {
        self.frames = frames;
        self
    }

    /// Execute the scenario and return the sealed snapshot.
    pub fn execute(&self) -> Result<ReplayLogSnapshot, ReplayRunError> {
        let cfg = match self.mode {
            DeterminismMode::Strict(c) => c,
            DeterminismMode::Lenient => return Err(ReplayRunError::NotStrictMode),
        };
        let mut log = ReplayLog::new();
        match self.scenario {
            ScenarioId::EngineFrameTick => {
                simulate_engine_frame_tick(&mut log, cfg, self.frames)?;
            }
            ScenarioId::OmegaStepPhases => {
                simulate_omega_step_phases(&mut log, cfg, self.frames)?;
            }
            ScenarioId::RenderStageDistribution => {
                simulate_render_stage_distribution(&mut log, cfg, self.frames)?;
            }
            ScenarioId::EntityTierCounts => {
                simulate_entity_tier_counts(&mut log, cfg, self.frames)?;
            }
            ScenarioId::SamplingDecimation => {
                simulate_sampling_decimation(&mut log, cfg, self.frames)?;
            }
        }
        Ok(log.snapshot())
    }
}

/// The replay-validator. Runs scenarios twice and reports bit-equal status.
#[derive(Debug, Clone)]
pub struct ReplayValidator {
    mode: DeterminismMode,
    frames: u64,
}

impl ReplayValidator {
    /// Construct a validator with a `Strict` mode.
    pub fn new(mode: DeterminismMode) -> Result<Self, ReplayRunError> {
        if !matches!(mode, DeterminismMode::Strict(_)) {
            return Err(ReplayRunError::NotStrictMode);
        }
        Ok(Self { mode, frames: 30 })
    }

    /// Override the per-scenario frame-count.
    #[must_use]
    pub const fn with_frames(mut self, frames: u64) -> Self {
        self.frames = frames;
        self
    }

    /// Run a single scenario twice. Returns the outcome.
    pub fn run_scenario(&self, scenario: ScenarioId) -> Result<ScenarioOutcome, ReplayRunError> {
        let run_a = ReplayRun::new(scenario, self.mode)?.with_frames(self.frames);
        let run_b = ReplayRun::new(scenario, self.mode)?.with_frames(self.frames);
        let snap_a = run_a.execute()?;
        let snap_b = run_b.execute()?;
        let diff = diff_snapshots(&snap_a, &snap_b)?;
        Ok(ScenarioOutcome {
            scenario,
            diff,
            run_a_snapshot: snap_a,
            run_b_snapshot: snap_b,
        })
    }

    /// Run all five canonical scenarios.
    pub fn run_all_scenarios(&self) -> Result<Vec<ScenarioOutcome>, ReplayRunError> {
        ScenarioId::ALL
            .iter()
            .map(|&s| self.run_scenario(s))
            .collect()
    }

    /// Convenience : true iff all five canonical scenarios pass.
    pub fn all_pass(&self) -> Result<bool, ReplayRunError> {
        let outcomes = self.run_all_scenarios()?;
        Ok(outcomes.iter().all(ScenarioOutcome::passed))
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Scenario simulators — internal ; deterministic-pure-fn-of-(cfg,frames)
//
//   Each simulator constructs metric-events using only `cfg.seed`,
//   `cfg.start_frame`, `frames`, and the per-scenario op-stream. NO
//   wallclock reads. NO randomness. The op-stream is intentionally
//   uninteresting (engine-tick = 1 op per frame ; phases = 6 ops per
//   frame ; etc) — the point is bit-equal verification, not realism.
// ───────────────────────────────────────────────────────────────────────

fn simulate_engine_frame_tick(
    log: &mut ReplayLog,
    cfg: ReplayStrictConfig,
    frames: u64,
) -> Result<(), ReplayRunError> {
    let metric_id = canonical_metric_id("engine.frame_n");
    for f in 0..frames {
        let frame_n = cfg.start_frame.saturating_add(f);
        let ev = MetricEvent {
            frame_n,
            sub_phase_index: SubPhase::FrameEnd.index(),
            kind: MetricEventKind::CounterIncBy,
            metric_id,
            value: MetricValue::from_u64(1),
            tag_hash: tag_hash_seed(cfg.seed, "engine"),
        };
        log.append(ev)
            .map_err(|_| ReplayRunError::ScenarioFailure {
                reason: "engine_frame_tick replay-log capacity exceeded",
            })?;
    }
    Ok(())
}

fn simulate_omega_step_phases(
    log: &mut ReplayLog,
    cfg: ReplayStrictConfig,
    frames: u64,
) -> Result<(), ReplayRunError> {
    let metric_id = canonical_metric_id("omega_step.phase_time_ns");
    let mut clock = StrictClock::at_frame(cfg.start_frame);
    for _ in 0..frames {
        for &phase in &SubPhase::ORDER {
            clock.jump_to(clock.cursor().0, phase);
            let ev = MetricEvent {
                frame_n: clock.cursor().0,
                sub_phase_index: phase.index(),
                kind: MetricEventKind::TimerRecordNs,
                metric_id,
                // Synthetic phase-budget allocation : COLLAPSE 4ms, etc.
                value: MetricValue::from_u64(phase_synthetic_ns(phase, cfg.seed)),
                tag_hash: tag_hash_seed(cfg.seed, phase.as_str()),
            };
            log.append(ev)
                .map_err(|_| ReplayRunError::ScenarioFailure {
                    reason: "omega_step_phases replay-log capacity exceeded",
                })?;
        }
        // Advance to next frame ; safe — bounded by `frames` outer loop.
        clock.jump_to(clock.cursor().0.saturating_add(1), SubPhase::Collapse);
    }
    Ok(())
}

fn simulate_render_stage_distribution(
    log: &mut ReplayLog,
    cfg: ReplayStrictConfig,
    frames: u64,
) -> Result<(), ReplayRunError> {
    let metric_id = canonical_metric_id("render.stage_time_ns");
    for f in 0..frames {
        let frame_n = cfg.start_frame.saturating_add(f);
        for stage in 1u32..=12 {
            let synth_ns = ((stage as u64).saturating_mul(0x1234)).wrapping_add(cfg.seed);
            let ev = MetricEvent {
                frame_n,
                sub_phase_index: SubPhase::Compose.index(),
                kind: MetricEventKind::HistogramRecord,
                metric_id,
                value: MetricValue::from_u64(synth_ns),
                tag_hash: tag_hash_stage(cfg.seed, stage),
            };
            log.append(ev)
                .map_err(|_| ReplayRunError::ScenarioFailure {
                    reason: "render_stage_distribution replay-log capacity exceeded",
                })?;
        }
    }
    Ok(())
}

fn simulate_entity_tier_counts(
    log: &mut ReplayLog,
    cfg: ReplayStrictConfig,
    frames: u64,
) -> Result<(), ReplayRunError> {
    let metric_id = canonical_metric_id("physics.entity_count");
    for f in 0..frames {
        let frame_n = cfg.start_frame.saturating_add(f);
        for tier in 0u32..4 {
            // Synthetic "count" per tier — deterministic-fn-of-(seed, tier, frame).
            let count_bits = ((tier as u64) ^ cfg.seed)
                .wrapping_mul(101)
                .wrapping_add(frame_n);
            let ev = MetricEvent {
                frame_n,
                sub_phase_index: SubPhase::Propagate.index(),
                kind: MetricEventKind::GaugeSet,
                metric_id,
                value: MetricValue::from_u64(count_bits),
                tag_hash: tag_hash_tier(cfg.seed, tier),
            };
            log.append(ev)
                .map_err(|_| ReplayRunError::ScenarioFailure {
                    reason: "entity_tier_counts replay-log capacity exceeded",
                })?;
        }
    }
    Ok(())
}

fn simulate_sampling_decimation(
    log: &mut ReplayLog,
    cfg: ReplayStrictConfig,
    frames: u64,
) -> Result<(), ReplayRunError> {
    let metric_id = canonical_metric_id("sampling.OneIn3_decision");
    let sampler = SamplingDiscipline::one_in(3)?;
    let tag_h = tag_hash_seed(cfg.seed, "decimation");
    for f in 0..frames {
        let frame_n = cfg.start_frame.saturating_add(f);
        let decision = sampler.should_sample(frame_n, tag_h);
        let ev = MetricEvent {
            frame_n,
            sub_phase_index: SubPhase::FrameEnd.index(),
            kind: MetricEventKind::SamplerDecision,
            metric_id,
            value: MetricValue::from_bool(decision),
            tag_hash: tag_h,
        };
        log.append(ev)
            .map_err(|_| ReplayRunError::ScenarioFailure {
                reason: "sampling_decimation replay-log capacity exceeded",
            })?;
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § Pure helpers — deterministic functions of inputs only
// ───────────────────────────────────────────────────────────────────────

fn canonical_metric_id(name: &'static str) -> u32 {
    // Deterministic mock metric-id : BLAKE3 short-prefix as u32.
    let h = blake3::hash(name.as_bytes());
    let bytes: [u8; 4] = h.as_bytes()[0..4].try_into().unwrap_or([0u8; 4]);
    u32::from_le_bytes(bytes)
}

fn tag_hash_seed(seed: u64, label: &'static str) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(&seed.to_le_bytes());
    h.update(label.as_bytes());
    let out = h.finalize();
    let bytes: [u8; 8] = out.as_bytes()[0..8].try_into().unwrap_or([0u8; 8]);
    u64::from_le_bytes(bytes)
}

fn tag_hash_stage(seed: u64, stage: u32) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(&seed.to_le_bytes());
    h.update(b"stage");
    h.update(&stage.to_le_bytes());
    let out = h.finalize();
    let bytes: [u8; 8] = out.as_bytes()[0..8].try_into().unwrap_or([0u8; 8]);
    u64::from_le_bytes(bytes)
}

fn tag_hash_tier(seed: u64, tier: u32) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(&seed.to_le_bytes());
    h.update(b"tier");
    h.update(&tier.to_le_bytes());
    let out = h.finalize();
    let bytes: [u8; 8] = out.as_bytes()[0..8].try_into().unwrap_or([0u8; 8]);
    u64::from_le_bytes(bytes)
}

fn phase_synthetic_ns(phase: SubPhase, seed: u64) -> u64 {
    // Synthetic-budget-derived ns : phase-index × 1ms + seed-low bits.
    let base = u64::from(phase.index()).saturating_mul(1_000_000);
    base.wrapping_add(seed & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strict_seed_zero() -> DeterminismMode {
        DeterminismMode::strict_with_seed(0)
    }

    #[test]
    fn t_validator_refuses_lenient() {
        let r = ReplayValidator::new(DeterminismMode::Lenient);
        assert_eq!(r.unwrap_err(), ReplayRunError::NotStrictMode);
    }

    #[test]
    fn t_run_refuses_lenient() {
        let r = ReplayRun::new(ScenarioId::EngineFrameTick, DeterminismMode::Lenient);
        assert_eq!(r.unwrap_err(), ReplayRunError::NotStrictMode);
    }

    #[test]
    fn t_run_engine_frame_tick_executes() {
        let r = ReplayRun::new(ScenarioId::EngineFrameTick, strict_seed_zero())
            .unwrap()
            .with_frames(5);
        let snap = r.execute().unwrap();
        assert_eq!(snap.event_count(), 5);
    }

    #[test]
    fn t_run_omega_phases_executes() {
        let r = ReplayRun::new(ScenarioId::OmegaStepPhases, strict_seed_zero())
            .unwrap()
            .with_frames(2);
        let snap = r.execute().unwrap();
        // 2 frames × 6 phases = 12 events
        assert_eq!(snap.event_count(), 12);
    }

    #[test]
    fn t_run_render_stages_executes() {
        let r = ReplayRun::new(ScenarioId::RenderStageDistribution, strict_seed_zero())
            .unwrap()
            .with_frames(3);
        let snap = r.execute().unwrap();
        // 3 frames × 12 stages = 36 events
        assert_eq!(snap.event_count(), 36);
    }

    #[test]
    fn t_run_tier_counts_executes() {
        let r = ReplayRun::new(ScenarioId::EntityTierCounts, strict_seed_zero())
            .unwrap()
            .with_frames(4);
        let snap = r.execute().unwrap();
        // 4 frames × 4 tiers = 16 events
        assert_eq!(snap.event_count(), 16);
    }

    #[test]
    fn t_run_sampling_executes() {
        let r = ReplayRun::new(ScenarioId::SamplingDecimation, strict_seed_zero())
            .unwrap()
            .with_frames(10);
        let snap = r.execute().unwrap();
        assert_eq!(snap.event_count(), 10);
    }

    #[test]
    fn t_validator_run_scenario_passes_when_deterministic() {
        let v = ReplayValidator::new(strict_seed_zero())
            .unwrap()
            .with_frames(5);
        let outcome = v.run_scenario(ScenarioId::EngineFrameTick).unwrap();
        assert!(outcome.passed());
    }

    #[test]
    fn t_validator_run_all_five_scenarios_pass() {
        let v = ReplayValidator::new(strict_seed_zero())
            .unwrap()
            .with_frames(5);
        let outcomes = v.run_all_scenarios().unwrap();
        assert_eq!(outcomes.len(), 5);
        for o in &outcomes {
            assert!(o.passed(), "scenario {:?} did not pass", o.scenario);
        }
    }

    #[test]
    fn t_validator_all_pass_helper() {
        let v = ReplayValidator::new(strict_seed_zero())
            .unwrap()
            .with_frames(3);
        assert!(v.all_pass().unwrap());
    }

    #[test]
    fn t_scenario_id_as_str_distinct() {
        let mut seen = std::collections::HashSet::new();
        for s in ScenarioId::ALL {
            assert!(seen.insert(s.as_str()), "duplicate as_str for {s:?}");
        }
    }

    #[test]
    fn t_scenario_id_all_len_5() {
        assert_eq!(ScenarioId::ALL.len(), 5);
    }

    #[test]
    fn t_canonical_metric_id_deterministic() {
        let a = canonical_metric_id("engine.frame_n");
        let b = canonical_metric_id("engine.frame_n");
        assert_eq!(a, b);
    }

    #[test]
    fn t_canonical_metric_id_distinct() {
        assert_ne!(
            canonical_metric_id("engine.frame_n"),
            canonical_metric_id("render.stage_time_ns")
        );
    }

    #[test]
    fn t_tag_hash_seed_deterministic() {
        assert_eq!(tag_hash_seed(0xC0FFEE, "x"), tag_hash_seed(0xC0FFEE, "x"));
    }

    #[test]
    fn t_tag_hash_stage_distinct_per_stage() {
        let h1 = tag_hash_stage(0, 1);
        let h2 = tag_hash_stage(0, 2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn t_phase_synthetic_ns_deterministic() {
        assert_eq!(
            phase_synthetic_ns(SubPhase::Collapse, 0),
            phase_synthetic_ns(SubPhase::Collapse, 0)
        );
    }

    #[test]
    fn t_run_with_frame_zero_empty_snapshot() {
        let r = ReplayRun::new(ScenarioId::EngineFrameTick, strict_seed_zero())
            .unwrap()
            .with_frames(0);
        let snap = r.execute().unwrap();
        assert_eq!(snap.event_count(), 0);
    }

    #[test]
    fn t_validator_seed_change_diverges() {
        let v0 = ReplayValidator::new(DeterminismMode::strict_with_seed(0))
            .unwrap()
            .with_frames(5);
        let v1 = ReplayValidator::new(DeterminismMode::strict_with_seed(1))
            .unwrap()
            .with_frames(5);
        let s0 = v0.run_scenario(ScenarioId::EngineFrameTick).unwrap();
        let s1 = v1.run_scenario(ScenarioId::EngineFrameTick).unwrap();
        // Both pass (each twice-replays bit-equally) but the snapshot bytes
        // are NOT equal across seeds.
        assert!(s0.passed());
        assert!(s1.passed());
        assert!(!s0.run_a_snapshot.is_bit_equal_to(&s1.run_a_snapshot));
    }

    #[test]
    fn t_validator_frame_count_independence() {
        // 5-frame and 6-frame validators each pass internally.
        let v_5 = ReplayValidator::new(strict_seed_zero())
            .unwrap()
            .with_frames(5);
        let v_6 = ReplayValidator::new(strict_seed_zero())
            .unwrap()
            .with_frames(6);
        assert!(v_5.all_pass().unwrap());
        assert!(v_6.all_pass().unwrap());
    }
}
