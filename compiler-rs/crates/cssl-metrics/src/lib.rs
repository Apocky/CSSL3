//! CSSLv3 stage0 — L2 telemetry foundation : Counter / Gauge / Histogram / Timer
//! + `MetricRegistry` per-subsystem namespace ; wires `cssl-telemetry` ring-buffer ;
//! replay-determinism preserved (sampled-deterministically OR no-op).
//!
//! § SPEC
//!   - Authoritative : `_drafts/phase_j/06_l2_telemetry_spec.md` § II — IV.
//!   - Inventory anchor : `DIAGNOSTIC_INFRA_PLAN.md` § 3 (75-metric table).
//!   - Effect-row baseline : `specs/22_TELEMETRY.csl` (R18 ring + scope-taxonomy).
//!   - Determinism contract : H5 (replay-bit-equal).
//!   - Privacy floor : T11-D130 (path-hash-only) + T11-D132 (biometric-refuse) +
//!     PRIME_DIRECTIVE.md § 1 (anti-surveillance).
//!
//! § DESIGN OVERVIEW
//!
//! ```text
//!   counter.rs    counter increment / saturating overflow / explicit reset
//!   gauge.rs      f64 set/inc/dec ; NaN-refused ; Inf-policy (refuse|clamp)
//!   histogram.rs  bucketed distribution ; canonical bucket-sets ; p50/p95/p99
//!   timer.rs      RAII handle ; LATENCY_NS_BUCKETS for percentile reads
//!   sampling.rs   deterministic decimation : OneIn / BurstThenDecimate /
//!                 Adaptive (last is REFUSED under feature `replay-strict`)
//!   tag.rs        TagKey/TagVal ; biometric-refuse + raw-path-refuse
//!   registry.rs   MetricRegistry + per-subsystem namespace + completeness-check
//!   schema.rs     MetricSchema + emit_into_ring (cssl-telemetry wiring)
//!   strict_clock  monotonic_ns indirection — deterministic in replay-strict
//!   error.rs      MetricError taxonomy (NaN / Inf / Overflow / SchemaCollision /
//!                 BiometricTagKey / RawPathTagValue / EffectRowMissing / etc)
//! ```
//!
//! § FEATURE FLAGS
//!
//! | flag                | effect                                                            |
//! |---------------------|-------------------------------------------------------------------|
//! | (none / default)    | full record-paths active ; runtime sampling ; wallclock timer    |
//! | `replay-strict`     | Adaptive sampling REFUSED ; timer-clock = `(frame_n,sub_phase)`  |
//! | `metrics-disabled`  | every record-site compiles to a no-op (zero observable overhead) |
//!
//! § PRIME-DIRECTIVE BINDING (§1)
//!   Every tag-construction routes through [`tag::validate_pair`] so
//!   biometric-key + raw-path-value are runtime-refused. Future cssl-effects
//!   lift extends this to compile-time refusal. The tag-list is bounded at
//!   4 inline entries ; spill is treated as a discipline violation rather
//!   than silently permitted (§ II.1 LM-8).
//!
//! § REPLAY-DETERMINISM (H5)
//!   Under feature `replay-strict` :
//!     - `monotonic_ns` redirects through [`strict_clock`] which is a pure
//!       function of `(frame_n, sub_phase)` ;
//!     - Adaptive sampling is REFUSED at construction (per § II.5) ;
//!     - Floating-point gauge values store the IEEE-754 bit-pattern via
//!       `to_bits` so no precision drift across replay-runs.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::similar_names)]
#![allow(clippy::missing_errors_doc)]
// Test-suite + reset-to / overflow / NaN paths intentionally compare f64
// equality (bit-pattern roundtrip is the contract under H5 replay-determinism).
#![allow(clippy::float_cmp)]
// Test-suite uses `_` prefix bindings for "I want this to live until end-of-
// scope but I don't read it" (drop-on-scope-exit semantics for TimerHandle etc).
#![allow(clippy::no_effect_underscore_binding)]
// MetricSchema + RegistryEntry are tiny POD structs whose `clone()` is the
// canonical copy-out path ; exercising it in tests is the contract.
#![allow(clippy::redundant_clone)]
// Tests use range-like comparisons (`a >= 0.0 && a <= 1.0`) for clarity over
// `(0.0..=1.0).contains(&a)` — same semantics, more readable in assertions.
#![allow(clippy::manual_range_contains)]
// Some `match` arms intentionally share a body (stage-0 mapping is a 1:1
// table now, will diverge as stages accrete).
#![allow(clippy::match_same_arms)]
// `clippy::collapsible_if` rewriting forces nesting changes that hurt
// readability under `cfg!(feature = ...)` discriminated branches.
#![allow(clippy::collapsible_if)]
// Histogram percentile uses load-modify-store on f64 bit-patterns ; the
// equivalent `mul_add` rewrite obscures intent here.
#![allow(clippy::suboptimal_flops)]
// Tag-substring scan uses single-char patterns ; the proposed `s.contains('/')`
// alternative is identical performance in practice.
#![allow(clippy::single_char_pattern)]
// MetricRegistry temporaries use Mutex-guarded values where the early-drop
// rewrite would obscure the lock-discipline.
#![allow(clippy::significant_drop_in_scrutinee)]
#![allow(clippy::significant_drop_tightening)]
// `feature = "metrics-disabled"` cfg blocks early-return (`return Ok(())` /
// `return;`) so the compiled-out path is unambiguous to read. The
// `clippy::needless_return` rewrite would change the alternative branch's
// shape — preserving early-return reads better given the cfg discriminant.
#![allow(clippy::needless_return)]
// The same cfg pattern produces unused-imports + dead-code warnings on the
// disabled-side. They're load-bearing on the enabled-side, so suppress.
#![allow(unused_imports)]
#![allow(dead_code)]

pub mod counter;
pub mod error;
pub mod gauge;
pub mod histogram;
pub mod registry;
pub mod sampling;
pub mod schema;
pub mod strict_clock;
pub mod tag;
pub mod timer;

pub use counter::Counter;
pub use error::{MetricError, MetricResult};
pub use gauge::{Gauge, InfPolicy};
pub use histogram::{
    Histogram, HistogramSnapshot, BYTES_BUCKETS, COUNT_BUCKETS, LATENCY_NS_BUCKETS, PIXEL_BUCKETS,
};
pub use registry::{
    global as global_registry, CompletenessReport, MetricKind, MetricRegistry, RegistryEntry,
    SubsystemRegistry,
};
pub use sampling::SamplingDiscipline;
pub use schema::{emit_into_ring, EffectRow, MetricSchema};
pub use strict_clock::{
    is_strict_mode, logical_clock_cursor, monotonic_ns, set_logical_clock,
    set_logical_clock_frame_ns,
};
pub use tag::{
    validate_pair, validate_tag_list, TagKey, TagSet, TagVal, BIOMETRIC_TAG_KEYS,
    BIOMETRIC_TAG_SUBSTRINGS,
};
pub use timer::{Timer, TimerHandle};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// True iff metrics are compiled-out (feature `metrics-disabled`).
#[must_use]
pub const fn is_metrics_disabled() -> bool {
    cfg!(feature = "metrics-disabled")
}

#[cfg(test)]
mod scaffold_tests {
    use super::{
        is_metrics_disabled, is_strict_mode, Counter, Gauge, Histogram, MetricKind, MetricRegistry,
        MetricSchema, SamplingDiscipline, SubsystemRegistry, TagKey, TagVal, Timer,
        LATENCY_NS_BUCKETS, STAGE0_SCAFFOLD,
    };

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn is_metrics_disabled_matches_feature() {
        assert_eq!(is_metrics_disabled(), cfg!(feature = "metrics-disabled"));
    }

    #[test]
    fn is_strict_mode_matches_feature() {
        assert_eq!(is_strict_mode(), cfg!(feature = "replay-strict"));
    }

    #[test]
    fn all_four_metric_kinds_constructable() {
        let _c = Counter::new("c").unwrap();
        let _g = Gauge::new("g").unwrap();
        let _h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        let _t = Timer::new("t").unwrap();
    }

    #[test]
    fn cross_crate_registration_via_subsystem() {
        let r = MetricRegistry::new();
        r.register("engine.frame_n", MetricKind::Counter, 1)
            .unwrap();
        r.register("engine.tick", MetricKind::Gauge, 2).unwrap();
        r.register("render.stage_time_ns", MetricKind::Timer, 3)
            .unwrap();
        // Three subsystem-namespaced metrics registered.
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn subsystem_registry_constructs() {
        let _ = SubsystemRegistry::new("test");
    }

    #[test]
    fn metric_schema_helper_works() {
        let s = MetricSchema::counter("engine.frame_n", 0, "06_l2 § III.1");
        assert_eq!(s.kind, MetricKind::Counter);
    }

    #[test]
    fn sampling_discipline_default_always() {
        let d = SamplingDiscipline::default();
        assert!(matches!(d, SamplingDiscipline::Always));
    }

    #[test]
    fn tag_key_construction_works() {
        let _k = TagKey::new("phase");
        let _v = TagVal::Static("compose");
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn end_to_end_counter_to_ring() {
        use crate::{emit_into_ring, MetricSchema};
        use cssl_telemetry::TelemetryRing;

        let ring = TelemetryRing::new(16);
        let counter = Counter::new("engine.frame_n").unwrap();
        counter.inc().unwrap();

        let schema = MetricSchema::counter("engine.frame_n", counter.schema_id(), "06_l2 § III.1");
        emit_into_ring(&ring, &schema, counter.snapshot());
        assert_eq!(ring.total_pushed(), 1);
    }

    #[cfg(feature = "metrics-disabled")]
    #[test]
    fn metrics_disabled_inc_is_no_op() {
        let c = Counter::new("c").unwrap();
        c.inc().unwrap();
        // Snapshot is the default zero ; no observable mutation.
        assert_eq!(c.snapshot(), 0);
    }

    #[cfg(feature = "metrics-disabled")]
    #[test]
    fn metrics_disabled_gauge_set_is_no_op() {
        let g = Gauge::new("g").unwrap();
        g.set(42.0).unwrap();
        assert_eq!(g.snapshot(), 0.0);
    }

    #[cfg(feature = "metrics-disabled")]
    #[test]
    fn metrics_disabled_histogram_record_is_no_op() {
        let h = Histogram::new("h", LATENCY_NS_BUCKETS).unwrap();
        h.record(50.0).unwrap();
        assert_eq!(h.count(), 0);
    }

    #[cfg(feature = "metrics-disabled")]
    #[test]
    fn metrics_disabled_timer_record_is_no_op() {
        let t = Timer::new("t").unwrap();
        t.record_ns(100);
        assert_eq!(t.count(), 0);
    }
}
