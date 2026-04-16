//! Benchmark oracle (`@bench`) — baseline-tracked performance measurement.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • bench + baseline-management.
//! § ROLE   : per-bench median of 10 runs on reference hardware, stored in
//!            `.perf-baseline/<bench-id>/*.json`. CI regression threshold ±10%.
//! § STATUS : T11 stub — implementation pending; framework wired to `.perf-baseline/`.

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

#[cfg(test)]
mod tests {
    use super::{Config, Dispatcher, Outcome, Stage0Stub};

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }
}
