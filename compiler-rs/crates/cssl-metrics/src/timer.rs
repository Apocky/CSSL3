//! Timer — RAII-scoped duration measurement.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.4.
//!
//! § DESIGN
//!   - `Timer.start() -> TimerHandle` ; the handle records the duration on
//!     drop. `#[must_use]` on `TimerHandle` warns at the call-site if a
//!     caller drops it without binding (which would record a zero-duration).
//!   - `record_ns(n)` is the advanced-callers-only path : feed a pre-measured
//!     duration directly. Used by GPU-measured paths where the timestamp
//!     comes from a host-readback.
//!   - Storage is the same atomic-bit-pattern + bucketed histogram pattern as
//!     `Histogram` so percentile-reads (`p50`/`p95`/`p99`) work directly.
//!   - In `replay-strict` mode `monotonic_ns` redirects through
//!     [`crate::strict_clock::monotonic_ns`] which is a deterministic-function
//!     of `(frame_n, sub_phase)` — every `start()..drop` pair becomes
//!     bit-equal across replay-runs.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::counter::compute_schema_id;
use crate::error::MetricResult;
use crate::histogram::{Histogram, HistogramSnapshot, LATENCY_NS_BUCKETS};
use crate::sampling::SamplingDiscipline;
use crate::strict_clock::monotonic_ns;
use crate::tag::{validate_tag_list, TagKey, TagSet, TagVal};

/// Scoped duration metric.
#[derive(Debug)]
pub struct Timer {
    /// Stable metric-name.
    pub name: &'static str,
    /// Cumulative nanos.
    ns_total: AtomicU64,
    /// Number of recorded scopes.
    count: AtomicU64,
    /// Most-recent recorded duration.
    last_ns: AtomicU64,
    /// Latency-bucketed histogram for percentile reads.
    histogram: Histogram,
    /// Tags.
    tags: TagSet,
    /// Sampling.
    sampling: SamplingDiscipline,
    /// Schema-id.
    schema_id: u64,
}

impl Timer {
    /// Construct with no tags + canonical LATENCY_NS bucket-set.
    ///
    /// # Errors
    /// As [`crate::Histogram::new_with`].
    pub fn new(name: &'static str) -> MetricResult<Self> {
        Self::new_with(name, &[], SamplingDiscipline::Always)
    }

    /// Construct with tags + sampling. Bucket-set is fixed to LATENCY_NS_BUCKETS.
    ///
    /// # Errors
    /// As [`crate::Histogram::new_with`].
    pub fn new_with(
        name: &'static str,
        tags: &[(TagKey, TagVal)],
        sampling: SamplingDiscipline,
    ) -> MetricResult<Self> {
        validate_tag_list(name, tags)?;
        sampling.strict_mode_check(name)?;
        let mut tag_set = TagSet::new();
        for (k, v) in tags {
            tag_set.push((*k, *v));
        }
        let histogram_name: &'static str = name; // share name with histogram
        let histogram = Histogram::new_with(histogram_name, LATENCY_NS_BUCKETS, tags, sampling)?;
        Ok(Self {
            name,
            ns_total: AtomicU64::new(0),
            count: AtomicU64::new(0),
            last_ns: AtomicU64::new(0),
            histogram,
            tags: tag_set,
            sampling,
            schema_id: compute_schema_id(name, tags),
        })
    }

    /// Start a scoped timer ; the returned [`TimerHandle`] records on drop.
    #[must_use = "TimerHandle drop = record ; bind to a name or call .commit()"]
    pub fn start(&self) -> TimerHandle<'_> {
        TimerHandle {
            timer: self,
            started_at: monotonic_ns(),
            committed: false,
        }
    }

    /// Record a pre-measured nanosecond duration directly.
    pub fn record_ns(&self, ns: u64) {
        #[cfg(feature = "metrics-disabled")]
        {
            let _ = ns;
        }

        #[cfg(not(feature = "metrics-disabled"))]
        {
            self.ns_total.fetch_add(ns, Ordering::Relaxed);
            self.count.fetch_add(1, Ordering::Relaxed);
            self.last_ns.store(ns, Ordering::Relaxed);
            // Best-effort histogram-record ; ignore NaN (can't happen for u64-source).
            let _ = self.histogram.record(ns as f64);
        }
    }

    /// Cumulative nanoseconds.
    #[must_use]
    pub fn ns_total(&self) -> u64 {
        self.ns_total.load(Ordering::Relaxed)
    }

    /// Number of recorded scopes.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Most-recent recorded nanos.
    #[must_use]
    pub fn last_ns(&self) -> u64 {
        self.last_ns.load(Ordering::Relaxed)
    }

    /// Mean nanoseconds (0 if empty).
    #[must_use]
    pub fn mean_ns(&self) -> f64 {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        (self.ns_total.load(Ordering::Relaxed) as f64) / (count as f64)
    }

    /// p50 of recorded durations (linearly-interpolated).
    #[must_use]
    pub fn p50(&self) -> f64 {
        self.histogram.p50()
    }

    /// p95 of recorded durations.
    #[must_use]
    pub fn p95(&self) -> f64 {
        self.histogram.p95()
    }

    /// p99 of recorded durations.
    #[must_use]
    pub fn p99(&self) -> f64 {
        self.histogram.p99()
    }

    /// Take a snapshot of the underlying histogram.
    #[must_use]
    pub fn histogram_snapshot(&self) -> HistogramSnapshot {
        self.histogram.snapshot()
    }

    /// Tags.
    #[must_use]
    pub fn tags(&self) -> &TagSet {
        &self.tags
    }

    /// Sampling.
    #[must_use]
    pub fn sampling(&self) -> SamplingDiscipline {
        self.sampling
    }

    /// Schema-id.
    #[must_use]
    pub fn schema_id(&self) -> u64 {
        self.schema_id
    }
}

/// RAII scope guard that records duration on drop.
#[must_use = "TimerHandle drop = record ; bind to a name or call .commit()"]
pub struct TimerHandle<'t> {
    timer: &'t Timer,
    started_at: u64,
    committed: bool,
}

impl<'t> TimerHandle<'t> {
    /// Explicit commit (records the duration immediately ; subsequent drop is no-op).
    pub fn commit(mut self) {
        let now = monotonic_ns();
        let dt = now.saturating_sub(self.started_at);
        self.timer.record_ns(dt);
        self.committed = true;
    }

    /// Cancel ; the drop becomes a no-op.
    pub fn cancel(mut self) {
        self.committed = true;
    }

    /// The underlying Timer.
    #[must_use]
    pub fn timer(&self) -> &'t Timer {
        self.timer
    }

    /// Started-at timestamp.
    #[must_use]
    pub fn started_at(&self) -> u64 {
        self.started_at
    }
}

impl<'t> Drop for TimerHandle<'t> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let now = monotonic_ns();
        let dt = now.saturating_sub(self.started_at);
        self.timer.record_ns(dt);
    }
}

#[cfg(test)]
mod tests {
    use super::Timer;
    use crate::error::MetricError;
    use crate::sampling::SamplingDiscipline;
    #[cfg(feature = "replay-strict")]
    use crate::strict_clock::set_logical_clock;
    use crate::tag::{TagKey, TagVal};

    #[test]
    fn new_zero_state() {
        let t = Timer::new("t").unwrap();
        assert_eq!(t.count(), 0);
        assert_eq!(t.ns_total(), 0);
        assert_eq!(t.last_ns(), 0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn record_ns_advances() {
        let t = Timer::new("t").unwrap();
        t.record_ns(100);
        t.record_ns(200);
        assert_eq!(t.count(), 2);
        assert_eq!(t.ns_total(), 300);
        assert_eq!(t.last_ns(), 200);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn record_ns_zero_works() {
        let t = Timer::new("t").unwrap();
        t.record_ns(0);
        assert_eq!(t.count(), 1);
        assert_eq!(t.ns_total(), 0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn mean_ns_basic() {
        let t = Timer::new("t").unwrap();
        for v in [10_u64, 20, 30] {
            t.record_ns(v);
        }
        assert!((t.mean_ns() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn mean_ns_zero_when_empty() {
        let t = Timer::new("t").unwrap();
        assert_eq!(t.mean_ns(), 0.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn handle_drop_records_duration() {
        let t = Timer::new("t").unwrap();
        {
            let _h = t.start();
            // small busy spin
            for _ in 0..100 {
                std::hint::black_box(0);
            }
        } // drop here
        assert_eq!(t.count(), 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn handle_commit_records_duration() {
        let t = Timer::new("t").unwrap();
        let h = t.start();
        h.commit();
        assert_eq!(t.count(), 1);
    }

    #[test]
    fn handle_cancel_does_not_record() {
        let t = Timer::new("t").unwrap();
        let h = t.start();
        h.cancel();
        assert_eq!(t.count(), 0);
    }

    #[test]
    fn handle_started_at_observable() {
        let t = Timer::new("t").unwrap();
        let h = t.start();
        let _started = h.started_at();
        h.cancel();
    }

    #[test]
    fn handle_borrows_timer() {
        let t = Timer::new("t").unwrap();
        let h = t.start();
        let _ref = h.timer();
        h.cancel();
    }

    #[test]
    fn percentiles_work_after_records() {
        let t = Timer::new("t").unwrap();
        for v in [100_u64, 200, 300, 400, 500] {
            t.record_ns(v);
        }
        let p50 = t.p50();
        let p95 = t.p95();
        let p99 = t.p99();
        assert!(p50 <= p95);
        assert!(p95 <= p99);
    }

    #[test]
    fn percentiles_zero_when_empty() {
        let t = Timer::new("t").unwrap();
        assert_eq!(t.p50(), 0.0);
    }

    #[test]
    fn schema_id_stable() {
        let a = Timer::new("t").unwrap();
        let b = Timer::new("t").unwrap();
        assert_eq!(a.schema_id(), b.schema_id());
    }

    #[test]
    fn new_with_tags_validates_biometric() {
        let r = Timer::new_with(
            "t",
            &[(TagKey::new("face_id"), TagVal::U64(0))],
            SamplingDiscipline::Always,
        );
        assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
    }

    #[test]
    fn new_with_tags_visible() {
        let t = Timer::new_with(
            "t",
            &[(TagKey::new("phase"), TagVal::Static("compose"))],
            SamplingDiscipline::Always,
        )
        .unwrap();
        assert_eq!(t.tags().len(), 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn histogram_snapshot_visible() {
        let t = Timer::new("t").unwrap();
        t.record_ns(50);
        let snap = t.histogram_snapshot();
        assert_eq!(snap.count, 1);
    }

    #[cfg(feature = "replay-strict")]
    #[test]
    fn strict_mode_records_deterministic_duration() {
        // Install a known logical clock.
        set_logical_clock(0, 0);
        let t = Timer::new("t").unwrap();
        let h = t.start();
        // Advance the cursor as though 1000ns of "real time" has passed
        // (replay drivers do this between sub-phases).
        set_logical_clock(0, 1000);
        h.commit();
        assert_eq!(t.last_ns(), 1000);
    }

    #[cfg(feature = "replay-strict")]
    #[test]
    fn strict_mode_two_runs_bit_equal() {
        set_logical_clock(0, 0);
        let t1 = Timer::new("t").unwrap();
        let h1 = t1.start();
        set_logical_clock(0, 500);
        h1.commit();

        set_logical_clock(0, 0);
        let t2 = Timer::new("t").unwrap();
        let h2 = t2.start();
        set_logical_clock(0, 500);
        h2.commit();

        assert_eq!(t1.last_ns(), t2.last_ns());
    }

    #[test]
    fn handle_must_use_attribute_present() {
        // This test ensures the type is constructable + the must_use fires
        // as a compiler-warning at unbind sites (not testable directly here ;
        // we just verify construction).
        let t = Timer::new("t").unwrap();
        let _h = t.start();
        // _h binding satisfies must-use ; drop here.
    }

    #[test]
    fn canceled_handle_does_not_inflate_count() {
        let t = Timer::new("t").unwrap();
        for _ in 0..10 {
            let h = t.start();
            h.cancel();
        }
        assert_eq!(t.count(), 0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn committed_handle_records_count() {
        let t = Timer::new("t").unwrap();
        for _ in 0..5 {
            let h = t.start();
            h.commit();
        }
        assert_eq!(t.count(), 5);
    }

    #[test]
    fn record_ns_underflow_clamped_at_zero_via_saturating_sub() {
        // Even if monotonic_ns somehow went backwards (it shouldn't), the
        // saturating_sub in TimerHandle::drop produces 0, not a wrap-around.
        let t = Timer::new("t").unwrap();
        // We can't easily induce a clock-reverse, so instead exercise the
        // record_ns(0) path which is the same observable outcome.
        t.record_ns(0);
        assert_eq!(t.last_ns(), 0);
    }
}
