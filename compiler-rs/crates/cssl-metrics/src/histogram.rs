//! Histogram — bucketed distribution + linearly-interpolated percentiles.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.3 (incl. canonical
//!         bucket-sets : `LATENCY_NS_BUCKETS` / `BYTES_BUCKETS` /
//!         `COUNT_BUCKETS` / `PIXEL_BUCKETS`).
//!
//! § DESIGN
//!   - `bucket_boundaries` is `&'static [f64]` — exclusive-upper-bounds, strict
//!     monotonic, COMPILE-TIME-CONSTANT (per § II.3 ‼ N! data-driven inference,
//!     N! adaptive histograms in strict-mode).
//!   - `counts` length = `bucket_boundaries.len() + 1` so the last bucket is
//!     the `[boundaries[N-1], +∞)` overflow bin.
//!   - `record(v)` advances `count` + `sum` (atomic) and the matched-bucket's
//!     count.
//!   - `percentile(p)` walks `counts` cumulatively and linearly-interpolates
//!     within the bucket containing the p-th observation. The interpolation
//!     uses bucket-edges as endpoints so the result lands within the matched
//!     bucket's range. p=0.0 returns the lowest-edge ; p=1.0 returns the
//!     highest-edge.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::counter::compute_schema_id;
use crate::error::{MetricError, MetricResult};
use crate::sampling::SamplingDiscipline;
use crate::tag::{validate_tag_list, TagKey, TagSet, TagVal};

/// Canonical latency-nanosecond bucket-set (§ II.3).
pub const LATENCY_NS_BUCKETS: &[f64] = &[
    10.0,
    100.0,
    1_000.0,
    10_000.0,
    100_000.0,
    1_000_000.0,
    10_000_000.0,
    100_000_000.0,
];

/// Canonical bytes bucket-set (§ II.3).
pub const BYTES_BUCKETS: &[f64] = &[
    64.0,
    256.0,
    1_024.0,
    4_096.0,
    16_384.0,
    65_536.0,
    262_144.0,
    1_048_576.0,
];

/// Canonical count bucket-set (§ II.3).
pub const COUNT_BUCKETS: &[f64] = &[1.0, 4.0, 16.0, 64.0, 256.0, 1_024.0, 4_096.0];

/// Canonical pixel bucket-set (§ II.3).
pub const PIXEL_BUCKETS: &[f64] = &[
    1.0, 4.0, 16.0, 64.0, 256.0, 1_024.0, 4_096.0, 16_384.0,
];

/// A read-only snapshot of histogram-state at one moment.
#[derive(Debug, Clone)]
pub struct HistogramSnapshot {
    /// Per-bucket counts (length = boundaries.len() + 1).
    pub counts: Vec<u64>,
    /// Bucket boundaries (excl. upper). Borrowed from the live histogram.
    pub boundaries: &'static [f64],
    /// Cumulative count (sum of all bucket-counts).
    pub count: u64,
    /// Cumulative sum of recorded values (f64 from-bits).
    pub sum: f64,
}

impl HistogramSnapshot {
    /// Mean of recorded values ; NaN-safe (returns 0 if count = 0).
    #[must_use]
    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.sum / (self.count as f64)
    }

    /// Linearly-interpolated p-th percentile.
    ///
    /// § BEHAVIOR
    ///   - `p = 0.0` ⇒ lowest-bucket lower-edge (`-∞` clamped to first boundary
    ///     for stability ; uses `boundaries[0] / 2.0` if positive, else `0.0`)
    ///   - `p = 1.0` ⇒ highest-bucket upper-edge (last boundary; the overflow-
    ///     bin reports `boundaries[N-1]` as a stable upper bound)
    ///   - `0 < p < 1` ⇒ walk cumulative counts, linearly-interpolate within
    ///     the bucket that contains the p-th observation
    #[must_use]
    pub fn percentile(&self, p: f64) -> f64 {
        if self.count == 0 || self.boundaries.is_empty() {
            return 0.0;
        }
        let p = p.clamp(0.0, 1.0);
        if p == 0.0 {
            return 0.0_f64.max(self.boundaries[0] / 2.0).min(self.boundaries[0]);
        }
        if p == 1.0 {
            return *self.boundaries.last().expect("boundaries non-empty");
        }
        let target = p * (self.count as f64);
        let mut cumulative: u64 = 0;
        for (i, c) in self.counts.iter().enumerate() {
            let new_cum = cumulative + *c;
            if (new_cum as f64) >= target {
                let in_bucket = target - (cumulative as f64);
                let bucket_count = *c as f64;
                let frac = if bucket_count > 0.0 {
                    in_bucket / bucket_count
                } else {
                    0.0
                };
                let lo = if i == 0 {
                    0.0_f64.max(self.boundaries[0] / 2.0).min(self.boundaries[0])
                } else {
                    self.boundaries[i - 1]
                };
                let hi = if i < self.boundaries.len() {
                    self.boundaries[i]
                } else {
                    self.boundaries[self.boundaries.len() - 1]
                };
                return lo + (hi - lo) * frac;
            }
            cumulative = new_cum;
        }
        // unreachable in well-formed input but defensive
        *self.boundaries.last().expect("boundaries non-empty")
    }
}

/// Bucketed distribution metric.
#[derive(Debug)]
pub struct Histogram {
    /// Stable metric-name.
    pub name: &'static str,
    boundaries: &'static [f64],
    counts: Vec<AtomicU64>,
    sum_bits: AtomicU64, // f64 packed bit-pattern of cumulative sum
    count: AtomicU64,
    tags: TagSet,
    sampling: SamplingDiscipline,
    schema_id: u64,
}

impl Histogram {
    /// Construct with a canonical bucket-set + no tags.
    ///
    /// # Errors
    /// - [`MetricError::Bucket`] on empty / non-monotonic / non-strictly-increasing
    ///   boundaries
    pub fn new(name: &'static str, boundaries: &'static [f64]) -> MetricResult<Self> {
        Self::new_with(name, boundaries, &[], SamplingDiscipline::Always)
    }

    /// Construct with tags + sampling.
    ///
    /// # Errors
    /// As [`Histogram::new`] + [`crate::Counter::new_with`].
    pub fn new_with(
        name: &'static str,
        boundaries: &'static [f64],
        tags: &[(TagKey, TagVal)],
        sampling: SamplingDiscipline,
    ) -> MetricResult<Self> {
        if boundaries.is_empty() {
            return Err(MetricError::Bucket {
                name,
                detail: "empty-boundaries",
            });
        }
        for w in boundaries.windows(2) {
            // partial_cmp is the discipline-respecting form for f64 comparison
            // (clippy::neg_cmp_op_on_partial_ord). NaN-pairs return None which
            // we treat the same as non-monotonic (the explicit NaN-check below
            // catches NaN earlier in well-formed input).
            if w[0].partial_cmp(&w[1]).map_or(true, |o| !o.is_lt()) {
                return Err(MetricError::Bucket {
                    name,
                    detail: "non-monotonic-boundaries",
                });
            }
        }
        for b in boundaries {
            if b.is_nan() {
                return Err(MetricError::Bucket {
                    name,
                    detail: "nan-boundary",
                });
            }
            if b.is_infinite() {
                return Err(MetricError::Bucket {
                    name,
                    detail: "inf-boundary",
                });
            }
        }
        validate_tag_list(name, tags)?;
        sampling.strict_mode_check(name)?;
        let mut tag_set = TagSet::new();
        for (k, v) in tags {
            tag_set.push((*k, *v));
        }
        let counts = (0..=boundaries.len())
            .map(|_| AtomicU64::new(0))
            .collect();
        Ok(Self {
            name,
            boundaries,
            counts,
            sum_bits: AtomicU64::new(0_u64),
            count: AtomicU64::new(0),
            tags: tag_set,
            sampling,
            schema_id: compute_schema_id(name, tags),
        })
    }

    /// Record a value into the matching bucket.
    ///
    /// # Errors
    /// - [`MetricError::Nan`] on NaN input
    pub fn record(&self, v: f64) -> MetricResult<()> {
        #[cfg(feature = "metrics-disabled")]
        {
            let _ = v;
            return Ok(());
        }

        #[cfg(not(feature = "metrics-disabled"))]
        {
            if v.is_nan() {
                return Err(MetricError::Nan { name: self.name });
            }
            // Inf goes to the overflow-bin (last bucket).
            let idx = if v.is_infinite() && v > 0.0 {
                self.boundaries.len()
            } else if v.is_infinite() {
                0
            } else {
                self.bucket_for(v)
            };
            self.counts[idx].fetch_add(1, Ordering::Relaxed);
            // Atomic-add to sum (compare-exchange loop on the bit-pattern).
            loop {
                let cur_bits = self.sum_bits.load(Ordering::Relaxed);
                let cur = f64::from_bits(cur_bits);
                let next = cur + v;
                let next_bits = next.to_bits();
                if self
                    .sum_bits
                    .compare_exchange(cur_bits, next_bits, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    break;
                }
            }
            self.count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    /// Lookup the bucket-index for `v` (linear scan ; boundaries.len() is
    /// small in practice — 7 to 8 entries — so this is faster than binary
    /// search for the typical case).
    fn bucket_for(&self, v: f64) -> usize {
        for (i, b) in self.boundaries.iter().enumerate() {
            if v < *b {
                return i;
            }
        }
        self.boundaries.len()
    }

    /// Take a snapshot of current state (consistent-enough for monitoring ;
    /// individual bucket reads are atomic but the cross-bucket total may
    /// drift by ≤ 1 increment under high concurrency).
    #[must_use]
    pub fn snapshot(&self) -> HistogramSnapshot {
        HistogramSnapshot {
            counts: self
                .counts
                .iter()
                .map(|c| c.load(Ordering::Relaxed))
                .collect(),
            boundaries: self.boundaries,
            count: self.count.load(Ordering::Relaxed),
            sum: f64::from_bits(self.sum_bits.load(Ordering::Relaxed)),
        }
    }

    /// Direct percentile read. Equivalent to `self.snapshot().percentile(p)`.
    #[must_use]
    pub fn percentile(&self, p: f64) -> f64 {
        self.snapshot().percentile(p)
    }

    /// p50 — convenience.
    #[must_use]
    pub fn p50(&self) -> f64 {
        self.percentile(0.5)
    }

    /// p95 — convenience.
    #[must_use]
    pub fn p95(&self) -> f64 {
        self.percentile(0.95)
    }

    /// p99 — convenience.
    #[must_use]
    pub fn p99(&self) -> f64 {
        self.percentile(0.99)
    }

    /// Cumulative count.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Cumulative sum.
    #[must_use]
    pub fn sum(&self) -> f64 {
        f64::from_bits(self.sum_bits.load(Ordering::Relaxed))
    }

    /// Bucket boundaries.
    #[must_use]
    pub fn boundaries(&self) -> &'static [f64] {
        self.boundaries
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

#[cfg(test)]
mod tests {
    use super::{
        Histogram, HistogramSnapshot, BYTES_BUCKETS, COUNT_BUCKETS, LATENCY_NS_BUCKETS,
        PIXEL_BUCKETS,
    };
    use crate::error::MetricError;
    use crate::sampling::SamplingDiscipline;
    use crate::tag::{TagKey, TagVal};

    #[test]
    fn canonical_latency_buckets_match_spec() {
        assert_eq!(
            LATENCY_NS_BUCKETS,
            &[
                10.0, 100.0, 1_000.0, 10_000.0, 100_000.0, 1_000_000.0, 10_000_000.0,
                100_000_000.0
            ]
        );
    }

    #[test]
    fn canonical_bytes_buckets_match_spec() {
        assert_eq!(
            BYTES_BUCKETS,
            &[
                64.0, 256.0, 1_024.0, 4_096.0, 16_384.0, 65_536.0, 262_144.0, 1_048_576.0
            ]
        );
    }

    #[test]
    fn canonical_count_buckets_match_spec() {
        assert_eq!(
            COUNT_BUCKETS,
            &[1.0, 4.0, 16.0, 64.0, 256.0, 1_024.0, 4_096.0]
        );
    }

    #[test]
    fn canonical_pixel_buckets_match_spec() {
        assert_eq!(
            PIXEL_BUCKETS,
            &[1.0, 4.0, 16.0, 64.0, 256.0, 1_024.0, 4_096.0, 16_384.0]
        );
    }

    #[test]
    fn new_with_canonical_works() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        assert_eq!(h.boundaries().len(), 8);
    }

    #[test]
    fn new_with_empty_boundaries_refused() {
        const EMPTY: &[f64] = &[];
        let r = Histogram::new("h", EMPTY);
        assert!(matches!(r, Err(MetricError::Bucket { .. })));
    }

    #[test]
    fn new_with_non_monotonic_refused() {
        const BAD: &[f64] = &[1.0, 2.0, 1.5];
        let r = Histogram::new("h", BAD);
        assert!(matches!(r, Err(MetricError::Bucket { detail, .. }) if detail == "non-monotonic-boundaries"));
    }

    #[test]
    fn new_with_duplicate_refused() {
        const BAD: &[f64] = &[1.0, 2.0, 2.0];
        let r = Histogram::new("h", BAD);
        assert!(matches!(r, Err(MetricError::Bucket { .. })));
    }

    #[test]
    fn new_with_nan_boundary_refused() {
        const BAD: &[f64] = &[1.0, f64::NAN];
        let r = Histogram::new("h", BAD);
        assert!(matches!(r, Err(MetricError::Bucket { detail, .. }) if detail == "non-monotonic-boundaries"
            || matches!(r, Err(MetricError::Bucket { detail, .. }) if detail == "nan-boundary")));
        // (Either path : non-monotonic comparison with NaN returns false ; some
        // platforms hit the explicit NaN check first.)
    }

    #[test]
    fn new_with_inf_boundary_refused() {
        const BAD: &[f64] = &[1.0, f64::INFINITY];
        let r = Histogram::new("h", BAD);
        assert!(matches!(r, Err(MetricError::Bucket { .. })));
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn record_basic_advances_count() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(50.0).unwrap();
        h.record(500.0).unwrap();
        assert_eq!(h.count(), 2);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn record_nan_refused() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        let r = h.record(f64::NAN);
        assert!(matches!(r, Err(MetricError::Nan { .. })));
        assert_eq!(h.count(), 0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn record_inf_goes_to_overflow_bin() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(f64::INFINITY).unwrap();
        let snap = h.snapshot();
        // Overflow-bin is at index = boundaries.len() = 8.
        assert_eq!(snap.counts[snap.counts.len() - 1], 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn record_neg_inf_goes_to_first_bin() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(f64::NEG_INFINITY).unwrap();
        let snap = h.snapshot();
        assert_eq!(snap.counts[0], 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn record_many_advances_sum() {
        let h = Histogram::new("h", COUNT_BUCKETS).unwrap();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            h.record(v).unwrap();
        }
        assert_eq!(h.sum(), 15.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn snapshot_bucket_counts_correct() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        // 50 → bucket-index 1 (10 ≤ 50 < 100)
        // 500 → bucket-index 2 (100 ≤ 500 < 1_000)
        // 5_000 → bucket-index 3 (1_000 ≤ 5_000 < 10_000)
        h.record(50.0).unwrap();
        h.record(500.0).unwrap();
        h.record(5_000.0).unwrap();
        let snap = h.snapshot();
        assert_eq!(snap.counts[1], 1);
        assert_eq!(snap.counts[2], 1);
        assert_eq!(snap.counts[3], 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn snapshot_first_bin_for_below_first_boundary() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(5.0).unwrap(); // 5 < 10 ⇒ bucket-0
        let snap = h.snapshot();
        assert_eq!(snap.counts[0], 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn snapshot_overflow_bin_for_above_last_boundary() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(1e10).unwrap(); // > 1e8 ⇒ overflow-bin
        let snap = h.snapshot();
        assert_eq!(snap.counts[snap.counts.len() - 1], 1);
    }

    #[test]
    fn percentile_zero_returns_low_edge() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(50.0).unwrap();
        let p = h.percentile(0.0);
        assert!(p >= 0.0 && p <= 10.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn percentile_one_returns_top_edge() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(50.0).unwrap();
        assert_eq!(h.percentile(1.0), 100_000_000.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn percentile_within_bucket_interpolated() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        // Drop 100 records into bucket-1 (10 ≤ v < 100) so percentile-50
        // lands in middle of that bucket.
        for _ in 0..100 {
            h.record(50.0).unwrap();
        }
        let p50 = h.p50();
        // Bucket-1 spans (10, 100) — interpolated p50 should be ~55.
        assert!(p50 >= 10.0 && p50 <= 100.0);
    }

    #[test]
    fn p50_p95_p99_helpers_work() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        for v in [50.0, 500.0, 5_000.0, 50_000.0, 500_000.0] {
            h.record(v).unwrap();
        }
        let p50 = h.p50();
        let p95 = h.p95();
        let p99 = h.p99();
        // Monotone : p50 ≤ p95 ≤ p99.
        assert!(p50 <= p95);
        assert!(p95 <= p99);
    }

    #[test]
    fn percentile_zero_count_returns_zero() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        assert_eq!(h.p50(), 0.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn snapshot_mean_correct() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        for v in [10.0, 20.0, 30.0] {
            h.record(v).unwrap();
        }
        let snap = h.snapshot();
        assert_eq!(snap.mean(), 20.0);
    }

    #[test]
    fn snapshot_mean_zero_when_empty() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        assert_eq!(h.snapshot().mean(), 0.0);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn count_and_bucket_consistency() {
        let h = Histogram::new("h", COUNT_BUCKETS).unwrap();
        for v in 0..50_u64 {
            h.record(v as f64).unwrap();
        }
        let snap = h.snapshot();
        let total: u64 = snap.counts.iter().sum();
        assert_eq!(total, 50);
        assert_eq!(snap.count, 50);
    }

    #[test]
    fn schema_id_stable() {
        let a = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        let b = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        assert_eq!(a.schema_id(), b.schema_id());
    }

    #[test]
    fn new_with_validates_biometric_tag() {
        let r = Histogram::new_with(
            "h",
            LATENCY_NS_BUCKETS,
            &[(TagKey::new("face_id"), TagVal::U64(0))],
            SamplingDiscipline::Always,
        );
        assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
    }

    #[test]
    fn snapshot_struct_clone_works() {
        let h = Histogram::new("h", COUNT_BUCKETS).unwrap();
        h.record(2.0).unwrap();
        let snap: HistogramSnapshot = h.snapshot();
        let _cloned = snap.clone();
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn snapshot_independent_from_live_record() {
        let h = Histogram::new("h", COUNT_BUCKETS).unwrap();
        h.record(2.0).unwrap();
        let snap = h.snapshot();
        h.record(2.0).unwrap();
        // Snapshot is the count at-the-time-of-snapshot, not live.
        assert_eq!(snap.count, 1);
        assert_eq!(h.count(), 2);
    }

    #[test]
    fn percentile_clamp_above_one() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(50.0).unwrap();
        // Above 1.0 should clamp to 1.0.
        assert_eq!(h.percentile(2.0), h.percentile(1.0));
    }

    #[test]
    fn percentile_clamp_below_zero() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(50.0).unwrap();
        assert_eq!(h.percentile(-1.0), h.percentile(0.0));
    }
}
