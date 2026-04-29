//! § T11-D159 : `engine_health()` aggregator + `HealthAggregate` + `HealthRegistry`
//!
//! Implements the worst-case-monoid from `_drafts/phase_j/06_l2_telemetry_spec.md`
//! § V.2 :
//!
//! ```text
//! Ok       ⊔ Ok       = Ok
//! Ok       ⊔ Degraded = Degraded
//! Degraded ⊔ Degraded = Degraded(merged)
//! _        ⊔ Failed   = Failed
//! Failed   ⊔ Failed   = Failed(merged)
//! _        ⊔ PrimeDirectiveTrip = PrimeDirectiveTrip      // ALWAYS-WINS
//! ```
//!
//! `PrimeDirectiveTrip` is the **absorbing element** : it survives any
//! aggregation and triggers fail-close per spec § V.2.

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

/// Per-probe entry in an aggregate report. Intentionally `Clone` so
/// downstream serializers (MCP / OTLP) can fan-out without re-querying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthEntry {
    /// Crate-name (matches `HealthProbe::name()`).
    pub name: &'static str,
    /// Snapshot of `HealthProbe::health()` taken @ aggregator-call-time.
    pub status: HealthStatus,
}

/// Aggregate-of-record returned by [`engine_health`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthAggregate {
    /// Worst-case across all per-subsystem reports.
    pub worst: HealthStatus,
    /// Per-subsystem entries, in the order they were polled.
    pub entries: Vec<HealthEntry>,
}

impl HealthAggregate {
    /// Number of subsystems that reported `Ok`.
    #[must_use]
    pub fn ok_count(&self) -> usize {
        self.entries.iter().filter(|e| e.status.is_ok()).count()
    }

    /// Number of subsystems that reported `Degraded { .. }`.
    #[must_use]
    pub fn degraded_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status.is_degraded())
            .count()
    }

    /// Number of subsystems that reported `Failed { .. }`.
    #[must_use]
    pub fn failed_count(&self) -> usize {
        self.entries.iter().filter(|e| e.status.is_failed()).count()
    }

    /// `true` iff any subsystem reported `Failed { kind: PrimeDirectiveTrip, .. }`.
    /// Engine-fail-close-condition per spec § V.2.
    #[must_use]
    pub fn is_prime_directive_trip(&self) -> bool {
        self.worst.is_prime_directive_trip()
    }
}

/// Worst-case combine. **Public** so test-code + replay-tooling can
/// exercise the monoid directly without going through a registry.
#[must_use]
pub fn combine(a: HealthStatus, b: HealthStatus) -> HealthStatus {
    use HealthStatus::{Degraded, Failed, Ok};

    // PrimeDirectiveTrip ALWAYS wins.
    if a.is_prime_directive_trip() {
        return a;
    }
    if b.is_prime_directive_trip() {
        return b;
    }

    match (a, b) {
        (Ok, Ok) => Ok,

        // Identity arms + Failed-dominates-Degraded all-yield-the-non-Ok-side.
        // Clippy `match_same_arms` requires they share one arm.
        (Ok, x @ (Degraded { .. } | Failed { .. }))
        | (x @ (Degraded { .. } | Failed { .. }), Ok)
        | (x @ Failed { .. }, Degraded { .. })
        | (Degraded { .. }, x @ Failed { .. }) => x,

        // Two Degradeds : keep the older `since_frame` (the one that's
        // been degraded longer wins) ; reasons are merged via static-str
        // discrimination — we keep the one with higher overshoot.
        (
            Degraded {
                reason: ra,
                budget_overshoot_bps: oa,
                since_frame: fa,
            },
            Degraded {
                reason: rb,
                budget_overshoot_bps: ob,
                since_frame: fb,
            },
        ) => Degraded {
            reason: if oa >= ob { ra } else { rb },
            budget_overshoot_bps: oa.max(ob),
            since_frame: fa.min(fb),
        },

        // Two Faileds : older `since_frame` wins ; merge upstream-chain
        // by promoting to UpstreamFailure if kinds differ.
        (
            Failed {
                reason: ra,
                kind: ka,
                since_frame: fa,
            },
            Failed {
                reason: rb,
                kind: kb,
                since_frame: fb,
            },
        ) => {
            // PrimeDirectiveTrip already-handled at top of fn ; here we
            // know neither side is the trip. Older-wins on since_frame.
            let (older_reason, older_kind, older_frame) =
                if fa <= fb { (ra, ka, fa) } else { (rb, kb, fb) };
            // If the two kinds disagree, mark the surviving one as having
            // an upstream-cascade so consumers see the chain.
            let kind = if ka == kb {
                older_kind
            } else {
                HealthFailureKind::UpstreamFailure {
                    upstream: "multiple",
                }
            };
            Failed {
                reason: older_reason,
                kind,
                since_frame: older_frame,
            }
        }
    }
}

/// Aggregate health across an arbitrary slice of probes. Probes are
/// polled left-to-right ; per spec § V.4 each call must be `≤ 100µs`
/// and aggregate `≤ 2ms`. Mock-impls in this crate are O(1) so the
/// bound holds trivially.
#[must_use]
pub fn engine_health(probes: &[&dyn HealthProbe]) -> HealthAggregate {
    let mut entries = Vec::with_capacity(probes.len());
    let mut worst = HealthStatus::Ok;

    for p in probes {
        let status = p.health();
        worst = combine(worst, status.clone());
        entries.push(HealthEntry {
            name: p.name(),
            status,
        });
    }

    HealthAggregate { worst, entries }
}

/// In-process registry. Per spec § V.4 a `#[ctor]`-bound static would
/// be wired by each subsystem-crate ; we ship a plain owned-vec here
/// so this leaf-crate can be built + tested without `#[ctor]` deps.
///
/// Real-integration slice (Wave-Jθ-4) replaces this with a global
/// `static HEALTH_REGISTRY: HealthRegistry`.
#[derive(Default)]
pub struct HealthRegistry {
    probes: Vec<Box<dyn HealthProbe>>,
}

impl HealthRegistry {
    /// New empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self { probes: Vec::new() }
    }

    /// Insert a probe. Order-preserving.
    pub fn register(&mut self, probe: Box<dyn HealthProbe>) {
        self.probes.push(probe);
    }

    /// Number of registered probes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.probes.len()
    }

    /// `true` iff no probes registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.probes.is_empty()
    }

    /// Aggregate health across all registered probes.
    #[must_use]
    pub fn engine_health(&self) -> HealthAggregate {
        let probes: Vec<&dyn HealthProbe> = self
            .probes
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect();
        engine_health(&probes)
    }

    /// Lookup a probe by name. O(N) ; N=12 ⇒ trivial.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&dyn HealthProbe> {
        self.probes
            .iter()
            .find(|p| p.name() == name)
            .map(std::convert::AsRef::as_ref)
    }
}

impl core::fmt::Debug for HealthRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HealthRegistry")
            .field("len", &self.probes.len())
            .field(
                "names",
                &self.probes.iter().map(|p| p.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::HealthFailureKind;

    fn ok() -> HealthStatus {
        HealthStatus::Ok
    }
    fn deg(reason: &'static str, frame: u64, bps: u16) -> HealthStatus {
        HealthStatus::Degraded {
            reason,
            budget_overshoot_bps: bps,
            since_frame: frame,
        }
    }
    fn fail(reason: &'static str, kind: HealthFailureKind, frame: u64) -> HealthStatus {
        HealthStatus::Failed {
            reason,
            kind,
            since_frame: frame,
        }
    }

    #[test]
    fn ok_idempotent() {
        assert_eq!(combine(ok(), ok()), ok());
    }

    #[test]
    fn ok_then_degraded_yields_degraded() {
        let d = deg("thermal", 5, 100);
        assert_eq!(combine(ok(), d.clone()), d);
        assert_eq!(combine(d.clone(), ok()), d);
    }

    #[test]
    fn ok_then_failed_yields_failed() {
        let f = fail("oom", HealthFailureKind::ResourceExhaustion, 5);
        assert_eq!(combine(ok(), f.clone()), f);
        assert_eq!(combine(f.clone(), ok()), f);
    }

    #[test]
    fn failed_dominates_degraded() {
        let d = deg("warn", 10, 50);
        let f = fail("err", HealthFailureKind::DeadlineMiss, 20);
        assert_eq!(combine(d.clone(), f.clone()), f);
        assert_eq!(
            combine(f, d),
            fail("err", HealthFailureKind::DeadlineMiss, 20)
        );
    }

    #[test]
    fn prime_directive_trip_always_wins() {
        let trip = fail("breach", HealthFailureKind::PrimeDirectiveTrip, 99);
        let oom = fail("oom", HealthFailureKind::ResourceExhaustion, 1);
        let warn = deg("warn", 1, 10);

        assert_eq!(combine(trip.clone(), ok()), trip);
        assert_eq!(combine(ok(), trip.clone()), trip);
        assert_eq!(combine(trip.clone(), warn.clone()), trip);
        assert_eq!(combine(warn, trip.clone()), trip);
        assert_eq!(combine(trip.clone(), oom.clone()), trip);
        assert_eq!(combine(oom, trip.clone()), trip);
    }

    #[test]
    fn two_degradeds_merge_keeps_higher_overshoot_and_older_frame() {
        let d1 = deg("a", 100, 50);
        let d2 = deg("b", 50, 200);
        let merged = combine(d1, d2);
        match merged {
            HealthStatus::Degraded {
                reason,
                budget_overshoot_bps,
                since_frame,
            } => {
                assert_eq!(reason, "b");
                assert_eq!(budget_overshoot_bps, 200);
                assert_eq!(since_frame, 50);
            }
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    #[test]
    fn two_faileds_with_different_kinds_promote_to_upstream() {
        let f1 = fail("oom", HealthFailureKind::ResourceExhaustion, 50);
        let f2 = fail("therm", HealthFailureKind::ThermalThrottle, 100);
        let merged = combine(f1, f2);
        match merged {
            HealthStatus::Failed {
                kind: HealthFailureKind::UpstreamFailure { upstream },
                since_frame,
                ..
            } => {
                assert_eq!(upstream, "multiple");
                assert_eq!(since_frame, 50);
            }
            other => panic!("expected UpstreamFailure cascade, got {other:?}"),
        }
    }

    #[test]
    fn aggregator_empty_is_ok() {
        let agg = engine_health(&[]);
        assert_eq!(agg.worst, HealthStatus::Ok);
        assert_eq!(agg.entries.len(), 0);
    }
}
