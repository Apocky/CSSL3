//! § T11-D159 : `HealthStatus` + `HealthFailureKind` enums
//!
//! Mirrors `_drafts/phase_j/06_l2_telemetry_spec.md` § V.1 verbatim.
//! Lives in a leaf-module so it can be re-exported from the crate root
//! without forcing consumers to reach through `aggregator::` or
//! `probes::`.

use core::fmt;

/// Health of one subsystem at one observation-point.
///
/// Derived per spec § V.1. `Degraded` and `Failed` carry **why** in
/// `&'static str` so that toy-impls can avoid heap-allocation in the
/// hot-path ; downstream serializers (MCP / OTLP) widen to owned
/// `String` only at export-time.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HealthStatus {
    /// Operational : within-budget + no-recent-errors.
    Ok,
    /// Functional but degraded : warning-only.
    Degraded {
        /// Human-readable description (static-str ; no heap-alloc).
        reason: &'static str,
        /// 0..100 integer percent overshoot relative to budget.
        /// Encoded as `u16` (not f32) so `Eq`+`Hash` are derivable.
        budget_overshoot_bps: u16,
        /// Frame-number degradation was first observed.
        since_frame: u64,
    },
    /// Failed : subsystem returns errors / cannot-make-progress.
    Failed {
        /// Human-readable description.
        reason: &'static str,
        /// Categorical failure-kind (see [`HealthFailureKind`]).
        kind: HealthFailureKind,
        /// Frame-number failure was first observed.
        since_frame: u64,
    },
}

/// Categorical failure-cause. Extension of spec § V.1 ; **`PrimeDirectiveTrip`
/// is the absorbing element of the aggregation-monoid** (§ V.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HealthFailureKind {
    /// Frame-deadline missed (DENSITY_BUDGET §V).
    DeadlineMiss,
    /// VRAM / RAM / fd / handle exhausted.
    ResourceExhaustion,
    /// CPU/GPU thermal-cap engaged ⊗ throttling.
    ThermalThrottle,
    /// PRIME-DIRECTIVE consent-violation detected (§1).
    ConsentViolationDetected,
    /// L0.5 invariant fail (e.g. σ-balance, replay-determinism).
    InvariantBreach,
    /// Upstream subsystem returned `Failed` ; cascade-marker.
    UpstreamFailure {
        /// Crate-name of the upstream subsystem that failed.
        upstream: &'static str,
    },
    /// PRIME_DIRECTIVE §1 / §11 breach ⊗ FAIL-CLOSED ⊗ ALWAYS-WINS.
    PrimeDirectiveTrip,
    /// Catch-all ; should be rare. Used by mock-impls for bench-stubs.
    Unknown,
}

impl HealthStatus {
    /// Convenience constructor for the most-common Degraded shape.
    #[must_use]
    pub const fn degraded(reason: &'static str, since_frame: u64) -> Self {
        Self::Degraded {
            reason,
            budget_overshoot_bps: 0,
            since_frame,
        }
    }

    /// Convenience constructor for the most-common Failed shape.
    #[must_use]
    pub const fn failed(reason: &'static str, kind: HealthFailureKind, since_frame: u64) -> Self {
        Self::Failed {
            reason,
            kind,
            since_frame,
        }
    }

    /// Numeric encoding for the `engine.health_state` gauge (per spec
    /// § III.1 : `0=Failed / 1=Degraded / 2=Ok`).
    #[must_use]
    pub const fn gauge_encoding(&self) -> u8 {
        match self {
            Self::Ok => 2,
            Self::Degraded { .. } => 1,
            Self::Failed { .. } => 0,
        }
    }

    /// `true` iff this status is `Failed { kind: PrimeDirectiveTrip, .. }`.
    /// Engine-shutdown-condition per spec § V.2 (FAIL-CLOSED).
    #[must_use]
    pub const fn is_prime_directive_trip(&self) -> bool {
        matches!(
            self,
            Self::Failed {
                kind: HealthFailureKind::PrimeDirectiveTrip,
                ..
            }
        )
    }

    /// `true` iff `Ok`.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    /// `true` iff `Degraded { .. }`.
    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        matches!(self, Self::Degraded { .. })
    }

    /// `true` iff `Failed { .. }`.
    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok => f.write_str("Ok"),
            Self::Degraded {
                reason,
                budget_overshoot_bps,
                since_frame,
            } => write!(
                f,
                "Degraded({reason} ; +{:.2}% since-frame={since_frame})",
                f64::from(*budget_overshoot_bps) / 100.0
            ),
            Self::Failed {
                reason,
                kind,
                since_frame,
            } => write!(f, "Failed({reason} ; {kind:?} ; since-frame={since_frame})"),
        }
    }
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_default() {
        assert_eq!(HealthStatus::default(), HealthStatus::Ok);
    }

    #[test]
    fn gauge_encoding_matches_spec() {
        // spec § III.1 : 0=Failed / 1=Degraded / 2=Ok
        assert_eq!(HealthStatus::Ok.gauge_encoding(), 2);
        assert_eq!(HealthStatus::degraded("test", 0).gauge_encoding(), 1);
        assert_eq!(
            HealthStatus::failed("x", HealthFailureKind::Unknown, 0).gauge_encoding(),
            0
        );
    }

    #[test]
    fn prime_directive_trip_predicate() {
        let trip =
            HealthStatus::failed("consent-breach", HealthFailureKind::PrimeDirectiveTrip, 42);
        assert!(trip.is_prime_directive_trip());
        assert!(trip.is_failed());

        let other = HealthStatus::failed("oom", HealthFailureKind::ResourceExhaustion, 7);
        assert!(!other.is_prime_directive_trip());
        assert!(other.is_failed());
    }

    #[test]
    fn degraded_constructor() {
        let d = HealthStatus::degraded("vram-pressure", 100);
        match d {
            HealthStatus::Degraded {
                reason,
                budget_overshoot_bps,
                since_frame,
            } => {
                assert_eq!(reason, "vram-pressure");
                assert_eq!(budget_overshoot_bps, 0);
                assert_eq!(since_frame, 100);
            }
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    #[test]
    fn display_all_variants() {
        let ok = HealthStatus::Ok.to_string();
        assert_eq!(ok, "Ok");

        let d = HealthStatus::Degraded {
            reason: "thermal",
            budget_overshoot_bps: 1234,
            since_frame: 99,
        }
        .to_string();
        assert!(d.contains("thermal"));
        assert!(d.contains("12.34%"));
        assert!(d.contains("99"));

        let f = HealthStatus::failed("oom", HealthFailureKind::ResourceExhaustion, 5).to_string();
        assert!(f.contains("oom"));
        assert!(f.contains("ResourceExhaustion"));
        assert!(f.contains('5'));
    }

    #[test]
    fn upstream_failure_carries_crate_name() {
        let kind = HealthFailureKind::UpstreamFailure {
            upstream: "cssl-wave-solver",
        };
        match kind {
            HealthFailureKind::UpstreamFailure { upstream } => {
                assert_eq!(upstream, "cssl-wave-solver");
            }
            _ => panic!("variant mismatch"),
        }
    }

    #[test]
    fn predicates_are_mutex() {
        for s in [
            HealthStatus::Ok,
            HealthStatus::degraded("d", 0),
            HealthStatus::failed("f", HealthFailureKind::Unknown, 0),
        ] {
            let count = u8::from(s.is_ok()) + u8::from(s.is_degraded()) + u8::from(s.is_failed());
            assert_eq!(count, 1, "exactly-one predicate must hold for {s:?}");
        }
    }

    #[test]
    fn equality_and_hash_derivable() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(HealthStatus::Ok);
        set.insert(HealthStatus::Ok);
        assert_eq!(set.len(), 1);
        set.insert(HealthStatus::degraded("x", 0));
        assert_eq!(set.len(), 2);
    }
}
