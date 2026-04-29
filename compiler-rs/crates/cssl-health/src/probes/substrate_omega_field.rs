//! § T11-D159 : Mock probe for `cssl-substrate-omega-field` (Ω-field keystone).
//!
//! Toy-state : `entropy_drift_sigma` (target ≤ 1.0) + `agency_violations`.
//! Per DENSITY_BUDGET §V.5 / §V.6. Agency-violations of kind {consent,
//! sov, reversibility, launder} are PRIME-DIRECTIVE-grade — they trip
//! the directive ⊗ FAIL-CLOSED.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-substrate-omega-field";
const ENTROPY_DRIFT_DEGRADE_BPS: u32 = 10_000; // sigma ≥ 1.0 (10000 bps)
const ENTROPY_DRIFT_FAIL_BPS: u32 = 30_000; // sigma ≥ 3.0

#[derive(Debug)]
pub struct MockProbe {
    entropy_drift_sigma_bps: AtomicU32,
    agency_violation_consent: AtomicBool,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entropy_drift_sigma_bps: AtomicU32::new(2_000),
            agency_violation_consent: AtomicBool::new(false),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_state(&self, sigma_bps: u32, consent_violation: bool, frame: u64) {
        self.entropy_drift_sigma_bps
            .store(sigma_bps, Ordering::Relaxed);
        self.agency_violation_consent
            .store(consent_violation, Ordering::Relaxed);
        self.since_frame.store(frame, Ordering::Relaxed);
    }
}

impl Default for MockProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthProbe for MockProbe {
    fn name(&self) -> &'static str {
        NAME
    }

    fn health(&self) -> HealthStatus {
        let frame = self.since_frame.load(Ordering::Relaxed);
        // PRIME_DIRECTIVE §1 : consent-violations trip the directive.
        if self.agency_violation_consent.load(Ordering::Relaxed) {
            return HealthStatus::failed(
                "agency-consent-violation",
                HealthFailureKind::PrimeDirectiveTrip,
                frame,
            );
        }
        let sigma = self.entropy_drift_sigma_bps.load(Ordering::Relaxed);
        if sigma >= ENTROPY_DRIFT_FAIL_BPS {
            return HealthStatus::failed(
                "entropy-drift-runaway",
                HealthFailureKind::InvariantBreach,
                frame,
            );
        }
        if sigma >= ENTROPY_DRIFT_DEGRADE_BPS {
            HealthStatus::Degraded {
                reason: "entropy-drift",
                budget_overshoot_bps: u16::try_from(sigma - ENTROPY_DRIFT_DEGRADE_BPS)
                    .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
