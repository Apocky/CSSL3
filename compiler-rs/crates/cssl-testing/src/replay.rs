//! Replay-determinism oracle (`@replay`) — `{Reversible}` + `{PureDet}` gate.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • replay-tests.
//! § GATE   : T29 (OG9) ship-gate — replay-determinism N=10 CI pass.
//! § ROLE   : record inputs + initial-state + timing-seed, replay N times, assert
//!            all runs bit-exact. Cross-machine replay (different CPU models, same arch) included.
//! § STATUS : T11 stub — implementation pending.

/// Config for the `@replay` oracle.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Number of replays to execute. Default 10 (OG9 CI-gate).
    pub n: u32,
    /// If `true`, include cross-backend replay (Vulkan × Level-Zero) when `{PureDet}+{Portable}` tagged.
    pub cross_backend: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            n: 10,
            cross_backend: true,
        }
    }
}

/// Outcome of running the `@replay` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11.
    Stage0Unimplemented,
    /// All N replays produced bit-exact outputs.
    Ok { replays: u32 },
    /// Replay `k` diverged from replay 0.
    Divergence { replay_index: u32, diff_bytes: u64 },
}

/// Dispatcher trait for `@replay` oracle.
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

    #[test]
    fn default_n_equals_10() {
        assert_eq!(Config::default().n, 10);
    }
}
