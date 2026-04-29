//! § T11-D159 : Mock probe for `cssl-anim-procedural` (procedural-anim curves).
//!
//! Toy-state : `creatures_active` per tier ; degrades on T0+T1 spillover
//! ; Failed if total exceeds DENSITY_BUDGET §IV cap (60K creatures).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-anim-procedural";
const CREATURE_BUDGET: u32 = 60_000;
const CREATURE_DEGRADE_THRESHOLD: u32 = 50_000;

#[derive(Debug)]
pub struct MockProbe {
    creatures_active: AtomicU32,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            creatures_active: AtomicU32::new(0),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_creatures_active(&self, count: u32, frame: u64) {
        self.creatures_active.store(count, Ordering::Relaxed);
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
        let count = self.creatures_active.load(Ordering::Relaxed);
        let frame = self.since_frame.load(Ordering::Relaxed);
        if count > CREATURE_BUDGET {
            HealthStatus::failed(
                "creature-budget-exceeded",
                HealthFailureKind::ResourceExhaustion,
                frame,
            )
        } else if count > CREATURE_DEGRADE_THRESHOLD {
            HealthStatus::Degraded {
                reason: "creature-pressure",
                budget_overshoot_bps: u16::try_from(
                    u64::from(count - CREATURE_DEGRADE_THRESHOLD) * 10_000
                        / u64::from(CREATURE_BUDGET),
                )
                .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
