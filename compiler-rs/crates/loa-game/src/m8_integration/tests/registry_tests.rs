//! Registry-surface tests — verify [`MetricsRegistry`] swap-in semantics
//! for the cssl-metrics integration (T11-D157 once landed).

use crate::m8_integration::pipeline::Pipeline;
use crate::m8_integration::{PassContext, StageId};
use crate::metrics_mock::{Histogram, MetricsRegistry, MockRegistry};
use std::sync::Arc;

#[test]
fn registry_pipeline_registers_all_12_namespaces() {
    let registry = Arc::new(MockRegistry::new());
    let _pipe = Pipeline::with_registry(Arc::clone(&registry) as Arc<dyn MetricsRegistry>);
    #[cfg(feature = "metrics")]
    {
        let names = registry.enumerate_namespaces();
        assert_eq!(names.len(), 12);
        for stage in StageId::ALL {
            assert!(names.contains(&stage.metric_namespace()));
        }
    }
}

#[cfg(feature = "metrics")]
#[test]
fn registry_lookup_returns_same_handle_post_register() {
    let registry: Arc<dyn MetricsRegistry> = Arc::new(MockRegistry::new());
    let pipe = Pipeline::with_registry(Arc::clone(&registry));
    pipe_run_one_frame(pipe);
    for stage in StageId::ALL {
        let lookup = registry.lookup_histogram(&stage.metric_namespace());
        assert!(lookup.is_some(), "{:?} histogram should be registered", stage);
    }
}

#[cfg(feature = "metrics")]
fn pipe_run_one_frame(mut pipe: Pipeline) {
    pipe.run_frame(PassContext {
        frame_n: 0,
        workload: 4,
        ..Default::default()
    });
}

#[cfg(feature = "metrics")]
#[test]
fn registry_namespace_visibility_through_pipeline() {
    let registry: Arc<dyn MetricsRegistry> = Arc::new(MockRegistry::new());
    let mut pipe = Pipeline::with_registry(Arc::clone(&registry));
    pipe.run_frame(PassContext {
        frame_n: 0,
        workload: 4,
        ..Default::default()
    });
    // Caller-side : query registry by canonical namespace.
    let h = registry
        .lookup_histogram("pipeline.stage_5_sdf_raymarch.frame_time_ms")
        .expect("stage 5 must be registered");
    assert!(!h.is_empty());
}

#[cfg(feature = "metrics")]
#[test]
fn registry_register_idempotent_returns_same_handle() {
    let r = MockRegistry::new();
    let h1 = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
    h1.record(42.0);
    let h2 = r.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
    // Same handle (Arc-cloned) ; sample visible
    assert_eq!(h2.len(), 1);
    h2.record(43.0);
    // h1 sees h2's sample too
    assert_eq!(h1.len(), 2);
}

#[test]
fn registry_separate_pipelines_independent() {
    let r1: Arc<dyn MetricsRegistry> = Arc::new(MockRegistry::new());
    let r2: Arc<dyn MetricsRegistry> = Arc::new(MockRegistry::new());
    let mut pipe1 = Pipeline::with_registry(Arc::clone(&r1));
    let _pipe2 = Pipeline::with_registry(Arc::clone(&r2));
    pipe1.run_frame(PassContext {
        frame_n: 0,
        workload: 1,
        ..Default::default()
    });
    // pipe2 not run.
    #[cfg(feature = "metrics")]
    {
        let h1 = r1
            .lookup_histogram("pipeline.stage_1_embodiment.frame_time_ms")
            .unwrap();
        let h2 = r2
            .lookup_histogram("pipeline.stage_1_embodiment.frame_time_ms")
            .unwrap();
        assert_eq!(h1.len(), 1);
        assert_eq!(h2.len(), 0);
    }
}

#[cfg(feature = "metrics")]
#[test]
fn registry_swap_in_external_registry() {
    // Demonstrate caller-side swap-in : use a MockRegistry as a stand-in
    // for cssl_metrics::Registry. Pre-register histograms before pipeline.
    let registry: Arc<dyn MetricsRegistry> = Arc::new(MockRegistry::new());
    // Pre-register a histogram (caller-side) to verify idempotence.
    let pre_h = registry.register_histogram("pipeline.stage_1_embodiment.frame_time_ms");
    pre_h.record(99.9);
    // Now construct pipeline ; it re-registers (idempotent).
    let mut pipe = Pipeline::with_registry(Arc::clone(&registry));
    pipe.run_frame(PassContext {
        frame_n: 0,
        workload: 1,
        ..Default::default()
    });
    // pre_h should now show 2 samples : 99.9 + the timer-recorded one.
    assert_eq!(pre_h.len(), 2);
}

#[cfg(feature = "metrics")]
#[test]
fn registry_arc_clone_share_state() {
    let r: Arc<dyn MetricsRegistry> = Arc::new(MockRegistry::new());
    let h_a = r.register_histogram("pipeline.stage_3_omega_field_update.frame_time_ms");
    h_a.record(1.0);
    let r_clone = Arc::clone(&r);
    let h_b = r_clone
        .lookup_histogram("pipeline.stage_3_omega_field_update.frame_time_ms")
        .unwrap();
    assert_eq!(h_b.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn registry_concurrent_access_safe_register() {
    let r: Arc<dyn MetricsRegistry> = Arc::new(MockRegistry::new());
    let mut handles = Vec::new();
    for stage in StageId::ALL {
        let r_clone = Arc::clone(&r);
        let ns = stage.metric_namespace();
        handles.push(std::thread::spawn(move || {
            let h = r_clone.register_histogram(&ns);
            for _ in 0..10 {
                h.record(1.0);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let names = r.enumerate_namespaces();
    assert_eq!(names.len(), 12);
    for stage in StageId::ALL {
        let h = r.lookup_histogram(&stage.metric_namespace()).unwrap();
        assert_eq!(h.len(), 10);
    }
}

#[test]
fn registry_pipeline_borrow_registry() {
    let pipe = Pipeline::new();
    let _registry = pipe.registry();
    // Compiles : `registry()` returns `&Arc<dyn MetricsRegistry>`.
}

#[test]
fn histogram_clone_independent_copy_until_record() {
    let h1 = Histogram::new();
    let h2 = h1.clone();
    // Empty initially
    assert_eq!(h1.len(), h2.len());
    #[cfg(feature = "metrics")]
    {
        h1.record(7.0);
        // Clone shares state via Arc<Mutex>>
        assert_eq!(h2.len(), 1);
    }
}

#[cfg(feature = "metrics")]
#[test]
fn registry_register_and_record_high_volume() {
    let r = MockRegistry::new();
    let h = r.register_histogram("pipeline.stage_4_wave_solver.frame_time_ms");
    for v in 0..5000 {
        h.record(f64::from(v));
    }
    // Capacity = TREND_WINDOW = 1024
    assert_eq!(h.len(), 1024);
    assert_eq!(h.total_count(), 5000);
}
