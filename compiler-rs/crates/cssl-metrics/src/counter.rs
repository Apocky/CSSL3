//! Counter — monotonic non-decreasing u64.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1.
//!
//! § DESIGN
//!   - `value` is an `AtomicU64` (Relaxed for monotonic-counter ; per spec
//!     "Relaxed permitted for monotonic-counter-only").
//!   - `inc` / `inc_by` / `add` are saturating-to-`u64::MAX`. The first time
//!     saturation occurs, the counter records [`MetricError::Overflow`] in
//!     its `last_error` field for caller-visibility ; subsequent inc-attempts
//!     remain silent (the saturation is the contract).
//!   - `reset_to(v)` is the only legitimate "set" path : it logs a
//!     RESET-EVENT into the counter's audit-trail (held in `reset_count`)
//!     and replaces the value. Plain `set(v < snapshot)` returns
//!     [`MetricError::CounterDecrement`] without mutating state.
//!   - `metrics-disabled` feature compiles all record-sites to no-ops
//!     (the function-bodies become `Ok(())` early-returns) so production
//!     builds pay zero observable overhead.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{MetricError, MetricResult};
use crate::sampling::SamplingDiscipline;
use crate::tag::{validate_tag_list, TagKey, TagSet, TagVal};

/// Monotonic non-decreasing u64 counter.
#[derive(Debug)]
pub struct Counter {
    /// Stable metric-name (e.g., `"engine.frame_n"`).
    pub name: &'static str,
    /// Current count (saturating at `u64::MAX`).
    value: AtomicU64,
    /// Number of explicit reset-events (via [`Counter::reset_to`]).
    reset_count: AtomicU64,
    /// Number of saturating-overflow events observed.
    overflow_count: AtomicU64,
    /// Tag-set bound at construction (immutable post-build).
    tags: TagSet,
    /// Sampling discipline (replay-strict-checked at construction).
    sampling: SamplingDiscipline,
    /// Schema-id (computed from name+tag-keys at construction).
    schema_id: u64,
}

impl Counter {
    /// Construct a new counter with no tags + Always sampling.
    ///
    /// # Errors
    /// Returns [`MetricError::AdaptiveUnderStrict`] if Adaptive sampling is
    /// supplied while feature `replay-strict` is on — but bare `new` uses
    /// Always so this can only fire from [`Counter::new_with`].
    pub fn new(name: &'static str) -> MetricResult<Self> {
        Self::new_with(name, &[], SamplingDiscipline::Always)
    }

    /// Construct with explicit tags + sampling discipline.
    ///
    /// # Errors
    /// - [`MetricError::TagOverflow`] if `tags.len() > 4`
    /// - [`MetricError::BiometricTagKey`] / [`MetricError::RawPathTagValue`]
    ///   per [`validate_tag_list`]
    /// - [`MetricError::AdaptiveUnderStrict`] if Adaptive used under
    ///   `replay-strict`
    pub fn new_with(
        name: &'static str,
        tags: &[(TagKey, TagVal)],
        sampling: SamplingDiscipline,
    ) -> MetricResult<Self> {
        validate_tag_list(name, tags)?;
        sampling.strict_mode_check(name)?;
        let schema_id = compute_schema_id(name, tags);
        let mut tag_set = TagSet::new();
        for (k, v) in tags {
            tag_set.push((*k, *v));
        }
        Ok(Self {
            name,
            value: AtomicU64::new(0),
            reset_count: AtomicU64::new(0),
            overflow_count: AtomicU64::new(0),
            tags: tag_set,
            sampling,
            schema_id,
        })
    }

    /// Increment by 1 (saturating at `u64::MAX`).
    ///
    /// # Errors
    /// Returns [`MetricError::Overflow`] on saturation (only on the first
    /// overflow ; subsequent inc-attempts return Ok with no state-change).
    pub fn inc(&self) -> MetricResult<()> {
        self.inc_by(1)
    }

    /// Increment by `n` (saturating at `u64::MAX`).
    ///
    /// # Errors
    /// Returns [`MetricError::Overflow`] on the first saturation ; subsequent
    /// calls return Ok with no state-change.
    pub fn inc_by(&self, n: u64) -> MetricResult<()> {
        #[cfg(feature = "metrics-disabled")]
        {
            let _ = n;
            return Ok(());
        }

        #[cfg(not(feature = "metrics-disabled"))]
        {
            let prev = self.value.fetch_add(n, Ordering::Relaxed);
            // Detect saturation using checked_add semantics.
            if prev.checked_add(n).is_none() {
                // Roll back to MAX so the saturation is observable.
                self.value.store(u64::MAX, Ordering::Relaxed);
                let prior_overflow = self.overflow_count.fetch_add(1, Ordering::Relaxed);
                if prior_overflow == 0 {
                    return Err(MetricError::Overflow { name: self.name });
                }
            }
            Ok(())
        }
    }

    /// Set absolute value. **Only-permitted** when `v >= current_snapshot` ;
    /// strict equal is fine (idempotent), strict greater advances the counter.
    /// Strict less returns [`MetricError::CounterDecrement`].
    ///
    /// § DISCIPLINE : for explicit reset-events use [`Counter::reset_to`].
    ///
    /// # Errors
    /// Returns [`MetricError::CounterDecrement`] on a silent decrement attempt.
    pub fn set(&self, v: u64) -> MetricResult<()> {
        #[cfg(feature = "metrics-disabled")]
        {
            let _ = v;
            return Ok(());
        }

        #[cfg(not(feature = "metrics-disabled"))]
        {
            let current = self.value.load(Ordering::Relaxed);
            if v < current {
                return Err(MetricError::CounterDecrement {
                    name: self.name,
                    current,
                    proposed: v,
                });
            }
            self.value.store(v, Ordering::Relaxed);
            Ok(())
        }
    }

    /// Explicit reset to `v` ; logs a RESET-EVENT in the audit-trail.
    /// Always succeeds (the reset itself is the audit-event).
    pub fn reset_to(&self, v: u64) {
        #[cfg(feature = "metrics-disabled")]
        {
            let _ = v;
        }
        #[cfg(not(feature = "metrics-disabled"))]
        {
            self.value.store(v, Ordering::Relaxed);
            self.reset_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Read current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Total reset-events observed.
    #[must_use]
    pub fn reset_count(&self) -> u64 {
        self.reset_count.load(Ordering::Relaxed)
    }

    /// Total saturating-overflow events observed.
    #[must_use]
    pub fn overflow_count(&self) -> u64 {
        self.overflow_count.load(Ordering::Relaxed)
    }

    /// Tag-set (immutable post-construction).
    #[must_use]
    pub fn tags(&self) -> &TagSet {
        &self.tags
    }

    /// Sampling discipline.
    #[must_use]
    pub fn sampling(&self) -> SamplingDiscipline {
        self.sampling
    }

    /// Schema-id (stable across runs ; depends on name + tag-keys).
    #[must_use]
    pub fn schema_id(&self) -> u64 {
        self.schema_id
    }
}

/// Compute a stable schema-id for a `(name, tag_keys)` pair via FNV-1a 64-bit.
///
/// § DETERMINISM : the hash depends only on the static-strings ; replay-runs
/// produce identical schema-ids.
#[must_use]
pub(crate) fn compute_schema_id(name: &str, tags: &[(TagKey, TagVal)]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let prime: u64 = 0x0000_0100_0000_01b3;
    for b in name.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(prime);
    }
    for (k, _) in tags {
        h ^= 0x5e;
        h = h.wrapping_mul(prime);
        for b in k.0.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(prime);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::Counter;
    use crate::error::MetricError;
    use crate::sampling::SamplingDiscipline;
    use crate::tag::{TagKey, TagVal};

    #[test]
    fn new_zero_initial() {
        let c = Counter::new("c").unwrap();
        assert_eq!(c.snapshot(), 0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn inc_increments_by_one() {
        let c = Counter::new("c").unwrap();
        c.inc().unwrap();
        assert_eq!(c.snapshot(), 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn inc_by_n_advances() {
        let c = Counter::new("c").unwrap();
        c.inc_by(5).unwrap();
        assert_eq!(c.snapshot(), 5);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn inc_repeated_monotonic() {
        let c = Counter::new("c").unwrap();
        for _ in 0..100 {
            c.inc().unwrap();
        }
        assert_eq!(c.snapshot(), 100);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_to_higher_works() {
        let c = Counter::new("c").unwrap();
        c.inc_by(10).unwrap();
        c.set(50).unwrap();
        assert_eq!(c.snapshot(), 50);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_to_equal_idempotent() {
        let c = Counter::new("c").unwrap();
        c.inc_by(10).unwrap();
        c.set(10).unwrap();
        assert_eq!(c.snapshot(), 10);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn set_to_lower_refused_with_decrement_error() {
        let c = Counter::new("c").unwrap();
        c.inc_by(10).unwrap();
        let r = c.set(5);
        assert!(matches!(r, Err(MetricError::CounterDecrement { .. })));
        assert_eq!(c.snapshot(), 10, "snapshot unchanged after refusal");
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn overflow_saturates_returns_first_error() {
        let c = Counter::new("c").unwrap();
        c.set(u64::MAX - 5).unwrap();
        c.inc_by(3).unwrap();
        let r = c.inc_by(100);
        assert!(matches!(r, Err(MetricError::Overflow { .. })));
        assert_eq!(c.snapshot(), u64::MAX);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn overflow_count_increments_on_saturation() {
        let c = Counter::new("c").unwrap();
        c.set(u64::MAX - 1).unwrap();
        let _ = c.inc_by(100); // saturate
        let _ = c.inc_by(100); // saturate again
        assert_eq!(c.overflow_count(), 2);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn reset_to_logs_reset_count() {
        let c = Counter::new("c").unwrap();
        c.inc_by(7).unwrap();
        c.reset_to(0);
        assert_eq!(c.snapshot(), 0);
        assert_eq!(c.reset_count(), 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn reset_to_higher_value_works() {
        let c = Counter::new("c").unwrap();
        c.reset_to(42);
        assert_eq!(c.snapshot(), 42);
        assert_eq!(c.reset_count(), 1);
    }

    #[test]
    fn new_with_tags_validates() {
        let r = Counter::new_with(
            "c",
            &[(TagKey::new("mode"), TagVal::Static("60"))],
            SamplingDiscipline::Always,
        );
        assert!(r.is_ok());
    }

    #[test]
    fn new_with_biometric_tag_refused() {
        let r = Counter::new_with(
            "c",
            &[(TagKey::new("face_id"), TagVal::U64(0))],
            SamplingDiscipline::Always,
        );
        assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
    }

    #[test]
    fn new_with_raw_path_value_refused() {
        let r = Counter::new_with(
            "c",
            &[(TagKey::new("path"), TagVal::Static("/etc/hosts"))],
            SamplingDiscipline::Always,
        );
        assert!(matches!(r, Err(MetricError::RawPathTagValue { .. })));
    }

    #[test]
    fn new_with_too_many_tags_refused() {
        let tags = [
            (TagKey::new("a"), TagVal::U64(1)),
            (TagKey::new("b"), TagVal::U64(2)),
            (TagKey::new("c"), TagVal::U64(3)),
            (TagKey::new("d"), TagVal::U64(4)),
            (TagKey::new("e"), TagVal::U64(5)),
        ];
        let r = Counter::new_with("c", &tags, SamplingDiscipline::Always);
        assert!(matches!(r, Err(MetricError::TagOverflow { .. })));
    }

    #[test]
    fn schema_id_stable_for_same_inputs() {
        let a = Counter::new_with(
            "engine.frame_n",
            &[(TagKey::new("mode"), TagVal::Static("60"))],
            SamplingDiscipline::Always,
        )
        .unwrap();
        let b = Counter::new_with(
            "engine.frame_n",
            &[(TagKey::new("mode"), TagVal::Static("60"))],
            SamplingDiscipline::Always,
        )
        .unwrap();
        assert_eq!(a.schema_id(), b.schema_id());
    }

    #[test]
    fn schema_id_differs_by_name() {
        let a = Counter::new("a").unwrap();
        let b = Counter::new("b").unwrap();
        assert_ne!(a.schema_id(), b.schema_id());
    }

    #[test]
    fn schema_id_differs_by_tag_keys() {
        let a = Counter::new_with(
            "x",
            &[(TagKey::new("k1"), TagVal::U64(1))],
            SamplingDiscipline::Always,
        )
        .unwrap();
        let b = Counter::new_with(
            "x",
            &[(TagKey::new("k2"), TagVal::U64(1))],
            SamplingDiscipline::Always,
        )
        .unwrap();
        assert_ne!(a.schema_id(), b.schema_id());
    }

    #[test]
    fn tags_observable_after_construction() {
        let c = Counter::new_with(
            "x",
            &[(TagKey::new("mode"), TagVal::Static("60"))],
            SamplingDiscipline::Always,
        )
        .unwrap();
        assert_eq!(c.tags().len(), 1);
        assert_eq!(c.tags()[0].0.as_str(), "mode");
    }

    #[test]
    fn sampling_observable_after_construction() {
        let c = Counter::new_with("x", &[], SamplingDiscipline::OneIn(7)).unwrap();
        assert!(matches!(c.sampling(), SamplingDiscipline::OneIn(7)));
    }

    #[cfg(feature = "replay-strict")]
    #[test]
    fn adaptive_under_strict_refused_at_construction() {
        let r = Counter::new_with(
            "c",
            &[],
            SamplingDiscipline::Adaptive { target_overhead_pct: 0.5 },
        );
        assert!(matches!(r, Err(MetricError::AdaptiveUnderStrict { .. })));
    }
}
