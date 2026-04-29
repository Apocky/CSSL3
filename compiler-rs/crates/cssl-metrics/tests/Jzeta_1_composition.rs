#![allow(clippy::float_cmp)]
#![allow(unused_imports)]

//! Wave-Jζ-1 / T11-D157 composition-tests.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.7 + § IV (catalog
//!         completeness check) + cross-crate registration.
//!
//! § COVERAGE : how cssl-metrics composes with cssl-telemetry + per-subsystem
//! catalogs + the eventual catalog-completeness gate.

use cssl_metrics::{
    emit_into_ring, CompletenessReport, Counter, Gauge, Histogram, MetricError, MetricKind,
    MetricRegistry, MetricSchema, Timer, LATENCY_NS_BUCKETS,
};
use cssl_telemetry::{TelemetryKind, TelemetryRing, TelemetryScope};

#[test]
fn comp_three_subsystems_register_without_collision() {
    let r = MetricRegistry::new();
    let c = Counter::new("engine.frame_n").unwrap();
    let g = Gauge::new("engine.tick_rate_hz").unwrap();
    let h = Histogram::new("render.sdf_marches", LATENCY_NS_BUCKETS).unwrap();
    r.register("engine.frame_n", MetricKind::Counter, c.schema_id())
        .unwrap();
    r.register("engine.tick_rate_hz", MetricKind::Gauge, g.schema_id())
        .unwrap();
    r.register("render.sdf_marches", MetricKind::Histogram, h.schema_id())
        .unwrap();
    assert_eq!(r.len(), 3);
}

#[test]
fn comp_subsystem_filter_isolates_namespace() {
    let r = MetricRegistry::new();
    r.register("engine.a", MetricKind::Counter, 1).unwrap();
    r.register("engine.b", MetricKind::Gauge, 2).unwrap();
    r.register("render.x", MetricKind::Timer, 3).unwrap();
    r.register("physics.y", MetricKind::Counter, 4).unwrap();
    assert_eq!(r.entries_with_prefix("engine.").len(), 2);
    assert_eq!(r.entries_with_prefix("render.").len(), 1);
    assert_eq!(r.entries_with_prefix("physics.").len(), 1);
}

#[test]
fn comp_completeness_check_full_coverage() {
    let r = MetricRegistry::new();
    r.register("engine.frame_n", MetricKind::Counter, 1)
        .unwrap();
    r.register("engine.tick", MetricKind::Gauge, 2).unwrap();
    let cat = [
        ("engine.frame_n", MetricKind::Counter),
        ("engine.tick", MetricKind::Gauge),
    ];
    let report = r.completeness_check(&cat);
    assert!(report.is_complete());
    assert_eq!(report.coverage_fraction(), 1.0);
}

#[test]
fn comp_completeness_check_partial_coverage() {
    let r = MetricRegistry::new();
    r.register("engine.frame_n", MetricKind::Counter, 1)
        .unwrap();
    let cat = [
        ("engine.frame_n", MetricKind::Counter),
        ("engine.tick", MetricKind::Gauge),
        ("render.x", MetricKind::Timer),
    ];
    let report = r.completeness_check(&cat);
    assert!(!report.is_complete());
    assert_eq!(report.missing.len(), 2);
    assert!(report.coverage_fraction() < 1.0);
}

#[test]
fn comp_completeness_report_kind_mismatch_listed() {
    let r = MetricRegistry::new();
    r.register("a", MetricKind::Counter, 1).unwrap();
    let cat = [("a", MetricKind::Gauge)];
    let report = r.completeness_check(&cat);
    assert!(!report.is_complete());
    assert_eq!(report.mismatched.len(), 1);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn comp_emit_counter_into_telemetry_ring() {
    let ring = TelemetryRing::new(64);
    let counter = Counter::new("comp.engine.frame_n").unwrap();
    counter.inc().unwrap();
    counter.inc().unwrap();
    let schema = MetricSchema::counter("comp.engine.frame_n", counter.schema_id(), "06_l2 § III.1");
    emit_into_ring(&ring, &schema, counter.snapshot());
    let slots = ring.drain_all();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].kind, TelemetryKind::Counter.as_u16());
    assert_eq!(slots[0].scope, TelemetryScope::Counters.as_u16());
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn comp_emit_gauge_into_telemetry_ring() {
    let ring = TelemetryRing::new(64);
    let gauge = Gauge::new("comp.engine.tick_rate_hz").unwrap();
    gauge.set(60.0).unwrap();
    let schema = MetricSchema::gauge(
        "comp.engine.tick_rate_hz",
        gauge.schema_id(),
        "06_l2 § III.1",
    );
    emit_into_ring(&ring, &schema, gauge.snapshot().to_bits());
    let slots = ring.drain_all();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].kind, TelemetryKind::Sample.as_u16());
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn comp_emit_histogram_record_visible_via_ring() {
    let ring = TelemetryRing::new(64);
    let h = Histogram::new("comp.render.sdf_marches", LATENCY_NS_BUCKETS).unwrap();
    h.record(50.0).unwrap();
    let schema = MetricSchema::histogram("comp.render.sdf_marches", h.schema_id(), "06_l2 § III.3");
    emit_into_ring(&ring, &schema, 50_u64);
    assert_eq!(ring.total_pushed(), 1);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn comp_emit_timer_into_ring() {
    let ring = TelemetryRing::new(64);
    let t = Timer::new("comp.render.stage_time_ns").unwrap();
    let h = t.start();
    h.commit();
    let schema = MetricSchema::timer("comp.render.stage_time_ns", t.schema_id(), "06_l2 § III.3");
    emit_into_ring(&ring, &schema, t.last_ns());
    assert_eq!(ring.total_pushed(), 1);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn comp_ring_overflow_does_not_block() {
    let ring = TelemetryRing::new(2);
    let s = MetricSchema::counter("comp.overflow", 1, "cite");
    for _ in 0..10 {
        emit_into_ring(&ring, &s, 1);
    }
    assert_eq!(ring.total_pushed(), 10);
    assert!(ring.overflow_count() >= 1);
}

#[test]
fn comp_strict_mode_logical_clock_indirection() {
    cssl_metrics::set_logical_clock(0, 0);
    let (f, s) = cssl_metrics::logical_clock_cursor();
    assert_eq!(f, 0);
    assert_eq!(s, 0);
    cssl_metrics::set_logical_clock(7, 42);
    let (f, s) = cssl_metrics::logical_clock_cursor();
    assert_eq!(f, 7);
    assert_eq!(s, 42);
}

#[test]
fn comp_completeness_report_coverage_fraction_quarter() {
    let r = MetricRegistry::new();
    r.register("a", MetricKind::Counter, 1).unwrap();
    let cat = [
        ("a", MetricKind::Counter),
        ("b", MetricKind::Counter),
        ("c", MetricKind::Counter),
        ("d", MetricKind::Counter),
    ];
    let report = r.completeness_check(&cat);
    assert!((report.coverage_fraction() - 0.25).abs() < 1e-9);
}

#[test]
fn comp_completeness_report_default() {
    let report: CompletenessReport = Default::default();
    assert!(report.is_complete());
    assert_eq!(report.coverage_fraction(), 1.0);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn comp_cross_crate_registration_error_path() {
    let r = MetricRegistry::new();
    let a = Counter::new("comp.collision").unwrap();
    let b = Gauge::new("comp.collision").unwrap();
    r.register("comp.collision", MetricKind::Counter, a.schema_id())
        .unwrap();
    // Same name + Gauge kind ⇒ collision (kind mismatch).
    let res = r.register("comp.collision", MetricKind::Gauge, b.schema_id());
    assert!(matches!(res, Err(MetricError::SchemaCollision { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn comp_full_pipeline_engine_metrics() {
    // A simulated pipeline that registers + records + emits.
    let r = MetricRegistry::new();
    let ring = TelemetryRing::new(128);

    let frame_n = Counter::new("pipe.engine.frame_n").unwrap();
    let tick_rate = Gauge::new("pipe.engine.tick_rate_hz").unwrap();
    let frame_time = Timer::new("pipe.engine.frame_time_ns").unwrap();

    r.register(
        "pipe.engine.frame_n",
        MetricKind::Counter,
        frame_n.schema_id(),
    )
    .unwrap();
    r.register(
        "pipe.engine.tick_rate_hz",
        MetricKind::Gauge,
        tick_rate.schema_id(),
    )
    .unwrap();
    r.register(
        "pipe.engine.frame_time_ns",
        MetricKind::Timer,
        frame_time.schema_id(),
    )
    .unwrap();

    // Run a few simulated frames.
    for _ in 0..5 {
        frame_n.inc().unwrap();
        tick_rate.set(60.0).unwrap();
        let h = frame_time.start();
        h.commit();

        emit_into_ring(
            &ring,
            &MetricSchema::counter("pipe.engine.frame_n", frame_n.schema_id(), "cite"),
            frame_n.snapshot(),
        );
    }
    assert_eq!(frame_n.snapshot(), 5);
    assert_eq!(ring.total_pushed(), 5);
    assert_eq!(r.len(), 3);
}
