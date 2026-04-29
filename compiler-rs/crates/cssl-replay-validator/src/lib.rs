//! CSSLv3 stage0 — Wave-Jζ-5 replay-determinism gate.
//!
//! § SPEC :
//!   - `_drafts/phase_j/06_l2_telemetry_spec.md` § VI (PILLAR-3.6 — Replay-determinism integration H5).
//!   - `_drafts/phase_j/wave_jz_implementation_prompts.md` § VII (slice T11-D154 / re-issued T11-D161).
//!
//! § ROLE
//!
//! This crate is the **replay-determinism gate** for the L2 telemetry layer.
//! It extends `cssl-metrics` + `cssl-log` + `cssl-spec-coverage` with :
//!
//! - `DeterminismMode::{Strict, Lenient}` — canonical mode-flag.
//!   `Strict` corresponds to the spec-§-VI "ReplayStrict" mode ;
//!   `Lenient` corresponds to the spec-§-VI "Realtime" mode.
//!   The `Mixed` variant is intentionally elided here (debug-only ; out-of-scope).
//! - `StrictClock` — the only path through which `monotonic_ns()` may
//!   be observed under `Strict`. The clock's output is a deterministic
//!   function of `(frame_n, sub_phase)` per `§ V` phase-ordering.
//! - `ReplayLog` — append-only metric-event log keyed by frame-N and
//!   replayable from a seed. The log is BLAKE3-hashable and produces
//!   bit-equal outputs across two replay-runs of the same seed.
//! - `ReplayValidator` — runs two replay-strict runs from the same
//!   seed and diffs their `ReplayLog` snapshots byte-for-byte.
//!
//! § H5 CONTRACT PRESERVATION
//!
//!   The H5 replay-determinism contract states : `omega_step` produces
//!   bit-deterministic output given `(seed, inputs)`. This crate extends
//!   that contract to metric-recording — so two replay-runs of the same
//!   seed produce **the same metric-event byte-sequence**, not just the
//!   same simulation state.
//!
//!   The existing H5 acceptance-test harness in `loa-game::tests::*` is
//!   NOT modified by this crate. Instead, this crate provides a
//!   *parallel* validator that the harness CAN call when `--replay-strict`
//!   is enabled (Wave-Jθ wires that ; Wave-Jζ-5 only-provides-the-tool).
//!
//! § FORBIDDEN PATTERNS (per § VI.4)
//!
//! The following are refused at compile-time or runtime under `Strict`:
//!
//! - LM-1 — `monotonic_ns` direct-call (must route via `StrictClock`).
//! - LM-2 — Adaptive sampling discipline (refused at construction).
//! - LM-3 — Welford-online-quantile (out-of-scope ; not exposed).
//! - LM-4 — Data-driven histogram boundaries (only `&[f64]` static).
//! - LM-5 — Atomic-relaxed multi-shard race (acquire-release with merge).
//!
//! § PRIME-DIRECTIVE BINDING
//!
//!   Replay-determinism = consent-to-truthful-self-reporting.
//!   The engine that can be replayed is the engine whose record-keeping
//!   is sovereign. PRIME-DIRECTIVE §1 §11 are honored by ensuring no
//!   wallclock-jitter, no non-determinism, no surveillance-channel can
//!   leak into the metric-history under `Strict`.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod determinism;
pub mod diff;
pub mod metric_event;
pub mod replay_log;
pub mod runner;
pub mod sampling;
pub mod shims;
pub mod strict_clock;

pub use determinism::{DeterminismMode, DeterminismModeKind, ReplayStrictConfig};
pub use diff::{HistoryDiff, HistoryDiffError, HistoryDiffKind};
pub use metric_event::{MetricEvent, MetricEventKind, MetricValue};
pub use replay_log::{ReplayLog, ReplayLogError, ReplayLogSnapshot};
pub use runner::{ReplayRun, ReplayRunError, ReplayValidator, ScenarioId, ScenarioOutcome};
pub use sampling::{
    sampling_decision_strict, SamplingDiscipline, SamplingDisciplineError, OneIn,
};
pub use shims::{
    LogRecord, LogShim, MetricsShim, RecordContext, ReplayLogIntegration, SpecAnchorMock,
    SpecCoverageShim, StrictAware,
};
pub use strict_clock::{
    strict_ns, sub_phase_offset_ns, StrictClock, StrictClockError, SubPhase, FRAME_NS,
};

/// Crate version exposed for scaffold verification + replay-log header.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// Magic-bytes header for the canonical [`ReplayLogSnapshot`] byte-format.
///
/// Discipline : the snapshot byte-format is part of the H5 contract.
/// Any change here is a contract-break and must be paired with a
/// `DECISIONS.md` slice-entry + new-version + replay-corpus regen.
pub const REPLAY_LOG_MAGIC: &[u8; 8] = b"CSSLZRL\x05";

/// Logical frame-N. Used as the canonical time-coordinate under [`Strict`]
/// mode (in place of wallclock).
///
/// [`Strict`]: DeterminismMode::Strict
pub type FrameN = u64;

/// Seed for a replay run. Bit-equal output is guaranteed across two
/// runs that share the same `(ReplayStrictConfig, FrameN-range, ops)`.
pub type ReplaySeed = u64;
