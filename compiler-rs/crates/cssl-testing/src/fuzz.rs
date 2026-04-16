//! Fuzz-with-oracle oracle (`@fuzz`) — coverage-guided + SMT oracle.
//!
//! § SPEC    : `specs/23_TESTING.csl` § oracle-modes • fuzz-with-oracles +
//!             `specs/20_SMT.csl` refinement-oracle.
//! § ROLE    : coverage-guided fuzzing (AFL++/libFuzzer-style); inputs constrained by
//!             refinement-types; SMT oracle verifies output-refinement holds.
//! § BUDGET  : 10 min / module in `nightly-extended` per §§ 23 schedule.
//! § STATUS  : T11 stub — implementation pending (blocks on `cssl-smt` + SMT-oracle).

use core::time::Duration;

/// Config for the `@fuzz` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Time budget for fuzzing (§§ 23 default 600s in nightly).
    pub budget: Duration,
    /// If `true`, SMT-oracle verifies refinement on every input.
    pub smt_oracle: bool,
    /// Target throughput per-second (for CI regression on fuzz infrastructure).
    pub min_exec_per_sec: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            budget: Duration::from_secs(600),
            smt_oracle: true,
            min_exec_per_sec: 1000,
        }
    }
}

/// Outcome of running the `@fuzz` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11.
    Stage0Unimplemented,
    /// Fuzzing completed without finding crashes or refinement violations.
    Ok { total_execs: u64 },
    /// Found crashing / UB / refinement-violation input; shrunk form attached.
    Counterexample {
        shrunk_input: String,
        message: String,
    },
}

/// Dispatcher trait for `@fuzz` oracle.
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
