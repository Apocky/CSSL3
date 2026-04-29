//! § T11-D159 : Mock probe for `cssl-fractal-amp` (mise-en-abyme amplifier).
//!
//! Toy-state : `recursion_depth_witnessed` ; degrades > RecursionDepthMax
//! ⊗ Failed when over 2x cap (per RENDERING_PIPELINE §III stage-9 ;
//! D119 fractal-amplifier extension).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-fractal-amp";
const RECURSION_DEPTH_MAX: u32 = 6;

#[derive(Debug)]
pub struct MockProbe {
    recursion_depth: AtomicU32,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            recursion_depth: AtomicU32::new(2),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_recursion_depth(&self, depth: u32, frame: u64) {
        self.recursion_depth.store(depth, Ordering::Relaxed);
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
        let depth = self.recursion_depth.load(Ordering::Relaxed);
        let frame = self.since_frame.load(Ordering::Relaxed);
        if depth > RECURSION_DEPTH_MAX * 2 {
            HealthStatus::failed(
                "recursion-depth-runaway",
                HealthFailureKind::InvariantBreach,
                frame,
            )
        } else if depth > RECURSION_DEPTH_MAX {
            HealthStatus::Degraded {
                reason: "recursion-depth-overshoot",
                budget_overshoot_bps: u16::try_from(u64::from(depth - RECURSION_DEPTH_MAX) * 1_667)
                    .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
