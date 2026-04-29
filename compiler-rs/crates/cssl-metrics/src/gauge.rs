//! Gauge — non-monotonic f64 (bit-pattern stored as `AtomicU64`).
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.2.
//!
//! § DESIGN
//!   - Storage = `AtomicU64` holding `f64::to_bits` ; this preserves the exact
//!     IEEE-754 representation across replay-runs (no precision drift).
//!   - `set(NaN)` is REFUSED (§ II.2 N! silent-write) ; returns
//!     [`MetricError::Nan`].
//!   - `set(±Inf)` is refused under default-policy ; constructors may opt-into
//!     a clamp via [`InfPolicy::ClampToFinite`] (rounds to ±MAX_FINITE).
//!   - `inc(delta)` / `dec(delta)` apply via load-modify-store ; if the result
//!     is NaN (e.g., `inf - inf`) or Inf-disallowed, the operation refuses
//!     and the previous value is restored.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{MetricError, MetricResult};
use crate::sampling::SamplingDiscipline;
use crate::tag::{validate_tag_list, TagKey, TagSet, TagVal};

/// Policy for handling `f64::INFINITY` / `f64::NEG_INFINITY` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InfPolicy {
    /// Refuse Inf writes ; return [`MetricError::Inf`]. **Default**.
    #[default]
    Refuse,
    /// Clamp +Inf to `f64::MAX` and -Inf to `f64::MIN` (the largest-finite
    /// representations). Useful for ratio-style gauges that legitimately
    /// reach ±Inf via `0/0` paths.
    ClampToFinite,
}

/// Non-monotonic f64 gauge.
#[derive(Debug)]
pub struct Gauge {
    /// Stable metric-name.
    pub name: &'static str,
    /// Storage : `f64::to_bits` packed into u64 for atomicity.
    bits: AtomicU64,
    /// NaN refusal counter.
    nan_refused: AtomicU64,
    /// Inf refusal counter (counts Refuse-policy refusals only).
    inf_refused: AtomicU64,
    /// Tags.
    tags: TagSet,
    /// Sampling.
    sampling: SamplingDiscipline,
    /// Inf-handling policy.
    inf_policy: InfPolicy,
    /// Schema-id.
    schema_id: u64,
}

impl Gauge {
    /// Construct with no tags ; default Inf-policy = Refuse.
    ///
    /// # Errors
    /// Inherits per [`Gauge::new_with`] (here unreachable since defaults are clean).
    pub fn new(name: &'static str) -> MetricResult<Self> {
        Self::new_with(name, &[], SamplingDiscipline::Always, InfPolicy::Refuse)
    }

    /// Construct with explicit tags + sampling + Inf policy.
    ///
    /// # Errors
    /// Same as [`crate::Counter::new_with`].
    pub fn new_with(
        name: &'static str,
        tags: &[(TagKey, TagVal)],
        sampling: SamplingDiscipline,
        inf_policy: InfPolicy,
    ) -> MetricResult<Self> {
        validate_tag_list(name, tags)?;
        sampling.strict_mode_check(name)?;
        let schema_id = crate::counter::compute_schema_id(name, tags);
        let mut tag_set = TagSet::new();
        for (k, v) in tags {
            tag_set.push((*k, *v));
        }
        Ok(Self {
            name,
            bits: AtomicU64::new(0_u64), // 0_u64 → +0.0_f64
            nan_refused: AtomicU64::new(0),
            inf_refused: AtomicU64::new(0),
            tags: tag_set,
            sampling,
            inf_policy,
            schema_id,
        })
    }

    /// Set absolute value.
    ///
    /// # Errors
    /// - [`MetricError::Nan`] on `f64::NAN`
    /// - [`MetricError::Inf`] on `±Inf` if `inf_policy = Refuse`
    pub fn set(&self, v: f64) -> MetricResult<()> {
        #[cfg(feature = "metrics-disabled")]
        {
            let _ = v;
            return Ok(());
        }

        #[cfg(not(feature = "metrics-disabled"))]
        {
            if v.is_nan() {
                self.nan_refused.fetch_add(1, Ordering::Relaxed);
                return Err(MetricError::Nan { name: self.name });
            }
            let stored = if v.is_infinite() {
                match self.inf_policy {
                    InfPolicy::Refuse => {
                        self.inf_refused.fetch_add(1, Ordering::Relaxed);
                        return Err(MetricError::Inf { name: self.name });
                    }
                    InfPolicy::ClampToFinite => {
                        if v > 0.0 {
                            f64::MAX
                        } else {
                            f64::MIN
                        }
                    }
                }
            } else {
                v
            };
            self.bits.store(stored.to_bits(), Ordering::Relaxed);
            Ok(())
        }
    }

    /// Increment by `delta`.
    ///
    /// # Errors
    /// Same as [`Gauge::set`] applied to `current + delta`.
    pub fn inc(&self, delta: f64) -> MetricResult<()> {
        let current = self.snapshot();
        self.set(current + delta)
    }

    /// Decrement by `delta`.
    ///
    /// # Errors
    /// Same as [`Gauge::set`] applied to `current - delta`.
    pub fn dec(&self, delta: f64) -> MetricResult<()> {
        let current = self.snapshot();
        self.set(current - delta)
    }

    /// Read current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> f64 {
        f64::from_bits(self.bits.load(Ordering::Relaxed))
    }

    /// Total NaN-refusals observed.
    #[must_use]
    pub fn nan_refused(&self) -> u64 {
        self.nan_refused.load(Ordering::Relaxed)
    }

    /// Total Inf-refusals observed (Refuse-policy only).
    #[must_use]
    pub fn inf_refused(&self) -> u64 {
        self.inf_refused.load(Ordering::Relaxed)
    }

    /// Tag-set.
    #[must_use]
    pub fn tags(&self) -> &TagSet {
        &self.tags
    }

    /// Sampling.
    #[must_use]
    pub fn sampling(&self) -> SamplingDiscipline {
        self.sampling
    }

    /// Inf-handling policy.
    #[must_use]
    pub fn inf_policy(&self) -> InfPolicy {
        self.inf_policy
    }

    /// Schema-id.
    #[must_use]
    pub fn schema_id(&self) -> u64 {
        self.schema_id
    }
}

#[cfg(test)]
mod tests {
    use super::{Gauge, InfPolicy};
    use crate::error::MetricError;
    use crate::sampling::SamplingDiscipline;
    use crate::tag::{TagKey, TagVal};

    #[test]
    fn new_starts_at_zero() {
        let g = Gauge::new("g").unwrap();
        assert_eq!(g.snapshot(), 0.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_basic() {
        let g = Gauge::new("g").unwrap();
        g.set(1.5).unwrap();
        assert_eq!(g.snapshot(), 1.5);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_negative() {
        let g = Gauge::new("g").unwrap();
        g.set(-3.25).unwrap();
        assert_eq!(g.snapshot(), -3.25);
    }

    #[test]
    fn set_zero_explicit() {
        let g = Gauge::new("g").unwrap();
        g.set(0.0).unwrap();
        assert_eq!(g.snapshot(), 0.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_replaces_previous() {
        let g = Gauge::new("g").unwrap();
        g.set(1.0).unwrap();
        g.set(2.0).unwrap();
        g.set(3.0).unwrap();
        assert_eq!(g.snapshot(), 3.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn inc_advances() {
        let g = Gauge::new("g").unwrap();
        g.set(1.0).unwrap();
        g.inc(2.5).unwrap();
        assert_eq!(g.snapshot(), 3.5);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn dec_reduces() {
        let g = Gauge::new("g").unwrap();
        g.set(10.0).unwrap();
        g.dec(2.5).unwrap();
        assert_eq!(g.snapshot(), 7.5);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_nan_refused() {
        let g = Gauge::new("g").unwrap();
        let r = g.set(f64::NAN);
        assert!(matches!(r, Err(MetricError::Nan { .. })));
        assert_eq!(g.snapshot(), 0.0, "snapshot unchanged after NaN refusal");
        assert_eq!(g.nan_refused(), 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_pos_inf_refused_default() {
        let g = Gauge::new("g").unwrap();
        let r = g.set(f64::INFINITY);
        assert!(matches!(r, Err(MetricError::Inf { .. })));
        assert_eq!(g.snapshot(), 0.0);
        assert_eq!(g.inf_refused(), 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_neg_inf_refused_default() {
        let g = Gauge::new("g").unwrap();
        let r = g.set(f64::NEG_INFINITY);
        assert!(matches!(r, Err(MetricError::Inf { .. })));
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_pos_inf_clamped_when_policy_clamp() {
        let g = Gauge::new_with(
            "g",
            &[],
            SamplingDiscipline::Always,
            InfPolicy::ClampToFinite,
        )
        .unwrap();
        g.set(f64::INFINITY).unwrap();
        assert_eq!(g.snapshot(), f64::MAX);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_neg_inf_clamped_when_policy_clamp() {
        let g = Gauge::new_with(
            "g",
            &[],
            SamplingDiscipline::Always,
            InfPolicy::ClampToFinite,
        )
        .unwrap();
        g.set(f64::NEG_INFINITY).unwrap();
        assert_eq!(g.snapshot(), f64::MIN);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn bit_pattern_roundtrip_basic() {
        let g = Gauge::new("g").unwrap();
        for v in [0.5_f64, 1.0, -1.0, 1e10, -1e-10, std::f64::consts::PI] {
            g.set(v).unwrap();
            assert_eq!(g.snapshot(), v);
        }
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn bit_pattern_roundtrip_subnormal() {
        let g = Gauge::new("g").unwrap();
        let subnormal = f64::MIN_POSITIVE / 2.0;
        g.set(subnormal).unwrap();
        assert_eq!(g.snapshot().to_bits(), subnormal.to_bits());
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn bit_pattern_roundtrip_max() {
        let g = Gauge::new("g").unwrap();
        g.set(f64::MAX).unwrap();
        assert_eq!(g.snapshot(), f64::MAX);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn bit_pattern_roundtrip_min_positive() {
        let g = Gauge::new("g").unwrap();
        g.set(f64::MIN_POSITIVE).unwrap();
        assert_eq!(g.snapshot(), f64::MIN_POSITIVE);
    }

    #[test]
    fn schema_id_stable() {
        let a = Gauge::new("g").unwrap();
        let b = Gauge::new("g").unwrap();
        assert_eq!(a.schema_id(), b.schema_id());
    }

    #[test]
    fn inf_policy_default_is_refuse() {
        let g = Gauge::new("g").unwrap();
        assert_eq!(g.inf_policy(), InfPolicy::Refuse);
    }

    #[test]
    fn new_with_validates_biometric() {
        let r = Gauge::new_with(
            "g",
            &[(TagKey::new("face_id"), TagVal::U64(0))],
            SamplingDiscipline::Always,
            InfPolicy::Refuse,
        );
        assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
    }

    #[test]
    fn tags_visible() {
        let g = Gauge::new_with(
            "g",
            &[(TagKey::new("mode"), TagVal::Static("60"))],
            SamplingDiscipline::Always,
            InfPolicy::Refuse,
        )
        .unwrap();
        assert_eq!(g.tags().len(), 1);
    }
}
