//! § T11-D159 : Mock probe for `cssl-render-v2` (12-stage SDF raymarcher).
//!
//! Toy-state : `vram_bytes_used` ; degrades when > 900MB (90% of 1GB
//! Ω-field budget per RENDERING_PIPELINE §III + DENSITY_BUDGET §III).
//! Returns Failed when > 1GB ⊗ ResourceExhaustion.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::probes::{HealthError, HealthProbe};
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-render-v2";
const VRAM_BUDGET_BYTES: u64 = 1_073_741_824; // 1 GiB
const VRAM_DEGRADE_THRESHOLD_BYTES: u64 = 966_367_641; // 90%

/// Mock probe for the 12-stage raymarch render-pipeline.
#[derive(Debug)]
pub struct MockProbe {
    vram_bytes_used: AtomicU64,
    degrade_refused: AtomicBool,
    since_frame: AtomicU64,
}

impl MockProbe {
    /// New probe in the Ok-state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vram_bytes_used: AtomicU64::new(0),
            degrade_refused: AtomicBool::new(false),
            since_frame: AtomicU64::new(0),
        }
    }

    /// Test-only : drive the toy-state. Real-integration replaces with
    /// a read-from-VkPhysicalDeviceMemoryProperties2 callback.
    pub fn set_vram_used(&self, bytes: u64, frame: u64) {
        self.vram_bytes_used.store(bytes, Ordering::Relaxed);
        self.since_frame.store(frame, Ordering::Relaxed);
    }

    /// Test-only : flip degrade-refusal mode.
    pub fn set_degrade_refused(&self, refused: bool) {
        self.degrade_refused.store(refused, Ordering::Relaxed);
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
        let used = self.vram_bytes_used.load(Ordering::Relaxed);
        let frame = self.since_frame.load(Ordering::Relaxed);
        if used > VRAM_BUDGET_BYTES {
            HealthStatus::failed(
                "vram-budget-exceeded",
                HealthFailureKind::ResourceExhaustion,
                frame,
            )
        } else if used > VRAM_DEGRADE_THRESHOLD_BYTES {
            // budget-overshoot in basis-points : (used - thresh) / budget
            let overshoot_bps =
                u16::try_from(((used - VRAM_DEGRADE_THRESHOLD_BYTES) * 10_000) / VRAM_BUDGET_BYTES)
                    .unwrap_or(u16::MAX);
            HealthStatus::Degraded {
                reason: "vram-pressure",
                budget_overshoot_bps: overshoot_bps,
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }

    fn degrade(&self, _reason: &str) -> Result<(), HealthError> {
        if self.degrade_refused.load(Ordering::Relaxed) {
            Err(HealthError::DegradeRefused(NAME))
        } else {
            // mock : reduce vram-used by 10% to simulate DFR-budget cut
            let cur = self.vram_bytes_used.load(Ordering::Relaxed);
            self.vram_bytes_used.store(cur * 9 / 10, Ordering::Relaxed);
            Ok(())
        }
    }
}
