//! § substrate_render — runtime entry-point for the Substrate-Resonance Pixel Field.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W17-SUBSTRATE-RENDER · Apocky-greenlit massive overhaul (2026-05-02)
//!
//! § APOCKY-DIRECTIVE
//!   "This is a massive overhaul I want! Completely new graphics paradigm!"
//!   "Completely novel and proprietary visual representation!"
//!   "Pure digital intelligence produced high-fidelity low-latency 3D realtime
//!    graphics with frame buffering or something similar for temporal smoothing!"
//!
//! § WHAT THIS MODULE DOES
//!
//! Owns the live `DigitalIntelligenceRenderer` + a small set of test crystals
//! procedurally-allocated at host-init. Each frame, the host calls
//! `tick(observer)` to advance the substrate-resonance pixel-field by one
//! frame. The output is an RGBA `PixelField` (256 × 256 default) that the
//! host can upload to a wgpu texture for display, OR inspect directly for
//! testing/telemetry.
//!
//! § STAGE-0 PRESENTATION
//!
//! For visible-on-screen demonstration the substrate pixel-field is uploaded
//! to a wgpu texture by `render.rs` (under the `runtime` feature). The
//! catalog-only mode (this module) still runs the substrate pipeline +
//! emits per-frame telemetry so test/CI flows verify the paradigm-shift
//! is active.
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody. Every pixel emission is per-observer-Σ-mask-gated.

use cssl_host_alien_materialization::{ObserverCoord, PixelField};
use cssl_host_crystallization::spectral::IlluminantBlend;
use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};
use cssl_host_digital_intelligence_render::{
    BlendKind, DigitalIntelligenceRenderer, FrameOutput, BUDGET_120HZ,
};
use cssl_rt::loa_startup::log_event;

/// Default substrate-render resolution. Stage-0 default is 256 × 256 — small
/// enough to run on CPU per-frame at 120Hz with 8 ray-samples per pixel,
/// large enough to demonstrate spatial structure. The host can resize at
/// any time (e.g., to match an HUD-overlay quad or a fullscreen pass).
pub const DEFAULT_SUBSTRATE_W: u32 = 256;
pub const DEFAULT_SUBSTRATE_H: u32 = 256;

/// Number of test crystals procedurally-allocated at startup. They're
/// arranged in a small ring around the test-room center so the player can
/// see substrate-resonance pixels regardless of where they look first.
pub const STARTUP_CRYSTAL_COUNT: usize = 5;

/// Holds all substrate-render state for one host instance.
pub struct SubstrateRenderState {
    pub renderer: DigitalIntelligenceRenderer,
    pub crystals: Vec<Crystal>,
    /// How many frames have ticked since init (for diagnostics).
    pub frame_count: u64,
}

impl Default for SubstrateRenderState {
    fn default() -> Self {
        Self::new()
    }
}

impl SubstrateRenderState {
    pub fn new() -> Self {
        let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
        // Place 5 crystals in a ring at z = 1500..3500mm at varying x.
        let placements: [(CrystalClass, WorldPos, u64); STARTUP_CRYSTAL_COUNT] = [
            (CrystalClass::Object, WorldPos::new(-2000, 0, 2500), 0xC1A1A_0001),
            (CrystalClass::Entity, WorldPos::new(-1000, 0, 2000), 0xC1A1A_0002),
            (CrystalClass::Aura, WorldPos::new(0, 0, 1500), 0xC1A1A_0003),
            (CrystalClass::Object, WorldPos::new(1000, 0, 2000), 0xC1A1A_0004),
            (CrystalClass::Environment, WorldPos::new(2000, 0, 2500), 0xC1A1A_0005),
        ];
        for (class, pos, seed) in placements.iter() {
            crystals.push(Crystal::allocate(*class, *seed, *pos));
        }
        log_event(
            "INFO",
            "loa-host/substrate-render",
            &format!(
                "init · {}×{} pixel-field · {} test-crystals procgen-allocated · paradigm = Substrate-Resonance Pixel Field",
                DEFAULT_SUBSTRATE_W, DEFAULT_SUBSTRATE_H, STARTUP_CRYSTAL_COUNT
            ),
        );
        Self {
            renderer: DigitalIntelligenceRenderer::new(DEFAULT_SUBSTRATE_W, DEFAULT_SUBSTRATE_H),
            crystals,
            frame_count: 0,
        }
    }

    /// Advance the substrate-render pipeline by one frame. Returns the
    /// frame's `FrameOutput` (resonance metadata + budget + fidelity).
    /// The current pixel-field is accessed via `current_display`.
    pub fn tick(&mut self, observer: ObserverCoord) -> FrameOutput {
        let out = self
            .renderer
            .tick(observer, &self.crystals, BUDGET_120HZ);
        self.frame_count = self.frame_count.wrapping_add(1);
        // Per-second telemetry (avoid per-frame log spam at 120 Hz).
        if self.frame_count % 120 == 0 {
            log_event(
                "DEBUG",
                "loa-host/substrate-render",
                &format!(
                    "tick · frame_n={} · pixels_lit={} · fidelity_tier={} · fingerprint={:08x} · blend={:?}",
                    out.frame_n,
                    out.resonance.n_pixels_lit,
                    out.fidelity_tier,
                    out.resonance.fingerprint,
                    out.blend_used,
                ),
            );
        }
        out
    }

    /// Return the current temporally-blended pixel-field. The host uploads
    /// this to a wgpu texture for display.
    pub fn current_display(&self) -> PixelField {
        self.renderer.current_display()
    }

    /// Set the global substrate-blend mode. Useful for combat (snap to
    /// `BlendKind::Instant`) vs cinematic (`Spring`).
    pub fn set_blend(&mut self, blend: BlendKind) {
        self.renderer.blend = blend;
    }

    /// Allocate a new crystal at `pos` (e.g., a player's just-described
    /// thing crystallizing into the world). Returns the new crystal's
    /// handle.
    pub fn spawn_crystal(&mut self, class: CrystalClass, seed: u64, pos: WorldPos) -> u32 {
        let c = Crystal::allocate(class, seed, pos);
        let h = c.handle;
        self.crystals.push(c);
        log_event(
            "INFO",
            "loa-host/substrate-render",
            &format!(
                "crystal-spawn · class={:?} · pos=({},{},{}) · handle=0x{:08x}",
                class, pos.x_mm, pos.y_mm, pos.z_mm, h
            ),
        );
        h
    }

    /// Forge an observer-coord matching a host-side camera + Σ-mask. Stage-0
    /// uses a simple position+yaw+pitch packing; full sensor + audio-listen
    /// fields wire in W18+.
    pub fn observer_for(
        &self,
        x_mm: i32,
        y_mm: i32,
        z_mm: i32,
        yaw_milli: u32,
        pitch_milli: u32,
        frame_t_milli: u64,
        sigma_mask_token: u32,
    ) -> ObserverCoord {
        ObserverCoord {
            x_mm,
            y_mm,
            z_mm,
            yaw_milli,
            pitch_milli,
            frame_t_milli,
            sigma_mask_token,
            illuminant_blend: IlluminantBlend::day(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_with_test_crystals() {
        let s = SubstrateRenderState::new();
        assert_eq!(s.crystals.len(), STARTUP_CRYSTAL_COUNT);
        assert_eq!(s.frame_count, 0);
    }

    #[test]
    fn tick_advances_frame_count() {
        let mut s = SubstrateRenderState::new();
        let observer = s.observer_for(0, 0, 0, 0, 0, 0, 0xFFFF_FFFF);
        let _ = s.tick(observer);
        assert_eq!(s.frame_count, 1);
    }

    #[test]
    fn current_display_has_correct_dimensions() {
        let s = SubstrateRenderState::new();
        let f = s.current_display();
        assert_eq!(f.width, DEFAULT_SUBSTRATE_W);
        assert_eq!(f.height, DEFAULT_SUBSTRATE_H);
    }

    #[test]
    fn spawn_crystal_increases_count() {
        let mut s = SubstrateRenderState::new();
        let n0 = s.crystals.len();
        let _h = s.spawn_crystal(CrystalClass::Event, 0xDEAD_BEEF, WorldPos::new(0, 0, 1000));
        assert_eq!(s.crystals.len(), n0 + 1);
    }

    #[test]
    fn substrate_pipeline_lights_pixels_when_observer_faces_crystal() {
        let mut s = SubstrateRenderState::new();
        let observer = s.observer_for(0, 0, 0, 0, 0, 0, 0xFFFF_FFFF);
        // Run a couple of frames so the temporal-coherence ring fills up.
        for _ in 0..3 {
            let _ = s.tick(observer);
        }
        let frame = s.tick(observer);
        // At least one of the test crystals is in front of the observer
        // and should have lit at least one pixel.
        assert!(
            frame.resonance.n_pixels_lit > 0,
            "expected at least one resonant pixel"
        );
    }
}
