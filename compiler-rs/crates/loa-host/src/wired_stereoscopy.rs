//! § wired_stereoscopy — thin loa-host wrapper around `cssl-host-stereoscopy`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports IPD-aware eye-pair geometry + RGBA composition functions
//!   so MCP tools can emit a default StereoConfig + compose stereoscopic
//!   captures without reaching into the path-dep directly.
//!
//! § wrapped surface
//!   - [`StereoConfig`] / [`StereoErr`] — IPD + convergence + validation.
//!   - [`EyePair`] / [`EyePose`] / [`eye_pair_from_mono`] — geometry.
//!   - [`compose_side_by_side`] / [`compose_top_bottom`] /
//!     [`compose_anaglyph_red_cyan`] — RGBA composition.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure-math only.

pub use cssl_host_stereoscopy::{
    compose_anaglyph_red_cyan, compose_side_by_side, compose_top_bottom, eye_pair_from_mono,
    ComposeErr, ComposeFormat, EyePair, EyePose, StereoCaptureManifest, StereoConfig, StereoErr,
};

/// Convenience : default stereo config (IPD = 63 mm) serialized to JSON.
///
/// Used by the `stereo.config_default_text` MCP tool to surface the canonical
/// IPD parameters without forcing every caller to know the StereoConfig
/// constructor signature.
pub fn default_config_json() -> String {
    let cfg = StereoConfig::default();
    serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_json_is_well_formed_object() {
        let s = default_config_json();
        // Parse-back to confirm round-trip stability.
        let parsed: serde_json::Value =
            serde_json::from_str(&s).expect("default_config_json must parse");
        assert!(parsed.is_object(), "default config must be a JSON object");
    }

    #[test]
    fn default_config_validates() {
        let cfg = StereoConfig::default();
        cfg.validate().expect("default config must validate");
    }
}
