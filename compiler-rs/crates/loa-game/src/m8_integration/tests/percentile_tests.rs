//! Percentile-read tests — verify p50/p95/p99 readable per-stage and
//! return monotone-increasing values for monotone-distributed samples.

use crate::m8_integration::pipeline::Pipeline;
use crate::m8_integration::{PassContext, StageId};
use crate::metrics_mock::Histogram;

fn ctx_for(frame: u64, workload: u32) -> PassContext {
    PassContext {
        frame_n: frame,
        workload,
        ..Default::default()
    }
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_p50_p95_p99_finite_after_frames() {
    let mut pipe = Pipeline::new();
    for f in 0..200 {
        pipe.run_frame(ctx_for(f, 8));
    }
    for stage in StageId::ALL {
        let p50 = pipe.p50_ms(stage);
        let p95 = pipe.p95_ms(stage);
        let p99 = pipe.p99_ms(stage);
        assert!(p50.is_finite(), "stage {:?} p50 not finite", stage);
        assert!(p95.is_finite(), "stage {:?} p95 not finite", stage);
        assert!(p99.is_finite(), "stage {:?} p99 not finite", stage);
    }
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_monotone_p50_le_p95_le_p99() {
    // Direct histogram test (bypass synthetic-noise of timer).
    let h = Histogram::new();
    for v in 1..=1000 {
        h.record(f64::from(v));
    }
    let p50 = h.p50();
    let p95 = h.p95();
    let p99 = h.p99();
    assert!(p50 <= p95, "p50={p50} ≤ p95={p95} expected");
    assert!(p95 <= p99, "p95={p95} ≤ p99={p99} expected");
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_synthetic_distribution_p50_at_median() {
    let h = Histogram::new();
    for v in 1..=100 {
        h.record(f64::from(v));
    }
    let p50 = h.p50();
    assert!(
        (p50 - 50.0).abs() < 2.0,
        "p50 ≈ 50 expected for 1..=100 ; got {p50}"
    );
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_synthetic_distribution_p95_at_95th() {
    let h = Histogram::new();
    for v in 1..=100 {
        h.record(f64::from(v));
    }
    let p95 = h.p95();
    assert!(
        (p95 - 95.0).abs() < 2.0,
        "p95 ≈ 95 expected for 1..=100 ; got {p95}"
    );
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_synthetic_distribution_p99_at_99th() {
    let h = Histogram::new();
    for v in 1..=100 {
        h.record(f64::from(v));
    }
    let p99 = h.p99();
    assert!(
        (p99 - 99.0).abs() < 2.0,
        "p99 ≈ 99 expected for 1..=100 ; got {p99}"
    );
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_pipeline_p50_per_stage_finite() {
    let mut pipe = Pipeline::new();
    for f in 0..30 {
        pipe.run_frame(ctx_for(f, 4));
    }
    for stage in StageId::ALL {
        let p50 = pipe.p50_ms(stage);
        assert!(p50.is_finite() && p50 >= 0.0);
    }
}

#[test]
fn percentile_pipeline_no_data_returns_nan() {
    let pipe = Pipeline::new();
    for stage in StageId::ALL {
        // No frames run yet
        let p50 = pipe.p50_ms(stage);
        let p95 = pipe.p95_ms(stage);
        let p99 = pipe.p99_ms(stage);
        // Without metrics : NaN ; with metrics : NaN (empty histograms)
        assert!(
            p50.is_nan(),
            "stage {:?} p50 should be NaN ; got {p50}",
            stage
        );
        assert!(
            p95.is_nan(),
            "stage {:?} p95 should be NaN ; got {p95}",
            stage
        );
        assert!(
            p99.is_nan(),
            "stage {:?} p99 should be NaN ; got {p99}",
            stage
        );
    }
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_reset_clears_to_nan() {
    let mut pipe = Pipeline::new();
    for f in 0..10 {
        pipe.run_frame(ctx_for(f, 2));
    }
    pipe.reset_histograms();
    for stage in StageId::ALL {
        assert!(pipe.p50_ms(stage).is_nan());
    }
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_uniform_distribution_quartiles() {
    let h = Histogram::with_capacity(2048);
    for v in 0..1000 {
        h.record(f64::from(v));
    }
    let p25 = h.percentile(0.25);
    let p50 = h.percentile(0.50);
    let p75 = h.percentile(0.75);
    assert!(p25 < p50);
    assert!(p50 < p75);
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_constant_distribution_all_equal() {
    let h = Histogram::new();
    for _ in 0..100 {
        h.record(7.5);
    }
    assert!((h.p50() - 7.5).abs() < f64::EPSILON);
    assert!((h.p95() - 7.5).abs() < f64::EPSILON);
    assert!((h.p99() - 7.5).abs() < f64::EPSILON);
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_skewed_distribution_p99_heavy_tail() {
    // 99 small values + 1 outlier
    let h = Histogram::new();
    for _ in 0..99 {
        h.record(1.0);
    }
    h.record(1000.0);
    let p50 = h.p50();
    let p99 = h.p99();
    assert!((p50 - 1.0).abs() < f64::EPSILON);
    assert!(p99 >= 1.0);
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_ring_overflow_drops_oldest() {
    // Capacity 10, record 30 ; only last 10 visible to percentile.
    let h = Histogram::with_capacity(10);
    for v in 1..=30 {
        h.record(f64::from(v));
    }
    let snap = h.snapshot_sorted();
    assert_eq!(snap.len(), 10);
    // Last 10 records are 21..=30 ; smallest = 21
    assert!((snap[0] - 21.0).abs() < f64::EPSILON);
    assert!((snap[9] - 30.0).abs() < f64::EPSILON);
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_each_stage_independent_aggregation() {
    let mut pipe = Pipeline::new();
    // Run frames with varying workload so different stages have different
    // synthetic-loop counts, but per-stage histograms remain independent.
    for f in 0..20 {
        pipe.run_frame(ctx_for(f, 8));
    }
    // Each stage's histogram is its own ; check len-distinct from total
    for stage in StageId::ALL {
        let h = pipe.histogram(stage);
        assert_eq!(h.len(), 20);
    }
    // Total = 12 * 20 = 240
    assert_eq!(pipe.total_samples_recorded(), 240);
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_p50_reads_via_pipeline_match_via_histogram() {
    let mut pipe = Pipeline::new();
    for f in 0..50 {
        pipe.run_frame(ctx_for(f, 4));
    }
    for stage in StageId::ALL {
        let pipe_p50 = pipe.p50_ms(stage);
        let hist_p50 = pipe.histogram(stage).p50();
        if pipe_p50.is_finite() && hist_p50.is_finite() {
            assert!((pipe_p50 - hist_p50).abs() < f64::EPSILON);
        }
    }
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_p95_reads_via_pipeline_match_via_histogram() {
    let mut pipe = Pipeline::new();
    for f in 0..50 {
        pipe.run_frame(ctx_for(f, 4));
    }
    for stage in StageId::ALL {
        let pipe_p = pipe.p95_ms(stage);
        let hist_p = pipe.histogram(stage).p95();
        if pipe_p.is_finite() && hist_p.is_finite() {
            assert!((pipe_p - hist_p).abs() < f64::EPSILON);
        }
    }
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_p99_reads_via_pipeline_match_via_histogram() {
    let mut pipe = Pipeline::new();
    for f in 0..50 {
        pipe.run_frame(ctx_for(f, 4));
    }
    for stage in StageId::ALL {
        let pipe_p = pipe.p99_ms(stage);
        let hist_p = pipe.histogram(stage).p99();
        if pipe_p.is_finite() && hist_p.is_finite() {
            assert!((pipe_p - hist_p).abs() < f64::EPSILON);
        }
    }
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_trend_window_default_is_1024() {
    use crate::metrics_mock::TREND_WINDOW;
    assert_eq!(TREND_WINDOW, 1024);
}

#[cfg(feature = "metrics")]
#[test]
fn percentile_long_run_caps_at_trend_window() {
    let mut pipe = Pipeline::new();
    // 2000 frames > TREND_WINDOW (1024)
    for f in 0..2000 {
        pipe.run_frame(ctx_for(f, 1));
    }
    for stage in StageId::ALL {
        let h = pipe.histogram(stage);
        assert!(
            h.len() <= 1024,
            "stage {:?} len={} should be ≤ 1024",
            stage,
            h.len()
        );
        assert_eq!(h.total_count(), 2000);
    }
}
