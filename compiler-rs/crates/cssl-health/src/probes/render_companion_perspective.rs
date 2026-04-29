//! § T11-D159 : Mock probe for `cssl-render-companion-perspective`.
//!
//! Companion-perspective renderer (D121 — extension of cssl-render-v2).
//! Toy-state : `view_count` (≤ 4 supported simultaneously) ; degrades
//! at 4 ⊗ Failed at >4.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-render-companion-perspective";
const COMPANION_VIEW_BUDGET: u32 = 4;

#[derive(Debug)]
pub struct MockProbe {
    view_count: AtomicU32,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            view_count: AtomicU32::new(0),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_view_count(&self, count: u32, frame: u64) {
        self.view_count.store(count, Ordering::Relaxed);
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
        let count = self.view_count.load(Ordering::Relaxed);
        let frame = self.since_frame.load(Ordering::Relaxed);
        match count.cmp(&COMPANION_VIEW_BUDGET) {
            std::cmp::Ordering::Greater => HealthStatus::failed(
                "companion-view-cap-exceeded",
                HealthFailureKind::ResourceExhaustion,
                frame,
            ),
            std::cmp::Ordering::Equal => HealthStatus::Degraded {
                reason: "companion-view-saturated",
                budget_overshoot_bps: 0,
                since_frame: frame,
            },
            std::cmp::Ordering::Less => HealthStatus::Ok,
        }
    }
}
