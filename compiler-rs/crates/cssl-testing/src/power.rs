//! Power regression oracle (`@power_bench`) — R18 Level-Zero sysman backed.
//!
//! § SPEC    : `specs/23_TESTING.csl` § oracle-modes • power-regression +
//!             `specs/22_TELEMETRY.csl` `{Telemetry<Power>}` scope.
//! § BACKING : `zesPowerGetEnergyCounter` via `cssl-host-level-zero` sysman.
//! § POLICY  : rolling-median 3-of-5 runs; >5% over-baseline → CI-fail.
//! § STATUS  : T11 stub — implementation pending.

/// Config for the `@power_bench` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Identifier for baseline lookup under `.perf-baseline/<id>/power.json`.
    pub bench_id: String,
    /// Number of runs per measurement (rolling-median 3-of-5 per §§ 23).
    pub runs: u32,
    /// Regression threshold (fractional), default 0.05 = ±5%.
    pub regression_threshold: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bench_id: String::new(),
            runs: 5,
            regression_threshold: 0.05,
        }
    }
}

/// Measured power sample — units per §§ 22 ring-buffer schema.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PowerSample {
    /// Total energy consumed over the run.
    pub total_joules: f64,
    /// Peak instantaneous power.
    pub peak_watts: f64,
    /// Average power.
    pub avg_watts: f64,
}

/// Outcome of running the `@power_bench` oracle.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11 (requires `cssl-host-level-zero`).
    Stage0Unimplemented,
    /// Power stayed within tolerance.
    Ok {
        measured: PowerSample,
        baseline: PowerSample,
    },
    /// Power regressed beyond `regression_threshold`.
    Regressed {
        measured: PowerSample,
        baseline: PowerSample,
        delta_pct: f64,
    },
    /// Sysman unavailable on this platform (non-Intel).
    SysmanUnavailable,
}

/// Dispatcher trait for `@power_bench` oracle.
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
