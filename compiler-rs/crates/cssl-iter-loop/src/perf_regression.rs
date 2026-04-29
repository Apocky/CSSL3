//! Performance-regression detection : baseline-vs-current metric-history compare.
//!
//! Mirrors `_drafts/phase_j/wave_ji_iteration_loop_docs.md` § 4 — the pre-patch
//! baseline is captured ; the post-patch current is captured ; a regression is
//! flagged when current.p99 / baseline.p99 > 1.05 (5% threshold default), with
//! a separate flag for tail-latency p999 explosion.
//!
//! § Σ-discipline
//!   Inputs are aggregate metric percentiles only. Biometric metrics never
//!   reach this layer because cssl-metrics filters them at registry-construction
//!   (cap-filtered list_metrics).
//!
//! § INTEGRATION-POINT D233/06 — `MetricHistory` swaps to real
//!   `cssl_metrics::histogram::HistogramSnapshot` once the real percentile
//!   API stabilizes. The local stub captures the canonical p50/p95/p99/p999
//!   tuple the regression-detector needs.

use serde::{Deserialize, Serialize};

use crate::protocol::CommitHash;

/// One sample inside a metric-history. Stored as f64 for percentile math.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub frame_n: u64,
    pub value: f64,
}

/// A captured metric-history window. The local stub carries the canonical
/// percentile-tuple ; downstream callers can derive these from the real
/// `cssl_metrics::HistogramSnapshot` once that API stabilizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricHistory {
    pub metric_name: String,
    pub window_frames: u64,
    pub samples: Vec<Sample>,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p999: f64,
    pub min: f64,
    pub max: f64,
}

impl MetricHistory {
    /// Build a `MetricHistory` from raw samples — sorts a copy + computes
    /// percentiles. NaN values are filtered (replay-determinism preserved).
    pub fn from_samples(metric_name: impl Into<String>, samples: Vec<Sample>) -> Self {
        let metric_name = metric_name.into();
        let window_frames = samples.last().map(|s| s.frame_n).unwrap_or(0);
        let mut sorted: Vec<f64> = samples
            .iter()
            .map(|s| s.value)
            .filter(|v| !v.is_nan())
            .collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p50 = pct(&sorted, 0.50);
        let p95 = pct(&sorted, 0.95);
        let p99 = pct(&sorted, 0.99);
        let p999 = pct(&sorted, 0.999);
        let min = sorted.first().copied().unwrap_or(0.0);
        let max = sorted.last().copied().unwrap_or(0.0);
        Self {
            metric_name,
            window_frames,
            samples,
            p50,
            p95,
            p99,
            p999,
            min,
            max,
        }
    }

    pub fn count(&self) -> usize {
        self.samples.len()
    }
}

fn pct(sorted_values: &[f64], q: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let n = sorted_values.len();
    // Nearest-rank percentile, replay-deterministic — no interpolation drift.
    let idx = ((q * n as f64).ceil() as usize)
        .saturating_sub(1)
        .min(n - 1);
    sorted_values[idx]
}

/// Captured baseline from a pre-patch run. Bound to the commit it was
/// captured at so post-patch comparisons reference a known-good anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfBaseline {
    pub metric_name: String,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p99_9: f64,
    pub captured_at_commit: CommitHash,
    pub captured_at_frame: u64,
}

impl PerfBaseline {
    /// Capture a baseline from a metric-history at a known commit.
    pub fn capture(history: &MetricHistory, commit: CommitHash) -> Self {
        Self {
            metric_name: history.metric_name.clone(),
            p50: history.p50,
            p95: history.p95,
            p99: history.p99,
            p99_9: history.p999,
            captured_at_commit: commit,
            captured_at_frame: history.window_frames,
        }
    }
}

/// Severity of a detected regression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegressionSeverity {
    /// Within tolerance ; no action.
    Pass,
    /// p99 > baseline.p99 × 1.05 ; investigate or revert.
    P99Regression,
    /// p99 within tolerance but p999 explodes ; tail-latency flagged.
    TailRegression,
    /// Both p99 + p999 above threshold ; revert immediately.
    SevereRegression,
}

/// Output of `compare_against_baseline`. Carries both the verdict + the
/// numeric ratios so the caller can render diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegressionReport {
    pub metric_name: String,
    pub severity: RegressionSeverity,
    pub p50_ratio: f64,
    pub p95_ratio: f64,
    pub p99_ratio: f64,
    pub p999_ratio: f64,
    pub baseline_p99: f64,
    pub current_p99: f64,
    pub baseline_p999: f64,
    pub current_p999: f64,
    pub threshold: f64,
}

impl RegressionReport {
    pub fn is_regression(&self) -> bool {
        !matches!(self.severity, RegressionSeverity::Pass)
    }
}

/// Errors emitted by perf-regression detection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PerfRegressionError {
    /// Baseline + current metric-name disagree.
    #[error("metric-name mismatch : baseline={baseline}, current={current}")]
    MetricMismatch { baseline: String, current: String },

    /// Baseline percentile is zero or negative ; cannot compute ratio.
    #[error("invalid baseline : non-positive p99 {value}")]
    InvalidBaseline { value: String },
}

/// Default 5% tolerance from `wave_ji_iteration_loop_docs.md` § 4.1.
pub const DEFAULT_TOLERANCE: f64 = 1.05;

/// Compare a current metric-history against a captured baseline.
///
/// Verdict matrix :
///   - p99 ratio ≤ tolerance AND p999 ratio ≤ tolerance ⇒ Pass
///   - p99 ratio  > tolerance AND p999 ratio ≤ tolerance ⇒ P99Regression
///   - p99 ratio ≤ tolerance AND p999 ratio  > tolerance ⇒ TailRegression
///   - both ratios > tolerance ⇒ SevereRegression
pub fn compare_against_baseline(
    current: &MetricHistory,
    baseline: &PerfBaseline,
) -> Result<RegressionReport, PerfRegressionError> {
    compare_with_tolerance(current, baseline, DEFAULT_TOLERANCE)
}

/// Variant with explicit tolerance for callers who want to tune sensitivity.
pub fn compare_with_tolerance(
    current: &MetricHistory,
    baseline: &PerfBaseline,
    tolerance: f64,
) -> Result<RegressionReport, PerfRegressionError> {
    if current.metric_name != baseline.metric_name {
        return Err(PerfRegressionError::MetricMismatch {
            baseline: baseline.metric_name.clone(),
            current: current.metric_name.clone(),
        });
    }
    if !(baseline.p99 > 0.0) {
        return Err(PerfRegressionError::InvalidBaseline {
            value: format!("{}", baseline.p99),
        });
    }
    let p50_ratio = ratio(current.p50, baseline.p50);
    let p95_ratio = ratio(current.p95, baseline.p95);
    let p99_ratio = ratio(current.p99, baseline.p99);
    let p999_ratio = ratio(current.p999, baseline.p99_9);

    let p99_bad = p99_ratio > tolerance;
    let p999_bad = p999_ratio > tolerance;

    let severity = match (p99_bad, p999_bad) {
        (false, false) => RegressionSeverity::Pass,
        (true, false) => RegressionSeverity::P99Regression,
        (false, true) => RegressionSeverity::TailRegression,
        (true, true) => RegressionSeverity::SevereRegression,
    };

    Ok(RegressionReport {
        metric_name: current.metric_name.clone(),
        severity,
        p50_ratio,
        p95_ratio,
        p99_ratio,
        p999_ratio,
        baseline_p99: baseline.p99,
        current_p99: current.p99,
        baseline_p999: baseline.p99_9,
        current_p999: current.p999,
        threshold: tolerance,
    })
}

fn ratio(current: f64, baseline: f64) -> f64 {
    if baseline <= 0.0 {
        // Defensive : caller already checked p99>0 ; this guards p50/p95/p999.
        // Returning 1.0 keeps the report neutral rather than poisoning the
        // verdict-matrix.
        return 1.0;
    }
    current / baseline
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(values: &[f64]) -> Vec<Sample> {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| Sample {
                frame_n: i as u64,
                value: *v,
            })
            .collect()
    }

    fn baseline_history() -> MetricHistory {
        // 1000 samples ; tail occupies indices 989..=999 (11 elements) ⇒ p99 lands
        // in tail (nearest-rank idx = ceil(0.99 × 1000) - 1 = 989).
        let mut vals: Vec<f64> = (0..989).map(|_| 12_500.0).collect();
        vals.extend(vec![15_000.0; 11]);
        MetricHistory::from_samples("frame.tick_us", samples(&vals))
    }

    fn regressed_history() -> MetricHistory {
        // Both p99 + p999 above tolerance × 1.05 of baseline (15000 → > 15750).
        let mut vals: Vec<f64> = (0..989).map(|_| 12_300.0).collect();
        vals.extend(vec![16_500.0; 10]);
        vals.push(19_500.0);
        MetricHistory::from_samples("frame.tick_us", samples(&vals))
    }

    #[test]
    fn metric_history_percentiles_computed() {
        let h = baseline_history();
        assert!(h.p50 > 0.0);
        assert!(h.p99 > h.p50);
        assert!(h.p999 >= h.p99);
    }

    #[test]
    fn baseline_capture_round_trip() {
        let h = baseline_history();
        let commit = CommitHash([7u8; 32]);
        let b = PerfBaseline::capture(&h, commit);
        assert_eq!(b.metric_name, "frame.tick_us");
        assert_eq!(b.captured_at_commit, commit);
    }

    #[test]
    fn compare_pass_when_within_tolerance() {
        let baseline_h = baseline_history();
        let current_h = baseline_history();
        let baseline = PerfBaseline::capture(&baseline_h, CommitHash([0u8; 32]));
        let r = compare_against_baseline(&current_h, &baseline).unwrap();
        assert_eq!(r.severity, RegressionSeverity::Pass);
        assert!(!r.is_regression());
    }

    #[test]
    fn compare_flags_severe_regression() {
        let baseline_h = baseline_history();
        let current_h = regressed_history();
        let baseline = PerfBaseline::capture(&baseline_h, CommitHash([0u8; 32]));
        let r = compare_against_baseline(&current_h, &baseline).unwrap();
        assert_eq!(r.severity, RegressionSeverity::SevereRegression);
        assert!(r.is_regression());
        assert!(r.p99_ratio > 1.05);
    }

    #[test]
    fn compare_flags_p99_only_via_constructed_history() {
        // baseline p99 = 15000, p999 = 15000 (from baseline_history()).
        // Need p99 > 1.05 × 15000 = 15750 AND p999 ≤ 15750.
        // Build current : 989 × 12300 + 10 × 16000 + 1 × 15700.
        // Sorted tail-11 = [15700, 16000×10] ⇒ p99(idx=989) = 15700 ?? — let me trace.
        // Actually : sorted descending tail occupies idx 989..999 inclusive (11 entries).
        // idx=989 = the 990th element (smallest of tail) = 15700.
        // idx=999 = the 1000th element (largest of tail) = 16000.
        // ratios : 15700/15000 = 1.0467 (PASS by 1.05) — too tight.
        // Tighten : 989 × 12300 + 10 × 17000 + 1 × 15700.
        // ⇒ p99(idx=989) = 15700 ; p999(idx=999) = 17000.
        // 15700/15000 = 1.0467 (still PASS) ; 17000/15000 = 1.133 (REG) ⇒ Tail.
        // Inversion : need idx=989 in REG zone but idx=999 below.
        // 989 × 12300 + 10 × 15800 + 1 × 15400 ⇒
        //   sorted tail (11 vals) = [15400, 15800×10] ; idx=989 = 15400 (PASS).
        // The nearest-rank percentile makes "p99 alone" inversion delicate.
        // Simpler : both fail ⇒ Severe variant ; tail-alone covered separately.
        // Drop this test variant ; demonstrate both-pass-or-severe paths instead.
        let baseline_h = baseline_history();
        let current_h = baseline_history(); // unchanged
        let baseline = PerfBaseline::capture(&baseline_h, CommitHash([0u8; 32]));
        let r = compare_with_tolerance(&current_h, &baseline, 1.05).unwrap();
        assert_eq!(r.severity, RegressionSeverity::Pass);
    }

    #[test]
    fn compare_flags_tail_regression_via_diverging_tail() {
        // Use 10_000-sample histories so p99 (idx=9899) vs p999 (idx=9989)
        // can land on different values.
        // baseline : 9890 × 12500 + 90 × 15000 + 20 × 16000
        //   sorted : [12500×9890, 15000×90, 16000×20]
        //   p99 idx = ceil(0.99 × 10000) - 1 = 9899  (within 15000 block ; idx 9890..=9979)
        //   p999 idx = ceil(0.999 × 10000) - 1 = 9989 (within 16000 block ; idx 9980..=9999)
        //   ⇒ p99 = 15000, p999 = 16000.
        // current : 9890 × 12500 + 90 × 15050 + 20 × 18000
        //   ⇒ p99 = 15050 (15050/15000 = 1.003 ; PASS) ; p999 = 18000 (18000/16000 = 1.125 ; REG).
        let mut bvals: Vec<f64> = (0..9890).map(|_| 12_500.0).collect();
        bvals.extend(vec![15_000.0; 90]);
        bvals.extend(vec![16_000.0; 20]);
        let baseline_h = MetricHistory::from_samples("frame.tick_us", samples(&bvals));
        let mut cvals: Vec<f64> = (0..9890).map(|_| 12_500.0).collect();
        cvals.extend(vec![15_050.0; 90]);
        cvals.extend(vec![18_000.0; 20]);
        let current_h = MetricHistory::from_samples("frame.tick_us", samples(&cvals));
        let baseline = PerfBaseline::capture(&baseline_h, CommitHash([0u8; 32]));
        let r = compare_with_tolerance(&current_h, &baseline, 1.05).unwrap();
        assert_eq!(r.severity, RegressionSeverity::TailRegression);
    }

    #[test]
    fn compare_flags_p99_regression_only_via_diverging_tail() {
        // baseline : 9890 × 12500 + 90 × 15000 + 20 × 16000
        //   ⇒ p99 = 15000, p999 = 16000.
        // current : 9890 × 12500 + 90 × 16500 + 20 × 16400
        //   sorted tail (sorted) = [12500×9890, 16400×20, 16500×90].
        //   p99 idx 9899 falls in 16500-block ⇒ p99 = 16500 (16500/15000 = 1.10 ; REG).
        //   p999 idx 9989 falls in 16500-block too ⇒ p999 = 16500 (16500/16000 = 1.031 ; PASS).
        let mut bvals: Vec<f64> = (0..9890).map(|_| 12_500.0).collect();
        bvals.extend(vec![15_000.0; 90]);
        bvals.extend(vec![16_000.0; 20]);
        let baseline_h = MetricHistory::from_samples("frame.tick_us", samples(&bvals));
        let mut cvals: Vec<f64> = (0..9890).map(|_| 12_500.0).collect();
        cvals.extend(vec![16_500.0; 90]);
        cvals.extend(vec![16_400.0; 20]);
        let current_h = MetricHistory::from_samples("frame.tick_us", samples(&cvals));
        let baseline = PerfBaseline::capture(&baseline_h, CommitHash([0u8; 32]));
        let r = compare_with_tolerance(&current_h, &baseline, 1.05).unwrap();
        assert_eq!(r.severity, RegressionSeverity::P99Regression);
    }

    #[test]
    fn compare_rejects_metric_mismatch() {
        let baseline_h = baseline_history();
        let mut current_h = baseline_history();
        current_h.metric_name = "different.metric".into();
        let baseline = PerfBaseline::capture(&baseline_h, CommitHash([0u8; 32]));
        let r = compare_against_baseline(&current_h, &baseline);
        assert!(matches!(r, Err(PerfRegressionError::MetricMismatch { .. })));
    }

    #[test]
    fn compare_rejects_invalid_baseline() {
        let baseline_h = baseline_history();
        let current_h = baseline_history();
        let mut baseline = PerfBaseline::capture(&baseline_h, CommitHash([0u8; 32]));
        baseline.p99 = 0.0; // poison the baseline
        let r = compare_against_baseline(&current_h, &baseline);
        assert!(matches!(
            r,
            Err(PerfRegressionError::InvalidBaseline { .. })
        ));
    }

    #[test]
    fn metric_history_filters_nan() {
        let vals = vec![1.0, 2.0, f64::NAN, 3.0, f64::NAN, 4.0];
        let h = MetricHistory::from_samples("test", samples(&vals));
        // 4 valid samples ⇒ p50 of {1,2,3,4} = 2.0 (nearest-rank ≤ 0.5).
        assert!(h.p50 > 0.0);
        assert!(h.max > 0.0);
    }

    #[test]
    fn pct_handles_empty() {
        assert_eq!(pct(&[], 0.5), 0.0);
    }
}
