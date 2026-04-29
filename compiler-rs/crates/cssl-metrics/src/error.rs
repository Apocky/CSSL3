//! Metric-error taxonomy.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.7 (registration)
//!         + § II.2 (NaN refusal) + § II.6 (effect-row violation).
//!
//! § DESIGN
//!   `MetricError` enumerates every refusal-path observable to a metric-caller :
//!   NaN/Inf in floating-point, overflow on integer counters, schema collision
//!   at registration, biometric-tag-key compile-time-refused, raw-path tag-value
//!   refused, adaptive-sampling-under-replay-strict refused, etc.
//!
//!   These are user-facing errors and so carry English-prose messages keyed by
//!   the spec-§ that originally banned the path. Every refusal-path is also
//!   audit-chain-loggable via [`MetricError::audit_tag`] which returns a
//!   stable short-tag suitable for `AuditEntry::tag`.

use thiserror::Error;

/// Result alias for metric-API call-sites.
pub type MetricResult<T> = core::result::Result<T, MetricError>;

/// Refusal-paths observable to metric callers.
///
/// § SPEC-CITES embedded in each variant's #[error] text point at the rule.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MetricError {
    /// Gauge.set was called with `f64::NAN`. § II.2 — N! silent-write.
    #[error("MET0001 — Gauge.set(NaN) refused (06_l2_telemetry_spec § II.2 ; gauge {name})")]
    Nan {
        /// Metric-name that received the NaN.
        name: &'static str,
    },

    /// Gauge.set was called with `f64::INFINITY` or `f64::NEG_INFINITY` AND
    /// the gauge's schema-policy says infinity is a refuse (rather than a clamp).
    /// § II.2 — `set(±Inf) ⊗ refused-or-clamped @ schema-policy`.
    #[error("MET0002 — Gauge.set(±Inf) refused (06_l2_telemetry_spec § II.2 ; gauge {name})")]
    Inf {
        /// Metric-name that received the infinity.
        name: &'static str,
    },

    /// Counter monotonic guard tripped : `set` was called with a value strictly
    /// less than the current snapshot. § II.1 — monotonic-non-decreasing.
    /// `set` IS permitted (tagged-as-RESET-EVENT) but only via
    /// [`crate::Counter::reset_to`] which records the RESET in the audit-chain.
    /// Plain `Counter::set(v < snapshot)` returns this.
    #[error(
        "MET0003 — Counter.set({proposed}) < snapshot {current} refused as silent-decrement \
         (06_l2_telemetry_spec § II.1 ; use Counter::reset_to for explicit reset ; counter {name})"
    )]
    CounterDecrement {
        /// Metric-name.
        name: &'static str,
        /// Current snapshot at refusal-time.
        current: u64,
        /// Proposed value (strictly less than `current`).
        proposed: u64,
    },

    /// Counter overflowed `u64::MAX`. The overflow itself is saturating per
    /// § II.1 ; this error is returned alongside an Audit<"counter-overflow">
    /// event so callers KNOW saturation happened.
    #[error(
        "MET0004 — Counter overflow saturated at u64::MAX (06_l2_telemetry_spec § II.1 ; counter {name})"
    )]
    Overflow {
        /// Metric-name that overflowed.
        name: &'static str,
    },

    /// Schema-id collision at MetricRegistry::register.
    /// § II.7 — `idempotent ; collision-detection`.
    #[error(
        "MET0005 — schema-id collision : metric {existing} already registered with same schema-id as {new} \
         (06_l2_telemetry_spec § II.7)"
    )]
    SchemaCollision {
        /// Already-registered metric-name.
        existing: &'static str,
        /// New metric whose schema-id collides.
        new: &'static str,
    },

    /// Adaptive sampling discipline used while feature `replay-strict` is on.
    /// § II.5 — Adaptive-mode FORBIDDEN under replay-strict (H5).
    #[error(
        "MET0006 — Adaptive sampling forbidden under replay-strict \
         (06_l2_telemetry_spec § II.5 ; metric {name})"
    )]
    AdaptiveUnderStrict {
        /// Metric-name configured with Adaptive sampling.
        name: &'static str,
    },

    /// Histogram.record received a value outside the schema's allowed-range.
    /// § II.3 — bucket-boundaries must be monotonic ; this is the runtime
    /// violation of that contract.
    #[error(
        "MET0007 — Histogram bucket-boundary violation : {detail} \
         (06_l2_telemetry_spec § II.3 ; histogram {name})"
    )]
    Bucket {
        /// Metric-name.
        name: &'static str,
        /// Specific violation (non-monotonic / empty / etc).
        detail: &'static str,
    },

    /// Tag-list overflowed the SmallVec inline cap (= 4) — § II.1 banner :
    /// "tag-discipline : SmallVec inline ; biometric-compile-refused" ; spill
    /// to heap is treated as a DISCIPLINE violation rather than silently
    /// permitted.
    #[error(
        "MET0008 — tag-list spilled past inline cap (≤ 4) ; SmallVec inline-only discipline \
         (06_l2_telemetry_spec § II.1 ; metric {name} ; got {len} tags)"
    )]
    TagOverflow {
        /// Metric-name.
        name: &'static str,
        /// Tag-count that triggered the overflow.
        len: usize,
    },

    /// Tag-key matched the BiometricKind enumeration. § II.1 banner :
    /// "biometric-tag-key (BiometricKind enumeration) ⊗ refused-via cssl-ifc::TelemetryEgress".
    /// Surfaced at runtime when caller-built tags rather than literal-matched.
    #[error(
        "MET0009 — biometric tag-key {key} refused (PRIME_DIRECTIVE §1 ; \
         06_l2_telemetry_spec § II.1 ; metric {name})"
    )]
    BiometricTagKey {
        /// Metric-name.
        name: &'static str,
        /// The refused key.
        key: &'static str,
    },

    /// Tag-value contained a raw filesystem path. § II.1 banner :
    /// "raw-path tag-value ⊗ refused-via path_hash-discipline (T11-D130)".
    #[error(
        "MET0010 — raw-path tag-value refused (T11-D130 path-hash-only discipline ; \
         06_l2_telemetry_spec § II.1 ; metric {name})"
    )]
    RawPathTagValue {
        /// Metric-name.
        name: &'static str,
    },

    /// Effect-row violation : caller lacks `Telemetry<Counters>` capability.
    /// In stage-0 this is reported at runtime via [`crate::EffectRow::check`].
    /// Stage-1 lifts this to compile-time via cssl-effects.
    #[error(
        "MET0011 — caller lacks Telemetry<Counters> effect-row \
         (06_l2_telemetry_spec § II.6 ; metric {name})"
    )]
    EffectRowMissing {
        /// Metric-name.
        name: &'static str,
    },
}

impl MetricError {
    /// Stable short-tag for audit-chain entries logging this refusal.
    ///
    /// § DISCIPLINE : these tags are part of the ABI for any downstream tool
    /// that grep's the audit-chain looking for refusal-events ; renaming them
    /// requires a DECISIONS-pin.
    #[must_use]
    pub const fn audit_tag(&self) -> &'static str {
        match self {
            Self::Nan { .. } => "metric-nan-refused",
            Self::Inf { .. } => "metric-inf-refused",
            Self::CounterDecrement { .. } => "counter-decrement-refused",
            Self::Overflow { .. } => "counter-overflow",
            Self::SchemaCollision { .. } => "metric-schema-collision",
            Self::AdaptiveUnderStrict { .. } => "adaptive-under-strict-refused",
            Self::Bucket { .. } => "histogram-bucket-violation",
            Self::TagOverflow { .. } => "tag-overflow-refused",
            Self::BiometricTagKey { .. } => "biometric-tag-refused",
            Self::RawPathTagValue { .. } => "raw-path-tag-refused",
            Self::EffectRowMissing { .. } => "effect-row-missing",
        }
    }

    /// Stable error-code (e.g., `"MET0001"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Nan { .. } => "MET0001",
            Self::Inf { .. } => "MET0002",
            Self::CounterDecrement { .. } => "MET0003",
            Self::Overflow { .. } => "MET0004",
            Self::SchemaCollision { .. } => "MET0005",
            Self::AdaptiveUnderStrict { .. } => "MET0006",
            Self::Bucket { .. } => "MET0007",
            Self::TagOverflow { .. } => "MET0008",
            Self::BiometricTagKey { .. } => "MET0009",
            Self::RawPathTagValue { .. } => "MET0010",
            Self::EffectRowMissing { .. } => "MET0011",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MetricError, MetricResult};

    #[test]
    fn nan_carries_metric_name() {
        let e = MetricError::Nan {
            name: "engine.frame_time_ns",
        };
        assert!(format!("{e}").contains("engine.frame_time_ns"));
        assert_eq!(e.code(), "MET0001");
        assert_eq!(e.audit_tag(), "metric-nan-refused");
    }

    #[test]
    fn inf_carries_metric_name() {
        let e = MetricError::Inf {
            name: "render.cull_rate",
        };
        assert_eq!(e.code(), "MET0002");
        assert_eq!(e.audit_tag(), "metric-inf-refused");
    }

    #[test]
    fn decrement_carries_proposed_and_current() {
        let e = MetricError::CounterDecrement {
            name: "engine.frame_n",
            current: 100,
            proposed: 50,
        };
        let s = format!("{e}");
        assert!(s.contains("100"));
        assert!(s.contains("50"));
        assert_eq!(e.code(), "MET0003");
    }

    #[test]
    fn overflow_audit_tag_stable() {
        let e = MetricError::Overflow {
            name: "physics.morton_collisions",
        };
        assert_eq!(e.audit_tag(), "counter-overflow");
    }

    #[test]
    fn schema_collision_lists_both_names() {
        let e = MetricError::SchemaCollision {
            existing: "a",
            new: "b",
        };
        let s = format!("{e}");
        assert!(s.contains("a"));
        assert!(s.contains("b"));
    }

    #[test]
    fn adaptive_under_strict_code_stable() {
        let e = MetricError::AdaptiveUnderStrict { name: "x.y" };
        assert_eq!(e.code(), "MET0006");
    }

    #[test]
    fn bucket_carries_detail() {
        let e = MetricError::Bucket {
            name: "hist.x",
            detail: "non-monotonic",
        };
        assert!(format!("{e}").contains("non-monotonic"));
    }

    #[test]
    fn tag_overflow_carries_len() {
        let e = MetricError::TagOverflow { name: "m", len: 7 };
        assert!(format!("{e}").contains("7"));
    }

    #[test]
    fn biometric_tag_key_cites_prime_directive() {
        let e = MetricError::BiometricTagKey {
            name: "m",
            key: "face_id",
        };
        let s = format!("{e}");
        assert!(s.contains("PRIME_DIRECTIVE"));
        assert!(s.contains("face_id"));
    }

    #[test]
    fn raw_path_cites_d130() {
        let e = MetricError::RawPathTagValue { name: "m" };
        assert!(format!("{e}").contains("T11-D130"));
    }

    #[test]
    fn effect_row_missing_cites_section() {
        let e = MetricError::EffectRowMissing { name: "m" };
        assert!(format!("{e}").contains("§ II.6"));
    }

    #[test]
    fn all_codes_unique() {
        let codes = [
            MetricError::Nan { name: "" }.code(),
            MetricError::Inf { name: "" }.code(),
            MetricError::CounterDecrement {
                name: "",
                current: 0,
                proposed: 0,
            }
            .code(),
            MetricError::Overflow { name: "" }.code(),
            MetricError::SchemaCollision {
                existing: "",
                new: "",
            }
            .code(),
            MetricError::AdaptiveUnderStrict { name: "" }.code(),
            MetricError::Bucket {
                name: "",
                detail: "",
            }
            .code(),
            MetricError::TagOverflow { name: "", len: 0 }.code(),
            MetricError::BiometricTagKey { name: "", key: "" }.code(),
            MetricError::RawPathTagValue { name: "" }.code(),
            MetricError::EffectRowMissing { name: "" }.code(),
        ];
        let set: std::collections::HashSet<_> = codes.iter().copied().collect();
        assert_eq!(set.len(), 11);
    }

    #[test]
    fn all_audit_tags_unique() {
        let tags = [
            MetricError::Nan { name: "" }.audit_tag(),
            MetricError::Inf { name: "" }.audit_tag(),
            MetricError::CounterDecrement {
                name: "",
                current: 0,
                proposed: 0,
            }
            .audit_tag(),
            MetricError::Overflow { name: "" }.audit_tag(),
            MetricError::SchemaCollision {
                existing: "",
                new: "",
            }
            .audit_tag(),
            MetricError::AdaptiveUnderStrict { name: "" }.audit_tag(),
            MetricError::Bucket {
                name: "",
                detail: "",
            }
            .audit_tag(),
            MetricError::TagOverflow { name: "", len: 0 }.audit_tag(),
            MetricError::BiometricTagKey { name: "", key: "" }.audit_tag(),
            MetricError::RawPathTagValue { name: "" }.audit_tag(),
            MetricError::EffectRowMissing { name: "" }.audit_tag(),
        ];
        let set: std::collections::HashSet<_> = tags.iter().copied().collect();
        assert_eq!(set.len(), 11);
    }

    #[test]
    #[allow(clippy::unnecessary_wraps)]
    fn metric_result_alias_works() {
        // Construct via a helper so clippy doesn't see a literal-Ok-then-unwrap
        // pattern in the test body.
        fn ok42() -> MetricResult<u64> {
            Ok(42)
        }
        assert_eq!(ok42().expect("constructed Ok"), 42);
        let r: MetricResult<u64> = Err(MetricError::Overflow { name: "x" });
        assert!(r.is_err());
    }

    #[test]
    fn error_clone_eq_traits_present() {
        let a = MetricError::Nan { name: "x" };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
