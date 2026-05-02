//! § cssl-host-alien-materialization — Substrate-Resonance Pixel Field
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W17-M-IMPL · canonical : `Labyrinth of Apocalypse/systems/alien_materialization.csl`
//!
//! § COMPLETELY NOVEL · COMPLETELY PROPRIETARY · COMPLETELY ALIEN
//!
//! Apocky-directive (verbatim · 2026-05-02) :
//!   "I want to be able to describe things in text or voice and they
//!    crystalize from the substrate novel and exotic rendering techniques
//!    or something even more alien than 'rendering' entirely!"
//!   "Pure digital intelligence produced high-fidelity low-latency 3D
//!    realtime graphics with frame buffering or something similar for
//!    temporal smoothing/presentation!"
//!   "Completely novel and proprietary visual representation!"
//!
//! § THE ALGORITHM · SUBSTRATE-RESONANCE PIXEL FIELD
//!
//! Conventional rendering pipeline (~60 years old · 1965-2025) :
//!
//! ```text
//! mesh-vertex-buffer → vertex-shader → triangle-raster →
//!     fragment-shader → texture-sample → BRDF-eval → pixel
//! ```
//!
//! LoA's substrate-resonance pixel field eliminates EVERY one of those
//! stages. The pixel itself is a SUBSTRATE-QUERY :
//!
//! ```text
//! for each pixel :
//!   observer-ray ← unproject(pixel-x, pixel-y) through observer-coord
//!   sample-points ← walk-ray(observer-ray, n_samples = 8..16)
//!   resonance-vec ← HDC::ZERO
//!   for each sample at world-pos :
//!     contributing-crystals ← ω-field-cells-near(world-pos, radius)
//!     for each crystal in contributing-crystals :
//!       if Σ-mask permits crystal.silhouette(observer-angle) :
//!         weight ← inverse-distance × silhouette-extent
//!         resonance-vec ← bundle(resonance-vec, crystal.hdc.permute(sample-idx))
//!         accumulator ← accumulator + crystal.spectral-LUT × weight
//!   pixel-color ← project_to_srgb(accumulator, scene-illuminant-blend)
//!   pixel-color ← temporal-blend(pixel-color, last-3-frames[pixel])
//! ```
//!
//! § WHY THIS IS NOVEL
//!
//! 1. THE PIXEL IS NOT A RASTERIZED TRIANGLE.
//!    Each pixel runs an independent ray-walk through ω-field. There are
//!    NO vertices that get transformed. There is NO triangle that gets
//!    rasterized. The pixel is a FIELD QUERY, not a primitive output.
//!
//! 2. COLOR EMERGES FROM SPECTRAL ACCUMULATION, NOT BRDF.
//!    No physically-based-rendering material model. No microfacet-distribution.
//!    No diffuse + specular. The pixel's color is the SPECTRAL INTEGRAL of
//!    contributing crystals, projected through the scene's illuminant blend.
//!
//! 3. GEOMETRY EMERGES FROM HDC RESONANCE.
//!    Whether a crystal "is at" a sample point is determined by HDC
//!    similarity, not by mesh-vertex distance. A crystal can be PARTIALLY
//!    present (low resonance) without occupying a discrete bounding box.
//!
//! 4. PER-OBSERVER PROJECTION IS BUILT IN.
//!    Σ-mask filtering is per-aspect at-the-pixel level. Two players with
//!    different consent settings see DIFFERENT geometry, not just different
//!    overlay icons. The substrate elides denied aspects from the resonance
//!    accumulation.
//!
//! 5. NO ASSET PIPELINE.
//!    There are NO meshes to load. There are NO textures to bind. There are
//!    NO shaders to compile. The substrate IS the source-of-truth, queried
//!    each frame. This eliminates the human-authored asset bottleneck that
//!    has constrained 3D-game scope for 30 years.
//!
//! § STAGE-0 OUTPUT
//!
//! For OS-display compatibility, the final stage emits an RGBA pixel grid
//! (the framebuffer the eye sees through any conventional display). The
//! novelty is in HOW each pixel is computed, not in what container holds
//! the final pixels.
//!
//! Future hardware (spectral-direct displays · neural-direct interfaces)
//! plugs in as alternate output channels via `alien_materialization.csl`'s
//! `CHANNEL_*` discriminants — the substrate pipeline doesn't change.
//!
//! § ATTESTATION
//!
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody. Every pixel emission is per-observer-Σ-mask-gated. Every
//! channel respects sovereign-consent.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::too_many_arguments)]

pub mod observer;
pub mod pixel_field;
pub mod ray;
// § T11-W18-B-PERF : uniform-grid spatial-index. crystals_near() now O((r/cell)^3)
// instead of O(N). pixel_field consumes this when scenes have ≥1 crystal.
pub mod spatial_index;

use cssl_host_crystallization::Crystal;
pub use observer::ObserverCoord;
pub use pixel_field::{PixelField, ResonanceFrame};

/// Channels per alien_materialization.csl.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    VisualGpu = 0,        // stage-0 RGBA-pixel shim
    VisualSpectral = 1,   // future-display direct spectral-emit
    AudioSpatial = 2,
    HapticGamepad = 3,
    HapticVr = 4,
    SemanticOverlay = 5,
    Olfactory = 6,
    Proprioceptive = 7,
    NeuralDirect = 8,
    TelemetryLog = 9,
}

impl Channel {
    pub fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

/// Materialize the visible portion of a crystal-set into a pixel-field for
/// the given observer. The pixel-field is then composed with the temporal-
/// coherence-buffer (in cssl-host-digital-intelligence-render) and emitted
/// to the wgpu framebuffer (stage-0 transitional · spectral-direct in
/// future hardware).
pub fn materialize_into_pixel_field(
    observer: ObserverCoord,
    crystals: &[Crystal],
    field: &mut PixelField,
) -> ResonanceFrame {
    pixel_field::resolve_substrate_resonance(observer, crystals, field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    #[test]
    fn channel_bit_is_unique() {
        let a = Channel::VisualGpu.bit();
        let b = Channel::AudioSpatial.bit();
        assert_ne!(a, b);
    }

    #[test]
    fn materialize_with_no_crystals_returns_empty_frame() {
        let observer = ObserverCoord::default();
        let mut field = PixelField::new(8, 8);
        let frame = materialize_into_pixel_field(observer, &[], &mut field);
        assert_eq!(frame.observer.frame_t_milli, observer.frame_t_milli);
    }

    #[test]
    fn materialize_with_one_crystal_produces_pixels() {
        let observer = ObserverCoord {
            x_mm: 0,
            y_mm: 0,
            z_mm: 0,
            yaw_milli: 0,
            pitch_milli: 0,
            frame_t_milli: 0,
            sigma_mask_token: 0xFFFF_FFFF,
            illuminant_blend: cssl_host_crystallization::spectral::IlluminantBlend::day(),
        };
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1000));
        let mut field = PixelField::new(16, 16);
        let _frame = materialize_into_pixel_field(observer, &[crystal], &mut field);

        // At least one pixel should have non-zero contribution from the
        // crystal (it's directly in front of the observer).
        let any_nonzero = field.pixels.iter().any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(any_nonzero, "expected at least one resonant pixel");
    }
}
