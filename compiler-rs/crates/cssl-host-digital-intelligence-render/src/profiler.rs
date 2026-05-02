//! § profiler — wall-clock frame profiler + AdaptiveDegrader.
//! § STAGE-0 STUB · full 144Hz validator coming as W18-K-redux w/ many-worlds-aware bench.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::time::Duration;

pub const ROLLING_WINDOW_FRAMES: usize = 144;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    RayWalk = 0,
    SpectralProject = 1,
    RingBlend = 2,
    Total = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierAction {
    Hold,
    Degrade,
    Recover,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FrameSample {
    pub frame_n: u64,
    pub fidelity_tier: u8,
    pub ray_walk_us: u32,
    pub spectral_us: u32,
    pub blend_us: u32,
    pub total_us: u32,
}

pub struct PhaseTimer {
    start: std::time::Instant,
    phase: Phase,
}

impl PhaseTimer {
    pub fn new(phase: Phase) -> Self {
        Self { start: std::time::Instant::now(), phase }
    }
    pub fn elapsed_us(&self) -> u32 {
        self.start.elapsed().as_micros().min(u32::MAX as u128) as u32
    }
    pub fn phase(&self) -> Phase { self.phase }
}

#[derive(Debug, Default, Clone)]
pub struct AdaptiveDegrader {
    consecutive_overruns: u8,
    consecutive_underruns: u8,
}

impl AdaptiveDegrader {
    pub fn new() -> Self { Self::default() }

    pub fn reset(&mut self) {
        self.consecutive_overruns = 0;
        self.consecutive_underruns = 0;
    }

    /// Decide based on (used_us vs budget_us) + current tier.
    /// 3 consecutive overruns → Degrade · 30 consecutive underruns → Recover.
    pub fn observe(&mut self, used_us: u32, budget_us: u32, tier: u8) -> TierAction {
        if used_us > budget_us {
            self.consecutive_overruns = self.consecutive_overruns.saturating_add(1);
            self.consecutive_underruns = 0;
            if self.consecutive_overruns >= 3 && tier < 7 {
                self.consecutive_overruns = 0;
                return TierAction::Degrade;
            }
        } else if used_us < (budget_us / 2) {
            self.consecutive_underruns = self.consecutive_underruns.saturating_add(1);
            self.consecutive_overruns = 0;
            if self.consecutive_underruns >= 30 && tier > 0 {
                self.consecutive_underruns = 0;
                return TierAction::Recover;
            }
        } else {
            self.consecutive_overruns = 0;
            self.consecutive_underruns = 0;
        }
        TierAction::Hold
    }
}

#[derive(Debug, Clone)]
pub struct FrameProfiler {
    pub samples: VecDeque<FrameSample>,
    pub degrader: AdaptiveDegrader,
    pub budget_us: u32,
    pub current_frame: FrameSample,
}

impl FrameProfiler {
    pub fn new(budget_us: u32) -> Self {
        Self {
            samples: VecDeque::with_capacity(ROLLING_WINDOW_FRAMES),
            degrader: AdaptiveDegrader::new(),
            budget_us,
            current_frame: FrameSample::default(),
        }
    }

    pub fn begin_frame(&mut self, fidelity_tier: u8) {
        self.current_frame = FrameSample {
            fidelity_tier,
            ..Default::default()
        };
    }

    /// Alias for `record_phase`. Both names accepted for ergonomics.
    pub fn record_phase_micros(&mut self, phase: Phase, us: u32) {
        self.record_phase(phase, us);
    }

    pub fn record_phase(&mut self, phase: Phase, us: u32) {
        match phase {
            Phase::RayWalk => self.current_frame.ray_walk_us = us,
            Phase::SpectralProject => self.current_frame.spectral_us = us,
            Phase::RingBlend => self.current_frame.blend_us = us,
            Phase::Total => self.current_frame.total_us = us,
        }
    }

    /// Alias for `end_frame` with auto-incrementing frame_n. Used by the
    /// renderer's tick() inside its profiler-attached path.
    pub fn commit_frame(&mut self) -> TierAction {
        let next_n = self.samples.back().map(|s| s.frame_n + 1).unwrap_or(0);
        self.end_frame(next_n)
    }

    pub fn end_frame(&mut self, frame_n: u64) -> TierAction {
        self.current_frame.frame_n = frame_n;
        if self.current_frame.total_us == 0 {
            self.current_frame.total_us =
                self.current_frame.ray_walk_us
                + self.current_frame.spectral_us
                + self.current_frame.blend_us;
        }
        let action = self.degrader.observe(
            self.current_frame.total_us,
            self.budget_us,
            self.current_frame.fidelity_tier,
        );
        if self.samples.len() >= ROLLING_WINDOW_FRAMES {
            self.samples.pop_front();
        }
        self.samples.push_back(self.current_frame);
        action
    }

    /// p99 of total_us over the rolling window (returns 0 if empty).
    pub fn p99_total_us(&self) -> u32 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut totals: Vec<u32> = self.samples.iter().map(|s| s.total_us).collect();
        totals.sort_unstable();
        let idx = ((totals.len() as f64 * 0.99) as usize).min(totals.len() - 1);
        totals[idx]
    }

    /// Mean jitter (stddev of total_us) over rolling window.
    pub fn jitter_stddev_us(&self) -> u32 {
        let n = self.samples.len();
        if n < 2 {
            return 0;
        }
        let mean = self.samples.iter().map(|s| s.total_us as u64).sum::<u64>() / n as u64;
        let variance = self
            .samples
            .iter()
            .map(|s| {
                let d = (s.total_us as i64) - (mean as i64);
                (d * d) as u64
            })
            .sum::<u64>()
            / n as u64;
        (variance as f64).sqrt() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn phase_timer_records_elapsed() {
        let t = PhaseTimer::new(Phase::RayWalk);
        sleep(Duration::from_millis(1));
        assert!(t.elapsed_us() > 0);
    }

    #[test]
    fn degrader_degrades_after_3_overruns() {
        let mut d = AdaptiveDegrader::new();
        assert_eq!(d.observe(10000, 6944, 0), TierAction::Hold);
        assert_eq!(d.observe(10000, 6944, 0), TierAction::Hold);
        assert_eq!(d.observe(10000, 6944, 0), TierAction::Degrade);
    }

    #[test]
    fn degrader_recovers_after_30_underruns() {
        let mut d = AdaptiveDegrader::new();
        let mut last = TierAction::Hold;
        for _ in 0..30 {
            last = d.observe(1000, 6944, 5);
        }
        assert_eq!(last, TierAction::Recover);
    }

    #[test]
    fn profiler_records_phases() {
        let mut p = FrameProfiler::new(6944);
        p.begin_frame(0);
        p.record_phase(Phase::RayWalk, 4000);
        p.record_phase(Phase::SpectralProject, 1000);
        p.record_phase(Phase::RingBlend, 500);
        let a = p.end_frame(1);
        assert_eq!(a, TierAction::Hold);
        assert_eq!(p.samples.len(), 1);
        assert_eq!(p.samples[0].total_us, 5500);
    }

    #[test]
    fn profiler_window_caps_at_144() {
        let mut p = FrameProfiler::new(6944);
        for i in 0..200 {
            p.begin_frame(0);
            p.record_phase(Phase::Total, 5000);
            p.end_frame(i);
        }
        assert_eq!(p.samples.len(), ROLLING_WINDOW_FRAMES);
    }

    #[test]
    fn p99_returns_high_percentile() {
        let mut p = FrameProfiler::new(6944);
        for i in 0..100 {
            p.begin_frame(0);
            let us = if i < 99 { 4000 } else { 9000 };
            p.record_phase(Phase::Total, us);
            p.end_frame(i);
        }
        assert!(p.p99_total_us() >= 4000);
    }
}
