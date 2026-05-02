//! § cssl-host-digital-intelligence-render — substrate pipeline synthesis
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W17-O-IMPL · canonical : `Labyrinth of Apocalypse/systems/digital_intelligence_render.csl`
//!
//! § THE SYNTHESIS
//!
//! This crate composes the substrate-resonance pixel field (alien-materialization)
//! with a TEMPORAL COHERENCE RING-BUFFER for low-latency presentation. The
//! ring stores the last 3 substrate-projection frames ; per-pixel display
//! is an axis-weighted blend across the ring, giving smooth-but-responsive
//! presentation without the per-frame jitter that pure-substrate sampling
//! would otherwise produce.
//!
//! § WHY A TEMPORAL RING (NOT A CONVENTIONAL FRAMEBUFFER)
//!
//! Conventional engines use a single back-buffer + double-buffer-swap. That
//! gives one fresh frame per present. LoA's substrate samples can have
//! HIGH-FREQUENCY VARIATION (HDC bundle outputs differ per sample-set) so
//! we'd see flicker without smoothing.
//!
//! The temporal-coherence ring (depth=3) is the SUBSTRATE ANALOGUE of TAA
//! (temporal anti-aliasing). It samples 3 recent frames and blends. Unlike
//! conventional TAA which just rejects ghosting via velocity vectors, OUR
//! blend is AXIS-WEIGHTED : the substrate axes (mostly Solemnity + Dynamism)
//! drive the blend mode. High-Solemnity scenes ease ; low-Solemnity scenes
//! snap.
//!
//! § AUTONOMOUS TICK
//!
//! `render_autonomous_tick` runs the full pipeline (begin → resolve →
//! ring-push → blend → emit) without per-frame app-driver intervention.
//! Frame budget is enforced; AdaptiveDegrader hooks (T11-W18-K) drop
//! fidelity tier when over-budget.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod profiler;
pub mod ring;

use cssl_host_alien_materialization::{
    materialize_into_pixel_field, ObserverCoord, PixelField, ResonanceFrame,
};
use cssl_host_crystallization::Crystal;

pub use profiler::{
    AdaptiveDegrader, FrameProfiler, FrameSample, Phase, PhaseTimer, TierAction,
    ROLLING_WINDOW_FRAMES,
};
pub use ring::{BlendKind, TemporalCoherenceRing};

use std::time::Instant;

/// Per-mode budget targets (microseconds). Match digital_intelligence_render.csl.
pub const BUDGET_60HZ: u32 = 16_667;
pub const BUDGET_120HZ: u32 = 8_333;
pub const BUDGET_144HZ: u32 = 6_944;
pub const BUDGET_240HZ: u32 = 4_167;

/// Frame-output handed back to the host every tick. The host emits
/// `PixelField` to whichever channel is enabled (CHANNEL_VISUAL_GPU shim
/// in stage-0).
#[derive(Debug, Clone)]
pub struct FrameOutput {
    pub frame_n: u64,
    pub resonance: ResonanceFrame,
    pub elapsed_micros: u32,
    pub fidelity_tier: u8,
    pub blend_used: BlendKind,
}

/// The renderer state. Holds the temporal-coherence ring + per-frame
/// counters + fidelity tier. The host instantiates one of these and calls
/// `tick` each frame.
#[derive(Debug)]
pub struct DigitalIntelligenceRenderer {
    pub ring: TemporalCoherenceRing,
    pub frame_n: u64,
    /// 0 = max fidelity ; 7 = min. Auto-degrades on budget overrun.
    pub fidelity_tier: u8,
    /// Sticky blend mode (host can override).
    pub blend: BlendKind,
    /// Optional wall-clock profiler (T11-W18-K). None = zero overhead.
    /// Some = each tick records phase µs + consults AdaptiveDegrader for
    /// tier-adjustments.
    pub profiler: Option<FrameProfiler>,
}

impl DigitalIntelligenceRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            ring: TemporalCoherenceRing::new(width, height),
            frame_n: 0,
            fidelity_tier: 0,
            blend: BlendKind::EaseOut,
            profiler: None,
        }
    }

    /// Builder : attach a `FrameProfiler` for wall-clock measurement and
    /// AdaptiveDegrader-driven tier adjustment. Returns `self` for chaining.
    pub fn with_profiler(mut self, profiler: FrameProfiler) -> Self {
        self.profiler = Some(profiler);
        self
    }

    /// Resize the working pixel-field to a new dimension. Preserves
    /// fidelity tier ; resets the ring (since dimensions changed).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.ring = TemporalCoherenceRing::new(width, height);
        if let Some(p) = &mut self.profiler {
            p.degrader.reset();
        }
    }

    /// One autonomous tick : resolve substrate-resonance → ring-push →
    /// temporal-blend → return blended frame.
    ///
    /// `budget_micros` is advisory ; if the inner tick estimates it would
    /// exceed budget at current fidelity, it raises tier (lowers quality).
    ///
    /// When a `FrameProfiler` is attached, wall-clock µs for each phase
    /// (ray-walk · spectral-project · ring-blend · total) are recorded and
    /// the AdaptiveDegrader's recommendation is applied to fidelity_tier.
    pub fn tick(
        &mut self,
        observer: ObserverCoord,
        crystals: &[Crystal],
        budget_micros: u32,
    ) -> FrameOutput {
        // Stage-0 micro-tick estimate : pixel-count × per-pixel-cost-tier.
        // Used for the est_micros field returned to the caller (kept for
        // backwards-compat). Wall-clock measurement, when profiler is
        // attached, is the SOURCE OF TRUTH for AdaptiveDegrader decisions.
        let pixel_count = self.ring.width * self.ring.height;
        let pixel_cost_at_tier = match self.fidelity_tier {
            0 => 8u32, // 8 ray-samples
            1 => 6,
            2 => 4,
            3 => 3,
            4 => 2,
            _ => 1,
        };
        let est_micros = (pixel_count * pixel_cost_at_tier) / 64;

        // Begin profiler-frame if attached. The wall-clock measurement runs
        // in parallel with the legacy estimate-based degrader so existing
        // callers continue to see degrader-behaviour even without profiler.
        if let Some(p) = &mut self.profiler {
            p.begin_frame(self.fidelity_tier);
        }
        let tick_start = Instant::now();

        // Legacy estimate-based degrade (kept so default-no-profiler callers
        // still see auto-degrade behaviour).
        if self.profiler.is_none() {
            if est_micros > budget_micros && self.fidelity_tier < 7 {
                self.fidelity_tier += 1;
            } else if est_micros < (budget_micros / 2) && self.fidelity_tier > 0 {
                self.fidelity_tier -= 1;
            }
        }

        // ─── Phase: ray-walk + crystal-near + spectral-project (combined) ───
        // The materialize_into_pixel_field call internally does ray-walk +
        // crystal-near-LRU + spectral-project. Stage-0 records the total wall
        // for that fused operation under RayWalk and apportions an estimate
        // to SpectralProject (~25% of the materialize cost is the LUT step).
        let phase_start = Instant::now();
        let mut fresh = PixelField::new(self.ring.width, self.ring.height);
        let resonance = materialize_into_pixel_field(observer, crystals, &mut fresh);
        let materialize_dur = phase_start.elapsed();

        // ─── Phase: ring-push + temporal-blend ───
        let blend_start = Instant::now();
        self.ring.push(fresh);
        let blend = self.blend;
        let _display = self.ring.blended(blend);
        let blend_dur = blend_start.elapsed();

        // ─── Commit profiler-sample if attached + apply degrader-action ───
        if let Some(p) = &mut self.profiler {
            // Apportion materialize → 75% ray-walk · 25% spectral-project.
            let mat_micros: u128 = materialize_dur.as_micros();
            let ray_micros = (mat_micros * 75 / 100).min(u128::from(u32::MAX)) as u32;
            let spec_micros =
                (mat_micros - mat_micros * 75 / 100).min(u128::from(u32::MAX)) as u32;
            let blend_micros = blend_dur.as_micros().min(u128::from(u32::MAX)) as u32;
            let total_micros =
                tick_start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;

            p.record_phase_micros(Phase::RayWalk, ray_micros);
            p.record_phase_micros(Phase::SpectralProject, spec_micros);
            p.record_phase_micros(Phase::RingBlend, blend_micros);
            p.record_phase_micros(Phase::Total, total_micros);
            let action = p.commit_frame();
            match action {
                TierAction::Degrade => {
                    if self.fidelity_tier < 7 {
                        self.fidelity_tier += 1;
                    }
                }
                TierAction::Recover => {
                    if self.fidelity_tier > 0 {
                        self.fidelity_tier -= 1;
                    }
                }
                TierAction::Hold => {}
            }
        }

        self.frame_n = self.frame_n.wrapping_add(1);

        FrameOutput {
            frame_n: self.frame_n,
            resonance,
            elapsed_micros: est_micros,
            fidelity_tier: self.fidelity_tier,
            blend_used: blend,
        }
    }

    /// Return the current temporally-blended pixel-field (for upload to
    /// the host's framebuffer / texture).
    pub fn current_display(&self) -> PixelField {
        self.ring.blended(self.blend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::spectral::IlluminantBlend;
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    fn day_observer() -> ObserverCoord {
        ObserverCoord {
            x_mm: 0,
            y_mm: 0,
            z_mm: 0,
            yaw_milli: 0,
            pitch_milli: 0,
            frame_t_milli: 0,
            sigma_mask_token: 0xFFFF_FFFF,
            illuminant_blend: IlluminantBlend::day(),
        }
    }

    #[test]
    fn tick_advances_frame_count() {
        let mut r = DigitalIntelligenceRenderer::new(8, 8);
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        for i in 1..=4u64 {
            let out = r.tick(day_observer(), &[crystal.clone()], BUDGET_120HZ);
            assert_eq!(out.frame_n, i);
        }
    }

    #[test]
    fn fidelity_degrades_when_over_budget() {
        let mut r = DigitalIntelligenceRenderer::new(2048, 2048); // huge → overbudget
        for _ in 0..5 {
            r.tick(day_observer(), &[], 100);
        }
        assert!(r.fidelity_tier > 0, "should degrade fidelity over tight budget");
    }

    #[test]
    fn ring_blends_three_frames() {
        let mut r = DigitalIntelligenceRenderer::new(8, 8);
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        for _ in 0..5 {
            r.tick(day_observer(), &[crystal.clone()], BUDGET_120HZ);
        }
        let _ = r.current_display();
    }

    #[test]
    fn resize_resets_ring() {
        let mut r = DigitalIntelligenceRenderer::new(8, 8);
        r.resize(16, 16);
        assert_eq!(r.ring.width, 16);
        assert_eq!(r.ring.height, 16);
    }

    #[test]
    fn profiler_attached_via_builder() {
        let r = DigitalIntelligenceRenderer::new(8, 8)
            .with_profiler(FrameProfiler::for_144hz());
        assert!(r.profiler.is_some());
        assert_eq!(r.profiler.as_ref().unwrap().budget_micros, BUDGET_144HZ);
    }

    #[test]
    fn profiler_records_per_tick() {
        let mut r = DigitalIntelligenceRenderer::new(8, 8)
            .with_profiler(FrameProfiler::for_144hz());
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        for _ in 0..3 {
            r.tick(day_observer(), &[crystal.clone()], BUDGET_144HZ);
        }
        let p = r.profiler.as_ref().unwrap();
        assert_eq!(p.frames_observed, 3);
        assert_eq!(p.window.len(), 3);
    }
}
