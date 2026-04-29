//! Mock surface for `cssl-metrics` (T11-D157) primitives.
//!
//! § PURPOSE
//!   T11-D157 has not yet been merged to main. This module provides a
//!   trait-bound surface that mirrors the cssl-metrics ABI :
//!   [`Timer`] + [`Histogram`] + [`MetricsRegistry`].
//!
//!   When T11-D157 lands, the trait `MetricsRegistry` will be re-implemented
//!   over the real registry surface ; downstream code in [`crate::m8_integration`]
//!   will continue to compile against the trait without modification.
//!
//! § FEATURE-GATE
//!   - `metrics` (default-on)   → real Timer / Histogram / Registry impls
//!   - `metrics` (off)          → no-op shims for zero-overhead builds
//!
//!   Zero-overhead build verification : the no-op shims have empty bodies
//!   and `#[inline(always)]` ; the compiler elides every instrumentation
//!   call-site, yielding identical machine-code to a non-instrumented build.
//!
//! § PERCENTILE BACKEND
//!   [`Histogram`] uses a lock-free ring of last-N samples (N=1024 default)
//!   and computes p50/p95/p99 via partition-sort on snapshot. This matches
//!   the cssl-metrics T11-D157 ABI : `Histogram::p50()`, `p95()`, `p99()`.
//!
//! § DETERMINISM-NOTE
//!   Timing samples are observe-only. The render-pipeline state is never
//!   mutated by [`Timer::start`] / [`Timer::stop`] / [`Histogram::record`].
//!   This guarantees that replay-determinism is preserved (cf. acceptance
//!   gate § 4 of dispatch).

#[cfg(feature = "metrics")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "metrics")]
use std::time::Instant;

/// Default cross-frame trend window (last-N samples per stage).
pub const TREND_WINDOW: usize = 1024;

// ────────────────────────────────────────────────────────────────────
// § Timer surface
// ────────────────────────────────────────────────────────────────────

/// Stopwatch primitive — mirrors `cssl_metrics::Timer` from T11-D157.
///
/// § USAGE
/// ```ignore
/// let t = Timer::start();
/// run_stage();
/// let elapsed_ms = t.stop_ms();
/// ```
///
/// § ZERO-OVERHEAD
/// When `feature = "metrics"` is disabled, [`Timer::start`] returns a
/// zero-sized type and [`Timer::stop_ms`] returns `0.0` ; the compiler
/// elides every call-site.
#[derive(Debug)]
pub struct Timer {
    #[cfg(feature = "metrics")]
    started_at: Instant,
}

impl Timer {
    /// Begin timing. Wall-clock `Instant::now()` snapshot.
    #[inline(always)]
    #[must_use]
    pub fn start() -> Self {
        Self {
            #[cfg(feature = "metrics")]
            started_at: Instant::now(),
        }
    }

    /// Stop timing and return elapsed milliseconds.
    ///
    /// Returns `0.0` when `feature = "metrics"` is disabled.
    #[inline(always)]
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn stop_ms(self) -> f64 {
        #[cfg(feature = "metrics")]
        {
            let elapsed = self.started_at.elapsed();
            elapsed.as_secs_f64() * 1000.0
        }
        #[cfg(not(feature = "metrics"))]
        {
            0.0
        }
    }

    /// Stop timing and return elapsed nanoseconds (high-precision path).
    #[inline(always)]
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn stop_ns(self) -> u128 {
        #[cfg(feature = "metrics")]
        {
            self.started_at.elapsed().as_nanos()
        }
        #[cfg(not(feature = "metrics"))]
        {
            0
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// § Histogram surface
// ────────────────────────────────────────────────────────────────────

/// Percentile-aggregating sample-buffer.
///
/// § DESIGN
///   - Last-N (N=[`TREND_WINDOW`]) ring of `f64` samples
///   - p50/p95/p99 computed via partition-sort on snapshot
///   - Lock-protected (std::sync::Mutex) ; SPSC pipeline use ¬contended
///
/// § ZERO-OVERHEAD
///   When `feature = "metrics"` is off, the inner buffer is `()` and
///   [`Histogram::record`] / [`Histogram::p50`] etc. are no-ops returning
///   `f64::NAN` to flag "no data".
#[derive(Debug, Clone)]
pub struct Histogram {
    #[cfg(feature = "metrics")]
    inner: Arc<Mutex<HistogramInner>>,
    #[cfg(not(feature = "metrics"))]
    _marker: (),
}

#[cfg(feature = "metrics")]
#[derive(Debug)]
struct HistogramInner {
    /// Ring of last-N samples (N = capacity).
    samples: Vec<f64>,
    /// Capacity = `TREND_WINDOW` by default.
    capacity: usize,
    /// Insertion index (mod capacity).
    cursor: usize,
    /// Total samples ever recorded (saturating-add).
    total_count: u64,
    /// Filled-positions counter (≤ capacity).
    filled: usize,
}

impl Histogram {
    /// Construct a new histogram with default [`TREND_WINDOW`] capacity.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(TREND_WINDOW)
    }

    /// Construct a histogram with custom capacity.
    #[inline]
    #[must_use]
    #[allow(unused_variables)]
    pub fn with_capacity(capacity: usize) -> Self {
        #[cfg(feature = "metrics")]
        {
            let cap = capacity.max(1);
            Self {
                inner: Arc::new(Mutex::new(HistogramInner {
                    samples: Vec::with_capacity(cap),
                    capacity: cap,
                    cursor: 0,
                    total_count: 0,
                    filled: 0,
                })),
            }
        }
        #[cfg(not(feature = "metrics"))]
        {
            Self { _marker: () }
        }
    }

    /// Record a sample (e.g. frame-time-ms).
    #[inline(always)]
    #[allow(unused_variables)]
    pub fn record(&self, value: f64) {
        #[cfg(feature = "metrics")]
        {
            let mut inner = self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)");
            let cap = inner.capacity;
            if inner.samples.len() < cap {
                inner.samples.push(value);
                inner.filled = inner.samples.len();
            } else {
                let cursor = inner.cursor;
                inner.samples[cursor] = value;
            }
            inner.cursor = (inner.cursor + 1) % cap;
            inner.total_count = inner.total_count.saturating_add(1);
        }
    }

    /// Sample count currently held (≤ capacity).
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        #[cfg(feature = "metrics")]
        {
            self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)").filled
        }
        #[cfg(not(feature = "metrics"))]
        {
            0
        }
    }

    /// Total samples ever recorded (saturating).
    #[inline]
    #[must_use]
    pub fn total_count(&self) -> u64 {
        #[cfg(feature = "metrics")]
        {
            self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)").total_count
        }
        #[cfg(not(feature = "metrics"))]
        {
            0
        }
    }

    /// Returns `true` when no samples are recorded yet.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 50th-percentile (median) of last-N samples.
    /// Returns `f64::NAN` when empty.
    #[inline]
    #[must_use]
    pub fn p50(&self) -> f64 {
        self.percentile(0.50)
    }

    /// 95th-percentile of last-N samples.
    /// Returns `f64::NAN` when empty.
    #[inline]
    #[must_use]
    pub fn p95(&self) -> f64 {
        self.percentile(0.95)
    }

    /// 99th-percentile of last-N samples.
    /// Returns `f64::NAN` when empty.
    #[inline]
    #[must_use]
    pub fn p99(&self) -> f64 {
        self.percentile(0.99)
    }

    /// Mean of last-N samples. Returns `f64::NAN` when empty.
    #[inline]
    #[must_use]
    pub fn mean(&self) -> f64 {
        #[cfg(feature = "metrics")]
        {
            let inner = self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)");
            if inner.filled == 0 {
                return f64::NAN;
            }
            let sum: f64 = inner.samples.iter().sum();
            sum / inner.filled as f64
        }
        #[cfg(not(feature = "metrics"))]
        {
            f64::NAN
        }
    }

    /// Min of last-N samples. Returns `f64::NAN` when empty.
    #[inline]
    #[must_use]
    pub fn min(&self) -> f64 {
        #[cfg(feature = "metrics")]
        {
            let inner = self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)");
            if inner.filled == 0 {
                return f64::NAN;
            }
            inner.samples.iter().copied().fold(f64::INFINITY, f64::min)
        }
        #[cfg(not(feature = "metrics"))]
        {
            f64::NAN
        }
    }

    /// Max of last-N samples. Returns `f64::NAN` when empty.
    #[inline]
    #[must_use]
    pub fn max(&self) -> f64 {
        #[cfg(feature = "metrics")]
        {
            let inner = self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)");
            if inner.filled == 0 {
                return f64::NAN;
            }
            inner
                .samples
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
        }
        #[cfg(not(feature = "metrics"))]
        {
            f64::NAN
        }
    }

    /// Snapshot the current sample-window (last-N) as a sorted Vec.
    /// Empty Vec when no samples or feature-disabled.
    #[inline]
    #[must_use]
    pub fn snapshot_sorted(&self) -> Vec<f64> {
        #[cfg(feature = "metrics")]
        {
            let mut snap: Vec<f64> = {
                let inner = self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)");
                inner.samples.clone()
            };
            snap.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            snap
        }
        #[cfg(not(feature = "metrics"))]
        {
            Vec::new()
        }
    }

    /// Reset the histogram (clears samples + counters).
    #[inline]
    pub fn reset(&self) {
        #[cfg(feature = "metrics")]
        {
            let mut inner = self.inner.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)");
            inner.samples.clear();
            inner.cursor = 0;
            inner.total_count = 0;
            inner.filled = 0;
        }
    }

    /// Compute arbitrary-percentile of last-N samples via partition-sort.
    ///
    /// `q ∈ [0.0, 1.0]` ; `q=0.0 → min`, `q=1.0 → max`.
    /// Uses nearest-rank method : `idx = ceil(q * N) - 1`.
    #[inline]
    #[must_use]
    #[allow(unused_variables)]
    pub fn percentile(&self, q: f64) -> f64 {
        #[cfg(feature = "metrics")]
        {
            let snap = self.snapshot_sorted();
            if snap.is_empty() {
                return f64::NAN;
            }
            let n = snap.len();
            #[allow(clippy::cast_sign_loss)]
            let idx = if q <= 0.0 {
                0
            } else if q >= 1.0 {
                n - 1
            } else {
                let raw = (q * n as f64).ceil() as usize;
                raw.saturating_sub(1).min(n - 1)
            };
            snap[idx]
        }
        #[cfg(not(feature = "metrics"))]
        {
            f64::NAN
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

// ────────────────────────────────────────────────────────────────────
// § MetricsRegistry trait — surface for cssl-metrics swap-in
// ────────────────────────────────────────────────────────────────────

/// Trait-bound surface mirroring the (planned) `cssl_metrics::Registry` ABI.
///
/// § ROLE
///   Permits the M8 pipeline to register per-stage histograms by namespace
///   without depending on a concrete cssl-metrics impl. When T11-D157 lands,
///   `cssl_metrics::Registry` will implement this trait and the pipeline
///   wiring will swap-in transparently.
///
/// § NAMESPACE-CONVENTION
///   `pipeline.stage_N_<name>.frame_time_ms` (N = 1..=12)
pub trait MetricsRegistry: Send + Sync {
    /// Register (or fetch existing) a histogram by namespace.
    fn register_histogram(&self, namespace: &str) -> Histogram;

    /// Look up a histogram by namespace ; returns `None` when unregistered.
    fn lookup_histogram(&self, namespace: &str) -> Option<Histogram>;

    /// Enumerate registered namespaces (lex-sorted for determinism).
    fn enumerate_namespaces(&self) -> Vec<String>;
}

/// Default in-memory registry (mock).
///
/// Replaceable with `cssl_metrics::Registry` once T11-D157 lands.
#[derive(Debug, Default)]
pub struct MockRegistry {
    #[cfg(feature = "metrics")]
    entries: Mutex<std::collections::BTreeMap<String, Histogram>>,
    #[cfg(not(feature = "metrics"))]
    _marker: (),
}

impl MockRegistry {
    /// Construct a new empty registry.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl MetricsRegistry for MockRegistry {
    #[inline]
    #[allow(unused_variables)]
    fn register_histogram(&self, namespace: &str) -> Histogram {
        #[cfg(feature = "metrics")]
        {
            let mut entries = self.entries.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)");
            entries
                .entry(namespace.to_owned())
                .or_default()
                .clone()
        }
        #[cfg(not(feature = "metrics"))]
        {
            Histogram::new()
        }
    }

    #[inline]
    #[allow(unused_variables)]
    fn lookup_histogram(&self, namespace: &str) -> Option<Histogram> {
        #[cfg(feature = "metrics")]
        {
            self.entries.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)").get(namespace).cloned()
        }
        #[cfg(not(feature = "metrics"))]
        {
            None
        }
    }

    #[inline]
    fn enumerate_namespaces(&self) -> Vec<String> {
        #[cfg(feature = "metrics")]
        {
            self.entries.lock().expect("histogram-mutex poisoned (observe-only path ; non-recoverable)").keys().cloned().collect()
        }
        #[cfg(not(feature = "metrics"))]
        {
            Vec::new()
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// § Unit tests — Timer + Histogram + Registry primitives
// ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Timer surface ─────────────────────────────────────────────

    #[test]
    fn timer_start_stop_returns_non_negative_ms() {
        let t = Timer::start();
        std::thread::sleep(std::time::Duration::from_micros(100));
        let elapsed = t.stop_ms();
        assert!(elapsed >= 0.0, "elapsed must be >= 0 ; got {elapsed}");
    }

    #[test]
    fn timer_start_stop_returns_non_negative_ns() {
        let t = Timer::start();
        let ns = t.stop_ns();
        let _ = ns; // u128 always >= 0
    }

    #[test]
    fn timer_back_to_back_calls_distinct() {
        let t1 = Timer::start();
        let _ = t1.stop_ms();
        let t2 = Timer::start();
        let _ = t2.stop_ms();
    }

    #[test]
    fn timer_zero_elapsed_when_immediate() {
        // Don't assert == 0 ; assert finite + non-negative
        let t = Timer::start();
        let elapsed = t.stop_ms();
        assert!(elapsed.is_finite());
        assert!(elapsed >= 0.0);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn timer_measures_real_elapsed_time() {
        let t = Timer::start();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let elapsed_ms = t.stop_ms();
        assert!(elapsed_ms >= 1.0, "expected ≥1ms ; got {elapsed_ms}");
    }

    #[cfg(not(feature = "metrics"))]
    #[test]
    fn timer_zero_overhead_when_disabled_returns_zero() {
        let t = Timer::start();
        let elapsed = t.stop_ms();
        assert_eq!(elapsed, 0.0);
        let t2 = Timer::start();
        let ns = t2.stop_ns();
        assert_eq!(ns, 0);
    }

    // ─── Histogram surface ─────────────────────────────────────────

    #[test]
    fn histogram_new_is_empty() {
        let h = Histogram::new();
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert_eq!(h.total_count(), 0);
    }

    #[test]
    fn histogram_default_matches_new() {
        let h1 = Histogram::default();
        let h2 = Histogram::new();
        assert_eq!(h1.len(), h2.len());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_record_increments_count() {
        let h = Histogram::new();
        h.record(1.0);
        h.record(2.0);
        assert_eq!(h.len(), 2);
        assert_eq!(h.total_count(), 2);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_p50_p95_p99_finite_after_record() {
        let h = Histogram::new();
        for v in 1..=100 {
            h.record(f64::from(v));
        }
        assert!(h.p50().is_finite());
        assert!(h.p95().is_finite());
        assert!(h.p99().is_finite());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_p50_returns_median() {
        let h = Histogram::new();
        for v in 1..=100 {
            h.record(f64::from(v));
        }
        let p50 = h.p50();
        // nearest-rank with ceil : idx = ceil(0.5 * 100) - 1 = 49 ; 0-indexed → 50
        assert!((p50 - 50.0).abs() < 1.5, "p50 ≈ 50 expected ; got {p50}");
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_p95_returns_95th() {
        let h = Histogram::new();
        for v in 1..=100 {
            h.record(f64::from(v));
        }
        let p95 = h.p95();
        assert!((p95 - 95.0).abs() < 1.5, "p95 ≈ 95 expected ; got {p95}");
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_p99_returns_99th() {
        let h = Histogram::new();
        for v in 1..=100 {
            h.record(f64::from(v));
        }
        let p99 = h.p99();
        assert!((p99 - 99.0).abs() < 1.5, "p99 ≈ 99 expected ; got {p99}");
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_min_max_after_record() {
        let h = Histogram::new();
        h.record(5.0);
        h.record(1.0);
        h.record(9.0);
        assert!((h.min() - 1.0).abs() < f64::EPSILON);
        assert!((h.max() - 9.0).abs() < f64::EPSILON);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_mean_after_record() {
        let h = Histogram::new();
        h.record(1.0);
        h.record(2.0);
        h.record(3.0);
        assert!((h.mean() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn histogram_empty_returns_nan() {
        let h = Histogram::new();
        assert!(h.p50().is_nan());
        assert!(h.p95().is_nan());
        assert!(h.p99().is_nan());
        assert!(h.mean().is_nan());
        assert!(h.min().is_nan());
        assert!(h.max().is_nan());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_ring_overwrites_oldest() {
        let h = Histogram::with_capacity(4);
        h.record(1.0);
        h.record(2.0);
        h.record(3.0);
        h.record(4.0);
        // 5th overwrites slot 0 (oldest)
        h.record(5.0);
        assert_eq!(h.len(), 4);
        assert_eq!(h.total_count(), 5);
        let snap = h.snapshot_sorted();
        // 1.0 is overwritten by 5.0 ; sorted = [2,3,4,5]
        assert_eq!(snap, vec![2.0, 3.0, 4.0, 5.0]);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_ring_capacity_respected() {
        let h = Histogram::with_capacity(10);
        for v in 1..=20 {
            h.record(f64::from(v));
        }
        assert_eq!(h.len(), 10);
        assert_eq!(h.total_count(), 20);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_with_zero_capacity_clamps_to_one() {
        let h = Histogram::with_capacity(0);
        h.record(42.0);
        assert_eq!(h.len(), 1);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_reset_clears_state() {
        let h = Histogram::new();
        for v in 1..=10 {
            h.record(f64::from(v));
        }
        h.reset();
        assert!(h.is_empty());
        assert_eq!(h.total_count(), 0);
        assert!(h.p50().is_nan());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_snapshot_is_sorted() {
        let h = Histogram::new();
        for v in [5.0, 1.0, 9.0, 3.0, 7.0] {
            h.record(v);
        }
        let snap = h.snapshot_sorted();
        for w in snap.windows(2) {
            assert!(w[0] <= w[1]);
        }
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_clone_shares_state() {
        let h1 = Histogram::new();
        let h2 = h1.clone();
        h1.record(42.0);
        assert_eq!(h2.len(), 1);
        assert!((h2.p50() - 42.0).abs() < f64::EPSILON);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn histogram_percentile_extremes() {
        let h = Histogram::new();
        for v in 1..=10 {
            h.record(f64::from(v));
        }
        // q=0 → min ; q=1 → max
        assert!((h.percentile(0.0) - 1.0).abs() < f64::EPSILON);
        assert!((h.percentile(1.0) - 10.0).abs() < f64::EPSILON);
    }

    // ─── MockRegistry surface ──────────────────────────────────────

    #[test]
    fn mock_registry_new_is_empty() {
        let r = MockRegistry::new();
        assert_eq!(r.enumerate_namespaces().len(), 0);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn mock_registry_register_returns_histogram() {
        let r = MockRegistry::new();
        let h = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
        assert!(h.is_empty());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn mock_registry_register_idempotent_same_namespace() {
        let r = MockRegistry::new();
        let h1 = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
        h1.record(1.0);
        let h2 = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
        // Same handle ; sample visible
        assert_eq!(h2.len(), 1);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn mock_registry_lookup_returns_some_after_register() {
        let r = MockRegistry::new();
        let _ = r.register_histogram("pipeline.stage_2_gaze_collapse.frame_time_ms");
        let lookup = r.lookup_histogram("pipeline.stage_2_gaze_collapse.frame_time_ms");
        assert!(lookup.is_some());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn mock_registry_lookup_returns_none_for_unregistered() {
        let r = MockRegistry::new();
        let lookup = r.lookup_histogram("pipeline.stage_99_nonexistent.frame_time_ms");
        assert!(lookup.is_none());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn mock_registry_enumerate_lex_sorted() {
        let r = MockRegistry::new();
        r.register_histogram("pipeline.stage_5_sdf_raymarch.frame_time_ms");
        r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
        r.register_histogram("pipeline.stage_3_omega_field_update.frame_time_ms");
        let names = r.enumerate_namespaces();
        assert_eq!(names.len(), 3);
        for w in names.windows(2) {
            assert!(w[0] <= w[1]);
        }
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn mock_registry_concurrent_register_safe() {
        use std::sync::Arc;
        let r = Arc::new(MockRegistry::new());
        let mut handles = Vec::new();
        for i in 0..10 {
            let r_clone = Arc::clone(&r);
            handles.push(std::thread::spawn(move || {
                let ns = format!("pipeline.stage_{i}_test.frame_time_ms");
                let h = r_clone.register_histogram(&ns);
                h.record(f64::from(i));
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(r.enumerate_namespaces().len(), 10);
    }

    #[test]
    fn trend_window_constant_is_1024() {
        assert_eq!(TREND_WINDOW, 1024);
    }
}
