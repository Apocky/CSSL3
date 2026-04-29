//! § T11-D159 : Mock probe for `cssl-physics-wave` (SDF-collision + XPBD-GPU).
//!
//! Toy-state : `entity_count` ; degrades when > 800K (80% of 1M tier
//! cap per DENSITY_BUDGET §IV) ; Failed when > 1M ⊗ ResourceExhaustion.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-physics-wave";
const ENTITY_BUDGET: u32 = 1_000_000;
const ENTITY_DEGRADE_THRESHOLD: u32 = 800_000;

/// Mock probe for SDF-collision-based physics.
#[derive(Debug)]
pub struct MockProbe {
    entity_count: AtomicU32,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entity_count: AtomicU32::new(0),
            since_frame: AtomicU64::new(0),
        }
    }

    /// Test-only : drive toy-state.
    pub fn set_entity_count(&self, count: u32, frame: u64) {
        self.entity_count.store(count, Ordering::Relaxed);
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
        let count = self.entity_count.load(Ordering::Relaxed);
        let frame = self.since_frame.load(Ordering::Relaxed);
        if count > ENTITY_BUDGET {
            HealthStatus::failed(
                "entity-budget-exceeded",
                HealthFailureKind::ResourceExhaustion,
                frame,
            )
        } else if count > ENTITY_DEGRADE_THRESHOLD {
            HealthStatus::Degraded {
                reason: "entity-pressure",
                budget_overshoot_bps: u16::try_from(
                    u64::from(count - ENTITY_DEGRADE_THRESHOLD) * 10_000 / u64::from(ENTITY_BUDGET),
                )
                .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
