//! Replay-determinism oracle (`@replay`) — `{Reversible}` + `{PureDet}` gate.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • replay-tests.
//! § GATE   : T29 (OG9) ship-gate — replay-determinism N=10 CI pass.
//! § ROLE   : record inputs + initial-state + timing-seed, replay N times, assert
//!            all runs bit-exact. Cross-machine replay (different CPU models, same arch) included.
//! § STATUS : T11-phase-2b live (seed-based replay) ; cross-machine + cross-backend
//!            variants deferred — still `Stage0Unimplemented` through the legacy dispatcher.

use crate::property::Lcg;

/// Config for the `@replay` oracle.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Number of replays to execute. Default 10 (OG9 CI-gate).
    pub n: u32,
    /// If `true`, include cross-backend replay (Vulkan × Level-Zero) when `{PureDet}+{Portable}` tagged.
    pub cross_backend: bool,
    /// Seed for the deterministic PRNG driving replays.
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            n: 10,
            cross_backend: true,
            seed: 0xc551_a770_c551_a770_u64,
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

// ─────────────────────────────────────────────────────────────────────────
// § Live replay-runner : seeds a deterministic PRNG N times + compares output.
// ─────────────────────────────────────────────────────────────────────────

/// Run `f` `config.n` times with the same seed. All outputs must be equal
/// (bit-exact for integer / bool types ; use a wrapper for float-ULP-tolerant
/// comparison). Returns `Ok { replays }` if every replay produces the same
/// output as replay-0 ; `Divergence { replay_index, diff_bytes }` at the first
/// mismatch, where `diff_bytes` is the byte-width of the output type as a
/// proxy for divergence-magnitude (real ULP-counting left for float-specific
/// oracles).
///
/// `f` receives a freshly-seeded PRNG and must derive its random inputs only
/// from it — any hidden state (globals, atomics, time) will break determinism.
///
/// § REPLAY-SAFETY
///   Same `config.seed` + same `f` → N identical outputs. This is the T29/OG9
///   ship-gate guarantee : captured inputs from CI must reproduce locally.
pub fn run_replay_deterministic<T, F>(config: &Config, mut f: F) -> Outcome
where
    T: PartialEq,
    F: FnMut(&mut Lcg) -> T,
{
    if config.n == 0 {
        return Outcome::Ok { replays: 0 };
    }
    let mut rng0 = Lcg::new(config.seed);
    let first = f(&mut rng0);
    for k in 1..config.n {
        let mut rng_k = Lcg::new(config.seed);
        let replay = f(&mut rng_k);
        if replay != first {
            return Outcome::Divergence {
                replay_index: k,
                diff_bytes: core::mem::size_of::<T>() as u64,
            };
        }
    }
    Outcome::Ok { replays: config.n }
}

#[cfg(test)]
mod tests {
    use super::{run_replay_deterministic, Config, Dispatcher, Outcome, Stage0Stub};

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

    #[test]
    fn deterministic_prng_reader_replays_bit_exact() {
        let config = Config {
            n: 10,
            cross_backend: false,
            seed: 42,
        };
        // Pure-PRNG-deriven function : always produces same sum for same seed.
        let outcome = run_replay_deterministic(&config, |rng| {
            let mut acc: i64 = 0;
            for _ in 0..100 {
                acc = acc.wrapping_add(rng.gen_i64(-1000, 1000));
            }
            acc
        });
        match outcome {
            Outcome::Ok { replays } => assert_eq!(replays, 10),
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn hidden_state_breaks_determinism() {
        let config = Config {
            n: 5,
            cross_backend: false,
            seed: 7,
        };
        // Non-deterministic function : uses a mutable cell that survives across replays.
        let mut hidden = 0i64;
        let outcome = run_replay_deterministic(&config, |_rng| {
            hidden += 1;
            hidden
        });
        match outcome {
            Outcome::Divergence { replay_index, .. } => {
                assert_eq!(replay_index, 1, "should diverge at first replay");
            }
            other => panic!("expected Divergence, got {other:?}"),
        }
    }

    #[test]
    fn zero_replays_is_ok_with_zero() {
        let config = Config {
            n: 0,
            cross_backend: false,
            seed: 1,
        };
        let outcome = run_replay_deterministic(&config, |rng| rng.gen_i64(0, 10));
        match outcome {
            Outcome::Ok { replays } => assert_eq!(replays, 0),
            other => panic!("expected Ok(0), got {other:?}"),
        }
    }

    #[test]
    fn single_replay_always_ok() {
        let config = Config {
            n: 1,
            cross_backend: false,
            seed: 99,
        };
        // Even a non-deterministic function trivially-passes N=1 replay.
        let mut hidden = 0i64;
        let outcome = run_replay_deterministic(&config, |_rng| {
            hidden += 1;
            hidden
        });
        match outcome {
            Outcome::Ok { replays } => assert_eq!(replays, 1),
            other => panic!("expected Ok(1), got {other:?}"),
        }
    }

    #[test]
    fn divergence_reports_byte_width_of_type() {
        let config = Config {
            n: 3,
            cross_backend: false,
            seed: 11,
        };
        let mut counter = 0u32;
        let outcome = run_replay_deterministic(&config, |_rng| {
            counter += 1;
            counter // u32 = 4 bytes
        });
        match outcome {
            Outcome::Divergence { diff_bytes, .. } => {
                assert_eq!(diff_bytes, 4, "u32 is 4 bytes");
            }
            other => panic!("expected Divergence, got {other:?}"),
        }
    }

    #[test]
    fn different_seeds_still_replay_deterministically() {
        // Same seed across runs + pure-PRNG-function = identical output.
        let mut results = Vec::new();
        for seed in [1, 2, 3, 4, 5] {
            let config = Config {
                n: 5,
                cross_backend: false,
                seed,
            };
            let outcome = run_replay_deterministic(&config, |rng| rng.gen_i64(-50, 50));
            results.push(outcome);
        }
        assert!(results.iter().all(|o| matches!(o, Outcome::Ok { .. })));
    }
}
