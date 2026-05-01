//! § manifest — Serializable record of a stereoscopic capture event.
//!
//! § PURPOSE
//!   Travels alongside the composed-image bytes so a downstream consumer
//!   (snapshot-archive, MCP client, replay verifier) can reconstruct exactly
//!   which eye-poses produced which pixels.
//!
//! § FORMAT NOTE
//!   `capture_ts_iso` is an ISO-8601 string in UTC ; producer-side. This crate
//!   does not depend on `chrono` to keep the dep-tree minimal — caller passes
//!   the timestamp string directly.

use crate::config::StereoConfig;
use crate::geometry::EyePose;
use serde::{Deserialize, Serialize};

/// How a stereo pair was composed for distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComposeFormat {
    SideBySide,
    TopBottom,
    AnaglyphRedCyan,
    LeftEyeOnly,
    RightEyeOnly,
}

/// Full record of a stereoscopic capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StereoCaptureManifest {
    pub config: StereoConfig,
    pub mono_pos: [f32; 3],
    pub left_pose: EyePose,
    pub right_pose: EyePose,
    pub width: u32,
    pub height: u32,
    pub capture_ts_iso: String,
    pub format: ComposeFormat,
}

impl StereoCaptureManifest {
    /// Serialize as pretty-printed JSON (human-inspectable).
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::eye_pair_from_mono;

    fn sample_manifest() -> StereoCaptureManifest {
        let cfg = StereoConfig::default();
        let mono = [0.0, 1.65, 0.0];
        let pair = eye_pair_from_mono(mono, [0.0, 0.0, -1.0], [0.0, 1.0, 0.0], &cfg).unwrap();
        StereoCaptureManifest {
            config: cfg,
            mono_pos: mono,
            left_pose: pair.left,
            right_pose: pair.right,
            width: 1920,
            height: 1080,
            capture_ts_iso: "2026-04-30T16:00:00Z".to_string(),
            format: ComposeFormat::SideBySide,
        }
    }

    #[test]
    fn round_trip_serialize() {
        let m = sample_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: StereoCaptureManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        // All ComposeFormat variants round-trip.
        for fmt in [
            ComposeFormat::SideBySide,
            ComposeFormat::TopBottom,
            ComposeFormat::AnaglyphRedCyan,
            ComposeFormat::LeftEyeOnly,
            ComposeFormat::RightEyeOnly,
        ] {
            let mut m2 = m.clone();
            m2.format = fmt;
            let j = serde_json::to_string(&m2).unwrap();
            let b: StereoCaptureManifest = serde_json::from_str(&j).unwrap();
            assert_eq!(b.format, fmt);
        }
    }

    #[test]
    fn pretty_print_includes_iso_timestamp() {
        let m = sample_manifest();
        let pretty = m.to_pretty_json().unwrap();
        assert!(pretty.contains("2026-04-30T16:00:00Z"));
        assert!(pretty.contains("\"format\""));
        assert!(pretty.contains("\"width\""));
        // Pretty-printed JSON has newlines (single-line would not).
        assert!(pretty.contains('\n'));
    }
}
