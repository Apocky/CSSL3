//! Property-based oracle (`@property`) — QuickCheck / Hypothesis lineage.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • property-based.
//! § ROLE   : generators derived from refinement-types (§§ 20) produce well-typed
//!            inputs; shrinking auto-derived from refinement constraints; seeds
//!            deterministic for replay-safety.
//! § STATUS : T11 stub — implementation pending.

/// Config for the `@property` oracle.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Number of generated cases per-run. Default 1000 (§§ 23 "scale"); 10000 in `nightly-extended`.
    pub cases: u32,
    /// Deterministic seed for generator (replay-safe).
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cases: 1000,
            seed: 0xc551_a770_c551_a770_u64,
        }
    }
}

/// Outcome of running the `@property` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11.
    Stage0Unimplemented,
    /// All cases passed.
    Ok { cases_run: u32 },
    /// A counterexample was found and shrunk to the given form.
    Counterexample {
        shrunk_input: String,
        message: String,
    },
}

/// Dispatcher trait for `@property` oracle.
pub trait Dispatcher {
    /// Execute the oracle against the given config.
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
