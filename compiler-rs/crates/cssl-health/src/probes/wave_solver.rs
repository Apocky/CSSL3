//! § T11-D159 : Mock probe for `cssl-wave-solver` (LBM ψ multi-band).
//!
//! Toy-state : `imex_substep_count` ; degrades > 4 substeps (per
//! DENSITY_BUDGET §VI 4→2 substep target) ; Failed if conservation
//! violated ⊗ InvariantBreach.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-wave-solver";
const IMEX_TARGET_SUBSTEPS: u32 = 4;

/// Mock probe for the LBM-based wave solver.
#[derive(Debug)]
pub struct MockProbe {
    imex_substep_count: AtomicU32,
    conservation_violated: AtomicBool,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            imex_substep_count: AtomicU32::new(2),
            conservation_violated: AtomicBool::new(false),
            since_frame: AtomicU64::new(0),
        }
    }

    /// Test-only : drive toy-state.
    pub fn set_state(&self, substeps: u32, conservation_violated: bool, frame: u64) {
        self.imex_substep_count.store(substeps, Ordering::Relaxed);
        self.conservation_violated
            .store(conservation_violated, Ordering::Relaxed);
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
        if self.conservation_violated.load(Ordering::Relaxed) {
            return HealthStatus::failed(
                "wave-conservation-violated",
                HealthFailureKind::InvariantBreach,
                frame,
            );
        }
        let substeps = self.imex_substep_count.load(Ordering::Relaxed);
        if substeps > IMEX_TARGET_SUBSTEPS {
            HealthStatus::Degraded {
                reason: "imex-substep-overshoot",
                budget_overshoot_bps: u16::try_from(
                    u64::from(substeps - IMEX_TARGET_SUBSTEPS) * 2_500,
                )
                .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
