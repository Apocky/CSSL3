//! § T11-D159 : Mock probe for `cssl-host-openxr` (VR/AR substrate-portal).
//!
//! Toy-state : `runtime_state` (enum-encoded ; 4=Focused/3=Visible/2=Synced/
//! 1=Stopping/0=Lost) + `boundary_breach_count` ; degrades on Visible-but-
//! not-Focused ; Failed on Lost / on boundary-breach > 0.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-host-openxr";
const STATE_LOST: u32 = 0;
const STATE_FOCUSED: u32 = 4;

#[derive(Debug)]
pub struct MockProbe {
    runtime_state: AtomicU32,
    boundary_breach_count: AtomicU32,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            runtime_state: AtomicU32::new(STATE_FOCUSED),
            boundary_breach_count: AtomicU32::new(0),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_state(&self, runtime_state: u32, boundary_breaches: u32, frame: u64) {
        self.runtime_state.store(runtime_state, Ordering::Relaxed);
        self.boundary_breach_count
            .store(boundary_breaches, Ordering::Relaxed);
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
        let state = self.runtime_state.load(Ordering::Relaxed);
        if state == STATE_LOST {
            return HealthStatus::failed(
                "openxr-session-lost",
                HealthFailureKind::UpstreamFailure {
                    upstream: "openxr-runtime",
                },
                frame,
            );
        }
        let breaches = self.boundary_breach_count.load(Ordering::Relaxed);
        if breaches > 0 {
            return HealthStatus::Degraded {
                reason: "guardian-boundary-breach",
                budget_overshoot_bps: u16::try_from(breaches.min(10_000)).unwrap_or(u16::MAX),
                since_frame: frame,
            };
        }
        if state < STATE_FOCUSED {
            HealthStatus::Degraded {
                reason: "openxr-not-focused",
                budget_overshoot_bps: 0,
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
