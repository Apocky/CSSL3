//! Benchmark oracle (`@bench`) — baseline-tracked performance measurement.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • bench + baseline-management.
//! § ROLE   : per-bench median of 10 runs on reference hardware, stored in
//!            `.perf-baseline/<bench-id>/*.json`. CI regression threshold ±10%.
//! § STATUS : T11-phase-2b live (timing-harness + median computation + threshold
//!            check). Baseline-file I/O uses a minimal plain-text format ;
//!            full JSON schema + multi-dimensional statistics (p50 + p95 + p99)
//!            deferred to T11-phase-2c.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Config for the `@bench` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Identifier used to locate `.perf-baseline/<id>/*.json`.
    pub bench_id: String,
    /// Number of measurement runs per invocation.
    pub runs: u32,
    /// Regression threshold as a fractional tolerance (e.g. 0.10 = ±10%).
    pub regression_threshold: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bench_id: String::new(),
            runs: 10,
            regression_threshold: 0.10,
        }
    }
}

/// Outcome of running the `@bench` oracle.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11.
    Stage0Unimplemented,
    /// Bench ran; median within tolerance.
    Ok { median_ns: u64, baseline_ns: u64 },
    /// Bench regressed beyond `regression_threshold`.
    Regressed {
        median_ns: u64,
        baseline_ns: u64,
        delta_pct: f64,
    },
    /// Baseline missing — first-run (writes new baseline under `--update-baseline`).
    NoBaseline { median_ns: u64 },
}

/// Dispatcher trait for `@bench` oracle.
pub trait Dispatcher {
    fn run(&self, config: &Config) -> Outcome;
}

/// Stage0 stub dispatcher — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Dispatcher for Stage0Stub {
    fn run(&self, _config: &Config) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Live timing-harness : runs F config.runs times, computes median,
//   compares to baseline file, emits Regressed / Ok / NoBaseline.
// ─────────────────────────────────────────────────────────────────────────

/// Measure `f` over `runs` repetitions, compute the median latency, compare
/// to a baseline file at `baseline_root/<bench_id>/latest.txt`. The baseline
/// file contains a single unsigned integer : the median nanoseconds from a
/// prior run (stable-hardware assumption per `specs/23` § baseline-mgmt).
///
/// `baseline_root` : e.g., `.perf-baseline/` relative to the crate root.
/// `f` receives the current run-index (0-based) in case the workload wants
/// to vary input. If it doesn't, ignore the argument.
///
/// Returns :
/// - `NoBaseline { median_ns }` if the baseline file is missing (first-run)
/// - `Ok { median_ns, baseline_ns }` if within `threshold`
/// - `Regressed { median_ns, baseline_ns, delta_pct }` if above
pub fn run_bench_vs_baseline<F>(config: &Config, baseline_root: &Path, mut f: F) -> Outcome
where
    F: FnMut(u32),
{
    let samples = run_and_collect_samples(config.runs, &mut f);
    let median_ns = median_ns(&samples);

    let baseline_path = baseline_path(baseline_root, &config.bench_id);
    let Some(baseline_ns) = read_baseline(&baseline_path) else {
        return Outcome::NoBaseline { median_ns };
    };

    classify(median_ns, baseline_ns, config.regression_threshold)
}

/// Pure-data classifier : decide Ok/Regressed given a measured median + baseline.
/// Extracted for testability (bench-regressions on CI) without needing to run
/// a real workload.
#[must_use]
#[allow(clippy::cast_precision_loss)] // nanosecond counts fit in f64 mantissa for realistic benches
pub fn classify(median_ns: u64, baseline_ns: u64, threshold: f64) -> Outcome {
    if baseline_ns == 0 {
        // Avoid division-by-zero ; absence of baseline treated as first-run.
        return Outcome::NoBaseline { median_ns };
    }
    let delta = median_ns as f64 - baseline_ns as f64;
    let delta_pct = delta / baseline_ns as f64;
    if delta_pct > threshold {
        Outcome::Regressed {
            median_ns,
            baseline_ns,
            delta_pct,
        }
    } else {
        Outcome::Ok {
            median_ns,
            baseline_ns,
        }
    }
}

/// Run `f` `runs` times, returning each execution's wall-clock latency in ns.
fn run_and_collect_samples<F>(runs: u32, f: &mut F) -> Vec<u64>
where
    F: FnMut(u32),
{
    let mut samples = Vec::with_capacity(runs as usize);
    for i in 0..runs {
        let t0 = Instant::now();
        f(i);
        let elapsed: Duration = t0.elapsed();
        samples.push(duration_to_ns(elapsed));
    }
    samples
}

fn duration_to_ns(d: Duration) -> u64 {
    // u128 → u64 saturates for workloads > 584 years ; acceptable.
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

/// Median of a slice. For an even-length slice, returns the lower midpoint
/// (no interpolation) — acceptable for bench-stability and avoids float-eq
/// surprises.
#[must_use]
pub fn median_ns(samples: &[u64]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted: Vec<u64> = samples.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn baseline_path(root: &Path, bench_id: &str) -> PathBuf {
    root.join(bench_id).join("latest.txt")
}

fn read_baseline(path: &Path) -> Option<u64> {
    let bytes = std::fs::read(path).ok()?;
    let s = core::str::from_utf8(&bytes).ok()?;
    s.trim().parse::<u64>().ok()
}

/// Write a new baseline — used by `csslc test --update-baseline`.
///
/// # Errors
/// Propagates any I/O error from `create_dir_all` or `write`.
pub fn update_baseline(root: &Path, bench_id: &str, median_ns: u64) -> std::io::Result<()> {
    let path = baseline_path(root, bench_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, median_ns.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        classify, median_ns, run_bench_vs_baseline, update_baseline, Config, Dispatcher, Outcome,
        Stage0Stub,
    };

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }

    #[test]
    fn median_of_odd_length_returns_middle() {
        assert_eq!(median_ns(&[5, 1, 3]), 3);
        assert_eq!(median_ns(&[10, 20, 30]), 20);
        assert_eq!(median_ns(&[100]), 100);
    }

    #[test]
    fn median_of_even_length_returns_upper_midpoint() {
        assert_eq!(median_ns(&[1, 2, 3, 4]), 3);
        assert_eq!(median_ns(&[10, 20]), 20);
    }

    #[test]
    fn median_of_empty_is_zero() {
        assert_eq!(median_ns(&[]), 0);
    }

    #[test]
    fn classify_within_tolerance_returns_ok() {
        let outcome = classify(105, 100, 0.10);
        match outcome {
            Outcome::Ok {
                median_ns,
                baseline_ns,
            } => {
                assert_eq!(median_ns, 105);
                assert_eq!(baseline_ns, 100);
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn classify_above_tolerance_returns_regressed() {
        let outcome = classify(120, 100, 0.10);
        match outcome {
            Outcome::Regressed {
                median_ns,
                baseline_ns,
                delta_pct,
            } => {
                assert_eq!(median_ns, 120);
                assert_eq!(baseline_ns, 100);
                assert!((delta_pct - 0.20).abs() < 1e-6);
            }
            other => panic!("expected Regressed, got {other:?}"),
        }
    }

    #[test]
    fn classify_below_baseline_is_ok_not_regressed() {
        // Faster-than-baseline (improvement) should NOT fire a regression.
        let outcome = classify(50, 100, 0.10);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn classify_zero_baseline_returns_no_baseline() {
        let outcome = classify(100, 0, 0.10);
        match outcome {
            Outcome::NoBaseline { median_ns } => assert_eq!(median_ns, 100),
            other => panic!("expected NoBaseline, got {other:?}"),
        }
    }

    #[test]
    fn run_bench_with_no_baseline_file_returns_no_baseline() {
        let tmp = std::env::temp_dir().join("cssl-bench-nobaseline");
        let _ = std::fs::remove_dir_all(&tmp);
        let config = Config {
            bench_id: "unknown".to_string(),
            runs: 3,
            regression_threshold: 0.10,
        };
        let outcome = run_bench_vs_baseline(&config, &tmp, |_| {
            // No-op workload ; we only care about the NoBaseline path.
        });
        assert!(matches!(outcome, Outcome::NoBaseline { .. }));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn update_baseline_then_run_reads_it_back() {
        let tmp = std::env::temp_dir().join("cssl-bench-roundtrip");
        let _ = std::fs::remove_dir_all(&tmp);
        update_baseline(&tmp, "my-bench", 1000).expect("write baseline");
        let config = Config {
            bench_id: "my-bench".to_string(),
            runs: 3,
            regression_threshold: 0.50, // generous tolerance to absorb clock jitter
        };
        let outcome = run_bench_vs_baseline(&config, &tmp, |_| {});
        // Either Ok or Regressed depending on how fast the no-op actually is ;
        // both are "not NoBaseline" which is what we're verifying.
        assert!(!matches!(outcome, Outcome::NoBaseline { .. }));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
