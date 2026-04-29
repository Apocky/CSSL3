//! Zero-overhead tests — verify that when `feature = "metrics"` is disabled,
//! the instrumentation reduces to no-op shims, the histograms hold no data,
//! and the binary-size diff (caller-side, manual `cargo build` check) shows
//! identical machine-code to a non-instrumented build.
//!
//! § AUTOMATED CHECKS (this file)
//!   - [`Timer::stop_ms`] returns 0.0 when feature off
//!   - [`Histogram::record`] is no-op when feature off
//!   - [`Histogram::p50`] / `p95` / `p99` return NaN when feature off
//!   - [`MockRegistry::register_histogram`] returns empty handle when off
//!
//! § MANUAL CHECK (CI gate)
//!   The binary-size diff is verified by the workspace gate :
//!     cargo build --release --no-default-features
//!     cargo build --release --features metrics
//!   The "metrics-on" build is permitted to be larger (Histogram allocs +
//!   Mutex impls) ; the "metrics-off" build proves zero-overhead because
//!   the instrumentation block compiles to nothing.
//!
//!   See `compiler-rs/scripts/d158_binary_size_diff.sh` (deferred — manual).

// Some imports are used only by the metrics-on-baseline submodule, others
// only by the metrics-off submodule ; allow unused at the outer layer to
// keep both submodules compiling cleanly.
#[allow(unused_imports)]
use crate::m8_integration::pipeline::Pipeline;
#[allow(unused_imports)]
use crate::m8_integration::{PassContext, StageId};
#[allow(unused_imports)]
use crate::metrics_mock::{Histogram, MetricsRegistry, MockRegistry, Timer};

fn ctx_for(frame: u64, workload: u32) -> PassContext {
    PassContext {
        frame_n: frame,
        workload,
        ..Default::default()
    }
}

#[cfg(not(feature = "metrics"))]
mod metrics_off {
    use super::*;

    #[test]
    fn timer_stop_ms_returns_zero() {
        let t = Timer::start();
        let elapsed = t.stop_ms();
        assert_eq!(elapsed, 0.0);
    }

    #[test]
    fn timer_stop_ns_returns_zero() {
        let t = Timer::start();
        let ns = t.stop_ns();
        assert_eq!(ns, 0);
    }

    #[test]
    fn histogram_record_is_noop() {
        let h = Histogram::new();
        for v in 1..=100 {
            h.record(f64::from(v));
        }
        assert_eq!(h.len(), 0);
        assert_eq!(h.total_count(), 0);
    }

    #[test]
    fn histogram_p50_returns_nan() {
        let h = Histogram::new();
        h.record(42.0);
        assert!(h.p50().is_nan());
    }

    #[test]
    fn histogram_p95_returns_nan() {
        let h = Histogram::new();
        h.record(42.0);
        assert!(h.p95().is_nan());
    }

    #[test]
    fn histogram_p99_returns_nan() {
        let h = Histogram::new();
        h.record(42.0);
        assert!(h.p99().is_nan());
    }

    #[test]
    fn histogram_min_max_mean_return_nan() {
        let h = Histogram::new();
        h.record(42.0);
        assert!(h.min().is_nan());
        assert!(h.max().is_nan());
        assert!(h.mean().is_nan());
    }

    #[test]
    fn histogram_snapshot_is_empty() {
        let h = Histogram::new();
        h.record(42.0);
        assert!(h.snapshot_sorted().is_empty());
    }

    #[test]
    fn registry_register_returns_empty_handle() {
        let r = MockRegistry::new();
        let h = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
        h.record(1.0);
        assert!(h.is_empty());
    }

    #[test]
    fn registry_lookup_returns_none() {
        let r = MockRegistry::new();
        let _ = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
        assert!(r
            .lookup_histogram("pipeline.stage_1_embodiment.frame_time_ms")
            .is_none());
    }

    #[test]
    fn registry_enumerate_is_empty() {
        let r = MockRegistry::new();
        let _ = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
        assert!(r.enumerate_namespaces().is_empty());
    }

    #[test]
    fn pipeline_runs_with_no_overhead_observable() {
        // With metrics off, calling `run_frame` should still execute the
        // synthetic workload (verified by the deterministic-output tests
        // in determinism_tests.rs), but produce no histogram data.
        let mut pipe = Pipeline::new();
        for f in 0..10 {
            pipe.run_frame(ctx_for(f, 4));
        }
        assert_eq!(pipe.frame_count(), 10);
        // Total samples = 0 because Histogram::record is no-op.
        assert_eq!(pipe.total_samples_recorded(), 0);
    }

    #[test]
    fn pipeline_p50_reads_nan_when_metrics_off() {
        let mut pipe = Pipeline::new();
        for f in 0..5 {
            pipe.run_frame(ctx_for(f, 2));
        }
        for stage in StageId::ALL {
            assert!(pipe.p50_ms(stage).is_nan());
            assert!(pipe.p95_ms(stage).is_nan());
            assert!(pipe.p99_ms(stage).is_nan());
        }
    }
}

#[cfg(feature = "metrics")]
mod metrics_on_baseline {
    //! Baseline tests : when metrics-feature is ON, instrumentation works.
    //! These mirror the off-suite for direct A/B comparison.

    use super::*;

    #[test]
    fn timer_stop_ms_non_negative() {
        let t = Timer::start();
        let elapsed = t.stop_ms();
        assert!(elapsed >= 0.0);
        assert!(elapsed.is_finite());
    }

    #[test]
    fn histogram_record_increments_count() {
        let h = Histogram::new();
        for v in 1..=100 {
            h.record(f64::from(v));
        }
        assert_eq!(h.len(), 100);
        assert_eq!(h.total_count(), 100);
    }

    #[test]
    fn histogram_p50_finite_after_record() {
        let h = Histogram::new();
        h.record(42.0);
        assert!(h.p50().is_finite());
    }

    #[test]
    fn pipeline_total_samples_nonzero_when_on() {
        let mut pipe = Pipeline::new();
        for f in 0..5 {
            pipe.run_frame(ctx_for(f, 2));
        }
        // 5 frames × 12 stages = 60
        assert_eq!(pipe.total_samples_recorded(), 60);
    }
}

// Tests that work in both feature configurations :

#[test]
fn timer_start_ergonomics_compile() {
    // Smoke-test : Timer::start() works in any feature config.
    let _ = Timer::start();
}

#[test]
fn histogram_constructors_work_both_configs() {
    let _ = Histogram::new();
    let _ = Histogram::with_capacity(64);
    let _ = Histogram::default();
}

#[test]
fn pipeline_constructors_work_both_configs() {
    let _ = Pipeline::new();
    let _ = Pipeline::default();
}

#[test]
fn pipeline_run_frame_works_both_configs() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    assert_eq!(pipe.frame_count(), 1);
}

#[test]
fn timer_self_consumes_no_state_leak() {
    // Timer::stop_ms takes self by value — no reuse possible.
    let t = Timer::start();
    let _ = t.stop_ms();
    // `t` is consumed ; this confirms the surface guarantees one-shot use.
}

#[test]
fn histogram_capacity_constructor_idempotent() {
    let h1 = Histogram::with_capacity(128);
    let h2 = Histogram::with_capacity(128);
    // Both empty initially.
    assert_eq!(h1.len(), h2.len());
}

#[test]
fn pipeline_namespaces_present_regardless_of_feature() {
    let pipe = Pipeline::new();
    let ns = pipe.registered_stage_namespaces();
    assert_eq!(ns.len(), 12);
}
