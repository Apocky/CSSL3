//! Shims for cssl-metrics + cssl-log + cssl-spec-coverage.
//!
//! § STATUS : MOCKED — real crates land in T11-D156 / T11-D157 / T11-D160.
//!     Once those slices merge, replace each shim with `pub use real_crate::*`.
//!
//! § ROLE
//!
//! This module exposes the canonical surfaces that this crate (D161)
//! needs from the three sibling slices :
//!
//! - `MetricsShim`      mocks `cssl-metrics` types (Counter / Gauge / etc).
//! - `LogShim`          mocks `cssl-log::AuditEmit::record`.
//! - `SpecCoverageShim` mocks `cssl-spec-coverage::SpecAnchor`.
//! - `ReplayLogIntegration` is the trait the real crates will implement
//!   to plug their record-paths into our [`ReplayLog`].
//! - `StrictAware` is the trait the real crates will implement to
//!   consult the [`DeterminismMode`] and route through [`StrictClock`].
//!
//! § SWAP-IN PROCEDURE
//!
//! When D156 / D157 / D160 land :
//!
//! - Delete the three shim structs.
//! - Replace `pub use shims::*` in `lib.rs` with `pub use cssl_metrics::*` (etc).
//! - The two traits ([`StrictAware`] + [`ReplayLogIntegration`]) STAY —
//!   they are the canonical extension-point this crate owns.
//!
//! [`ReplayLog`]: crate::ReplayLog
//! [`DeterminismMode`]: crate::DeterminismMode
//! [`StrictClock`]: crate::StrictClock

use crate::determinism::DeterminismMode;
use crate::metric_event::{MetricEvent, MetricEventKind, MetricValue};
use crate::replay_log::ReplayLog;
use crate::strict_clock::SubPhase;
use crate::FrameN;

/// Recording context — bundles `(frame_n, sub_phase, metric_id, tag_hash)`
/// to keep [`MetricsShim`] method signatures readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordContext {
    pub frame_n: FrameN,
    pub sub_phase: SubPhase,
    pub metric_id: u32,
    pub tag_hash: u64,
}

impl RecordContext {
    /// Construct from explicit fields.
    #[must_use]
    pub const fn new(
        frame_n: FrameN,
        sub_phase: SubPhase,
        metric_id: u32,
        tag_hash: u64,
    ) -> Self {
        Self {
            frame_n,
            sub_phase,
            metric_id,
            tag_hash,
        }
    }
}

/// Mock surface for `cssl-metrics` (D157).
///
/// § Will-be-replaced-by : `cssl_metrics::{Counter, Gauge, Histogram, Timer}`.
#[derive(Debug, Clone, Default)]
pub struct MetricsShim {
    /// Last recorded value, for testing the mock surface.
    pub last_recorded: Option<MetricValue>,
    /// Total record-calls made through this shim.
    pub record_count: u64,
}

impl MetricsShim {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_recorded: None,
            record_count: 0,
        }
    }

    /// Mock counter-inc.
    pub fn counter_inc(
        &mut self,
        log: &mut ReplayLog,
        mode: DeterminismMode,
        ctx: RecordContext,
        n: u64,
    ) {
        self.record_count += 1;
        self.last_recorded = Some(MetricValue::from_u64(n));
        if mode.engages_replay_log() {
            // In Strict mode : append to replay-log, do NOT perturb engine.
            let ev = MetricEvent {
                frame_n: ctx.frame_n,
                sub_phase_index: ctx.sub_phase.index(),
                kind: MetricEventKind::CounterIncBy,
                metric_id: ctx.metric_id,
                value: MetricValue::from_u64(n),
                tag_hash: ctx.tag_hash,
            };
            // Errors deliberately swallowed : a real cssl-metrics will
            // emit an Audit-event on capacity-exceeded ; the shim doesn't
            // need to preserve that path.
            let _ = log.append(ev);
        }
    }

    /// Mock gauge-set.
    pub fn gauge_set(
        &mut self,
        log: &mut ReplayLog,
        mode: DeterminismMode,
        ctx: RecordContext,
        v: f64,
    ) {
        self.record_count += 1;
        self.last_recorded = Some(MetricValue::from_f64(v));
        if mode.engages_replay_log() {
            let ev = MetricEvent {
                frame_n: ctx.frame_n,
                sub_phase_index: ctx.sub_phase.index(),
                kind: MetricEventKind::GaugeSet,
                metric_id: ctx.metric_id,
                value: MetricValue::from_f64(v),
                tag_hash: ctx.tag_hash,
            };
            let _ = log.append(ev);
        }
    }

    /// Mock histogram-record.
    pub fn histogram_record(
        &mut self,
        log: &mut ReplayLog,
        mode: DeterminismMode,
        ctx: RecordContext,
        v: f64,
    ) {
        self.record_count += 1;
        self.last_recorded = Some(MetricValue::from_f64(v));
        if mode.engages_replay_log() {
            let ev = MetricEvent {
                frame_n: ctx.frame_n,
                sub_phase_index: ctx.sub_phase.index(),
                kind: MetricEventKind::HistogramRecord,
                metric_id: ctx.metric_id,
                value: MetricValue::from_f64(v),
                tag_hash: ctx.tag_hash,
            };
            let _ = log.append(ev);
        }
    }

    /// Mock timer-record-ns.
    pub fn timer_record_ns(
        &mut self,
        log: &mut ReplayLog,
        mode: DeterminismMode,
        ctx: RecordContext,
        ns: u64,
    ) {
        self.record_count += 1;
        self.last_recorded = Some(MetricValue::from_u64(ns));
        if mode.engages_replay_log() {
            let ev = MetricEvent {
                frame_n: ctx.frame_n,
                sub_phase_index: ctx.sub_phase.index(),
                kind: MetricEventKind::TimerRecordNs,
                metric_id: ctx.metric_id,
                value: MetricValue::from_u64(ns),
                tag_hash: ctx.tag_hash,
            };
            let _ = log.append(ev);
        }
    }
}

/// Mock surface for `cssl-log` (D156).
///
/// § Will-be-replaced-by : `cssl_log::AuditEmit::record_*`.
///
/// § ROLE in D161 : the log shim ensures `AuditEmit` calls do NOT leak
/// wallclock. In Strict mode, every recorded entry has a logical-frame-N
/// timestamp instead of a wallclock-derived one.
#[derive(Debug, Clone, Default)]
pub struct LogShim {
    pub records: Vec<LogRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub frame_n: FrameN,
    pub sub_phase: SubPhase,
    pub message_hash: [u8; 8],
}

impl LogShim {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Record a log entry. In Strict mode, the timestamp is logical-frame-N.
    /// In Lenient mode, the shim still records the (frame_n, sub_phase) it
    /// was given — it's the engine-side caller's responsibility to fill
    /// those values from wallclock OR strict-clock per its mode.
    pub fn record(
        &mut self,
        _mode: DeterminismMode,
        frame_n: FrameN,
        sub_phase: SubPhase,
        message: &str,
    ) {
        let h = blake3::hash(message.as_bytes());
        let mut message_hash = [0u8; 8];
        message_hash.copy_from_slice(&h.as_bytes()[0..8]);
        self.records.push(LogRecord {
            frame_n,
            sub_phase,
            message_hash,
        });
    }

    /// Sealed-bytes for replay-comparison.
    #[must_use]
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.records.len() * 17);
        for r in &self.records {
            buf.extend_from_slice(&r.frame_n.to_le_bytes());
            buf.push(r.sub_phase.index());
            buf.extend_from_slice(&r.message_hash);
        }
        buf
    }
}

/// Mock surface for `cssl-spec-coverage` (D160).
///
/// § Will-be-replaced-by : `cssl_spec_coverage::{SpecAnchor, SpecCoverageRegistry}`.
///
/// § ROLE in D161 : the spec-coverage shim asserts that the citing-metrics
/// recorded under Strict mode are deterministic-attributable. The
/// real `SpecCoverageRegistry::metric_to_spec_anchor` lookup is a pure
/// function — it requires no mode-awareness — but we surface the shim
/// here so that this crate's tests can verify the registry-lookup is
/// deterministic in both modes.
#[derive(Debug, Clone, Default)]
pub struct SpecCoverageShim {
    pub anchors: Vec<SpecAnchorMock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecAnchorMock {
    pub spec_section: &'static str,
    pub citing_metric: &'static str,
}

impl SpecCoverageShim {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            anchors: Vec::new(),
        }
    }

    pub fn register(&mut self, anchor: SpecAnchorMock) {
        self.anchors.push(anchor);
    }

    /// Lookup the spec-section that cites a given metric. Pure-fn ; no
    /// wallclock.
    #[must_use]
    pub fn metric_to_spec_section(&self, metric_name: &str) -> Option<&'static str> {
        self.anchors
            .iter()
            .find(|a| a.citing_metric == metric_name)
            .map(|a| a.spec_section)
    }

    /// Sealed-bytes for replay-comparison.
    #[must_use]
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        for a in &self.anchors {
            buf.extend_from_slice(a.spec_section.as_bytes());
            buf.push(0); // separator
            buf.extend_from_slice(a.citing_metric.as_bytes());
            buf.push(0);
        }
        buf
    }
}

/// Trait the REAL `cssl-metrics` types will implement to consult
/// [`DeterminismMode`] before recording.
///
/// § ROLE : when D157 lands, every `Counter` / `Gauge` / `Histogram` /
/// `Timer` instance will impl `StrictAware` so that the recording-path
/// can branch on mode. In `Strict`, the path goes through the replay-log
/// (via [`ReplayLogIntegration`]) instead of the live ring.
pub trait StrictAware {
    /// Read the current determinism mode of the metric instance.
    fn determinism_mode(&self) -> DeterminismMode;

    /// Whether wallclock reads are permitted at the current mode.
    fn permits_wallclock(&self) -> bool {
        self.determinism_mode().permits_wallclock()
    }
}

/// Trait the REAL `cssl-metrics` types will implement to plug into the
/// replay-log when in `Strict` mode.
pub trait ReplayLogIntegration: StrictAware {
    /// Append a single canonical metric-event to the active replay-log.
    /// Returns `true` if the append happened (Strict mode), `false`
    /// if skipped (Lenient mode — recording goes to the live ring instead).
    fn append_to_replay_log(&self, log: &mut ReplayLog, ev: MetricEvent) -> bool {
        if self.determinism_mode().engages_replay_log() {
            let _ = log.append(ev);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_metrics_shim_strict_records_to_log() {
        let mut shim = MetricsShim::new();
        let mut log = ReplayLog::new();
        let ctx = RecordContext::new(0, SubPhase::Collapse, 42, 0xAA);
        shim.counter_inc(&mut log, DeterminismMode::strict_with_seed(0), ctx, 1);
        assert_eq!(log.len(), 1);
        assert_eq!(shim.record_count, 1);
    }

    #[test]
    fn t_metrics_shim_lenient_skips_log() {
        let mut shim = MetricsShim::new();
        let mut log = ReplayLog::new();
        let ctx = RecordContext::new(0, SubPhase::Collapse, 42, 0xAA);
        shim.counter_inc(&mut log, DeterminismMode::Lenient, ctx, 1);
        assert_eq!(log.len(), 0);
        // Shim still records that the call happened.
        assert_eq!(shim.record_count, 1);
    }

    #[test]
    fn t_metrics_shim_gauge_set_strict() {
        let mut shim = MetricsShim::new();
        let mut log = ReplayLog::new();
        let ctx = RecordContext::new(1, SubPhase::Propagate, 7, 0);
        shim.gauge_set(
            &mut log,
            DeterminismMode::strict_with_seed(0),
            ctx,
            std::f64::consts::PI,
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log.events()[0].kind, MetricEventKind::GaugeSet);
    }

    #[test]
    fn t_metrics_shim_histogram_record_strict() {
        let mut shim = MetricsShim::new();
        let mut log = ReplayLog::new();
        let ctx = RecordContext::new(2, SubPhase::Compose, 8, 0);
        shim.histogram_record(&mut log, DeterminismMode::strict_with_seed(0), ctx, 1.5);
        assert_eq!(log.events()[0].kind, MetricEventKind::HistogramRecord);
    }

    #[test]
    fn t_metrics_shim_timer_record_ns_strict() {
        let mut shim = MetricsShim::new();
        let mut log = ReplayLog::new();
        let ctx = RecordContext::new(3, SubPhase::Cohomology, 9, 0);
        shim.timer_record_ns(
            &mut log,
            DeterminismMode::strict_with_seed(0),
            ctx,
            123_456_789,
        );
        assert_eq!(log.events()[0].kind, MetricEventKind::TimerRecordNs);
        assert_eq!(log.events()[0].value.as_u64(), 123_456_789);
    }

    #[test]
    fn t_log_shim_records_message_hash() {
        let mut shim = LogShim::new();
        shim.record(
            DeterminismMode::strict_with_seed(0),
            5,
            SubPhase::Agency,
            "test message",
        );
        assert_eq!(shim.records.len(), 1);
        assert_eq!(shim.records[0].frame_n, 5);
        assert_eq!(shim.records[0].sub_phase, SubPhase::Agency);
    }

    #[test]
    fn t_log_shim_snapshot_bytes_deterministic() {
        let mut a = LogShim::new();
        let mut b = LogShim::new();
        for i in 0..3 {
            a.record(
                DeterminismMode::strict_with_seed(0),
                i,
                SubPhase::Collapse,
                "x",
            );
            b.record(
                DeterminismMode::strict_with_seed(0),
                i,
                SubPhase::Collapse,
                "x",
            );
        }
        assert_eq!(a.snapshot_bytes(), b.snapshot_bytes());
    }

    #[test]
    fn t_spec_coverage_shim_register_and_lookup() {
        let mut shim = SpecCoverageShim::new();
        shim.register(SpecAnchorMock {
            spec_section: "§ V phase-COLLAPSE",
            citing_metric: "omega_step.phase_time_ns",
        });
        assert_eq!(
            shim.metric_to_spec_section("omega_step.phase_time_ns"),
            Some("§ V phase-COLLAPSE")
        );
        assert_eq!(shim.metric_to_spec_section("nonexistent"), None);
    }

    #[test]
    fn t_spec_coverage_shim_snapshot_bytes_deterministic() {
        let mut a = SpecCoverageShim::new();
        let mut b = SpecCoverageShim::new();
        for _ in 0..3 {
            a.register(SpecAnchorMock {
                spec_section: "§ I",
                citing_metric: "engine.frame_n",
            });
            b.register(SpecAnchorMock {
                spec_section: "§ I",
                citing_metric: "engine.frame_n",
            });
        }
        assert_eq!(a.snapshot_bytes(), b.snapshot_bytes());
    }

    #[test]
    fn t_strict_aware_trait_via_struct() {
        struct M(DeterminismMode);
        impl StrictAware for M {
            fn determinism_mode(&self) -> DeterminismMode {
                self.0
            }
        }
        impl ReplayLogIntegration for M {}
        let strict = M(DeterminismMode::strict_with_seed(0));
        assert!(!strict.permits_wallclock());
        let lenient = M(DeterminismMode::Lenient);
        assert!(lenient.permits_wallclock());
    }

    #[test]
    fn t_replay_log_integration_default_impl_strict_appends() {
        struct M(DeterminismMode);
        impl StrictAware for M {
            fn determinism_mode(&self) -> DeterminismMode {
                self.0
            }
        }
        impl ReplayLogIntegration for M {}
        let m = M(DeterminismMode::strict_with_seed(0));
        let mut log = ReplayLog::new();
        let ev = MetricEvent {
            frame_n: 0,
            sub_phase_index: 0,
            kind: MetricEventKind::CounterIncBy,
            metric_id: 0,
            value: MetricValue::from_u64(1),
            tag_hash: 0,
        };
        let appended = m.append_to_replay_log(&mut log, ev);
        assert!(appended);
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn t_replay_log_integration_default_impl_lenient_skips() {
        struct M(DeterminismMode);
        impl StrictAware for M {
            fn determinism_mode(&self) -> DeterminismMode {
                self.0
            }
        }
        impl ReplayLogIntegration for M {}
        let m = M(DeterminismMode::Lenient);
        let mut log = ReplayLog::new();
        let ev = MetricEvent {
            frame_n: 0,
            sub_phase_index: 0,
            kind: MetricEventKind::CounterIncBy,
            metric_id: 0,
            value: MetricValue::from_u64(1),
            tag_hash: 0,
        };
        let appended = m.append_to_replay_log(&mut log, ev);
        assert!(!appended);
        assert_eq!(log.len(), 0);
    }
}
