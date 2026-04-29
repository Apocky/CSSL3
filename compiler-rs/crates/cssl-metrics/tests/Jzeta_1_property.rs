#![allow(clippy::float_cmp)]
#![allow(clippy::manual_range_contains)]
#![allow(unused_imports)]

//! Wave-Jζ-1 / T11-D157 property-tests for cssl-metrics.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.5.
//!
//! § COVERAGE : property-style assertions over generated input ranges.
//!   - Counter monotonicity
//!   - Gauge bit-pattern roundtrip ∀ f64 \ NaN
//!   - Histogram sum + count saturating-monoid
//!   - Sampling decimation frequency-bound
//!   - Tag-validation idempotency

use cssl_metrics::{
    Counter, Gauge, Histogram, SamplingDiscipline, TagKey, TagVal, COUNT_BUCKETS,
    LATENCY_NS_BUCKETS,
};

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_counter_monotonic_under_inc() {
    let c = Counter::new("prop.mono").unwrap();
    let mut last = 0_u64;
    for i in 1..=100_u64 {
        c.inc_by(i).unwrap();
        let snap = c.snapshot();
        assert!(snap >= last);
        last = snap;
    }
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_counter_inc_count_eq_inc_by_one_count() {
    let c = Counter::new("prop.inc_eq").unwrap();
    for _ in 0..1_000 {
        c.inc().unwrap();
    }
    assert_eq!(c.snapshot(), 1_000);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_gauge_bit_pattern_roundtrip_subset() {
    let g = Gauge::new("prop.bits").unwrap();
    let cases = [
        0.0_f64,
        -0.0,
        1.0,
        -1.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        std::f64::consts::PI,
        std::f64::consts::E,
        -std::f64::consts::PI,
        1e-308,
        1e308,
        f64::EPSILON,
    ];
    for v in cases {
        g.set(v).unwrap();
        assert_eq!(g.snapshot().to_bits(), v.to_bits(), "v = {v}");
    }
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_histogram_sum_eq_sum_of_records() {
    let h = Histogram::new("prop.sum", COUNT_BUCKETS).unwrap();
    let mut expected = 0.0_f64;
    for i in 1..=50_u32 {
        let v = f64::from(i);
        h.record(v).unwrap();
        expected += v;
    }
    assert!((h.sum() - expected).abs() < 1e-6);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_histogram_count_eq_record_count() {
    let h = Histogram::new("prop.count", COUNT_BUCKETS).unwrap();
    for i in 0..200_u32 {
        h.record(f64::from(i)).unwrap();
    }
    assert_eq!(h.count(), 200);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_histogram_total_bucket_count_eq_count() {
    let h = Histogram::new("prop.totals", COUNT_BUCKETS).unwrap();
    for i in 0..1_000_u32 {
        h.record(f64::from(i % 50)).unwrap();
    }
    let snap = h.snapshot();
    let total: u64 = snap.counts.iter().sum();
    assert_eq!(total, h.count());
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_histogram_p50_le_p95_le_p99() {
    let h = Histogram::new("prop.p", LATENCY_NS_BUCKETS).unwrap();
    for v in [10.0, 100.0, 1_000.0, 10_000.0, 100_000.0, 1_000_000.0] {
        for _ in 0..30 {
            h.record(v).unwrap();
        }
    }
    let p50 = h.p50();
    let p95 = h.p95();
    let p99 = h.p99();
    assert!(p50 <= p95);
    assert!(p95 <= p99);
}

#[test]
fn property_sampling_one_in_n_within_decimation_bound() {
    for n in [2_u32, 3, 5, 7, 11, 13, 100] {
        let s = SamplingDiscipline::OneIn(n);
        let count = (0..10_000_u64)
            .filter(|i| s.should_sample(*i, 0, *i))
            .count();
        let expected = 10_000 / (n as usize);
        let lo = expected.saturating_sub(expected / 10);
        let hi = expected + expected / 10;
        assert!(
            count >= lo && count <= hi.max(1),
            "n={n} expected≈{expected} got={count}"
        );
    }
}

#[test]
fn property_sampling_idempotent_under_repeated_call() {
    let s = SamplingDiscipline::OneIn(11);
    for i in 0..100_u64 {
        let a = s.should_sample(i, 31, 0);
        let b = s.should_sample(i, 31, 0);
        assert_eq!(a, b, "i={i}");
    }
}

#[test]
fn property_tag_validation_idempotent() {
    let key = TagKey::new("mode");
    let val = TagVal::Static("60");
    for _ in 0..100 {
        cssl_metrics::validate_pair("m", key, val).unwrap();
    }
}

#[test]
fn property_biometric_canonical_keys_all_refused() {
    for k in cssl_metrics::BIOMETRIC_TAG_KEYS {
        let key = TagKey::new(k);
        assert!(key.is_biometric(), "{k} should be biometric");
    }
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_counter_overflow_is_saturating() {
    let c = Counter::new("prop.sat").unwrap();
    c.set(u64::MAX - 1).unwrap();
    let _ = c.inc_by(1_000_000); // saturate
    assert_eq!(c.snapshot(), u64::MAX);
    let _ = c.inc_by(1_000_000); // saturate again
    assert_eq!(c.snapshot(), u64::MAX);
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_timer_count_eq_handle_drops() {
    use cssl_metrics::Timer;
    let t = Timer::new("prop.timer_count").unwrap();
    for _ in 0..50 {
        let h = t.start();
        h.commit();
    }
    assert_eq!(t.count(), 50);
}

#[test]
fn property_schema_id_only_depends_on_name_and_tag_keys() {
    // Different VALUES for the same KEY should produce same schema-id
    // (the discipline lives at "name + tag-keys" scope ; per-tag-VALUE
    // permutations are part of the live-stream not the schema).
    let a = Counter::new_with(
        "prop.schema",
        &[(TagKey::new("mode"), TagVal::Static("60"))],
        SamplingDiscipline::Always,
    )
    .unwrap();
    let b = Counter::new_with(
        "prop.schema",
        &[(TagKey::new("mode"), TagVal::Static("90"))],
        SamplingDiscipline::Always,
    )
    .unwrap();
    assert_eq!(a.schema_id(), b.schema_id());
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn property_histogram_record_then_count_invariant() {
    let h = Histogram::new("prop.invariant", LATENCY_NS_BUCKETS).unwrap();
    for i in 0..500_u32 {
        let v = f64::from(i) * 100.0;
        h.record(v).unwrap();
    }
    let snap = h.snapshot();
    assert_eq!(snap.count, 500);
    assert_eq!(snap.counts.iter().sum::<u64>(), 500);
}
