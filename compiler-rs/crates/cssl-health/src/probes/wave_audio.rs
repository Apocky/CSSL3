//! § T11-D159 : Mock probe for `cssl-wave-audio` (wave-substrate-coupled synth).
//!
//! Toy-state : `audio_underrun_count` + `frames_dropped` ; per
//! AUDIO-OPS in specs/22 — underruns are user-perceptible so they
//! degrade ; persistent underruns Fail with DeadlineMiss.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::probes::HealthProbe;
use crate::status::{HealthFailureKind, HealthStatus};

const NAME: &str = "cssl-wave-audio";
const UNDERRUN_DEGRADE_THRESHOLD: u32 = 1;
const UNDERRUN_FAIL_THRESHOLD: u32 = 16;

#[derive(Debug)]
pub struct MockProbe {
    underrun_count: AtomicU32,
    frames_dropped: AtomicU32,
    since_frame: AtomicU64,
}

impl MockProbe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            underrun_count: AtomicU32::new(0),
            frames_dropped: AtomicU32::new(0),
            since_frame: AtomicU64::new(0),
        }
    }

    pub fn set_state(&self, underruns: u32, dropped: u32, frame: u64) {
        self.underrun_count.store(underruns, Ordering::Relaxed);
        self.frames_dropped.store(dropped, Ordering::Relaxed);
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
        let underruns = self.underrun_count.load(Ordering::Relaxed);
        let dropped = self.frames_dropped.load(Ordering::Relaxed);
        let frame = self.since_frame.load(Ordering::Relaxed);
        if underruns >= UNDERRUN_FAIL_THRESHOLD {
            HealthStatus::failed(
                "audio-persistent-underrun",
                HealthFailureKind::DeadlineMiss,
                frame,
            )
        } else if underruns >= UNDERRUN_DEGRADE_THRESHOLD || dropped > 0 {
            HealthStatus::Degraded {
                reason: "audio-underrun",
                budget_overshoot_bps: u16::try_from(
                    underruns.saturating_mul(625).min(u32::from(u16::MAX)),
                )
                .unwrap_or(u16::MAX),
                since_frame: frame,
            }
        } else {
            HealthStatus::Ok
        }
    }
}
