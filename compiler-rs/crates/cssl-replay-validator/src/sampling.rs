//! Sampling-discipline for metric recording.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.5 + § VI.4
//!     (LM-2 — Adaptive sampling refused under `Strict`).
//!
//! § DISCIPLINE
//!
//! The `OneIn(N)` decimation is keyed on `frame_n`, NOT wallclock :
//!
//! `should_sample` is `(frame_n + tag_hash) % N == 0`.
//!
//! This makes the decimation schedule a deterministic function of the
//! replay seed + the catalog of tag-hashes — bit-equal across replay
//! runs, no wallclock leak.

use crate::DeterminismMode;
use crate::FrameN;
use thiserror::Error;

/// Sampling-discipline for metric recording.
///
/// § SPEC : § II.5.
///
/// `Adaptive` is intentionally absent from this enum — adaptive sampling
/// CANNOT be expressed in a `Strict` context (LM-2). Callers who want
/// adaptive sampling MUST use `Lenient` mode (and thereby opt out of the
/// H5 contract) — they will use a separate constructor in cssl-metrics
/// (Wave-Jζ-1) that this crate does not expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SamplingDiscipline {
    /// Always record (1-of-1).
    Always,
    /// Record one in every `N` ; deterministic via `frame_n + tag_hash` keying.
    OneIn(OneIn),
    /// First `burst` records always recorded ; thereafter `OneIn(then_one_in)`.
    BurstThenDecimate {
        burst: u32,
        then_one_in: OneIn,
    },
}

/// Validated `OneIn(N)` ; guarantees `N >= 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OneIn(u32);

impl OneIn {
    /// Construct with validation. `n == 0` is refused.
    pub const fn new(n: u32) -> Result<Self, SamplingDisciplineError> {
        if n == 0 {
            return Err(SamplingDisciplineError::ZeroDivisor);
        }
        Ok(Self(n))
    }

    /// Read the divisor.
    #[must_use]
    pub const fn n(&self) -> u32 {
        self.0
    }
}

impl SamplingDiscipline {
    /// Decide whether to sample at this `frame_n` for a metric tagged with
    /// `tag_hash`. Bit-deterministic — no wallclock read.
    ///
    /// § SPEC : § II.5 + slice-prompt LOGICAL-FRAME-N DISCIPLINE.
    #[must_use]
    pub fn should_sample(&self, frame_n: FrameN, tag_hash: u64) -> bool {
        match self {
            Self::Always => true,
            Self::OneIn(one_in) => is_decimated_hit(frame_n, tag_hash, one_in.n()),
            Self::BurstThenDecimate { burst, then_one_in } => {
                if frame_n < FrameN::from(*burst) {
                    true
                } else {
                    is_decimated_hit(frame_n, tag_hash, then_one_in.n())
                }
            }
        }
    }

    /// Construct `Always` (the default-spec `1-of-1`).
    #[must_use]
    pub const fn always() -> Self {
        Self::Always
    }

    /// Construct `OneIn(N)` with validation.
    pub const fn one_in(n: u32) -> Result<Self, SamplingDisciplineError> {
        match OneIn::new(n) {
            Ok(o) => Ok(Self::OneIn(o)),
            Err(e) => Err(e),
        }
    }

    /// Construct `BurstThenDecimate` with validation on the divisor.
    pub const fn burst_then_decimate(
        burst: u32,
        then_one_in: u32,
    ) -> Result<Self, SamplingDisciplineError> {
        match OneIn::new(then_one_in) {
            Ok(o) => Ok(Self::BurstThenDecimate {
                burst,
                then_one_in: o,
            }),
            Err(e) => Err(e),
        }
    }

    /// Whether this discipline is permissible under the given mode.
    /// (`Adaptive` would be refused here ; it is simply absent from this
    /// enum — the type system enforces LM-2.)
    #[must_use]
    pub const fn is_permissible_under(&self, _mode: DeterminismMode) -> bool {
        // All three variants above are deterministic-keyed-on-frame_n ; they
        // are permissible in BOTH modes. Adaptive sampling — which would NOT
        // be permissible under Strict — is not constructible via this enum.
        true
    }
}

impl Default for SamplingDiscipline {
    fn default() -> Self {
        Self::Always
    }
}

/// Errors from sampling-discipline construction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SamplingDisciplineError {
    /// `OneIn(0)` would be a division by zero. Refused.
    #[error("PD0162 — OneIn(N) requires N >= 1 ; division by zero")]
    ZeroDivisor,
}

/// Convenience : check `(frame_n + tag_hash) % N == 0` with overflow safety.
#[must_use]
fn is_decimated_hit(frame_n: FrameN, tag_hash: u64, n: u32) -> bool {
    debug_assert!(n != 0, "OneIn(0) should be unreachable via constructor guard");
    let combined = frame_n.wrapping_add(tag_hash);
    combined % u64::from(n) == 0
}

/// Free-function variant for callers that don't want to materialize a
/// `SamplingDiscipline` ; identical decimation as `OneIn(N).should_sample`.
///
/// § SPEC : § VI.2 — "Sampling : `OneIn(N)` discipline keyed-on `frame_n`
///     (NOT wallclock) ⊗ deterministic"
#[must_use]
pub fn sampling_decision_strict(frame_n: FrameN, tag_hash: u64, n: u32) -> bool {
    if n == 0 {
        // Treat invalid divisor as never-sample (defensive ; primary path
        // is the validated OneIn constructor).
        return false;
    }
    is_decimated_hit(frame_n, tag_hash, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_always_samples_every_frame() {
        let s = SamplingDiscipline::always();
        for f in 0..1_000 {
            assert!(s.should_sample(f, 0xDEAD_BEEF));
        }
    }

    #[test]
    fn t_one_in_n_zero_refused() {
        assert_eq!(OneIn::new(0), Err(SamplingDisciplineError::ZeroDivisor));
    }

    #[test]
    fn t_one_in_2_alternates() {
        let s = SamplingDiscipline::one_in(2).unwrap();
        assert!(s.should_sample(0, 0));
        assert!(!s.should_sample(1, 0));
        assert!(s.should_sample(2, 0));
        assert!(!s.should_sample(3, 0));
    }

    #[test]
    fn t_one_in_3_pattern() {
        let s = SamplingDiscipline::one_in(3).unwrap();
        let pat = (0..6)
            .map(|f| s.should_sample(f, 0))
            .collect::<Vec<_>>();
        assert_eq!(pat, vec![true, false, false, true, false, false]);
    }

    #[test]
    fn t_one_in_keyed_by_tag_hash() {
        let s = SamplingDiscipline::one_in(2).unwrap();
        assert!(s.should_sample(0, 0));
        assert!(s.should_sample(0, 2));
        // tag_hash=1 shifts decimation by one.
        assert!(!s.should_sample(0, 1));
        assert!(s.should_sample(1, 1));
    }

    #[test]
    fn t_burst_then_decimate() {
        let s = SamplingDiscipline::burst_then_decimate(3, 2).unwrap();
        // Burst : 0..3 always-sample.
        for f in 0..3 {
            assert!(s.should_sample(f, 0));
        }
        // After burst : OneIn(2) keyed-on frame_n.
        // frame=3 ⇒ 3 % 2 = 1 → not-hit
        assert!(!s.should_sample(3, 0));
        // frame=4 ⇒ 4 % 2 = 0 → hit
        assert!(s.should_sample(4, 0));
    }

    #[test]
    fn t_burst_then_decimate_zero_divisor_refused() {
        assert_eq!(
            SamplingDiscipline::burst_then_decimate(3, 0),
            Err(SamplingDisciplineError::ZeroDivisor)
        );
    }

    #[test]
    fn t_decimation_deterministic_repeat() {
        let s = SamplingDiscipline::one_in(7).unwrap();
        let a: Vec<_> = (0..100).map(|f| s.should_sample(f, 0xC0FFEE)).collect();
        let b: Vec<_> = (0..100).map(|f| s.should_sample(f, 0xC0FFEE)).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn t_sampling_decision_strict_zero_div_safe() {
        assert!(!sampling_decision_strict(0, 0, 0));
    }

    #[test]
    #[allow(non_snake_case)]
    fn t_sampling_decision_strict_matches_oneIn() {
        let s = SamplingDiscipline::one_in(5).unwrap();
        for f in 0..50 {
            for h in 0..10u64 {
                assert_eq!(s.should_sample(f, h), sampling_decision_strict(f, h, 5));
            }
        }
    }

    #[test]
    fn t_permissible_under_lenient_or_strict() {
        let s = SamplingDiscipline::always();
        assert!(s.is_permissible_under(DeterminismMode::Lenient));
        assert!(s.is_permissible_under(DeterminismMode::strict_with_seed(0)));
    }

    #[test]
    fn t_default_is_always() {
        assert_eq!(SamplingDiscipline::default(), SamplingDiscipline::Always);
    }

    #[test]
    fn t_one_in_n_value_round_trips() {
        let one_in = OneIn::new(13).unwrap();
        assert_eq!(one_in.n(), 13);
    }
}
