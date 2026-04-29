//! § T11-D159 : Mock probe for `cssl-gaze-collapse` (observer-collapse oracle).
//!
//! Toy-state : `consent_witnessed` ; if false ⊗ Failed ⊗ PrimeDirectiveTrip
//! (gaze data ¬ collected without explicit-consent per PRIME_DIRECTIVE §1).
//! Degrades on `eye_tracker_confidence < 0.7`.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-gaze-collapse";
const CONFIDENCE_DEGRADE_THRESHOLD_BPS: u32 = 7_000; // 0.70 in basis-points

#[derive(Debug)]
pub struct MockProbe {
    consent_witnessed: AtomicBool,
    eye_tracker_confidence_bps: AtomicU32, // 0..10000
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            consent_witnessed: AtomicBool::new(true),
            eye_tracker_confidence_bps: AtomicU32::new(9_500),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_state(&self, consent: bool, confidence_bps: u32, frame: u64) {
        self.consent_witnessed.store(consent, Ordering::Relaxed);
        self.eye_tracker_confidence_bps
            .store(confidence_bps, Ordering::Relaxed);
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
        // PRIME_DIRECTIVE §1 : without consent, this subsystem MUST fail-close.
        if !self.consent_witnessed.load(Ordering::Relaxed) {
            return HealthStatus::failed(
                "gaze-without-consent",
                HealthFailureKind::PrimeDirectiveTrip,
                frame,
            );
        }
        let confidence = self.eye_tracker_confidence_bps.load(Ordering::Relaxed);
        if confidence < CONFIDENCE_DEGRADE_THRESHOLD_BPS {
            HealthStatus::Degraded {
                reason: "low-eye-tracker-confidence",
                budget_overshoot_bps: u16::try_from(CONFIDENCE_DEGRADE_THRESHOLD_BPS - confidence)
                    .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
