//! Sampling-discipline (deterministic decimation) per § II.5.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.5.
//!
//! § DISCIPLINE
//!   - `Always` is the default (every record commits).
//!   - `OneIn(N)` decimates to 1-of-N via a deterministic predicate
//!     `should_sample(frame_n, tag_hash) = (frame_n + tag_hash) % N == 0`.
//!     This is replay-strict-safe : the same `frame_n` + `tag_hash` always
//!     produces the same answer.
//!   - `BurstThenDecimate { burst, then_one_in }` records the first `burst`
//!     events unconditionally, then decimates the remainder via OneIn.
//!   - `Adaptive { target_overhead_pct }` allows runtime auto-tune ; FORBIDDEN
//!     under feature `replay-strict` (compile-time-rejected via
//!     [`SamplingDiscipline::strict_mode_check`]).

use crate::error::{MetricError, MetricResult};

/// Sampling-discipline assignable to a metric.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SamplingDiscipline {
    /// 1-of-1 (default ; record every event).
    Always,
    /// 1-of-N decimation. `n=0` is treated as `Always` to avoid panic-on-mod-by-zero.
    OneIn(u32),
    /// Record `burst` first then 1-of-N.
    BurstThenDecimate {
        /// Initial unconditional-record count.
        burst: u32,
        /// Decimation rate after burst is exhausted.
        then_one_in: u32,
    },
    /// Adaptive — auto-tune to a target-overhead percentage. NOT replay-safe.
    Adaptive {
        /// Target overhead-budget (percent).
        target_overhead_pct: f32,
    },
}

impl SamplingDiscipline {
    /// Returns `Err(MetricError::AdaptiveUnderStrict)` if Adaptive is used while
    /// feature `replay-strict` is enabled.
    ///
    /// # Errors
    /// Returns [`MetricError::AdaptiveUnderStrict`] when the discipline is
    /// `Adaptive` AND the `replay-strict` feature is on.
    pub fn strict_mode_check(self, metric_name: &'static str) -> MetricResult<()> {
        if cfg!(feature = "replay-strict") {
            if matches!(self, Self::Adaptive { .. }) {
                return Err(MetricError::AdaptiveUnderStrict { name: metric_name });
            }
        }
        Ok(())
    }

    /// Deterministic predicate : should `(frame_n, tag_hash, event_index)`
    /// commit to the underlying counter ?
    ///
    /// § DETERMINISM : depends ONLY on `frame_n + tag_hash`. `event_index` is
    /// used for `BurstThenDecimate` to know whether burst is exhausted.
    /// `Adaptive` returns `true` always in stage-0 (the auto-tune logic
    /// requires per-metric runtime state ; it lives elsewhere).
    #[must_use]
    pub fn should_sample(self, frame_n: u64, tag_hash: u64, event_index: u64) -> bool {
        match self {
            Self::Always => true,
            Self::OneIn(n) => {
                if n == 0 {
                    return true;
                }
                let n = u64::from(n);
                (frame_n.wrapping_add(tag_hash)) % n == 0
            }
            Self::BurstThenDecimate { burst, then_one_in } => {
                if event_index < u64::from(burst) {
                    return true;
                }
                if then_one_in == 0 {
                    return true;
                }
                let n = u64::from(then_one_in);
                (frame_n.wrapping_add(tag_hash)) % n == 0
            }
            Self::Adaptive { .. } => true,
        }
    }

    /// True iff this discipline is replay-safe (Adaptive is the only-not-safe variant).
    #[must_use]
    pub const fn is_replay_safe(self) -> bool {
        !matches!(self, Self::Adaptive { .. })
    }
}

impl Default for SamplingDiscipline {
    fn default() -> Self {
        Self::Always
    }
}

#[cfg(test)]
mod tests {
    use super::SamplingDiscipline;
    use crate::error::MetricError;

    #[test]
    fn default_is_always() {
        assert!(matches!(
            SamplingDiscipline::default(),
            SamplingDiscipline::Always
        ));
    }

    #[test]
    fn always_samples_every_event() {
        let s = SamplingDiscipline::Always;
        for i in 0..10_u64 {
            assert!(s.should_sample(i, 0, i));
        }
    }

    #[test]
    fn one_in_n_basic_deterministic() {
        let s = SamplingDiscipline::OneIn(3);
        // (frame_n + tag_hash=0) % 3 == 0 ⇒ frame_n ∈ {0, 3, 6, 9, ...}
        assert!(s.should_sample(0, 0, 0));
        assert!(!s.should_sample(1, 0, 1));
        assert!(!s.should_sample(2, 0, 2));
        assert!(s.should_sample(3, 0, 3));
        assert!(s.should_sample(6, 0, 6));
    }

    #[test]
    fn one_in_n_with_tag_hash_offset() {
        let s = SamplingDiscipline::OneIn(4);
        // tag_hash=2 ⇒ (frame_n+2) % 4 == 0 ⇒ frame_n ∈ {2, 6, 10, ...}
        assert!(!s.should_sample(0, 2, 0));
        assert!(s.should_sample(2, 2, 0));
        assert!(s.should_sample(6, 2, 0));
    }

    #[test]
    fn one_in_zero_is_always() {
        let s = SamplingDiscipline::OneIn(0);
        for i in 0..10_u64 {
            assert!(s.should_sample(i, 0, i));
        }
    }

    #[test]
    fn one_in_n_deterministic_repeated_calls() {
        let s = SamplingDiscipline::OneIn(7);
        let first = (0..100)
            .map(|i| s.should_sample(i, 11, 0))
            .collect::<Vec<_>>();
        let second = (0..100)
            .map(|i| s.should_sample(i, 11, 0))
            .collect::<Vec<_>>();
        assert_eq!(first, second);
    }

    #[test]
    fn burst_then_decimate_first_n_unconditional() {
        let s = SamplingDiscipline::BurstThenDecimate {
            burst: 5,
            then_one_in: 1000,
        };
        // First 5 events sample regardless of frame_n / tag_hash.
        for i in 0..5_u64 {
            // pick frame_n that wouldn't otherwise sample
            assert!(s.should_sample(1, 0, i));
        }
        // After burst, OneIn(1000) so event 5 with frame_n=1 ⇒ false.
        assert!(!s.should_sample(1, 0, 5));
    }

    #[test]
    fn burst_then_decimate_post_burst_uses_one_in() {
        let s = SamplingDiscipline::BurstThenDecimate {
            burst: 0,
            then_one_in: 3,
        };
        assert!(s.should_sample(0, 0, 0));
        assert!(!s.should_sample(1, 0, 1));
    }

    #[test]
    fn burst_then_decimate_zero_decimation() {
        let s = SamplingDiscipline::BurstThenDecimate {
            burst: 0,
            then_one_in: 0,
        };
        for i in 0..10_u64 {
            assert!(s.should_sample(i, 0, i));
        }
    }

    #[test]
    fn adaptive_samples_every_event_in_stage0() {
        let s = SamplingDiscipline::Adaptive {
            target_overhead_pct: 0.5,
        };
        for i in 0..10_u64 {
            assert!(s.should_sample(i, 0, i));
        }
    }

    #[test]
    fn adaptive_is_not_replay_safe() {
        let s = SamplingDiscipline::Adaptive {
            target_overhead_pct: 0.5,
        };
        assert!(!s.is_replay_safe());
    }

    #[test]
    fn always_one_in_burst_are_replay_safe() {
        assert!(SamplingDiscipline::Always.is_replay_safe());
        assert!(SamplingDiscipline::OneIn(3).is_replay_safe());
        assert!(SamplingDiscipline::BurstThenDecimate {
            burst: 5,
            then_one_in: 3,
        }
        .is_replay_safe());
    }

    #[test]
    fn strict_mode_check_under_non_strict_passes() {
        // Adaptive under non-strict is OK (default builds).
        if !cfg!(feature = "replay-strict") {
            let s = SamplingDiscipline::Adaptive {
                target_overhead_pct: 0.5,
            };
            assert!(s.strict_mode_check("m").is_ok());
        }
    }

    #[test]
    fn strict_mode_check_always_passes_regardless() {
        let s = SamplingDiscipline::Always;
        assert!(s.strict_mode_check("m").is_ok());
    }

    #[cfg(feature = "replay-strict")]
    #[test]
    fn strict_mode_check_refuses_adaptive() {
        let s = SamplingDiscipline::Adaptive {
            target_overhead_pct: 0.5,
        };
        let r = s.strict_mode_check("m");
        assert!(matches!(r, Err(MetricError::AdaptiveUnderStrict { .. })));
    }

    #[test]
    fn frequency_one_in_n_within_bound_property() {
        // For OneIn(N), over 1000 frames with tag_hash=0 we expect ~1000/N samples.
        // Allow ±20% slack.
        for n in [2_u32, 5, 10, 13, 100] {
            let s = SamplingDiscipline::OneIn(n);
            let count = (0..1000_u64).filter(|i| s.should_sample(*i, 0, *i)).count();
            let expected = 1000 / (n as usize);
            let lo = expected.saturating_sub(expected / 5);
            let hi = expected + expected / 5;
            assert!(
                count >= lo && count <= hi.max(1),
                "n={n} expected≈{expected} got={count}"
            );
        }
        // Suppress unused-import warning when replay-strict not on.
        let _ = MetricError::Overflow { name: "" };
    }
}
