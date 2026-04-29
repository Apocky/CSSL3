//! § T11-D159 : Mock probe for `cssl-substrate-kan` (KAN-substrate runtime).
//!
//! Toy-state : `kan_backend` (0=Scalar/1=SIMD/2=CoopMatrix) ; degrades
//! when fallen-back to Scalar after CoopMatrix ; Failed on backend-init
//! failure (Backend=255 sentinel) ⊗ UpstreamFailure { gpu-driver }.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-substrate-kan";
const BACKEND_SCALAR: u32 = 0;
const BACKEND_INIT_FAILED: u32 = 255;

#[derive(Debug)]
pub struct MockProbe {
    kan_backend: AtomicU32,
    fallback_event_count: AtomicU32,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            kan_backend: AtomicU32::new(2), // CoopMatrix
            fallback_event_count: AtomicU32::new(0),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_state(&self, backend: u32, fallback_events: u32, frame: u64) {
        self.kan_backend.store(backend, Ordering::Relaxed);
        self.fallback_event_count
            .store(fallback_events, Ordering::Relaxed);
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
        let backend = self.kan_backend.load(Ordering::Relaxed);
        if backend == BACKEND_INIT_FAILED {
            return HealthStatus::failed(
                "kan-backend-init-failed",
                HealthFailureKind::UpstreamFailure {
                    upstream: "gpu-driver",
                },
                frame,
            );
        }
        let fallbacks = self.fallback_event_count.load(Ordering::Relaxed);
        if backend == BACKEND_SCALAR && fallbacks > 0 {
            HealthStatus::Degraded {
                reason: "kan-backend-scalar-fallback",
                budget_overshoot_bps: u16::try_from(fallbacks.min(10_000)).unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else if fallbacks > 0 {
            HealthStatus::Degraded {
                reason: "kan-backend-degraded",
                budget_overshoot_bps: u16::try_from(fallbacks.min(10_000)).unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
