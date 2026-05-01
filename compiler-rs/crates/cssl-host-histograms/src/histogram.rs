//! # Histogram : single-stream bounded-memory recorder
//!
//! One named distribution over `u64` µs samples. O(1) record · ~600 byte
//! footprint · O(BUCKETS) percentile query · serializable + mergeable.
//!
//! ## Stats kept
//!
//! - `count` : total samples recorded.
//! - `sum_us` : sum of all values (µs ; widened to `u128` to avoid overflow
//!   over long sessions).
//! - `sum_sq_us` : sum of squared values (`u128` ; for variance / stddev).
//! - `min_us` / `max_us` : extrema.
//! - `buckets[64]` : count per power-of-2 bucket, used for percentile estimation.
//!
//! ## Percentile estimation
//!
//! Walk the bucket array accumulating count until the target rank is reached,
//! then linearly interpolate inside that bucket using the bucket's lower/upper
//! bounds and the partial count consumed. Error is bounded by the bucket
//! width — for 60-fps frame-time samples (bucket 14, 16-32 ms) the worst-case
//! error is ~8 ms relative to the true sample mass within the bucket.

use serde::{Deserialize, Serialize};

use crate::buckets::{bucket_index, bucket_lower_bound, bucket_upper_bound, BUCKETS};

/// Serde adapter : serialize `[u64; BUCKETS]` as a flat `Vec<u64>`, length-verified
/// on deserialize. `serde` does not derive `Deserialize` for arbitrary array
/// sizes (only up to 32) without the `serde-big-array` crate ; rather than
/// adding a dep, we route through a `Vec<u64>` and assert length on the way
/// back in. This keeps the public field as `[u64; BUCKETS]` for O(1) indexing
/// on the hot path.
mod serde_buckets {
    use super::BUCKETS;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(
        buckets: &[u64; BUCKETS],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        // Serialize as a Vec<u64> ; round-trips through any serde format.
        buckets.as_slice().serialize(s)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<[u64; BUCKETS], D::Error> {
        let v: Vec<u64> = Vec::deserialize(d)?;
        if v.len() != BUCKETS {
            return Err(serde::de::Error::invalid_length(
                v.len(),
                &"a Vec<u64> of length 64",
            ));
        }
        let mut out = [0u64; BUCKETS];
        out.copy_from_slice(&v);
        Ok(out)
    }
}

/// A single named histogram : O(1) record, O(buckets) query, ~600 bytes total.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram {
    /// Stream name (e.g. `"frame.total"`, `"gpu.shadow_pass"`).
    pub name: String,
    /// Total recorded samples.
    pub count: u64,
    /// Sum of all sample values (µs). `u128` so a 24-hr session at 1 ms/sample
    /// (~86 M samples × ~1000 µs = ~86 G) doesn't approach overflow.
    pub sum_us: u128,
    /// Sum of squared sample values (µs²). `u128` so squared values remain
    /// representable for the typical telemetry range.
    pub sum_sq_us: u128,
    /// Minimum value seen (µs). `u64::MAX` sentinel before first record.
    pub min_us: u64,
    /// Maximum value seen (µs). `0` before first record (record() updates it
    /// monotonically up).
    pub max_us: u64,
    /// Per-bucket counts. `buckets[i]` is the number of samples in bucket `i`
    /// (see `crate::buckets` for the index mapping).
    #[serde(with = "serde_buckets")]
    pub buckets: [u64; BUCKETS],
}

impl Histogram {
    /// Construct a new empty histogram with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            count: 0,
            sum_us: 0,
            sum_sq_us: 0,
            min_us: u64::MAX,
            max_us: 0,
            buckets: [0; BUCKETS],
        }
    }

    /// Record a sample value (µs) into the histogram.
    ///
    /// O(1) : one bucket-index lookup + a handful of integer ops. Never allocates.
    pub fn record(&mut self, value_us: u64) {
        let idx = bucket_index(value_us);
        self.buckets[idx] = self.buckets[idx].saturating_add(1);
        self.count = self.count.saturating_add(1);
        self.sum_us = self.sum_us.saturating_add(u128::from(value_us));
        let v128 = u128::from(value_us);
        self.sum_sq_us = self.sum_sq_us.saturating_add(v128.saturating_mul(v128));
        if value_us < self.min_us {
            self.min_us = value_us;
        }
        if value_us > self.max_us {
            self.max_us = value_us;
        }
    }

    /// Total samples recorded.
    #[inline]
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Mean value (µs). Returns `0.0` for an empty histogram.
    #[must_use]
    pub fn mean_us(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        // u128 → f64 has bounded loss for values < 2^53 ; sums up to ~9 × 10^15
        // µs (~285 years) preserve full precision.
        (self.sum_us as f64) / (self.count as f64)
    }

    /// Standard deviation (µs) via the sum-of-squares method :
    ///   variance = E[X²] - (E[X])²
    /// Returns `0.0` for an empty histogram or one with all-equal samples.
    /// The `max(0.0)` clamp guards against floating-point negative results
    /// from catastrophic cancellation when variance is near-zero.
    #[must_use]
    pub fn stddev_us(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let n = self.count as f64;
        let mean = (self.sum_us as f64) / n;
        let mean_sq = (self.sum_sq_us as f64) / n;
        let variance = (mean_sq - mean * mean).max(0.0);
        variance.sqrt()
    }

    /// Estimate the percentile-`p` value (µs), where `p ∈ [0.0, 1.0]`.
    ///
    /// `p` is clamped to `[0.0, 1.0]`. For an empty histogram returns `0`.
    ///
    /// Algorithm : walk the bucket array accumulating count, find the bucket
    /// containing the target rank, linearly interpolate inside that bucket
    /// using its `[lower, upper)` bounds and the fraction of the rank still
    /// remaining when the bucket is entered.
    ///
    /// The interpolated value is clamped to `[min_us, max_us]` so the
    /// estimator never reports a value outside the recorded range.
    #[must_use]
    pub fn percentile(&self, p: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let p_clamped = p.clamp(0.0, 1.0);
        // Target rank in samples : ceil(p * count). Clamp to count.
        let target_rank = ((p_clamped * (self.count as f64)).ceil() as u64).max(1);
        let target_rank = target_rank.min(self.count);

        let mut cumulative: u64 = 0;
        for (idx, &bcount) in self.buckets.iter().enumerate() {
            if bcount == 0 {
                continue;
            }
            let next_cumulative = cumulative + bcount;
            if next_cumulative >= target_rank {
                // Target rank lies in this bucket. Interpolate.
                let lo = bucket_lower_bound(idx) as f64;
                // For saturation bucket (63) `upper = u64::MAX` ; clamp to a
                // sensible interpolation target using max_us so the estimator
                // doesn't produce astronomical values for a lonely top-bucket
                // sample.
                let raw_upper = bucket_upper_bound(idx);
                let upper_clamp = if raw_upper == u64::MAX {
                    self.max_us.max(bucket_lower_bound(idx))
                } else {
                    raw_upper.saturating_sub(1)
                } as f64;
                // Fraction within the bucket : (target_rank - cumulative) / bcount
                let in_bucket_rank = (target_rank - cumulative) as f64;
                let frac = (in_bucket_rank / (bcount as f64)).clamp(0.0, 1.0);
                let interp = lo + frac * (upper_clamp - lo);
                let interp_u64 = interp.round().max(0.0) as u64;
                return interp_u64.clamp(self.min_us, self.max_us);
            }
            cumulative = next_cumulative;
        }
        // Should be unreachable when count > 0 ; defensive return.
        self.max_us
    }

    /// P50 (median).
    #[inline]
    #[must_use]
    pub fn p50(&self) -> u64 {
        self.percentile(0.50)
    }
    /// P95.
    #[inline]
    #[must_use]
    pub fn p95(&self) -> u64 {
        self.percentile(0.95)
    }
    /// P99.
    #[inline]
    #[must_use]
    pub fn p99(&self) -> u64 {
        self.percentile(0.99)
    }
    /// P999 (P99.9).
    #[inline]
    #[must_use]
    pub fn p999(&self) -> u64 {
        self.percentile(0.999)
    }

    /// Merge another histogram's data additively into self.
    ///
    /// - Bucket counts add element-wise.
    /// - `count` / `sum_us` / `sum_sq_us` add (saturating).
    /// - `min_us` becomes the smaller, `max_us` the larger.
    /// - Name is unchanged ; the merge does not assert name equality (caller
    ///   may merge same-stream histograms from different workers, OR merge
    ///   differently-named histograms intentionally for aggregate reporting).
    pub fn merge_from(&mut self, other: &Histogram) {
        for i in 0..BUCKETS {
            self.buckets[i] = self.buckets[i].saturating_add(other.buckets[i]);
        }
        self.count = self.count.saturating_add(other.count);
        self.sum_us = self.sum_us.saturating_add(other.sum_us);
        self.sum_sq_us = self.sum_sq_us.saturating_add(other.sum_sq_us);
        if other.count > 0 {
            if other.min_us < self.min_us {
                self.min_us = other.min_us;
            }
            if other.max_us > self.max_us {
                self.max_us = other.max_us;
            }
        }
    }

    /// Reset all stats + bucket counts ; preserve name.
    pub fn reset(&mut self) {
        self.count = 0;
        self.sum_us = 0;
        self.sum_sq_us = 0;
        self.min_us = u64::MAX;
        self.max_us = 0;
        self.buckets = [0; BUCKETS];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § empty histogram returns all-zero stats AND zero percentiles.
    #[test]
    fn empty_histogram_zero_stats() {
        let h = Histogram::new("empty");
        assert_eq!(h.count(), 0);
        assert!((h.mean_us() - 0.0).abs() < f64::EPSILON);
        assert!((h.stddev_us() - 0.0).abs() < f64::EPSILON);
        assert_eq!(h.p50(), 0);
        assert_eq!(h.p95(), 0);
        assert_eq!(h.p99(), 0);
        assert_eq!(h.p999(), 0);
        assert_eq!(h.percentile(0.5), 0);
    }

    /// § single-record histogram : all percentiles return ≈ the recorded value.
    #[test]
    fn single_record_percentiles() {
        let mut h = Histogram::new("single");
        h.record(16_667);
        assert_eq!(h.count(), 1);
        // mean equals the single sample
        assert!((h.mean_us() - 16_667.0).abs() < 0.001);
        // stddev of a single-sample population is 0
        assert!(h.stddev_us() < 0.001);
        // percentiles are clamped to [min, max] which are equal here
        assert_eq!(h.p50(), 16_667);
        assert_eq!(h.p95(), 16_667);
        assert_eq!(h.p99(), 16_667);
        assert_eq!(h.p999(), 16_667);
    }

    /// § uniform distribution : record values 1000..2000 (1000 samples).
    /// p50 should be near 1500 (the mean of the distribution).
    #[test]
    fn uniform_distribution_p50_near_mean() {
        let mut h = Histogram::new("uniform");
        for v in 1000..2000u64 {
            h.record(v);
        }
        assert_eq!(h.count(), 1000);
        let mean = h.mean_us();
        assert!(
            (mean - 1499.5).abs() < 1.0,
            "expected mean ~1499.5 got {mean}",
        );
        // p50 within one bucket-width of mean. All samples fall into bucket 10
        // ([1024, 2048)) so p50 interpolation is bounded by the bucket range.
        let p50 = h.p50();
        assert!((1024..2048).contains(&p50), "p50={p50} out of bucket range");
        // p50 should be in the lower-half of the distribution range or near it
        let diff = if p50 >= 1500 { p50 - 1500 } else { 1500 - p50 };
        assert!(diff < 1024, "p50={p50} far from 1500 (diff={diff})");
    }

    /// § merge_from is additive over both per-stat fields and per-bucket counts.
    #[test]
    fn merge_additive() {
        let mut a = Histogram::new("a");
        a.record(1000);
        a.record(2000);
        let mut b = Histogram::new("b");
        b.record(3000);
        b.record(4000);
        let count_before = a.count();
        let sum_before = a.sum_us;
        a.merge_from(&b);
        assert_eq!(a.count(), count_before + b.count());
        assert_eq!(a.sum_us, sum_before + b.sum_us);
        assert_eq!(a.min_us, 1000);
        assert_eq!(a.max_us, 4000);
        // Bucket totals should sum to 4 across all buckets
        let total: u64 = a.buckets.iter().sum();
        assert_eq!(total, 4);
    }

    /// § reset clears stats but preserves name.
    #[test]
    fn reset_clears() {
        let mut h = Histogram::new("clearme");
        for v in 100..200u64 {
            h.record(v);
        }
        assert!(h.count() > 0);
        h.reset();
        assert_eq!(h.count(), 0);
        assert_eq!(h.sum_us, 0);
        assert_eq!(h.min_us, u64::MAX);
        assert_eq!(h.max_us, 0);
        assert!(h.buckets.iter().all(|&c| c == 0));
        assert_eq!(h.name, "clearme"); // name preserved
    }

    /// § serde round-trip preserves all state exactly.
    #[test]
    fn serde_round_trip() {
        let mut h = Histogram::new("rt");
        for v in [16_000u64, 16_500, 17_000, 33_000, 50_000] {
            h.record(v);
        }
        let json = serde_json::to_string(&h).expect("serialize");
        let h2: Histogram = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(h.name, h2.name);
        assert_eq!(h.count, h2.count);
        assert_eq!(h.sum_us, h2.sum_us);
        assert_eq!(h.sum_sq_us, h2.sum_sq_us);
        assert_eq!(h.min_us, h2.min_us);
        assert_eq!(h.max_us, h2.max_us);
        assert_eq!(h.buckets, h2.buckets);
    }

    /// § percentile clamps p to [0, 1] and never returns outside [min, max].
    #[test]
    fn percentile_clamps_to_bounds() {
        let mut h = Histogram::new("clamp");
        h.record(1000);
        h.record(2000);
        h.record(3000);
        // p < 0 clamps to 0 → returns the smallest sample (≥ min_us)
        let p_neg = h.percentile(-0.5);
        assert!(p_neg >= h.min_us, "p<0 returned {p_neg} < min={}", h.min_us);
        // p > 1 clamps to 1 → returns the largest sample (≤ max_us)
        let p_over = h.percentile(1.5);
        assert!(p_over <= h.max_us, "p>1 returned {p_over} > max={}", h.max_us);
        assert!(p_over >= h.min_us);
        // p ∈ [0, 1] always within [min, max]
        for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let v = h.percentile(p);
            assert!(v >= h.min_us, "p={p}: v={v} < min={}", h.min_us);
            assert!(v <= h.max_us, "p={p}: v={v} > max={}", h.max_us);
        }
    }

    /// § stddev = 0 for a constant-value distribution (Welford-stable).
    #[test]
    fn stddev_zero_for_constant() {
        let mut h = Histogram::new("constant");
        for _ in 0..1000 {
            h.record(1500);
        }
        // All samples are identical → variance is 0
        let stddev = h.stddev_us();
        assert!(
            stddev < 0.5,
            "expected stddev≈0 for constant distribution, got {stddev}",
        );
        assert!((h.mean_us() - 1500.0).abs() < 0.001);
    }
}
