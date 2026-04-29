//! § cascade — radiance-cascade integration hooks
//!
//! § PRIMER
//!
//! The radiance-cascade GI subsystem (Axiom-10 § IV ;
//! `07_AESTHETIC/02_RADIANCE_CASCADE_GI.csl`) builds a cascade of
//! probe-fields at multiple resolutions :
//!
//! ```text
//! level i : spatial-resolution s_i = s_0 / 2^i
//! level i : angular-resolution a_i = a_0 × 2^i
//! ```
//!
//! The trade-off is chosen so the per-level work stays roughly constant
//! and the total cost is `O(N log N)` instead of the `O(N²)` of naive
//! ray-tracing. Each level's spatial hierarchy is exactly a multi-band
//! wavelet pyramid, and each band's update-rate maps onto a different
//! wavelet-coefficient level :
//!
//! ```text
//! LIGHT  60 Hz   full-cascade   -> finest bands of probe pyramid
//! HEAT    1 Hz   half-cascade   -> coarser-only bands
//! MANA    4 Hz   full-cascade
//! SCENT   1 Hz   half-cascade
//! AUDIO  60 Hz   quarter-cascade
//! ```
//!
//! This module provides the integration types that the radiance-cascade
//! subsystem consumes : `CascadeBand` selects the band ; `ProbeCoarsen`
//! wraps the wavelet+MERA pyramid construction with the cascade's
//! spatial-vs-angular trade-off knobs ; `CascadeProbePyramid` ties the
//! two together.
//!
//! § SCOPE
//!
//! This crate provides the *math substrate* for the cascade pyramid —
//! per-level coarse-graining, multi-band coefficient layout, and the
//! wavelet ↔ MERA mapping. The actual probe-construction + ray-marching
//! + cascade-merging passes live in the renderer crate (`cssl-render`)
//! and consume the types here. Adding a new band is mechanical : declare
//! a `CascadeBand::Custom(name, update_hz)` variant, build a
//!   `CascadeProbePyramid` for it, and register the per-level update rate
//!   in the renderer's scheduler.

use crate::boundary::BoundaryMode;
use crate::haar::Haar;
use crate::mera::{MeraLayer, MeraPyramid};
use crate::mra::{MraCoeffs, MultiResolution};
use crate::WaveletBasis;

/// § Built-in cascade bands per Axiom-10 § IV. Custom bands extend the
/// list via `Custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CascadeBand {
    /// Visible-spectrum global illumination ; canonical 60 Hz update.
    Light,
    /// Thermal-radiation band ; 1 Hz update.
    Heat,
    /// Λ-token / magic-flow visualization ; 4 Hz update.
    Mana,
    /// Molecular-diffusion / NPC-perception band ; 1 Hz update.
    Scent,
    /// Sound-propagation / impulse-response band ; 60 Hz update.
    Audio,
}

impl CascadeBand {
    /// Canonical update-rate (Hz) for this band per the
    /// `02_RADIANCE_CASCADE_GI.csl § IV` table.
    #[must_use]
    pub fn update_hz(self) -> f32 {
        match self {
            Self::Light => 60.0,
            Self::Audio => 60.0,
            Self::Mana => 4.0,
            Self::Heat => 1.0,
            Self::Scent => 1.0,
        }
    }

    /// Cascade-resolution share : `1.0` = full cascade, `0.5` = half,
    /// `0.25` = quarter. Per the per-band table.
    #[must_use]
    pub fn cascade_share(self) -> f32 {
        match self {
            Self::Light | Self::Mana => 1.0,
            Self::Heat | Self::Scent => 0.5,
            Self::Audio => 0.25,
        }
    }

    /// Human-readable band name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "LIGHT",
            Self::Heat => "HEAT",
            Self::Mana => "MANA",
            Self::Scent => "SCENT",
            Self::Audio => "AUDIO",
        }
    }
}

/// § Configuration for one band's probe-pyramid coarsening :
/// how many cascade levels, which wavelet basis to use, and what
/// boundary mode the spatial pyramid should apply.
#[derive(Debug, Clone, Copy)]
pub struct ProbeCoarsen {
    /// Number of cascade levels (depth of the wavelet pyramid).
    pub levels: usize,
    /// Boundary policy applied at every level.
    pub boundary: BoundaryMode,
}

impl Default for ProbeCoarsen {
    fn default() -> Self {
        Self {
            levels: 4,
            boundary: BoundaryMode::Periodic,
        }
    }
}

impl ProbeCoarsen {
    /// Construct a coarsening configuration with `levels` levels.
    #[must_use]
    pub const fn new(levels: usize) -> Self {
        Self {
            levels,
            boundary: BoundaryMode::Periodic,
        }
    }
}

/// § A per-band probe pyramid : cascade band + wavelet-coefficient
/// hierarchy + MERA tensor-network summary. The radiance-cascade renderer
/// consumes this directly ; per-band update scheduling is handled
/// upstream.
///
/// The pyramid stores both a wavelet-coefficient view (`coeffs`) and a
/// MERA-tensor-network view (`mera`) of the same coarsening hierarchy.
/// This is the dual representation that Axiom-10 § III refers to when
/// it says "MERA replaces wavelet-decomposition" — the two are
/// equivalent on a binary-tree structure with identity disentanglers,
/// and the cascade subsystem chooses which view to query based on the
/// downstream pass (wavelet view for spectral analysis, MERA view for
/// summary-projection / ray-march-skipping).
#[derive(Debug, Clone)]
pub struct CascadeProbePyramid {
    /// Which physical band this pyramid represents.
    pub band: CascadeBand,
    /// Wavelet-pyramid view of the probe field.
    pub coeffs: MraCoeffs,
    /// MERA-tensor-network view of the same hierarchy.
    pub mera: MeraPyramid,
}

impl CascadeProbePyramid {
    /// Build a probe pyramid for the given band from a base-level probe
    /// field. The wavelet basis defaults to Haar (matches the MERA-Haar
    /// equivalent layer) ; pass a different basis via `build_with`.
    #[must_use]
    pub fn build(band: CascadeBand, probes: &[f32], coarsen: ProbeCoarsen) -> Self {
        Self::build_with(band, probes, coarsen, &Haar::new())
    }

    /// Build a probe pyramid using a specified wavelet basis.
    #[must_use]
    pub fn build_with<W: WaveletBasis>(
        band: CascadeBand,
        probes: &[f32],
        coarsen: ProbeCoarsen,
        wavelet: &W,
    ) -> Self {
        let coeffs = MultiResolution::decompose(probes, wavelet, coarsen.levels, coarsen.boundary);
        // Use a Haar-equivalent MERA pyramid for the tensor-network
        // view ; this matches the wavelet pyramid 1-1.
        let layers: Vec<MeraLayer> = (0..coarsen.levels)
            .map(|_| MeraLayer::haar_equivalent())
            .collect();
        let mut mera = MeraPyramid::new(layers);
        mera.build(probes);
        Self { band, coeffs, mera }
    }

    /// Reconstruct the base-level probe field from the wavelet-coefficient
    /// view.
    #[must_use]
    pub fn reconstruct<W: WaveletBasis>(&self, wavelet: &W, boundary: BoundaryMode) -> Vec<f32> {
        MultiResolution::reconstruct(&self.coeffs, wavelet, boundary)
    }

    /// Return the MERA-summary at the given cascade level. `scale = 0`
    /// is the base-level probes ; `scale = i` is the i-times-coarsened
    /// summary.
    #[must_use]
    pub fn summary_at(&self, scale: usize) -> Option<&[f32]> {
        self.mera.summary_at(scale)
    }

    /// Number of cascade levels stored.
    #[must_use]
    pub fn level_count(&self) -> usize {
        self.coeffs.levels()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_bands_have_update_rates() {
        assert!((CascadeBand::Light.update_hz() - 60.0).abs() < 1e-6);
        assert!((CascadeBand::Heat.update_hz() - 1.0).abs() < 1e-6);
        assert!((CascadeBand::Mana.update_hz() - 4.0).abs() < 1e-6);
        assert!((CascadeBand::Scent.update_hz() - 1.0).abs() < 1e-6);
        assert!((CascadeBand::Audio.update_hz() - 60.0).abs() < 1e-6);
    }

    #[test]
    fn cascade_bands_have_share() {
        assert!((CascadeBand::Light.cascade_share() - 1.0).abs() < 1e-6);
        assert!((CascadeBand::Heat.cascade_share() - 0.5).abs() < 1e-6);
        assert!((CascadeBand::Audio.cascade_share() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn cascade_band_name() {
        assert_eq!(CascadeBand::Light.name(), "LIGHT");
        assert_eq!(CascadeBand::Audio.name(), "AUDIO");
    }

    #[test]
    fn probe_coarsen_default_periodic() {
        let p = ProbeCoarsen::default();
        assert_eq!(p.levels, 4);
        assert_eq!(p.boundary, BoundaryMode::Periodic);
    }

    #[test]
    fn cascade_pyramid_builds_and_reconstructs_haar() {
        let probes: Vec<f32> = (0..16).map(|i| i as f32 * 0.5).collect();
        let coarsen = ProbeCoarsen::new(2);
        let pyr = CascadeProbePyramid::build(CascadeBand::Light, &probes, coarsen);
        assert_eq!(pyr.band, CascadeBand::Light);
        assert_eq!(pyr.level_count(), 2);
        let recon = pyr.reconstruct(&Haar::new(), BoundaryMode::Periodic);
        for (a, b) in probes.iter().zip(recon.iter()) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }

    #[test]
    fn cascade_pyramid_summary_at_levels() {
        let probes = vec![1.0_f32; 16];
        let coarsen = ProbeCoarsen::new(2);
        let pyr = CascadeProbePyramid::build(CascadeBand::Audio, &probes, coarsen);
        assert_eq!(pyr.summary_at(0).unwrap().len(), 16);
        assert_eq!(pyr.summary_at(1).unwrap().len(), 8);
        assert_eq!(pyr.summary_at(2).unwrap().len(), 4);
    }

    #[test]
    fn cascade_pyramid_summary_l2_norm_monotone() {
        // Cascade summary view is lossy by design (LOD-summary tier — the
        // discarded orthogonal complement is the detail-coefficient channel).
        // Verify the summary-norm is monotone non-increasing as we coarsen.
        let probes: Vec<f32> = (0..16).map(|i| (i as f32 * 0.3).sin() + 1.0).collect();
        let coarsen = ProbeCoarsen::new(2);
        let pyr = CascadeProbePyramid::build(CascadeBand::Mana, &probes, coarsen);
        let base_l2: f32 = probes.iter().map(|x| x * x).sum();
        let mut prev = base_l2;
        for level in 1..=pyr.level_count() {
            let summary = pyr.summary_at(level).unwrap();
            let l2: f32 = summary.iter().map(|x| x * x).sum();
            assert!(
                l2 <= prev + 1e-2,
                "level {level} : norm = {l2} should be ≤ prev = {prev}"
            );
            prev = l2;
        }
    }

    #[test]
    fn cascade_pyramid_all_bands_build() {
        let probes = vec![1.0_f32; 8];
        let coarsen = ProbeCoarsen::new(2);
        for band in [
            CascadeBand::Light,
            CascadeBand::Heat,
            CascadeBand::Mana,
            CascadeBand::Scent,
            CascadeBand::Audio,
        ] {
            let pyr = CascadeProbePyramid::build(band, &probes, coarsen);
            assert_eq!(pyr.band, band);
        }
    }
}
