#![allow(clippy::float_cmp)]
#![allow(unused_imports)]

//! Wave-Jζ-1 / T11-D157 golden-bytes tests.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.3 (canonical-bucket
//!         sets) + § II.7 (schema-id stability).
//!
//! § DISCIPLINE : these tests pin byte-level invariants that downstream
//! tooling (OTLP exporter, replay-determinism check, golden-fixture
//! comparators) RELY ON. A change here is a wire-format break.

use cssl_metrics::{
    Counter, MetricSchema, BYTES_BUCKETS, COUNT_BUCKETS, LATENCY_NS_BUCKETS, PIXEL_BUCKETS,
};

#[test]
fn golden_latency_ns_buckets_canonical() {
    assert_eq!(
        LATENCY_NS_BUCKETS,
        &[
            10.0,
            100.0,
            1_000.0,
            10_000.0,
            100_000.0,
            1_000_000.0,
            10_000_000.0,
            100_000_000.0
        ]
    );
}

#[test]
fn golden_bytes_buckets_canonical() {
    assert_eq!(
        BYTES_BUCKETS,
        &[
            64.0,
            256.0,
            1_024.0,
            4_096.0,
            16_384.0,
            65_536.0,
            262_144.0,
            1_048_576.0
        ]
    );
}

#[test]
fn golden_count_buckets_canonical() {
    assert_eq!(
        COUNT_BUCKETS,
        &[1.0, 4.0, 16.0, 64.0, 256.0, 1_024.0, 4_096.0]
    );
}

#[test]
fn golden_pixel_buckets_canonical() {
    assert_eq!(
        PIXEL_BUCKETS,
        &[1.0, 4.0, 16.0, 64.0, 256.0, 1_024.0, 4_096.0, 16_384.0]
    );
}

#[test]
fn golden_latency_buckets_strictly_monotonic() {
    let bounds = LATENCY_NS_BUCKETS;
    for w in bounds.windows(2) {
        assert!(w[0] < w[1]);
    }
}

#[test]
fn golden_schema_id_engine_frame_n_stable_across_runs() {
    // Same name, same (empty) tags ⇒ schema-id is a deterministic function
    // of the inputs, so multiple instantiations produce the same id.
    let a = Counter::new("engine.frame_n").unwrap().schema_id();
    let b = Counter::new("engine.frame_n").unwrap().schema_id();
    assert_eq!(a, b);
    // The id is non-zero for non-empty input.
    assert_ne!(a, 0);
}

#[test]
fn golden_schema_id_render_stage_time_ns_stable() {
    let a = Counter::new("render.stage_time_ns").unwrap().schema_id();
    let b = Counter::new("render.stage_time_ns").unwrap().schema_id();
    assert_eq!(a, b);
}

#[test]
fn golden_metric_schema_counter_kind_emits_counter_kind() {
    let s = MetricSchema::counter("x", 1, "cite");
    assert_eq!(s.kind, cssl_metrics::MetricKind::Counter);
}

#[test]
fn golden_canonical_bucket_lengths_match_spec() {
    assert_eq!(LATENCY_NS_BUCKETS.len(), 8);
    assert_eq!(BYTES_BUCKETS.len(), 8);
    assert_eq!(COUNT_BUCKETS.len(), 7);
    assert_eq!(PIXEL_BUCKETS.len(), 8);
}
