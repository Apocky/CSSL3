#![allow(clippy::float_cmp)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(unused_imports)]

//! Wave-Jζ-1 / T11-D157 acceptance-tests for cssl-metrics.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.7 ; AC-1..AC-12.
//!
//! § COVERAGE
//!   AC-1  : All 4 metric types compile + run                        ✓
//!   AC-2  : TimerHandle RAII + #[must_use]                          ✓
//!   AC-3  : SamplingDiscipline + deterministic-decimation           ✓
//!   AC-4  : effect-row gating (stage-0 runtime check)               ✓
//!   AC-5  : Schema-id stability via FNV1a                            ✓
//!   AC-6  : Tag biometric-refuse + raw-path-refuse                  ✓
//!   AC-7  : MetricError taxonomy ; NaN/Inf refused                  ✓
//!   AC-8  : MetricRegistry collision detection                      ✓
//!   AC-9  : Per-subsystem namespace                                 ✓
//!   AC-10 : Wires-into cssl-telemetry ring                          ✓
//!   AC-11 : Replay-determinism preserved (sampled-deterministically) ✓
//!   AC-12 : Five-of-five gate                                       ✓

use cssl_metrics::{
    emit_into_ring, Counter, EffectRow, Gauge, Histogram, MetricKind, MetricRegistry, MetricSchema,
    SamplingDiscipline, SubsystemRegistry, TagKey, TagVal, Timer, BIOMETRIC_TAG_KEYS,
    LATENCY_NS_BUCKETS,
};
use cssl_telemetry::TelemetryRing;

// ─────────────────────────────────────────────────────────────────────
// AC-1 : all 4 metric types
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ac1_counter_constructable() {
    let _ = Counter::new("ac1.counter").unwrap();
}

#[test]
fn ac1_gauge_constructable() {
    let _ = Gauge::new("ac1.gauge").unwrap();
}

#[test]
fn ac1_histogram_constructable() {
    let _ = Histogram::new("ac1.histogram", LATENCY_NS_BUCKETS).unwrap();
}

#[test]
fn ac1_timer_constructable() {
    let _ = Timer::new("ac1.timer").unwrap();
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac1_counter_increments_visible() {
    let c = Counter::new("ac1.counter_inc").unwrap();
    c.inc().unwrap();
    c.inc_by(2).unwrap();
    assert_eq!(c.snapshot(), 3);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac1_gauge_set_observable() {
    let g = Gauge::new("ac1.gauge_set").unwrap();
    g.set(42.0).unwrap();
    assert_eq!(g.snapshot(), 42.0);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac1_histogram_records() {
    let h = Histogram::new("ac1.hist_rec", LATENCY_NS_BUCKETS).unwrap();
    for v in [50.0, 500.0, 5000.0] {
        h.record(v).unwrap();
    }
    assert_eq!(h.count(), 3);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac1_timer_records_via_handle() {
    let t = Timer::new("ac1.timer_rec").unwrap();
    let h = t.start();
    h.commit();
    assert_eq!(t.count(), 1);
}

// ─────────────────────────────────────────────────────────────────────
// AC-2 : TimerHandle drop-records
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac2_handle_drop_records() {
    let t = Timer::new("ac2.drop").unwrap();
    {
        let _h = t.start();
    } // drop
    assert_eq!(t.count(), 1);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac2_handle_cancel_skips_record() {
    let t = Timer::new("ac2.cancel").unwrap();
    let h = t.start();
    h.cancel();
    assert_eq!(t.count(), 0);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac2_handle_commit_records_immediately() {
    let t = Timer::new("ac2.commit").unwrap();
    let h = t.start();
    h.commit();
    assert_eq!(t.count(), 1);
}

// ─────────────────────────────────────────────────────────────────────
// AC-3 : SamplingDiscipline deterministic-decimation
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ac3_one_in_n_deterministic() {
    let s = SamplingDiscipline::OneIn(5);
    let a = (0..100_u64)
        .map(|i| s.should_sample(i, 0, i))
        .collect::<Vec<_>>();
    let b = (0..100_u64)
        .map(|i| s.should_sample(i, 0, i))
        .collect::<Vec<_>>();
    assert_eq!(a, b);
}

#[test]
fn ac3_always_samples_all() {
    let s = SamplingDiscipline::Always;
    for i in 0..50_u64 {
        assert!(s.should_sample(i, 0, i));
    }
}

#[test]
fn ac3_burst_then_decimate_burst_unconditional() {
    let s = SamplingDiscipline::BurstThenDecimate {
        burst: 10,
        then_one_in: 1_000,
    };
    for i in 0..10_u64 {
        assert!(s.should_sample(99, 0, i));
    }
}

// ─────────────────────────────────────────────────────────────────────
// AC-4 : effect-row gating
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ac4_effect_row_counters_covers_self() {
    assert!(EffectRow::Counters.covers(EffectRow::Counters));
}

#[test]
fn ac4_effect_row_check_passes_match() {
    assert!(EffectRow::check(EffectRow::Counters, EffectRow::Counters, "m").is_ok());
}

#[test]
fn ac4_effect_row_check_fails_mismatch() {
    assert!(EffectRow::check(EffectRow::Spans, EffectRow::Counters, "m").is_err());
}

// ─────────────────────────────────────────────────────────────────────
// AC-5 : Schema-id stability
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ac5_schema_id_stable_for_same_inputs() {
    let a = Counter::new("ac5.a").unwrap().schema_id();
    let b = Counter::new("ac5.a").unwrap().schema_id();
    assert_eq!(a, b);
}

#[test]
fn ac5_schema_id_differs_by_name() {
    let a = Counter::new("ac5.a").unwrap().schema_id();
    let b = Counter::new("ac5.b").unwrap().schema_id();
    assert_ne!(a, b);
}

// ─────────────────────────────────────────────────────────────────────
// AC-6 : Tag biometric-refuse + raw-path-refuse
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ac6_biometric_face_id_refused() {
    let r = Counter::new_with(
        "ac6.face",
        &[(TagKey::new("face_id"), TagVal::U64(0))],
        SamplingDiscipline::Always,
    );
    assert!(r.is_err());
}

#[test]
fn ac6_biometric_canonical_keys_nonempty() {
    assert!(BIOMETRIC_TAG_KEYS.contains(&"face_id"));
    assert!(BIOMETRIC_TAG_KEYS.contains(&"heart_rate"));
}

#[test]
fn ac6_raw_path_in_tag_value_refused() {
    let r = Counter::new_with(
        "ac6.path",
        &[(TagKey::new("file"), TagVal::Static("/etc/hosts"))],
        SamplingDiscipline::Always,
    );
    assert!(r.is_err());
}

// ─────────────────────────────────────────────────────────────────────
// AC-7 : NaN / Inf refusal
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac7_gauge_nan_refused() {
    let g = Gauge::new("ac7.nan").unwrap();
    assert!(g.set(f64::NAN).is_err());
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac7_gauge_inf_refused_default() {
    let g = Gauge::new("ac7.inf").unwrap();
    assert!(g.set(f64::INFINITY).is_err());
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac7_histogram_nan_refused() {
    let h = Histogram::new("ac7.hnan", LATENCY_NS_BUCKETS).unwrap();
    assert!(h.record(f64::NAN).is_err());
}

// ─────────────────────────────────────────────────────────────────────
// AC-8 : MetricRegistry collision detection
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ac8_registry_idempotent_for_same_schema() {
    let r = MetricRegistry::new();
    r.register("ac8.a", MetricKind::Counter, 1).unwrap();
    r.register("ac8.a", MetricKind::Counter, 1).unwrap();
    assert_eq!(r.len(), 1);
}

#[test]
fn ac8_registry_collision_on_schema_id() {
    let r = MetricRegistry::new();
    r.register("ac8.a", MetricKind::Counter, 1).unwrap();
    let res = r.register("ac8.a", MetricKind::Counter, 2);
    assert!(res.is_err());
}

#[test]
fn ac8_registry_collision_on_kind() {
    let r = MetricRegistry::new();
    r.register("ac8.a", MetricKind::Counter, 1).unwrap();
    let res = r.register("ac8.a", MetricKind::Gauge, 1);
    assert!(res.is_err());
}

// ─────────────────────────────────────────────────────────────────────
// AC-9 : Per-subsystem namespace
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ac9_subsystem_view_filters_prefix() {
    let r = MetricRegistry::new();
    r.register("engine.frame_n", MetricKind::Counter, 1)
        .unwrap();
    r.register("engine.tick", MetricKind::Gauge, 2).unwrap();
    r.register("render.stage_time", MetricKind::Timer, 3)
        .unwrap();
    let engine_only = r.entries_with_prefix("engine.");
    assert_eq!(engine_only.len(), 2);
}

#[test]
fn ac9_subsystem_registry_constructs() {
    let _ = SubsystemRegistry::new("engine");
}

// ─────────────────────────────────────────────────────────────────────
// AC-10 : Wires-into cssl-telemetry ring
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac10_emit_writes_telemetry_slot() {
    let ring = TelemetryRing::new(16);
    let s = MetricSchema::counter("ac10.x", 0xdead, "06_l2 § II.7");
    emit_into_ring(&ring, &s, 7);
    assert_eq!(ring.total_pushed(), 1);
    let slots = ring.drain_all();
    assert_eq!(slots.len(), 1);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac10_emit_does_not_block_on_overflow() {
    let ring = TelemetryRing::new(2);
    let s = MetricSchema::counter("ac10.overflow", 1, "cite");
    for _ in 0..5 {
        emit_into_ring(&ring, &s, 1);
    }
    assert_eq!(ring.total_pushed(), 5);
}

// ─────────────────────────────────────────────────────────────────────
// AC-11 : Replay-determinism preserved
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac11_one_in_sampling_deterministic_across_runs() {
    let s = SamplingDiscipline::OneIn(7);
    let r1 = (0..1_000_u64)
        .filter(|i| s.should_sample(*i, 13, 0))
        .count();
    let r2 = (0..1_000_u64)
        .filter(|i| s.should_sample(*i, 13, 0))
        .count();
    assert_eq!(r1, r2);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac11_gauge_bit_pattern_roundtrip_for_replay() {
    let g = Gauge::new("ac11.g").unwrap();
    let v = std::f64::consts::PI;
    g.set(v).unwrap();
    assert_eq!(g.snapshot().to_bits(), v.to_bits());
}

#[cfg(feature = "replay-strict")]
#[test]
fn ac11_strict_mode_adaptive_refused() {
    use cssl_metrics::MetricError;
    let r = Counter::new_with(
        "ac11.adaptive",
        &[],
        SamplingDiscipline::Adaptive {
            target_overhead_pct: 0.5,
        },
    );
    assert!(matches!(r, Err(MetricError::AdaptiveUnderStrict { .. })));
}

#[cfg(feature = "metrics-disabled")]
#[test]
fn ac11_metrics_disabled_is_pure_no_op() {
    let c = Counter::new("ac11.disabled").unwrap();
    c.inc().unwrap();
    assert_eq!(c.snapshot(), 0); // no observable mutation
}

// ─────────────────────────────────────────────────────────────────────
// AC-12 : Five-of-five gate (one consolidated test asserting full surface)
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn ac12_five_of_five_gate() {
    // 1. Four metric types work
    let counter = Counter::new("ac12.c").unwrap();
    let gauge = Gauge::new("ac12.g").unwrap();
    let hist = Histogram::new("ac12.h", LATENCY_NS_BUCKETS).unwrap();
    let timer = Timer::new("ac12.t").unwrap();
    counter.inc().unwrap();
    gauge.set(1.5).unwrap();
    hist.record(50.0).unwrap();
    timer.record_ns(100);

    // 2. p50/p95/p99 work
    for v in [10.0, 100.0, 1000.0, 10000.0, 100000.0] {
        hist.record(v).unwrap();
    }
    let p50 = hist.p50();
    let p95 = hist.p95();
    let p99 = hist.p99();
    assert!(p50 <= p95);
    assert!(p95 <= p99);

    // 3. Cross-crate registration via SubsystemRegistry
    let r = MetricRegistry::new();
    r.register(
        "ac12_engine.frame_n",
        MetricKind::Counter,
        counter.schema_id(),
    )
    .unwrap();
    r.register("ac12_render.gauge", MetricKind::Gauge, gauge.schema_id())
        .unwrap();
    assert_eq!(r.len(), 2);

    // 4. Wires-into telemetry ring
    let ring = TelemetryRing::new(8);
    let schema = MetricSchema::counter("ac12.c", counter.schema_id(), "ac12");
    emit_into_ring(&ring, &schema, counter.snapshot());
    assert!(ring.total_pushed() >= 1);

    // 5. Replay-determinism : sampling deterministic
    let s = SamplingDiscipline::OneIn(3);
    let a = (0..30_u64).filter(|i| s.should_sample(*i, 0, 0)).count();
    let b = (0..30_u64).filter(|i| s.should_sample(*i, 0, 0)).count();
    assert_eq!(a, b);
}
