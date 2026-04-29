//! § tonemap_pass — Stage 10 : spectral → tristimulus → display-RGB.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 10 of the pipeline. Drives `cssl-spectral-render::tristimulus::
//!   SpectralTristimulus` to convert 16-band hyperspectral output into
//!   display-RGB via CIE-XYZ + ACES-2 tonemap.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use cssl_spectral_render::{
    band::BandTable, Cie1931Xyz, DisplayPrimaries, SpectralRadiance, SpectralTristimulus, SrgbColor,
};

use super::companion_semantic_pass::CompanionSemanticOutputs;
use super::mise_en_abyme_pass::MiseEnAbymeOutputs;

/// Outputs of Stage 10 — tonemapped final RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneMapOutputs {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// CIE-XYZ tristimulus (linear, observer-relative).
    pub xyz: Cie1931Xyz,
    /// Linear-RGB (display primaries).
    pub linear_rgb: SrgbColor,
    /// Display primaries used.
    pub primaries: DisplayPrimaries,
}

impl ToneMapOutputs {
    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        for f in [self.xyz.x, self.xyz.y, self.xyz.z] {
            f.to_bits().hash(&mut h);
        }
        for f in [self.linear_rgb.r, self.linear_rgb.g, self.linear_rgb.b] {
            f.to_bits().hash(&mut h);
        }
        // primaries enum
        match self.primaries {
            DisplayPrimaries::Srgb => 0_u8,
            DisplayPrimaries::DciP3 => 1_u8,
            DisplayPrimaries::Rec2020 => 2_u8,
        }
        .hash(&mut h);
        h.finish()
    }
}

/// Stage 10 driver.
#[derive(Debug, Clone, Copy)]
pub struct ToneMapDriver {
    tonemap: SpectralTristimulus,
    band_table: BandTable,
}

impl ToneMapDriver {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tonemap: SpectralTristimulus::srgb_default(),
            band_table: BandTable::d65(),
        }
    }

    /// Run Stage 10. Synthesizes a SpectralRadiance from the upstream
    /// abyme + companion outputs, then tonemaps.
    pub fn run(
        &self,
        abyme: &MiseEnAbymeOutputs,
        companion: &CompanionSemanticOutputs,
        frame_idx: u64,
    ) -> ToneMapOutputs {
        // Build a deterministic 16-band spectral radiance keyed off the
        // upstream stages. This is the bring-up shape — production wiring
        // pulls real per-pixel SpectralRadiance from Stage 7+9 outputs.
        let mut bands = [0.0_f32; 16];
        let depth_factor = (abyme.max_depth_reached as f32) / 5.0;
        let cells_factor = (companion.cells_evaluated as f32) * 0.01;
        for (i, b) in bands.iter_mut().enumerate() {
            let phase = ((frame_idx as f32) * 0.01 + (i as f32) * 0.07).sin();
            *b = (0.1 + 0.05 * phase + depth_factor * 0.05 + cells_factor * 0.001).max(0.0);
        }
        let radiance = SpectralRadiance::from_bands(bands, &self.band_table);
        let xyz = self.tonemap.integrate_cie1931(&radiance, &self.band_table);
        let linear_rgb = self.tonemap.xyz_to_linear_rgb(xyz);
        ToneMapOutputs {
            frame_idx,
            xyz,
            linear_rgb,
            primaries: self.tonemap.primaries,
        }
    }
}

impl Default for ToneMapDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abyme() -> MiseEnAbymeOutputs {
        MiseEnAbymeOutputs {
            frame_idx: 0,
            max_depth_reached: 5,
            hard_cap: 5,
            recursion_bounded: true,
            mirror_count: 1,
        }
    }

    fn companion() -> CompanionSemanticOutputs {
        CompanionSemanticOutputs::skipped(0)
    }

    #[test]
    fn tonemap_constructs() {
        let _ = ToneMapDriver::new();
    }

    #[test]
    fn tonemap_runs() {
        let d = ToneMapDriver::new();
        let o = d.run(&abyme(), &companion(), 0);
        assert_eq!(o.frame_idx, 0);
    }

    #[test]
    fn tonemap_replay_bit_equal() {
        let d1 = ToneMapDriver::new();
        let d2 = ToneMapDriver::new();
        let a = d1.run(&abyme(), &companion(), 7);
        let b = d2.run(&abyme(), &companion(), 7);
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn tonemap_produces_finite_rgb() {
        let d = ToneMapDriver::new();
        let o = d.run(&abyme(), &companion(), 0);
        assert!(o.linear_rgb.r.is_finite());
        assert!(o.linear_rgb.g.is_finite());
        assert!(o.linear_rgb.b.is_finite());
    }
}
