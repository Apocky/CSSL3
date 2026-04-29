//! § T11-D159 : Mock probe for `cssl-spectral-render` (KAN-BRDF + tonemap).
//!
//! Toy-state : `kan_eval_count_per_pixel` (≤ budget per stage-6/7) +
//! `thermal_state` ; degrades when thermal-throttle engaged ⊗ Failed
//! when KAN-eval overruns 4x budget.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-spectral-render";
const KAN_EVAL_BUDGET_PER_PIXEL: u32 = 8;

#[derive(Debug)]
pub struct MockProbe {
    kan_evals_per_pixel: AtomicU32,
    thermal_throttle: AtomicBool,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            kan_evals_per_pixel: AtomicU32::new(2),
            thermal_throttle: AtomicBool::new(false),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_state(&self, kan_evals: u32, thermal: bool, frame: u64) {
        self.kan_evals_per_pixel.store(kan_evals, Ordering::Relaxed);
        self.thermal_throttle.store(thermal, Ordering::Relaxed);
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
        let kan = self.kan_evals_per_pixel.load(Ordering::Relaxed);
        if kan > KAN_EVAL_BUDGET_PER_PIXEL * 4 {
            return HealthStatus::failed(
                "kan-eval-budget-exceeded",
                HealthFailureKind::DeadlineMiss,
                frame,
            );
        }
        if self.thermal_throttle.load(Ordering::Relaxed) {
            return HealthStatus::Degraded {
                reason: "thermal-throttle",
                budget_overshoot_bps: 1_000,
                since_frame: frame,
            };
        }
        if kan > KAN_EVAL_BUDGET_PER_PIXEL {
            HealthStatus::Degraded {
                reason: "kan-eval-pressure",
                budget_overshoot_bps: u16::try_from(
                    u64::from(kan - KAN_EVAL_BUDGET_PER_PIXEL) * 1_250,
                )
                .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
