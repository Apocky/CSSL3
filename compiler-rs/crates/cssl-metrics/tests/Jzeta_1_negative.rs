#![allow(clippy::float_cmp)]
#![allow(unused_imports)]

//! Wave-Jζ-1 / T11-D157 negative-path tests.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.6 ; LM-1..15.
//!
//! § COVERAGE : every refusal-path that the API contract documents.

use cssl_metrics::{
    Counter, Gauge, Histogram, MetricError, SamplingDiscipline, TagKey, TagVal, COUNT_BUCKETS,
    LATENCY_NS_BUCKETS,
};

// ─────────────────────────────────────────────────────────────────────
// Tag refusal
// ─────────────────────────────────────────────────────────────────────

#[test]
fn neg_face_id_refused() {
    let r = Counter::new_with(
        "neg.face",
        &[(TagKey::new("face_id"), TagVal::U64(0))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
}

#[test]
fn neg_iris_id_refused() {
    let r = Counter::new_with(
        "neg.iris",
        &[(TagKey::new("iris_id"), TagVal::U64(0))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
}

#[test]
fn neg_heart_rate_refused() {
    let r = Counter::new_with(
        "neg.heart",
        &[(TagKey::new("heart_rate"), TagVal::U64(0))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
}

#[test]
fn neg_gaze_direction_refused() {
    let r = Counter::new_with(
        "neg.gaze",
        &[(TagKey::new("gaze_direction"), TagVal::U64(0))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
}

#[test]
fn neg_pii_substring_refused() {
    let r = Counter::new_with(
        "neg.pii",
        &[(TagKey::new("user_pii_id"), TagVal::U64(0))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
}

#[test]
fn neg_raw_path_unix_refused() {
    let r = Counter::new_with(
        "neg.unix_path",
        &[(TagKey::new("path"), TagVal::Static("/etc/passwd"))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::RawPathTagValue { .. })));
}

#[test]
fn neg_raw_path_windows_refused() {
    let r = Counter::new_with(
        "neg.win_path",
        &[(TagKey::new("path"), TagVal::Static("C:\\Users"))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::RawPathTagValue { .. })));
}

#[test]
fn neg_drive_letter_refused() {
    let r = Counter::new_with(
        "neg.drive",
        &[(TagKey::new("loc"), TagVal::Static("D:"))],
        SamplingDiscipline::Always,
    );
    assert!(matches!(r, Err(MetricError::RawPathTagValue { .. })));
}

#[test]
fn neg_too_many_tags_refused() {
    let tags = [
        (TagKey::new("a"), TagVal::U64(1)),
        (TagKey::new("b"), TagVal::U64(2)),
        (TagKey::new("c"), TagVal::U64(3)),
        (TagKey::new("d"), TagVal::U64(4)),
        (TagKey::new("e"), TagVal::U64(5)),
    ];
    let r = Counter::new_with("neg.overflow", &tags, SamplingDiscipline::Always);
    assert!(matches!(r, Err(MetricError::TagOverflow { .. })));
}

// ─────────────────────────────────────────────────────────────────────
// Numeric refusal
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_gauge_nan_refused() {
    let g = Gauge::new("neg.nan").unwrap();
    let r = g.set(f64::NAN);
    assert!(matches!(r, Err(MetricError::Nan { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_gauge_pos_inf_refused_default() {
    let g = Gauge::new("neg.pos_inf").unwrap();
    let r = g.set(f64::INFINITY);
    assert!(matches!(r, Err(MetricError::Inf { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_gauge_neg_inf_refused_default() {
    let g = Gauge::new("neg.neg_inf").unwrap();
    let r = g.set(f64::NEG_INFINITY);
    assert!(matches!(r, Err(MetricError::Inf { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_gauge_inc_to_inf_refused() {
    let g = Gauge::new("neg.inc_inf").unwrap();
    g.set(f64::MAX).unwrap();
    let r = g.inc(f64::MAX);
    // f64::MAX + f64::MAX = +Inf ⇒ refused under default Inf-policy.
    assert!(matches!(r, Err(MetricError::Inf { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_histogram_nan_refused() {
    let h = Histogram::new("neg.hnan", LATENCY_NS_BUCKETS).unwrap();
    let r = h.record(f64::NAN);
    assert!(matches!(r, Err(MetricError::Nan { .. })));
}

#[test]
fn neg_histogram_empty_boundaries_refused() {
    const EMPTY: &[f64] = &[];
    let r = Histogram::new("neg.empty", EMPTY);
    assert!(matches!(r, Err(MetricError::Bucket { .. })));
}

#[test]
fn neg_histogram_non_monotonic_boundaries_refused() {
    const BAD: &[f64] = &[1.0, 5.0, 3.0];
    let r = Histogram::new("neg.unsorted", BAD);
    assert!(matches!(r, Err(MetricError::Bucket { .. })));
}

#[test]
fn neg_histogram_duplicate_boundary_refused() {
    const BAD: &[f64] = &[1.0, 2.0, 2.0, 3.0];
    let r = Histogram::new("neg.dup", BAD);
    assert!(matches!(r, Err(MetricError::Bucket { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_counter_decrement_refused() {
    let c = Counter::new("neg.dec").unwrap();
    c.set(10).unwrap();
    let r = c.set(5);
    assert!(matches!(r, Err(MetricError::CounterDecrement { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_counter_overflow_first_returns_err() {
    let c = Counter::new("neg.overflow").unwrap();
    c.set(u64::MAX - 1).unwrap();
    let r = c.inc_by(100);
    assert!(matches!(r, Err(MetricError::Overflow { .. })));
}

#[test]
fn neg_registry_collision_returns_err() {
    let r = cssl_metrics::MetricRegistry::new();
    r.register("x", cssl_metrics::MetricKind::Counter, 1)
        .unwrap();
    let res = r.register("x", cssl_metrics::MetricKind::Counter, 2);
    assert!(matches!(res, Err(MetricError::SchemaCollision { .. })));
}

#[test]
fn neg_registry_kind_mismatch_returns_err() {
    let r = cssl_metrics::MetricRegistry::new();
    r.register("x", cssl_metrics::MetricKind::Counter, 1)
        .unwrap();
    let res = r.register("x", cssl_metrics::MetricKind::Gauge, 1);
    assert!(matches!(res, Err(MetricError::SchemaCollision { .. })));
}

#[test]
fn neg_effect_row_mismatch_returns_err() {
    use cssl_metrics::EffectRow;
    let r = EffectRow::check(EffectRow::Spans, EffectRow::Counters, "m");
    assert!(matches!(r, Err(MetricError::EffectRowMissing { .. })));
}

#[cfg(feature = "replay-strict")]
#[test]
fn neg_adaptive_under_strict_refused() {
    let r = Counter::new_with(
        "neg.adaptive",
        &[],
        SamplingDiscipline::Adaptive {
            target_overhead_pct: 0.5,
        },
    );
    assert!(matches!(r, Err(MetricError::AdaptiveUnderStrict { .. })));
}

#[cfg(not(feature = "metrics-disabled"))]
#[test]
fn neg_histogram_sum_unaffected_on_nan_refusal() {
    let h = Histogram::new("neg.sum_safe", COUNT_BUCKETS).unwrap();
    h.record(10.0).unwrap();
    let _ = h.record(f64::NAN); // refused
    assert_eq!(h.sum(), 10.0);
    assert_eq!(h.count(), 1);
}
