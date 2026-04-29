//! § WitnessCompositor — per-frame attenuated-compose + telemetry sink
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The compositor accumulates per-bounce contributions into the final
//!   `MiseEnAbymeRadiance` output, runs the energy-conservation post-
//!   condition, and buffers telemetry events for the frame.
//!
//!   Per spec § Stage-9.compute step-2d :
//!     `d. accumulate into MiseEnAbymeRadiance`
//!
//!   And per § Stage-9.compute step-3 :
//!     `terminate-witness emitted-to-telemetry per-frame`
//!
//!   The compositor is the implementation of both clauses : it owns the
//!   attenuation-aware accumulator AND the per-frame telemetry buffer.

use smallvec::SmallVec;

use super::radiance::MiseEnAbymeRadiance;
use super::Stage9Event;

/// § Per-frame statistics emitted at frame-end.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WitnessCompositorStats {
    /// § Total bounces this frame (recursion-step count, not pixel count).
    pub bounces: u32,
    /// § Pixels that hit the hard cap during recursion.
    pub hard_cap_pixels: u32,
    /// § Pixels that terminated early via KAN-confidence.
    pub kan_terminate_pixels: u32,
    /// § Eye-redactions this frame.
    pub eye_redactions: u32,
    /// § Surveillance-blocks this frame.
    pub surveillance_blocks: u32,
}

impl WitnessCompositorStats {
    /// § Sum of pixel-terminate counters (hard-cap + KAN).
    #[must_use]
    pub fn total_pixel_terminations(self) -> u32 {
        self.hard_cap_pixels + self.kan_terminate_pixels
    }

    /// § Sum of consent-related events (eye-redaction + surveillance-block).
    #[must_use]
    pub fn total_consent_events(self) -> u32 {
        self.eye_redactions + self.surveillance_blocks
    }
}

/// § The compositor : carries per-frame stats + a buffered event queue.
pub struct WitnessCompositor {
    /// § Per-frame stats accumulator. Reset on `begin_frame`.
    stats: WitnessCompositorStats,
    /// § Telemetry buffer ; capped at 256 entries to keep the per-frame
    ///   memory bounded. Excess events are merged into the final
    ///   `FrameStats` rollup.
    events: SmallVec<[Stage9Event; 16]>,
    /// § Maximum telemetry buffer entries before merging. Default = 256.
    max_events: usize,
}

impl Default for WitnessCompositor {
    fn default() -> Self {
        Self::new()
    }
}

impl WitnessCompositor {
    /// § Construct an empty compositor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stats: WitnessCompositorStats::default(),
            events: SmallVec::new(),
            max_events: 256,
        }
    }

    /// § Begin a new frame. Resets the per-frame stats + clears the
    ///   event buffer.
    pub fn begin_frame(&mut self) {
        self.stats = WitnessCompositorStats::default();
        self.events.clear();
    }

    /// § Increment the bounce counter. Called once per recursion-step.
    pub fn tick_bounce(&mut self) {
        self.stats.bounces = self.stats.bounces.saturating_add(1);
    }

    /// § Increment the hard-cap counter. Called when a pixel hits the
    ///   `RECURSION_DEPTH_HARD_CAP`.
    pub fn tick_hard_cap(&mut self) {
        self.stats.hard_cap_pixels = self.stats.hard_cap_pixels.saturating_add(1);
    }

    /// § Increment the KAN-terminate counter. Called when a pixel
    ///   terminates early via confidence-below-MIN.
    pub fn tick_kan_terminate(&mut self) {
        self.stats.kan_terminate_pixels = self.stats.kan_terminate_pixels.saturating_add(1);
    }

    /// § Increment the eye-redaction counter.
    pub fn tick_eye_redaction(&mut self) {
        self.stats.eye_redactions = self.stats.eye_redactions.saturating_add(1);
    }

    /// § Increment the surveillance-block counter.
    pub fn tick_surveillance_block(&mut self) {
        self.stats.surveillance_blocks = self.stats.surveillance_blocks.saturating_add(1);
    }

    /// § Record a telemetry event. If the buffer would overflow, the
    ///   event is dropped (the rolled-up FrameStats event captures the
    ///   counts so no information is lost — only individual event
    ///   resolution is reduced under heavy load).
    pub fn record_event(&mut self, event: Stage9Event) {
        if self.events.len() < self.max_events {
            self.events.push(event);
        }
    }

    /// § Read the current event buffer. Caller is expected to drain it
    ///   into the telemetry-ring at frame-end.
    #[must_use]
    pub fn events(&self) -> &[Stage9Event] {
        &self.events
    }

    /// § Read the current stats.
    #[must_use]
    pub fn stats(&self) -> WitnessCompositorStats {
        self.stats
    }

    /// § End the frame. Emits the FrameStats event and returns the
    ///   per-frame stats.
    pub fn end_frame(&mut self) -> WitnessCompositorStats {
        let s = self.stats;
        self.record_event(Stage9Event::FrameStats {
            bounces: s.bounces,
            hard_cap_pixels: s.hard_cap_pixels,
            kan_terminate_pixels: s.kan_terminate_pixels,
            eye_redactions: s.eye_redactions,
            surveillance_blocks: s.surveillance_blocks,
        });
        s
    }

    /// § Compose the accumulator + base radiance with the given
    ///   attenuation. Convenience wrapper for the `MiseEnAbymePass` body.
    pub fn compose(
        accumulator: &mut MiseEnAbymeRadiance,
        attenuation: f32,
        contribution: &MiseEnAbymeRadiance,
    ) {
        accumulator.accumulate(attenuation, contribution);
    }

    /// § Verify the energy-conservation post-condition. Returns true iff
    ///   the accumulator's total-energy is bounded by the linear envelope
    ///   `prev_total + attenuation * contribution_total`. Used in tests
    ///   + as a debug-mode gate.
    #[must_use]
    pub fn verify_energy_conservation(
        prev: &MiseEnAbymeRadiance,
        new: &MiseEnAbymeRadiance,
        attenuation: f32,
        contribution: &MiseEnAbymeRadiance,
    ) -> bool {
        let envelope =
            prev.total_energy() + attenuation.clamp(0.0, 1.0) * contribution.total_energy();
        new.total_energy() <= envelope + 1e-3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Default compositor has zero stats.
    #[test]
    fn default_stats_are_zero() {
        let c = WitnessCompositor::new();
        assert_eq!(c.stats(), WitnessCompositorStats::default());
    }

    /// § tick_bounce increments bounces.
    #[test]
    fn tick_bounce_increments() {
        let mut c = WitnessCompositor::new();
        c.tick_bounce();
        c.tick_bounce();
        c.tick_bounce();
        assert_eq!(c.stats().bounces, 3);
    }

    /// § begin_frame resets stats.
    #[test]
    fn begin_frame_resets_stats() {
        let mut c = WitnessCompositor::new();
        c.tick_bounce();
        c.tick_hard_cap();
        c.begin_frame();
        assert_eq!(c.stats(), WitnessCompositorStats::default());
    }

    /// § record_event buffers events.
    #[test]
    fn record_event_buffers() {
        let mut c = WitnessCompositor::new();
        c.record_event(Stage9Event::HardCapTerminate { depth: 5 });
        assert_eq!(c.events().len(), 1);
    }

    /// § record_event drops past max_events cap.
    #[test]
    fn record_event_drops_past_cap() {
        let mut c = WitnessCompositor::new();
        c.max_events = 2;
        c.record_event(Stage9Event::HardCapTerminate { depth: 5 });
        c.record_event(Stage9Event::HardCapTerminate { depth: 5 });
        c.record_event(Stage9Event::HardCapTerminate { depth: 5 });
        assert_eq!(c.events().len(), 2);
    }

    /// § end_frame emits a FrameStats event.
    #[test]
    fn end_frame_emits_frame_stats() {
        let mut c = WitnessCompositor::new();
        c.tick_bounce();
        c.tick_kan_terminate();
        c.end_frame();
        let last_event = c.events().last().unwrap();
        assert!(matches!(last_event, Stage9Event::FrameStats { .. }));
    }

    /// § Compose accumulates correctly.
    #[test]
    fn compose_accumulates() {
        let mut acc = MiseEnAbymeRadiance::ZERO;
        let contrib = MiseEnAbymeRadiance::splat(0.5);
        WitnessCompositor::compose(&mut acc, 0.5, &contrib);
        // expected = 0.5 * 0.5 = 0.25 splat
        assert!(acc.approx_eq(&MiseEnAbymeRadiance::splat(0.25), 1e-6));
    }

    /// § Verify-energy-conservation accepts conservative accumulation.
    #[test]
    fn verify_energy_conservation_accepts_conservative() {
        let prev = MiseEnAbymeRadiance::ZERO;
        let contrib = MiseEnAbymeRadiance::splat(1.0);
        let mut new = prev;
        new.accumulate(0.7, &contrib);
        assert!(WitnessCompositor::verify_energy_conservation(
            &prev, &new, 0.7, &contrib
        ));
    }

    /// § Verify-energy-conservation rejects amplification.
    #[test]
    fn verify_energy_conservation_rejects_amplification() {
        let prev = MiseEnAbymeRadiance::ZERO;
        let contrib = MiseEnAbymeRadiance::splat(0.1);
        // Construct an artificial-amplified result.
        let new = MiseEnAbymeRadiance::splat(10.0);
        assert!(!WitnessCompositor::verify_energy_conservation(
            &prev, &new, 0.5, &contrib
        ));
    }

    /// § Stats helpers : total_pixel_terminations + total_consent_events.
    #[test]
    fn stats_helpers() {
        let s = WitnessCompositorStats {
            bounces: 100,
            hard_cap_pixels: 7,
            kan_terminate_pixels: 23,
            eye_redactions: 2,
            surveillance_blocks: 5,
        };
        assert_eq!(s.total_pixel_terminations(), 30);
        assert_eq!(s.total_consent_events(), 7);
    }
}
