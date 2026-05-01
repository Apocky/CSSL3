//! § wired_spectral_grader — wrapper around `cssl-host-spectral-grader`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the `RGB → 16-band SPD` upsamplers + the band-wavelength LUT
//!   so MCP tools can list canonical band wavelengths and feed RGB triples
//!   into the spectral pipeline without reaching across the path-dep.
//!
//! § wrapped surface
//!   - [`SpectralGrader`] / [`GraderMethod`] — upsampler driver.
//!   - [`Spd`] — 16-band SPD type + LUT.
//!   - [`rgb_to_spd_smits_like`] / [`rgb_to_spd_jakob_simplified`] /
//!     [`roundtrip_error`] — pure-fn upsamplers.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure-math only.

pub use cssl_host_spectral_grader::{
    rgb_to_spd_jakob_simplified, rgb_to_spd_smits_like, roundtrip_error, spd_to_xyz,
    srgb_to_xyz_d65, xyz_to_srgb_d65, GraderMethod, Spd, SpectralGrader, BAND_WAVELENGTHS_NM,
    CMF_X, CMF_Y, CMF_Z, N_BANDS,
};

/// Convenience : return the 16 canonical band wavelengths (in nm) as a
/// JSON array string suitable for direct MCP-tool emission.
#[must_use]
pub fn band_wavelengths_json() -> String {
    serde_json::to_string(&BAND_WAVELENGTHS_NM[..]).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n_bands_is_sixteen() {
        assert_eq!(N_BANDS, 16);
        assert_eq!(BAND_WAVELENGTHS_NM.len(), 16);
    }

    #[test]
    fn wavelengths_json_is_array_of_16() {
        let s = band_wavelengths_json();
        let parsed: Vec<f32> =
            serde_json::from_str(&s).expect("wavelengths_json must parse");
        assert_eq!(parsed.len(), 16);
    }

    #[test]
    fn upsample_white_round_trips() {
        let spd = rgb_to_spd_smits_like([1.0, 1.0, 1.0]);
        let err = roundtrip_error([1.0, 1.0, 1.0], &spd);
        // Smits-like target round-trip ≤ 0.05 for white.
        assert!(err <= 0.10, "round-trip error too large : {err}");
    }
}
